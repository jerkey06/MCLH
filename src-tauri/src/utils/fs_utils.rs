// src/utils/fs_utils.rs

use crate::error::{AppError, Result as AppResult};
use log::{debug, error, trace, warn};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write}; // Import io module
use std::path::{Path, PathBuf};
use std::ffi::OsStr; // For safer path handling
use walkdir::WalkDir;

/// Creates a directory and all its parent components if they are missing.
/// Idempotent: Does nothing if the directory already exists.
pub fn ensure_directory(path: &Path) -> AppResult<()> {
    if !path.exists() {
        debug!("Creating directory: {}", path.display());
        fs::create_dir_all(path).map_err(|e| {
            AppError::IoError(io::Error::new(
                e.kind(),
                format!("Failed to create directory {}: {}", path.display(), e),
            ))
        })?;
    } else if !path.is_dir() {
        error!(
            "Path exists but is not a directory: {}",
            path.display()
        );
        return Err(AppError::IoError(io::Error::new(
            io::ErrorKind::AlreadyExists, // Or InvalidInput?
            format!("Path exists but is not a directory: {}", path.display()),
        )));
    } else {
        trace!("Directory already exists: {}", path.display());
    }
    Ok(())
}

/// Reads the entire contents of a file into a string.
pub fn read_file_to_string(path: &Path) -> AppResult<String> {
    trace!("Reading file to string: {}", path.display());
    fs::read_to_string(path).map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!("Failed to read file {}: {}", path.display(), e),
        ))
    })
}

/// Writes a string slice to a file, creating it if it doesn't exist, truncating if it does.
pub fn write_string_to_file(path: &Path, content: &str) -> AppResult<()> {
    trace!("Writing string to file: {}", path.display());
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        ensure_directory(parent)?;
    }

    let mut file = File::create(path).map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!("Failed to create/truncate file {}: {}", path.display(), e),
        ))
    })?;
    file.write_all(content.as_bytes()).map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!("Failed to write to file {}: {}", path.display(), e),
        ))
    })?;
    file.flush().map_err(|e| { // Ensure contents are flushed
        AppError::IoError(io::Error::new(
            e.kind(),
            format!("Failed to flush file {}: {}", path.display(), e),
        ))
    })?;
    Ok(())
}

/// Appends a string slice to a file, creating the file if it doesn't exist.
pub fn append_string_to_file(path: &Path, content: &str) -> AppResult<()> {
    trace!("Appending string to file: {}", path.display());
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        ensure_directory(parent)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| {
            AppError::IoError(io::Error::new(
                e.kind(),
                format!("Failed to open file {} for appending: {}", path.display(), e),
            ))
        })?;
    file.write_all(content.as_bytes()).map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!("Failed to append to file {}: {}", path.display(), e),
        ))
    })?;
    file.flush().map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!("Failed to flush file {} after append: {}", path.display(), e),
        ))
    })?;
    Ok(())
}

/// Copies a file from source to destination. Fails if destination exists.
/// Consider using `copy_file_overwrite` for overwriting behavior.
pub fn copy_file(source: &Path, destination: &Path) -> AppResult<u64> {
    trace!("Copying file from {} to {}", source.display(), destination.display());
    // Ensure destination parent directory exists
    if let Some(parent) = destination.parent() {
        ensure_directory(parent)?;
    }
    fs::copy(source, destination).map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!(
                "Failed to copy file from {} to {}: {}",
                source.display(),
                destination.display(),
                e
            ),
        ))
    })
}

// Optional: Add an overwriting version if needed
// pub fn copy_file_overwrite(source: &Path, destination: &Path) -> AppResult<u64> { ... }


/// Moves (renames) a file from source to destination.
/// Behavior on existing destination might be platform-specific (often overwrites).
pub fn move_file(source: &Path, destination: &Path) -> AppResult<()> {
    trace!("Moving file from {} to {}", source.display(), destination.display());
    // Ensure destination parent directory exists
    if let Some(parent) = destination.parent() {
        ensure_directory(parent)?;
    }
    fs::rename(source, destination).map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!(
                "Failed to move file from {} to {}: {}",
                source.display(),
                destination.display(),
                e
            ),
        ))
    })
}

/// Removes a file if it exists. Does nothing if it doesn't exist.
pub fn remove_file(path: &Path) -> AppResult<()> {
    if path.exists() {
        trace!("Removing file: {}", path.display());
        fs::remove_file(path).map_err(|e| {
            AppError::IoError(io::Error::new(
                e.kind(),
                format!("Failed to remove file {}: {}", path.display(), e),
            ))
        })?;
    } else {
        trace!("File not found, nothing to remove: {}", path.display());
    }
    Ok(())
}

/// Removes a directory and all its contents recursively if it exists.
/// Does nothing if it doesn't exist. Use with caution!
pub fn remove_directory(path: &Path) -> AppResult<()> {
    if path.exists() {
        if path.is_dir() { // Extra check to avoid removing files accidentally
            debug!("Recursively removing directory: {}", path.display());
            fs::remove_dir_all(path).map_err(|e| {
                AppError::IoError(io::Error::new(
                    e.kind(),
                    format!("Failed to remove directory {}: {}", path.display(), e),
                ))
            })?;
        } else {
            error!("Attempted to remove directory, but path is not a directory: {}", path.display());
            return Err(AppError::IoError(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Path is not a directory: {}", path.display()),
            )));
        }
    } else {
        trace!("Directory not found, nothing to remove: {}", path.display());
    }
    Ok(())
}

/// Creates a backup copy of a file with a timestamp appended to its name.
/// Returns the path to the created backup file.
pub fn backup_file(path: &Path) -> AppResult<PathBuf> {
    trace!("Creating backup for file: {}", path.display());
    if !path.exists() {
        warn!("Cannot backup file - not found: {}", path.display());
        return Err(AppError::IoError(io::Error::new(
            io::ErrorKind::NotFound,
            format!("File not found for backup: {}", path.display()),
        )));
    }
    if !path.is_file() {
        warn!("Cannot backup path - not a file: {}", path.display());
        return Err(AppError::IoError(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Path is not a file: {}", path.display()),
        )));
    }

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let original_filename = path.file_name().unwrap_or_else(|| OsStr::new("file")); // Handle potential missing filename
    let mut backup_filename = original_filename.to_os_string();
    backup_filename.push(format!(".backup_{}", timestamp));

    let backup_path = path.with_file_name(backup_filename);

    debug!("Backup destination: {}", backup_path.display());
    fs::copy(path, &backup_path).map_err(|e| {
        AppError::IoError(io::Error::new(
            e.kind(),
            format!(
                "Failed to copy backup from {} to {}: {}",
                path.display(),
                backup_path.display(),
                e
            ),
        ))
    })?;

    info!(
        "Successfully created backup: {}",
        backup_path.display()
    );
    Ok(backup_path)
}

/// Lists all files (non-recursively) in a directory matching a specific extension.
/// Case-insensitive comparison for the extension.
pub fn list_files_with_extension(dir: &Path, extension: &str) -> AppResult<Vec<PathBuf>> {
    trace!("Listing files with extension '{}' in directory: {}", extension, dir.display());
    let mut result = Vec::new();

    if !dir.exists() {
        warn!("Directory not found for listing files: {}", dir.display());
        return Ok(result); // Return empty list if dir doesn't exist
    }
    if !dir.is_dir() {
        error!("Path is not a directory: {}", dir.display());
        return Err(AppError::IoError(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Path is not a directory: {}", dir.display()),
        )));
    }

    // Use fs::read_dir for non-recursive listing
    for entry_result in fs::read_dir(dir)? {
        let entry = entry_result?;
        let path = entry.path();

        // Check if it's a file and the extension matches (case-insensitive)
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(OsStr::to_str) {
                if ext.eq_ignore_ascii_case(extension) {
                    trace!("Found matching file: {}", path.display());
                    result.push(path);
                }
            }
        }
    }

    debug!("Found {} files with extension '{}' in {}", result.len(), extension, dir.display());
    Ok(result)
}

/// Checks if a file exists and is readable by the current user.
pub fn is_file_readable(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_file() {
                // Basic check: can we open it for reading?
                // This doesn't guarantee read permission in all edge cases (ACLs etc.)
                // but is usually sufficient.
                File::open(path).is_ok()
            } else {
                false // It's a directory or other type
            }
        },
        Err(_) => false, // Doesn't exist or error accessing metadata
    }
}

/// Checks if a file is writable by the current user.
/// If the file exists, tries to open it for writing.
/// If it doesn't exist, tries to create it, then removes it.
pub fn is_file_writable(path: &Path) -> bool {
    // Check parent directory first - fail fast if we can't even write *there*
    if let Some(parent) = path.parent() {
        if !is_directory_writable(parent) {
            return false;
        }
    } else {
        // Path has no parent (e.g., root "/"), rely on file-specific checks
    }


    if path.exists() {
        if !path.is_file() { return false; } // Cannot write to directory like this
        OpenOptions::new().write(true).open(path).is_ok()
    } else {
        // Try to create the file, then immediately remove it
        match File::create(path) {
            Ok(_) => {
                let _ = fs::remove_file(path); // Attempt cleanup, ignore error
                true
            }
            Err(_) => false,
        }
    }
}

/// Checks if a directory exists and is writable by the current user.
pub fn is_directory_writable(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                // Try creating and removing a temporary file inside the directory
                let test_file_name = format!(".write_test_{}", rand::random::<u64>());
                let test_file_path = path.join(test_file_name);
                is_file_writable(&test_file_path) // Reuse file check logic
            } else {
                false // Not a directory
            }
        },
        Err(_) => false, // Doesn't exist or error accessing metadata
    }
}

/// Ensures a directory exists and is writable by the current user.
pub fn ensure_writable_directory(path: &Path) -> AppResult<()> {
    ensure_directory(path)?; // Create if needed
    if !is_directory_writable(path) {
        error!("Directory is not writable: {}", path.display());
        Err(AppError::IoError(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("Directory is not writable: {}", path.display()),
        )))
    } else {
        trace!("Directory confirmed writable: {}", path.display());
        Ok(())
    }
}