use crate::error::AppError; // Use AppError directly
use crate::models::log_entry::LogEntry;
use crate::models::metrics::MetricsData;
use crate::models::server_status::ServerStatus;
use log::{debug, warn}; // Use the log crate
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{channel, SendError, Sender}; // Use standard MPSC
use std::sync::RwLock;

/// The name of the event emitted to the Tauri frontend.
pub const TAURI_BACKEND_EVENT: &str = "backend-event";

/// Type alias for the sender part of the internal event channel.
/// We send `Event` directly, errors should be wrapped in `Event::Error` variant.
pub type EventSender = Sender<Event>;

/// Type alias for the receiver part of the internal event channel.
pub type EventReceiver = std::sync::mpsc::Receiver<Event>;

/// Global static storage for the event sender. Uses RwLock for safe access.
static EVENT_SENDER: Lazy<RwLock<Option<EventSender>>> = Lazy::new(|| RwLock::new(None));

/// Defines the different types of events that can occur within the backend.
/// These events are sent to the internal MPSC channel and then bridged to Tauri.
/// `Serialize` is crucial for sending to the frontend via Tauri.
/// `tag = "type", content = "payload"` makes the JSON structure predictable for JS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Event {
    /// Server status has changed (e.g., Starting, Running, Stopped).
    StatusChanged(ServerStatus),
    /// A log message was generated.
    Log(LogEntry),
    /// An alert condition was met (could be a specific LogEntry or custom struct).
    Alert(String), // Simple string alert for now
    /// Performance metrics were updated.
    MetricsUpdated(MetricsData),
    /// A player joined the Minecraft server. Contains player name.
    PlayerJoined(String),
    /// A player left the Minecraft server. Contains player name.
    PlayerLeft(String),
    /// Server process is initiating startup sequence.
    ServerStarting,
    /// Server process has successfully started and is likely accepting connections.
    ServerStarted,
    /// Server process is initiating shutdown sequence.
    ServerStopping,
    /// Server process has stopped cleanly.
    ServerStopped,
    /// A command was sent to the server process. Includes success status and output if available.
    CommandExecuted {
        command: String,
        success: bool,
        output: Option<String>,
    },
    /// Backup process has started.
    BackupStarted,
    /// Backup process completed. Contains Result to indicate success or failure message.
    BackupCompleted(Result<(), String>),
    /// General application error occurred that the frontend should be aware of.
    Error(String),
    /// Notifies the frontend about the current EULA acceptance status.
    EulaStatus(bool),
    /// Indicates progress during a long operation like modpack install.
    ProgressUpdate { task: String, progress: f32, message: String },
    // Add more specific event types as your application evolves
}

/// Sets the global event sender. Should only be called once during application setup.
pub fn set_event_sender(sender: EventSender) {
    let mut writer = EVENT_SENDER
        .write()
        .expect("Failed to lock EVENT_SENDER for writing");
    if writer.is_some() {
        warn!("Attempted to set event sender after it was already set.");
        return;
    }
    *writer = Some(sender);
    debug!("Global event sender set successfully.");
}

/// Retrieves a clone of the global event sender. Returns None if not set yet.
fn get_event_sender() -> Option<EventSender> {
    EVENT_SENDER
        .read()
        .expect("Failed to lock EVENT_SENDER for reading")
        .clone()
}

/// Emits an event onto the internal MPSC channel.
/// Logs a warning if the sender hasn't been set or if sending fails (receiver disconnected).
pub fn emit_event(event: Event) {
    if let Some(sender) = get_event_sender() {
        debug!("Emitting event: {:?}", event); // Log event emission (use trace for production)
        if let Err(SendError(failed_event)) = sender.send(event) {
            // This usually means the receiver (event bridge thread) has terminated.
            warn!(
                "Failed to send internal event (receiver disconnected): {:?}",
                failed_event
            );
        }
    } else {
        warn!(
            "Attempted to emit event, but event sender is not set: {:?}",
            event
        );
    }
}

// --- Convenience functions for emitting specific events ---

/// Emits a log event.
pub fn emit_log(level: log::Level, message: String, source: String) {
    let log_entry = LogEntry::new(level.to_string(), message, source);
    emit_event(Event::Log(log_entry));
}

/// Emits an info-level log event.
pub fn emit_info(message: String, source: String) {
    emit_log(log::Level::Info, message, source);
}

/// Emits a warning-level log event.
pub fn emit_warn(message: String, source: String) {
    emit_log(log::Level::Warn, message, source);
}

/// Emits an error-level log event AND a general Error event.
pub fn emit_error(message: String, source: String) {
    let full_message = format!("[{}] {}", source, message);
    emit_log(log::Level::Error, message, source);
    // Also emit a general error event for frontend notifications
    emit_event(Event::Error(full_message));
}

/// Emits a server status change event.
pub fn emit_status_change(status: ServerStatus) {
    emit_event(Event::StatusChanged(status));
}

/// Emits a metrics update event.
pub fn emit_metrics_update(metrics: MetricsData) {
    emit_event(Event::MetricsUpdated(metrics));
}

/// Emits a player joined event and an associated info log.
pub fn emit_player_joined(player_name: String) {
    emit_event(Event::PlayerJoined(player_name.clone()));
    emit_info(format!("Player joined: {}", player_name), "Server".to_string());
}

/// Emits a player left event and an associated info log.
pub fn emit_player_left(player_name: String) {
    emit_event(Event::PlayerLeft(player_name.clone()));
    emit_info(format!("Player left: {}", player_name), "Server".to_string());
}

/// Emits an event indicating the EULA status.
pub fn emit_eula_status(accepted: bool) {
    emit_event(Event::EulaStatus(accepted));
}

/// Emits a general application error event based on AppError.
pub fn emit_app_error(error: &AppError) {
    log::error!("Application Error: {}", error); // Log the error regardless
    emit_event(Event::Error(error.to_string()));
}

/// Emits a general application error event from a string message.
pub fn emit_error_str(message: &str) {
    log::error!("Application Error: {}", message);
    emit_event(Event::Error(message.to_string()));
}

/// Emits a progress update event.
pub fn emit_progress(task: &str, progress: f32, message: &str) {
    emit_event(Event::ProgressUpdate {
        task: task.to_string(),
        progress,
        message: message.to_string(),
    });
}

// --- Function to create the channel ---

/// Creates a new MPSC channel for internal events.
pub fn create_event_channel() -> (EventSender, EventReceiver) {
    channel::<Event>()
}