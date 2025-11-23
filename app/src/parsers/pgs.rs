// ==============================================================================
// pgs.rs - Polygenic Score (PGS) Parser
// ==============================================================================
// Description: Parser for polygenic score data with z-score normalization
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2025-11-06
// Version: 1.0.0
// ==============================================================================
// Format: CSV file with header
// Example:
//   ID,PGS_label,score_value
//   sample1,Height,1.234
//   sample1,BMI,0.456
//   sample2,Height,1.567
// ==============================================================================

use csv::ReaderBuilder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

/// Polygenic score record
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PgsRecord {
    /// Sample identifier
    #[serde(rename = "ID")]
    pub sample_id: String,

    /// Polygenic score label/trait name (e.g., "Height", "BMI")
    #[serde(rename = "PGS_label")]
    pub label: String,

    /// Raw score value
    #[serde(rename = "score_value")]
    pub value: f64,
}

/// PGS dataset with both unscaled and scaled versions
#[derive(Debug, Clone)]
pub struct PgsDataset {
    /// Original unscaled PGS values
    pub unscaled: Vec<PgsRecord>,

    /// Z-score normalized PGS values (per label)
    pub scaled: Vec<PgsRecord>,
}

/// Errors that can occur during PGS file parsing
#[derive(Error, Debug)]
pub enum PgsParseError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("CSV parsing error: {0}")]
    CsvError(#[from] csv::Error),

    #[error("File is empty or contains no valid records")]
    EmptyFile,

    #[error("Invalid score value: {0}")]
    InvalidValue(String),
}

/// PGS file parser with z-score scaling capability
pub struct PgsParser;

impl PgsParser {
    /// Parse PGS scores from CSV file
    ///
    /// # Arguments
    /// * `path` - Path to the PGS scores file (scores.txt)
    ///
    /// # Returns
    /// * `Ok(PgsDataset)` - Dataset with unscaled and scaled versions
    /// * `Err(PgsParseError)` - Parse error
    ///
    /// # Format
    /// Supports two formats:
    ///
    /// 1. Long-form (original):
    ///    ID,PGS_label,score_value
    ///    sample1,Height,1.234
    ///
    /// 2. Wide-form (Michigan Imputation Server):
    ///    "sample","PGS000008","PGS000006",...
    ///    "samp1",0.365,-0.300,...
    ///
    /// # Z-score Normalization
    /// Scaling is performed per PGS label:
    /// - scaled_value = (value - mean) / std_dev
    /// - Each trait is normalized independently
    /// - If std_dev = 0 (constant values), scaled_value = 0
    ///
    /// # Example
    /// ```
    /// use genetics_processor::parsers::pgs::PgsParser;
    ///
    /// let dataset = PgsParser::parse("scores.txt")?;
    /// println!("Unscaled records: {}", dataset.unscaled.len());
    /// println!("Scaled records: {}", dataset.scaled.len());
    /// ```
    pub fn parse(path: impl AsRef<Path>) -> Result<PgsDataset, PgsParseError> {
        let mut reader = ReaderBuilder::new()
            .has_headers(true)
            .from_path(path.as_ref())?;

        // Get headers to determine format
        let headers = reader.headers()?.clone();

        // Check first column name to determine format
        let first_col = headers.get(0).ok_or(PgsParseError::EmptyFile)?;

        let unscaled = if first_col == "sample" || first_col == "\"sample\"" {
            // Wide-form format (Michigan Imputation Server)
            Self::parse_wide_format(&mut reader, &headers)?
        } else {
            // Long-form format (original)
            Self::parse_long_format(&mut reader)?
        };

        if unscaled.is_empty() {
            return Err(PgsParseError::EmptyFile);
        }

        // Scale by PGS label (z-score normalization)
        let scaled = Self::scale_pgs(&unscaled);

        Ok(PgsDataset { unscaled, scaled })
    }

    /// Parse long-form CSV (original format)
    fn parse_long_format(reader: &mut csv::Reader<std::fs::File>) -> Result<Vec<PgsRecord>, PgsParseError> {
        let mut unscaled = Vec::new();

        for (idx, result) in reader.deserialize().enumerate() {
            let record: PgsRecord = result.map_err(|e| {
                PgsParseError::CsvError(csv::Error::from(e))
            })?;

            // Validate score value is finite
            if !record.value.is_finite() {
                return Err(PgsParseError::InvalidValue(
                    format!("Non-finite value at record {}: {}", idx + 1, record.value)
                ));
            }

            unscaled.push(record);
        }

        Ok(unscaled)
    }

    /// Parse wide-form CSV (Michigan Imputation Server format)
    fn parse_wide_format(
        reader: &mut csv::Reader<std::fs::File>,
        headers: &csv::StringRecord,
    ) -> Result<Vec<PgsRecord>, PgsParseError> {
        let mut unscaled = Vec::new();

        // Extract PGS labels from headers (skip first column which is sample ID)
        let pgs_labels: Vec<String> = headers.iter()
            .skip(1)
            .map(|h| h.trim_matches('"').to_string())
            .collect();

        // Read each row (one sample per row)
        for (row_idx, result) in reader.records().enumerate() {
            let record = result.map_err(PgsParseError::CsvError)?;

            // First column is sample ID
            let sample_id = record.get(0)
                .ok_or(PgsParseError::EmptyFile)?
                .trim_matches('"')
                .to_string();

            // Remaining columns are PGS scores
            for (col_idx, value_str) in record.iter().skip(1).enumerate() {
                let value: f64 = value_str.trim_matches('"')
                    .parse()
                    .map_err(|e| PgsParseError::InvalidValue(
                        format!("Failed to parse value '{}' at row {}, col {}: {}",
                            value_str, row_idx + 1, col_idx + 1, e)
                    ))?;

                // Validate score value is finite
                if !value.is_finite() {
                    return Err(PgsParseError::InvalidValue(
                        format!("Non-finite value at row {}, col {}: {}",
                            row_idx + 1, col_idx + 1, value)
                    ));
                }

                unscaled.push(PgsRecord {
                    sample_id: sample_id.clone(),
                    label: pgs_labels[col_idx].clone(),
                    value,
                });
            }
        }

        Ok(unscaled)
    }

    /// Apply z-score normalization per PGS label
    ///
    /// # Algorithm
    /// For each unique PGS label:
    /// 1. Compute mean: μ = Σ(x) / n
    /// 2. Compute standard deviation: σ = sqrt(Σ(x - μ)² / n)
    /// 3. Scale each value: z = (x - μ) / σ
    /// 4. If σ = 0 (constant values), z = 0
    ///
    /// # Arguments
    /// * `records` - Unscaled PGS records
    ///
    /// # Returns
    /// * Scaled PGS records with z-score normalized values
    fn scale_pgs(records: &[PgsRecord]) -> Vec<PgsRecord> {
        // Group by label
        let mut by_label: HashMap<String, Vec<&PgsRecord>> = HashMap::new();
        for record in records {
            by_label
                .entry(record.label.clone())
                .or_insert_with(Vec::new)
                .push(record);
        }

        let mut scaled = Vec::new();

        // For each label, compute mean and SD, then scale
        for (label, group) in by_label {
            // Compute mean
            let sum: f64 = group.iter().map(|r| r.value).sum();
            let n = group.len() as f64;
            let mean = sum / n;

            // Compute standard deviation
            let variance: f64 = group
                .iter()
                .map(|r| (r.value - mean).powi(2))
                .sum::<f64>()
                / n;
            let std_dev = variance.sqrt();

            // Scale each record
            for record in group {
                let scaled_value = if std_dev > 0.0 {
                    (record.value - mean) / std_dev
                } else {
                    // Handle constant values (all same)
                    0.0
                };

                scaled.push(PgsRecord {
                    sample_id: record.sample_id.clone(),
                    label: label.clone(),
                    value: scaled_value,
                });
            }
        }

        scaled
    }

    /// Get statistics for a specific PGS label
    ///
    /// # Arguments
    /// * `records` - PGS records to analyze
    /// * `label` - PGS label to compute statistics for
    ///
    /// # Returns
    /// * `Some(PgsStats)` - Statistics for the label
    /// * `None` - Label not found
    pub fn get_stats(records: &[PgsRecord], label: &str) -> Option<PgsStats> {
        let filtered: Vec<f64> = records
            .iter()
            .filter(|r| r.label == label)
            .map(|r| r.value)
            .collect();

        if filtered.is_empty() {
            return None;
        }

        let n = filtered.len() as f64;
        let sum: f64 = filtered.iter().sum();
        let mean = sum / n;

        let variance: f64 = filtered.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();

        let min = filtered.iter().copied().fold(f64::INFINITY, f64::min);
        let max = filtered.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        Some(PgsStats {
            label: label.to_string(),
            count: filtered.len(),
            mean,
            std_dev,
            min,
            max,
        })
    }
}

/// Statistics for a PGS label
#[derive(Debug, Clone, PartialEq)]
pub struct PgsStats {
    pub label: String,
    pub count: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_z_score_normalization() {
        // Create test records with known values
        // Height: [1.0, 2.0, 3.0] -> mean=2.0, std_dev≈0.816
        // Expected scaled: [-1.224, 0.0, 1.224]
        let records = vec![
            PgsRecord {
                sample_id: "s1".to_string(),
                label: "Height".to_string(),
                value: 1.0,
            },
            PgsRecord {
                sample_id: "s2".to_string(),
                label: "Height".to_string(),
                value: 2.0,
            },
            PgsRecord {
                sample_id: "s3".to_string(),
                label: "Height".to_string(),
                value: 3.0,
            },
        ];

        let scaled = PgsParser::scale_pgs(&records);

        // Find scaled records
        let s1_scaled = scaled.iter().find(|r| r.sample_id == "s1").unwrap();
        let s2_scaled = scaled.iter().find(|r| r.sample_id == "s2").unwrap();
        let s3_scaled = scaled.iter().find(|r| r.sample_id == "s3").unwrap();

        // Check mean is approximately 0
        let scaled_mean = (s1_scaled.value + s2_scaled.value + s3_scaled.value) / 3.0;
        assert!(scaled_mean.abs() < 1e-10, "Mean should be ~0, got {}", scaled_mean);

        // Check values are symmetric around mean
        assert!(s1_scaled.value < 0.0, "Below mean should be negative");
        assert!(s2_scaled.value.abs() < 0.01, "At mean should be ~0");
        assert!(s3_scaled.value > 0.0, "Above mean should be positive");

        // Check specific values (mean=2.0, std=sqrt(2/3)≈0.8165)
        // z1 = (1 - 2) / 0.8165 ≈ -1.2247
        // z2 = (2 - 2) / 0.8165 = 0
        // z3 = (3 - 2) / 0.8165 ≈ 1.2247
        assert!((s1_scaled.value - (-1.2247)).abs() < 0.01, "s1 scaled value incorrect");
        assert!(s2_scaled.value.abs() < 0.01, "s2 scaled value incorrect");
        assert!((s3_scaled.value - 1.2247).abs() < 0.01, "s3 scaled value incorrect");
    }

    #[test]
    fn test_multiple_labels() {
        // Test that different labels are scaled independently
        let records = vec![
            PgsRecord {
                sample_id: "s1".to_string(),
                label: "Height".to_string(),
                value: 100.0,
            },
            PgsRecord {
                sample_id: "s1".to_string(),
                label: "BMI".to_string(),
                value: 10.0,
            },
            PgsRecord {
                sample_id: "s2".to_string(),
                label: "Height".to_string(),
                value: 200.0,
            },
            PgsRecord {
                sample_id: "s2".to_string(),
                label: "BMI".to_string(),
                value: 20.0,
            },
        ];

        let scaled = PgsParser::scale_pgs(&records);

        // Each label should have its own scaling
        let height_records: Vec<_> = scaled.iter().filter(|r| r.label == "Height").collect();
        let bmi_records: Vec<_> = scaled.iter().filter(|r| r.label == "BMI").collect();

        assert_eq!(height_records.len(), 2);
        assert_eq!(bmi_records.len(), 2);

        // Each label should have mean ≈ 0 and std_dev ≈ 1
        let height_mean = height_records.iter().map(|r| r.value).sum::<f64>() / 2.0;
        let bmi_mean = bmi_records.iter().map(|r| r.value).sum::<f64>() / 2.0;

        assert!(height_mean.abs() < 1e-10, "Height mean should be ~0");
        assert!(bmi_mean.abs() < 1e-10, "BMI mean should be ~0");
    }

    #[test]
    fn test_constant_values() {
        // Test handling of constant values (all same)
        let records = vec![
            PgsRecord {
                sample_id: "s1".to_string(),
                label: "Constant".to_string(),
                value: 5.0,
            },
            PgsRecord {
                sample_id: "s2".to_string(),
                label: "Constant".to_string(),
                value: 5.0,
            },
            PgsRecord {
                sample_id: "s3".to_string(),
                label: "Constant".to_string(),
                value: 5.0,
            },
        ];

        let scaled = PgsParser::scale_pgs(&records);

        // All scaled values should be 0.0 (std_dev = 0)
        for record in scaled {
            assert_eq!(record.value, 0.0, "Constant values should scale to 0");
        }
    }

    #[test]
    fn test_get_stats() {
        let records = vec![
            PgsRecord {
                sample_id: "s1".to_string(),
                label: "Height".to_string(),
                value: 1.0,
            },
            PgsRecord {
                sample_id: "s2".to_string(),
                label: "Height".to_string(),
                value: 2.0,
            },
            PgsRecord {
                sample_id: "s3".to_string(),
                label: "Height".to_string(),
                value: 3.0,
            },
        ];

        let stats = PgsParser::get_stats(&records, "Height").unwrap();

        assert_eq!(stats.label, "Height");
        assert_eq!(stats.count, 3);
        assert!((stats.mean - 2.0).abs() < 1e-10);
        assert!((stats.std_dev - 0.8165).abs() < 0.01);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 3.0);
    }

    #[test]
    fn test_get_stats_missing_label() {
        let records = vec![
            PgsRecord {
                sample_id: "s1".to_string(),
                label: "Height".to_string(),
                value: 1.0,
            },
        ];

        let stats = PgsParser::get_stats(&records, "BMI");
        assert!(stats.is_none(), "Should return None for missing label");
    }

    #[test]
    fn test_single_value_per_label() {
        // Edge case: only one value for a label
        let records = vec![
            PgsRecord {
                sample_id: "s1".to_string(),
                label: "Rare".to_string(),
                value: 42.0,
            },
        ];

        let scaled = PgsParser::scale_pgs(&records);

        // Single value: std_dev = 0, should scale to 0
        assert_eq!(scaled[0].value, 0.0, "Single value should scale to 0");
    }
}
