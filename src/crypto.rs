//! Field-level encryption for PII data at rest.
//!
//! On first launch the system generates a 256-bit AES key and persists it
//! to `<data_dir>/encryption.key`.  All PII fields in the database are then
//! encrypted with AES-256-GCM before storage and transparently decrypted on
//! read.
//!
//! For fields that need equality lookups (email, telegram_id, whatsapp_id)
//! we store a deterministic HMAC-SHA-256 *blind index* alongside the
//! encrypted value so SQL `WHERE` clauses still work without ever storing
//! plaintext.

use std::path::Path;
use std::sync::Arc;

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit, Nonce};
use data_encoding::BASE64;
use hmac::Mac;
use sha2::Sha256;
use tracing::info;

use crate::error::{Result, SafeAgentError};

/// Prefix prepended to every encrypted value so we can distinguish
/// encrypted from legacy plaintext in the database.
const ENC_PREFIX: &str = "ENC$";

/// Versioned prefix for key-rotation support.  New encryptions always
/// use this format; decrypt accepts both `ENC$v1$…` and legacy `ENC$…`.
const ENC_V1_PREFIX: &str = "ENC$v1$";

type HmacSha256 = hmac::Hmac<Sha256>;

/// Field-level encryptor backed by a 256-bit AES-GCM key.
#[derive(Clone)]
pub struct FieldEncryptor {
    /// The raw 32-byte key.
    key_bytes: [u8; 32],
    /// A derived HMAC key (HMAC-SHA-256 of "blind-index" with the main key).
    blind_key: [u8; 32],
}

impl FieldEncryptor {
    // -----------------------------------------------------------------
    // Key lifecycle
    // -----------------------------------------------------------------

    /// Load the encryption key via the OS keyring (with file fallback),
    /// generating a new one on the very first launch.
    ///
    /// On Linux the key is stored in the kernel keyring (keyutils) and
    /// mirrored to `<data_dir>/encryption.key` as a hex file (0600).
    /// Existing file-only installations are migrated into the keyring
    /// automatically.
    pub fn ensure_key(data_dir: &Path) -> Result<Arc<Self>> {
        let km = crate::keyring::KeyManager::new(data_dir);

        let key_bytes: [u8; 32] = match km.load_key()? {
            Some(key) => {
                // Opportunistically migrate file-based key into keyring
                let _ = km.migrate_to_keyring();
                info!("loaded existing PII encryption key");
                key
            }
            None => {
                // First launch — generate a fresh 256-bit key
                let key = Aes256Gcm::generate_key(OsRng);
                let mut buf = [0u8; 32];
                buf.copy_from_slice(&key);
                km.store_key(&buf)?;
                info!("generated new PII encryption key (first launch)");
                buf
            }
        };

        // Derive a separate HMAC key for blind indexes so the blind index
        // cannot be used to reverse-engineer the AES key.
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&key_bytes)
            .expect("HMAC can take any key size");
        mac.update(b"safe-agent-blind-index-v1");
        let derived = mac.finalize().into_bytes();
        let mut blind_key = [0u8; 32];
        blind_key.copy_from_slice(&derived);

        Ok(Arc::new(Self { key_bytes, blind_key }))
    }

    // -----------------------------------------------------------------
    // Encrypt / decrypt
    // -----------------------------------------------------------------

    /// Encrypt a plaintext string → `ENC$v1$<base64(nonce ‖ ciphertext)>`.
    ///
    /// Returns the original value unchanged if it's empty (no point
    /// encrypting empty strings) or already encrypted.
    pub fn encrypt(&self, plaintext: &str) -> String {
        if plaintext.is_empty() || plaintext.starts_with(ENC_PREFIX) {
            return plaintext.to_string();
        }

        let key = Key::<Aes256Gcm>::from_slice(&self.key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Aes256Gcm::generate_nonce(OsRng);

        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .expect("AES-GCM encryption should not fail");

        // nonce (12 bytes) ‖ ciphertext+tag
        let mut combined = Vec::with_capacity(12 + ciphertext.len());
        combined.extend_from_slice(&nonce);
        combined.extend_from_slice(&ciphertext);

        format!("{ENC_V1_PREFIX}{}", BASE64.encode(&combined))
    }

    /// Decrypt a value produced by [`encrypt`].
    ///
    /// Accepts both versioned (`ENC$v1$…`) and legacy (`ENC$…`) formats.
    /// If the value doesn't carry the `ENC$` prefix it's treated as
    /// legacy plaintext and returned as-is (graceful migration).
    pub fn decrypt(&self, stored: &str) -> Result<String> {
        if stored.is_empty() {
            return Ok(String::new());
        }

        // Try versioned format first, then legacy
        let encoded = if let Some(e) = stored.strip_prefix(ENC_V1_PREFIX) {
            e
        } else if let Some(e) = stored.strip_prefix(ENC_PREFIX) {
            e
        } else {
            // Legacy plaintext — return as-is
            return Ok(stored.to_string());
        };

        let combined = BASE64.decode(encoded.as_bytes())
            .map_err(|e| SafeAgentError::Config(format!("PII decrypt: bad base64: {e}")))?;

        if combined.len() < 12 {
            return Err(SafeAgentError::Config("PII decrypt: ciphertext too short".into()));
        }

        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let key = Key::<Aes256Gcm>::from_slice(&self.key_bytes);
        let cipher = Aes256Gcm::new(key);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| SafeAgentError::Config("PII decrypt: authentication failed (wrong key or corrupted data)".into()))?;

        String::from_utf8(plaintext)
            .map_err(|e| SafeAgentError::Config(format!("PII decrypt: invalid UTF-8: {e}")))
    }

    /// Encrypt an `Option<T>` where `T: AsRef<str>`, returning `None` for `None`.
    pub fn encrypt_opt(&self, value: Option<&str>) -> Option<String> {
        value.map(|v| self.encrypt(v))
    }

    /// Decrypt an `Option<String>`, returning `None` for `None`.
    pub fn decrypt_opt(&self, stored: Option<String>) -> Result<Option<String>> {
        match stored {
            None => Ok(None),
            Some(v) if v.is_empty() => Ok(Some(String::new())),
            Some(v) => self.decrypt(&v).map(Some),
        }
    }

    /// Encrypt an optional i64 by converting to string first.
    pub fn encrypt_i64_opt(&self, value: Option<i64>) -> Option<String> {
        value.map(|v| self.encrypt(&v.to_string()))
    }

    /// Decrypt an optional i64 that was encrypted as a string.
    pub fn decrypt_i64_opt(&self, stored: Option<String>) -> Result<Option<i64>> {
        match stored {
            None => Ok(None),
            Some(v) if v.is_empty() => Ok(None),
            Some(v) => {
                let plain = self.decrypt(&v)?;
                if plain.is_empty() {
                    return Ok(None);
                }
                plain.parse::<i64>()
                    .map(Some)
                    .map_err(|e| SafeAgentError::Config(format!("PII decrypt i64: {e}")))
            }
        }
    }

    // -----------------------------------------------------------------
    // Blind index (deterministic HMAC-SHA-256 for lookups)
    // -----------------------------------------------------------------

    /// Compute a deterministic blind index for equality lookups.
    ///
    /// Returns a 64-char hex string (SHA-256 output).  The same plaintext
    /// always produces the same hash, so we can `WHERE email_blind = ?`.
    pub fn blind_index(&self, plaintext: &str) -> String {
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&self.blind_key)
            .expect("HMAC can take any key size");
        mac.update(plaintext.as_bytes());
        let result = mac.finalize().into_bytes();
        hex_encode(&result)
    }

    /// Blind index for an i64 value.
    pub fn blind_index_i64(&self, value: i64) -> String {
        self.blind_index(&value.to_string())
    }

    /// Blind index for an optional value.  Returns empty string for None.
    pub fn blind_index_opt(&self, value: Option<&str>) -> String {
        match value {
            Some(v) if !v.is_empty() => self.blind_index(v),
            _ => String::new(),
        }
    }

    /// Blind index for an optional i64.
    pub fn blind_index_i64_opt(&self, value: Option<i64>) -> String {
        match value {
            Some(v) => self.blind_index_i64(v),
            None => String::new(),
        }
    }

    // -----------------------------------------------------------------
    // Migration helper
    // -----------------------------------------------------------------

    /// Returns `true` if the value appears to be plaintext (not yet encrypted).
    pub fn is_plaintext(value: &str) -> bool {
        !value.is_empty() && !value.starts_with(ENC_PREFIX)
    }

    // -----------------------------------------------------------------
    // Key rotation
    // -----------------------------------------------------------------

    /// Generate a fresh key and persist it, replacing the current one.
    ///
    /// Returns the new `FieldEncryptor`.  Callers must then re-encrypt all
    /// stored data with the new key (use `re_encrypt` to translate individual
    /// values from old → new).
    pub fn rotate_key(data_dir: &Path) -> Result<Arc<Self>> {
        let km = crate::keyring::KeyManager::new(data_dir);
        let key = Aes256Gcm::generate_key(OsRng);
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&key);
        km.store_key(&buf)?;

        let mut mac = <HmacSha256 as Mac>::new_from_slice(&buf)
            .expect("HMAC can take any key size");
        mac.update(b"safe-agent-blind-index-v1");
        let derived = mac.finalize().into_bytes();
        let mut blind_key = [0u8; 32];
        blind_key.copy_from_slice(&derived);

        info!("encryption key rotated — all stored data must be re-encrypted");
        Ok(Arc::new(Self {
            key_bytes: buf,
            blind_key,
        }))
    }

    /// Re-encrypt a value: decrypt with `self` (old key), encrypt with `new`
    /// key.  Passes through empty and plaintext values unchanged.
    pub fn re_encrypt(&self, stored: &str, new: &FieldEncryptor) -> Result<String> {
        if stored.is_empty() || !stored.starts_with(ENC_PREFIX) {
            // Plaintext — just encrypt with new key
            return Ok(if stored.is_empty() {
                String::new()
            } else {
                new.encrypt(stored)
            });
        }
        let plaintext = self.decrypt(stored)?;
        Ok(new.encrypt(&plaintext))
    }
}

// ---------------------------------------------------------------------------
// Password hashing (Argon2id)
// ---------------------------------------------------------------------------

/// Prefix on Argon2id PHC strings, used to detect already-hashed passwords.
const ARGON2_PREFIX: &str = "$argon2id$";

/// Hash a password with Argon2id using OWASP-recommended minimum parameters:
/// m=19456 (19 MiB), t=2 iterations, p=1 lane.
pub fn hash_password(plaintext: &str) -> Result<String> {
    use argon2::{Argon2, PasswordHasher, password_hash::SaltString};

    let params = argon2::Params::new(19456, 2, 1, None)
        .map_err(|e| SafeAgentError::Config(format!("argon2 params: {e}")))?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let salt = SaltString::generate(OsRng);
    let hash = argon2
        .hash_password(plaintext.as_bytes(), &salt)
        .map_err(|e| SafeAgentError::Config(format!("argon2 hash: {e}")))?;
    Ok(hash.to_string())
}

/// Verify a plaintext password against an Argon2id PHC hash string.
pub fn verify_password(plaintext: &str, hash: &str) -> Result<bool> {
    use argon2::{Argon2, PasswordVerifier, password_hash::PasswordHash};

    let parsed = PasswordHash::new(hash)
        .map_err(|e| SafeAgentError::Config(format!("argon2 parse hash: {e}")))?;
    Ok(Argon2::default().verify_password(plaintext.as_bytes(), &parsed).is_ok())
}

/// Returns `true` if the value is an Argon2id PHC hash string.
pub fn is_argon2_hash(value: &str) -> bool {
    value.starts_with(ARGON2_PREFIX)
}

// ---------------------------------------------------------------------------
// Hex helpers (no extra dependency)
// ---------------------------------------------------------------------------

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(hex: &str, out: &mut [u8]) -> Result<()> {
    if hex.len() != out.len() * 2 {
        return Err(SafeAgentError::Config("hex decode: length mismatch".into()));
    }
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Ok(())
}

fn hex_nibble(c: u8) -> Result<u8> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(SafeAgentError::Config(format!("invalid hex char: {c}"))),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_encryptor() -> FieldEncryptor {
        let key_bytes = [0x42u8; 32];
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&key_bytes).unwrap();
        mac.update(b"safe-agent-blind-index-v1");
        let derived = mac.finalize().into_bytes();
        let mut blind_key = [0u8; 32];
        blind_key.copy_from_slice(&derived);
        FieldEncryptor { key_bytes, blind_key }
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let enc = test_encryptor();
        let original = "alice@example.com";
        let encrypted = enc.encrypt(original);

        assert!(encrypted.starts_with(ENC_PREFIX));
        assert_ne!(encrypted, original);

        let decrypted = enc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn encrypt_empty_is_noop() {
        let enc = test_encryptor();
        assert_eq!(enc.encrypt(""), "");
        assert_eq!(enc.decrypt("").unwrap(), "");
    }

    #[test]
    fn encrypt_idempotent() {
        let enc = test_encryptor();
        let encrypted = enc.encrypt("secret");
        let double_encrypted = enc.encrypt(&encrypted);
        // Should not double-encrypt
        assert_eq!(encrypted, double_encrypted);
    }

    #[test]
    fn decrypt_plaintext_passthrough() {
        let enc = test_encryptor();
        // Legacy plaintext should pass through
        assert_eq!(enc.decrypt("plain@example.com").unwrap(), "plain@example.com");
    }

    #[test]
    fn decrypt_wrong_key_fails() {
        let enc1 = test_encryptor();
        let encrypted = enc1.encrypt("secret data");

        let enc2 = FieldEncryptor {
            key_bytes: [0x99u8; 32],
            blind_key: [0u8; 32],
        };
        assert!(enc2.decrypt(&encrypted).is_err());
    }

    #[test]
    fn blind_index_deterministic() {
        let enc = test_encryptor();
        let idx1 = enc.blind_index("alice@example.com");
        let idx2 = enc.blind_index("alice@example.com");
        assert_eq!(idx1, idx2);
        assert_eq!(idx1.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn blind_index_different_inputs_differ() {
        let enc = test_encryptor();
        let idx1 = enc.blind_index("alice@example.com");
        let idx2 = enc.blind_index("bob@example.com");
        assert_ne!(idx1, idx2);
    }

    #[test]
    fn encrypt_i64_roundtrip() {
        let enc = test_encryptor();
        let encrypted = enc.encrypt_i64_opt(Some(123456789));
        assert!(encrypted.is_some());

        let decrypted = enc.decrypt_i64_opt(encrypted).unwrap();
        assert_eq!(decrypted, Some(123456789));
    }

    #[test]
    fn is_plaintext_detection() {
        assert!(FieldEncryptor::is_plaintext("hello"));
        assert!(!FieldEncryptor::is_plaintext(""));
        assert!(!FieldEncryptor::is_plaintext("ENC$abc123"));
        assert!(!FieldEncryptor::is_plaintext("ENC$v1$abc123"));
    }

    #[test]
    fn encrypt_uses_v1_prefix() {
        let enc = test_encryptor();
        let ct = enc.encrypt("test");
        assert!(ct.starts_with("ENC$v1$"), "expected v1 prefix, got: {ct}");
    }

    #[test]
    fn re_encrypt_with_new_key() {
        let enc1 = test_encryptor();
        let enc2 = FieldEncryptor {
            key_bytes: [0x99u8; 32],
            blind_key: [0u8; 32],
        };

        let ct1 = enc1.encrypt("secret");
        let ct2 = enc1.re_encrypt(&ct1, &enc2).unwrap();

        // ct2 should be decryptable with enc2
        assert_eq!(enc2.decrypt(&ct2).unwrap(), "secret");
        // ct2 should NOT be decryptable with enc1
        assert!(enc1.decrypt(&ct2).is_err());
    }

    #[test]
    fn hex_roundtrip() {
        let bytes = [0xDE, 0xAD, 0xBE, 0xEF];
        let hex = hex_encode(&bytes);
        assert_eq!(hex, "deadbeef");

        let mut decoded = [0u8; 4];
        hex_decode(&hex, &mut decoded).unwrap();
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn argon2_hash_and_verify() {
        let hash = super::hash_password("my-secret-pw").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(super::verify_password("my-secret-pw", &hash).unwrap());
        assert!(!super::verify_password("wrong-pw", &hash).unwrap());
    }

    #[test]
    fn argon2_is_argon2_hash() {
        assert!(super::is_argon2_hash("$argon2id$v=19$m=19456,t=2,p=1$salt$hash"));
        assert!(!super::is_argon2_hash("ENC$abc"));
        assert!(!super::is_argon2_hash("plaintext"));
    }

    #[test]
    fn ensure_key_creates_and_reloads() {
        let dir = std::env::temp_dir().join(format!("safe-agent-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // First call creates the key
        let enc1 = FieldEncryptor::ensure_key(&dir).unwrap();
        assert!(dir.join("encryption.key").exists());

        // Second call loads the same key
        let enc2 = FieldEncryptor::ensure_key(&dir).unwrap();

        // Same key should encrypt/decrypt interchangeably
        let ct = enc1.encrypt("test");
        assert_eq!(enc2.decrypt(&ct).unwrap(), "test");

        std::fs::remove_dir_all(&dir).ok();
    }
}
