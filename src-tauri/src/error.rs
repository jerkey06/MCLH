use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum AppError {
    IoError(io::Error),
    ProcessError(String),
    ConfigError(String),
    ServerError(String),
    WebSocketError(String),
    JavaNotFound,
    ServerJarNotFound(PathBuf),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::IoError(err) => write!(f, "IO error: {}", err),
            AppError::ProcessError(err) => write!(f, "Process error: {}", err),
            AppError::ConfigError(err) => write!(f, "Configuration error: {}", err),
            AppError::ServerError(err) => write!(f, "Server error: {}", err),
            AppError::WebSocketError(err) => write!(f, "WebSocket error: {}", err),
            AppError::JavaNotFound => write!(f, "Java runtime not found"),
            AppError::ServerJarNotFound(path) => write!(f, "Server JAR not found at: {:?}", path),
        }
    }
}

impl std::error::Error for AppError {}

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> Self {
        AppError::IoError(err)
    }
}

pub type Result<T> = std::result::Result<T, AppError>;