// ==============================================================================
// secure_delete.rs - DoD 5220.22-M Secure File Deletion
// ==============================================================================
// Description: 7-pass overwrite for secure deletion of genetic data
// Author: Matt Barham
// Created: 2025-10-31
// Modified: 2026-11-26
// Version: 1.1.0
// Security: DoD 5220.22-M standard (7-pass overwrite)
// ==============================================================================

use anyhow::{Context, Result, ensure};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use tracing::{debug, info};

/// Bytes held in memory while overwriting, whatever the file size
const OVERWRITE_BUFFER: usize = 1024 * 1024;

const PASSES: usize = 7;

/// What one overwrite pass writes
#[derive(Debug, Clone, Copy)]
enum Pass {
    Fill(u8),
    Random,
}

/// Securely delete a file using the DoD 5220.22-M style 7-pass overwrite
///
/// Pass pattern:
/// 1. 0x00 (all zeros)
/// 2. 0xFF (all ones)
///    3-6. Random data
/// 7. 0x00 (all zeros), then read back
///
/// After overwriting, the file is unlinked from the filesystem. Overwriting in
/// place cannot guarantee erasure on SSDs or copy-on-write filesystems; the
/// encrypted volume remains the primary protection.
pub async fn secure_delete_file(path: &Path) -> Result<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || delete_file(&path))
        .await
        .context("Secure deletion task failed")?
}

/// Securely delete an entire directory and all its contents
///
/// Symbolic links are removed, never followed. Every file is attempted even if
/// an earlier one fails; the first failure is returned.
pub async fn secure_delete_directory(path: &Path) -> Result<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || delete_directory(&path))
        .await
        .context("Secure deletion task failed")?
}

fn delete_file(path: &Path) -> Result<()> {
    info!("Securely deleting file: {:?}", path);

    overwrite(path)?;
    std::fs::remove_file(path).context("Failed to remove file after secure overwrite")?;

    info!("File securely deleted: {:?}", path);
    Ok(())
}

fn delete_directory(path: &Path) -> Result<()> {
    info!("Securely deleting directory: {:?}", path);

    let mut first_error = None;
    // Contents first, so each directory is empty by the time it is reached
    for entry in walkdir::WalkDir::new(path).contents_first(true) {
        let result = entry.context("Failed to walk directory").and_then(|entry| {
            let file_type = entry.file_type();
            if file_type.is_file() {
                delete_file(entry.path())
            } else if file_type.is_dir() {
                std::fs::remove_dir(entry.path()).context("Failed to remove directory")
            } else {
                std::fs::remove_file(entry.path()).context("Failed to remove link")
            }
        });
        if let Err(error) = result {
            first_error.get_or_insert(error);
        }
    }

    match first_error {
        Some(error) => Err(error),
        None => {
            info!("Directory securely deleted: {:?}", path);
            Ok(())
        }
    }
}

/// Overwrite a file's contents in place, syncing after each pass
fn overwrite(path: &Path) -> Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .context("Failed to open file for writing")?;
    let size = file
        .metadata()
        .context("Failed to get file metadata")?
        .len();
    let mut buffer = vec![0u8; OVERWRITE_BUFFER];

    let passes: [Pass; PASSES] = [
        Pass::Fill(0x00),
        Pass::Fill(0xFF),
        Pass::Random,
        Pass::Random,
        Pass::Random,
        Pass::Random,
        Pass::Fill(0x00),
    ];

    for (number, pass) in passes.into_iter().enumerate() {
        debug!("Pass {}/{}: {:?}", number + 1, PASSES, pass);
        if let Pass::Fill(byte) = pass {
            buffer.fill(byte);
        }

        file.seek(SeekFrom::Start(0))
            .context("Failed to seek to file start")?;
        let mut remaining = size;
        while remaining > 0 {
            let length = chunk_length(remaining);
            if let Pass::Random = pass {
                rand::fill(&mut buffer[..length]);
            }
            file.write_all(&buffer[..length])
                .context("Failed to write overwrite pattern")?;
            remaining -= length as u64;
        }

        // Sync to disk (ensure data is written, not just buffered)
        file.sync_all().context("Failed to sync file to disk")?;
    }

    verify_zeroed(&mut file, size, &mut buffer)
}

/// Read the file back and confirm the final pass left only zeros
fn verify_zeroed(file: &mut File, size: u64, buffer: &mut [u8]) -> Result<()> {
    file.seek(SeekFrom::Start(0))
        .context("Failed to seek to file start")?;
    let mut remaining = size;
    while remaining > 0 {
        let length = chunk_length(remaining);
        file.read_exact(&mut buffer[..length])
            .context("Failed to read back overwritten file")?;
        ensure!(
            buffer[..length].iter().all(|byte| *byte == 0),
            "Overwrite verification failed"
        );
        remaining -= length as u64;
    }
    Ok(())
}

fn chunk_length(remaining: u64) -> usize {
    usize::try_from(remaining).map_or(OVERWRITE_BUFFER, |r| r.min(OVERWRITE_BUFFER))
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

    #[tokio::test]
    async fn overwrite_covers_files_larger_than_one_buffer() {
        let temp_file = NamedTempFile::new().unwrap();
        let length = OVERWRITE_BUFFER * 2 + 17;
        std::fs::write(temp_file.path(), vec![0xAB; length]).unwrap();

        overwrite(temp_file.path()).unwrap();

        let after = std::fs::read(temp_file.path()).unwrap();
        assert_eq!(after.len(), length);
        assert!(after.iter().all(|byte| *byte == 0));
    }

    #[tokio::test]
    async fn directory_deletion_never_follows_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("keep.txt");
        std::fs::write(&outside, b"keep me").unwrap();
        let target = dir.path().join("job");
        std::fs::create_dir_all(target.join("nested")).unwrap();
        std::fs::write(target.join("nested/genome.txt"), b"data").unwrap();
        std::os::unix::fs::symlink(&outside, target.join("link")).unwrap();

        secure_delete_directory(&target).await.unwrap();

        assert!(!target.exists());
        assert_eq!(std::fs::read(&outside).unwrap(), b"keep me");
    }
}
