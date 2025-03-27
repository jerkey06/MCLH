use std::sync::{Arc, Mutex, RwLock};
use crate::models::server_status::ServerStatus;
use crate::models::metrics::MetricsData;
use crate::error::Result;
use std::path::PathBuf;
use std::process::Child; // <--- IMPORT Child

/// Holds the shared state of the application.
#[derive(Debug)]
pub struct AppState {
    /// Current status of the Minecraft server process.
    pub server_status: Mutex<ServerStatus>,
    /// Latest performance metrics collected.
    pub metrics: Mutex<MetricsData>,
    /// The root directory where the server files are located.
    pub server_directory: PathBuf,
    /// Path to the detected Java executable.
    pub java_path: PathBuf,
    /// Name of the server JAR file (e.g., "server.jar", "paper.jar").
    pub server_jar: String,
    /// Command-line arguments to pass to the Java process.
    pub server_args: RwLock<Vec<String>>,

    // --- ADDED ---
    /// Handle to the running server process, if active.
    /// This is managed exclusively by the process_manager module.
    pub process_handle: Mutex<Option<Child>>, // <--- ADD THIS LINE

    // --- Optional: Configuration for process manager ---
    /// Timeout in seconds for graceful server shutdown before forcing termination.
    pub stop_timeout_secs: u64,
}

impl AppState {
    pub fn new(server_directory: String, java_path: String, server_jar: String) -> Result<Arc<Self>> {
        let server_dir_path = PathBuf::from(server_directory);
        let java_path_buf = PathBuf::from(java_path);

        let default_args = vec![
            "-Xmx2G".to_string(),
            "-Xms1G".to_string(),
            "-jar".to_string(),
        ];

        Ok(Arc::new(Self {
            server_status: Mutex::new(ServerStatus::Stopped),
            metrics: Mutex::new(MetricsData::default()),
            server_directory: server_dir_path,
            java_path: java_path_buf,
            server_jar,
            server_args: RwLock::new(default_args),
            process_handle: Mutex::new(None), // <--- INITIALIZE TO None
            stop_timeout_secs: 30, // Default timeout
        }))
    }

    // --- Helper methods (get_status, set_status etc. remain the same) ---
    // No public methods needed for direct process_handle manipulation from outside process_manager

    /// Safely gets the process handle (internal use by process_manager mostly).
    /// Takes the handle out, leaving None. Use with care.
    pub(crate) fn take_process_handle(&self) -> Result<Option<Child>> {
        self.process_handle
            .lock()
            .map(|mut guard| guard.take()) // take() removes the value from Option
            .map_err(|e| AppError::LockError(format!("Failed to lock process_handle: {}", e)))
    }

    /// Safely sets the process handle (internal use by process_manager).
    pub(crate) fn set_process_handle(&self, process: Option<Child>) -> Result<()> {
        let mut guard = self.process_handle
            .lock()
            .map_err(|e| AppError::LockError(format!("Failed to lock process_handle for writing: {}", e)))?;
        *guard = process;
        Ok(())
    }

    /// Gets the configured stop timeout.
    pub fn get_stop_timeout(&self) -> Duration {
        Duration::from_secs(self.stop_timeout_secs)
    }

    // ... other helper methods ...
    /// Gets the full path to the server JAR file.
    pub fn get_server_jar_path(&self) -> PathBuf {
        self.server_directory.join(&self.server_jar)
    }
}

// Need to implement Send + Sync for Child within the Mutex for thread safety.
// Child itself is Send + Sync on supported platforms (Unix, Windows).
// If targeting a platform where it isn't, this would need conditional compilation or alternative approach.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}
// NOTE: Using unsafe impl Send/Sync relies on the underlying types (like Child)
// being correctly Send/Sync. This is generally true for std::process::Child
// but be mindful if adding non-Send/Sync types later without proper wrappers.