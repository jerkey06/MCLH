use crate::error::{AppError, Result as AppResult};
use log::{debug, trace};
use sysinfo::{Pid, System, SystemExt};

/// Checks if a process with the given PID is currently running.
/// Note: PID recycling means a new process could have the same PID later.
/// This check is a snapshot in time.
pub fn is_process_running(pid: u32) -> bool {
    trace!("Checking if process with PID {} is running...", pid);
    let mut sys = System::new(); // Create a new system instance for refresh
    sys.refresh_process(Pid::from_u32(pid)); // Refresh only the specific process

    let is_running = sys.process(Pid::from_u32(pid)).is_some();
    debug!("Process PID {} running status: {}", pid, is_running);
    is_running
}

// Potential future functions:
// pub fn kill_process(pid: u32, force: bool) -> AppResult<()> { ... }
// pub fn get_process_resource_usage(pid: u32) -> AppResult<ProcessMetrics> { ... }