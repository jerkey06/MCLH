// src/lib.rs

// Declare modules for the library
pub mod api;
pub mod app_state;
pub mod commands;
pub mod config;
pub mod error;
pub mod models;
pub mod monitoring;
pub mod utils;

// --- Imports ---
use crate::api::events::{self, Event, TAURI_BACKEND_EVENT};
use crate::app_state::AppState;
use crate::config::{eula_manager, server_properties}; // Import specific config modules
use crate::error::{AppError, Result};
// Import monitoring components
use crate::monitoring::{
    alert_manager::AlertManager, metrics_collector::MetricsCollector, resource_monitor,
};
use crate::utils::java_detector;
use log::{debug, error, info, warn}; // Use log crate
use std::{
    fs, // Filesystem operations
    path::PathBuf,
    sync::{mpsc, Arc}, // Standard channel and Atomic Ref Counting
    thread,             // For event bridge thread
};
use tauri::{AppHandle, Manager, State}; // Tauri specific imports

// --- Constants ---
// You could centralize more config keys here if desired

// --- Event Bridge Setup ---

/// Sets up and runs the MPSC -> Tauri event bridge.
/// This runs in a separate thread, listening for internal backend events
/// and emitting them to the Tauri frontend.
fn setup_event_bridge(app_handle: AppHandle, event_receiver: mpsc::Receiver<Event>) {
    let handle = app_handle.clone(); // Clone handle for the thread

    thread::spawn(move || {
        info!("Event bridge MPSC -> Tauri started.");
        // Loop indefinitely, receiving events from the backend channel
        while let Ok(event) = event_receiver.recv() {
            debug!("Event bridge received: {:?}", event); // Log received event

            // Emit the event to all frontend windows using the predefined event name
            if let Err(e) = handle.emit_all(TAURI_BACKEND_EVENT, &event) {
                warn!(
                    "Failed to emit Tauri event '{}': {}",
                    TAURI_BACKEND_EVENT, e
                );
            } else {
                // Use trace for production to avoid spamming logs
                log::trace!(
                    "Emitted Tauri event of type: {}",
                    std::any::type_name_of_val(&event)
                );
            }
        }
        // If recv() returns Err, the sender has been dropped (app shutting down)
        info!("Event bridge MPSC -> Tauri stopped (sender closed).");
    });
}

// --- Application Initialization ---

/// Initializes application state, background tasks, and performs initial checks.
/// This is called from within Tauri's `setup` closure.
fn initialize_app(app: &mut tauri::App) -> Result<()> {
    info!("Initializing application backend...");
    let app_handle = app.handle();

    // --- 1. Detect Java ---
    let java_path =
        java_detector::find_java_path().map_err(|_| AppError::JavaNotFound)?; // Convert error if needed
    info!("Java found at: {:?}", java_path);

    // --- 2. Determine Server Directory & Log Directory ---
    let app_data_dir = app_handle
        .path_resolver()
        .app_data_dir()
        .ok_or_else(|| AppError::ConfigError("Could not determine app data directory".to_string()))?;

    let server_dir = app_data_dir.join("server"); // Store server files in AppData/YourApp/server

    let log_dir = app_handle
        .path_resolver()
        .app_log_dir() // Use dedicated log dir from Tauri
        .ok_or_else(|| AppError::ConfigError("Could not determine app log directory".to_string()))?;

    // --- 3. Ensure Directories Exist ---
    for dir in [&server_dir, &log_dir] {
        if !dir.exists() {
            info!("Creating directory: {}", dir.display());
            fs::create_dir_all(dir).map_err(|e| {
                AppError::IoError(std::io::Error::new(
                    e.kind(),
                    format!("Failed to create directory {}: {}", dir.display(), e),
                ))
            })?;
        } else {
            info!("Using existing directory: {}", dir.display());
        }
    }

    // --- 4. Create Event Channel & Set Global Sender ---
    let (event_sender, event_receiver) = events::create_event_channel();
    events::set_event_sender(event_sender);

    // --- 5. Create and Manage AppState ---
    // TODO: Load server_jar name and potentially java_args from a persistent config file
    let server_jar_name = "server.jar".to_string(); // Default, maybe loaded from config
    let app_state = AppState::new(
        server_dir.to_string_lossy().into_owned(),
        java_path.to_string_lossy().into_owned(),
        server_jar_name,
    )?; // Propagate error from AppState::new if any
    app.manage(app_state.clone()); // Make AppState available via app.state()
    info!("AppState initialized and managed.");

    // --- 6. Create Monitoring Components ---
    // MetricsCollector needs the log directory path
    let metrics_collector = Arc::new(MetricsCollector::new(log_dir.clone()));
    // AlertManager uses default thresholds initially (should be configurable later)
    let alert_manager = Arc::new(AlertManager::new());
    // Store these Arcs in AppState if other parts of the app need to access them directly?
    // For now, we only pass them to the monitoring thread.
    // app.manage(metrics_collector.clone()); // Optional: If needed via Tauri state
    // app.manage(alert_manager.clone());    // Optional: If needed via Tauri state

    // --- 7. Start Event Bridge ---
    // Needs to run after event sender is set and potentially after other components are ready
    setup_event_bridge(app_handle.clone(), event_receiver);

    // --- 8. Start Background Tasks ---
    info!("Starting background monitoring task...");
    // Clone Arcs needed for the monitoring task
    let monitoring_state = app_state.clone();
    let mc_clone = metrics_collector.clone();
    let am_clone = alert_manager.clone();
    // Spawn the monitoring task using tokio
    tokio::spawn(async move {
        // Pass state, collector, and alerter
        crate::monitoring::resource_monitor::start_monitoring(monitoring_state, mc_clone, am_clone)
            .await;
        // This task should ideally run for the lifetime of the application.
        // If it finishes, something went wrong or the design needs review.
        warn!("Resource monitoring task finished unexpectedly!");
    });

    // --- 9. Perform Initial Config/State Checks ---
    info!("Performing initial configuration checks...");
    // Ensure default server.properties exists if needed
    if let Err(e) = server_properties::create_default_properties_if_missing(&app_state) {
        error!("Failed to ensure default server properties exist: {}", e);
        // Decide if this is critical - maybe emit an error event
        events::emit_app_error(&e);
    }

    // Check EULA status and emit initial event
    let eula_check_state = app_state.clone();
    tokio::spawn(async move {
        match eula_manager::is_eula_accepted(&eula_check_state) {
            Ok(accepted) => {
                info!("Initial EULA accepted status: {}", accepted);
                events::emit_eula_status(accepted);
            }
            Err(e) => {
                error!("Failed to check initial EULA status: {}", e);
                events::emit_app_error(&e);
            }
        }
    });

    // TODO: Check if a modpack is installed and load its info into ServerConfig/AppState?
    // TODO: Load persisted ServerConfig (including java_args, thresholds) if it exists.

    info!("Backend initialization complete.");
    Ok(())
}

// --- Tauri Entry Point ---

/// Tauri command example (keep if useful, remove otherwise)
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Main entry point for the Tauri application setup and run.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Note: Logger should be initialized in main.rs *before* calling this run function.
    info!("Starting Tauri application setup...");

    // --- Build Tauri Application ---
    let builder_result = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init()) // Use the shell plugin
        // .manage() // AppState, Collector, Alerter are managed inside .setup() now
        .setup(|app| {
            // Perform initialization that requires the App handle
            if let Err(e) = initialize_app(app) {
                // Use logging and maybe a dialog box to inform the user
                error!("Critical error during application initialization: {}", e);
                // Use Tauri dialog API to show error to the user
                let handle = app.handle();
                handle
                    .dialog()
                    .message(format!(
                        "A critical error occurred during startup:\n\n{}\n\nThe application might not function correctly.",
                        e
                    ))
                    .title("Initialization Failed")
                    .kind(tauri::api::dialog::MessageDialogKind::Error)
                    .show(|_| {}); // Show dialog asynchronously

                // Decide how to proceed. Allow app to run but potentially broken, or exit?
                // For a server manager, likely best to prevent broken state.
                // std::process::exit(1); // Force exit (might be abrupt)
                // Or return an error if Tauri's setup supports clean abortion (check Tauri v2 docs)
                // return Err(Box::new(e)); // Example if setup supported Box<dyn Error>
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // --- Register ALL commands from api::rest ---
            greet, // Keep example command?
            api::rest::get_server_status,
            api::rest::get_server_metrics,
            api::rest::start_server,
            api::rest::stop_server,
            api::rest::restart_server,
            api::rest::execute_command,
            api::rest::get_server_config, // Changed from get_server_properties
            api::rest::update_server_config, // Changed from update_server_properties
            api::rest::accept_eula,
            api::rest::is_eula_accepted,
            api::rest::install_modpack,
            api::rest::create_backup,
            // TODO: Add commands for get/set alert thresholds
        ])
        .build(tauri::generate_context!()); // Use build() before run()

    // --- Run Tauri Application ---
    match builder_result {
        Ok(app) => {
            info!("Tauri application built successfully. Running...");
            app.run(|_app_handle, event| {
                // --- Optional: Handle Tauri Runtime Events ---
                match event {
                    tauri::RunEvent::ExitRequested { api, .. } => {
                        info!("Tauri exit requested.");
                        // Example: Prevent exit if server is running
                        // let state = _app_handle.state::<Arc<AppState>>();
                        // match state.get_status() {
                        //     Ok(ServerStatus::Stopped) | Ok(ServerStatus::Error(_)) => {
                        //         info!("Server stopped or errored. Allowing exit.");
                        //     }
                        //     _ => {
                        //         info!("Server is running or changing state. Preventing immediate exit.");
                        //         api.prevent_exit();
                        //         // Optionally: Trigger stop_server command via invoke or directly
                        //         // let _ = _app_handle.invoke("stop_server", ()); // Example, check payload
                        //         // Show dialog to user?
                        //           _app_handle.dialog().message("Please stop the server before closing the application.")
                        //             .title("Server Running")
                        //             .ok_button("OK")
                        //             .show(|_| {});
                        //     }
                        // }
                    }
                    tauri::RunEvent::Exit => {
                        info!("Tauri application exiting.");
                        // Perform any final cleanup here if needed (though graceful shutdown is preferred)
                    }
                    // Handle other events like WindowCreated, WindowCloseRequested etc. if needed
                    _ => {}
                }
            });
            info!("Tauri run loop finished."); // Usually only reached after all windows close
        }
        Err(e) => {
            // Failed to build the Tauri application itself
            error!("Failed to build Tauri application: {}", e);
            // Maybe show a native OS error message box here before exiting
            panic!("Failed to build Tauri application: {}", e); // Panic as a last resort
        }
    }
}