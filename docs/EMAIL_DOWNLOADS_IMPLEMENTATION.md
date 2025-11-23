# Email-Based Download Implementation Plan

**Created:** 2025-11-18
**Status:** Planning
**Version:** 1.0.0
**Target:** genetics v1.0.0

---

## Overview

Implement secure, email-based download system for genetics processing results to replace direct download links. Downloads are secured via password-protected tokens sent via email, eliminating need for Authentik authentication while maintaining security.

## Security Model

### Current (Insecure)
- ❌ Anyone with job ID can download results
- ❌ No authentication required
- ❌ Results accessible indefinitely

### Target (Secure)
- ✅ User provides email address when submitting job
- ✅ Results ready → email sent with download token + password
- ✅ Download requires both token and password
- ✅ Token single-use or time-limited (24 hours)
- ✅ Rate limiting on download attempts

---

## Database Schema Changes

### 1. Update `genetics_jobs` Table

**New columns:**
```sql
ALTER TABLE genetics_jobs
ADD COLUMN user_email TEXT NOT NULL DEFAULT 'noreply@example.com',
ADD COLUMN download_token VARCHAR(64) UNIQUE,
ADD COLUMN download_password_hash VARCHAR(128),
ADD COLUMN download_attempts INTEGER DEFAULT 0,
ADD COLUMN download_max_attempts INTEGER DEFAULT 5,
ADD COLUMN token_expires_at TIMESTAMPTZ,
ADD COLUMN downloaded_at TIMESTAMPTZ,
ADD COLUMN email_sent_at TIMESTAMPTZ,
ADD COLUMN email_delivery_status TEXT CHECK (email_delivery_status IN ('pending', 'sent', 'failed', 'bounced'));
```

**Indexes:**
```sql
CREATE INDEX idx_genetics_jobs_download_token ON genetics_jobs(download_token) WHERE download_token IS NOT NULL;
CREATE INDEX idx_genetics_jobs_token_expires ON genetics_jobs(token_expires_at) WHERE token_expires_at IS NOT NULL;
CREATE INDEX idx_genetics_jobs_email ON genetics_jobs(user_email);
```

### 2. Create Download Audit Log

**New table:**
```sql
CREATE TABLE genetics_download_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES genetics_jobs(id) ON DELETE CASCADE,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    success BOOLEAN NOT NULL,
    failure_reason TEXT,
    ip_address INET,
    user_agent TEXT,
    provided_token TEXT,
    provided_password_valid BOOLEAN
);

CREATE INDEX idx_genetics_download_attempts_job_id ON genetics_download_attempts(job_id);
CREATE INDEX idx_genetics_download_attempts_attempted_at ON genetics_download_attempts(attempted_at DESC);
CREATE INDEX idx_genetics_download_attempts_ip ON genetics_download_attempts(ip_address);
```

---

## Email Configuration

### Environment Variables (genetics.env)

```bash
# Email Configuration (protonmail-bridge)
GENETICS_SMTP_HOST=protonmail-bridge
GENETICS_SMTP_PORT=587
GENETICS_SMTP_USERNAME=your-email@example.com
GENETICS_SMTP_FROM=genetics@your-domain.com
GENETICS_SMTP_FROM_NAME=GeneGnome Genetics Processor
GENETICS_SMTP_USE_TLS=false  # Internal Docker network
GENETICS_SMTP_TIMEOUT=30

# Download Security
GENETICS_DOWNLOAD_TOKEN_LENGTH=32  # bytes (64 hex chars)
GENETICS_DOWNLOAD_TOKEN_EXPIRY_HOURS=24
GENETICS_DOWNLOAD_MAX_ATTEMPTS=5
GENETICS_PASSWORD_LENGTH=16  # Generated password length
```

### Docker Secrets

Create new secret for SMTP password:
```bash
echo "your-smtp-password" > secrets/genetics_smtp_password
```

Update docker-compose.yml:
```yaml
secrets:
  genetics_smtp_password:
    file: ${DOCKERDIR}/secrets/genetics/genetics_smtp_password
```

---

## Rust Dependencies

### Add to `api-gateway/Cargo.toml`:

```toml
# Email sending
lettre = "0.11"
lettre_email = "0.9"

# Password hashing
argon2 = "0.5"

# Random generation
rand = "0.8"

# HTML templates (already has)
# tera or handlebars for email templates
```

---

## API Changes

### 1. Job Submission Endpoint

**Before:**
```json
POST /api/genetics/jobs
{
  "user_id": "anonymous",
  "formats": ["parquet", "vcf"]
}
```

**After:**
```json
POST /api/genetics/jobs
{
  "user_email": "user@example.com",
  "formats": ["parquet", "vcf"]
}
```

**Validation:**
- Email format validation
- Disposable email detection (optional)
- Rate limiting per email (e.g., 5 jobs/day)

### 2. New Download Endpoint

**Old (insecure):**
```
GET /api/genetics/results/{job_id}
```

**New (secure):**
```
GET /api/genetics/download/{token}?password={password}
```

**Flow:**
1. Client receives email with token + password
2. Client requests download with both credentials
3. API validates:
   - Token exists and not expired
   - Password matches hash
   - Max attempts not exceeded
   - Job status is 'completed'
4. On success:
   - Mark as downloaded
   - Increment download count
   - Log attempt (success)
5. On failure:
   - Increment attempt counter
   - Log attempt (failure)
   - Return 401 if max attempts exceeded

### 3. Results Ready Notification

**Trigger:** Worker marks job as 'completed'

**Process:**
1. Generate secure random token (32 bytes → 64 hex chars)
2. Generate random password (16 chars, alphanumeric + symbols)
3. Hash password with Argon2id
4. Store token, hash, expiry in database
5. Send email with download link + password
6. Mark email_sent_at timestamp

---

## Email Templates

### Location
```
stacks/genetics/api-gateway/src/templates/emails/
├── results_ready.html    # HTML email
└── results_ready.txt     # Plain text fallback
```

### HTML Template (`results_ready.html`)

```html
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body { font-family: Arial, sans-serif; line-height: 1.6; color: #333; }
        .container { max-width: 600px; margin: 0 auto; padding: 20px; }
        .header { background: #4CAF50; color: white; padding: 20px; text-align: center; }
        .content { background: #f9f9f9; padding: 20px; border: 1px solid #ddd; }
        .credential-box { background: #fff; border: 2px solid #4CAF50; padding: 15px; margin: 15px 0; }
        .credential-label { font-weight: bold; color: #4CAF50; }
        .credential-value { font-family: monospace; font-size: 1.1em; background: #f4f4f4; padding: 8px; margin: 5px 0; word-break: break-all; }
        .button { display: inline-block; background: #4CAF50; color: white; padding: 12px 30px; text-decoration: none; border-radius: 4px; margin: 15px 0; }
        .warning { background: #fff3cd; border-left: 4px solid #ffc107; padding: 10px; margin: 15px 0; }
        .footer { text-align: center; color: #666; font-size: 0.9em; margin-top: 20px; }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🧬 Your Genetics Results Are Ready!</h1>
        </div>

        <div class="content">
            <p>Hello,</p>

            <p>Your genetic data processing job has completed successfully!</p>

            <p><strong>Job Details:</strong></p>
            <ul>
                <li>Job ID: <code>{{ job_id }}</code></li>
                <li>Submitted: {{ submitted_at }}</li>
                <li>Completed: {{ completed_at }}</li>
                <li>Processing Time: {{ processing_time }}</li>
                <li>Output Formats: {{ formats }}</li>
            </ul>

            <div class="warning">
                <strong>⚠️ Security Information:</strong><br>
                Your results are protected with a secure password. You'll need BOTH the download link and password to access your data.
            </div>

            <div class="credential-box">
                <div class="credential-label">Download Link:</div>
                <div class="credential-value">{{ download_url }}</div>

                <div class="credential-label" style="margin-top: 15px;">Password:</div>
                <div class="credential-value">{{ password }}</div>
            </div>

            <p style="text-align: center;">
                <a href="{{ download_url }}" class="button">Download Results</a>
            </p>

            <p><strong>Important:</strong></p>
            <ul>
                <li>This download link expires in <strong>24 hours</strong></li>
                <li>You have <strong>5 download attempts</strong> (failed password attempts count)</li>
                <li>After successful download, the link remains valid until expiration</li>
                <li>Keep this email secure - it contains your access credentials</li>
            </ul>

            <p><strong>File Sizes:</strong></p>
            <ul>
                <li>Parquet: ~436 MB (if selected)</li>
                <li>VCF (gzipped): ~243 MB (if selected)</li>
                <li>SQLite: ~1.3 GB (if selected)</li>
            </ul>

            <p>If you didn't request this, you can safely ignore this email. The download link will expire automatically.</p>
        </div>

        <div class="footer">
            <p>GeneGnome Genetics Processor | your-domain.com</p>
            <p>This is an automated message. Please do not reply to this email.</p>
        </div>
    </div>
</body>
</html>
```

### Plain Text Template (`results_ready.txt`)

```text
Your Genetics Results Are Ready!

Hello,

Your genetic data processing job has completed successfully!

Job Details:
- Job ID: {{ job_id }}
- Submitted: {{ submitted_at }}
- Completed: {{ completed_at }}
- Processing Time: {{ processing_time }}
- Output Formats: {{ formats }}

DOWNLOAD CREDENTIALS:
--------------------
Download Link: {{ download_url }}
Password: {{ password }}

IMPORTANT SECURITY INFORMATION:
- This download link expires in 24 hours
- You have 5 download attempts (failed password attempts count)
- Keep this email secure - it contains your access credentials

To download your results:
1. Click the download link (or copy to your browser)
2. Enter the password when prompted
3. Your files will begin downloading

File Sizes:
- Parquet: ~436 MB (if selected)
- VCF (gzipped): ~243 MB (if selected)
- SQLite: ~1.3 GB (if selected)

If you didn't request this, you can safely ignore this email.
The download link will expire automatically.

---
GeneGnome Genetics Processor | your-domain.com
This is an automated message. Please do not reply to this email.
```

---

## Implementation Phases

### Phase 1: Database Schema (v1.0.0-alpha)
- [ ] Create migration script (init.sql v1.1.0)
- [ ] Add email columns to genetics_jobs
- [ ] Create genetics_download_attempts table
- [ ] Test migration on dev database

### Phase 2: Email Infrastructure (v1.0.0-alpha)
- [ ] Add email dependencies to Cargo.toml
- [ ] Create email service module (email.rs)
- [ ] Implement SMTP connection with protonmail-bridge
- [ ] Create email templates (HTML + text)
- [ ] Add email configuration to .env

### Phase 3: Token Generation (v1.0.0-beta)
- [ ] Implement secure token generation (32 bytes random)
- [ ] Implement secure password generation (16 chars)
- [ ] Implement Argon2id password hashing
- [ ] Add token validation logic
- [ ] Add rate limiting per email

### Phase 4: API Updates (v1.0.0-beta)
- [ ] Update job submission to require email
- [ ] Implement new /download/{token} endpoint
- [ ] Add password verification
- [ ] Add attempt tracking and max attempts
- [ ] Add download audit logging

### Phase 5: Worker Integration (v1.0.0-beta)
- [ ] Update worker to generate tokens on completion
- [ ] Trigger email sending on job completion
- [ ] Handle email delivery failures (retry logic)

### Phase 6: Frontend Updates (v1.0.0-rc)
- [ ] Add email input field to upload form
- [ ] Update success message (check email for download)
- [ ] Remove direct download buttons
- [ ] Add download page with password prompt

### Phase 7: Testing & Security Audit (v1.0.0-rc)
- [ ] Unit tests for token generation
- [ ] Unit tests for password hashing/verification
- [ ] Integration tests for email delivery
- [ ] Security testing (brute force, token guessing)
- [ ] Rate limiting tests
- [ ] Email deliverability tests

### Phase 8: Production Deployment (v1.0.0)
- [ ] Deploy database migration
- [ ] Deploy updated API gateway
- [ ] Deploy updated worker
- [ ] Deploy updated frontend
- [ ] Monitor email delivery rates
- [ ] Monitor download success rates

---

## Security Considerations

### Token Generation
- Use cryptographically secure random (rand::CryptoRng)
- 32 bytes (256 bits) provides ~10^77 possible tokens
- Base64 or hex encoding for URL safety

### Password Generation
- 16 characters: a-zA-Z0-9 + symbols (!@#$%^&*)
- Entropy: ~95^16 ≈ 2^105 combinations
- Exclude ambiguous characters (0/O, 1/l/I)

### Password Storage
- Argon2id (memory-hard, side-channel resistant)
- Recommended params: m=19456, t=2, p=1
- Never log or transmit plaintext passwords

### Rate Limiting
- Per email: 5 jobs per day
- Per IP: 50 requests per hour
- Per token: 5 failed attempts → permanent block

### Email Security
- Validate email format (RFC 5322)
- Optional: Check disposable email providers
- Optional: Verify MX records
- Log all email delivery attempts

### Download Security
- Constant-time password comparison
- Log all download attempts (success + failure)
- Block after max attempts exceeded
- Expire tokens after 24 hours
- Optional: Require CAPTCHA after 3 failed attempts

---

## Testing Checklist

### Unit Tests
- [ ] Token generation (uniqueness, length, format)
- [ ] Password generation (complexity, no ambiguous chars)
- [ ] Password hashing (Argon2id params)
- [ ] Password verification (correct + incorrect)
- [ ] Email template rendering
- [ ] Token expiration logic

### Integration Tests
- [ ] Job submission with email
- [ ] Email delivery via protonmail-bridge
- [ ] Download with valid token + password
- [ ] Download with invalid token
- [ ] Download with invalid password
- [ ] Download with expired token
- [ ] Max attempts enforcement
- [ ] Rate limiting enforcement

### Security Tests
- [ ] Token guessing resistance (256-bit entropy)
- [ ] Password brute force resistance (max attempts)
- [ ] Timing attack resistance (constant-time comparison)
- [ ] SQL injection (parameterized queries)
- [ ] Email header injection
- [ ] CSRF protection (not applicable for API)

---

## Monitoring & Alerts

### Metrics to Track
- Email delivery rate (sent / total)
- Email bounce rate
- Download success rate
- Failed download attempts (per token)
- Average time from completion to download
- Token expiration without download (abandoned jobs)

### Alerts
- Email delivery failure rate > 5%
- Download failure rate > 10%
- Unusual download attempt patterns (brute force)
- High number of expired tokens (UX issue?)

---

## Migration Plan

### Step 1: Deploy Schema Changes
```bash
# Backup database
pg_dump genetics > genetics_backup_$(date +%Y%m%d).sql

# Apply migration
psql -U genetics_api -d genetics < database/migrations/001_add_email_downloads.sql

# Verify
psql -U genetics_api -d genetics -c "\d genetics_jobs"
psql -U genetics_api -d genetics -c "\d genetics_download_attempts"
```

### Step 2: Deploy API Gateway (Blue-Green)
1. Build new image with email support
2. Deploy alongside existing gateway
3. Route test traffic to new version
4. Verify email delivery
5. Switch production traffic
6. Monitor for issues
7. Decommission old version

### Step 3: Update Frontend
1. Add email input field
2. Update success messaging
3. Deploy to test environment
4. User acceptance testing
5. Deploy to production

---

## Rollback Plan

### If Email Delivery Fails
1. Revert to direct downloads (temporary)
2. Generate tokens but don't enforce
3. Send emails with direct + token links
4. Debug email delivery issues
5. Re-enable enforcement

### If Database Migration Fails
1. Restore from backup
2. Review migration SQL
3. Test on staging database
4. Re-attempt migration

---

## Future Enhancements (v1.1.0+)

- [ ] Email verification (click link to confirm)
- [ ] SMS delivery option (Twilio)
- [ ] Multi-file zip support (single download)
- [ ] Download resume support (HTTP range requests)
- [ ] Encrypted email attachments (GPG)
- [ ] OAuth login option (Google, GitHub)
- [ ] Web UI for password-protected downloads
- [ ] QR code for mobile downloads
- [ ] Download analytics (format popularity)

---

**Next Steps:**
1. Review and approve this plan
2. Create database migration script
3. Add email dependencies to Cargo.toml
4. Implement email service module
5. Test email delivery with protonmail-bridge

**Estimated Timeline:**
- Phase 1-2: 2 days (database + email infrastructure)
- Phase 3-4: 3 days (tokens + API)
- Phase 5-6: 2 days (worker + frontend)
- Phase 7-8: 3 days (testing + deployment)
- **Total: ~10 days** for v1.0.0 release

---

**Author**: Matt Barham (via Claude Code)
**Date**: 2025-11-18
**Status**: Awaiting Approval
