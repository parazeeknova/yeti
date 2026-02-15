use std::fmt;

#[derive(Debug)]
pub enum YetiError {
    NotAGitRepo,
    NoChangesToCommit,
    InvalidApiKey(String),
    ApiError { status: u16, message: String },
    NetworkError(String),
    CommitFailed(String),
    IoError(String),
}

impl fmt::Display for YetiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            YetiError::NotAGitRepo => write!(f, "Not inside a git repository"),
            YetiError::NoChangesToCommit => write!(f, "No changes to commit"),
            YetiError::InvalidApiKey(msg) => write!(f, "Invalid API key: {}", msg),
            YetiError::ApiError { status, message } => {
                write!(f, "API error ({}): {}", status, message)
            }
            YetiError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            YetiError::CommitFailed(msg) => write!(f, "Git commit failed: {}", msg),
            YetiError::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for YetiError {}

impl From<std::io::Error> for YetiError {
    fn from(err: std::io::Error) -> Self {
        YetiError::IoError(err.to_string())
    }
}

impl From<git2::Error> for YetiError {
    fn from(err: git2::Error) -> Self {
        YetiError::IoError(err.to_string())
    }
}

impl From<toml::de::Error> for YetiError {
    fn from(err: toml::de::Error) -> Self {
        YetiError::IoError(format!("Config parse error: {}", err))
    }
}

impl From<serde_json::Error> for YetiError {
    fn from(err: serde_json::Error) -> Self {
        YetiError::IoError(format!("JSON error: {}", err))
    }
}

pub type Result<T> = std::result::Result<T, YetiError>;
