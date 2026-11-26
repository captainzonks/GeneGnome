# Roadmap

<!--
==============================================================================
ROADMAP.md - Known defects and planned work
==============================================================================
Description: What is wrong or missing in GeneGnome today, in priority order
Author: Matt Barham
Created: 2026-11-26
Modified: 2026-11-26
Version: 1.3.0
==============================================================================
Document Type: Reference
Audience: Developer, Operator
Status: Active
==============================================================================
-->

Items are in priority order. Released changes are listed in
[CHANGELOG.md](CHANGELOG.md).

## 1. Merge correctness

- [ ] Decide whether the quality threshold should use the user's imputation R²
      as well as the reference panel's

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

- [ ] The web form offers no SQLite output, which the API supports
- [ ] Genotyped and imputed variant counts reflect the reference panel's
      `TYPED` flag, not where the user's calls came from
- [ ] Processing progress is reported as a fixed 50% by the status endpoint
- [ ] Output file hashes are stored as a placeholder
- [ ] Remove the older pipeline in `app/src/processor.rs`
- [ ] Take the public hostnames out of the Traefik rules in `traefik/`
- [ ] Fix the image health checks: the API image probes the wrong port and path,
      and the frontend image probes `localhost`, which resolves to IPv6 first
