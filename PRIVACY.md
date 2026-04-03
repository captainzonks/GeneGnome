# GeneGnome Privacy Policy

**Last updated:** April 3, 2026

GeneGnome is an open-source genetic data processing platform. This policy explains how your data is handled when you use the service.

## What Data We Collect

### VCF Generator (Client-Side)

The VCF Generator runs **entirely in your browser** using WebAssembly. Your 23andMe raw data file is never uploaded to our servers. No data is collected, transmitted, or stored.

### Genetics Processor (Server-Side)

When you submit a processing job, we temporarily store:

- **Your uploaded files** (23andMe raw data, imputed VCF files)
- **Your email address** (used solely to send the download link)
- **Job metadata** (job ID, status, timestamps)

We do **not** store your name, account information, IP address logs, or any other personal identifiers beyond your email address.

## How Your Data Is Protected

### Encryption at Rest

All uploaded and processed files are stored on a **LUKS AES-256-XTS encrypted volume**. Data is only accessible while the volume is mounted by the processing system.

### Secure File Deletion

After processing completes, uploaded input files are **securely wiped** using DoD 5220.22-M compliant overwrite patterns (multiple passes of random data followed by verification). This goes beyond simple file deletion — the original data is irrecoverable.

### Password-Protected Downloads

Results are delivered via a secure, password-protected download link sent to your email. Download links:

- Expire after **24 hours**
- Allow a maximum of **5 password attempts**
- Use cryptographically random tokens and passwords

### Network Isolation

The processing worker and job queue run on an **isolated network** with no internet access. Only the API gateway and frontend are publicly accessible.

## Data Retention

| Data | Retention |
|------|-----------|
| Uploaded input files | Securely wiped immediately after processing |
| Processed results (ZIP) | **24 hours** from job completion, then permanently deleted |
| Job metadata (ID, status, timestamps) | Deleted with the job |
| Email address | Deleted with the job |
| Recovery code hashes | Deleted with the job (CASCADE) |

Jobs deleted via self-service (using recovery codes) are cleaned up on the next hourly maintenance sweep.

## Self-Service Data Control

You have full control over your data:

- **Recovery codes** are provided at job submission (shown once, never stored in plaintext)
- **Email resend** lets you request a new download link if the original is lost
- **Self-service deletion** lets you permanently delete all your data at any time using a recovery code, without needing email access

## What We Do NOT Do

- We do **not** retain copies of your genomic data after expiration
- We do **not** sell, share, or transfer your data to third parties
- We do **not** use your data for research, analytics, or any purpose other than processing your job
- We do **not** track users across sessions or use cookies for analytics
- We do **not** log or store IP addresses

## Open Source Transparency

GeneGnome is fully open source. You can verify every privacy claim by reviewing the source code:

- [Secure deletion implementation](https://github.com/captainzonks/GeneGnome/blob/main/app/src/secure_delete.rs)
- [Job cleanup and expiration](https://github.com/captainzonks/GeneGnome/blob/main/worker/src/main.rs)
- [Encrypted volume setup](https://github.com/captainzonks/GeneGnome/blob/main/scripts/setup_encrypted_volume.sh)
- [Network isolation](https://github.com/captainzonks/GeneGnome/blob/main/docker-compose.yml)

## Contact

For privacy questions or concerns, open an issue on the [GeneGnome GitHub repository](https://github.com/captainzonks/GeneGnome/issues).
