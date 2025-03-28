use crate::error::{AppError, Result};
use crate::models::config::ServerConfig; // Import ServerConfig for direct property access (optional)
use crate::models::metrics::MetricsData;
use crate::models::server_status::ServerStatus;
use log::{error, trace}; // Import log
use std::collections::HashMap; // For property access
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// Holds the shared state of the application.
#[derive(Debug)]
pub struct AppState {
    /// Current status of the Minecraft server process.
    pub server_status: Mutex<ServerStatus>,
    /// Latest performance metrics collected. Holds current player_count.
    pub metrics: Mutex<MetricsData>,
    /// The root directory where the server files are located.
    pub server_directory: PathBuf,
    /// Path to the detected Java executable.
    pub java_path: PathBuf,
    /// Name of the server JAR file (e.g., "server.jar", "paper.jar").
    pub server_jar: String,
    /// Command-line arguments to pass to the Java process.
    pub server_args: RwLock<Vec<String>>,
    /// Handle to the running server process, if active. Managed by process_manager.
    pub process_handle: Mutex<Option<Child>>,
    /// Timeout in seconds for graceful server shutdown before forcing termination.
    pub stop_timeout_secs: u64,

    // Store server properties directly here for quick access by monitor? Or read file?
    // Reading file might be slow. Let's assume it's updated here when config changes.
    // This duplicates data from ServerConfig persistence layer, needs careful syncing.
    // Alternative: ResourceMonitor reads file or accesses ServerConfig state if managed by Tauri.
    // For simplicity now, let resource_monitor read from here if populated.
    // Needs to be updated when update_config_fully runs.
    pub server_properties: RwLock<HashMap<String, String>>,
}

impl AppState {
    /// Creates a new instance of the application state, wrapped in an Arc.
    pub fn new(server_directory: String, java_path: String, server_jar: String) -> Result<Arc<Self>> {
        let server_dir_path = PathBuf::from(server_directory);
        let java_path_buf = PathBuf::from(java_path);

        // Default Java arguments (consider making these configurable elsewhere)
        let default_java_args = vec![
            "-Xmx2G".to_string(), // Example: 2GB max heap
            "-Xms1G".to_string(), // Example: 1GB initial heap
        ];

        // Load initial properties (empty if file doesn't exist yet)
        // Requires AppState to be Arc later, do it separately or pass paths
        // Let's initialize empty and load later in initialize_app maybe
        let initial_properties = HashMap::new();


        Ok(Arc::new(Self {
            server_status: Mutex::new(ServerStatus::Stopped),
            metrics: Mutex::new(MetricsData::default()), // player_count starts at 0 here
            server_directory: server_dir_path,
            java_path: java_path_buf,
            server_jar,
            server_args: RwLock::new(default_java_args),
            process_handle: Mutex::new(None),
            stop_timeout_secs: 30, // Default timeout
            server_properties: RwLock::new(initial_properties), // Start empty
        }))
    }

    // --- Helper methods ---

    /// Gets the current server status.
    pub fn get_status(&self) -> Result<ServerStatus> {
        self.server_status
            .lock()
            .map(|guard| guard.clone())
            .map_err(|e| AppError::LockError(format!("Failed to lock server_status: {}", e)))
    }

    /// Sets the server status.
    pub fn set_status(&self, new_status: ServerStatus) -> Result<()> {
        let mut guard = self.server_status
            .lock()
            .map_err(|e| AppError::LockError(format!("Failed to lock server_status for writing: {}", e)))?;
        *guard = new_status;
        Ok(())
    }

    /// Gets a clone of the current metrics data.
    pub fn get_metrics(&self) -> Result<MetricsData> {
        self.metrics
            .lock()
            .map(|guard| guard.clone())
            .map_err(|e| AppError::LockError(format!("Failed to lock metrics: {}", e)))
    }

    /// Updates the metrics data (usually called by resource_monitor).
    pub fn update_metrics(&self, new_metrics: MetricsData) -> Result<()> {
        let mut guard = self.metrics
            .lock()
            .map_err(|e| AppError::LockError(format!("Failed to lock metrics for writing: {}", e)))?;
        *guard = new_metrics;
        Ok(())
    }


    // --- Player Count Management (internal use by process_manager) ---

    /// Safely increments the player count in the metrics data.
    pub(crate) fn increment_player_count(&self) {
        match self.metrics.lock() {
            Ok(mut guard) => {
                guard.player_count = guard.player_count.saturating_add(1); // Prevent overflow
                trace!("Player count incremented to: {}", guard.player_count);
            }
            Err(e) => {
                error!("Failed to lock metrics to increment player count: {}", e);
            }
        }
    }

    /// Safely decrements the player count in the metrics data.
    pub(crate) fn decrement_player_count(&self) {
        match self.metrics.lock() {
            Ok(mut guard) => {
                guard.player_count = guard.player_count.saturating_sub(1); // Prevent underflow below 0
                trace!("Player count decremented to: {}", guard.player_count);
            }
            Err(e) => {
                error!("Failed to lock metrics to decrement player count: {}", e);
            }
        }
    }

    /// Safely resets the player count to 0.
    pub(crate) fn reset_player_count(&self) {
        match self.metrics.lock() {
            Ok(mut guard) => {
                if guard.player_count != 0 {
                    trace!("Resetting player count from {} to 0.", guard.player_count);
                    guard.player_count = 0;
                }
            }
            Err(e) => {
                error!("Failed to lock metrics to reset player count: {}", e);
            }
        }
    }

    // --- Config Accessors / Mutators ---

    /// Gets a clone of the server arguments.
    pub fn get_server_args(&self) -> Result<Vec<String>> {
        self.server_args
            .read()
            .map(|guard| guard.clone())
            .map_err(|e| AppError::LockError(format!("Failed to lock server_args for reading: {}", e)))
    }

    /// Updates the server arguments.
    pub fn set_server_args(&self, new_args: Vec<String>) -> Result<()> {
        let mut guard = self.server_args
            .write()
            .map_err(|e| AppError::LockError(format!("Failed to lock server_args for writing: {}", e)))?;
        *guard = new_args;
        Ok(())
    }

    /// Gets a clone of the cached server properties.
    pub fn get_server_properties(&self) -> Result<HashMap<String, String>> {
        self.server_properties
            .read()
            .map(|guard| guard.clone())
            .map_err(|e| AppError::LockError(format!("Failed to lock server_properties for reading: {}", e)))
    }

    /// Updates the cached server properties. Called after config is saved.
    pub fn update_server_properties_cache(&self, new_props: HashMap<String, String>) -> Result<()> {
        let mut guard = self.server_properties
            .write()
            .map_err(|e| AppError::LockError(format!("Failed to lock server_properties for writing: {}", e)))?;
        *guard = new_props;
        Ok(())
    }


    // --- Process Handle Management (internal use by process_manager) ---

    /// Safely gets the process handle, taking it out and leaving None. Use with care.
    pub(crate) fn take_process_handle(&self) -> Result<Option<Child>> {
        self.process_handle
            .lock()
            .map(|mut guard| guard.take())
            .map_err(|e| AppError::LockError(format!("Failed to lock process_handle: {}", e)))
    }

    /// Safely sets the process handle.
    pub(crate) fn set_process_handle(&self, process: Option<Child>) -> Result<()> {
        let mut guard = self.process_handle
            .lock()
            .map_err(|e| AppError::LockError(format!("Failed to lock process_handle for writing: {}", e)))?;
        *guard = process;
        Ok(())
    }

    // --- Other Getters ---

    /// Gets the configured stop timeout.
    pub fn get_stop_timeout(&self) -> Duration {
        Duration::from_secs(self.stop_timeout_secs)
    }

    /// Gets the full path to the server JAR file.
    pub fn get_server_jar_path(&self) -> PathBuf {
        self.server_directory.join(&self.server_jar)
    }
}

// Need to implement Send + Sync for Child within the Mutex for thread safety.
// Child itself is Send + Sync on supported platforms (Unix, Windows).
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}