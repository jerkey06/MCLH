// src/monitoring/resource_monitor.rs

use crate::api::events::{self, emit_event, emit_info}; // Use helpers
use crate::app_state::AppState;
use crate::error::{AppError, Result}; // Use Result
use crate::models::metrics::MetricsData;
use crate::models::server_status::ServerStatus;
// Import collector and alerter
use crate::monitoring::alert_manager::AlertManager;
use crate::monitoring::metrics_collector::MetricsCollector;
use log::{debug, error, info, warn};
use std::path::PathBuf; // Import PathBuf
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessExt, System, SystemExt}; // Import Pid

const MONITOR_INTERVAL: Duration = Duration::from_secs(1); // Check every second

/// Starts the main monitoring loop in a separate thread.
///
/// - Periodically checks the server status.
/// - If running, uses `sysinfo` to get CPU/Memory for the Java process.
/// - Gathers other metrics (uptime, player count - placeholder).
/// - Updates `AppState.metrics`.
/// - Sends `MetricsUpdated` events.
/// - Calls `MetricsCollector::add_metrics`.
/// - Calls `AlertManager::check_alerts`.
pub async fn start_monitoring(
    state: Arc<AppState>,
    // Removed AppHandle - events are sent via MPSC now
    // app_handle: AppHandle,
    metrics_collector: Arc<MetricsCollector>, // Pass collector
    alert_manager: Arc<AlertManager>,       // Pass alerter
) {
    info!("Starting resource monitoring thread...");

    thread::spawn(move || {
        let mut sys = System::new_all();
        let mut server_pid: Option<Pid> = None; // Store the PID when found
        let mut last_metrics_update = Instant::now();
        let server_start_time: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

        loop {
            // --- Determine Target PID based on Status ---
            let status = match state.get_status() {
                Ok(s) => s,
                Err(e) => {
                    error!("Monitor: Failed to get server status: {}", e);
                    thread::sleep(MONITOR_INTERVAL * 5); // Wait longer on error
                    continue;
                }
            };

            // If running or starting, try to find/confirm the PID
            if status == ServerStatus::Running || status == ServerStatus::Starting {
                if server_pid.is_none() {
                    // Try to find the PID if we don't have it
                    sys.refresh_processes(); // Refresh process list
                    server_pid = find_server_pid(&sys, &state);
                    if server_pid.is_some() {
                        info!("Monitor: Found server process PID: {:?}", server_pid.unwrap());
                        // Reset start time when PID is first found while Running/Starting
                        let mut start_time_guard = server_start_time.lock().unwrap();
                        if start_time_guard.is_none() {
                            *start_time_guard = Some(Instant::now());
                            info!("Monitor: Server start time recorded.");
                        }
                    } else {
                        warn!("Monitor: Server status is {:?}, but process PID not found!", status);
                    }
                }
            } else {
                // If stopped, stopping or error, clear the PID and start time
                if server_pid.is_some() {
                    info!("Monitor: Server not running. Clearing PID and start time.");
                    server_pid = None;
                    let mut start_time_guard = server_start_time.lock().unwrap();
                    *start_time_guard = None;
                }
            }

            // --- Collect Metrics if PID is known ---
            if let Some(pid) = server_pid {
                // Refresh specific process and system info (more efficient than refresh_all)
                sys.refresh_process(pid);
                // Refresh memory only occasionally if needed for system total
                // sys.refresh_memory();

                if let Some(process) = sys.process(pid) {
                    // --- Create MetricsData ---
                    let current_time_secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or(Duration::ZERO)
                        .as_secs();

                    let uptime_secs = server_start_time.lock().unwrap()
                        .map_or(0, |start| start.elapsed().as_secs());

                    // TODO: Get player count and max players accurately
                    // This might involve parsing logs, RCON, or query protocol. Hardcoding for now.
                    let player_count = 0; // Placeholder
                    let max_players = state.server_properties // Assuming ServerConfig model exists
                        .get("max-players")
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0); // Get from actual config!

                    // TODO: Get TPS accurately (requires server interaction)
                    let tps = None; // Placeholder

                    let metrics = MetricsData {
                        timestamp: current_time_secs,
                        // Note: sysinfo cpu_usage() is often over the lifetime of the process.
                        // You might need to calculate usage between intervals manually for current load.
                        // Or rely on system-wide CPU load if process-specific isn't accurate enough.
                        cpu_usage: process.cpu_usage(),
                        memory_usage: process.memory(), // sysinfo memory is in Bytes directly
                        system_memory_total: sys.total_memory(), // Total system memory in Bytes
                        player_count,
                        max_players,
                        tps,
                        uptime: uptime_secs,
                    };
                    debug!("Collected Metrics: {:?}", metrics);

                    // --- Update Shared State ---
                    if let Err(e) = state.update_metrics(metrics.clone()) {
                        error!("Monitor: Failed to update AppState metrics: {}", e);
                    }

                    // --- Add to Collector ---
                    if let Err(e) = metrics_collector.add_metrics(metrics.clone()) {
                        error!("Monitor: Failed to add metrics to collector: {}", e);
                    }

                    // --- Check Alerts ---
                    alert_manager.check_alerts(&metrics);


                    // --- Emit Event (Rate Limited) ---
                    if last_metrics_update.elapsed() >= MONITOR_INTERVAL {
                        emit_event(events::Event::MetricsUpdated(metrics.clone()));
                        // No need for separate Tauri event, bridge handles it
                        // let _ = app_handle.emit_all("metrics-updated", metrics);
                        last_metrics_update = Instant::now();
                    }

                } else {
                    // Process with stored PID not found - it probably terminated unexpectedly
                    error!("Monitor: Server process with PID {:?} disappeared!", pid);
                    server_pid = None; // Clear PID
                    let mut start_time_guard = server_start_time.lock().unwrap();
                    *start_time_guard = None; // Clear start time

                    // Update status if it wasn't already Stopping/Stopped
                    if let Ok(current_status @ (ServerStatus::Running | ServerStatus::Starting)) = state.get_status() {
                        warn!("Monitor: Updating server status to Stopped due to process disappearance.");
                        if state.set_status(ServerStatus::Stopped).is_ok() {
                            events::emit_status_change(ServerStatus::Stopped);
                            events::emit_warn("Server process stopped unexpectedly (disappeared).".to_string(), "Monitor".to_string());
                        } else {
                            error!("Monitor: Failed to lock state to set status to Stopped after process disappearance.");
                        }
                        // Clear handle in AppState just in case
                        let _ = state.set_process_handle(None);
                    }
                }
            } // end if let Some(pid)

            // --- Wait for next cycle ---
            thread::sleep(MONITOR_INTERVAL);

        } // end loop
        // info!("Resource monitoring thread finished."); // Loop should not finish
    }); // end thread::spawn
}


/// Helper to find the PID of the Java server process.
/// Looks for a "java" process whose command line arguments include the server JAR name.
fn find_server_pid(sys: &System, state: &Arc<AppState>) -> Option<Pid> {
    let server_jar_name = &state.server_jar; // Get the JAR name from state
    debug!("Searching for java process matching JAR: {}", server_jar_name);

    // Iterate through processes named "java" (or "java.exe" on Windows?)
    for (pid, process) in sys.processes_by_exact_name("java") { // Use exact name if possible
        // Check command line arguments
        let cmd_line = process.cmd().join(" ");
        // Simple check: does command line contain the JAR name?
        // This might need refinement if multiple java servers run or args are complex.
        if cmd_line.contains(server_jar_name) {
            debug!("Found potential match PID: {}, Cmd: '{}'", pid, cmd_line);
            // Add more checks? e.g., is it running in the state.server_directory?
            // if process.cwd() == Some(&state.server_directory) { ... }
            return Some(*pid); // Return the first match found
        }
    }

    // Check for java.exe on Windows if "java" not found
    #[cfg(target_os = "windows")]
    {
        for (pid, process) in sys.processes_by_exact_name("java.exe") {
            let cmd_line = process.cmd().join(" ");
            if cmd_line.contains(server_jar_name) {
                debug!("Found potential match PID: {}, Cmd: '{}'", pid, cmd_line);
                return Some(*pid);
            }
        }
    }


    debug!("No matching Java process found.");
    None // No matching process found
}