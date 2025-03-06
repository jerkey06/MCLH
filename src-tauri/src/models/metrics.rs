use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsData {
    pub timestamp: u64,
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub memory_total: u64,
    pub player_count: u32,
    pub max_players: u32,
    pub tps: Option<f32>,
    pub uptime: u64,
}

impl Default for MetricsData {
    fn default() -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            cpu_usage: 0.0,
            memory_usage: 0,
            memory_total: 0,
            player_count: 0,
            max_players: 20,
            tps: None,
            uptime: 0,
        }
    }
}