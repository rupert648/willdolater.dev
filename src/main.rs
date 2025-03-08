use std::path::PathBuf;
use std::time::Duration;
use std::{collections::HashSet, sync::Arc};

use axum::{
    Router,
    extract::{Form, State},
    response::IntoResponse,
    routing::{get, post},
};
use blame_finder::{Repository, TodoItem};
use leaderboard::SharedLeaderboard;
use serde::Deserialize;
use templates::{error_page, index_page, leaderboard_page, result_page};
use tokio::sync::Mutex;
use tokio::task;
use tokio::time;
use tower_http::services::ServeDir;
use tracing::{error, info};

mod templates;

#[derive(Clone)]
struct AppState {
    numb_active_jobs: Arc<Mutex<u32>>,
    // maintain list of repos currently being operated on to prevent deleting a repo mid-use
    active_repo_paths: Arc<Mutex<HashSet<PathBuf>>>,
    leaderboard: SharedLeaderboard<TodoItem>,
}

// Form data for repository URL submission
#[derive(Deserialize)]
struct RepoForm {
    repo_url: String,
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let leaderboard = SharedLeaderboard::new("data/leaderboard.json".to_string(), 100)
        .await
        .expect("Failed to create leaderboard");

    // Create application state
    let state = AppState {
        numb_active_jobs: Arc::new(Mutex::new(0)),
        active_repo_paths: Arc::new(Mutex::new(HashSet::new())),
        leaderboard,
    };

    // Start cleanup task
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

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/find-oldest-todo", post(find_todo_handler))
        .route("/leaderboard", get(leaderboard_handler))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);

    info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn index_handler() -> impl IntoResponse {
    index_page()
}

// Handler for finding the oldest TODO
async fn find_todo_handler(
    State(state): State<AppState>,
    Form(form): Form<RepoForm>,
) -> impl IntoResponse {
    // TODO: remove unwrap such that we can impl IntoResponse on the errors
    let repo = Repository::new(&form.repo_url).await.unwrap();

    let mut numb_active_jobs = state.numb_active_jobs.lock().await;
    let mut active_repos = state.active_repo_paths.lock().await;
    *numb_active_jobs += 1;
    active_repos.insert(repo.path().to_path_buf());
    drop(numb_active_jobs); // Release lock
    drop(active_repos); // Release lock

    let result = blame_finder::find_oldest_todo(&repo).await;

    // Decrement active jobs counter
    let mut numb_active_jobs = state.numb_active_jobs.lock().await;
    let mut active_repos = state.active_repo_paths.lock().await;
    active_repos.remove(&repo.path().to_path_buf());
    *numb_active_jobs -= 1;
    drop(numb_active_jobs); // Release lock
    drop(active_repos); // Release lock

    match result {
        Ok(Some(todo)) => {
            // Check if this TODO is one of the oldest and add it to the leaderboard if it is
            let _ = state.leaderboard.try_add(todo.clone()).await;
            result_page(todo)
        }
        Ok(None) => error_page("No TODO comments found in this repository"),
        Err(e) => {
            error!("Error finding oldest TODO: {}", e);
            // TODO: make this error more hidden?
            error_page(&format!("Error: {}", e))
        }
    }
}

async fn leaderboard_handler(State(state): State<AppState>) -> impl IntoResponse {
    // Fetch the top TODOs from the leaderboard
    let items = state.leaderboard.get_items().await;

    leaderboard_page(items)
}
