use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub server_properties: HashMap<String, String>,
    pub java_args: Vec<String>,
    pub modpack: Option<ModpackConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackConfig {
    pub name: String,
    pub version: String,
    pub installer_url: Option<String>,
    pub forge_version: Option<String>,
    pub fabric_version: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        let mut server_properties = HashMap::new();
        server_properties.insert("server-port".to_string(), "25565".to_string());
        server_properties.insert("gamemode".to_string(), "survival".to_string());
        server_properties.insert("difficulty".to_string(), "normal".to_string());
        server_properties.insert("max-players".to_string(), "20".to_string());
        server_properties.insert("spawn-protection".to_string(), "16".to_string());
        server_properties.insert("enable-command-block".to_string(), "false".to_string());

        Self {
            server_properties,
            java_args: vec!["-Xmx2G".to_string(), "-Xms1G".to_string()],
            modpack: None,
        }
    }
}