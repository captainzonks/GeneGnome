// ==============================================================================
// parsers/mod.rs - File parser modules
// ==============================================================================
// Description: Parsers for genetic data file formats
// Author: Matt Barham
// Created: 2025-11-03
// Modified: 2025-11-06
// Version: 1.2.0
// ==============================================================================

pub mod genome23andme;
pub mod pgs;
pub mod vcf;

pub use genome23andme::{Genome23Parser, Genome23Record};
pub use pgs::{PgsDataset, PgsParseError, PgsParser, PgsRecord, PgsStats};
pub use vcf::{VCFParseError, VCFParser, VCFRecord};
