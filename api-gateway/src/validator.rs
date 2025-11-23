// ==============================================================================
// validator.rs - File Upload Validation (API Gateway)
// ==============================================================================
// Description: Validates uploaded files at API layer before writing to disk
// Author: Matt Barham
// Created: 2025-11-26
// Modified: 2025-11-26
// Version: 1.0.0
// Security: Allowlist-only file types, magic number verification, size limits
// ==============================================================================

use anyhow::{Context, Result};
use axum::body::Bytes;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use tracing::{debug, info, warn};

// Maximum file sizes (enforced at validation layer)
const MAX_GENOME_FILE_SIZE: usize = 100 * 1024 * 1024; // 100 MB
const MAX_VCF_FILE_SIZE: usize = 200 * 1024 * 1024;    // 200 MB
const MAX_PGS_FILE_SIZE: usize = 10 * 1024 * 1024;     // 10 MB
const MAX_CHUNK_SIZE: usize = 50 * 1024 * 1024;        // 50 MB per chunk

#[derive(Debug)]
pub struct ValidatedFile {
    pub original_name: String,
    pub safe_name: String,
    pub extension: String,
    pub size: usize,
    pub hash_sha256: String,
    pub validated_at: chrono::DateTime<chrono::Utc>,
}

pub struct FileValidator {
    allowed_types: HashMap<String, Vec<u8>>,
}

impl FileValidator {
    pub fn new() -> Self {
        let mut allowed_types = HashMap::new();

        // 23andMe raw text file (plain text, no specific magic number)
        allowed_types.insert("txt".to_string(), vec![]);

        // Gzip compressed files (VCF.gz)
        allowed_types.insert("vcf.gz".to_string(), vec![0x1f, 0x8b, 0x08]);

        // BGZF tabix index
        allowed_types.insert("vcf.gz.tbi".to_string(), vec![0x1f, 0x8b, 0x08]);

        // PGS score files
        allowed_types.insert("pgs".to_string(), vec![]);

        Self { allowed_types }
    }

    /// Validate file upload from multipart form data
    pub fn validate_upload(
        &self,
        filename: &str,
        file_data: &Bytes,
        file_type: &str, // "genome", "vcf", "pgs", "chunk"
    ) -> Result<ValidatedFile> {
        info!("Validating file: {} (type: {})", filename, file_type);

        // 1. Size check (BEFORE any processing)
        let size = file_data.len();
        let max_size = match file_type {
            "genome" => MAX_GENOME_FILE_SIZE,
            "vcf" => MAX_VCF_FILE_SIZE,
            "pgs" => MAX_PGS_FILE_SIZE,
            "chunk" => MAX_CHUNK_SIZE,
            _ => return Err(anyhow::anyhow!("Unknown file type: {}", file_type)),
        };

        if size > max_size {
            anyhow::bail!(
                "File too large: {} bytes (max: {} bytes for {})",
                size,
                max_size,
                file_type
            );
        }
        debug!("Size check passed: {} bytes", size);

        // 2. Filename sanitization
        let safe_name = self.sanitize_filename(filename)?;
        debug!("Sanitized filename: {}", safe_name);

        // 3. Extension check (allowlist)
        let ext = self.get_extension(&safe_name)?;
        if !self.allowed_types.contains_key(&ext) {
            anyhow::bail!("Invalid file type: .{}", ext);
        }
        debug!("Extension check passed: .{}", ext);

        // 4. Magic number verification
        if let Some(expected_magic) = self.allowed_types.get(&ext) {
            if !expected_magic.is_empty() {
                if file_data.len() < expected_magic.len() {
                    anyhow::bail!("File too small to contain magic number");
                }
                let actual_magic = &file_data[..expected_magic.len()];
                if !self.verify_magic_number(expected_magic, actual_magic) {
                    anyhow::bail!("Magic number mismatch for .{} file", ext);
                }
                debug!("Magic number check passed");
            }
        }

        // 5. Content validation (basic format check)
        self.validate_content(file_data, &ext)?;
        debug!("Content validation passed");

        // 6. Compute SHA-256 hash
        let hash = self.compute_sha256(file_data);
        debug!("SHA-256: {}", hash);

        Ok(ValidatedFile {
            original_name: filename.to_string(),
            safe_name,
            extension: ext,
            size,
            hash_sha256: hash,
            validated_at: chrono::Utc::now(),
        })
    }

    fn sanitize_filename(&self, name: &str) -> Result<String> {
        // Remove path separators, null bytes, control characters
        let safe = name
            .replace(['/', '\\', '\0'], "_")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.' || *c == '-')
            .collect::<String>();

        // Limit length to 255 characters
        let truncated: String = safe.chars().take(255).collect();

        // Must not be empty after sanitization
        if truncated.is_empty() {
            anyhow::bail!("Invalid filename after sanitization");
        }

        // Must not start with . (hidden file)
        if truncated.starts_with('.') {
            anyhow::bail!("Filename cannot start with '.'");
        }

        Ok(truncated)
    }

    fn get_extension(&self, filename: &str) -> Result<String> {
        // Handle compound extensions like .vcf.gz
        if filename.ends_with(".vcf.gz") {
            return Ok("vcf.gz".to_string());
        } else if filename.ends_with(".vcf.gz.tbi") {
            return Ok("vcf.gz.tbi".to_string());
        } else if filename.ends_with(".pgs") || filename.ends_with(".txt.gz") {
            // PGS files can be .pgs or .txt.gz
            return Ok("pgs".to_string());
        }

        // Single extension
        filename
            .rsplit('.')
            .next()
            .map(|s| s.to_lowercase())
            .ok_or_else(|| anyhow::anyhow!("No file extension found"))
    }

    fn verify_magic_number(&self, expected: &[u8], actual: &[u8]) -> bool {
        expected.len() <= actual.len()
            && expected.iter().zip(actual.iter()).all(|(e, a)| e == a)
    }

    fn validate_content(&self, data: &Bytes, ext: &str) -> Result<()> {
        match ext {
            "txt" => self.validate_23andme_format(data),
            "vcf.gz" => self.validate_vcf_format(data),
            "vcf.gz.tbi" => Ok(()), // Tabix index, no content validation needed
            "pgs" => self.validate_pgs_format(data),
            _ => Ok(()),
        }
    }

    fn validate_23andme_format(&self, data: &Bytes) -> Result<()> {
        let reader = BufReader::new(&data[..]);
        let mut lines = reader.lines();

        // Check for 23andMe header
        let first_line = lines
            .next()
            .ok_or_else(|| anyhow::anyhow!("File is empty"))??;

        if !first_line.contains("23andMe") && !first_line.starts_with('#') {
            anyhow::bail!("Not a valid 23andMe format file");
        }

        // Find first data line (skip comments)
        for line in lines.take(100) {
            // Only check first 100 lines
            let line = line?;
            if !line.starts_with('#') && !line.trim().is_empty() {
                // First data line should have 4 columns: rsid, chromosome, position, genotype
                let columns: Vec<&str> = line.split_whitespace().collect();
                if columns.len() != 4 {
                    anyhow::bail!(
                        "Invalid 23andMe format: expected 4 columns, found {}",
                        columns.len()
                    );
                }
                break;
            }
        }

        Ok(())
    }

    fn validate_vcf_format(&self, data: &Bytes) -> Result<()> {
        // VCF files are gzipped, need to decompress to check header
        let decoder = flate2::read::GzDecoder::new(&data[..]);
        let reader = BufReader::new(decoder);
        let mut lines = reader.lines();

        // First line should be ##fileformat=VCFv4.x
        let first_line = lines
            .next()
            .ok_or_else(|| anyhow::anyhow!("VCF file is empty"))??;

        if !first_line.starts_with("##fileformat=VCFv4.") {
            anyhow::bail!("Invalid VCF format: missing fileformat header");
        }

        Ok(())
    }

    fn validate_pgs_format(&self, data: &Bytes) -> Result<()> {
        // PGS files are tab-separated or space-separated text files
        // Should have at least a header line with rsid and effect columns
        let reader = BufReader::new(&data[..]);
        let mut lines = reader.lines();

        // Check first non-comment line
        for line in lines.take(100) {
            let line = line?;
            if !line.starts_with('#') && !line.trim().is_empty() {
                // Should have at least 2 columns
                let columns: Vec<&str> = line.split_whitespace().collect();
                if columns.len() < 2 {
                    anyhow::bail!(
                        "Invalid PGS format: expected at least 2 columns, found {}",
                        columns.len()
                    );
                }
                break;
            }
        }

        Ok(())
    }

    fn compute_sha256(&self, data: &Bytes) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Quick validation for chunked uploads (less strict, worker will re-validate)
    pub fn validate_chunk(
        &self,
        filename: &str,
        chunk_data: &Bytes,
        chunk_index: usize,
        total_chunks: usize,
    ) -> Result<()> {
        // 1. Size check
        if chunk_data.len() > MAX_CHUNK_SIZE {
            anyhow::bail!(
                "Chunk too large: {} bytes (max: {} bytes)",
                chunk_data.len(),
                MAX_CHUNK_SIZE
            );
        }

        // 2. Filename sanitization
        let _safe_name = self.sanitize_filename(filename)?;

        // 3. Chunk index validation
        if chunk_index >= total_chunks {
            anyhow::bail!(
                "Invalid chunk index: {} (total chunks: {})",
                chunk_index,
                total_chunks
            );
        }

        // 4. Reasonable total chunks (prevent excessive chunking)
        if total_chunks > 100 {
            anyhow::bail!(
                "Too many chunks: {} (max: 100)",
                total_chunks
            );
        }

        Ok(())
    }
}

impl Default for FileValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        let validator = FileValidator::new();

        assert_eq!(
            validator.sanitize_filename("genome_file.txt").unwrap(),
            "genome_file.txt"
        );

        assert_eq!(
            validator.sanitize_filename("../../../etc/passwd").unwrap(),
            ".._.._.._etc_passwd"
        );

        assert!(validator.sanitize_filename(".hidden").is_err());
    }

    #[test]
    fn test_get_extension() {
        let validator = FileValidator::new();

        assert_eq!(
            validator.get_extension("chr1.dose.vcf.gz").unwrap(),
            "vcf.gz"
        );

        assert_eq!(validator.get_extension("genome.txt").unwrap(), "txt");

        assert_eq!(
            validator.get_extension("chr1.dose.vcf.gz.tbi").unwrap(),
            "vcf.gz.tbi"
        );
    }

    #[test]
    fn test_size_limits() {
        let validator = FileValidator::new();

        // Test oversized genome file
        let large_data = Bytes::from(vec![0u8; 101 * 1024 * 1024]); // 101 MB
        let result = validator.validate_upload("test.txt", &large_data, "genome");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }
}
