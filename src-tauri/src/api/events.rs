use std::sync::mpsc::Sender;
use std::sync::RwLock;
use once_cell::sync::Lazy;
use serde::{Serialize, Deserialize};

use crate::models::server_status::ServerStatus;
use crate::models::log_entry::LogEntry;
use crate::models::metrics::MetricsData;

pub type EventSender = Sender<Event>;

static EVENT_SENDER: Lazy<RwLock<Option<EventSender>>> = Lazy::new(|| RwLock::new(None));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    StatusChanged(ServerStatus),
    Log(LogEntry),
    Alert(LogEntry),
    MetricsUpdated(MetricsData),
    PlayerJoined(String),
    PlayerLeft(String),
    ServerStarted,
    ServerStopped,
    CommandExecuted(String),
    BackupStarted,
    BackupCompleted,
}

pub fn set_event_sender(sender: EventSender) {
    let mut writer = EVENT_SENDER.write().unwrap();
    *writer = Some(sender);
}

pub fn get_event_sender() -> Option<EventSender> {
    EVENT_SENDER.read().unwrap().clone()
}

pub fn emit_event(event: Event) {
    if let Some(sender) = get_event_sender() {
        let _ = sender.send(event);
    }
}

pub fn emit_log(message: String, source: String) {
    let log_entry = LogEntry::info(message, source);
    emit_event(Event::Log(log_entry));
}

pub fn emit_error(message: String, source: String) {
    let log_entry = LogEntry::error(message, source);
    emit_event(Event::Log(log_entry));
}

pub fn emit_status_change(status: ServerStatus) {
    emit_event(Event::StatusChanged(status));
}

pub fn emit_player_joined(player_name: String) {
    emit_event(Event::PlayerJoined(player_name.clone()));
    emit_log(format!("Player joined: {}", player_name), "server".to_string());
}

pub fn emit_player_left(player_name: String) {
    emit_event(Event::PlayerLeft(player_name.clone()));
    emit_log(format!("Player left: {}", player_name), "server".to_string());
}