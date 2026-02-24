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

    /// Load the encryption key from `<data_dir>/encryption.key`, generating
    /// a new one on the very first launch.
    ///
    /// The key file is 32 random bytes stored as 64 hex characters plus a
    /// trailing newline.  File permissions are set to 0600 on Unix.
    pub fn ensure_key(data_dir: &Path) -> Result<Arc<Self>> {
        let key_path = data_dir.join("encryption.key");

        let key_bytes: [u8; 32] = if key_path.exists() {
            // Load existing key
            let hex = std::fs::read_to_string(&key_path)
                .map_err(|e| SafeAgentError::Config(format!("failed to read encryption key: {e}")))?;
            let hex = hex.trim();
            if hex.len() != 64 {
                return Err(SafeAgentError::Config(format!(
                    "encryption key file corrupt (expected 64 hex chars, got {})",
                    hex.len()
                )));
            }
            let mut buf = [0u8; 32];
            hex_decode(hex, &mut buf)?;
            info!("loaded existing PII encryption key");
            buf
        } else {
            // Generate new key — this is the first launch
            let key = Aes256Gcm::generate_key(OsRng);
            let mut buf = [0u8; 32];
            buf.copy_from_slice(&key);

            // Ensure parent directory exists
            if let Some(parent) = key_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| SafeAgentError::Config(format!("failed to create data dir: {e}")))?;
            }

            // Write key as hex
            std::fs::write(&key_path, format!("{}\n", hex_encode(&buf)))
                .map_err(|e| SafeAgentError::Config(format!("failed to write encryption key: {e}")))?;

            // Restrict permissions to owner-only on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o600);
                let _ = std::fs::set_permissions(&key_path, perms);
            }

            info!(path = %key_path.display(), "generated new PII encryption key (first launch)");
            buf
        };

        // Derive a separate HMAC key for blind indexes so the blind index
        // cannot be used to reverse-engineer the AES key.
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&key_bytes)
            .expect("HMAC can take any key size");
        mac.update(b"safeclaw-blind-index-v1");
        let derived = mac.finalize().into_bytes();
        let mut blind_key = [0u8; 32];
        blind_key.copy_from_slice(&derived);

        Ok(Arc::new(Self { key_bytes, blind_key }))
    }

    // -----------------------------------------------------------------
    // Encrypt / decrypt
    // -----------------------------------------------------------------

    /// Encrypt a plaintext string → `ENC$<base64(nonce ‖ ciphertext)>`.
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

        format!("{ENC_PREFIX}{}", BASE64.encode(&combined))
    }

    /// Decrypt a value produced by [`encrypt`].
    ///
    /// If the value doesn't carry the `ENC$` prefix it's treated as
    /// legacy plaintext and returned as-is (graceful migration).
    pub fn decrypt(&self, stored: &str) -> Result<String> {
        if stored.is_empty() {
            return Ok(String::new());
        }

        let Some(encoded) = stored.strip_prefix(ENC_PREFIX) else {
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

    // -----------------------------------------------------------------
    // Migration helper
    // -----------------------------------------------------------------

    /// Returns `true` if the value appears to be plaintext (not yet encrypted).
    pub fn is_plaintext(value: &str) -> bool {
        !value.is_empty() && !value.starts_with(ENC_PREFIX)
    }
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

    fn test_encryptor() -> FieldEncryptor {
        let key_bytes = [0x42u8; 32];
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&key_bytes).unwrap();
        mac.update(b"safeclaw-blind-index-v1");
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
    fn is_plaintext_detection() {
        assert!(FieldEncryptor::is_plaintext("hello"));
        assert!(!FieldEncryptor::is_plaintext(""));
        assert!(!FieldEncryptor::is_plaintext("ENC$abc123"));
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
    fn ensure_key_creates_and_reloads() {
        let dir = std::env::temp_dir().join(format!("safeclaw-test-{}", uuid::Uuid::new_v4()));
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
