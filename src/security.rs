use std::path::{Path, PathBuf};

use reqwest::Url;

use crate::error::{Result, SafeAgentError};

/// Sandboxed filesystem — all file I/O is confined to the data directory.
#[derive(Debug, Clone)]
pub struct SandboxedFs {
    root: PathBuf,
}

impl SandboxedFs {
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        let root = root
            .canonicalize()
            .map_err(|e| SafeAgentError::SandboxViolation(format!("cannot canonicalize root: {e}")))?;
        Ok(Self { root })
    }

    /// Resolve a relative path within the sandbox. Rejects any path that escapes.
    pub fn resolve(&self, relative: &Path) -> Result<PathBuf> {
        if relative.is_absolute() {
            return Err(SafeAgentError::SandboxViolation(
                "absolute paths are not allowed".into(),
            ));
        }

        let candidate = self.root.join(relative);

        // Create parent dirs so canonicalize works on new files
        if let Some(parent) = candidate.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // For existing paths, canonicalize and check containment
        if candidate.exists() {
            let canonical = candidate.canonicalize()?;
            if !canonical.starts_with(&self.root) {
                return Err(SafeAgentError::SandboxViolation(format!(
                    "path escapes sandbox: {}",
                    relative.display()
                )));
            }
            return Ok(canonical);
        }

        // For new paths, canonicalize the parent and check
        if let Some(parent) = candidate.parent() {
            let canonical_parent = parent.canonicalize()?;
            if !canonical_parent.starts_with(&self.root) {
                return Err(SafeAgentError::SandboxViolation(format!(
                    "path escapes sandbox: {}",
                    relative.display()
                )));
            }
            let filename = candidate
                .file_name()
                .ok_or_else(|| SafeAgentError::SandboxViolation("invalid filename".into()))?;
            return Ok(canonical_parent.join(filename));
        }

        Err(SafeAgentError::SandboxViolation(
            "cannot resolve path".into(),
        ))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn read(&self, relative: &Path) -> Result<Vec<u8>> {
        let path = self.resolve(relative)?;
        Ok(std::fs::read(path)?)
    }

    pub fn write(&self, relative: &Path, data: &[u8]) -> Result<()> {
        let path = self.resolve(relative)?;
        Ok(std::fs::write(path, data)?)
    }

    pub fn read_to_string(&self, relative: &Path) -> Result<String> {
        let path = self.resolve(relative)?;
        Ok(std::fs::read_to_string(path)?)
    }

    pub fn exists(&self, relative: &Path) -> bool {
        self.resolve(relative).map(|p| p.exists()).unwrap_or(false)
    }
}

/// HTTP client that only allows requests to allowlisted hosts.
#[derive(Debug, Clone)]
pub struct AllowlistedHttpClient {
    client: reqwest::Client,
    allowed_hosts: Vec<String>,
}

impl AllowlistedHttpClient {
    pub fn new(allowed_hosts: Vec<String>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .user_agent("safe-agent/0.1.0")
            .build()?;
        Ok(Self {
            client,
            allowed_hosts,
        })
    }

    fn check_url(&self, url: &str) -> Result<Url> {
        let parsed: Url = url
            .parse()
            .map_err(|e| SafeAgentError::NetworkNotAllowed(format!("invalid URL: {e}")))?;

        let host = parsed
            .host_str()
            .ok_or_else(|| SafeAgentError::NetworkNotAllowed("URL has no host".into()))?;

        if !self.allowed_hosts.iter().any(|h| h == host) {
            return Err(SafeAgentError::NetworkNotAllowed(format!(
                "host not in allowlist: {host}"
            )));
        }

        Ok(parsed)
    }

    pub fn get(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let parsed = self.check_url(url)?;
        Ok(self.client.get(parsed))
    }

    pub fn post(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let parsed = self.check_url(url)?;
        Ok(self.client.post(parsed))
    }

    pub fn put(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let parsed = self.check_url(url)?;
        Ok(self.client.put(parsed))
    }

    pub fn delete(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let parsed = self.check_url(url)?;
        Ok(self.client.delete(parsed))
    }
}
