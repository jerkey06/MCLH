use crate::api::events::{self, emit_event, emit_log}; // Use helpers
use crate::app_state::AppState;
use crate::error::Result; // For potential future use
use crate::models::log_entry::LogLevel; // Use our LogLevel
use crate::models::metrics::MetricsData;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize}; // For config persistence
use std::sync::{Arc, Mutex, RwLock}; // Use RwLock for thresholds if needed
use std::time::Duration; // For alert cooldown

// Configuration for alert thresholds. Could be loaded from a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// CPU usage percentage (0-100) above which an alert is triggered.
    pub cpu_threshold_percent: f32,
    /// Memory usage percentage (0-100) above which an alert is triggered.
    pub memory_threshold_percent: f32,
    /// Player count at or above which a "server nearly full" alert is triggered.
    pub player_threshold_count: u32,
    /// Minimum duration (in seconds) between identical alerts to prevent spam.
    pub alert_cooldown_secs: u64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            cpu_threshold_percent: 85.0,   // Default 85% CPU
            memory_threshold_percent: 85.0, // Default 85% Memory
            player_threshold_count: 18,     // Default 18 players (if max is 20)
            alert_cooldown_secs: 300,       // Default 5 minutes cooldown
        }
    }
}

/// Manages checking metrics against thresholds and emitting alert events.
pub struct AlertManager {
    // No AppState needed if metrics are passed directly
    // state: Arc<AppState>,
    /// Configurable thresholds for triggering alerts. RwLock allows concurrent reads.
    thresholds: RwLock<AlertThresholds>,
    /// Tracks the last time (timestamp) each type of alert was triggered.
    last_cpu_alert_ts: Mutex<Option<u64>>,
    last_memory_alert_ts: Mutex<Option<u64>>,
    last_player_alert_ts: Mutex<Option<u64>>,
}

impl AlertManager {
    /// Creates a new AlertManager with default thresholds.
    pub fn new() -> Self { // Removed AppState dependency
        info!("Initializing AlertManager with default thresholds.");
        Self {
            // state,
            thresholds: RwLock::new(AlertThresholds::default()),
            last_cpu_alert_ts: Mutex::new(None),
            last_memory_alert_ts: Mutex::new(None),
            last_player_alert_ts: Mutex::new(None),
        }
    }

    /// Updates the alert thresholds.
    /// TODO: Persist these thresholds to a config file.
    pub fn set_thresholds(&self, thresholds: AlertThresholds) -> Result<()> {
        info!(
            "Updating alert thresholds: CPU={:.1}%, Mem={:.1}%, Players={}, Cooldown={}s",
            thresholds.cpu_threshold_percent,
            thresholds.memory_threshold_percent,
            thresholds.player_threshold_count,
            thresholds.alert_cooldown_secs
        );
        let mut writer = self.thresholds.write().map_err(|e| {
            AppError::LockError(format!("Failed to lock thresholds for writing: {}", e))
        })?;
        *writer = thresholds;
        Ok(())
    }

    /// Returns a clone of the current alert thresholds.
    pub fn get_thresholds(&self) -> Result<AlertThresholds> {
        self.thresholds
            .read()
            .map(|guard| guard.clone())
            .map_err(|e| AppError::LockError(format!("Failed to lock thresholds for reading: {}", e)))
    }


    /// Checks the given metrics against the configured thresholds and triggers alerts if needed.
    pub fn check_alerts(&self, metrics: &MetricsData) {
        // Use read lock for thresholds - allows concurrent checks if thresholds aren't being modified
        let thresholds = match self.thresholds.read() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Failed to read alert thresholds: {}", e);
                return; // Cannot check alerts without thresholds
            }
        };

        let now = metrics.timestamp; // Use timestamp from metrics data
        let cooldown_duration = thresholds.alert_cooldown_secs;

        // --- Check CPU Alert ---
        if metrics.cpu_usage > thresholds.cpu_threshold_percent {
            self.check_and_send_alert(
                &self.last_cpu_alert_ts,
                now,
                cooldown_duration,
                || { // Closure to generate message only if needed
                    format!(
                        "High CPU Usage: {:.1}% (Threshold: {:.1}%)",
                        metrics.cpu_usage, thresholds.cpu_threshold_percent
                    )
                },
            );
        }

        // --- Check Memory Alert ---
        // Avoid division by zero if system_memory_total is 0
        if metrics.system_memory_total > 0 {
            let memory_percent =
                (metrics.memory_usage as f64 / metrics.system_memory_total as f64 * 100.0) as f32; // Use f64 for intermediate calc
            if memory_percent > thresholds.memory_threshold_percent {
                self.check_and_send_alert(
                    &self.last_memory_alert_ts,
                    now,
                    cooldown_duration,
                    || {
                        format!(
                            "High Memory Usage: {:.1}% ({:.1} MiB / {:.1} MiB) (Threshold: {:.1}%)",
                            memory_percent,
                            metrics.memory_usage as f64 / 1024.0 / 1024.0,
                            metrics.system_memory_total as f64 / 1024.0 / 1024.0,
                            thresholds.memory_threshold_percent
                        )
                    }
                );
            }
        } else {
            // Log warning if total memory is unknown
            // This check should only happen once ideally
            warn!("Cannot calculate memory percentage: system_memory_total is zero.");
        }


        // --- Check Player Count Alert ---
        // Ensure max_players is valid to avoid nonsensical alerts
        if metrics.max_players > 0 && metrics.player_count >= thresholds.player_threshold_count {
            self.check_and_send_alert(
                &self.last_player_alert_ts,
                now,
                cooldown_duration,
                || {
                    format!(
                        "Server Almost Full: {} / {} players (Threshold: {})",
                        metrics.player_count, metrics.max_players, thresholds.player_threshold_count
                    )
                }
            );
        }
    }

    /// Helper function to check cooldown and send an alert message.
    fn check_and_send_alert<F>(
        &self,
        last_alert_mutex: &Mutex<Option<u64>>,
        current_timestamp: u64,
        cooldown_secs: u64,
        message_fn: F, // Use a closure to generate message lazily
    ) where
        F: FnOnce() -> String,
    {
        let mut last_alert_ts_guard = match last_alert_mutex.lock() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Failed to lock alert timestamp: {}", e);
                return;
            }
        };

        let should_alert = match *last_alert_ts_guard {
            Some(last_ts) => (current_timestamp > last_ts) && (current_timestamp - last_ts >= cooldown_secs),
            None => true, // Alert if never alerted before
        };

        if should_alert {
            let message = message_fn(); // Generate the message only now
            info!("Triggering Alert: {}", message); // Log the alert
            self.send_alert_event(&message); // Send the event
            *last_alert_ts_guard = Some(current_timestamp); // Update last alert time
        }
    }

    /// Sends an alert event and a corresponding warning log event.
    fn send_alert_event(&self, message: &str) {
        // Use helpers from api::events
        // Alert event (specific type for UI filtering?)
        emit_event(events::Event::Alert(message.to_string()));
        // Also send as a standard log message
        emit_log(LogLevel::Warn, message.to_string(), "AlertManager".to_string());
    }
}