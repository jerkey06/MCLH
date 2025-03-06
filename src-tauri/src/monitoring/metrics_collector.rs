use std::sync::Arc;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use std::fs::File;
use std::io::Write;

use crate::app_state::AppState;
use crate::models::metrics::MetricsData;

const MAX_HISTORY_SIZE: usize = 3600; // Store 1 hour of metrics at 1 per second

pub struct MetricsCollector {
    state: Arc<AppState>,
    history: VecDeque<MetricsData>,
    last_persisted: Instant,
}

impl MetricsCollector {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            history: VecDeque::with_capacity(MAX_HISTORY_SIZE),
            last_persisted: Instant::now(),
        }
    }

    pub fn add_metrics(&mut self, metrics: MetricsData) {
        self.history.push_back(metrics);

        if self.history.len() > MAX_HISTORY_SIZE {
            self.history.pop_front();
        }

        if self.last_persisted.elapsed() > Duration::from_secs(300) {
            self.persist_metrics();
            self.last_persisted = Instant::now();
        }
    }

    pub fn get_history(&self) -> Vec<MetricsData> {
        self.history.iter().cloned().collect()
    }

    pub fn get_average_metrics(&self, duration: Duration) -> Option<MetricsData> {
        if self.history.is_empty() {
            return None;
        }

        let now = self.history.back().unwrap().timestamp;
        let threshold = now - duration.as_secs();

        let relevant_metrics: Vec<&MetricsData> = self.history.iter()
            .filter(|m| m.timestamp >= threshold)
            .collect();

        if relevant_metrics.is_empty() {
            return None;
        }

        let count = relevant_metrics.len() as f32;
        let avg_cpu = relevant_metrics.iter().map(|m| m.cpu_usage).sum::<f32>() / count;
        let avg_memory = relevant_metrics.iter().map(|m| m.memory_usage).sum::<u64>() / count as u64;

        let latest = self.history.back().unwrap().clone();
        Some(MetricsData {
            cpu_usage: avg_cpu,
            memory_usage: avg_memory,
            ..latest
        })
    }

    fn persist_metrics(&self) {
        if self.history.is_empty() {
            return;
        }

        let log_path = format!("{}/logs/metrics_{}.json",
                               &self.state.server_directory,
                               chrono::Local::now().format("%Y%m%d"));

        if let Ok(mut file) = File::create(log_path) {
            if let Ok(json) = serde_json::to_string(&self.history) {
                let _ = file.write_all(json.as_bytes());
            }
        }
    }
}