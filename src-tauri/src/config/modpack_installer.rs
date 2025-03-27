// src/config/modpack_installer.rs

use crate::api::events::{emit_error, emit_info, emit_progress};
use crate::app_state::AppState;
use crate::error::{AppError, Result};
use log::{debug, error, info};
use std::fs::{self, File};
use std::io::{self, Cursor}; // Use io::Cursor for in-memory zip reading
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Installs a modpack from a given URL.
///
/// Downloads the zip file, validates it, clears the server directory (optional),
/// extracts the contents, and potentially performs post-install actions.
/// Emits `ProgressUpdate` events during download and extraction.
pub fn install(state: Arc<AppState>, url: &str) -> Result<()> {
    info!("Starting modpack installation from URL: {}", url);

    // --- 1. Define Download Path ---
    // Use a temporary directory or a dedicated downloads folder within AppData
    let temp_dir = state.server_directory.join(".temp_download");
    if !temp_dir.exists() {
        fs::create_dir_all(&temp_dir)?;
    }
    let filename = url.split('/').last().unwrap_or("modpack.zip"); // Basic filename extraction
    let download_path = temp_dir.join(filename);
    info!("Downloading to: {}", download_path.display());

    // --- 2. Download the Modpack ---
    emit_progress("Download", 0.0, "Starting download...");
    // Use reqwest for downloading. Needs to be run in an async context
    // Since this function is called via spawn_blocking, we need to setup a local runtime
    // or preferably restructure the command handling in rest.rs to await this directly.
    // For now, using blocking reqwest as a simpler (but less ideal) example.
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300)) // 5 min timeout
        .build()
        .map_err(|e| AppError::ModpackError(format!("Failed to create HTTP client: {}", e)))?;

    let response = client.get(url).send().map_err(|e| {
        AppError::ModpackError(format!("Failed to send download request to {}: {}", url, e))
    })?;

    if !response.status().is_success() {
        return Err(AppError::ModpackError(format!(
            "Download failed: Server returned status {}",
            response.status()
        )));
    }

    let total_size = response
        .content_length()
        .unwrap_or(0); // Get expected size for progress

    let mut downloaded_bytes: u64 = 0;
    let mut download_dest = File::create(&download_path)?;
    let mut stream = response; // reqwest::blocking::Response is a reader

    let mut buffer = [0; 8192]; // 8KB buffer
    loop {
        let bytes_read = stream.read(&mut buffer).map_err(|e| AppError::ModpackError(format!("Error during download: {}", e)))?;
        if bytes_read == 0 {
            break; // Download complete
        }
        download_dest.write_all(&buffer[..bytes_read])?;
        downloaded_bytes += bytes_read as u64;

        if total_size > 0 {
            let progress = (downloaded_bytes as f32 / total_size as f32) * 100.0;
            emit_progress(
                "Download",
                progress,
                &format!("Downloading... {:.1}%", progress),
            );
        } else {
            // Unknown total size, just show bytes downloaded
            emit_progress(
                "Download",
                -1.0, // Indicate indeterminate progress
                &format!("Downloading... {} bytes", downloaded_bytes),
            );
        }
    }
    download_dest.flush()?; // Ensure buffer is written
    info!("Download complete: {} bytes", downloaded_bytes);
    emit_progress("Download", 100.0, "Download complete.");

    // --- 3. Clear Server Directory (Optional but Recommended) ---
    // Decide which files/folders to keep (e.g., maybe keep world data?, backups?)
    info!("Clearing server directory before extraction (WARNING: DELETES FILES)...");
    // Example: Simple clear - THIS IS DESTRUCTIVE! Add more sophisticated logic later.
    clear_server_directory(&state.server_directory, &temp_dir)?; // Pass temp_dir to avoid deleting it
    emit_progress("Setup", 0.0, "Preparing server directory...");


    // --- 4. Extract the Modpack ---
    info!("Starting extraction of {}...", download_path.display());
    emit_progress("Extract", 0.0, "Starting extraction...");
    extract_zip(&download_path, &state.server_directory)?; // Pass server dir as target
    emit_progress("Extract", 100.0, "Extraction complete.");

    // --- 5. Post-Installation Steps ---
    // - Run Forge/Fabric installer if needed? (More complex)
    // - Ensure correct server JAR is selected in AppState?
    // - Apply default configs?
    info!("Running post-installation steps...");
    emit_progress("Setup", 50.0, "Running post-install tasks...");
    // Example: ensure default properties exist if server.properties wasn't in the pack
    create_default_properties_if_missing(&state)?;
    // Example: ensure EULA is prompted again
    // You might want to *check* if eula.txt was in the zip and respect it? Or always force re-accept?
    // Forcing re-accept is safer:
    // fs::remove_file(state.server_directory.join("eula.txt")).ok(); // Ignore error if not present
    emit_eula_status(false); // Assume EULA needs re-accepting


    emit_progress("Setup", 100.0, "Installation complete.");
    info!("Modpack installation finished successfully.");

    // --- 6. Cleanup ---
    debug!("Cleaning up temporary files...");
    fs::remove_dir_all(&temp_dir)?; // Remove the .temp_download directory

    Ok(())
}


/// Helper function to clear the server directory before extraction.
/// BE VERY CAREFUL with this function. It deletes files!
fn clear_server_directory(server_dir: &Path, exclude_dir: &Path) -> Result<()> {
    info!("Clearing contents of {}", server_dir.display());
    for entry_result in fs::read_dir(server_dir)? {
        let entry = entry_result?;
        let path = entry.path();

        // IMPORTANT: Do not delete the directory we downloaded to!
        if path == exclude_dir {
            debug!("Skipping deletion of excluded directory: {}", path.display());
            continue;
        }

        // Add more exclusions? e.g., keep world saves, backups, specific configs?
        // Example: if path.file_name().map_or(false, |n| n == "world" || n == "backups") { continue; }

        debug!("Removing: {}", path.display());
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}


/// Helper function to extract a zip archive.
fn extract_zip(zip_path: &Path, target_dir: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::ModpackError(format!("Failed to open zip archive: {}", e)))?;

    let total_files = archive.len();
    info!("Extracting {} files to {}...", total_files, target_dir.display());

    for i in 0..total_files {
        let mut file = archive.by_index(i)
            .map_err(|e| AppError::ModpackError(format!("Failed read file index {} from zip: {}", i, e)))?;

        // Sanitize file path: prevent path traversal (../../..)
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => {
                warn!("Skipping potentially unsafe file path in zip: {}", file.name());
                continue;
            }
        };

        let file_name = file.name().to_string(); // Get name for logging before borrow ends

        if (*file.name()).ends_with('/') {
            // It's a directory
            debug!("Creating directory: {}", outpath.display());
            fs::create_dir_all(&outpath)?;
        } else {
            // It's a file
            debug!("Extracting file: {} ({} bytes)", outpath.display(), file.size());
            // Ensure parent directory exists for the file
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
            // Create the file and copy contents
            let mut outfile = File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }

        // Optional: Update progress more granularly during extraction
        let progress = ((i + 1) as f32 / total_files as f32) * 100.0;
        if i % 50 == 0 || i == total_files - 1 { // Update every 50 files or on the last file
            emit_progress("Extract", progress, &format!("Extracting: {}", file_name));
        }

        // Get and Set permissions in Unix-like systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
            }
        }
    }
    info!("Extraction complete.");
    Ok(())
}