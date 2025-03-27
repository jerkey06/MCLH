use crate::app_state::AppState;
use crate::error::Result; // For potential future use
use crate::models::metrics::MetricsData;
use log::{debug, error, info, warn};
use serde_json; // For serialization
use std::collections::VecDeque;
use std::fs::{self, File}; // Need fs for create_dir_all
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant}; // Use Instant for elapsed time

/// Maximum number of metrics entries to keep in memory (e.g., 1 hour worth).
const MAX_HISTORY_SIZE: usize = 3600;
/// How often to persist the metrics history to a file.
const PERSIST_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes

/// Collects and stores a history of server metrics.
/// Also handles persisting metrics to disk periodically.
pub struct MetricsCollector {
    // No AppState needed if log path is generated differently or passed in
    // state: Arc<AppState>,
    /// In-memory buffer holding recent metrics data.
    history: Mutex<VecDeque<MetricsData>>, // Wrap history in Mutex for thread safety
    /// Timestamp of the last time metrics were persisted to disk.
    last_persisted: Mutex<Instant>,
    /// Path to the directory where metrics logs should be stored.
    log_directory: PathBuf,
}

impl MetricsCollector {
    /// Creates a new MetricsCollector.
    pub fn new(log_directory: PathBuf) -> Self { // Removed AppState dependency
        info!(
            "Initializing MetricsCollector. History size: {}, Persist interval: {:?}",
            MAX_HISTORY_SIZE, PERSIST_INTERVAL
        );
        // Ensure log directory exists
        if !log_directory.exists() {
            info!("Creating metrics log directory: {}", log_directory.display());
            if let Err(e) = fs::create_dir_all(&log_directory) {
                error!("Failed to create metrics log directory '{}': {}", log_directory.display(), e);
                // Continue without persistence? Or panic? For now, just log error.
            }
        }

        Self {
            // state,
            history: Mutex::new(VecDeque::with_capacity(MAX_HISTORY_SIZE)),
            last_persisted: Mutex::new(Instant::now()),
            log_directory,
        }
    }

    /// Adds a new metrics data point to the history.
    /// Trims old data if history exceeds `MAX_HISTORY_SIZE`.
    /// Triggers persistence check.
    pub fn add_metrics(&self, metrics: MetricsData) -> Result<()> {
        let mut history_guard = self
            .history
            .lock()
            .map_err(|e| AppError::LockError(format!("Failed to lock metrics history: {}", e)))?;

        history_guard.push_back(metrics);
        debug!("Added metrics to history. Current size: {}", history_guard.len());

        // Trim history if it exceeds the maximum size
        while history_guard.len() > MAX_HISTORY_SIZE {
            history_guard.pop_front();
        }

        // Release history lock before checking persistence lock
        drop(history_guard);

        // Check if it's time to persist metrics
        let should_persist = {
            let last_persisted_guard = self.last_persisted.lock().map_err(|e| {
                AppError::LockError(format!("Failed to lock last_persisted time: {}", e))
            })?;
            last_persisted_guard.elapsed() >= PERSIST_INTERVAL
        }; // Lock released

        if should_persist {
            info!("Persist interval reached. Persisting metrics...");
            match self.persist_metrics() {
                Ok(_) => {
                    // Update last persisted time only on success
                    let mut last_persisted_guard = self.last_persisted.lock().map_err(|e| {
                        AppError::LockError(format!("Failed to lock last_persisted time after persist: {}", e))
                    })?;
                    *last_persisted_guard = Instant::now(); // Reset timer
                    info!("Metrics persisted successfully.");
                }
                Err(e) => {
                    error!("Failed to persist metrics: {}", e);
                    // Don't update last_persisted time, try again next time
                }
            }
        }
        Ok(())
    }

    /// Returns a clone of the entire metrics history.
    /// Potentially memory-intensive if history is large.
    pub fn get_history(&self) -> Result<Vec<MetricsData>> {
        self.history
            .lock()
            .map(|guard| guard.iter().cloned().collect())
            .map_err(|e| AppError::LockError(format!("Failed to lock metrics history for get: {}", e)))
    }

    /// Calculates average metrics over a specified recent duration.
    /// Returns None if no data is available in the specified duration.
    pub fn get_average_metrics(&self, duration: Duration) -> Result<Option<MetricsData>> {
        let history_guard = self
            .history
            .lock()
            .map_err(|e| AppError::LockError(format!("Failed to lock metrics history for average: {}", e)))?;

        if history_guard.is_empty() {
            debug!("Cannot calculate average metrics: History is empty.");
            return Ok(None);
        }

        // Use timestamp from the latest entry as 'now'
        let latest_metric = history_guard.back().unwrap(); // Safe due to is_empty check
        let now_ts = latest_metric.timestamp;
        let cutoff_ts = now_ts.saturating_sub(duration.as_secs()); // Prevent underflow

        let relevant_metrics: Vec<&MetricsData> = history_guard
            .iter()
            .filter(|m| m.timestamp >= cutoff_ts)
            .collect();

        let count = relevant_metrics.len();
        if count == 0 {
            debug!("No metrics found within the last {:?}.", duration);
            return Ok(None);
        }

        debug!("Calculating average metrics over {} data points.", count);
        let count_f32 = count as f32;
        let count_u64 = count as u64;

        let sum_cpu: f32 = relevant_metrics.iter().map(|m| m.cpu_usage).sum();
        let sum_memory: u64 = relevant_metrics.iter().map(|m| m.memory_usage).sum();
        // Average TPS only if available
        let (sum_tps, tps_count) = relevant_metrics.iter().fold((0.0, 0), |(sum, count), m| {
            if let Some(tps) = m.tps {
                (sum + tps, count + 1)
            } else {
                (sum, count)
            }
        });

        let avg_cpu = sum_cpu / count_f32;
        let avg_memory = sum_memory / count_u64;
        let avg_tps = if tps_count > 0 { Some(sum_tps / tps_count as f32) } else { None };

        // Return a new MetricsData with averaged values, using latest for others
        Ok(Some(MetricsData {
            cpu_usage: avg_cpu,
            memory_usage: avg_memory,
            tps: avg_tps,
            timestamp: now_ts, // Timestamp of the latest considered metric
            // Copy other fields from the latest metric
            system_memory_total: latest_metric.system_memory_total,
            player_count: latest_metric.player_count, // Average player count might also be useful
            max_players: latest_metric.max_players,
            uptime: latest_metric.uptime,
        }))
    }

    /// Persists the current metrics history to a JSON file (e.g., logs/metrics_YYYYMMDD.json).
    fn persist_metrics(&self) -> Result<()> {
        let history_guard = self
            .history
            .lock()
            .map_err(|e| AppError::LockError(format!("Failed to lock metrics history for persist: {}", e)))?;

        if history_guard.is_empty() {
            debug!("Skipping metrics persistence: History is empty.");
            return Ok(());
        }

        // Generate filename based on current date
        let filename = format!("metrics_{}.json", chrono::Local::now().format("%Y%m%d"));
        let log_path = self.log_directory.join(filename);
        debug!("Persisting metrics to: {}", log_path.display());

        // Serialize the VecDeque directly
        let json_data = serde_json::to_string_pretty(&*history_guard).map_err(|e| {
            AppError::InternalEventError(format!("Failed to serialize metrics history: {}", e))
            // Using InternalEventError might not be perfect, maybe a SerializationError?
        })?;

        // Write to file
        let mut file = File::create(&log_path).map_err(|e| {
            AppError::IoError(io::Error::new(
                e.kind(),
                format!("Failed to create metrics file {}: {}", log_path.display(), e),
            ))
        })?;

        file.write_all(json_data.as_bytes()).map_err(|e| {
            AppError::IoError(io::Error::new(
                e.kind(),
                format!("Failed to write metrics file {}: {}", log_path.display(), e),
            ))
        })?;

        file.flush()?; // Ensure write completes

        Ok(())
    }
}