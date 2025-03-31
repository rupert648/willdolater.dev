use log::debug;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use url::Url;

use crate::error::BlameError;

/// Repository represents a Git repository that has been cloned locally
#[derive(Debug, Hash, Eq, PartialEq, Clone, Deserialize, Serialize)]
pub struct Repository {
    /// URL of the remote repository
    url: String,

    /// Local path where the repository is cloned
    path: PathBuf,

    /// Name of the repository (extracted from URL)
    name: String,
}

impl Repository {
    pub async fn new(repo_url: &str) -> Result<Self, BlameError> {
        let url = Self::validate_url(repo_url)?;

        let name = Self::extract_repo_name(&url)?;

        let path = Self::create_repo_path(&name)?;

        Ok(Repository { url, path, name })
    }

    /// Validate and normalize the repository URL
    fn validate_url(repo_url: &str) -> Result<String, BlameError> {
        let url = match Url::parse(repo_url) {
            Ok(url) => url,
            Err(_) => return Err(BlameError::InvalidUrl(repo_url.to_string())),
        };

        // Ensure it's a GitHub, GitLab, or other common Git host
        let host = url
            .host_str()
            .ok_or_else(|| BlameError::InvalidUrl(format!("Missing host in URL: {}", repo_url)))?;

        if !["github.com", "gitlab.com", "bitbucket.org"].contains(&host) && !host.contains("git") {
            // Not a recognized Git host, but we'll still try if it ends with .git
            if !repo_url.ends_with(".git") {
                return Err(BlameError::InvalidUrl(format!(
                    "Unrecognized Git host: {}",
                    host
                )));
            }
        }

        // Normalize the URL - ensure it ends with .git for consistency
        let normalized_url = if repo_url.ends_with(".git") {
            repo_url.to_string()
        } else {
            format!("{}.git", repo_url)
        };

        Ok(normalized_url)
    }

    /// Extract the repository name from a Git URL
    fn extract_repo_name(repo_url: &str) -> Result<String, BlameError> {
        let url = Url::parse(repo_url)
            .map_err(|_| BlameError::InvalidUrl(format!("Failed to parse URL: {}", repo_url)))?;

        // The path segments will include the username and repository name
        let path_segments: Vec<&str> = url.path().trim_start_matches('/').split('/').collect();

        if path_segments.len() < 2 {
            return Err(BlameError::InvalidUrl(format!(
                "URL does not appear to contain a repository path: {}",
                repo_url
            )));
        }

        // The last segment should be the repository name (possibly with .git)
        let repo_name = path_segments.last().unwrap().trim_end_matches(".git");

        // Create a unique identifier that includes the organization/user
        let owner = path_segments[path_segments.len() - 2];
        let qualified_name = format!("{}-{}", owner, repo_name);

        Ok(qualified_name)
    }

    /// Create a path for the local repository clone
    fn create_repo_path(repo_name: &str) -> Result<PathBuf, BlameError> {
        let repos_dir = Self::get_repos_dir()?;

        // Create a more unique folder name by adding a hash of the name
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        repo_name.hash(&mut hasher);
        let hash = hasher.finish();

        let repo_dir = repos_dir.join(format!("{}-{:x}", repo_name, hash));

        Ok(repo_dir)
    }

    /// Get the base directory for all repository clones
    pub fn get_repos_dir() -> Result<PathBuf, BlameError> {
        // Use a folder in the user's home directory
        let home_dir = dirs_next::home_dir().ok_or_else(|| {
            BlameError::DirectoryError("Could not determine home directory".to_string())
        })?;

        let repos_dir = home_dir.join(".oldest-todo-finder").join("repos");

        // Make sure the directory exists
        if !repos_dir.exists() {
            fs::create_dir_all(&repos_dir).map_err(|e| {
                BlameError::DirectoryError(format!(
                    "Failed to create repositories directory: {}",
                    e
                ))
            })?;
        }

        Ok(repos_dir)
    }

    /// Get the path to the local repository
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the name of the repository
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the URL of the repository
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Clone or update the repository
    pub async fn prepare(&self) -> Result<(), BlameError> {
        if self.path.exists() {
            debug!("path exists");
            // Repository already exists, just fetch latest changes
            self.update().await
        } else {
            debug!("path doesn't exist, cloning");
            // Repository doesn't exist yet, clone it
            self.clone().await
        }
    }

    /// Clone the repository
    async fn clone(&self) -> Result<(), BlameError> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                BlameError::DirectoryError(format!("Failed to create parent directory: {}", e))
            })?;
        }

        // Try main branch first, fall back to master if needed
        let result = self.clone_branch("main").await;
        if result.is_err() {
            self.clone_branch("master").await?;
        }

        // Deepen history after successful clone
        self.deepen_history(10000).await?;

        Ok(())
    }

    async fn clone_branch(&self, branch: &str) -> Result<(), BlameError> {
        // Clone the repository with optimizations
        let output = Command::new("git")
            .arg("clone")
            .arg("--single-branch")
            .arg("--branch")
            .arg(branch)
            .arg("--filter=blob:none")
            .arg("--depth=1000")
            .arg("-c")
            .arg("core.compression=0")
            .arg("-c")
            .arg("http.postBuffer=524288000")
            .arg("-c")
            .arg("pack.threads=8")
            .arg(&self.url)
            .arg(&self.path)
            .output()
            .await
            .map_err(|e| BlameError::GitError(format!("Failed to execute git clone: {}", e)))?;

        if !output.status.success() {
            return Err(BlameError::GitError(format!(
                "Git clone of branch '{}' failed: {}",
                branch,
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }

    async fn deepen_history(&self, additional_depth: u32) -> Result<(), BlameError> {
        let output = Command::new("git")
            .current_dir(&self.path)
            .arg("fetch")
            .arg("--deepen")
            .arg(additional_depth.to_string())
            .arg("origin")
            .output()
            .await
            .map_err(|e| BlameError::GitError(format!("Failed to deepen history: {}", e)))?;

        if !output.status.success() {
            return Err(BlameError::GitError(format!(
                "Failed to deepen history: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }

    /// Update an existing repository
    async fn update(&self) -> Result<(), BlameError> {
        // Fetch latest changes
        let output = Command::new("git")
            .current_dir(&self.path)
            .arg("fetch")
            .arg("--all")
            .output()
            .await
            .map_err(|e| BlameError::GitError(format!("Failed to execute git fetch: {}", e)))?;

        if !output.status.success() {
            return Err(BlameError::GitError(format!(
                "Git fetch failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        // Reset to match the fetched head
        let output = Command::new("git")
            .current_dir(&self.path)
            .arg("reset")
            .arg("--hard")
            .arg("origin/main") // Try main first
            .output()
            .await
            .map_err(|e| BlameError::GitError(format!("Failed to execute git reset: {}", e)))?;

        // If main doesn't exist, try master
        if !output.status.success() {
            let output = Command::new("git")
                .current_dir(&self.path)
                .arg("reset")
                .arg("--hard")
                .arg("origin/master")
                .output()
                .await
                .map_err(|e| BlameError::GitError(format!("Failed to execute git reset: {}", e)))?;

            if !output.status.success() {
                return Err(BlameError::GitError(format!(
                    "Git reset failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }

        // Update the last modified time
        let current_time = std::time::SystemTime::now();
        filetime::set_file_mtime(
            &self.path,
            filetime::FileTime::from_system_time(current_time),
        )
        .map_err(|e| BlameError::IoError(e))?;

        Ok(())
    }
}
