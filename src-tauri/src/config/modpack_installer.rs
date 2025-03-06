use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use reqwest;

use crate::app_state::AppState;
use crate::models::config::ModpackConfig;
use crate::error::{AppError, Result};
use crate::models::log_entry::{LogEntry, LogLevel};
use crate::api::events;

pub struct ModpackInstaller {
    state: Arc<AppState>,
}

impl ModpackInstaller {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub async fn install_modpack(&self, config: &ModpackConfig) -> Result<()> {
        let server_dir = PathBuf::from(&self.state.server_directory);

        // Log the start of installation
        self.log_info(format!("Starting installation of modpack: {}", config.name));

        // First handle Forge or Fabric installation if needed
        if let Some(forge_version) = &config.forge_version {
            self.install_forge(forge_version).await?;
        } else if let Some(fabric_version) = &config.fabric_version {
            self.install_fabric(fabric_version).await?;
        }

        // Then download and install modpack if URL is provided
        if let Some(installer_url) = &config.installer_url {
            self.download_and_install_modpack(installer_url).await?;
        }

        self.log_info(format!("Modpack {} installed successfully", config.name));

        Ok(())
    }

    async fn install_forge(&self, forge_version: &str) -> Result<()> {
        let server_dir = PathBuf::from(&self.state.server_directory);
        let installer_filename = format!("forge-{}-installer.jar", forge_version);
        let installer_path = server_dir.join(&installer_filename);

        self.log_info(format!("Installing Forge version {}", forge_version));

        // Download Forge installer
        let forge_url = format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{}/forge-{}-installer.jar",
            forge_version, forge_version
        );

        self.download_file(&forge_url, &installer_path).await?;

        // Run Forge installer
        let output = Command::new(&self.state.java_path)
            .arg("-jar")
            .arg(&installer_path)
            .arg("--installServer")
            .current_dir(&server_dir)
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            self.log_error(format!("Forge installation failed: {}", error));
            return Err(AppError::ProcessError(format!("Forge installation failed: {}", error)));
        }

        // Update server JAR name in state
        // This would require modifying the Mutex, so we'll just log it for now
        self.log_info(format!("Forge installed successfully, server JAR should now point to forge-{}-universal.jar", forge_version));

        // Clean up installer
        fs::remove_file(installer_path)?;

        Ok(())
    }

    async fn install_fabric(&self, fabric_version: &str) -> Result<()> {
        let server_dir = PathBuf::from(&self.state.server_directory);
        let installer_path = server_dir.join("fabric-installer.jar");

        self.log_info(format!("Installing Fabric version {}", fabric_version));

        // Download Fabric installer
        let fabric_url = "https://maven.fabricmc.net/net/fabricmc/fabric-installer/latest/fabric-installer.jar";
        self.download_file(fabric_url, &installer_path).await?;

        // Run Fabric installer
        let output = Command::new(&self.state.java_path)
            .arg("-jar")
            .arg(&installer_path)
            .arg("server")
            .arg("-mcversion")
            .arg(fabric_version)
            .current_dir(&server_dir)
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            self.log_error(format!("Fabric installation failed: {}", error));
            return Err(AppError::ProcessError(format!("Fabric installation failed: {}", error)));
        }

        // Update server JAR name in state
        self.log_info("Fabric installed successfully, server JAR should now point to fabric-server-launch.jar");

        // Clean up installer
        fs::remove_file(installer_path)?;

        Ok(())
    }

    async fn download_and_install_modpack(&self, url: &str) -> Result<()> {
        let server_dir = PathBuf::from(&self.state.server_directory);
        let filename = url.split('/').last().unwrap_or("modpack.zip");
        let download_path = server_dir.join(filename);

        self.log_info(format!("Downloading modpack from {}", url));

        // Download modpack
        self.download_file(url, &download_path).await?;

        // Handle different modpack formats
        if filename.ends_with(".zip") {
            self.extract_zip(&download_path, &server_dir)?;
        } else if filename.ends_with(".jar") {
            // Some modpacks are distributed as executable JARs
            let output = Command::new(&self.state.java_path)
                .arg("-jar")
                .arg(&download_path)
                .current_dir(&server_dir)
                .output()?;

            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                self.log_error(format!("Modpack installation failed: {}", error));
                return Err(AppError::ProcessError(format!("Modpack installation failed: {}", error)));
            }
        }

        // Clean up
        fs::remove_file(download_path)?;

        self.log_info("Modpack files extracted successfully");

        Ok(())
    }

    async fn download_file(&self, url: &str, path: &PathBuf) -> Result<()> {
        let client = reqwest::Client::new();
        let response = client.get(url).send().await.map_err(|e| {
            AppError::ProcessError(format!("Failed to download file: {}", e))
        })?;

        if !response.status().is_success() {
            return Err(AppError::ProcessError(format!(
                "Failed to download, status code: {}", response.status()
            )));
        }

        let content = response.bytes().await.map_err(|e| {
            AppError::ProcessError(format!("Failed to read response: {}", e))
        })?;

        let mut file = File::create(path)?;
        file.write_all(&content)?;

        Ok(())
    }

    fn extract_zip(&self, zip_path: &PathBuf, target_dir: &PathBuf) -> Result<()> {
        let file = File::open(zip_path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = match file.enclosed_name() {
                Some(path) => target_dir.join(path),
                None => continue,
            };

            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = File::create(&outpath)?;
                io::copy(&mut file, &mut outfile)?;
            }
        }

        Ok(())
    }

    fn log_info(&self, message: String) {
        let log_entry = LogEntry::info(message, "modpack_installer".to_string());
        if let Some(sender) = events::get_event_sender() {
            let _ = sender.send(events::Event::Log(log_entry));
        }
    }

    fn log_error(&self, message: String) {
        let log_entry = LogEntry::error(message, "modpack_installer".to_string());
        if let Some(sender) = events::get_event_sender() {
            let _ = sender.send(events::Event::Log(log_entry));
        }
    }
}