use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::config::server_properties; // Import for default properties logic

/// Represents the complete server configuration managed by the application.
/// This structure can be serialized/deserialized to/from a persistent format (e.g., JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Key-value pairs loaded from/to be saved to server.properties.
    pub server_properties: HashMap<String, String>,
    /// Java Virtual Machine arguments (e.g., ["-Xmx2G", "-Xms1G", "-jar"]).
    /// Note: The "-jar server.jar nogui" part is typically added dynamically by process_manager.
    pub java_args: Vec<String>,
    /// Information about the installed modpack, if any.
    pub modpack: Option<ModpackConfig>,
    // Add other manager-specific settings here if needed in the future
    // e.g., backup_schedule: Option<String>, auto_restart_on_crash: bool
}

/// Represents metadata about an installed modpack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackConfig {
    /// Display name of the modpack.
    pub name: String,
    /// Version identifier of the modpack.
    pub version: String,
    /// URL the modpack was originally downloaded from (for reference or updates).
    pub source_url: Option<String>, // Changed from installer_url for generality
    /// Required Forge version, if applicable.
    pub forge_version: Option<String>,
    /// Required Fabric version, if applicable.
    pub fabric_version: Option<String>,
    // Add other relevant metadata, e.g., manifest ID, author
}

impl Default for ServerConfig {
    /// Provides a default configuration, useful for initializing or resetting.
    fn default() -> Self {
        // Use the default properties creation logic from server_properties module
        let default_props = server_properties::get_default_properties_map();

        // Default Java arguments (consider making these configurable elsewhere)
        let default_java_args = vec![
            "-Xmx2G".to_string(), // Example: 2GB max heap
            "-Xms1G".to_string(), // Example: 1GB initial heap
            // Note: process_manager should add "-jar <jarname> nogui" dynamically
        ];

        Self {
            server_properties: default_props,
            java_args: default_java_args,
            modpack: None,
        }
    }
}