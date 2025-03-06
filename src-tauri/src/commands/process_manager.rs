use std::sync::{Arc, Mutex};
use std::process::{Command, Child, Stdio};
use std::path::Path;
use std::io::BufReader;
use std::io::BufRead;
use std::thread;

use crate::app_state::AppState;
use crate::error::{AppError, Result};
use crate::models::server_status::ServerStatus;
use crate::models::log_entry::{LogEntry, LogLevel};
use crate::api::events;

static PROCESS: Mutex<Option<Child>> = Mutex::new(None);

pub fn start_server(state: Arc<AppState>) -> Result<()> {
    let mut status_guard = state.server_status.lock().unwrap();

    if *status_guard != ServerStatus::Stopped {
        return Err(AppError::ServerError("Server is already running".to_string()));
    }

    *status_guard = ServerStatus::Starting;
    drop(status_guard);

    let java_path = &state.java_path;
    let server_dir = &state.server_directory;
    let server_jar = &state.server_jar;
    let server_args = &state.server_args;

    let server_path = Path::new(server_dir).join(server_jar);
    if !server_path.exists() {
        return Err(AppError::ServerJarNotFound(server_path));
    }

    let mut args = server_args.clone();
    args.push(server_jar.clone());

    let process = Command::new(java_path)
        .args(&args)
        .current_dir(server_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()?;

    let process_id = process.id();
    let stdout = process.stdout.unwrap();
    let stderr = process.stderr.unwrap();

    let state_clone = state.clone();
    {
        let mut process_guard = PROCESS.lock().unwrap();
        *process_guard = Some(process);
    }

    let event_sender = events::get_event_sender();

    thread::spawn(move || {
        let stdout_reader = BufReader::new(stdout);
        for line in stdout_reader.lines() {
            if let Ok(line) = line {
                let log_entry = LogEntry::info(line.clone(), "server".to_string());
                if let Some(sender) = &event_sender {
                    let _ = sender.send(events::Event::Log(log_entry));
                }

                if line.contains("Done") {
                    let mut status = state_clone.server_status.lock().unwrap();
                    *status = ServerStatus::Running;
                    if let Some(sender) = &event_sender {
                        let _ = sender.send(events::Event::StatusChanged(ServerStatus::Running));
                    }
                }
            }
        }

        let mut status = state_clone.server_status.lock().unwrap();
        *status = ServerStatus::Stopped;
        if let Some(sender) = &event_sender {
            let _ = sender.send(events::Event::StatusChanged(ServerStatus::Stopped));
        }
    });

    thread::spawn(move || {
        let stderr_reader = BufReader::new(stderr);
        for line in stderr_reader.lines() {
            if let Ok(line) = line {
                let log_entry = LogEntry::error(line, "server".to_string());
                if let Some(sender) = &event_sender {
                    let _ = sender.send(events::Event::Log(log_entry));
                }
            }
        }
    });

    Ok(())
}

pub fn stop_server(state: Arc<AppState>) -> Result<()> {
    let mut status_guard = state.server_status.lock().unwrap();

    if *status_guard == ServerStatus::Stopped {
        return Ok(());
    }

    *status_guard = ServerStatus::Stopping;
    drop(status_guard);

    let mut process_guard = PROCESS.lock().unwrap();
    if let Some(process) = process_guard.take() {
        execute_command(state.clone(), "stop".to_string())?;

        thread::spawn(move || {
            let timeout = std::time::Duration::from_secs(30);
            let _ = process.wait_timeout(timeout).unwrap();

            if let Ok(Some(_)) = process.try_wait() {
                // Server stopped gracefully
            } else {
                // Force kill if timed out
                let _ = process.kill();
            }
        });
    }

    Ok(())
}

pub fn execute_command(state: Arc<AppState>, command: String) -> Result<()> {
    let status = state.server_status.lock().unwrap().clone();

    if status != ServerStatus::Running {
        return Err(AppError::ServerError("Server is not running".to_string()));
    }

    let mut process_guard = PROCESS.lock().unwrap();
    if let Some(ref mut process) = *process_guard {
        if let Some(ref mut stdin) = process.stdin {
            use std::io::Write;
            writeln!(stdin, "{}", command)?;
            return Ok(());
        }
    }

    Err(AppError::ServerError("Failed to send command to server".to_string()))
}