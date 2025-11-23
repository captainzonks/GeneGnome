#!/usr/bin/env Rscript
# ==============================================================================
# convert_reference_to_db.R - Convert R Reference Panel to SQLite Database
# ==============================================================================
# Description: Converts VCF.Files3.RData to reference_panel.db for Rust processor
# Author: Matthew Barham
# Created: 2025-11-12
# Modified: 2025-11-20
# Version: 1.1.0
# ==============================================================================
#
# Purpose:
#   The Rust processor cannot read R binary data files (.RData), so we convert
#   the 50-sample imputed reference panel to SQLite format for efficient lookups.
#
# Input:
#   - VCF.Files3.RData (167 MB) - R binary with vcf.Chr1...vcf.Chr22 objects
#   - Source: http://www.matthewckeller.com/public/VCF.Files3.RData
#
# Output:
#   - reference_panel.db (~4.7 GB) - SQLite database with reference_variants table
#   - Schema: chromosome, position, rsid, alleles, quality metrics, sample genotypes
#
# Usage:
#   # Default paths (expects VCF.Files3.RData in ../reference/)
#   Rscript convert_reference_to_db.R
#
#   # Custom paths via environment variables
#   VCF_INPUT_FILE=/path/to/VCF.Files3.RData \
#   DB_OUTPUT_PATH=/path/to/reference_panel.db \
#   Rscript convert_reference_to_db.R
#
# Requirements:
#   - R 4.0+
#   - Packages: DBI, RSQLite, jsonlite
#   - Install: R -e "install.packages(c('DBI', 'RSQLite', 'jsonlite'))"
#
# ==============================================================================

.libPaths(c("~/R/library", .libPaths()))
library(DBI)
library(RSQLite)
library(jsonlite)

# Load reference panel data
cat("Loading VCF.Files3.RData...\n")
# Adjust this path to where you downloaded VCF.Files3.RData
input_file <- Sys.getenv("VCF_INPUT_FILE", "../reference/VCF.Files3.RData")
if (!file.exists(input_file)) {
  stop("Input file not found: ", input_file, "\nPlease download from http://www.matthewckeller.com/public/VCF.Files3.RData")
}
load(input_file)

# Create SQLite database
db_path <- Sys.getenv("DB_OUTPUT_PATH", "../reference/reference_panel.db")
if (file.exists(db_path)) {
  file.remove(db_path)
}
con <- dbConnect(SQLite(), db_path)

# Create metadata table
cat("Creating metadata table...\n")
dbExecute(con, "
CREATE TABLE metadata (
  key TEXT PRIMARY KEY,
  value TEXT
)")

dbExecute(con, "INSERT INTO metadata VALUES ('source', 'VCF.Files3.RData')")
dbExecute(con, "INSERT INTO metadata VALUES ('description', 'Reference panel - 50 sample anonymized genotypes')")
dbExecute(con, "INSERT INTO metadata VALUES ('num_samples', '50')")
dbExecute(con, "INSERT INTO metadata VALUES ('build', 'GRCh37/hg19')")
dbExecute(con, "INSERT INTO metadata VALUES ('created', ?)", params = list(Sys.time()))

# Create reference variants table
cat("Creating reference_variants table...\n")
dbExecute(con, "
CREATE TABLE reference_variants (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  chromosome INTEGER NOT NULL,
  position INTEGER NOT NULL,
  rsid TEXT,
  ref_allele TEXT NOT NULL,
  alt_allele TEXT NOT NULL,
  phased INTEGER,
  allele_freq REAL,
  minor_allele_freq REAL,
  imputation_quality REAL,
  is_typed INTEGER,
  sample_genotypes TEXT NOT NULL
)")

# Create index for efficient lookups
dbExecute(con, "CREATE INDEX idx_chr_pos ON reference_variants(chromosome, position)")
dbExecute(con, "CREATE INDEX idx_rsid ON reference_variants(rsid)")

# Convert each chromosome
total_variants <- 0
for (chr in 1:22) {
  vcf_name <- paste0("vcf.Chr", chr)
  vcf_data <- get(vcf_name)

  cat(sprintf("Processing Chr%d: %d variants...\n", chr, nrow(vcf_data)))

  # Prepare data for insertion
  sample_cols <- paste0("samp", 1:50)

  # Create JSON for each variant's sample genotypes
  sample_genotypes_list <- apply(vcf_data[, sample_cols], 1, function(row) {
    toJSON(as.list(row), auto_unbox = TRUE)
  })

  # Prepare DataFrame for bulk insert
  insert_data <- data.frame(
    chromosome = chr,
    position = vcf_data$bp37,
    rsid = vcf_data$rsid,
    ref_allele = vcf_data$REF,
    alt_allele = vcf_data$ALT,
    phased = as.integer(vcf_data$PHASED),
    allele_freq = vcf_data$AF,
    minor_allele_freq = vcf_data$MAF,
    imputation_quality = vcf_data$R2,
    is_typed = as.integer(vcf_data$TYPED),
    sample_genotypes = sample_genotypes_list,
    stringsAsFactors = FALSE
  )

  # Bulk insert
  dbWriteTable(con, "reference_variants", insert_data, append = TRUE)

  total_variants <- total_variants + nrow(vcf_data)
}

cat(sprintf("\nTotal variants inserted: %d\n", total_variants))

# Update metadata with total count
dbExecute(con, "INSERT INTO metadata VALUES ('total_variants', ?)", params = list(as.character(total_variants)))

# Verify
cat("\nVerifying database...\n")
result <- dbGetQuery(con, "SELECT chromosome, COUNT(*) as count FROM reference_variants GROUP BY chromosome ORDER BY chromosome")
print(result)

cat("\nDatabase size:\n")
cat(sprintf("File: %s\n", db_path))
cat(sprintf("Size: %.1f MB\n", file.size(db_path) / 1024 / 1024))

dbDisconnect(con)

cat("\nConversion complete!\n")
cat(sprintf("Database saved to: %s\n", db_path))
