#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod app_state;
mod commands;
mod config;
mod api;
mod monitoring;
mod models;
mod utils;
mod error;

use std::sync::Arc;
use tauri::{Manager, State};
use app_state::AppState;
use api::rest;
use api::websocket;
use utils::java_detector;

#[tauri::command]
async fn start_server(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    commands::process_manager::start_server(state.inner().clone())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_server(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    commands::process_manager::stop_server(state.inner().clone())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_server_status(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let status = state.server_status.lock().unwrap();
    Ok(status.to_string())
}

#[tauri::command]
async fn get_server_metrics(state: State<'_, Arc<AppState>>) -> Result<models::metrics::MetricsData, String> {
    let metrics = state.metrics.lock().unwrap().clone();
    Ok(metrics)
}

#[tauri::command]
async fn update_server_properties(
    properties: Vec<(String, String)>,
    state: State<'_, Arc<AppState>>
) -> Result<(), String> {
    config::server_properties::update_properties(properties, state.inner().clone())
        .map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let java_path = java_detector::find_java_path()
                .expect("Failed to find Java installation");

            let server_dir = app.path_resolver()
                .app_data_dir()
                .unwrap()
                .join("server");

            if !server_dir.exists() {
                std::fs::create_dir_all(&server_dir).unwrap();
            }

            let state = AppState::new(
                server_dir.to_string_lossy().into_owned(),
                java_path,
                "server.jar".to_string()
            );

            app.manage(state.clone());

            let app_handle = app.handle();
            monitoring::resource_monitor::start_monitoring(state.clone(), app_handle);
            websocket::start_websocket_server(state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            get_server_status,
            get_server_metrics,
            update_server_properties
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Tauri application");
}