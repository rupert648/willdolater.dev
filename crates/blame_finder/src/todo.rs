use anyhow::Result;
use chrono::Utc;
use log::debug;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::Path;
use tokio::process::Command;

use crate::blame::BlameInfo;
use crate::error::BlameError;
use crate::helpers::extract_path_segments;
use crate::repo::Repository;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// Relative path to the file containing the TODO
    pub file_path: String,

    /// Line number where the TODO appears
    pub line_number: u32,

    /// The actual TODO text
    pub todo_text: String,

    /// Surrounding code context
    pub context_code: String,

    /// Information about the commit that introduced this TODO
    pub blame_info: Option<BlameInfo>,

    /// The source repo url, copied here for easy displaying
    pub source_repo_url: String,
}

impl PartialEq for TodoItem {
    fn eq(&self, other: &Self) -> bool {
        // Consider two TodoItems equal if they have the same file path, line number, and source repo
        self.file_path == other.file_path
            && self.line_number == other.line_number
            && self.source_repo_url == other.source_repo_url
    }
}

impl Eq for TodoItem {}

impl Ord for TodoItem {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare by age (oldest first), then by file path, then by line number
        // for stable ordering when ages are equal
        self.get_age_in_days()
            .cmp(&other.get_age_in_days())
            .then_with(|| self.file_path.cmp(&other.file_path))
            .then_with(|| self.line_number.cmp(&other.line_number))
    }
}

impl PartialOrd for TodoItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl BlameInfo {
    pub fn get_age_in_days(&self) -> i64 {
        (Utc::now() - self.date).num_days()
    }
}

impl TodoItem {
    // Helper method to get the age of a TODO item
    fn get_age_in_days(&self) -> i64 {
        self.blame_info
            .as_ref()
            .map(|blame| blame.get_age_in_days())
            .unwrap_or(0)
    }

    pub fn get_permalink_url(&self) -> String {
        // No blame info means we can't generate a precise permalink
        if self.blame_info.is_none() {
            return self.source_repo_url.clone();
        }

        let blame_info = self.blame_info.as_ref().unwrap();

        // Extract the file path without the line number (which we store separately)
        let path = match self.file_path.split(':').next() {
            Some(p) => p,
            None => &self.file_path,
        };

        // Create different permalink formats based on the repository host
        let source_repo_url = match self.source_repo_url.strip_suffix(".git") {
            Some(s) => s.to_string(),
            None => self.source_repo_url.clone(),
        };
        if source_repo_url.contains("github.com") {
            // GitHub format: https://github.com/owner/repo/blob/commit-hash/path/to/file#L123
            format!(
                "{}/blob/{}/{}#L{}",
                source_repo_url, blame_info.commit_hash, path, self.line_number
            )
        } else if source_repo_url.contains("gitlab.com") {
            // GitLab format: https://gitlab.com/owner/repo/-/blob/commit-hash/path/to/file#L123
            format!(
                "{}/-/blob/{}/{}#L{}",
                source_repo_url, blame_info.commit_hash, path, self.line_number
            )
        } else if source_repo_url.contains("bitbucket.org") {
            // Bitbucket format: https://bitbucket.org/owner/repo/src/commit-hash/path/to/file#lines-123
            format!(
                "{}/src/{}/{}#lines-{}",
                source_repo_url, blame_info.commit_hash, path, self.line_number
            )
        } else {
            // Default case for other repository hosts - return repo URL
            self.source_repo_url.clone()
        }
    }

    /// Extract and return the repository display name from the source URL
    /// For example, "https://github.com/tokio-rs/tokio" becomes "tokio-rs/tokio"
    pub fn get_repo_display_name(&self) -> String {
        // Parse the URL to extract the owner/repo part
        let url = &self.source_repo_url;

        // For GitHub URLs: https://github.com/owner/repo
        if url.contains("github.com") {
            return extract_path_segments(url, "github.com");
        }

        // For GitLab URLs: https://gitlab.com/owner/repo
        if url.contains("gitlab.com") {
            return extract_path_segments(url, "gitlab.com");
        }

        // For Bitbucket URLs: https://bitbucket.org/owner/repo
        if url.contains("bitbucket.org") {
            return extract_path_segments(url, "bitbucket.org");
        }

        // For other repository hosts, return the domain + first path segment
        // Try to extract something meaningful from the URL
        if let Some(domain_start) = url.find("://") {
            if let Some(domain_end) = url[domain_start + 3..].find('/') {
                let path = &url[domain_start + 3 + domain_end + 1..];

                // Return first two path segments if available
                if let Some(path_sep) = path.find('/') {
                    let owner = &path[..path_sep];
                    let repo = &path[path_sep + 1..];

                    if let Some(query_sep) = repo.find('?') {
                        return format!("{}/{}", owner, &repo[..query_sep]);
                    } else {
                        return format!("{}/{}", owner, repo);
                    }
                }

                // If only one segment, return it
                return path.to_string();
            }
        }

        // Fallback: just return the URL as is
        url.clone()
    }
}

/// Find all TODOs in the repository using ripgrep
pub async fn find_todos(repo: &Repository) -> Result<Vec<TodoItem>, BlameError> {
    debug!("Starting search for todos w/ rg");
    let output = Command::new("rg")
        .current_dir(repo.path())
        .arg("TODO")
        .arg("--line-number") // Include line numbers in the output
        .arg("--no-heading") // Don't group matches by file
        .arg("--color=never") // No color codes in output
        .arg("--max-columns=1000") // Avoid truncating long lines
        .arg("-g") // Specify glob patterns
        .arg("!.git/") // Exclude .git directory
        .output()
        .await
        .map_err(|e| BlameError::SearchError(format!("Failed to execute ripgrep: {}", e)))?;
    debug!("finished search with rg");

    if !output.status.success() && !output.stderr.is_empty() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        // Check if it's just a "no matches found" (exit code 1 in ripgrep)
        if output.status.code() == Some(1) && err.trim().is_empty() {
            return Ok(Vec::new());
        }
        return Err(BlameError::SearchError(format!(
            "Ripgrep search failed: {}",
            err
        )));
    }

    // Parse the output
    let output_str = String::from_utf8_lossy(&output.stdout);
    parse_ripgrep_output(repo.path(), repo.url().to_owned(), &output_str)
}

/// Parse the output from ripgrep into TodoItem structs
fn parse_ripgrep_output(
    repo_path: &Path,
    repo_url: String,
    output: &str,
) -> Result<Vec<TodoItem>, BlameError> {
    let mut todos = Vec::new();

    for line in output.lines() {
        // Format: file:line:content
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() != 3 {
            continue; // Skip invalid lines
        }

        let file_path = parts[0].trim();
        let line_number = parts[1]
            .trim()
            .parse::<u32>()
            .map_err(|_| BlameError::ParseError(format!("Invalid line number: {}", parts[1])))?;
        let todo_text = parts[2].trim();

        // Read the file to get context
        let context_code = get_context(repo_path, file_path, line_number)?;

        todos.push(TodoItem {
            file_path: file_path.to_string(),
            line_number,
            todo_text: todo_text.to_string(),
            context_code,
            blame_info: None, // Will be filled in later
            source_repo_url: repo_url.clone(),
        });
    }

    Ok(todos)
}

/// Get the code context around a specific line in a file
fn get_context(repo_path: &Path, file_path: &str, line_number: u32) -> Result<String, BlameError> {
    let full_path = repo_path.join(file_path);
    if !full_path.exists() {
        return Err(BlameError::FileError(format!(
            "File not found: {}",
            file_path
        )));
    }

    let content = std::fs::read_to_string(&full_path)
        .map_err(|e| BlameError::FileError(format!("Failed to read file {}: {}", file_path, e)))?;

    let lines: Vec<&str> = content.lines().collect();

    // Line numbers in the file are 1-indexed
    let line_idx = line_number as usize - 1;

    // Get context (2 lines before and after)
    let start_line = line_idx.saturating_sub(2);
    let end_line = std::cmp::min(line_idx + 3, lines.len());

    let context = lines[start_line..end_line].join("\n");

    Ok(context)
}
