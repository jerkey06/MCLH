use crate::api::events::{self, emit_event, emit_log, emit_warn}; // Use helpers
use crate::app_state::AppState;
use crate::error::{AppError, Result}; // Use Result
use crate::models::log_entry::LogLevel; // Import LogLevel
use crate::models::metrics::MetricsData;
use crate::models::server_status::ServerStatus;
// Import collector and alerter
use crate::monitoring::alert_manager::AlertManager;
use crate::monitoring::metrics_collector::MetricsCollector;
use log::{debug, error, info, trace, warn};
use std::path::PathBuf; // Import PathBuf
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH}; // Import SystemTime, UNIX_EPOCH
use sysinfo::{Pid, ProcessExt, System, SystemExt}; // Import Pid

const MONITOR_INTERVAL: Duration = Duration::from_secs(1); // Check every second

/// Starts the main monitoring loop in a separate thread.
///
/// - Periodically checks the server status.
/// - If running, uses `sysinfo` to get CPU/Memory for the Java process.
/// - Gathers other metrics (uptime, player count from AppState).
/// - Updates `AppState.metrics`.
/// - Sends `MetricsUpdated` events via MPSC channel.
/// - Calls `MetricsCollector::add_metrics`.
/// - Calls `AlertManager::check_alerts`.
pub async fn start_monitoring(
    state: Arc<AppState>,
    metrics_collector: Arc<MetricsCollector>,
    alert_manager: Arc<AlertManager>,
) {
    info!("Starting resource monitoring thread...");

    thread::spawn(move || {
        let mut sys = System::new_all();
        let mut server_pid: Option<Pid> = None; // Store the PID when found
        let mut last_metrics_update = Instant::now();
        // Track server start time *relative to when monitor detects Running/Starting*
        let server_start_time: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

        loop {
            // --- Wait for next cycle ---
            thread::sleep(MONITOR_INTERVAL);

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
                    debug!("Monitor: Searching for server process PID...");
                    sys.refresh_processes(); // Refresh process list before searching
                    server_pid = find_server_pid(&sys, &state);
                    if let Some(pid) = server_pid {
                        info!("Monitor: Found server process PID: {:?}", pid);
                        // Record start time when PID is first found while Running/Starting
                        let mut start_time_guard = server_start_time.lock().unwrap();
                        if start_time_guard.is_none() {
                            *start_time_guard = Some(Instant::now());
                            info!("Monitor: Server start time recorded.");
                        }
                    } else {
                        // This can happen briefly during startup before process is fully listed
                        trace!("Monitor: Server status is {:?}, but process PID not found yet.", status);
                    }
                } else {
                    // We have a PID, make sure it still exists (refresh_process does this)
                    if !sys.refresh_process(server_pid.unwrap()) {
                        error!("Monitor: Server process with PID {:?} disappeared unexpectedly!", server_pid.unwrap());
                        server_pid = None; // Clear PID
                        let mut start_time_guard = server_start_time.lock().unwrap();
                        *start_time_guard = None; // Clear start time

                        // Update status if it wasn't already Stopping/Stopped
                        if let Ok(current_status @ (ServerStatus::Running | ServerStatus::Starting)) = state.get_status() {
                            warn!("Monitor: Updating server status to Stopped due to process disappearance.");
                            state.reset_player_count(); // Reset count on crash
                            if state.set_status(ServerStatus::Stopped).is_ok() {
                                events::emit_status_change(ServerStatus::Stopped);
                                events::emit_warn("Server process stopped unexpectedly (disappeared).".to_string(), "Monitor".to_string());
                            } else {
                                error!("Monitor: Failed to lock state to set status to Stopped after process disappearance.");
                            }
                            // Clear handle in AppState just in case
                            let _ = state.set_process_handle(None);
                        }
                        continue; // Skip metric collection for this cycle
                    }
                }
            } else {
                // If stopped, stopping or error, clear the PID and start time
                if server_pid.is_some() {
                    info!("Monitor: Server not running/starting. Clearing PID and start time.");
                    server_pid = None;
                    let mut start_time_guard = server_start_time.lock().unwrap();
                    *start_time_guard = None;
                    // Ensure metrics are reset or show zero when stopped
                    match state.metrics.lock() {
                        Ok(mut metrics_guard) => {
                            *metrics_guard = MetricsData::default(); // Reset to defaults
                            trace!("Monitor: Reset AppState metrics as server is stopped.");
                        },
                        Err(e) => error!("Monitor: Failed to lock metrics for reset: {}", e),
                    }

                }
                // Continue loop to wait for state change
                continue;
            }

            // --- Collect Metrics if PID is known ---
            if let Some(pid) = server_pid {
                // We already refreshed the process above, just need system memory occasionally
                sys.refresh_memory(); // Refresh system memory info

                if let Some(process) = sys.process(pid) {
                    // --- Create MetricsData ---
                    let current_time_secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or(Duration::ZERO)
                        .as_secs();

                    let uptime_secs = server_start_time
                        .lock()
                        .unwrap()
                        .map_or(0, |start| start.elapsed().as_secs());

                    // Get current player count from AppState.metrics
                    // Get max players from cached AppState.server_properties
                    let (player_count, current_max_players_metric) = {
                        match state.metrics.lock() {
                            Ok(guard) => (guard.player_count, guard.max_players),
                            Err(e) => {
                                error!("Monitor: Failed to lock metrics to read player/max count: {}", e);
                                (0, 0) // Fallback values
                            }
                        }
                    };

                    // Read max_players from properties cache for comparison/update
                    let max_players_prop = state
                        .get_server_properties() // Use helper
                        .ok() // Ignore lock errors for this non-critical read? Or log?
                        .and_then(|props| props.get("max-players").and_then(|s| s.parse::<u32>().ok()))
                        .unwrap_or(0); // Default to 0 if not found/parsable

                    // If max_players in metrics differs from properties cache, update metrics
                    if current_max_players_metric != max_players_prop {
                        match state.metrics.lock() {
                            Ok(mut guard) => {
                                trace!("Monitor: Updating max_players in metrics from {} to {}", guard.max_players, max_players_prop);
                                guard.max_players = max_players_prop;
                            }
                            Err(e) => error!("Monitor: Failed to update max_players in metrics: {}", e),
                        }
                    }

                    // TODO: Get TPS accurately
                    let tps = None; // Placeholder

                    let metrics = MetricsData {
                        timestamp: current_time_secs,
                        // sysinfo cpu_usage() needs careful interpretation.
                        // It's often % since process start or last refresh cycle.
                        // For more accurate *current* load, consider system-wide load
                        // or calculating diffs between successive process CPU times.
                        cpu_usage: process.cpu_usage(), // Use with caution, might not be interval load %
                        memory_usage: process.memory(), // Bytes
                        system_memory_total: sys.total_memory(), // Bytes
                        player_count, // Read from metrics lock
                        max_players: max_players_prop, // Use value read from properties
                        tps,
                        uptime: uptime_secs,
                    };
                    trace!("Collected Metrics: {:?}", metrics);

                    // --- Update Shared State (Metrics) ---
                    // No need to call state.update_metrics if we modified it directly above?
                    // Re-evaluate: It's safer to update the *whole* metrics struct at once
                    // after collecting all data to maintain consistency.
                    // Let's revert to calling update_metrics.
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
                        trace!("Monitor: Emitting MetricsUpdated event.");
                        emit_event(events::Event::MetricsUpdated(metrics.clone()));
                        last_metrics_update = Instant::now();
                    }
                }
                // else case (process disappeared) handled by refresh_process check earlier
            } // end if let Some(pid)
        } // end loop
    }); // end thread::spawn
}

/// Helper to find the PID of the Java server process.
/// Looks for a "java" process whose command line arguments include the server JAR name.
fn find_server_pid(sys: &System, state: &Arc<AppState>) -> Option<Pid> {
    let server_jar_name = &state.server_jar; // Get the JAR name from state
    trace!("Searching for java process matching JAR: {}", server_jar_name);

    let process_name = if cfg!(target_os = "windows") { "java.exe" } else { "java" };

    // Iterate through processes with the exact name
    for (pid, process) in sys.processes_by_exact_name(process_name) {
        // Check command line arguments
        let cmd_line = process.cmd().join(" ");
        // Simple check: does command line contain the JAR name?
        // And does it run in the expected directory? (More robust)
        if cmd_line.contains(server_jar_name) {
            if let Some(cwd) = process.cwd() {
                if cwd == state.server_directory {
                    debug!("Found matching PID by name, JAR, and CWD: {}, Cmd: '{}'", pid, cmd_line);
                    return Some(*pid);
                } else {
                    trace!("PID {} matches name/JAR but not CWD ({} != {})", pid, cwd.display(), state.server_directory.display());
                }
            } else {
                // Fallback if CWD isn't available, just match name/JAR
                debug!("Found potential match PID by name/JAR (CWD unknown): {}, Cmd: '{}'", pid, cmd_line);
                return Some(*pid);
            }
        }
    }

    trace!("No matching Java process found by exact name.");
    None // No matching process found
}