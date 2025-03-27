use crate::error::AppError; // For potential future use
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Represents a single log message with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// UNIX timestamp (seconds since epoch) when the log was created.
    pub timestamp: u64,
    /// Severity level of the log message.
    pub level: LogLevel,
    /// The actual log message content.
    pub message: String,
    /// Source identifier (e.g., "Server", "ProcessManager", "ModpackInstaller").
    pub source: String,
}

/// Defines the severity levels for log entries.
/// Matches standard logging levels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)] // Added PartialEq, Eq, Hash
pub enum LogLevel {
    Info,
    Warn, // Changed from Warning for consistency with `log` crate
    Error,
    Debug,
    Trace, // Added Trace level
}

impl LogEntry {
    /// Creates a new LogEntry instance.
    pub fn new(level: LogLevel, message: String, source: String) -> Self { // Changed level type
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|e| {
                // Handle system time error gracefully (log and use 0)
                eprintln!("WARN: System time is before UNIX EPOCH: {}", e);
                Duration::ZERO // Use Duration::ZERO from std::time
            })
            .as_secs();

        Self {
            timestamp,
            level, // Use the passed LogLevel directly
            message,
            source,
        }
    }

    // Convenience functions using the corrected `new`

    pub fn info(message: String, source: String) -> Self {
        Self::new(LogLevel::Info, message, source)
    }

    pub fn warn(message: String, source: String) -> Self { // Renamed from warning
        Self::new(LogLevel::Warn, message, source)
    }

    pub fn error(message: String, source: String) -> Self {
        Self::new(LogLevel::Error, message, source)
    }

    pub fn debug(message: String, source: String) -> Self {
        Self::new(LogLevel::Debug, message, source)
    }

    pub fn trace(message: String, source: String) -> Self {
        Self::new(LogLevel::Trace, message, source)
    }
}

// Implement Display for LogLevel for easy printing
impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Trace => write!(f, "TRACE"),
        }
    }
}

// Optional: Allow conversion from `log::Level` (from the `log` crate)
impl From<log::Level> for LogLevel {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => LogLevel::Error,
            log::Level::Warn => LogLevel::Warn,
            log::Level::Info => LogLevel::Info,
            log::Level::Debug => LogLevel::Debug,
            log::Level::Trace => LogLevel::Trace,
        }
    }
}

// Optional: Allow conversion to `log::Level`
impl From<LogLevel> for log::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => log::Level::Error,
            LogLevel::Warn => log::Level::Warn,
            LogLevel::Info => log::Level::Info,
            LogLevel::Debug => log::Level::Debug,
            LogLevel::Trace => log::Level::Trace,
        }
    }
}