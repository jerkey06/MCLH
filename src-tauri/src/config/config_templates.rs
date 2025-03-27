use crate::app_state::AppState;
use crate::error::{AppError, Result};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Returns the path to the templates directory within the server directory.
fn get_templates_dir(state: &AppState) -> PathBuf {
    state.server_directory.join("templates")
}

/// Applies a template file, replacing placeholders with provided values.
///
/// Reads a template file (`template_name`) from the `templates` subdirectory,
/// replaces `{{ key }}` placeholders with values from the `replacements` map,
/// and writes the result to `output_path`.
pub fn apply_template(
    template_name: &str,
    replacements: &HashMap<String, String>,
    output_path: &Path,
    state: &Arc<AppState>, // Borrow Arc directly
) -> Result<()> {
    let templates_dir = get_templates_dir(state);
    let template_path = templates_dir.join(template_name);
    info!(
        "Applying template '{}' to output '{}'",
        template_path.display(),
        output_path.display()
    );

    if !template_path.exists() {
        warn!("Template file not found: {}", template_path.display());
        return Err(AppError::ConfigError(format!(
            "Template file not found: {}",
            template_path.display()
        )));
    }

    debug!("Reading template file: {}", template_path.display());
    let template_content = fs::read_to_string(&template_path).map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!("Failed to read template {}: {}", template_path.display(), e),
        ))
    })?;

    let mut result = template_content;
    for (key, value) in replacements {
        let placeholder = format!("{{{{ {} }}}}", key.trim()); // Ensure key is trimmed
        debug!("Replacing '{}' with '{}'", placeholder, value);
        result = result.replace(&placeholder, value);
    }

    // Ensure the output directory exists
    if let Some(parent_dir) = output_path.parent() {
        if !parent_dir.exists() {
            debug!("Creating parent directory: {}", parent_dir.display());
            fs::create_dir_all(parent_dir).map_err(|e| {
                AppError::IoError(io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to create directory {}: {}",
                        parent_dir.display(),
                        e
                    ),
                ))
            })?;
        }
    } else {
        warn!("Output path '{}' has no parent directory.", output_path.display());
        // Optionally return an error here depending on expected usage
    }

    debug!("Writing processed template to: {}", output_path.display());
    let mut file = File::create(output_path).map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!("Failed to create output file {}: {}", output_path.display(), e),
        ))
    })?;
    file.write_all(result.as_bytes()).map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!(
                "Failed to write to output file {}: {}",
                output_path.display(),
                e
            ),
        ))
    })?;

    info!(
        "Successfully applied template '{}' to '{}'",
        template_path.display(),
        output_path.display()
    );
    Ok(())
}

/// Installs default template files into the `templates` subdirectory if they don't exist.
/// Note: These templates are not used by the default `server_properties` or `eula_manager` logic.
pub fn install_default_templates(state: &Arc<AppState>) -> Result<()> {
    let templates_dir = get_templates_dir(state);
    info!(
        "Checking for default templates in: {}",
        templates_dir.display()
    );

    if !templates_dir.exists() {
        debug!(
            "Templates directory not found, creating: {}",
            templates_dir.display()
        );
        fs::create_dir_all(&templates_dir)?;
    }

    // --- Template Content Definitions ---
    // Using include_str! to embed templates directly in the binary is often better
    // for distribution, avoiding the need to manage separate template files.
    // Example: static SERVER_PROPERTIES_TEMPLATE: &str = include_str!("../../../templates/server.properties.tmpl");
    // For now, we keep the hardcoded strings as per your original code.

    let templates_to_install = [
        (
            "server.properties.tmpl",
            r#"# Minecraft server properties template
# Generated on {{ timestamp }}
# Placeholders: {{ port }}, {{ gamemode }}, {{ difficulty }}, etc.
server-port={{ port }}
gamemode={{ gamemode }}
difficulty={{ difficulty }}
level-seed={{ seed }}
enable-command-block={{ command_blocks }}
max-players={{ max_players }}
spawn-protection={{ spawn_protection }}
view-distance={{ view_distance }}
# Add other properties as needed...
"#,
        ),
        (
            "spigot.yml.tmpl",
            r#"# Spigot configuration template
# Example placeholder: {{ minecraft_version }}
settings:
  # ... (rest of your spigot template)
  timeout-time: 60
world-settings:
  default:
    # Example placeholder: {{ view_distance }}
    view-distance: {{ view_distance }}
    # ... (rest of your spigot template)
"#,
        ),
        (
            "bukkit.yml.tmpl",
            r#"# Bukkit configuration template
# (No placeholders shown in your example, add if needed)
settings:
  allow-end: true
  # ... (rest of your bukkit template)
"#,
        ),
    ];

    for (filename, content) in templates_to_install {
        let template_path = templates_dir.join(filename);
        if !template_path.exists() {
            info!("Installing default template: {}", template_path.display());
            let mut file = File::create(&template_path)?;
            file.write_all(content.as_bytes())?;
        } else {
            debug!("Template already exists: {}", template_path.display());
        }
    }

    info!("Default template check complete.");
    Ok(())
}