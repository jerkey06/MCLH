// --- Assume AppState is defined like this (in app_state.rs or similar) ---
// use std::sync::{Arc, Mutex};
// use std::process::Child;
// use crate::models::server_status::ServerStatus;
//
// pub struct AppState {
//     pub server_status: Mutex<ServerStatus>,
//     // Stores the handle to the Minecraft server process when it's active
//     pub process_handle: Mutex<Option<Child>>,
//     pub java_path: String,
//     pub server_directory: String,
//     pub server_jar: String,
//     // JVM arguments (e.g., -Xmx4G -Xms1G)
//     pub server_args: Vec<String>,
//     // You could add the stop timeout here if you want it configurable
//     // pub stop_timeout_secs: u64,
// }
// --- End of assumed AppState definition ---


use std::sync::{Arc, Mutex};
use std::process::{Command, Child, Stdio};
use std::path::Path;
use std::io::{BufReader, BufRead, Write}; // Required for stdin.write_all/writeln
use wait_timeout::ChildExt;
use std::thread;
use std::time::Duration;

// Dependencies from your crate (ensure paths are correct)
use crate::app_state::AppState;
use crate::error::{AppError, Result}; // Your custom Result and AppError types
use crate::models::server_status::ServerStatus;
use crate::models::log_entry::{LogEntry, LogLevel}; // For structured logs
use crate::api::events; // For sending events (logs, status changes)

/// Starts the Minecraft server process.
///
/// Changes state to Starting, spawns the Java process, stores the handle,
/// and creates threads to monitor stdout/stderr.
pub fn start_server(state: Arc<AppState>) -> Result<()> {
    // Lock the status to check and update
    let mut status_guard = state.server_status.lock()
        .expect("BUG: Server status mutex poisoned. Cannot recover.");

    // Prevent starting if it's already in a state other than Stopped
    if *status_guard != ServerStatus::Stopped {
        return Err(AppError::ServerError(format!("Server is not stopped (current state: {:?})", *status_guard)));
    }

    // Update state to Starting
    *status_guard = ServerStatus::Starting;
    // Send status change event (ignore send errors for simplicity here)
    if let Some(sender) = events::get_event_sender() {
        let _ = sender.send(events::Event::StatusChanged(ServerStatus::Starting));
    }
    // Release the status lock as soon as possible
    drop(status_guard);

    // Build the full path to the server JAR
    let server_path = Path::new(&state.server_directory).join(&state.server_jar);
    if !server_path.exists() {
        // If the JAR doesn't exist, revert state to Stopped and return an error
        *state.server_status.lock().expect("Mutex poisoned") = ServerStatus::Stopped;
        // Send status change event
        if let Some(sender) = events::get_event_sender() {
            let _ = sender.send(events::Event::StatusChanged(ServerStatus::Stopped));
        }
        return Err(AppError::ServerJarNotFound(server_path));
    }

    // Prepare arguments for the Java command
    // Includes JVM args from AppState, then "-jar <server.jar>"
    let mut args = state.server_args.clone();
    args.push("-jar".to_string()); // Standard argument to execute a JAR
    args.push(state.server_jar.clone());
    // Some Minecraft servers might need "nogui" at the end
    // args.push("nogui".to_string()); // Uncomment if needed

    // Configure the command to launch the Java process
    let mut command = Command::new(&state.java_path);
    command.args(&args)
        .current_dir(&state.server_directory) // Execute from the server directory
        .stdout(Stdio::piped()) // Capture standard output
        .stderr(Stdio::piped()) // Capture standard error
        .stdin(Stdio::piped()); // Allow sending commands (standard input)

    // Try to spawn the process
    let mut process = match command.spawn() {
        Ok(p) => p,
        Err(e) => {
            // If spawn fails, revert state and return error
            *state.server_status.lock().expect("Mutex poisoned") = ServerStatus::Stopped;
            if let Some(sender) = events::get_event_sender() {
                let _ = sender.send(events::Event::StatusChanged(ServerStatus::Stopped));
            }
            eprintln!("Error spawning the server process: {}", e);
            return Err(AppError::IoError(e)); // Wrap the IO error
        }
    };

    let process_id = process.id();
    println!("Minecraft server started with PID: {}", process_id); // Basic log

    // Get stdout and stderr handles BEFORE moving `process` into AppState
    // Use take() to take ownership; failure here is a critical error.
    let stdout = process.stdout.take()
        .ok_or_else(|| AppError::ServerError("Could not capture server stdout.".to_string()))?;
    let stderr = process.stderr.take()
        .ok_or_else(|| AppError::ServerError("Could not capture server stderr.".to_string()))?;

    // Store the process handle in the shared state
    { // Short scope to hold the lock for minimal time
        let mut process_handle_guard = state.process_handle.lock()
            .expect("BUG: Process handle mutex poisoned.");
        *process_handle_guard = Some(process); // `process` is moved here
    }

    // --- Thread to read STDOUT ---
    let state_stdout = state.clone(); // Clone Arc for the thread
    let sender_stdout = events::get_event_sender(); // Get sender for the thread
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line_result in reader.lines() {
            match line_result {
                Ok(line) => {
                    // Send each line as a Log event
                    let log_entry = LogEntry::info(line.clone(), "server_stdout".to_string());
                    if let Some(ref sender) = sender_stdout {
                        // Ignore if the channel is closed (receiver might have terminated)
                        let _ = sender.send(events::Event::Log(log_entry));
                    }

                    // Detect the typical Minecraft message when loading finishes
                    // Adjust "Done" if your version/mod shows a different message (e.g., "loaded")
                    if line.contains("Done") || line.contains("loaded") { // Server startup message check
                        let mut status = state_stdout.server_status.lock().expect("Mutex poisoned");
                        // Only change to Running if we were in Starting state
                        if *status == ServerStatus::Starting {
                            *status = ServerStatus::Running;
                            println!("Server detected as Running."); // Basic log
                            if let Some(ref sender) = sender_stdout {
                                let _ = sender.send(events::Event::StatusChanged(ServerStatus::Running));
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error reading server stdout: {}", e);
                    break; // Exit loop on read error
                }
            }
        }

        // If the loop ends (EOF or error), the process likely terminated.
        println!("Stdout thread finished. Server process likely terminated.");
        let mut status = state_stdout.server_status.lock().expect("Mutex poisoned");
        // If not already Stopping or Stopped, mark as Stopped.
        if *status != ServerStatus::Stopping && *status != ServerStatus::Stopped {
            *status = ServerStatus::Stopped;
            println!("Status changed to Stopped (detected by stdout EOF)."); // Log
            if let Some(sender) = sender_stdout {
                let _ = sender.send(events::Event::StatusChanged(ServerStatus::Stopped));
            }
        }
    });

    // --- Thread to read STDERR ---
    // Doesn't need to clone `state` if only sending logs
    let sender_stderr = events::get_event_sender();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line_result in reader.lines() {
            match line_result {
                Ok(line) => {
                    // Send error lines as Error Log events
                    let log_entry = LogEntry::error(line, "server_stderr".to_string());
                    if let Some(ref sender) = sender_stderr {
                        let _ = sender.send(events::Event::Log(log_entry));
                    }
                }
                Err(e) => {
                    eprintln!("Error reading server stderr: {}", e);
                    break;
                }
            }
        }
        println!("Stderr thread finished."); // Log
        // Status is not managed here; stdout thread or stop_server handles it.
    });

    Ok(())
}

/// Sends the "stop" command to the server and waits for it to terminate.
///
/// Changes state to Stopping, attempts graceful shutdown via stdin,
/// waits with a timeout, and forces termination (kill) if necessary.
pub fn stop_server(state: Arc<AppState>) -> Result<()> {
    // Lock status to check and update
    let mut status_guard = state.server_status.lock().expect("Mutex poisoned");

    // If already stopped or stopping, do nothing.
    if *status_guard == ServerStatus::Stopped || *status_guard == ServerStatus::Stopping {
        println!("Stop request ignored (current state: {:?})", *status_guard);
        return Ok(());
    }

    // Update state to Stopping
    *status_guard = ServerStatus::Stopping;
    if let Some(sender) = events::get_event_sender() {
        let _ = sender.send(events::Event::StatusChanged(ServerStatus::Stopping));
    }
    println!("Initiating server stop sequence..."); // Log
    drop(status_guard); // Release status lock

    // Get the process handle from shared state
    let process_to_stop: Option<Child>;
    { // Short scope for the handle lock
        let mut handle_guard = state.process_handle.lock().expect("Mutex poisoned");
        // take() extracts the value from Option<Child>, leaving None behind.
        // This transfers ownership of the Child to `process_to_stop`.
        process_to_stop = handle_guard.take();
    }

    // If we found an active process...
    if let Some(mut process) = process_to_stop { // `mut` is crucial here for calling kill/wait
        let pid = process.id();
        println!("Attempting graceful shutdown of process {}...", pid); // Log

        // Try sending the "stop" command via stdin.
        // This might fail if the process is already closing stdin, which is acceptable.
        match execute_command(state.clone(), "stop".to_string()) {
            Ok(_) => println!("'stop' command sent successfully to process {}.", pid),
            Err(e) => eprintln!("Could not send 'stop' command to process {} (may be normal if closing): {:?}", pid, e),
        }

        // Dedicated thread to wait/kill the process without blocking the stop_server call
        let state_stop = state.clone();
        let sender_stop = events::get_event_sender();
        thread::spawn(move || {
            // Timeout for graceful shutdown (could be configurable)
            // TODO: Make timeout configurable via AppState
            let timeout = Duration::from_secs(30);
            println!("Waiting up to {:?} for process {} to terminate...", timeout, pid);

            // Wait with timeout. wait_timeout requires &mut self.
            match process.wait_timeout(timeout) {
                Ok(Some(status)) => {
                    // Process terminated on its own before timeout
                    println!("Process {} terminated gracefully with status: {}", pid, status);
                }
                Ok(None) => {
                    // Timeout reached, process still alive. Force termination.
                    eprintln!("Timeout waiting for process {}. Forcing termination (kill)...", pid);
                    // kill() requires &mut self.
                    if let Err(e) = process.kill() {
                        eprintln!("Error forcing termination (kill) of process {}: {}", pid, e);
                        // Still proceed to mark as Stopped.
                    } else {
                        println!("Process {} forced to terminate.", pid);
                        // Waiting a bit after kill might be useful for OS cleanup (optional)
                        match process.wait() {
                            Ok(status) => println!("Final status of process {} after kill: {}", pid, status),
                            Err(e) => eprintln!("Error waiting for process {} after kill: {}", pid, e),
                        }
                    }
                }
                Err(e) => {
                    // Unexpected error during wait_timeout
                    eprintln!("Unexpected error waiting for process {}: {}", pid, e);
                    // Try killing as a last resort
                    if let Err(kill_e) = process.kill() {
                        eprintln!("Error trying to kill process {} after wait error: {}", pid, kill_e);
                    }
                }
            }

            // Regardless of how it ended, update the final state to Stopped
            println!("Marking server as Stopped (from stop thread).");
            let mut status_guard = state_stop.server_status.lock().expect("Mutex poisoned");
            *status_guard = ServerStatus::Stopped;
            if let Some(sender) = sender_stop {
                let _ = sender.send(events::Event::StatusChanged(ServerStatus::Stopped));
            }
            // The process handle was already taken, no need to clear it here.
        });

    } else {
        // No process handle was stored.
        // This might happen if start_server failed, or if the process already terminated
        // and the stdout thread marked it as Stopped.
        println!("Warning: No active process handle found to stop. Ensuring state is Stopped.");
        // Ensure the status is Stopped just in case
        let mut status_guard = state.server_status.lock().expect("Mutex poisoned");
        if *status_guard != ServerStatus::Stopped {
            *status_guard = ServerStatus::Stopped;
            if let Some(sender) = events::get_event_sender() {
                let _ = sender.send(events::Event::StatusChanged(ServerStatus::Stopped));
            }
        }
    }

    Ok(())
}

/// Sends a console command to the Minecraft server's stdin.
///
/// Requires the server to be in the Running state.
pub fn execute_command(state: Arc<AppState>, command: String) -> Result<()> {
    // Verify that the server is running
    // Clone to release the lock quickly if not Running
    let status = state.server_status.lock().expect("Mutex poisoned").clone();
    if status != ServerStatus::Running {
        return Err(AppError::ServerError(format!("Server is not running (state: {:?}). Cannot send command.", status)));
    }

    // Access the process handle to get stdin
    let mut handle_guard = state.process_handle.lock().expect("Mutex poisoned");

    // Use `as_mut()` to get a mutable reference to `Child` inside the `Option`
    if let Some(process) = handle_guard.as_mut() {
        // Access stdin, which also needs to be mutable
        if let Some(stdin) = process.stdin.as_mut() {
            // Write the command followed by a newline (required for Minecraft console)
            if let Err(e) = writeln!(stdin, "{}", command) {
                eprintln!("Error writing command '{}' to stdin: {}", command, e);
                Err(AppError::IoError(e)) // Return error
            } else {
                // Flush to ensure the command is sent immediately
                if let Err(e) = stdin.flush() {
                    eprintln!("Error flushing stdin after command '{}': {}", command, e);
                    Err(AppError::IoError(e))
                } else {
                    println!("Command '{}' sent to server.", command); // Log
                    Ok(()) // Success
                }
            }
        } else {
            // This would be unusual if Stdio::piped() worked initially
            Err(AppError::ServerError("Stdin is not available for the server process.".to_string()))
        }
    } else {
        // Handle is None, server is not (or no longer) active
        Err(AppError::ServerError("No active server process found to send command.".to_string()))
    }
    // Lock on handle_guard is released here automatically
}