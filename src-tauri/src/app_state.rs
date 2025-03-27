use std::sync::{Arc, Mutex};
use crate::models::server_status::ServerStatus;
use crate::models::metrics::MetricsData;

pub struct AppState {
    pub server_status: Mutex<ServerStatus>,
    pub metrics: Mutex<MetricsData>,
    pub server_directory: String,
    pub java_path: String,
    pub server_jar: String,
    pub server_args: Vec<String>,
    pub process_handle: ()
}

impl AppState {
    pub fn new(server_directory: String, java_path: String, server_jar: String) -> Arc<Self> {
        Arc::new(Self {
            server_status: Mutex::new(ServerStatus::Stopped),
            metrics: Mutex::new(MetricsData::default()),
            server_directory,
            java_path,
            server_jar,
            server_args: vec!["-Xmx2G".to_string(), "-jar".to_string()],
        })
    }
}