// ==============================================================================
// security.rs - Security Functions (Token & Password Generation, Hashing)
// ==============================================================================
// Description: Token generation, password generation, and Argon2id hashing
// Author: Matt Barham
// Created: 2025-11-18
// Modified: 2025-11-18
// Version: 1.0.0
// Phase: Phase 3 - Token & Password Generation
// ==============================================================================

use anyhow::{Context, Result};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2, Algorithm, Params, Version,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::Rng;

// ==============================================================================
// CONSTANTS
// ==============================================================================

/// Token length in bytes (32 bytes = 256 bits)
const TOKEN_BYTES: usize = 32;

/// Password length in characters
const PASSWORD_LENGTH: usize = 16;

/// Password character set (alphanumeric + symbols, excluding ambiguous chars)
const PASSWORD_CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789!@#$%^&*";

/// Recovery code character set (uppercase alphanumeric, no ambiguous chars)
const RECOVERY_CODE_CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Recovery code length in characters (displayed as XXXX-XXXX-XXXX)
const RECOVERY_CODE_LENGTH: usize = 12;

/// Number of recovery codes generated per job
pub const RECOVERY_CODE_COUNT: usize = 8;

// ==============================================================================
// TOKEN GENERATION
// ==============================================================================

/// Generates a cryptographically secure random token for download URLs
///
/// Returns a URL-safe base64-encoded string of 32 random bytes (256 bits of entropy).
/// The resulting string is approximately 43 characters long.
///
/// # Examples
///
/// ```
/// let token = generate_download_token()?;
/// assert_eq!(token.len(), 43); // Base64 URL-safe encoding of 32 bytes
/// ```
///
/// # Errors
///
/// Returns an error if the random number generator fails (extremely rare)
pub fn generate_download_token() -> Result<String> {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; TOKEN_BYTES];
    rng.fill(&mut bytes);

    // Encode to URL-safe base64 without padding
    let token = URL_SAFE_NO_PAD.encode(&bytes);

    Ok(token)
}

// ==============================================================================
// PASSWORD GENERATION
// ==============================================================================

/// Generates a secure random password for download protection
///
/// Returns a 16-character password using a charset of alphanumeric characters
/// and symbols, excluding visually ambiguous characters (0/O, 1/l/I).
///
/// Character set: A-Z (except I, O), a-z (except l, o), 2-9, !@#$%^&*
///
/// # Examples
///
/// ```
/// let password = generate_download_password()?;
/// assert_eq!(password.len(), 16);
/// ```
///
/// # Errors
///
/// Returns an error if the random number generator fails (extremely rare)
pub fn generate_download_password() -> Result<String> {
    let mut rng = rand::thread_rng();
    let password: String = (0..PASSWORD_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..PASSWORD_CHARSET.len());
            PASSWORD_CHARSET[idx] as char
        })
        .collect();

    Ok(password)
}

// ==============================================================================
// PASSWORD HASHING (ARGON2ID)
// ==============================================================================

/// Hashes a password using Argon2id with secure parameters
///
/// Uses Argon2id algorithm (winner of Password Hashing Competition 2015):
/// - Memory: 47104 KiB (46 MiB)
/// - Iterations: 3
/// - Parallelism: 4
/// - Salt: 16 bytes (cryptographically random)
///
/// The returned hash string is in PHC format and contains the algorithm,
/// parameters, salt, and hash.
///
/// # Arguments
///
/// * `password` - The plain text password to hash
///
/// # Examples
///
/// ```
/// let password = "MySecurePassword123!";
/// let hash = hash_password(password)?;
/// assert!(hash.starts_with("$argon2id$"));
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - Salt generation fails (extremely rare)
/// - Password hashing fails (extremely rare)
pub fn hash_password(password: &str) -> Result<String> {
    // Generate a random salt
    let salt = SaltString::generate(&mut OsRng);

    // Configure Argon2id with secure parameters
    // Memory: 47104 KiB (46 MiB), Iterations: 3, Parallelism: 4
    let params = Params::new(47104, 3, 4, None)
        .context("Failed to create Argon2 parameters")?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    // Hash the password
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .context("Failed to hash password")?
        .to_string();

    Ok(password_hash)
}

/// Verifies a password against an Argon2id hash
///
/// # Arguments
///
/// * `password` - The plain text password to verify
/// * `hash` - The Argon2id hash string (PHC format)
///
/// # Examples
///
/// ```
/// let password = "MySecurePassword123!";
/// let hash = hash_password(password)?;
/// assert!(verify_password(password, &hash)?);
/// assert!(!verify_password("WrongPassword", &hash)?);
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The hash string is malformed
/// - Password verification fails
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let parsed_hash = PasswordHash::new(hash)
        .context("Failed to parse password hash")?;

    let argon2 = Argon2::default();

    match argon2.verify_password(password.as_bytes(), &parsed_hash) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(anyhow::anyhow!("Password verification error: {}", e)),
    }
}

// ==============================================================================
// RECOVERY CODE GENERATION
// ==============================================================================

/// Generates a single recovery code (12 alphanumeric characters)
///
/// Returns a formatted string like "ABCD-1234-EFGH".
fn generate_recovery_code() -> String {
    let mut rng = rand::thread_rng();
    let raw: String = (0..RECOVERY_CODE_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..RECOVERY_CODE_CHARSET.len());
            RECOVERY_CODE_CHARSET[idx] as char
        })
        .collect();

    format!("{}-{}-{}", &raw[0..4], &raw[4..8], &raw[8..12])
}

/// Normalizes a recovery code by stripping dashes and uppercasing
pub fn normalize_recovery_code(code: &str) -> String {
    code.replace('-', "").to_uppercase()
}

/// Generates 8 recovery codes with their Argon2id hashes
///
/// Returns `(plaintext_formatted_codes, hashed_codes)`.
/// Plaintext codes are formatted as "XXXX-XXXX-XXXX".
/// Hashes are computed on the raw 12-char string (no dashes) for flexible input matching.
///
/// This function is CPU-intensive (~2.4s for 8 Argon2id hashes) and should be
/// called via `tokio::task::spawn_blocking`.
pub fn generate_recovery_codes() -> Result<(Vec<String>, Vec<String>)> {
    let mut plaintext_codes = Vec::with_capacity(RECOVERY_CODE_COUNT);
    let mut hashed_codes = Vec::with_capacity(RECOVERY_CODE_COUNT);

    for _ in 0..RECOVERY_CODE_COUNT {
        let formatted = generate_recovery_code();
        let raw = normalize_recovery_code(&formatted);
        let hash = hash_password(&raw)?;
        plaintext_codes.push(formatted);
        hashed_codes.push(hash);
    }

    Ok((plaintext_codes, hashed_codes))
}

// ==============================================================================
// TESTS
// ==============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_download_token() {
        let token = generate_download_token().unwrap();

        // Token should be URL-safe base64 of 32 bytes (43 chars)
        assert_eq!(token.len(), 43);

        // Should only contain URL-safe base64 characters
        assert!(token.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'));

        // Two tokens should be different
        let token2 = generate_download_token().unwrap();
        assert_ne!(token, token2);
    }

    #[test]
    fn test_generate_download_password() {
        let password = generate_download_password().unwrap();

        // Password should be 16 characters
        assert_eq!(password.len(), PASSWORD_LENGTH);

        // Should only contain characters from charset
        let charset_str = String::from_utf8(PASSWORD_CHARSET.to_vec()).unwrap();
        assert!(password.chars().all(|c| charset_str.contains(c)));

        // Should not contain ambiguous characters
        assert!(!password.contains('0'));
        assert!(!password.contains('O'));
        assert!(!password.contains('1'));
        assert!(!password.contains('l'));
        assert!(!password.contains('I'));

        // Two passwords should be different
        let password2 = generate_download_password().unwrap();
        assert_ne!(password, password2);
    }

    #[test]
    fn test_hash_password() {
        let password = "TestPassword123!";
        let hash = hash_password(password).unwrap();

        // Hash should start with Argon2id identifier
        assert!(hash.starts_with("$argon2id$"));

        // Hash should contain version, parameters, salt, and hash
        assert!(hash.contains("$v=19$"));
        assert!(hash.contains("$m=47104,t=3,p=4$"));

        // Two hashes of same password should be different (different salts)
        let hash2 = hash_password(password).unwrap();
        assert_ne!(hash, hash2);
    }

    #[test]
    fn test_verify_password() {
        let password = "CorrectPassword123!";
        let hash = hash_password(password).unwrap();

        // Correct password should verify
        assert!(verify_password(password, &hash).unwrap());

        // Incorrect password should not verify
        assert!(!verify_password("WrongPassword", &hash).unwrap());

        // Case sensitivity matters
        assert!(!verify_password("correctpassword123!", &hash).unwrap());
    }

    #[test]
    fn test_verify_password_invalid_hash() {
        let result = verify_password("password", "not-a-valid-hash");
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_recovery_code_format() {
        let code = generate_recovery_code();

        // Should be formatted as XXXX-XXXX-XXXX (14 chars with dashes)
        assert_eq!(code.len(), 14);
        assert_eq!(&code[4..5], "-");
        assert_eq!(&code[9..10], "-");

        // Should only contain valid charset + dashes
        let charset_str = String::from_utf8(RECOVERY_CODE_CHARSET.to_vec()).unwrap();
        assert!(code.chars().all(|c| c == '-' || charset_str.contains(c)));
    }

    #[test]
    fn test_normalize_recovery_code() {
        assert_eq!(normalize_recovery_code("ABCD-1234-EFGH"), "ABCD1234EFGH");
        assert_eq!(normalize_recovery_code("abcd-1234-efgh"), "ABCD1234EFGH");
        assert_eq!(normalize_recovery_code("ABCD1234EFGH"), "ABCD1234EFGH");
    }

    #[test]
    fn test_generate_recovery_codes() {
        let (plaintexts, hashes) = generate_recovery_codes().unwrap();

        assert_eq!(plaintexts.len(), RECOVERY_CODE_COUNT);
        assert_eq!(hashes.len(), RECOVERY_CODE_COUNT);

        // Each hash should be valid Argon2id
        for hash in &hashes {
            assert!(hash.starts_with("$argon2id$"));
        }

        // Each plaintext code should verify against its hash
        for (plaintext, hash) in plaintexts.iter().zip(hashes.iter()) {
            let raw = normalize_recovery_code(plaintext);
            assert!(verify_password(&raw, hash).unwrap());
        }

        // All codes should be unique
        let unique: std::collections::HashSet<_> = plaintexts.iter().collect();
        assert_eq!(unique.len(), RECOVERY_CODE_COUNT);
    }
}
