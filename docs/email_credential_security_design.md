# Email-Based Credential Security System Design

## Overview
Implement Michigan Imputation Server-style email credential delivery for secure job access with encrypted data at rest.

## Components

### 1. Database Schema Updates

```sql
-- Add to genetics_jobs table
ALTER TABLE genetics_jobs ADD COLUMN password_hash TEXT NOT NULL;
ALTER TABLE genetics_jobs ADD COLUMN access_token_hash TEXT NOT NULL;
ALTER TABLE genetics_jobs ADD COLUMN user_email TEXT NOT NULL;
ALTER TABLE genetics_jobs ADD COLUMN encryption_salt BYTEA NOT NULL;  -- For file encryption
ALTER TABLE genetics_jobs ADD COLUMN created_at TIMESTAMP DEFAULT NOW();
```

### 2. Credential Generation

**On Job Creation:**
1. Generate strong random password (20 chars: alphanumeric + symbols)
2. Generate access token (32 bytes = 64 hex chars)
3. Hash both with Argon2id before storing
4. Derive encryption key from password using Argon2id with unique salt
5. Store hashes and salt in database

**Libraries:**
- `argon2` crate for password hashing
- `rand` crate for secure random generation
- `aes-gcm` crate for file encryption

### 3. Email Notification

**SMTP Configuration:**
- Host: `protonmail-bridge` (internal Docker network)
- Port: `1026`
- From: `genetics@your-domain.com`
- Auth: your-email@example.com credentials

**Email Template:**
```
Subject: Your Genomic Analysis Job is Processing - Credentials Inside

Hello,

Your genomic analysis job has been queued successfully!

Job ID: {job_id}
Status: Processing

IMPORTANT - Save these credentials:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Password: {password}
Access Token: {token}

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Download URL: https://your-domain.com/api/genetics/results/{job_id}

You will need either your password OR access token to download your results.

SECURITY NOTES:
✓ This email contains sensitive credentials - delete after saving
✓ Your data is encrypted at rest and will be automatically deleted after 24 hours
✓ Only you have access to this job using the credentials above
✓ If you lose your credentials, your data cannot be recovered

Questions? Reply to this email.

Best regards,
GeneGnome Genetics Platform
```

### 4. File Encryption

**Encryption Strategy:**
- Use AES-256-GCM for authenticated encryption
- Derive key from password using Argon2id:
  - Time cost: 3
  - Memory cost: 64 MiB
  - Parallelism: 4
  - Salt: unique per job (stored in DB)
- Encrypt files after generation, before storage
- Store: `{filename}.encrypted` + `{filename}.nonce`

**Files to Encrypt:**
- SQLite database (5-10GB)
- VCF.gz file (4.8GB)
- Parquet file (436MB)

### 5. Download Authentication

**API Endpoint:**
```
GET /results/{job_id}?format={format}
Headers:
  Authorization: Bearer {token}
  OR
  X-Job-Password: {password}
```

**Authentication Flow:**
1. Extract token from `Authorization` header OR password from `X-Job-Password`
2. Query database for job's `access_token_hash` and `password_hash`
3. Verify provided credential matches either hash
4. If valid:
   - Retrieve encryption salt from DB
   - Derive decryption key from password (if password provided) OR use token
   - Decrypt file on-the-fly
   - Stream to client
5. If invalid: Return 401 Unauthorized

### 6. Security Features

**Defense in Depth:**
- ✓ Credentials never stored in plaintext
- ✓ Files encrypted at rest with AES-256-GCM
- ✓ Argon2id resistant to GPU cracking
- ✓ Unique salt per job
- ✓ 24-hour auto-deletion
- ✓ Credentials delivered via encrypted email (ProtonMail)
- ✓ Two authentication factors (password OR token)
- ✓ No public access via URL alone

**Compliance:**
- GDPR: User data encrypted, auto-deleted, no tracking
- HIPAA-adjacent: Genetic data treated as sensitive PHI
- CCPA: User controls their data, can request deletion

### 7. Implementation Plan

**Phase 1: Database & Credential Generation**
- [ ] Add database migration for new columns
- [ ] Implement password/token generation
- [ ] Implement Argon2id hashing

**Phase 2: Email Integration**
- [ ] Add `lettre` crate for SMTP
- [ ] Create email template
- [ ] Send credentials on job creation
- [ ] Test with protonmail-bridge

**Phase 3: File Encryption**
- [ ] Add `aes-gcm` crate
- [ ] Implement encryption after file generation
- [ ] Store encrypted files + nonces
- [ ] Test encryption/decryption roundtrip

**Phase 4: Download Authentication**
- [ ] Add authentication middleware
- [ ] Verify password OR token
- [ ] Decrypt files on-the-fly during download
- [ ] Return 401 for invalid credentials

**Phase 5: Frontend Updates**
- [ ] Remove direct download links
- [ ] Add password/token input form
- [ ] Update UI to show "credentials sent via email"
- [ ] Add download button that sends credentials

### 8. Libraries Needed

Add to `Cargo.toml`:
```toml
[dependencies]
# Password hashing (Argon2id)
argon2 = "0.5"

# Encryption (AES-256-GCM)
aes-gcm = "0.10"

# Random generation
rand = "0.8"

# Email sending
lettre = "0.11"

# Async runtime for email
tokio = { version = "1", features = ["full"] }
```

### 9. Environment Variables

Add to `genetics.env`:
```bash
# Email Configuration
SMTP_HOST=protonmail-bridge
SMTP_PORT=1026
SMTP_USERNAME=your-email@example.com
SMTP_FROM=genetics@your-domain.com
SMTP_PASSWORD_FILE=/run/secrets/proton_bridge_password

# Security
ARGON2_TIME_COST=3
ARGON2_MEMORY_COST=65536  # 64 MiB
ARGON2_PARALLELISM=4
```

### 10. Testing Strategy

**Unit Tests:**
- Password generation strength
- Argon2 hashing verification
- Token generation uniqueness
- Encryption/decryption roundtrip

**Integration Tests:**
- Email sending via protonmail-bridge
- Full job creation → email → download flow
- Authentication with valid/invalid credentials
- File decryption on download

**Security Tests:**
- Timing attack resistance (constant-time comparison)
- Brute force protection (rate limiting)
- SQL injection prevention (parameterized queries)

---

## Migration Path

**Existing Jobs:**
- No retroactive encryption (would need password)
- Continue 24-hour deletion
- New security applies to jobs created after deployment

**Deployment:**
1. Run database migration
2. Deploy updated API gateway + worker
3. Test with small test file
4. Monitor email delivery
5. Full rollout

---

## Future Enhancements

- [ ] Optional: Allow user to set custom password (vs auto-generated)
- [ ] Optional: 2FA via SMS/TOTP for high-security users
- [ ] Optional: GPG encryption for email body
- [ ] Optional: Web3 wallet authentication
- [ ] Monitoring: Track failed authentication attempts
- [ ] Audit log: Record all downloads with timestamps
