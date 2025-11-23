// ==============================================================================
// reference_panel.rs - Reference Panel Database Reader
// ==============================================================================
// Description: Reads 50-sample reference panel from SQLite database
// Author: Matt Barham
// Created: 2025-11-12
// Modified: 2025-11-12
// Version: 1.0.0
// ==============================================================================

use anyhow::{Context, Result};
use rusqlite::{Connection, params, OptionalExtension};
use serde_json;
use std::path::Path;
use tracing::info;

use crate::models::ReferencePanelVariant;

/// Reference panel database reader
pub struct ReferencePanelReader {
    conn: Connection,
}

impl ReferencePanelReader {
    /// Open reference panel database
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path.as_ref())
            .context("Failed to open reference panel database")?;

        Ok(Self { conn })
    }

    /// Get metadata from database
    pub fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM metadata WHERE key = ?1")?;
        let result = stmt.query_row(params![key], |row| row.get(0)).optional()?;
        Ok(result)
    }

    /// Get all reference variants for a specific chromosome
    pub fn get_chromosome_variants(&self, chromosome: u8) -> Result<Vec<ReferencePanelVariant>> {
        let mut stmt = self.conn.prepare(
            "SELECT chromosome, position, rsid, ref_allele, alt_allele, phased,
                    allele_freq, minor_allele_freq, imputation_quality, is_typed,
                    sample_genotypes
             FROM reference_variants
             WHERE chromosome = ?1
             ORDER BY position"
        )?;

        let variant_iter = stmt.query_map(params![chromosome], |row| {
            let sample_genotypes_json: String = row.get(10)?;

            // Deserialize as a map with sample IDs as keys (e.g., {"samp1": "0|0", "samp2": "0|1", ...})
            let sample_map: std::collections::HashMap<String, String> = serde_json::from_str(&sample_genotypes_json)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    10,
                    rusqlite::types::Type::Text,
                    Box::new(e)
                ))?;

            // Extract genotypes in order: samp1, samp2, ..., samp50
            let mut sample_genotypes = Vec::with_capacity(50);
            for i in 1..=50 {
                let sample_id = format!("samp{}", i);
                let genotype = sample_map.get(&sample_id)
                    .ok_or_else(|| rusqlite::Error::FromSqlConversionFailure(
                        10,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Missing genotype for {}", sample_id)
                        ))
                    ))?
                    .clone();
                sample_genotypes.push(genotype);
            }

            Ok(ReferencePanelVariant {
                chromosome: row.get(0)?,
                position: row.get(1)?,
                rsid: row.get(2)?,
                ref_allele: row.get(3)?,
                alt_allele: row.get(4)?,
                phased: row.get::<_, i64>(5)? != 0,
                allele_freq: row.get(6)?,
                minor_allele_freq: row.get(7)?,
                imputation_quality: row.get(8)?,
                is_typed: row.get::<_, i64>(9)? != 0,
                sample_genotypes,
            })
        })?;

        let mut variants = Vec::new();
        for variant in variant_iter {
            variants.push(variant?);
        }

        info!(
            "Loaded {} reference variants for chromosome {}",
            variants.len(),
            chromosome
        );

        Ok(variants)
    }

    /// Get total variant count across all chromosomes
    pub fn get_total_variant_count(&self) -> Result<usize> {
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM reference_variants")?;
        let count: usize = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    }

    /// Get variant count for a specific chromosome
    pub fn get_chromosome_variant_count(&self, chromosome: u8) -> Result<usize> {
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM reference_variants WHERE chromosome = ?1")?;
        let count: usize = stmt.query_row(params![chromosome], |row| row.get(0))?;
        Ok(count)
    }

    /// Check if database is properly formatted
    pub fn validate(&self) -> Result<()> {
        // Check that metadata table exists
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM metadata")?;
        let _count: usize = stmt.query_row([], |row| row.get(0))?;

        // Check that reference_variants table exists
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM reference_variants")?;
        let count: usize = stmt.query_row([], |row| row.get(0))?;

        info!("Reference panel database validated: {} variants", count);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_reference_panel_structure() {
        // This test just validates the data structures compile correctly
        let _variant = ReferencePanelVariant {
            chromosome: 1,
            position: 12345,
            rsid: Some("rs12345".to_string()),
            ref_allele: "A".to_string(),
            alt_allele: "G".to_string(),
            phased: true,
            allele_freq: Some(0.5),
            minor_allele_freq: Some(0.5),
            imputation_quality: Some(0.95),
            is_typed: true,
            sample_genotypes: vec!["0|0".to_string(); 50],
        };
    }

    // Additional tests will be added once reference_panel.db is available
}
