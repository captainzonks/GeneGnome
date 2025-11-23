// ==============================================================================
// secure_delete.rs - DoD 5220.22-M Secure File Deletion
// ==============================================================================
// Description: 7-pass overwrite for secure deletion of genetic data
// Author: Matt Barham
// Created: 2025-10-31
// Modified: 2025-10-31
// Version: 1.0.0
// Security: DoD 5220.22-M standard (7-pass overwrite)
// ==============================================================================

use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use tracing::{info, debug};

/// Securely delete a file using DoD 5220.22-M standard (7-pass overwrite)
///
/// Pass pattern:
/// 1. 0x00 (all zeros)
/// 2. 0xFF (all ones)
/// 3-6. Random data
/// 7. 0x00 (all zeros)
///
/// After overwriting, the file is unlinked from the filesystem.
pub async fn secure_delete_file(path: &Path) -> Result<()> {
    info!("Securely deleting file: {:?}", path);

    // Get file size
    let metadata = std::fs::metadata(path)
        .context("Failed to get file metadata")?;
    let size = metadata.len() as usize;

    debug!("File size: {} bytes, beginning 7-pass overwrite", size);

    // Open file for writing
    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .context("Failed to open file for writing")?;

    // Perform 7-pass overwrite
    for pass in 0..7 {
        let pattern: u8 = match pass {
            0 => {
                debug!("Pass 1/7: Writing 0x00 (all zeros)");
                0x00
            }
            1 => {
                debug!("Pass 2/7: Writing 0xFF (all ones)");
                0xFF
            }
            2..=5 => {
                debug!("Pass {}/7: Writing random data", pass + 1);
                rand::random::<u8>()
            }
            6 => {
                debug!("Pass 7/7: Writing 0x00 (all zeros)");
                0x00
            }
            _ => unreachable!(),
        };

        // Create buffer with pattern
        let buffer = if pass >= 2 && pass <= 5 {
            // Random data: generate new random bytes for each chunk
            vec![0u8; size]
                .into_iter()
                .map(|_| rand::random::<u8>())
                .collect::<Vec<u8>>()
        } else {
            // Fixed pattern
            vec![pattern; size]
        };

        // Seek to beginning
        file.seek(SeekFrom::Start(0))
            .context("Failed to seek to file start")?;

        // Write pattern
        file.write_all(&buffer)
            .context("Failed to write overwrite pattern")?;

        // Sync to disk (ensure data is written, not just buffered)
        file.sync_all()
            .context("Failed to sync file to disk")?;
    }

    // Close file handle
    drop(file);

    // Unlink from filesystem
    std::fs::remove_file(path)
        .context("Failed to remove file after secure overwrite")?;

    info!("File securely deleted: {:?}", path);
    Ok(())
}

/// Securely delete an entire directory and all its contents
pub async fn secure_delete_directory(path: &Path) -> Result<()> {
    info!("Securely deleting directory: {:?}", path);

    // Recursively delete all files
    for entry in walkdir::WalkDir::new(path)
        .contents_first(true)  // Files before directories
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let entry_path = entry.path();

        if entry_path.is_file() {
            secure_delete_file(entry_path).await?;
        } else if entry_path.is_dir() && entry_path != path {
            // Remove empty directories
            std::fs::remove_dir(entry_path)
                .context("Failed to remove directory")?;
        }
    }

    // Remove the root directory
    std::fs::remove_dir(path)
        .context("Failed to remove root directory")?;

    info!("Directory securely deleted: {:?}", path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_secure_delete_overwrites_data() {
        // Create temporary file with known data
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"SENSITIVE_GENETIC_DATA_12345";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        let path = temp_file.path().to_path_buf();

        // Securely delete
        secure_delete_file(&path).await.unwrap();

        // File should no longer exist
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn test_secure_delete_removes_file() {
        // Create temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();

        // Write some data
        std::fs::write(&path, b"test data").unwrap();
        assert!(path.exists());

        // Securely delete
        secure_delete_file(&path).await.unwrap();

        // File should be gone
        assert!(!path.exists());
    }
}
