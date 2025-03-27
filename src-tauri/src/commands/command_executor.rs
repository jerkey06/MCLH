use crate::app_state::AppState;
use crate::commands::process_manager;
use crate::error::{AppError, Result};
use log::debug;
use std::sync::Arc;

/// Responsible for interpreting user-level commands and delegating
/// actions to the appropriate process management functions.
pub struct CommandExecutor {
    state: Arc<AppState>,
}

impl CommandExecutor {
    /// Creates a new CommandExecutor.
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Executes a given command string.
    ///
    /// Handles keywords like "start", "stop", "restart".
    /// Any other non-empty string is treated as a command to be sent
    /// directly to the running Minecraft server console.
    pub fn execute(&self, command: &str) -> Result<()> {
        debug!("Executing command via CommandExecutor: {}", command);
        let command_trimmed = command.trim(); // Trim whitespace

        if command_trimmed.is_empty() {
            return Err(AppError::ProcessError("Command cannot be empty.".to_string()));
        }

        match command_trimmed {
            "start" => process_manager::start_server(self.state.clone()),
            "stop" => process_manager::stop_server(self.state.clone()),
            "restart" => process_manager::restart_server(self.state.clone()),
            // Any other command is passed directly to the server process
            _ => process_manager::send_command_to_server(self.state.clone(), command_trimmed.to_string()),
        }
    }
}