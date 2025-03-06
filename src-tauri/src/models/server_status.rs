use std::fmt;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error(String),
}

impl fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerStatus::Starting => write!(f, "starting"),
            ServerStatus::Running => write!(f, "running"),
            ServerStatus::Stopping => write!(f, "stopping"),
            ServerStatus::Stopped => write!(f, "stopped"),
            ServerStatus::Error(err) => write!(f, "error: {}", err),
        }
    }
}