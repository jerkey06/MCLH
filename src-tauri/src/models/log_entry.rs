use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: u64,
    pub level: LogLevel,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
    Debug,
}

impl LogEntry {
    pub fn new(message: String, level: LogLevel, source: String) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            level,
            message,
            source,
        }
    }

    pub fn info(message: String, source: String) -> Self {
        Self::new(message, LogLevel::Info, source)
    }

    pub fn warning(message: String, source: String) -> Self {
        Self::new(message, LogLevel::Warning, source)
    }

    pub fn error(message: String, source: String) -> Self {
        Self::new(message, LogLevel::Error, source)
    }

    pub fn debug(message: String, source: String) -> Self {
        Self::new(message, LogLevel::Debug, source)
    }
}