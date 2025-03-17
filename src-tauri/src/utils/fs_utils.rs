use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write, Result};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use chrono::Local;

use crate::error::{AppError, Result as AppResult};

/// Creates a directory if it doesn't exist
pub fn ensure_directory(path: &Path) -> AppResult<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Reads the entire contents of a file into a string
pub fn read_file_to_string(path: &Path) -> AppResult<String> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

/// Writes a string to a file, creating the file if it doesn't exist
pub fn write_string_to_file(path: &Path, content: &str) -> AppResult<()> {
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

/// Appends a string to a file, creating the file if it doesn't exist
pub fn append_string_to_file(path: &Path, content: &str) -> AppResult<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

/// Copies a file from source to destination
pub fn copy_file(source: &Path, destination: &Path) -> AppResult<()> {
    fs::copy(source, destination)?;
    Ok(())
}

/// Moves a file from source to destination
pub fn move_file(source: &Path, destination: &Path) -> AppResult<()> {
    fs::rename(source, destination)?;
    Ok(())
}

/// Removes a file
pub fn remove_file(path: &Path) -> AppResult<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Removes a directory and all its contents
pub fn remove_directory(path: &Path) -> AppResult<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// Creates a backup of a file with a timestamp
pub fn backup_file(path: &Path) -> AppResult<PathBuf> {
    if !path.exists() {
        return Err(AppError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "File not found",
        )));
    }

    let timestamp = Local::now().format("%Y%m%d%H%M%S");
    let file_name = path.file_name().unwrap().to_str().unwrap();
    let backup_name = format!("{}.backup_{}", file_name, timestamp);
    let backup_path = path.with_file_name(backup_name);

    fs::copy(path, &backup_path)?;
    Ok(backup_path)
}

/// Lists all files in a directory with a specific extension
pub fn list_files_with_extension(dir: &Path, extension: &str) -> AppResult<Vec<PathBuf>> {
    let mut result = Vec::new();

    if !dir.exists() {
        return Ok(result);
    }

    for entry in WalkDir::new(dir).min_depth(1).max_depth(1) {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().unwrap_or_default() == extension {
            result.push(path.to_path_buf());
        }
    }

    Ok(result)
}

/// Checks if a file exists and is readable
pub fn is_file_readable(path: &Path) -> bool {
    if !path.exists() || !path.is_file() {
        return false;
    }

    match File::open(path) {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Checks if a file exists and is writable
pub fn is_file_writable(path: &Path) -> bool {
    if path.exists() && path.is_file() {
        match OpenOptions::new().write(true).open(path) {
            Ok(_) => true,
            Err(_) => false,
        }
    } else {
        match File::create(path) {
            Ok(_) => {
                let _ = fs::remove_file(path);
                true
            }
            Err(_) => false,
        }
    }
}

/// Ensures a directory exists and is writable
pub fn ensure_writable_directory(path: &Path) -> AppResult<()> {
    ensure_directory(path)?;

    let test_file = path.join(".write_test");
    if !is_file_writable(&test_file) {
        return Err(AppError::IoError(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Directory is not writable",
        )));
    }

    Ok(())
}