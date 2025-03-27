use std::{fmt, io, path::PathBuf};
use serde::Serialize; // Added for potential future error serialization

/// Custom error types for the application.
#[derive(Debug)]
pub enum AppError {
    IoError(io::Error),
    ProcessError(String),
    ConfigError(String),
    ServerError(String),
    // WebSocketError removed
    JavaNotFound,
    ServerJarNotFound(PathBuf),
    LockError(String), // For Mutex/RwLock poisoning errors
    InternalEventError(String), // For errors related to the event system itself
    NotImplemented(String), // Placeholder for features not yet implemented
    ModpackError(String), // Specific errors during modpack installation
    BackupError(String), // Specific errors during backup
    // Add other specific error types as needed
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::IoError(err) => write!(f, "IO error: {}", err),
            AppError::ProcessError(err) => write!(f, "Process error: {}", err),
            AppError::ConfigError(err) => write!(f, "Configuration error: {}", err),
            AppError::ServerError(err) => write!(f, "Server logic error: {}", err),
            AppError::JavaNotFound => write!(f, "Java runtime not found on this system"),
            AppError::ServerJarNotFound(path) => write!(f, "Server JAR file not found at: {:?}", path),
            AppError::LockError(msg) => write!(f, "Concurrency lock error: {}", msg),
            AppError::InternalEventError(msg) => write!(f, "Internal event system error: {}", msg),
            AppError::NotImplemented(feature) => write!(f, "Feature not implemented yet: {}", feature),
            AppError::ModpackError(msg) => write!(f, "Modpack installation failed: {}", msg),
            AppError::BackupError(msg) => write!(f, "Backup operation failed: {}", msg),
        }
    }
}

// Implement the standard Error trait
impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::IoError(err) => Some(err),
            // Other variants don't wrap another error directly in this basic form
            _ => None,
        }
    }
}

// Allow converting io::Error into AppError easily
impl From<io::Error> for AppError {
    fn from(err: ioError) -> Self {
        AppError::IoError(err)
    }
}

// Allow converting String (e.g., from poisoned locks) into AppError easily
impl From<String> for AppError {
    fn from(err: String) -> Self {
        // Decide on a default error type or require more context
        // For now, let's use LockError as a common case for mutex issues
        AppError::LockError(err)
    }
}


/// Type alias for Result using the application's custom error type.
pub type Result<T> = std::result::Result<T, AppError>;