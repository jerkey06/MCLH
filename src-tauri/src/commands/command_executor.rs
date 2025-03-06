use crate::app_state::AppState;
use crate::commands::process_manager;
use crate::error::Result;
use std::sync::Arc;

pub struct CommandExecutor {
    state: Arc<AppState>,
}

impl CommandExecutor {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub fn execute(&self, command: &str) -> Result<()> {
        match command {
            cmd if cmd.starts_with("/") => {
                process_manager::execute_command(self.state.clone(), cmd[1..].to_string())
            },
            "start" => process_manager::start_server(self.state.clone()),
            "stop" => process_manager::stop_server(self.state.clone()),
            "restart" => {
                process_manager::stop_server(self.state.clone())?;
                // Wait a bit for cleanup
                std::thread::sleep(std::time::Duration::from_secs(2));
                process_manager::start_server(self.state.clone())
            },
            _ => {
                // If not a special command, pass directly to server
                process_manager::execute_command(self.state.clone(), command.to_string())
            }
        }
    }
}