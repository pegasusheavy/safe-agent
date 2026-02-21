//! OS keyring integration for encryption key storage.
//!
//! Provides a `KeyManager` that stores the master encryption key in the OS
//! keyring (Linux keyutils) with transparent fallback to the legacy
//! `encryption.key` file.  On first use, if a file-based key exists it is
//! migrated into the keyring and the file removed.
//!
//! Hierarchy of trust:
//!   1. OS keyring (kernel-managed, never touches disk in plaintext)
//!   2. File at `<data_dir>/encryption.key` (0600, hex-encoded)

use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use crate::error::{Result, SafeAgentError};

const SERVICE_NAME: &str = "safe-agent";
const KEY_USERNAME: &str = "encryption-key";
const KEY_FILE_NAME: &str = "encryption.key";

/// Manages encryption key storage across OS keyring and file fallback.
pub struct KeyManager {
    data_dir: PathBuf,
}

impl KeyManager {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
        }
    }

    /// Load the encryption key, trying OS keyring first then file fallback.
    ///
    /// Returns `None` if no key exists in either location.
    pub fn load_key(&self) -> Result<Option<[u8; 32]>> {
        // 1. Try OS keyring
        match self.load_from_keyring() {
            Ok(Some(key)) => {
                debug!("loaded encryption key from OS keyring");
                return Ok(Some(key));
            }
            Ok(None) => {}
            Err(e) => {
                debug!("keyring unavailable, falling back to file: {e}");
            }
        }

        // 2. Fallback to file
        self.load_from_file()
    }

    /// Store the key in both OS keyring (best-effort) and file (guaranteed).
    ///
    /// The file is always written as the authoritative fallback.  The keyring
    /// store is best-effort — failure is logged but not fatal.
    pub fn store_key(&self, key: &[u8; 32]) -> Result<()> {
        // Always write the file as reliable fallback
        self.store_to_file(key)?;

        // Best-effort keyring store
        if let Err(e) = self.store_to_keyring(key) {
            warn!("could not store key in OS keyring (file fallback active): {e}");
        } else {
            info!("stored encryption key in OS keyring");
        }

        Ok(())
    }

    /// Migrate an existing file-based key into the OS keyring.
    ///
    /// If the key is already in the keyring, or the keyring is unavailable,
    /// this is a no-op.  The file is intentionally *kept* as fallback after
    /// migration — removal is a future opt-in step.
    pub fn migrate_to_keyring(&self) -> Result<()> {
        // Already in keyring?
        match self.load_from_keyring() {
            Ok(Some(_)) => {
                debug!("encryption key already in OS keyring, skipping migration");
                return Ok(());
            }
            Ok(None) => {}
            Err(e) => {
                debug!("keyring unavailable, skipping migration: {e}");
                return Ok(());
            }
        }

        // Load from file
        let Some(key) = self.load_from_file()? else {
            return Ok(());
        };

        // Store into keyring
        match self.store_to_keyring(&key) {
            Ok(()) => {
                info!("migrated encryption key from file to OS keyring");
            }
            Err(e) => {
                debug!("keyring migration skipped (not available): {e}");
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------
    // OS keyring operations
    // -----------------------------------------------------------------

    #[cfg(target_os = "linux")]
    fn load_from_keyring(&self) -> Result<Option<[u8; 32]>> {
        let entry = keyring::Entry::new(SERVICE_NAME, KEY_USERNAME)
            .map_err(|e| SafeAgentError::Config(format!("keyring entry: {e}")))?;

        match entry.get_password() {
            Ok(hex) => {
                let hex = hex.trim();
                if hex.len() != 64 {
                    return Err(SafeAgentError::Config(format!(
                        "keyring key corrupt (expected 64 hex chars, got {})",
                        hex.len()
                    )));
                }
                let mut buf = [0u8; 32];
                hex_decode(hex, &mut buf)?;
                Ok(Some(buf))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SafeAgentError::Config(format!("keyring get: {e}"))),
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn load_from_keyring(&self) -> Result<Option<[u8; 32]>> {
        Ok(None)
    }

    #[cfg(target_os = "linux")]
    fn store_to_keyring(&self, key: &[u8; 32]) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, KEY_USERNAME)
            .map_err(|e| SafeAgentError::Config(format!("keyring entry: {e}")))?;

        entry
            .set_password(&hex_encode(key))
            .map_err(|e| SafeAgentError::Config(format!("keyring set: {e}")))
    }

    #[cfg(not(target_os = "linux"))]
    fn store_to_keyring(&self, _key: &[u8; 32]) -> Result<()> {
        Err(SafeAgentError::Config(
            "OS keyring not supported on this platform".into(),
        ))
    }

    // -----------------------------------------------------------------
    // File-based key operations
    // -----------------------------------------------------------------

    fn key_path(&self) -> PathBuf {
        self.data_dir.join(KEY_FILE_NAME)
    }

    fn load_from_file(&self) -> Result<Option<[u8; 32]>> {
        let path = self.key_path();
        if !path.exists() {
            return Ok(None);
        }

        let hex = std::fs::read_to_string(&path)
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
        debug!("loaded encryption key from file");
        Ok(Some(buf))
    }

    fn store_to_file(&self, key: &[u8; 32]) -> Result<()> {
        let path = self.key_path();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| SafeAgentError::Config(format!("failed to create data dir: {e}")))?;
        }

        std::fs::write(&path, format!("{}\n", hex_encode(key)))
            .map_err(|e| SafeAgentError::Config(format!("failed to write encryption key: {e}")))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&path, perms);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Hex helpers (mirrors crypto.rs — kept private to avoid cross-module coupling)
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

    #[test]
    fn file_store_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("safe-agent-km-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let km = KeyManager::new(&dir);
        let key = [0x42u8; 32];

        // No key initially
        assert!(km.load_from_file().unwrap().is_none());

        // Store and reload
        km.store_to_file(&key).unwrap();
        let loaded = km.load_from_file().unwrap().unwrap();
        assert_eq!(loaded, key);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_key_returns_none_when_empty() {
        let dir = std::env::temp_dir().join(format!("safe-agent-km-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let km = KeyManager::new(&dir);
        assert!(km.load_key().unwrap().is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn store_key_creates_file() {
        let dir = std::env::temp_dir().join(format!("safe-agent-km-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let km = KeyManager::new(&dir);
        let key = [0xAB; 32];
        km.store_key(&key).unwrap();

        assert!(dir.join(KEY_FILE_NAME).exists());
        let loaded = km.load_from_file().unwrap().unwrap();
        assert_eq!(loaded, key);

        std::fs::remove_dir_all(&dir).ok();
    }
}
