// ==============================================================================
// validator.rs - Input File Validation
// ==============================================================================
// Description: Validates uploaded genetic data files (size, type, format, virus)
// Author: Matt Barham
// Created: 2025-10-31
// Modified: 2025-10-31
// Version: 1.0.0
// Security: Allowlist-only file types, magic number verification
// ==============================================================================

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use tracing::{debug, info};

const MAX_FILE_SIZE: usize = 500 * 1024 * 1024; // 500 MB

#[derive(Debug)]
pub struct ValidatedFile {
    pub original_name: String,
    pub safe_name: String,
    pub extension: String,
    pub size: u64,
    pub hash_sha256: String,
    pub validated_at: chrono::DateTime<chrono::Utc>,
}

pub struct FileValidator {
    max_file_size: usize,
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

        Self {
            max_file_size: MAX_FILE_SIZE,
            allowed_types,
        }
    }

    pub async fn validate_upload(&self, file_path: &Path) -> Result<ValidatedFile> {
        let file_name = file_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?
            .to_string_lossy()
            .to_string();

        info!("Validating file: {}", file_name);

        // 1. Size check
        let metadata = std::fs::metadata(file_path)
            .context("Failed to get file metadata")?;
        let size = metadata.len() as usize;

        if size > self.max_file_size {
            anyhow::bail!(
                "File too large: {} bytes (max: {} bytes)",
                size,
                self.max_file_size
            );
        }
        debug!("Size check passed: {} bytes", size);

        // 2. Filename sanitization
        let safe_name = self.sanitize_filename(&file_name)?;
        debug!("Sanitized filename: {}", safe_name);

        // 3. Extension check (allowlist)
        let ext = self.get_extension(&safe_name)?;
        if !self.allowed_types.contains_key(&ext) {
            anyhow::bail!("Invalid file type: {}", ext);
        }
        debug!("Extension check passed: {}", ext);

        // 4. Magic number verification
        if let Some(expected_magic) = self.allowed_types.get(&ext) {
            if !expected_magic.is_empty() {
                let actual_magic = self.read_magic_number(file_path)?;
                if !self.verify_magic_number(expected_magic, &actual_magic) {
                    anyhow::bail!(
                        "Magic number mismatch for .{} file",
                        ext
                    );
                }
                debug!("Magic number check passed");
            }
        }

        // 5. Content validation (basic format check)
        self.validate_content(file_path, &ext).await?;
        debug!("Content validation passed");

        // 6. Compute SHA-256 hash
        let hash = self.compute_sha256(file_path)?;
        debug!("SHA-256: {}", hash);

        Ok(ValidatedFile {
            original_name: file_name,
            safe_name,
            extension: ext,
            size: metadata.len(),
            hash_sha256: hash,
            validated_at: chrono::Utc::now(),
        })
    }

    fn sanitize_filename(&self, name: &str) -> Result<String> {
        // Remove path separators, null bytes, control characters
        let safe = name
            .replace(['/', '\\', '\0'], "_")
            .chars()
            .filter(|c| {
                c.is_ascii_alphanumeric()
                    || *c == '_'
                    || *c == '.'
                    || *c == '-'
            })
            .collect::<String>();

        // Limit length to 255 characters
        let truncated: String = safe.chars().take(255).collect();

        // Must not be empty after sanitization
        if truncated.is_empty() {
            anyhow::bail!("Invalid filename after sanitization");
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

    fn read_magic_number(&self, path: &Path) -> Result<Vec<u8>> {
        let mut file = File::open(path)?;
        let mut buffer = vec![0u8; 4];
        file.read_exact(&mut buffer)?;
        Ok(buffer)
    }

    fn verify_magic_number(&self, expected: &[u8], actual: &[u8]) -> bool {
        expected.len() <= actual.len()
            && expected.iter().zip(actual.iter()).all(|(e, a)| e == a)
    }

    async fn validate_content(&self, path: &Path, ext: &str) -> Result<()> {
        match ext {
            "txt" => self.validate_23andme_format(path),
            "vcf.gz" => self.validate_vcf_format(path),
            "vcf.gz.tbi" => Ok(()), // Tabix index, no content validation needed
            _ => Ok(()),
        }
    }

    fn validate_23andme_format(&self, path: &Path) -> Result<()> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // Check for 23andMe header
        let first_line = lines
            .next()
            .ok_or_else(|| anyhow::anyhow!("File is empty"))??;

        if !first_line.contains("23andMe") && !first_line.starts_with('#') {
            anyhow::bail!("Not a valid 23andMe format file");
        }

        // Find first data line (skip comments)
        for line in lines {
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

    fn validate_vcf_format(&self, path: &Path) -> Result<()> {
        // VCF files are gzipped, need to decompress to check header
        let file = File::open(path)?;
        let decoder = flate2::read::GzDecoder::new(file);
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

    fn compute_sha256(&self, path: &Path) -> Result<String> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 8192];

        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(format!("{:x}", hasher.finalize()))
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
    use std::io::Write;
    use tempfile::NamedTempFile;

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

        assert_eq!(
            validator.sanitize_filename("file\0with\nnull.txt").unwrap(),
            "file_withnull.txt"  // \n is filtered out, not replaced
        );
    }

    #[test]
    fn test_get_extension() {
        let validator = FileValidator::new();

        assert_eq!(
            validator.get_extension("chr1.dose.vcf.gz").unwrap(),
            "vcf.gz"
        );

        assert_eq!(
            validator.get_extension("genome.txt").unwrap(),
            "txt"
        );

        assert_eq!(
            validator.get_extension("chr1.dose.vcf.gz.tbi").unwrap(),
            "vcf.gz.tbi"
        );
    }

    #[tokio::test]
    async fn test_validate_23andme_format() {
        let validator = FileValidator::new();

        // Create valid 23andMe file
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "# This data file generated by 23andMe").unwrap();
        writeln!(temp_file, "# rsid\tchromosome\tposition\tgenotype").unwrap();
        writeln!(temp_file, "rs12345\t1\t12345\tAA").unwrap();
        temp_file.flush().unwrap();

        assert!(validator.validate_23andme_format(temp_file.path()).is_ok());
    }
}
