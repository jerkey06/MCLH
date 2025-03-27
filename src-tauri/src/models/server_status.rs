use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents the possible lifecycle states of the Minecraft server process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerStatus {
    /// The server process is starting up but not yet fully loaded.
    Starting,
    /// The server process is running and has likely finished loading.
    Running,
    /// The server process is shutting down gracefully.
    Stopping,
    /// The server process is not running.
    Stopped,
    /// The server encountered an error state (details in the String).
    Error(String),
    // Consider adding more states if needed, e.g., Backup, InstallingModpack
    // Backup,
    // Installing,
}

impl fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerStatus::Starting => write!(f, "Starting"), // Capitalized for UI consistency?
            ServerStatus::Running => write!(f, "Running"),
            ServerStatus::Stopping => write!(f, "Stopping"),
            ServerStatus::Stopped => write!(f, "Stopped"),
            ServerStatus::Error(err) => write!(f, "Error: {}", err),
            // ServerStatus::Backup => write!(f, "Backup in Progress"),
            // ServerStatus::Installing => write!(f, "Installing Modpack"),
        }
    }
}

impl Default for ServerStatus {
    /// The default status when the application starts.
    fn default() -> Self {
        ServerStatus::Stopped
    }
}