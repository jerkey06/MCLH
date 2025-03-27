use crate::error::{AppError, Result as AppResult};
use log::{debug, info, trace, warn};
use regex::Regex;
use std::env::{consts, var_os}; // Use var_os for better env var handling
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio}; // Need Stdio for stderr capture
use which::which;

// Lazy static compilation for the version regex
lazy_static::lazy_static! {
    // Regex to capture Java version numbers like "1.8.0_291", "11.0.11", "17"
    // Groups: 1: Major (e.g., 1 or 11 or 17), 2: Minor (Optional), 3: Patch (Optional), 4: Build/Update (Optional)
    static ref JAVA_VERSION_REGEX: Regex = Regex::new(r#"version "([1-9]\d*)(?:(?:\.(\d+))?(?:\.(\d+))?)?(?:_(\d+))?(?:[^\"]*)?""#).unwrap();
    // Simpler alternative if only major matters or pre-Java 9 format isn't needed:
    // static ref JAVA_VERSION_REGEX: Regex = Regex::new(r#"version "([1-9]\d*)(?:\.[^"]*)?""#).unwrap();
}

/// Finds a suitable Java executable path.
/// Checks JAVA_HOME, then PATH, then common locations.
/// Returns `AppResult<PathBuf>`.
pub fn find_java_path() -> AppResult<PathBuf> {
    info!("Attempting to find Java executable...");

    // 1. Check JAVA_HOME environment variable
    if let Some(java_home_osstr) = var_os("JAVA_HOME") {
        let java_home = PathBuf::from(java_home_osstr);
        debug!("Checking JAVA_HOME: {}", java_home.display());
        let java_exec = get_java_executable_from_home(&java_home);
        if is_valid_java(&java_exec) {
            info!("Found valid Java in JAVA_HOME: {}", java_exec.display());
            return Ok(java_exec);
        } else {
            warn!(
                "JAVA_HOME found, but {} does not appear to be valid Java.",
                java_exec.display()
            );
        }
    } else {
        trace!("JAVA_HOME environment variable not set.");
    }

    // 2. Check system PATH using 'which' crate
    trace!("Checking system PATH for 'java'...");
    match which("java") {
        Ok(java_path) => {
            debug!("Found 'java' in PATH: {}", java_path.display());
            if is_valid_java(&java_path) {
                info!("Found valid Java in PATH: {}", java_path.display());
                return Ok(java_path);
            } else {
                warn!(
                    "Found 'java' in PATH, but {} does not appear to be valid.",
                    java_path.display()
                );
            }
        }
        Err(_) => {
            trace!("'java' not found in system PATH.");
        }
    }

    // 3. Check common installation locations (OS-specific)
    info!("Checking common Java installation locations...");
    let common_locations = get_common_java_locations();
    for location_str in common_locations {
        let location = PathBuf::from(location_str);
        trace!("Checking location: {}", location.display());
        if location.exists() && location.is_dir() {
            let java_exec = get_java_executable_from_home(&location);
            if java_exec.exists() && is_valid_java(&java_exec) {
                info!(
                    "Found valid Java in common location: {}",
                    java_exec.display()
                );
                return Ok(java_exec);
            }
        } else {
            trace!("Location does not exist or is not a directory.");
        }
    }

    error!("Java executable could not be found via JAVA_HOME, PATH, or common locations.");
    Err(AppError::JavaNotFound)
}

/// Checks if the executable at the given path is a valid Java installation
/// by running `java -version` and checking the exit code.
fn is_valid_java(java_path: &Path) -> bool {
    trace!("Validating Java path: {}", java_path.display());
    match Command::new(java_path)
        .arg("-version")
        .stdout(Stdio::null()) // Ignore stdout
        .stderr(Stdio::piped()) // Capture stderr (where version info is printed)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                trace!(
                    "Validation successful for {}",
                    java_path.display()
                );
                true
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!(
                    "Validation failed for {}. Exit code: {:?}, Stderr: {}",
                    java_path.display(),
                    output.status.code(),
                    stderr.trim()
                );
                false
            }
        }
        Err(e) => {
            warn!(
                "Error executing Java validation command for {}: {}",
                java_path.display(),
                e
            );
            false
        }
    }
}

/// Constructs the expected path to the `java` or `java.exe` executable
/// within a given Java home directory.
fn get_java_executable_from_home(java_home: &Path) -> PathBuf {
    let mut path = java_home.to_path_buf();
    path.push("bin");
    let exec_name = if consts::OS == "windows" { "java.exe" } else { "java" };
    path.push(exec_name);
    path
}

/// Provides a list of common directory paths where Java might be installed.
/// This is OS-specific and not exhaustive.
fn get_common_java_locations() -> Vec<String> {
    // Keep the existing lists, they are reasonable fallbacks.
    // Convert them to use PathBuf internally maybe, but String is fine for this list.
    trace!("Getting common Java locations for OS: {}", consts::OS);
    match consts::OS {
        "windows" => vec![
            // Prioritize newer versions? Or let user choose later?
            r"C:\Program Files\Eclipse Adoptium\jdk-17".to_string(),
            r"C:\Program Files\Eclipse Adoptium\jdk-11".to_string(),
            r"C:\Program Files\Eclipse Adoptium\jdk-8".to_string(),
            r"C:\Program Files\Java\jdk-17".to_string(),
            r"C:\Program Files\Java\jdk-11".to_string(),
            r"C:\Program Files\Java\jdk-8".to_string(),
            // Add other vendors/paths if common (Amazon Corretto, Zulu, etc.)
        ],
        "macos" => vec![
            // Use `java_home -V` output format if possible?
            "/Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/temurin-11.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/temurin-8.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/adoptopenjdk-17.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/adoptopenjdk-11.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/adoptopenjdk-8.jdk/Contents/Home".to_string(),
            // System Java (might be older)
            "/usr/bin/java".to_string(), // This isn't a JAVA_HOME, handle separately?
            // Need to use /usr/libexec/java_home maybe?
        ],
        _ => vec![ // Linux/Other Unix
                   // Common distribution paths
                   "/usr/lib/jvm/java-17-openjdk".to_string(),
                   "/usr/lib/jvm/java-11-openjdk".to_string(),
                   "/usr/lib/jvm/java-8-openjdk".to_string(),
                   "/usr/lib/jvm/default-java".to_string(), // Symlink often present
                   // Add paths for other distributions/vendors
                   "/opt/java/jdk-17".to_string(), // Manual installs
                   "/opt/java/jdk-11".to_string(),
                   "/opt/java/jdk-8".to_string(),
        ],
    }
}

/// Attempts to parse the Java version (major, minor, patch, build) from `java -version` output.
pub fn get_java_version(java_path: &Path) -> AppResult<(u32, u32, u32, u32)> {
    trace!("Getting Java version for: {}", java_path.display());
    let output = Command::new(java_path)
        .arg("-version")
        .stderr(Stdio::piped()) // Version info is on stderr
        .stdout(Stdio::null())
        .output()
        .map_err(|e| {
            AppError::ProcessError(format!(
                "Failed to execute '{} -version': {}",
                java_path.display(),
                e
            ))
        })?;

    if !output.status.success() {
        return Err(AppError::ProcessError(format!(
            "'{} -version' failed with status: {:?}",
            java_path.display(),
            output.status.code()
        )));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    trace!("Java version output (stderr): {}", stderr.trim());

    // Use the precompiled regex
    if let Some(captures) = JAVA_VERSION_REGEX.captures(&stderr) {
        // Group 1 is mandatory (major version)
        let major = captures.get(1)
            .map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));

        // Java 9+ format often just has Major.Minor.Patch (e.g., 11.0.1)
        // Java 8 format is 1.8.0_BUILD (e.g., 1.8.0_291)
        let minor: u32;
        let patch: u32;
        let build: u32; // Or update number for Java 8 style

        if major == 1 {
            // Handle 1.x format (like Java 8)
            minor = captures.get(2).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));
            patch = captures.get(3).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));
            build = captures.get(4).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0)); // Group 4 is build/update
        } else {
            // Handle Java 9+ format (Major.Minor.Patch)
            minor = captures.get(2).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));
            patch = captures.get(3).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));
            build = captures.get(4).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0)); // Build might still exist
            // Note: Regex needs adjustment if build number isn't prefixed by '_' for Java 9+
        }


        // Treat Java 1.8 as major version 8 for simplicity
        let effective_major = if major == 1 { minor } else { major };

        info!(
            "Detected Java version: {}.{}.{}_{} (Effective Major: {})",
            major, minor, patch, build, effective_major
        );
        Ok((effective_major, minor, patch, build)) // Return effective major

    } else {
        warn!("Could not parse Java version from output: {}", stderr.trim());
        Err(AppError::ProcessError(
            "Could not determine Java version from 'java -version' output.".to_string(),
        ))
    }
}

/// Checks if the detected Java version (Major) is potentially compatible.
/// Needs refinement based on actual Minecraft version requirements.
pub fn is_java_compatible(java_path: &Path) -> AppResult<bool> {
    match get_java_version(java_path) {
        Ok((major, _, _, _)) => {
            // Minecraft 1.17+ requires Java 16/17+
            // Minecraft 1.12-1.16.5 generally needs Java 8 (but newer might work)
            // This is a very basic check.
            let compatible = major >= 8; // Minimum requirement for older versions
            info!("Java major version {} is{} compatible (basic check).", major, if compatible {""} else {" NOT"});
            Ok(compatible)
            // TODO: Add Minecraft version specific checks, e.g.
            // if mc_version >= "1.17" { Ok(major >= 16) } else { Ok(major >= 8) }
        }
        Err(e) => {
            error!("Could not determine Java version for compatibility check: {}", e);
            Err(e) // Propagate the error
        }
    }
}

/// Placeholder: Finds the most suitable Java version for a specific Minecraft version.
/// This requires knowing Minecraft version requirements and potentially checking multiple Java installs.
/// Returns the path found by `find_java_path` for now.
pub fn find_suitable_java(minecraft_version: &str) -> AppResult<PathBuf> {
    warn!("find_suitable_java is a placeholder. Selecting first valid Java found.");
    // TODO: Implement logic:
    // 1. Get list of *all* valid Java installations found (modify find_java_path logic).
    // 2. Get version for each installation using get_java_version.
    // 3. Determine required Java version range based on minecraft_version string (e.g., "1.18.2", "1.16.5").
    // 4. Select the best match from the available installations (e.g., highest compatible version).
    // 5. Return the PathBuf for the selected installation.

    // For now, just return the first valid one found by find_java_path:
    find_java_path()
}