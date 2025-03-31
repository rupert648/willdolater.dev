use blame_finder::{BlameError, Repository, TodoItem, blame, todo};
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
                message: "Git Blaming each TODO...".to_string(),
                stage: state::Stage::Scan,
                percentage: Some(30),
                error: None,
                redirect_url: None,
            },
        )
        .await;
    let oldest = blame::find_oldest_todo(&repo, todos).await?;

    Ok(Some(oldest))
}
