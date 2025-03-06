use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{ProcessExt, System, SystemExt};
use tauri::AppHandle;

use crate::app_state::AppState;
use crate::models::server_status::ServerStatus;
use crate::models::metrics::MetricsData;
use crate::api::events;

pub fn start_monitoring(state: Arc<AppState>, app_handle: AppHandle) {
    thread::spawn(move || {
        let mut sys = System::new_all();
        let mut last_update = Instant::now();
        let mut start_time: Option<Instant> = None;

        loop {
            thread::sleep(Duration::from_secs(1));

            let status = state.server_status.lock().unwrap().clone();
            if status == ServerStatus::Running {
                if start_time.is_none() {
                    start_time = Some(Instant::now());
                }

                sys.refresh_all();
                let java_processes = sys.processes_by_name("java");

                let mut metrics = MetricsData::default();

                for process in java_processes {
                    let cmd = process.cmd().join(" ");
                    if cmd.contains(&state.server_jar) {
                        metrics.cpu_usage = process.cpu_usage();
                        metrics.memory_usage = process.memory() * 1024; // Convert to bytes
                        break;
                    }
                }

                metrics.memory_total = sys.total_memory();

                if let Some(start) = start_time {
                    metrics.uptime = start.elapsed().as_secs();
                }

                // Extract TPS and player count from logs (would require more complex parsing)
                // For now, set placeholder values
                metrics.player_count = 0;
                metrics.max_players = 20;

                *state.metrics.lock().unwrap() = metrics.clone();

                // Only send events at a reasonable rate to avoid flooding
                if last_update.elapsed() >= Duration::from_secs(1) {
                    last_update = Instant::now();

                    // Send metric update via event
                    if let Some(sender) = events::get_event_sender() {
                        let _ = sender.send(events::Event::MetricsUpdated(metrics));
                    }

                    // Also emit Tauri event for the frontend
                    let _ = app_handle.emit_all("metrics-updated", metrics);
                }
            } else {
                start_time = None;
            }
        }
    });
}