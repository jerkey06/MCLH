use std::sync::{Arc, Mutex};
use crate::app_state::AppState;
use crate::models::metrics::MetricsData;
use crate::models::log_entry::LogEntry;
use crate::api::events;

pub struct AlertThresholds {
    pub cpu_threshold: f32,
    pub memory_threshold: f32,
    pub player_threshold: u32,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            cpu_threshold: 80.0,
            memory_threshold: 80.0,
            player_threshold: 18,
        }
    }
}

pub struct AlertManager {
    state: Arc<AppState>,
    thresholds: Mutex<AlertThresholds>,
    last_cpu_alert: Mutex<Option<u64>>,
    last_memory_alert: Mutex<Option<u64>>,
    last_player_alert: Mutex<Option<u64>>,
}

impl AlertManager {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            thresholds: Mutex::new(AlertThresholds::default()),
            last_cpu_alert: Mutex::new(None),
            last_memory_alert: Mutex::new(None),
            last_player_alert: Mutex::new(None),
        }
    }

    pub fn set_thresholds(&self, thresholds: AlertThresholds) {
        let mut t = self.thresholds.lock().unwrap();
        *t = thresholds;
    }

    pub fn check_alerts(&self, metrics: &MetricsData) {
        let thresholds = self.thresholds.lock().unwrap();
        let now = metrics.timestamp;

        // CPU alert
        if metrics.cpu_usage > thresholds.cpu_threshold {
            let mut last_alert = self.last_cpu_alert.lock().unwrap();
            if last_alert.is_none() || now - last_alert.unwrap() > 300 {
                *last_alert = Some(now);

                let message = format!("High CPU usage: {:.1}% (threshold: {:.1}%)",
                                      metrics.cpu_usage, thresholds.cpu_threshold);
                self.send_alert(message);
            }
        }

        // Memory alert
        let memory_percent = metrics.memory_usage as f32 / metrics.memory_total as f32 * 100.0;
        if memory_percent > thresholds.memory_threshold {
            let mut last_alert = self.last_memory_alert.lock().unwrap();
            if last_alert.is_none() || now - last_alert.unwrap() > 300 {
                *last_alert = Some(now);

                let message = format!("High memory usage: {:.1}% (threshold: {:.1}%)",
                                      memory_percent, thresholds.memory_threshold);
                self.send_alert(message);
            }
        }

        // Player count alert
        if metrics.player_count >= thresholds.player_threshold {
            let mut last_alert = self.last_player_alert.lock().unwrap();
            if last_alert.is_none() || now - last_alert.unwrap() > 300 {
                *last_alert = Some(now);

                let message = format!("Server almost full: {}/{} players (threshold: {})",
                                      metrics.player_count, metrics.max_players,
                                      thresholds.player_threshold);
                self.send_alert(message);
            }
        }
    }

    fn send_alert(&self, message: String) {
        let log_entry = LogEntry::warning(message.clone(), "alert_manager".to_string());

        if let Some(sender) = events::get_event_sender() {
            let _ = sender.send(events::Event::Alert(log_entry.clone()));
            let _ = sender.send(events::Event::Log(log_entry));
        }
    }
}