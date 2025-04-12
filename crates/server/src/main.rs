use std::time::Duration;

use axum::extract::Path;
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::{
    Router,
    extract::{Form, State},
    response::IntoResponse,
    routing::{get, post},
};
use blame_finder::Repository;
use constants::MAX_AGE_REQUESTS_HOURS;
use futures::{sink::SinkExt, stream::StreamExt};
use leaderboard::SharedLeaderboard;
use log::{error, info};
use serde::Deserialize;
use state::{AppState, StatusUpdate};
use templates::{error_page, index_page, leaderboard_page, result_page};
use tokio::task;
use tokio::time;
use tower_http::services::ServeDir;

mod constants;
mod logger;
mod state;
mod templates;
mod todo_entrypoint;

// Form data for repository URL submission
#[derive(Deserialize)]
struct RepoForm {
    repo_url: String,
}

#[tokio::main]
async fn main() {
    logger::setup_logger().unwrap();

    let leaderboard = SharedLeaderboard::new("data/leaderboard.json".to_string(), 100)
        .await
        .expect("Failed to create leaderboard");

    let state = AppState::new(leaderboard);
    // Start cleanup task for old repos
    let cleanup_state = state.clone();
    task::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(24 * 60 * 60)); // Run once daily

        loop {
            interval.tick().await;
            info!("Running repository cleanup task");

            // TODO: fine-tune, 7 days might be too long
            match blame_finder::cleanup_old_repos(7, Some(cleanup_state.active_repo_paths.clone()))
                .await
            {
                Ok(count) => {
                    if count > 0 {
                        info!("Cleaned up {} old repositories", count);
                    }
                }
                Err(e) => {
                    error!("Error during repository cleanup: {}", e);
                }
            }
        }
    });

    let cleanup_state = state.clone();
    task::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(60 * 10)); // Run once every 10 mins

        loop {
            interval.tick().await;
            info!("Running results clearup");
            cleanup_state
                .cleanup_old_requests(MAX_AGE_REQUESTS_HOURS)
                .await;
        }
    });

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/find-oldest-todo", post(find_todo_handler))
        .route("/results/:request_id", get(results_handler))
        .route("/ws/scan-status/:request_id", get(ws_status_handler))
        .route("/leaderboard", get(leaderboard_handler))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8998".to_string());
    let addr = format!("0.0.0.0:{}", port);

    info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn index_handler() -> impl IntoResponse {
    index_page()
}

// Handler for finding the oldest TODO
use axum::Json;
use axum::http::StatusCode;
use uuid::Uuid;

// Handler for finding the oldest TODO
async fn find_todo_handler(
    State(state): State<AppState>,
    Form(form): Form<RepoForm>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let request_id = Uuid::new_v4().to_string();
    state.register_request(&request_id).await;

    // Send initial status
    state
        .send_status(
            &request_id,
            StatusUpdate {
                message: "Request received, preparing to clone repository...".to_string(),
                stage: state::Stage::Init,
                percentage: Some(0),
                error: None,
                redirect_url: None,
            },
        )
        .await;

    let repo_url = form.repo_url.clone();
    let state_clone = state.clone();
    let request_id_clone = request_id.clone();

    // Spawn background task
    tokio::spawn(async move {
        match Repository::new(&repo_url).await {
            Ok(repo) => {
                // Track active job
                let mut numb_active_jobs = state_clone.numb_active_jobs.lock().await;
                let mut active_repos = state_clone.active_repo_paths.lock().await;
                *numb_active_jobs += 1;
                active_repos.insert(repo.path().to_path_buf());
                drop(numb_active_jobs);
                drop(active_repos);

                // Execute the search process
                let result = todo_entrypoint::find_oldest_todo(
                    &repo,
                    &state_clone,
                    &request_id_clone,
                    &repo_url,
                )
                .await;

                // Decrement job counter
                let mut numb_active_jobs = state_clone.numb_active_jobs.lock().await;
                let mut active_repos = state_clone.active_repo_paths.lock().await;
                active_repos.remove(&repo.path().to_path_buf());
                *numb_active_jobs -= 1;
                drop(numb_active_jobs);
                drop(active_repos);

                // Process result and store it for later retrieval
                match result {
                    Ok(Some(todo)) => {
                        // Add to leaderboard
                        let _ = state_clone.leaderboard.try_add(todo.clone()).await;

                        // Store the result for this request_id
                        state_clone
                            .store_result(&request_id_clone, Some(todo), None)
                            .await;

                        // Send complete status with redirect URL
                        state_clone
                            .send_status(
                                &request_id_clone,
                                StatusUpdate {
                                    message: "Scan complete! Found oldest TODO.".to_string(),
                                    stage: state::Stage::Complete,
                                    percentage: Some(100),
                                    error: None,
                                    redirect_url: Some(format!("/results/{}", request_id_clone)),
                                },
                            )
                            .await;
                    }
                    Ok(None) => {
                        // Store the empty result
                        state_clone
                            .store_result(
                                &request_id_clone,
                                None,
                                Some("No TODO comments found in this repository".to_string()),
                            )
                            .await;

                        // Send error status
                        state_clone
                            .send_status(
                                &request_id_clone,
                                StatusUpdate {
                                    message: "Scan complete, but no TODO comments were found."
                                        .to_string(),
                                    stage: state::Stage::Error,
                                    percentage: Some(100),
                                    error: Some("No TODO comments found".to_string()),
                                    redirect_url: Some(format!("/results/{}", request_id_clone)),
                                },
                            )
                            .await;
                    }
                    Err(e) => {
                        let error_msg = format!("Error finding oldest TODO: {}", e);
                        error!("{}", error_msg);

                        // Store the error
                        state_clone
                            .store_result(&request_id_clone, None, Some(error_msg.clone()))
                            .await;

                        // Send error status
                        state_clone
                            .send_status(
                                &request_id_clone,
                                StatusUpdate {
                                    message: "Error during scan.".to_string(),
                                    stage: state::Stage::Error,
                                    percentage: Some(100),
                                    error: Some(error_msg),
                                    redirect_url: Some(format!("/results/{}", request_id_clone)),
                                },
                            )
                            .await;
                    }
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to clone repository: {}", e);
                error!("{}", error_msg);

                // Store the error
                state_clone
                    .store_result(&request_id_clone, None, Some(error_msg.clone()))
                    .await;

                // Send error status
                state_clone
                    .send_status(
                        &request_id_clone,
                        StatusUpdate {
                            message: "Failed to clone repository.".to_string(),
                            stage: state::Stage::Error,
                            percentage: Some(100),
                            error: Some(error_msg),
                            redirect_url: Some(format!("/results/{}", request_id_clone)),
                        },
                    )
                    .await;
            }
        }
    });

    // Return the request ID immediately
    Ok(Json(serde_json::json!({
        "request_id": request_id,
        "status": "processing"
    })))
}

// WebSocket handler for status updates
async fn ws_status_handler(
    ws: WebSocketUpgrade,
    Path(request_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, request_id, state))
}

// Handle the WebSocket connection
async fn handle_socket(socket: WebSocket, request_id: String, state: AppState) {
    let (mut sender, _receiver) = socket.split();

    // Get a receiver for this request's status channel
    let mut status_rx = match state.status_channels.lock().await.get(&request_id) {
        Some(tx) => tx.subscribe(),
        None => {
            // Request ID not found, close the connection
            let _ = sender
                .send(axum::extract::ws::Message::Text(
                    serde_json::to_string(&StatusUpdate {
                        message: "Invalid request ID".to_string(),
                        stage: state::Stage::Error,
                        percentage: None,
                        error: Some("Request not found or expired".to_string()),
                        redirect_url: None,
                    })
                    .unwrap(),
                ))
                .await;
            let _ = sender.close().await;
            return;
        }
    };

    // First, send all past status updates
    let history = state.get_status_history(&request_id).await;
    for status in history {
        if sender
            .send(axum::extract::ws::Message::Text(
                serde_json::to_string(&status).unwrap(),
            ))
            .await
            .is_err()
        {
            // Client disconnected
            return;
        }
    }

    // Check if there's already a result for this request
    if let Some(result) = state.get_result(&request_id).await {
        if result.completed {
            let status = if result.todo_item.is_some() {
                StatusUpdate {
                    message: "Scan already completed.".to_string(),
                    stage: state::Stage::Complete,
                    percentage: Some(100),
                    error: None,
                    redirect_url: Some(format!("/results/{}", request_id)),
                }
            } else {
                StatusUpdate {
                    message: "Scan already completed with errors.".to_string(),
                    stage: state::Stage::Error,
                    percentage: Some(100),
                    error: result.error,
                    redirect_url: Some(format!("/results/{}", request_id)),
                }
            };

            let _ = sender
                .send(axum::extract::ws::Message::Text(
                    serde_json::to_string(&status).unwrap(),
                ))
                .await;
            let _ = sender.close().await;
            return;
        }
    }

    // Forward status updates to the WebSocket
    while let Ok(status) = status_rx.recv().await {
        match sender
            .send(axum::extract::ws::Message::Text(
                serde_json::to_string(&status).unwrap(),
            ))
            .await
        {
            Ok(_) => {
                // If this is a final message, close the connection
                if status.stage == state::Stage::Complete || status.stage == state::Stage::Error {
                    let _ = sender.close().await;
                    break;
                }
            }
            Err(_) => break, // Client disconnected
        }
    }
}

async fn leaderboard_handler(State(state): State<AppState>) -> impl IntoResponse {
    // Fetch the top TODOs from the leaderboard
    let items = state.leaderboard.get_items().await;

    leaderboard_page(items)
}

// Handler for retrieving results by request ID
async fn results_handler(
    Path(request_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.get_result(&request_id).await {
        Some(result) => {
            if result.completed {
                match result.todo_item {
                    Some(todo) => result_page(todo),
                    None => {
                        let error_message = result.error.unwrap_or_else(|| {
                            "No TODO comments found in this repository".to_string()
                        });
                        error_page(&error_message)
                    }
                }
            } else {
                // Still processing
                index_page() // Maybe redirect to a "still processing" page instead
            }
        }
        None => error_page("Invalid or expired request ID"),
    }
}
