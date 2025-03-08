use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlameError {
    #[error("Invalid repository URL: {0}")]
    InvalidUrl(String),

    #[error("Git operation failed: {0}")]
    GitError(String),

    #[error("Ripgrep search failed: {0}")]
    SearchError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse output: {0}")]
    ParseError(String),

    #[error("Failed to create or access directory: {0}")]
    DirectoryError(String),

    #[error("Failed to access or read file: {0}")]
    FileError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

// Allow conversion from anyhow::Error to our BlameError
impl From<anyhow::Error> for BlameError {
    fn from(err: anyhow::Error) -> Self {
        BlameError::InternalError(err.to_string())
    }
}
