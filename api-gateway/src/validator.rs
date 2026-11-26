// ==============================================================================
// validator.rs - File Upload Validation (API Gateway)
// ==============================================================================
// Description: Validates uploaded files at API layer before writing to disk
// Author: Matt Barham
// Created: 2025-11-26
// Modified: 2026-11-26
// Version: 1.0.1
// Security: Allowlist-only file types, magic number verification, size limits
// ==============================================================================

use anyhow::Result;
use axum::body::Bytes;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use tracing::{debug, info};

// Maximum file sizes (enforced at validation layer)
const MAX_GENOME_FILE_SIZE: usize = 100 * 1024 * 1024; // 100 MB
const MAX_VCF_FILE_SIZE: usize = 200 * 1024 * 1024; // 200 MB
const MAX_PGS_FILE_SIZE: usize = 10 * 1024 * 1024; // 10 MB
const PGS_EXTENSIONS: [&str; 3] = ["txt", "csv", "pgs"];
const MAX_CHUNK_SIZE: usize = 50 * 1024 * 1024; // 50 MB per chunk

#[derive(Debug)]
pub struct ValidatedFile {
    pub safe_name: String,
    pub size: usize,
    pub hash_sha256: String,
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
        // Score tables have their own extension list, so that adding one never
        // widens what a genome or VCF upload may be called.
        let allowed = if file_type == "pgs" {
            PGS_EXTENSIONS.contains(&ext.as_str())
        } else {
            self.allowed_types.contains_key(&ext)
        };
        if !allowed {
            anyhow::bail!("Invalid file type: .{}", ext);
        }
        debug!("Extension check passed: .{}", ext);

        // 4. Magic number verification
        if let Some(expected_magic) = self.allowed_types.get(&ext)
            && !expected_magic.is_empty()
        {
            if file_data.len() < expected_magic.len() {
                anyhow::bail!("File too small to contain magic number");
            }
            let actual_magic = &file_data[..expected_magic.len()];
            if !self.verify_magic_number(expected_magic, actual_magic) {
                anyhow::bail!("Magic number mismatch for .{} file", ext);
            }
            debug!("Magic number check passed");
        }

        // 5. Content validation (basic format check)
        if file_type == "pgs" {
            self.validate_pgs_format(file_data)?;
        } else {
            self.validate_content(file_data, &ext)?;
        }
        debug!("Content validation passed");

        // 6. Compute SHA-256 hash
        let hash = self.compute_sha256(file_data);
        debug!("SHA-256: {}", hash);

        Ok(ValidatedFile {
            safe_name,
            size,
            hash_sha256: hash,
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
        }

        // Single extension
        filename
            .rsplit('.')
            .next()
            .map(|s| s.to_lowercase())
            .ok_or_else(|| anyhow::anyhow!("No file extension found"))
    }

    fn verify_magic_number(&self, expected: &[u8], actual: &[u8]) -> bool {
        expected.len() <= actual.len() && expected.iter().zip(actual.iter()).all(|(e, a)| e == a)
    }

    fn validate_content(&self, data: &Bytes, ext: &str) -> Result<()> {
        match ext {
            "txt" => self.validate_23andme_format(data),
            "vcf.gz" => self.validate_vcf_format(data),
            "vcf.gz.tbi" => Ok(()), // Tabix index, no content validation needed
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

    /// Checks the header of a precomputed score table. The two accepted layouts
    /// match the parser in app/src/parsers/pgs.rs:
    /// wide `sample,<score>,...` and long `ID,PGS_label,score_value`.
    fn validate_pgs_format(&self, data: &Bytes) -> Result<()> {
        let header = BufReader::new(&data[..])
            .lines()
            .next()
            .ok_or_else(|| anyhow::anyhow!("PGS file is empty"))??;
        let columns: Vec<&str> = header
            .split(',')
            .map(|c| c.trim().trim_matches('"'))
            .collect();

        let wide = columns.len() >= 2 && columns[0] == "sample";
        let long = ["ID", "PGS_label", "score_value"]
            .iter()
            .all(|name| columns.contains(name));
        if !wide && !long {
            anyhow::bail!(
                "Invalid PGS format: expected a CSV header of 'sample,<score>,...' \
                 or 'ID,PGS_label,score_value'"
            );
        }

        Ok(())
    }

    fn compute_sha256(&self, data: &Bytes) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    }

    /// Quick validation for chunked uploads (less strict, worker will re-validate).
    /// Returns the sanitized filename - callers must use this, not the raw
    /// `filename` argument, when building any on-disk path.
    pub fn validate_chunk(
        &self,
        filename: &str,
        chunk_data: &Bytes,
        chunk_index: usize,
        total_chunks: usize,
    ) -> Result<String> {
        // 1. Size check
        if chunk_data.len() > MAX_CHUNK_SIZE {
            anyhow::bail!(
                "Chunk too large: {} bytes (max: {} bytes)",
                chunk_data.len(),
                MAX_CHUNK_SIZE
            );
        }

        // 2. Filename sanitization
        let safe_name = self.sanitize_filename(filename)?;

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
            anyhow::bail!("Too many chunks: {} (max: 100)", total_chunks);
        }

        Ok(safe_name)
    }

    /// Validates a client-supplied chunked-upload session id before it is
    /// ever used to build a filesystem path (uploads/chunks/{upload_id}).
    /// Rejects rather than rewrites - the same id string must match across
    /// every chunk request and the finalize request, so silently mutating
    /// it here would break that correlation.
    pub fn validate_upload_id(&self, upload_id: &str) -> Result<()> {
        if upload_id.is_empty() || upload_id.len() > 128 {
            anyhow::bail!("Invalid upload_id length");
        }

        if !upload_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            anyhow::bail!("upload_id contains invalid characters");
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
    #[should_panic]
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
    fn accepts_michigan_wide_scores_named_scores_txt() {
        let data =
            Bytes::from_static(b"\"sample\",\"PGS000008\",\"PGS000006\"\n\"samp1\",0.365,-0.3\n");
        let validator = FileValidator::new();
        assert!(
            validator
                .validate_upload("scores.txt", &data, "pgs")
                .is_ok()
        );
    }

    #[test]
    fn accepts_long_form_scores() {
        let data = Bytes::from_static(b"ID,PGS_label,score_value\nsamp1,Height,1.2\n");
        let validator = FileValidator::new();
        for name in ["scores.txt", "mine.pgs", "scores.csv"] {
            assert!(
                validator.validate_upload(name, &data, "pgs").is_ok(),
                "{name}"
            );
        }
    }

    #[test]
    fn rejects_files_that_are_not_score_tables() {
        let validator = FileValidator::new();
        let genome = Bytes::from_static(b"# 23andMe\nrs1\t1\t100\tAA\n");
        assert!(
            validator
                .validate_upload("scores.txt", &genome, "pgs")
                .is_err()
        );
        let scores = Bytes::from_static(b"ID,PGS_label,score_value\nsamp1,Height,1.2\n");
        assert!(
            validator
                .validate_upload("scores.vcf.gz", &scores, "pgs")
                .is_err()
        );
        let empty = Bytes::new();
        assert!(
            validator
                .validate_upload("scores.txt", &empty, "pgs")
                .is_err()
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
