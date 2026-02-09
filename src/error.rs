use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SyncroError {
    #[error("config error: {0}")]
    Config(String),
    #[error("git error in {repo}: {message}")]
    Git { repo: String, message: String },
    #[error("path not found: {0}")]
    PathNotFound(PathBuf),
    #[error("not a git repository: {0}")]
    NotAGitRepo(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
