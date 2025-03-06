use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use crate::app_state::AppState;
use crate::error::Result;

pub fn accept_eula(state: Arc<AppState>) -> Result<()> {
    let server_dir = &state.server_directory;
    let eula_path = Path::new(server_dir).join("eula.txt");

    let mut file = File::create(&eula_path)?;

    writeln!(file, "#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://account.mojang.com/documents/minecraft_eula).")?;
    writeln!(file, "#{}", chrono::Local::now().to_rfc3339())?;
    writeln!(file, "eula=true")?;

    Ok(())
}

pub fn is_eula_accepted(state: Arc<AppState>) -> Result<bool> {
    let server_dir = &state.server_directory;
    let eula_path = Path::new(server_dir).join("eula.txt");

    if !eula_path.exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(&eula_path)?;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("eula=") {
            let value = line.split('=').nth(1).unwrap_or("false");
            return Ok(value.trim() == "true");
        }
    }

    Ok(false)
}