use crate::app_state::AppState;
use crate::error::{AppError, Result};
use log::{debug, error, info, warn};
use std::fs::{self, File};
use std::io::{self, Write}; // Import io for better error wrapping
use std::path::PathBuf;
use std::sync::Arc;

/// Returns the full path to the eula.txt file.
fn get_eula_path(state: &AppState) -> PathBuf {
    state.server_directory.join("eula.txt")
}

/// Accepts the Minecraft EULA by writing `eula=true` to `eula.txt`.
/// Creates the file if it doesn't exist.
pub fn accept_eula(state: &Arc<AppState>) -> Result<()> {
    let eula_path = get_eula_path(state);
    info!("Attempting to accept EULA at: {}", eula_path.display());

    // Use try_exists for better error handling if permissions are an issue
    match eula_path.try_exists() {
        Ok(true) => debug!("eula.txt exists, will overwrite."),
        Ok(false) => debug!("eula.txt does not exist, will create."),
        Err(e) => {
            warn!("Could not check existence of {}: {}", eula_path.display(), e);
            // Continue anyway, File::create will likely fail if permissions are bad
        }
    }

    let mut file = File::create(&eula_path).map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!("Failed to create/open {}: {}", eula_path.display(), e),
        ))
    })?;

    // Use chrono from dependencies
    let timestamp = chrono::Local::now().to_rfc3339();

    writeln!(file, "# By changing the setting below to TRUE you are indicating your agreement to our EULA (https://account.mojang.com/documents/minecraft_eula).")?;
    writeln!(file, "# {}", timestamp)?; // Add timestamp comment
    writeln!(file, "eula=true")?; // The crucial line

    file.flush()?; // Ensure data is written to disk

    info!("Successfully wrote eula=true to {}", eula_path.display());
    Ok(())
}

/// Checks if the EULA has been accepted by reading `eula.txt`.
/// Returns `Ok(true)` if `eula=true` is found, `Ok(false)` otherwise (including if file doesn't exist).
pub fn is_eula_accepted(state: &Arc<AppState>) -> Result<bool> {
    let eula_path = get_eula_path(state);
    debug!("Checking EULA status at: {}", eula_path.display());

    match eula_path.try_exists() {
        Ok(true) => {
            // File exists, read and parse it
            let content = fs::read_to_string(&eula_path).map_err(|e| {
                AppError::IoError(io::Error::new(e.kind(), format!("Failed to read {}: {}", eula_path.display(), e)))
            })?;

            for line in content.lines() {
                let line_trimmed = line.trim();
                // Check for the specific line, case-insensitively for robustness? Maybe not needed.
                if line_trimmed.starts_with("eula=") {
                    let value = line_trimmed.splitn(2, '=').nth(1).unwrap_or("").trim();
                    let accepted = value.eq_ignore_ascii_case("true");
                    debug!("Found eula line: '{}', value: '{}', accepted: {}", line_trimmed, value, accepted);
                    return Ok(accepted);
                }
            }
            // eula= line not found in the file
            warn!("eula.txt exists but 'eula=' line was not found.");
            Ok(false)
        }
        Ok(false) => {
            // File does not exist
            debug!("eula.txt not found.");
            Ok(false)
        }
        Err(e) => {
            error!("Failed to check existence of {}: {}", eula_path.display(), e);
            Err(AppError::IoError(io::Error::new(e.kind(), format!("Failed to check existence of {}: {}", eula_path.display(), e))))
        }
    }
}