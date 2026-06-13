//! API key generation and verification using argon2 password hashing.
//!
//! Keys are generated as `kk_<32-char-random-base64url>`.
//! Only the argon2 hash is stored; the plaintext key is shown once at creation.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

use crate::error::CloudError;

/// A freshly-generated API key (returned once; the plaintext is never stored).
#[derive(Debug)]
pub struct GeneratedApiKey {
    /// The full key string to show the user, e.g. `kk_abc123…`.
    pub plaintext: String,
    /// The 8-character display prefix stored in the database.
    pub prefix: String,
    /// The argon2 hash stored in the database.
    pub hash: String,
}

/// Generates a new API key, hashes it with argon2, and returns both.
///
/// # Errors
///
/// Returns [`CloudError::Internal`] if argon2 hashing fails.
pub fn generate() -> Result<GeneratedApiKey, CloudError> {
    let mut raw = [0u8; 24];
    // Fill with cryptographically random bytes.
    use argon2::password_hash::rand_core::RngCore;
    OsRng.fill_bytes(&mut raw);

    let token = URL_SAFE_NO_PAD.encode(raw);
    let plaintext = format!("kk_{token}");
    let prefix = plaintext[..8].to_string(); // "kk_" + 5 chars

    let hash = hash_key(&plaintext)?;
    Ok(GeneratedApiKey {
        plaintext,
        prefix,
        hash,
    })
}

/// Hashes an API key with argon2id.
///
/// # Errors
///
/// Returns [`CloudError::Internal`] if hashing fails.
pub fn hash_key(key: &str) -> Result<String, CloudError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(key.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| CloudError::Internal(format!("argon2 hash failed: {e}")))
}

/// Verifies a plaintext key against a stored argon2 hash.
///
/// Returns `true` if the key matches; `false` if it does not.
///
/// # Errors
///
/// Returns [`CloudError::Internal`] if the stored hash is malformed.
pub fn verify(key: &str, stored_hash: &str) -> Result<bool, CloudError> {
    let parsed = PasswordHash::new(stored_hash)
        .map_err(|e| CloudError::Internal(format!("malformed stored hash: {e}")))?;
    Ok(Argon2::default()
        .verify_password(key.as_bytes(), &parsed)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_starts_with_kk() {
        let key = generate().unwrap();
        assert!(key.plaintext.starts_with("kk_"), "got: {}", key.plaintext);
    }

    #[test]
    fn prefix_is_first_8_chars() {
        let key = generate().unwrap();
        assert_eq!(&key.plaintext[..8], key.prefix);
    }

    #[test]
    fn verify_succeeds_with_correct_key() {
        let key = generate().unwrap();
        assert!(verify(&key.plaintext, &key.hash).unwrap());
    }

    #[test]
    fn verify_fails_with_wrong_key() {
        let key = generate().unwrap();
        assert!(!verify("kk_wrongkey", &key.hash).unwrap());
    }

    #[test]
    fn verify_errors_on_malformed_hash() {
        let result = verify("kk_any", "not-a-valid-argon2-hash");
        assert!(result.is_err());
    }
}
