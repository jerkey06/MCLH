use crate::api::events::{self, emit_app_error, emit_event, emit_info, emit_status_change, emit_warn, Event};
use crate::app_state::AppState;
use crate::error::{AppError, Result};
use crate::models::log_entry::LogEntry;
use crate::models::server_status::ServerStatus;
use log::{debug, error, info, warn}; // Use log crate
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use wait_timeout::ChildExt;

const STDOUT_SOURCE: &str = "Server";
const STDERR_SOURCE: &str = "Server"; // Or distinguish if needed, e.g., "Server Error"

/// Starts the Minecraft server process.
///
/// - Checks current state.
/// - Sets state to `Starting` and emits event.
/// - Validates server JAR path.
/// - Spawns the Java process with configured arguments and captures stdio.
/// - Stores the `Child` handle in `AppState`.
/// - Spawns threads to monitor stdout and stderr, emitting logs and detecting `Running` state.
pub fn start_server(state: Arc<AppState>) -> Result<()> {
    info!("Attempting to start the server...");

    // --- State Check and Update ---
    { // Scope for status lock
        let mut status_guard = state.server_status.lock()
            .map_err(|e| AppError::LockError(format!("Failed to lock server_status: {}", e)))?;

        if *status_guard != ServerStatus::Stopped {
            warn!(
                "Start command ignored. Server is not stopped (current state: {:?})",
                *status_guard
            );
            return Err(AppError::ServerError(format!(
                "Server is not stopped (current state: {:?})",
                *status_guard
            )));
        }
        *status_guard = ServerStatus::Starting;
        emit_status_change(ServerStatus::Starting); // Emit event
        info!("Server status set to Starting.");
    } // Status lock released

    // --- Path and Config Validation ---
    let server_jar_path = state.get_server_jar_path();
    if !server_jar_path.exists() {
        error!("Server JAR file not found at: {:?}", server_jar_path);
        // Revert state on failure
        state.set_status(ServerStatus::Stopped)?;
        emit_status_change(ServerStatus::Stopped);
        return Err(AppError::ServerJarNotFound(server_jar_path));
    }

    let java_args = state.get_server_args()?; // Read args using lock helper
    let mut final_args = java_args.clone(); // Start with configured JVM args
    // "-jar" should already be in default_args, but check just in case
    if !final_args.contains(&"-jar".to_string()){
        final_args.push("-jar".to_string());
    }
    final_args.push(state.server_jar.clone()); // Add the specific jar name
    // Add nogui if needed for server type (often prevents separate GUI window)
    final_args.push("nogui".to_string());
    debug!("Java arguments: {:?}", final_args);

    // --- Process Spawning ---
    let mut command = Command::new(&state.java_path);
    command
        .args(&final_args)
        .current_dir(&state.server_directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped());

    info!("Spawning Java process: {:?} with args {:?}", state.java_path, final_args);
    let mut process: Child = match command.spawn() {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to spawn server process: {}", e);
            state.set_status(ServerStatus::Stopped)?; // Revert state
            emit_status_change(ServerStatus::Stopped);
            emit_app_error(&AppError::IoError(e)); // Emit error event
            return Err(AppError::IoError(e));
        }
    };

    let process_id = process.id();
    info!("Server process spawned successfully with PID: {}", process_id);

    // --- Capture StdIO Handles ---
    // Must be done *before* moving the process handle into AppState
    let stdout = process
        .stdout
        .take()
        .ok_or_else(|| AppError::ServerError("Could not capture server stdout.".to_string()))?;
    let stderr = process
        .stderr
        .take()
        .ok_or_else(|| AppError::ServerError("Could not capture server stderr.".to_string()))?;

    // --- Store Process Handle ---
    state.set_process_handle(Some(process))?; // Use helper to store `process` (moved)
    debug!("Process handle stored in AppState.");

    // --- Stdout Monitoring Thread ---
    let state_stdout = state.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut detected_running = false;
        info!("Stdout monitoring thread started for PID {}", process_id);

        for line_result in reader.lines() {
            match line_result {
                Ok(line) => {
                    // Emit line as info log
                    emit_info(line.clone(), STDOUT_SOURCE.to_string());

                    // Check for server startup completion message (adjust keywords as needed)
                    if !detected_running && (line.contains("Done") || line.contains("server marked as active")) {
                        debug!("Detected server startup message: '{}'", line);
                        match state_stdout.get_status() {
                            Ok(ServerStatus::Starting) => {
                                if state_stdout.set_status(ServerStatus::Running).is_ok() {
                                    emit_status_change(ServerStatus::Running);
                                    info!("Server status updated to Running.");
                                    detected_running = true;
                                } else {
                                    error!("Failed to lock state for updating status to Running.");
                                }
                            },
                            Ok(current_status) => {
                                // Avoid changing status if it was already changed (e.g., by stop command)
                                debug!("Startup message detected, but status is already {:?}. Ignoring.", current_status);
                            },
                            Err(e) => error!("Failed to get status for startup check: {}", e),
                        }
                    }
                }
                Err(e) => {
                    error!("Error reading server stdout: {}", e);
                    emit_error(format!("Error reading server stdout: {}", e), "ProcessManager".to_string());
                    break; // Exit loop on read error
                }
            }
        }

        info!("Stdout monitoring thread finished for PID {}. (EOF or error)", process_id);
        // Handle unexpected termination (crash detection)
        match state_stdout.get_status() {
            Ok(ServerStatus::Running) | Ok(ServerStatus::Starting) => {
                warn!("Server process terminated unexpectedly (stdout closed while Running or Starting).");
                if state_stdout.set_status(ServerStatus::Stopped).is_ok() {
                    emit_status_change(ServerStatus::Stopped);
                    emit_warn("Server process stopped unexpectedly.".to_string(), "ProcessManager".to_string());
                } else {
                    error!("Failed to lock state to set status to Stopped after crash.");
                }
                // Clear the handle just in case stop_server didn't run
                let _ = state_stdout.set_process_handle(None);
            },
            Ok(_) | Err(_) => {
                // Status is Stopped, Stopping, or error getting status - likely intended shutdown or already handled.
                debug!("Stdout EOF, but server status indicates shutdown or stopped state. No action needed.");
            }
        }
    });

    // --- Stderr Monitoring Thread ---
    let state_stderr = state.clone(); // Clone state only if needed (e.g., for context in errors)
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        info!("Stderr monitoring thread started for PID {}", process_id);
        for line_result in reader.lines() {
            match line_result {
                Ok(line) => {
                    // Emit stderr lines as error logs
                    emit_error(line, STDERR_SOURCE.to_string());
                }
                Err(e) => {
                    error!("Error reading server stderr: {}", e);
                    emit_error(format!("Error reading server stderr: {}", e), "ProcessManager".to_string());
                    break;
                }
            }
        }
        info!("Stderr monitoring thread finished for PID {}", process_id);
    });

    Ok(())
}

/// Stops the Minecraft server process gracefully, with a timeout and force kill fallback.
///
/// - Checks current state.
/// - Sets state to `Stopping` and emits event.
/// - Takes the `Child` handle from `AppState`.
/// - Attempts to send the "stop" command via stdin.
/// - Spawns a thread to wait for process termination with a configured timeout.
/// - If timeout occurs, kills the process.
/// - Updates state to `Stopped` and emits event in the waiting thread.
pub fn stop_server(state: Arc<AppState>) -> Result<()> {
    info!("Attempting to stop the server...");

    // --- State Check and Update ---
    { // Scope for status lock
        let mut status_guard = state.server_status.lock()
            .map_err(|e| AppError::LockError(format!("Failed to lock server_status: {}", e)))?;

        match *status_guard {
            ServerStatus::Stopped => {
                info!("Stop command ignored. Server is already stopped.");
                return Ok(());
            }
            ServerStatus::Stopping => {
                info!("Stop command ignored. Server is already stopping.");
                return Ok(());
            }
            _ => { // Starting or Running
                *status_guard = ServerStatus::Stopping;
                emit_status_change(ServerStatus::Stopping);
                info!("Server status set to Stopping.");
            }
        }
    } // Status lock released

    // --- Retrieve Process Handle ---
    // `take_process_handle` removes the Child from AppState, giving us ownership.
    let process_to_stop = match state.take_process_handle() {
        Ok(Some(child)) => child,
        Ok(None) => {
            warn!("No active process handle found to stop. Server might have crashed or already stopped.");
            // Ensure state is correctly set to Stopped if it wasn't already
            if state.set_status(ServerStatus::Stopped).is_ok() {
                emit_status_change(ServerStatus::Stopped);
            }
            return Ok(()); // Nothing more to do if no handle
        }
        Err(e) => {
            error!("Failed to get process handle: {}", e);
            // Attempt to set state to stopped as a fallback
            if state.set_status(ServerStatus::Stopped).is_ok() {
                emit_status_change(ServerStatus::Stopped);
            }
            return Err(e); // Propagate the lock error
        }
    };

    let pid = process_to_stop.id();
    info!("Retrieved handle for process PID {}", pid);

    // --- Attempt Graceful Shutdown Command ---
    // We send "stop" directly here for efficiency, avoiding re-locking in execute_command
    let mut handle_guard = state.process_handle.lock() // Need temp lock again for stdin access
        .map_err(|e| AppError::LockError(format!("Failed to lock process_handle for stop command: {}", e)))?;

    // Temporary re-insertion for stdin access (will be taken again by wait thread or cleared)
    // This is slightly complex; alternative is passing stdin handle separately if possible.
    // For simplicity, let's just try sending command without re-inserting,
    // using the `process_to_stop` variable directly before moving it to the thread.
    // THIS WON'T WORK because `process_to_stop` doesn't have stdin after take().
    // Let's revert to calling `send_command_to_server` which handles locking internally.
    // Note: `send_command_to_server` checks if status is Running, which it isn't anymore (it's Stopping).
    // We need a way to send command *while* Stopping. Let's create a helper.

    match send_command_internal(&state, &mut handle_guard, "stop".to_string()) {
        Ok(_) => info!("'stop' command sent successfully to process {}.", pid),
        Err(e) => warn!("Could not send 'stop' command to process {} (may be normal if closing): {:?}", pid, e),
    }
    // Release the temporary lock for stdin access
    drop(handle_guard);

    // The `process_to_stop` variable now owns the Child handle.
    // We move it into the waiting thread.

    // --- Wait/Kill Thread ---
    let state_stop = state.clone();
    let stop_timeout = state_stop.get_stop_timeout(); // Get configured timeout
    thread::spawn(move || {
        let mut owned_process = process_to_stop; // Take ownership in the thread
        info!("Waiting up to {:?} for process {} to terminate...", stop_timeout, pid);

        match owned_process.wait_timeout(stop_timeout) {
            Ok(Some(status)) => {
                info!("Process {} terminated gracefully with status: {}", pid, status);
            }
            Ok(None) => {
                warn!("Timeout waiting for process {}. Forcing termination (kill)...", pid);
                if let Err(e) = owned_process.kill() {
                    error!("Error forcing termination (kill) of process {}: {}", pid, e);
                    emit_error(format!("Error killing process {}: {}", pid, e), "ProcessManager".to_string());
                } else {
                    info!("Process {} killed successfully.", pid);
                    // Optionally wait briefly after kill
                    match owned_process.wait() {
                        Ok(status) => info!("Final status of process {} after kill: {}", pid, status),
                        Err(e) => warn!("Error waiting for process {} after kill: {}", pid, e),
                    }
                }
            }
            Err(e) => {
                error!("Unexpected error waiting for process {}: {}", pid, e);
                emit_error(format!("Error waiting for process {}: {}", pid, e), "ProcessManager".to_string());
                // Try killing as a last resort
                if let Err(kill_e) = owned_process.kill() {
                    error!("Error trying to kill process {} after wait error: {}", pid, kill_e);
                }
            }
        }

        // --- Final State Update (in thread) ---
        info!("Marking server as Stopped (from stop thread for PID {}).", pid);
        if state_stop.set_status(ServerStatus::Stopped).is_ok() {
            emit_status_change(ServerStatus::Stopped);
        } else {
            error!("Failed to lock state to set status to Stopped in stop thread.");
        }
        // Ensure handle is None in AppState (it was already taken, but double-check)
        if let Err(e) = state_stop.set_process_handle(None) {
            error!("Error ensuring process handle is None after stop: {}", e);
        }
    });

    Ok(())
}


/// Restarts the server by stopping it and then starting it again.
pub fn restart_server(state: Arc<AppState>) -> Result<()> {
    info!("Restart command received. Stopping server first...");
    // Call stop_server. It handles state changes and runs async in a thread for waiting.
    stop_server(state.clone())?;

    // We need to wait until the server is *actually* stopped before starting again.
    // Polling the status is one way.
    let poll_interval = Duration::from_millis(500);
    let max_wait = Duration::from_secs(state.get_stop_timeout().as_secs() + 10); // Wait slightly longer than stop timeout
    let start_time = std::time::Instant::now();

    info!("Waiting for server to fully stop before restarting...");
    loop {
        let current_status = state.get_status()?;
        if current_status == ServerStatus::Stopped {
            info!("Server confirmed stopped. Proceeding with start.");
            break;
        }
        if start_time.elapsed() > max_wait {
            error!("Timeout waiting for server to stop during restart sequence.");
            return Err(AppError::ServerError("Server did not stop within expected time for restart.".to_string()));
        }
        thread::sleep(poll_interval);
    }

    // Now that we're sure it's stopped, start it again.
    info!("Restart sequence: Starting server...");
    start_server(state)
}


/// Sends a console command to the Minecraft server's stdin.
/// Requires the server to be running.
pub fn send_command_to_server(state: Arc<AppState>, command: String) -> Result<()> {
    debug!("Attempting to send command: '{}'", command);
    // Lock status first to check if running
    let status = state.get_status()?;
    if status != ServerStatus::Running {
        warn!("Command '{}' not sent. Server is not running (state: {:?}).", command, status);
        return Err(AppError::ServerError(format!(
            "Server is not running (state: {:?}). Cannot send command.",
            status
        )));
    }

    // Lock handle to access stdin
    let mut handle_guard = state.process_handle.lock()
        .map_err(|e| AppError::LockError(format!("Failed to lock process_handle for command: {}", e)))?;

    send_command_internal(&state, &mut handle_guard, command)
}


/// Internal helper to write a command to stdin.
/// Assumes the process handle mutex is already locked by the caller.
fn send_command_internal(
    state: &Arc<AppState>, // Borrow state for logging/events if needed
    handle_guard: &mut std::sync::MutexGuard<Option<Child>>, // Pass the lock guard
    command: String
) -> Result<()> {

    if let Some(process) = handle_guard.as_mut() {
        if let Some(stdin) = process.stdin.as_mut() {
            debug!("Writing command '{}' to stdin...", command);
            if let Err(e) = writeln!(stdin, "{}", command) {
                error!("Error writing command '{}' to stdin: {}", command, e);
                // Emit event?
                emit_event(Event::CommandExecuted { command: command.clone(), success: false, output: Some(e.to_string()) });
                return Err(AppError::IoError(e));
            }
            if let Err(e) = stdin.flush() {
                error!("Error flushing stdin after command '{}': {}", command, e);
                emit_event(Event::CommandExecuted { command: command.clone(), success: false, output: Some(e.to_string()) });
                return Err(AppError::IoError(e));
            }
            info!("Command '{}' sent successfully.", command);
            emit_event(Event::CommandExecuted { command, success: true, output: None });
            Ok(())
        } else {
            error!("Stdin is not available for the server process.");
            emit_event(Event::CommandExecuted { command, success: false, output: Some("Stdin not available".to_string()) });
            Err(AppError::ServerError("Stdin is not available for the server process.".to_string()))
        }
    } else {
        warn!("No active server process found to send command '{}'.", command);
        emit_event(Event::CommandExecuted { command, success: false, output: Some("Server process not active".to_string()) });
        Err(AppError::ServerError("No active server process found to send command.".to_string()))
    }
}