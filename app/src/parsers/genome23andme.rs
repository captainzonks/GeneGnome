// ==============================================================================
// genome23andme.rs - 23andMe Raw Data Parser
// ==============================================================================
// Description: Parser for 23andMe raw genome data files
// Author: Matt Barham
// Created: 2025-11-04
// Modified: 2025-11-04
// Version: 1.0.0
// ==============================================================================
// Format: Tab-delimited text with header comments
// Example:
//   # rsid    chromosome    position    genotype
//   rs548049170    1    69869    TT
//   rs13328684    1    74792    --
//   rs9283150    1    565508    AA
// ==============================================================================

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use thiserror::Error;

/// 23andMe genome record
#[derive(Debug, Clone, PartialEq)]
pub struct Genome23Record {
    /// SNP identifier (e.g., "rs548049170")
    pub rsid: String,
    /// Chromosome ("1"-"22", "X", "Y", "MT")
    pub chromosome: String,
    /// Base pair position (GRCh37/hg19)
    pub position: u64,
    /// Two-letter genotype (e.g., "TT", "AG", "--" for no-call)
    pub genotype: String,
}

/// Parser for 23andMe raw genome files
#[derive(Debug, Clone)]
pub struct Genome23Parser {
    /// Chromosomes to include (e.g., vec!["1", "2", ..., "22"])
    /// If empty, includes all chromosomes
    pub include_chromosomes: Vec<String>,
}

/// Errors that can occur during 23andMe file parsing
#[derive(Error, Debug)]
pub enum Genome23ParseError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid line format at line {line}: {details}")]
    InvalidFormat { line: usize, details: String },

    #[error("Invalid position value at line {line}: {value}")]
    InvalidPosition { line: usize, value: String },

    #[error("File is empty or contains only comments")]
    EmptyFile,
}

impl Default for Genome23Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Genome23Parser {
    /// Create a new parser that includes all chromosomes
    pub fn new() -> Self {
        Self {
            include_chromosomes: Vec::new(),
        }
    }

    /// Create a parser that only includes autosomal chromosomes (1-22)
    pub fn autosomal_only() -> Self {
        Self {
            include_chromosomes: (1..=22).map(|n| n.to_string()).collect(),
        }
    }

    /// Create a parser with specific chromosomes to include
    pub fn with_chromosomes(chromosomes: Vec<String>) -> Self {
        Self {
            include_chromosomes: chromosomes,
        }
    }

    /// Parse a 23andMe genome file
    ///
    /// # Arguments
    /// * `path` - Path to the 23andMe raw data file (genome_*.txt)
    ///
    /// # Returns
    /// * `Ok(Vec<Genome23Record>)` - Successfully parsed records
    /// * `Err(Genome23ParseError)` - Parse error
    ///
    /// # Format
    /// The file is tab-delimited with 4 columns:
    /// - rsid: SNP identifier
    /// - chromosome: Chromosome number or name
    /// - position: Base pair position (GRCh37)
    /// - genotype: Two-letter genotype or "--" for no-call
    ///
    /// Lines starting with '#' are treated as comments and skipped.
    pub fn parse(&self, path: impl AsRef<Path>) -> Result<Vec<Genome23Record>, Genome23ParseError> {
        let file = File::open(path.as_ref())?;
        let reader = BufReader::new(file);

        let mut records = Vec::new();
        let mut line_number = 0;

        for line_result in reader.lines() {
            line_number += 1;
            let line = line_result?;

            // Skip comment lines (start with '#')
            if line.trim().starts_with('#') || line.trim().is_empty() {
                continue;
            }

            let record = self.parse_line(&line, line_number)?;

            // Filter by chromosome if specified
            if !self.include_chromosomes.is_empty()
                && !self.include_chromosomes.contains(&record.chromosome)
            {
                continue;
            }

            records.push(record);
        }

        if records.is_empty() {
            return Err(Genome23ParseError::EmptyFile);
        }

        Ok(records)
    }

    /// Parse a single line from the 23andMe file
    fn parse_line(&self, line: &str, line_number: usize) -> Result<Genome23Record, Genome23ParseError> {
        let fields: Vec<&str> = line.split('\t').collect();

        if fields.len() != 4 {
            return Err(Genome23ParseError::InvalidFormat {
                line: line_number,
                details: format!("Expected 4 tab-delimited fields, found {}", fields.len()),
            });
        }

        let rsid = fields[0].trim().to_string();
        let chromosome = fields[1].trim().to_string();
        let position_str = fields[2].trim();
        let genotype = fields[3].trim().to_string();

        // Parse position
        let position = position_str.parse::<u64>().map_err(|_| {
            Genome23ParseError::InvalidPosition {
                line: line_number,
                value: position_str.to_string(),
            }
        })?;

        Ok(Genome23Record {
            rsid,
            chromosome,
            position,
            genotype,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Create a temporary test file with sample 23andMe data
    fn create_test_file(contents: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_parse_valid_file() {
        let contents = "\
# rsid\tchromosome\tposition\tgenotype
rs548049170\t1\t69869\tTT
rs13328684\t1\t74792\t--
rs9283150\t1\t565508\tAA
rs12345678\t2\t100000\tAG
";
        let file = create_test_file(contents);
        let parser = Genome23Parser::new();

        let records = parser.parse(file.path()).unwrap();

        assert_eq!(records.len(), 4);

        // Check first record
        assert_eq!(records[0].rsid, "rs548049170");
        assert_eq!(records[0].chromosome, "1");
        assert_eq!(records[0].position, 69869);
        assert_eq!(records[0].genotype, "TT");

        // Check no-call record
        assert_eq!(records[1].genotype, "--");

        // Check chromosome 2 record
        assert_eq!(records[3].chromosome, "2");
        assert_eq!(records[3].genotype, "AG");
    }

    #[test]
    fn test_parse_with_chromosome_filter() {
        let contents = "\
# rsid\tchromosome\tposition\tgenotype
rs548049170\t1\t69869\tTT
rs12345678\t2\t100000\tAG
rs98765432\t3\t200000\tCC
";
        let file = create_test_file(contents);
        let parser = Genome23Parser::with_chromosomes(vec!["1".to_string(), "3".to_string()]);

        let records = parser.parse(file.path()).unwrap();

        // Should only include chr1 and chr3, not chr2
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].chromosome, "1");
        assert_eq!(records[1].chromosome, "3");
    }

    #[test]
    fn test_autosomal_only() {
        let contents = "\
# rsid\tchromosome\tposition\tgenotype
rs548049170\t1\t69869\tTT
rs12345678\tX\t100000\tAG
rs98765432\t22\t200000\tCC
rs11111111\tY\t300000\tTT
rs22222222\tMT\t400000\tAA
";
        let file = create_test_file(contents);
        let parser = Genome23Parser::autosomal_only();

        let records = parser.parse(file.path()).unwrap();

        // Should only include chr1 and chr22, not X, Y, MT
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].chromosome, "1");
        assert_eq!(records[1].chromosome, "22");
    }

    #[test]
    fn test_invalid_format_too_few_fields() {
        let contents = "\
# rsid\tchromosome\tposition\tgenotype
rs548049170\t1\t69869
";
        let file = create_test_file(contents);
        let parser = Genome23Parser::new();

        let result = parser.parse(file.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            Genome23ParseError::InvalidFormat { line, .. } => {
                assert_eq!(line, 2); // Line 1 is comment
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_invalid_position() {
        let contents = "\
# rsid\tchromosome\tposition\tgenotype
rs548049170\t1\tNOT_A_NUMBER\tTT
";
        let file = create_test_file(contents);
        let parser = Genome23Parser::new();

        let result = parser.parse(file.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            Genome23ParseError::InvalidPosition { line, value } => {
                assert_eq!(line, 2);
                assert_eq!(value, "NOT_A_NUMBER");
            }
            _ => panic!("Expected InvalidPosition error"),
        }
    }

    #[test]
    fn test_empty_file() {
        let contents = "\
# rsid\tchromosome\tposition\tgenotype
# Just comments, no data
";
        let file = create_test_file(contents);
        let parser = Genome23Parser::new();

        let result = parser.parse(file.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            Genome23ParseError::EmptyFile => {}
            _ => panic!("Expected EmptyFile error"),
        }
    }

    #[test]
    fn test_whitespace_handling() {
        let contents = "\
# rsid\tchromosome\tposition\tgenotype
  rs548049170  \t  1  \t  69869  \t  TT
";
        let file = create_test_file(contents);
        let parser = Genome23Parser::new();

        let records = parser.parse(file.path()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].rsid, "rs548049170");
        assert_eq!(records[0].chromosome, "1");
        assert_eq!(records[0].position, 69869);
        assert_eq!(records[0].genotype, "TT");
    }

    #[test]
    fn test_mixed_chromosomes() {
        let contents = "\
# rsid\tchromosome\tposition\tgenotype
rs1\t1\t100\tAA
rs2\tX\t200\tXY
rs3\t10\t300\tGG
rs4\tY\t400\tTT
rs5\t22\t500\tCC
rs6\tMT\t600\tAA
";
        let file = create_test_file(contents);
        let parser = Genome23Parser::new();

        let records = parser.parse(file.path()).unwrap();
        assert_eq!(records.len(), 6);

        // Verify chromosome values are preserved correctly
        assert_eq!(records[0].chromosome, "1");
        assert_eq!(records[1].chromosome, "X");
        assert_eq!(records[2].chromosome, "10");
        assert_eq!(records[3].chromosome, "Y");
        assert_eq!(records[4].chromosome, "22");
        assert_eq!(records[5].chromosome, "MT");
    }
}
