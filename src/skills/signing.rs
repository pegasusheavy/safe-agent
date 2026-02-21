//! Ed25519 skill signing and verification.
//!
//! Every skill is signed on import by hashing its content-addressable tree
//! (all files minus ephemeral artefacts) and signing the hash with the
//! agent's Ed25519 private key.  On load the signature is verified — a
//! tampered skill is refused.
//!
//! ## Keypair lifecycle
//!
//! On first launch `SkillSigner::ensure` generates a new Ed25519 keypair.
//! The private key is encrypted at rest via `FieldEncryptor` and stored in
//! `<data_dir>/signing.key`.  The public key is written in plain hex to
//! `<data_dir>/signing.pub` so external tooling can verify signatures.
//!
//! ## Content hash
//!
//! The content hash is SHA-256 over a deterministic walk of the skill
//! directory.  Files are sorted by relative path, and each entry is
//! `"<relative_path>\0<sha256_hex>\n"`.  Ephemeral paths like `.venv/`,
//! `.git/`, `data/`, `skill.log`, and `.signature` are excluded.

use std::collections::BTreeMap;
use std::path::Path;

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use tracing::info;

use crate::crypto::FieldEncryptor;
use crate::error::{Result, SafeAgentError};

const SIGNING_KEY_FILE: &str = "signing.key";
const SIGNING_PUB_FILE: &str = "signing.pub";
const SIGNATURE_FILE: &str = ".signature";

/// Paths excluded from the content hash (ephemeral or generated).
const EXCLUDED_PATHS: &[&str] = &[
    ".venv",
    ".git",
    "data",
    "skill.log",
    ".signature",
    "__pycache__",
    "node_modules",
];

/// Agent's Ed25519 signing keypair.
pub struct SkillSigner {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl SkillSigner {
    /// Load or generate the Ed25519 keypair.
    ///
    /// The private key is stored encrypted via `FieldEncryptor` so it never
    /// appears in plaintext on disk.
    pub fn ensure(data_dir: &Path, enc: &FieldEncryptor) -> Result<Self> {
        let key_path = data_dir.join(SIGNING_KEY_FILE);
        let pub_path = data_dir.join(SIGNING_PUB_FILE);

        let signing_key = if key_path.exists() {
            // Load existing encrypted key
            let stored = std::fs::read_to_string(&key_path)
                .map_err(|e| SafeAgentError::Config(format!("read signing key: {e}")))?;
            let hex = enc.decrypt(stored.trim())?;
            let bytes = hex_decode_32(&hex)?;
            SigningKey::from_bytes(&bytes)
        } else {
            // First launch — generate keypair
            let signing_key = SigningKey::generate(&mut aes_gcm::aead::OsRng);
            let secret_hex = hex_encode(signing_key.as_bytes());
            let encrypted = enc.encrypt(&secret_hex);

            if let Some(parent) = key_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| SafeAgentError::Config(format!("create data dir: {e}")))?;
            }

            std::fs::write(&key_path, &encrypted)
                .map_err(|e| SafeAgentError::Config(format!("write signing key: {e}")))?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
            }

            // Write public key in plain hex for external verification
            let pub_hex = hex_encode(signing_key.verifying_key().as_bytes());
            std::fs::write(&pub_path, format!("{pub_hex}\n"))
                .map_err(|e| SafeAgentError::Config(format!("write signing pub: {e}")))?;

            info!("generated new Ed25519 skill signing keypair");
            signing_key
        };

        let verifying_key = signing_key.verifying_key();

        // Ensure the public key file is in sync
        if !pub_path.exists() {
            let pub_hex = hex_encode(verifying_key.as_bytes());
            let _ = std::fs::write(&pub_path, format!("{pub_hex}\n"));
        }

        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    /// Sign a skill directory. Writes `.signature` into the skill dir.
    ///
    /// Returns the content hash for logging/display.
    pub fn sign_skill(&self, skill_dir: &Path) -> Result<String> {
        let content_hash = content_hash(skill_dir)?;
        let signature = self.signing_key.sign(content_hash.as_bytes());
        let sig_hex = hex_encode(&signature.to_bytes());
        let pub_hex = hex_encode(self.verifying_key.as_bytes());

        let sig_content = format!(
            "hash={content_hash}\nsig={sig_hex}\npub={pub_hex}\n"
        );

        std::fs::write(skill_dir.join(SIGNATURE_FILE), &sig_content)
            .map_err(|e| SafeAgentError::Config(format!("write signature: {e}")))?;

        info!(
            dir = %skill_dir.display(),
            hash = &content_hash[..16],
            "skill signed"
        );

        Ok(content_hash)
    }

    /// Verify a skill's signature. Returns `Ok(())` if valid.
    pub fn verify_skill(&self, skill_dir: &Path) -> Result<()> {
        let sig_path = skill_dir.join(SIGNATURE_FILE);
        if !sig_path.exists() {
            return Err(SafeAgentError::Config(format!(
                "skill at '{}' has no signature — re-import to sign",
                skill_dir.display()
            )));
        }

        let sig_content = std::fs::read_to_string(&sig_path)
            .map_err(|e| SafeAgentError::Config(format!("read signature: {e}")))?;

        let fields = parse_signature_file(&sig_content)?;
        let stored_hash = fields.get("hash").ok_or_else(|| {
            SafeAgentError::Config("signature file missing 'hash' field".into())
        })?;
        let sig_hex = fields.get("sig").ok_or_else(|| {
            SafeAgentError::Config("signature file missing 'sig' field".into())
        })?;

        // Verify the Ed25519 signature over the stored hash
        let sig_bytes = hex_decode_64(sig_hex)?;
        let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);
        self.verifying_key
            .verify(stored_hash.as_bytes(), &signature)
            .map_err(|_| {
                SafeAgentError::Config(format!(
                    "signature verification failed for '{}' — signing key mismatch or tampered .signature",
                    skill_dir.display()
                ))
            })?;

        // Recompute the content hash and compare
        let current_hash = content_hash(skill_dir)?;
        if current_hash != *stored_hash {
            return Err(SafeAgentError::Config(format!(
                "skill content tampered at '{}': expected hash {}…, got {}…",
                skill_dir.display(),
                &stored_hash[..16],
                &current_hash[..16],
            )));
        }

        Ok(())
    }

    /// Get the public key fingerprint (first 16 hex chars) for display.
    pub fn fingerprint(&self) -> String {
        let hex = hex_encode(self.verifying_key.as_bytes());
        hex[..16].to_string()
    }
}

/// Compute a deterministic content hash for a skill directory.
///
/// Walks all files (sorted), hashing each, then hashes the combined
/// manifest of `"<relative_path>\0<file_sha256>\n"` entries.
pub fn content_hash(skill_dir: &Path) -> Result<String> {
    let mut file_hashes: BTreeMap<String, String> = BTreeMap::new();
    walk_dir(skill_dir, skill_dir, &mut file_hashes)?;

    // Build the manifest string and hash it
    let mut manifest_hasher = Sha256::new();
    for (path, hash) in &file_hashes {
        manifest_hasher.update(format!("{path}\0{hash}\n").as_bytes());
    }

    Ok(hex_encode(&manifest_hasher.finalize()))
}

/// Recursively walk a directory, collecting file hashes.
fn walk_dir(
    base: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, String>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| SafeAgentError::Config(format!("read dir '{}': {e}", dir.display())))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap_or(&path);

        // Check exclusions against the top-level component
        if let Some(first) = rel.components().next() {
            let name = first.as_os_str().to_string_lossy();
            if EXCLUDED_PATHS.iter().any(|e| *e == name.as_ref()) {
                continue;
            }
        }

        if path.is_dir() {
            walk_dir(base, &path, out)?;
        } else {
            let hash = hash_file(&path)?;
            let rel_str = rel.to_string_lossy().to_string();
            out.insert(rel_str, hash);
        }
    }

    Ok(())
}

/// SHA-256 hash of a single file.
fn hash_file(path: &Path) -> Result<String> {
    let data = std::fs::read(path)
        .map_err(|e| SafeAgentError::Config(format!("read '{}': {e}", path.display())))?;
    let hash = Sha256::digest(&data);
    Ok(hex_encode(&hash))
}

/// Parse a `.signature` file into key=value pairs.
fn parse_signature_file(content: &str) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.to_string(), value.to_string());
        }
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Hex helpers
// ---------------------------------------------------------------------------

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode_32(hex: &str) -> Result<[u8; 32]> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return Err(SafeAgentError::Config(format!(
            "expected 64 hex chars for 32-byte key, got {}",
            hex.len()
        )));
    }
    let mut buf = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        buf[i] = (hex_nibble(chunk[0])? << 4) | hex_nibble(chunk[1])?;
    }
    Ok(buf)
}

fn hex_decode_64(hex: &str) -> Result<[u8; 64]> {
    let hex = hex.trim();
    if hex.len() != 128 {
        return Err(SafeAgentError::Config(format!(
            "expected 128 hex chars for 64-byte signature, got {}",
            hex.len()
        )));
    }
    let mut buf = [0u8; 64];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        buf[i] = (hex_nibble(chunk[0])? << 4) | hex_nibble(chunk[1])?;
    }
    Ok(buf)
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

    fn test_encryptor() -> std::sync::Arc<FieldEncryptor> {
        let dir = std::env::temp_dir().join(format!("sa-sign-test-enc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let enc = FieldEncryptor::ensure_key(&dir).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        enc
    }

    fn make_test_skill(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("skill.toml"),
            "name = \"test-skill\"\ndescription = \"a test\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("main.py"), "print('hello')\n").unwrap();
    }

    #[test]
    fn keypair_generation_and_reload() {
        let dir = std::env::temp_dir().join(format!("sa-sign-kp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let enc = test_encryptor();

        let signer1 = SkillSigner::ensure(&dir, &enc).unwrap();
        let fp1 = signer1.fingerprint();

        // Reload should give the same keypair
        let signer2 = SkillSigner::ensure(&dir, &enc).unwrap();
        let fp2 = signer2.fingerprint();
        assert_eq!(fp1, fp2);

        // Files should exist
        assert!(dir.join(SIGNING_KEY_FILE).exists());
        assert!(dir.join(SIGNING_PUB_FILE).exists());

        // Private key should be encrypted on disk
        let stored = std::fs::read_to_string(dir.join(SIGNING_KEY_FILE)).unwrap();
        assert!(stored.trim().starts_with("ENC$"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let dir = std::env::temp_dir().join(format!("sa-sign-rt-{}", uuid::Uuid::new_v4()));
        let data_dir = dir.join("data");
        let skill_dir = dir.join("skill");
        std::fs::create_dir_all(&data_dir).unwrap();
        make_test_skill(&skill_dir);

        let enc = test_encryptor();
        let signer = SkillSigner::ensure(&data_dir, &enc).unwrap();

        // Sign
        let hash = signer.sign_skill(&skill_dir).unwrap();
        assert!(!hash.is_empty());
        assert!(skill_dir.join(SIGNATURE_FILE).exists());

        // Verify — should pass
        signer.verify_skill(&skill_dir).unwrap();

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tampered_content_detected() {
        let dir = std::env::temp_dir().join(format!("sa-sign-tamp-{}", uuid::Uuid::new_v4()));
        let data_dir = dir.join("data");
        let skill_dir = dir.join("skill");
        std::fs::create_dir_all(&data_dir).unwrap();
        make_test_skill(&skill_dir);

        let enc = test_encryptor();
        let signer = SkillSigner::ensure(&data_dir, &enc).unwrap();
        signer.sign_skill(&skill_dir).unwrap();

        // Tamper with a file after signing
        std::fs::write(skill_dir.join("main.py"), "print('MODIFIED CONTENT')\n").unwrap();

        // Verify should fail with tamper detection
        let result = signer.verify_skill(&skill_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("tampered"), "unexpected error: {err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_signature_detected() {
        let dir = std::env::temp_dir().join(format!("sa-sign-nosig-{}", uuid::Uuid::new_v4()));
        let data_dir = dir.join("data");
        let skill_dir = dir.join("skill");
        std::fs::create_dir_all(&data_dir).unwrap();
        make_test_skill(&skill_dir);

        let enc = test_encryptor();
        let signer = SkillSigner::ensure(&data_dir, &enc).unwrap();

        // No .signature file
        let result = signer.verify_skill(&skill_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no signature"), "unexpected error: {err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn content_hash_deterministic() {
        let dir = std::env::temp_dir().join(format!("sa-sign-det-{}", uuid::Uuid::new_v4()));
        make_test_skill(&dir);

        let h1 = content_hash(&dir).unwrap();
        let h2 = content_hash(&dir).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 = 64 hex chars

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn content_hash_excludes_ephemeral() {
        let dir = std::env::temp_dir().join(format!("sa-sign-excl-{}", uuid::Uuid::new_v4()));
        make_test_skill(&dir);

        let h1 = content_hash(&dir).unwrap();

        // Add excluded paths — hash should not change
        std::fs::create_dir_all(dir.join(".venv/lib")).unwrap();
        std::fs::write(dir.join(".venv/lib/something.py"), "x").unwrap();
        std::fs::write(dir.join("skill.log"), "log line").unwrap();
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(dir.join("data/state.json"), "{}").unwrap();
        std::fs::write(dir.join(".signature"), "hash=abc\nsig=def\n").unwrap();

        let h2 = content_hash(&dir).unwrap();
        assert_eq!(h1, h2);

        std::fs::remove_dir_all(&dir).ok();
    }
}
