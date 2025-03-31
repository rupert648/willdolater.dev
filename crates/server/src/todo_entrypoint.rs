use blame_finder::{
    BlameError, Repository, TodoItem,
    blame::{self, get_git_depth},
    todo,
};
use log::debug;

use crate::state::{self, AppState, StatusUpdate};

/// Main entry point for finding the oldest TODO in a git repository
pub async fn find_oldest_todo(
    repo: &Repository,
    app_state: &AppState,
    request_id: &str,
    repo_url: &str,
) -> Result<Option<TodoItem>, BlameError> {
    // Clone or fetch the repository
    debug!("getting repo");
    app_state
        .send_status(
            request_id,
            StatusUpdate {
                message: format!("Cloning repository: {}", repo_url),
                stage: state::Stage::Clone,
                percentage: Some(10),
                error: None,
                redirect_url: None,
            },
        )
        .await;
    repo.prepare().await?;
    debug!("done preparing");

    // Find all TODO comments
    app_state
        .send_status(
            request_id,
            StatusUpdate {
                message: "Repository cloned successfully. Starting TODO scan...".to_string(),
                stage: state::Stage::Scan,
                percentage: Some(30),
                error: None,
                redirect_url: None,
            },
        )
        .await;
    let todos = todo::find_todos(&repo).await?;

    if todos.is_empty() {
        return Ok(None);
    }

    // Find the oldest TODO by analyzing git blame for each
    app_state
        .send_status(
            request_id,
            StatusUpdate {
                message: format!("Found {} TODOs, Git Blaming each one...", todos.len()),
                stage: state::Stage::Scan,
                percentage: Some(30),
                error: None,
                redirect_url: None,
            },
        )
        .await;
    let git_depth = get_git_depth(repo).await;
    if git_depth.is_ok() && *git_depth.as_ref().unwrap() > 500 {
        app_state
            .send_status(
                request_id,
                StatusUpdate {
                    message: format!(
                        "Git Depth of {}, this could take a while...",
                        git_depth.unwrap()
                    ),
                    stage: state::Stage::Scan,
                    percentage: Some(30),
                    error: None,
                    redirect_url: None,
                },
            )
            .await;
    }
    let oldest = blame::find_oldest_todo(&repo, todos).await?;

    Ok(Some(oldest))
}
