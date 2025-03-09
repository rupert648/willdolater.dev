use chrono::{DateTime, Utc};
use log::debug;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::process::Command;

use crate::error::BlameError;
use crate::repo::Repository;
use crate::todo::TodoItem;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub struct BlameInfo {
    /// The commit hash that introduced this line
    pub commit_hash: String,

    /// Author of the commit
    pub author: String,

    /// Author's email
    pub author_email: String,

    /// When the commit was made
    pub date: DateTime<Utc>,

    /// Commit message summary
    pub summary: String,

    /// Age since the commit, calculated in days since date
    pub age_in_days: i64,
}

/// Find the oldest TODO among the provided list
pub async fn find_oldest_todo(
    repo: &Repository,
    todos: Vec<TodoItem>,
) -> Result<TodoItem, BlameError> {
    if todos.is_empty() {
        return Err(BlameError::InternalError("No TODOs provided".to_string()));
    }

    // Process blame information for each TODO in parallel using Tokio
    debug!("Getting blame info for todos in parallel");

    use tokio::task;

    // Create a vector to hold all the task handles
    let mut blame_tasks = Vec::with_capacity(todos.len());

    // Spawn a task for each TODO item
    for todo in todos {
        let repo_clone = repo.clone();

        // Spawn a Tokio task for each TODO
        let task_handle = task::spawn(async move {
            let mut todo_clone = todo;
            match get_blame_info(&repo_clone, &todo_clone).await {
                Ok(blame_info) => {
                    todo_clone.blame_info = Some(blame_info);
                    Some(todo_clone)
                }
                Err(e) => {
                    eprintln!(
                        "Error getting blame info for {}: {}",
                        todo_clone.file_path, e
                    );
                    None
                }
            }
        });

        blame_tasks.push(task_handle);
    }

    let mut todos_with_blame = Vec::new();
    for task in blame_tasks {
        match task.await {
            Ok(Some(todo)) => todos_with_blame.push(todo),
            Ok(None) => {} // Skip TODOs that failed to get blame info
            Err(e) => eprintln!("Task join error: {}", e),
        }
    }

    debug!("Finished getting all blame info's in parallel");

    if todos_with_blame.is_empty() {
        return Err(BlameError::InternalError(
            "Failed to get blame info for any TODOs".to_string(),
        ));
    }

    // Find the oldest TODO by commit date
    let oldest_todo = todos_with_blame
        .into_iter()
        .min_by_key(|t| t.blame_info.as_ref().unwrap().date)
        .unwrap();

    Ok(oldest_todo)
}

// Optimized git blame command
async fn get_blame_info(repo: &Repository, todo: &TodoItem) -> Result<BlameInfo, BlameError> {
    debug!("Starting blame info for todo: {}", todo.file_path);

    // Using Tokio's Command for async process execution
    let output = tokio::process::Command::new("git")
        .current_dir(repo.path())
        .arg("blame")
        .arg("-p") // porcelain format for easier parsing
        .arg("--no-progress") // reduce output
        .arg("-L")
        .arg(format!("{},{}", todo.line_number, todo.line_number))
        .arg("--")
        .arg(&todo.file_path)
        .output()
        .await
        .map_err(|e| BlameError::GitError(format!("Failed to execute git blame: {}", e)))?;

    debug!("finished blame info for todo: {}", todo.file_path);

    if !output.status.success() {
        return Err(BlameError::GitError(format!(
            "Git blame failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    // Parse the blame output
    let blame_output = String::from_utf8_lossy(&output.stdout);
    parse_blame_output(&blame_output, repo.path()).await
}
/// Parse git blame output in porcelain format
async fn parse_blame_output(blame_output: &str, repo_path: &Path) -> Result<BlameInfo, BlameError> {
    let lines: Vec<&str> = blame_output.lines().collect();

    if lines.is_empty() {
        return Err(BlameError::ParseError("Empty blame output".to_string()));
    }

    // First line has the commit hash
    let first_line_parts: Vec<&str> = lines[0].split_whitespace().collect();
    if first_line_parts.is_empty() {
        return Err(BlameError::ParseError(
            "Invalid blame output format".to_string(),
        ));
    }

    let commit_hash = first_line_parts[0].to_string();

    // Parse the rest of the porcelain output
    let mut author = String::new();
    let mut author_email = String::new();
    let mut author_time = 0;

    for line in &lines[1..] {
        if line.starts_with("author ") {
            author = line["author ".len()..].to_string();
        } else if line.starts_with("author-mail ") {
            author_email = line["author-mail ".len()..].to_string();
            // Clean up email format: <email> -> email
            author_email = author_email
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();
        } else if line.starts_with("author-time ") {
            author_time = line["author-time ".len()..]
                .parse::<i64>()
                .map_err(|_| BlameError::ParseError("Invalid author time".to_string()))?;
        }
    }

    // Get the commit message summary
    let summary = get_commit_summary(&commit_hash, repo_path).await?;

    // Convert timestamp to DateTime
    let date = chrono::DateTime::<Utc>::from_timestamp(author_time, 0)
        .ok_or_else(|| BlameError::ParseError("Invalid timestamp".to_string()))?;

    let age_in_days = (Utc::now() - date).num_days();

    Ok(BlameInfo {
        commit_hash,
        author,
        author_email,
        date,
        summary,
        age_in_days,
    })
}

/// Get the summary (first line) of a commit message
async fn get_commit_summary(commit_hash: &str, repo_path: &Path) -> Result<String, BlameError> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .arg("show")
        .arg("-s")
        .arg("--format=%s") // Just the subject line
        .arg(commit_hash)
        .output()
        .await
        .map_err(|e| BlameError::GitError(format!("Failed to get commit message: {}", e)))?;

    if !output.status.success() {
        return Err(BlameError::GitError(format!(
            "Failed to get commit message: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let summary = String::from_utf8_lossy(&output.stdout).trim().to_string();

    Ok(summary)
}
