use std::process::Command;
use std::path::{Path, PathBuf};
use std::env::{var, consts};
use which::which;

use crate::error::{AppError, Result};

/// Finds the Java executable path based on the system configuration
pub fn find_java_path() -> Result<String> {
    // Check if Java path is set in environment variables
    if let Ok(java_home) = var("JAVA_HOME") {
        let java_exec = get_java_executable(&java_home);
        if is_valid_java(&java_exec) {
            return Ok(java_exec);
        }
    }

    // Try to find Java in PATH
    if let Ok(java_path) = which("java") {
        if is_valid_java(java_path.to_str().unwrap()) {
            return Ok(java_path.to_str().unwrap().to_string());
        }
    }

    // Search in common locations based on OS
    let common_locations = get_common_java_locations();
    for location in common_locations {
        let java_exec = get_java_executable(&location);
        if is_valid_java(&java_exec) {
            return Ok(java_exec);
        }
    }

    Err(AppError::JavaNotFound)
}

/// Checks if Java is valid by executing a version check
fn is_valid_java(java_path: &str) -> bool {
    let output = Command::new(java_path)
        .arg("-version")
        .output();

    match output {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Gets the path to the Java executable based on the Java home directory
fn get_java_executable(java_home: &str) -> String {
    let mut path = PathBuf::from(java_home);
    path.push("bin");

    if consts::OS == "windows" {
        path.push("java.exe");
    } else {
        path.push("java");
    }

    path.to_str().unwrap().to_string()
}

/// Gets common Java installation locations based on the OS
fn get_common_java_locations() -> Vec<String> {
    match consts::OS {
        "windows" => vec![
            r"C:\Program Files\Java\jdk-11".to_string(),
            r"C:\Program Files\Java\jdk-17".to_string(),
            r"C:\Program Files\Java\jdk-8".to_string(),
            r"C:\Program Files\Java\jdk1.8.0".to_string(),
            r"C:\Program Files\Java\jre-11".to_string(),
            r"C:\Program Files\Java\jre-17".to_string(),
            r"C:\Program Files\Java\jre-8".to_string(),
            r"C:\Program Files\Java\jre1.8.0".to_string(),
            r"C:\Program Files\AdoptOpenJDK\jdk-11".to_string(),
            r"C:\Program Files\AdoptOpenJDK\jdk-17".to_string(),
            r"C:\Program Files\AdoptOpenJDK\jdk-8".to_string(),
            r"C:\Program Files\Eclipse Adoptium\jdk-11".to_string(),
            r"C:\Program Files\Eclipse Adoptium\jdk-17".to_string(),
            r"C:\Program Files\Eclipse Adoptium\jdk-8".to_string(),
        ],
        "macos" => vec![
            "/Library/Java/JavaVirtualMachines/jdk-11.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/jdk-17.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/jdk-8.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/adoptopenjdk-11.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/adoptopenjdk-17.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/adoptopenjdk-8.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/temurin-11.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home".to_string(),
            "/Library/Java/JavaVirtualMachines/temurin-8.jdk/Contents/Home".to_string(),
        ],
        _ => vec![
            "/usr/lib/jvm/java-11-openjdk".to_string(),
            "/usr/lib/jvm/java-17-openjdk".to_string(),
            "/usr/lib/jvm/java-8-openjdk".to_string(),
            "/usr/lib/jvm/java-11-oracle".to_string(),
            "/usr/lib/jvm/java-17-oracle".to_string(),
            "/usr/lib/jvm/java-8-oracle".to_string(),
            "/usr/lib/jvm/adoptopenjdk-11".to_string(),
            "/usr/lib/jvm/adoptopenjdk-17".to_string(),
            "/usr/lib/jvm/adoptopenjdk-8".to_string(),
            "/usr/lib/jvm/temurin-11".to_string(),
            "/usr/lib/jvm/temurin-17".to_string(),
            "/usr/lib/jvm/temurin-8".to_string(),
        ],
    }
}

/// Returns the Java version as a tuple of (major, minor, patch)
pub fn get_java_version(java_path: &str) -> Result<(u32, u32, u32)> {
    let output = Command::new(java_path)
        .arg("-version")
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Extract version from output
    let version_regex = regex::Regex::new(r"version \"(\d+)(?:\.(\d+))?(?:\.(\d+))?(_\d+)?(?:-.+)?\"").unwrap();

    if let Some(captures) = version_regex.captures(&stderr) {
        let major = captures.get(1).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));
        let minor = captures.get(2).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));
        let patch = captures.get(3).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));

        Ok((major, minor, patch))
    } else {
        Err(AppError::ProcessError("Could not determine Java version".to_string()))
    }
}

/// Checks if the Java version is compatible with Minecraft
pub fn is_java_compatible(java_path: &str) -> Result<bool> {
    let (major, _, _) = get_java_version(java_path)?;

    // Minecraft 1.17+ requires Java 16+
    // Minecraft 1.12-1.16.5 works best with Java 8
    // This check is simplified; you might want to add more specific checks
    Ok(major >= 8)
}

/// Finds the most suitable Java version for a specific Minecraft version
pub fn find_suitable_java(minecraft_version: &str) -> Result<String> {
    let java_path = find_java_path()?;