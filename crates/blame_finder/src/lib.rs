use std::path::Path;
use std::sync::Arc;
use std::{collections::HashSet, path::PathBuf, time::SystemTime};

use log::debug;
use tokio::sync::Mutex;

pub mod blame;
mod error;
mod helpers;
mod repo;
pub mod todo;

pub use blame::BlameInfo;
pub use error::BlameError;
pub use repo::Repository;
pub use todo::TodoItem;

/// Main entry point for finding the oldest TODO in a git repository
/// similar to one found in server/src/todo_entrypoint
/// minus the app state updates
pub async fn find_oldest_todo(repo: &Repository) -> Result<Option<TodoItem>, BlameError> {
    // Clone or fetch the repository
    debug!("getting repo");
    repo.prepare().await?;
    debug!("done preparing");

    // Find all TODO comments
    let todos = todo::find_todos(&repo).await?;

    if todos.is_empty() {
        return Ok(None);
    }

    // Find the oldest TODO by analyzing git blame for each
    let oldest = blame::find_oldest_todo(&repo, todos).await?;

    Ok(Some(oldest))
}

/// Clean up old repositories that haven't been accessed recently
pub async fn cleanup_old_repos(
    max_age_days: u64,
    active_repos: Option<Arc<Mutex<HashSet<PathBuf>>>>,
) -> Result<usize, BlameError> {
    let repos_dir = Repository::get_repos_dir()?;
    let max_age = std::time::Duration::from_secs(max_age_days * 24 * 60 * 60);
    let now = SystemTime::now();
    let mut deleted_count = 0;

    let entries = match std::fs::read_dir(&repos_dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(0),
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip if repository is active
        if let Some(active_repos) = active_repos.as_ref() {
            let lock = active_repos.lock().await;
            if lock.contains(&path) {
                continue;
            }
            drop(lock);
        }

        // Skip if not a directory
        if !path.is_dir() {
            continue;
        }

        // Delete if older than max_age
        if should_delete_repo(&path, &now, &max_age) {
            if let Err(e) = std::fs::remove_dir_all(&path) {
                eprintln!("Error removing old repository at {}: {}", path.display(), e);
            } else {
                deleted_count += 1;
            }
        }
    }

    Ok(deleted_count)
}

/// Helper function to determine if a repository should be deleted based on its age
fn should_delete_repo(path: &Path, now: &SystemTime, max_age: &std::time::Duration) -> bool {
    match std::fs::metadata(path) {
        Ok(metadata) => match metadata.modified() {
            Ok(modified) => match now.duration_since(modified) {
                Ok(age) => age > *max_age,
                Err(_) => false,
            },
            Err(_) => false,
        },
        Err(_) => false,
    }
}
