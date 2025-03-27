use std::sync::{Arc, Mutex, RwLock}; // Use RwLock for read-heavy data if applicable
use crate::models::server_status::ServerStatus;
use crate::models::metrics::MetricsData;
use crate::error::{AppError, Result}; // Import the Result type alias

// Consider using more specific types if possible, e.g., PathBuf for paths
use std::path::PathBuf;

/// Holds the shared state of the application.
#[derive(Debug)] // Added Debug derive
pub struct AppState {
    /// Current status of the Minecraft server process.
    /// Mutex is suitable here as status changes often but reads might be frequent too.
    pub server_status: Mutex<ServerStatus>,

    /// Latest performance metrics collected.
    /// Mutex is fine if updates and reads are balanced.
    pub metrics: Mutex<MetricsData>,

    /// The root directory where the server files are located.
    pub server_directory: PathBuf, // Using PathBuf is better for paths

    /// Path to the detected Java executable.
    pub java_path: PathBuf, // Using PathBuf

    /// Name of the server JAR file (e.g., "server.jar", "paper.jar").
    pub server_jar: String,

    /// Command-line arguments to pass to the Java process (e.g., memory allocation).
    /// RwLock might be better if args are read often but changed rarely after init.
    pub server_args: RwLock<Vec<String>>,

    // Removed `process_handle: ()`. The actual process handle should be managed
    // within the `process_manager` module, which can access and update
    // the `server_status` in this AppState.
}

impl AppState {
    /// Creates a new instance of the application state, wrapped in an Arc for shared ownership.
    pub fn new(server_directory: String, java_path: String, server_jar: String) -> Result<Arc<Self>> {
        let server_dir_path = PathBuf::from(server_directory);
        let java_path_buf = PathBuf::from(java_path);

        // Basic validation
        if !java_path_buf.exists() {
            // This check might be better placed in java_detector or main setup
            // but serves as an example.
            // return Err(AppError::JavaNotFound); // Or handle earlier
        }

        // Default Java arguments
        // TODO: Load these from a persistent config file eventually
        let default_args = vec![
            "-Xmx2G".to_string(), // Example: 2GB max heap
            "-Xms1G".to_string(), // Example: 1GB initial heap
            // Consider adding Aikar's flags for optimized servers (Paper/Spigot)
            // "-XX:+UseG1GC", "-XX:+ParallelRefProcEnabled", ... etc.
            "-jar".to_string(),
            // server_jar will be added dynamically before starting the process
        ];

        Ok(Arc::new(Self {
            server_status: Mutex::new(ServerStatus::Stopped),
            metrics: Mutex::new(MetricsData::default()),
            server_directory: server_dir_path,
            java_path: java_path_buf,
            server_jar,
            server_args: RwLock::new(default_args),
        }))
    }

    // --- Helper methods to access state safely ---

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

    /// Updates the metrics data.
    pub fn update_metrics(&self, new_metrics: MetricsData) -> Result<()> {
        let mut guard = self.metrics
            .lock()
            .map_err(|e| AppError::LockError(format!("Failed to lock metrics for writing: {}", e)))?;
        *guard = new_metrics;
        Ok(())
    }

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

    /// Gets the full path to the server JAR file.
    pub fn get_server_jar_path(&self) -> PathBuf {
        self.server_directory.join(&self.server_jar)
    }
}