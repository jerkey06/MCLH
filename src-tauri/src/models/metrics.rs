use crate::error::AppError; // For potential future use
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Represents a snapshot of server performance metrics at a specific time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsData {
    /// UNIX timestamp (seconds since epoch) when the metrics were collected.
    pub timestamp: u64,
    /// CPU usage of the server process (percentage). How this is calculated depends on the monitoring implementation.
    pub cpu_usage: f32,
    /// Memory currently used by the server process (in bytes).
    pub memory_usage: u64,
    /// Total physical memory available on the system (in bytes). Added for context.
    pub system_memory_total: u64, // Renamed from memory_total for clarity
    /// Current number of players connected to the server.
    pub player_count: u32,
    /// Maximum number of players allowed, according to server configuration.
    pub max_players: u32,
    /// Ticks Per Second (TPS) of the server, if available (often requires server-specific commands or plugins).
    pub tps: Option<f32>,
    /// Server process uptime in seconds.
    pub uptime: u64,
}

impl Default for MetricsData {
    /// Provides a default, zeroed-out MetricsData instance.
    fn default() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO) // Use Duration::ZERO on error
            .as_secs();

        Self {
            timestamp,
            cpu_usage: 0.0,
            memory_usage: 0,
            system_memory_total: 0, // Will be populated by monitor
            player_count: 0,
            max_players: 0, // Should be updated from config by monitor
            tps: None,
            uptime: 0,
        }
    }
}