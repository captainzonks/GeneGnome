# Roadmap

<!--
==============================================================================
ROADMAP.md - Known defects and planned work
==============================================================================
Description: What is wrong or missing in GeneGnome today, in priority order
Author: Matt Barham
Created: 2026-11-26
Modified: 2026-11-26
Version: 1.0.0
==============================================================================
Document Type: Reference
Audience: Developer, Operator
Status: Active
==============================================================================
-->

Items are in priority order. Released changes are listed in
[CHANGELOG.md](CHANGELOG.md).

## 1. Merge correctness

- [ ] Match 23andMe genotypes by position and then compare alleles, so that
      homozygous and reverse-ordered calls count as genotyped
- [ ] Mark a variant with no user data as missing instead of `0|0`
- [ ] Apply strand-flip handling in the worker merge
- [ ] Decide whether the quality threshold should use the user's imputation R²
      as well as the reference panel's
- [ ] Golden-file tests for the merge, with synthetic inputs and expected output

## 2. Data handling

- [ ] State in [PRIVACY.md](PRIVACY.md) what overwriting can and cannot
      guarantee on SSDs and copy-on-write filesystems
- [ ] Overwrite files when the API deletes an unfinished job
- [ ] Measure the cost of overwriting full-size inputs
- [ ] Write processing events to the audit table from the API and worker
- [ ] Clean up abandoned uploads and jobs that never started
- [ ] Limit what a job ID alone allows, including using up that job's guess
      allowance
- [ ] Verify at startup that the data directory is the encrypted volume

## 3. Demonstrability

- [ ] Synthetic reference panel and inputs in the repository, so the whole
      pipeline runs from a clone without genetic data
- [ ] Screenshots of the upload, progress and visualization pages
- [ ] A committed benchmark that reproduces the performance figures

## 4. Input validation

- [ ] Reject inputs that are not GRCh37/hg19
- [ ] Report unsupported chromosomes clearly

## 5. Polygenic scores

Uploaded score tables are passed through to the results. Calculation is not
implemented.

- [ ] Calculate scores from PGS Catalog weight files
- [ ] Validate the calculation against an established tool
- [ ] Report z-scores only where the comparison group is large enough to mean
      something

## 6. Smaller defects

- [ ] Job status endpoint reports completed jobs as queued
- [ ] Processing progress is reported as a fixed 50% by the status endpoint
- [ ] Output file hashes are stored as a placeholder
- [ ] Remove the older pipeline in `app/src/processor.rs`
- [ ] Take the public hostnames out of the Traefik rules in `traefik/`
