use crate::api::events::{emit_app_error, emit_eula_status, emit_event, Event}; // Use event emitters
use crate::app_state::AppState;
use crate::config::{eula_manager, modpack_installer, server_properties}; // Added modpack_installer
use crate::error::{AppError, Result}; // Use our Result and AppError
use crate::models::config::ServerConfig; // Assuming this struct exists and is Serialize/Deserialize
use crate::models::metrics::MetricsData;
use crate::models::server_status::ServerStatus;
// Import process_manager for start/stop/command/restart
use crate::commands::process_manager;
use log::{error, info}; // Use log crate
use serde::Serialize;
use std::sync::Arc;
use tauri::{command, AppHandle, Manager, State}; // Manager might not be needed if using MPSC only

/// Standard API response structure for Tauri commands.
#[derive(Debug, Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

// --- ApiResponse Helpers ---

impl<T: Serialize> ApiResponse<T> {
    /// Creates a success response with data.
    fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Creates a success response without data.
    fn success_empty() -> Self {
        Self {
            success: true,
            data: None,
            error: None,
        }
    }

    /// Creates an error response.
    fn error(error_message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error_message),
        }
    }

    /// Creates an ApiResponse from a Result<T, AppError>.
    fn from_result(result: Result<T>) -> Self {
        match result {
            Ok(data) => Self::success(data),
            Err(e) => {
                // Log the detailed error on the backend
                error!("API command failed: {}", e);
                // Optionally emit an error event to the frontend as well
                // emit_app_error(&e);
                // Return a user-friendly error message string
                Self::error(e.to_string())
            }
        }
    }

    /// Creates an ApiResponse from a Result<(), AppError>.
    fn from_empty_result(result: Result<()>) -> Self {
        match result {
            Ok(_) => Self::success_empty(),
            Err(e) => {
                error!("API command failed: {}", e);
                // emit_app_error(&e);
                Self::error(e.to_string())
            }
        }
    }
}

// --- Tauri Commands ---

/// Gets the current server status.
#[command]
pub async fn get_server_status(state: State<'_, Arc<AppState>>) -> ApiResponse<ServerStatus> {
    ApiResponse::from_result(state.get_status())
}

/// Gets the latest server performance metrics.
#[command]
pub async fn get_server_metrics(state: State<'_, Arc<AppState>>) -> ApiResponse<MetricsData> {
    ApiResponse::from_result(state.get_metrics())
}

/// Starts the Minecraft server process.
#[command]
pub async fn start_server(state: State<'_, Arc<AppState>>) -> ApiResponse<()> {
    info!("'start_server' command received.");
    // Clone Arc for the new task
    let app_state_clone = state.inner().clone();
    // Run blocking task in Tokio's blocking thread pool
    let result = tokio::task::spawn_blocking(move || {
        process_manager::start_server(app_state_clone) // This function should emit StatusChanged events
    })
        .await;

    // Handle potential Tokio task join error and the inner Result
    match result {
        Ok(inner_result) => ApiResponse::from_empty_result(inner_result),
        Err(join_error) => {
            error!("Task execution error for start_server: {}", join_error);
            ApiResponse::error(format!("Failed to execute start task: {}", join_error))
        }
    }
}

/// Stops the Minecraft server process gracefully.
#[command]
pub async fn stop_server(state: State<'_, Arc<AppState>>) -> ApiResponse<()> {
    info!("'stop_server' command received.");
    let app_state_clone = state.inner().clone();
    let result = tokio::task::spawn_blocking(move || {
        process_manager::stop_server(app_state_clone) // Should emit StatusChanged events
    })
        .await;

    match result {
        Ok(inner_result) => ApiResponse::from_empty_result(inner_result),
        Err(join_error) => {
            error!("Task execution error for stop_server: {}", join_error);
            ApiResponse::error(format!("Failed to execute stop task: {}", join_error))
        }
    }
}

/// Restarts the Minecraft server (stop + start).
#[command]
pub async fn restart_server(state: State<'_, Arc<AppState>>) -> ApiResponse<()> {
    info!("'restart_server' command received.");
    let app_state_clone = state.inner().clone();
    // We can run this sequentially or create a dedicated restart function
    // Using spawn_blocking as stop/start involve blocking I/O potentially
    let result = tokio::task::spawn_blocking(move || {
        process_manager::restart_server(app_state_clone) // This function should handle stop, wait, start logic
    })
        .await;

    match result {
        Ok(inner_result) => ApiResponse::from_empty_result(inner_result),
        Err(join_error) => {
            error!("Task execution error for restart_server: {}", join_error);
            ApiResponse::error(format!("Failed to execute restart task: {}", join_error))
        }
    }
}

/// Sends a command string to the running Minecraft server's input.
#[command]
pub async fn execute_command(command: String, state: State<'_, Arc<AppState>>) -> ApiResponse<()> {
    info!("'execute_command' received: {}", command);
    let app_state_clone = state.inner().clone();
    let command_clone = command.clone();

    // Sending command might be quick I/O, but depends on implementation
    // Using spawn_blocking is safer if process_manager interacts with stdin/stdout pipes in a blocking way
    let result = tokio::task::spawn_blocking(move || {
        process_manager::send_command_to_server(app_state_clone, &command_clone)
    })
        .await;

    match result {
        Ok(inner_result) => {
            // Optionally emit CommandExecuted event here or within send_command_to_server
            match &inner_result {
                Ok(_) => emit_event(Event::CommandExecuted { command, success: true, output: None }),
                Err(e) => emit_event(Event::CommandExecuted { command, success: false, output: Some(e.to_string())}),
            }
            ApiResponse::from_empty_result(inner_result)
        },
        Err(join_error) => {
            error!("Task execution error for execute_command: {}", join_error);
            emit_event(Event::CommandExecuted{ command, success: false, output: Some(join_error.to_string())});
            ApiResponse::error(format!("Failed to execute command task: {}", join_error))
        }
    }
}

/// Retrieves the complete server configuration (properties, Java args, etc.).
#[command]
pub async fn get_server_config(state: State<'_, Arc<AppState>>) -> ApiResponse<ServerConfig> {
    info!("'get_server_config' command received.");
    // Reading config might involve file I/O, consider spawn_blocking if it becomes slow
    // Assuming read_config_fully is relatively fast for now
    match server_properties::read_config_fully(state.inner().clone()) {
        Ok(config) => ApiResponse::success(config),
        Err(e) => {
            // Emit specific error event if desired
            emit_app_error(&e);
            ApiResponse::error(format!("Failed to read server configuration: {}", e))
        }
    }
}

/// Updates the server configuration.
#[command]
pub async fn update_server_config(
    config: ServerConfig, // Receive the full config object
    state: State<'_, Arc<AppState>>,
) -> ApiResponse<()> {
    info!("'update_server_config' command received.");
    let app_state_clone = state.inner().clone();
    // Saving config involves file I/O, use spawn_blocking
    let result = tokio::task::spawn_blocking(move || {
        server_properties::update_config_fully(config, app_state_clone)
    }).await;

    match result {
        Ok(inner_result) => ApiResponse::from_empty_result(inner_result),
        Err(join_error) => {
            error!("Task execution error for update_server_config: {}", join_error);
            ApiResponse::error(format!("Failed to execute config update task: {}", join_error))
        }
    }
}

/// Accepts the Minecraft EULA.
#[command]
pub async fn accept_eula(state: State<'_, Arc<AppState>>) -> ApiResponse<()> {
    info!("'accept_eula' command received.");
    let app_state_clone = state.inner().clone();
    // Writing eula.txt is fast I/O, spawn_blocking likely not essential but harmless
    let result = tokio::task::spawn_blocking(move || {
        eula_manager::accept_eula(app_state_clone)
    }).await;

    match result {
        Ok(inner_result) => {
            if inner_result.is_ok() {
                // Emit event only on successful acceptance
                emit_eula_status(true);
            }
            ApiResponse::from_empty_result(inner_result)
        },
        Err(join_error) => {
            error!("Task execution error for accept_eula: {}", join_error);
            ApiResponse::error(format!("Failed to execute EULA acceptance task: {}", join_error))
        }
    }
}

/// Checks if the EULA has been accepted.
#[command]
pub async fn is_eula_accepted(state: State<'_, Arc<AppState>>) -> ApiResponse<bool> {
    info!("'is_eula_accepted' command received.");
    // Reading eula.txt is fast I/O
    ApiResponse::from_result(eula_manager::is_eula_accepted(state.inner().clone()))
}

/// Installs or updates a modpack from a given URL or identifier.
#[command]
pub async fn install_modpack(url: String, state: State<'_, Arc<AppState>>) -> ApiResponse<()> {
    info!("'install_modpack' command received for URL: {}", url);
    let app_state_clone = state.inner().clone();
    let url_clone = url.clone();

    // Modpack installation involves network I/O and file I/O (heavy), use spawn_blocking
    let result = tokio::task::spawn_blocking(move || {
        // This function should emit ProgressUpdate events
        modpack_installer::install(app_state_clone, &url_clone)
    })
        .await;

    match result {
        Ok(inner_result) => ApiResponse::from_empty_result(inner_result),
        Err(join_error) => {
            error!("Task execution error for install_modpack: {}", join_error);
            ApiResponse::error(format!("Failed to execute modpack install task: {}", join_error))
        }
    }
}

/// Creates a backup of the server world and potentially configuration.
#[command]
pub async fn create_backup(state: State<'_, Arc<AppState>>) -> ApiResponse<()> {
    info!("'create_backup' command received.");
    let app_state_clone = state.inner().clone();

    // Backup involves file I/O (potentially heavy), use spawn_blocking
    let result = tokio::task::spawn_blocking(move || {
        // Replace with actual backup logic, which should emit BackupStarted/Completed events
        // crate::backup::create_backup(app_state_clone)
        emit_event(Event::BackupStarted);
        // Simulate work
        std::thread::sleep(std::time::Duration::from_secs(2));
        let backup_result: Result<(), String> = Err("Backup feature not fully implemented".to_string()); // Placeholder
        emit_event(Event::BackupCompleted(backup_result.clone()));

        if let Err(e) = backup_result {
            Err(AppError::BackupError(e)) // Convert String error to AppError
        } else {
            Ok(())
        }

    }).await;

    match result {
        Ok(inner_result) => ApiResponse::from_empty_result(inner_result),
        Err(join_error) => {
            error!("Task execution error for create_backup: {}", join_error);
            emit_event(Event::BackupCompleted(Err(join_error.to_string())));
            ApiResponse::error(format!("Failed to execute backup task: {}", join_error))
        }
    }
}