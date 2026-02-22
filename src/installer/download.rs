use std::io::{Cursor, Read};
use std::path::Path;

use tracing::{info, error};

use crate::error::{Result, SafeAgentError};
use super::registry::ArchiveFormat;

/// Detect the platform architecture string for download URLs.
///
/// Returns "amd64" or "arm64" (the most common naming convention).
pub fn detect_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => {
            error!(arch = other, "unsupported architecture");
            "amd64" // fallback
        }
    }
}

/// Download a file from a URL and return the bytes.
pub async fn fetch_url(url: &str) -> Result<Vec<u8>> {
    info!(url, "downloading binary");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| SafeAgentError::Config(format!("http client error: {e}")))?;

    let resp = client.get(url).send().await
        .map_err(|e| SafeAgentError::Config(format!("download failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(SafeAgentError::Config(format!(
            "download returned HTTP {}", resp.status()
        )));
    }

    let bytes = resp.bytes().await
        .map_err(|e| SafeAgentError::Config(format!("download read error: {e}")))?;

    info!(bytes = bytes.len(), "download complete");
    Ok(bytes.to_vec())
}

/// Extract a binary from downloaded bytes and write it to `dest_path`.
///
/// For `ArchiveFormat::None`, writes the bytes directly.
/// For archives, finds `binary_name` inside and extracts it.
pub fn extract_binary(
    data: &[u8],
    format: ArchiveFormat,
    binary_name: &str,
    dest_path: &Path,
) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| SafeAgentError::Config(format!(
                "failed to create install dir: {e}"
            )))?;
    }

    match format {
        ArchiveFormat::None => {
            std::fs::write(dest_path, data)
                .map_err(|e| SafeAgentError::Config(format!(
                    "failed to write binary: {e}"
                )))?;
        }
        ArchiveFormat::TarGz => {
            let decoder = flate2::read::GzDecoder::new(Cursor::new(data));
            let mut archive = tar::Archive::new(decoder);

            let mut found = false;
            for entry in archive.entries()
                .map_err(|e| SafeAgentError::Config(format!("tar read error: {e}")))?
            {
                let mut entry = entry
                    .map_err(|e| SafeAgentError::Config(format!("tar entry error: {e}")))?;

                let path = entry.path()
                    .map_err(|e| SafeAgentError::Config(format!("tar path error: {e}")))?;

                // Match on filename (last component) since archives may have directory prefixes
                if path.file_name().and_then(|f| f.to_str()) == Some(binary_name) {
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf)
                        .map_err(|e| SafeAgentError::Config(format!(
                            "tar extract error: {e}"
                        )))?;
                    std::fs::write(dest_path, &buf)
                        .map_err(|e| SafeAgentError::Config(format!(
                            "failed to write extracted binary: {e}"
                        )))?;
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(SafeAgentError::Config(format!(
                    "binary '{binary_name}' not found in tar.gz archive"
                )));
            }
        }
        ArchiveFormat::Zip => {
            let cursor = Cursor::new(data);
            let mut archive = zip::ZipArchive::new(cursor)
                .map_err(|e| SafeAgentError::Config(format!("zip read error: {e}")))?;

            let mut found = false;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i)
                    .map_err(|e| SafeAgentError::Config(format!(
                        "zip entry error: {e}"
                    )))?;

                let name = file.name().rsplit('/').next().unwrap_or(file.name());
                if name == binary_name {
                    let mut buf = Vec::new();
                    file.read_to_end(&mut buf)
                        .map_err(|e| SafeAgentError::Config(format!(
                            "zip extract error: {e}"
                        )))?;
                    std::fs::write(dest_path, &buf)
                        .map_err(|e| SafeAgentError::Config(format!(
                            "failed to write extracted binary: {e}"
                        )))?;
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(SafeAgentError::Config(format!(
                    "binary '{binary_name}' not found in zip archive"
                )));
            }
        }
    }

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(dest_path, perms)
            .map_err(|e| SafeAgentError::Config(format!(
                "failed to set executable permission: {e}"
            )))?;
    }

    info!(path = %dest_path.display(), "binary installed");
    Ok(())
}
