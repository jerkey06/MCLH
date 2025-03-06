use serde::{Serialize, Deserialize};
use std::sync::Arc;
use tauri::{command, State};

use crate::app_state::AppState;
use crate::commands::command_executor::CommandExecutor;
use crate::config::server_properties;
use crate::config::eula_manager;
use crate::models::server_status::ServerStatus;
use crate::models::metrics::MetricsData;
use crate::models::config::ServerConfig;

#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T> ApiResponse<T> {
    fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    fn error(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
        }
    }
}

#[command]
pub async fn get_server_status(state: State<'_, Arc<AppState>>) -> ApiResponse<String> {
    let status = state.server_status.lock().unwrap().clone();
    ApiResponse::success(status.to_string())
}

#[command]
pub async fn get_server_metrics(state: State<'_, Arc<AppState>>) -> ApiResponse<MetricsData> {
    let metrics = state.metrics.lock().unwrap().clone();
    ApiResponse::success(metrics)
}

#[command]
pub async fn start_server(state: State<'_, Arc<AppState>>) -> ApiResponse<()> {
    match crate::commands::process_manager::start_server(state.inner().clone()) {
        Ok(_) => ApiResponse::success(()),
        Err(e) => ApiResponse::error(e.to_string()),
    }
}

#[command]
pub async fn stop_server(state: State<'_, Arc<AppState>>) -> ApiResponse<()> {
    match crate::commands::process_manager::stop_server(state.inner().clone()) {
        Ok(_) => ApiResponse::success(()),
        Err(e) => ApiResponse::error(e.to_string()),
    }
}

#[command]
pub async fn restart_server(state: State<'_, Arc<AppState>>) -> ApiResponse<()> {
    let cmd_executor = CommandExecutor::new(state.inner().clone());

    match cmd_executor.execute("restart") {
        Ok(_) => ApiResponse::success(()),
        Err(e) => ApiResponse::error(e.to_string()),
    }
}

#[command]
pub async fn execute_command(
    command: String,
    state: State<'_, Arc<AppState>>
) -> ApiResponse<()> {
    let cmd_executor = CommandExecutor::new(state.inner().clone());

    match cmd_executor.execute(&command) {
        Ok(_) => ApiResponse::success(()),
        Err(e) => ApiResponse::error(e.to_string()),
    }
}

#[command]
pub async fn get_server_properties(state: State<'_, Arc<AppState>>) -> ApiResponse<ServerConfig> {
    match server_properties::read_properties(state.inner().clone()) {
        Ok(properties) => {
            let config = ServerConfig {
                server_properties: properties,
                java_args: state.inner().server_args.clone(),
                modpack: None,
            };
            ApiResponse::success(config)
        },
        Err(e) => ApiResponse::error(e.to_string()),
    }
}

#[command]
pub async fn update_server_properties(
    properties: Vec<(String, String)>,
    state: State<'_, Arc<AppState>>
) -> ApiResponse<()> {
    match server_properties::update_properties(properties, state.inner().clone()) {
        Ok(_) => ApiResponse::success(()),
        Err(e) => ApiResponse::error(e.to_string()),
    }
}

#[command]
pub async fn accept_eula(state: State<'_, Arc<AppState>>) -> ApiResponse<()> {
    match eula_manager::accept_eula(state.inner().clone()) {
        Ok(_) => ApiResponse::success(()),
        Err(e) => ApiResponse::error(e.to_string()),
    }
}

#[command]
pub async fn is_eula_accepted(state: State<'_, Arc<AppState>>) -> ApiResponse<bool> {
    match eula_manager::is_eula_accepted(state.inner().clone()) {
        Ok(accepted) => ApiResponse::success(accepted),
        Err(e) => ApiResponse::error(e.to_string()),
    }
}