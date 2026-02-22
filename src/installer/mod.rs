pub mod download;
pub mod registry;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::error::{Result, SafeAgentError};
use registry::{BinaryDef, InstallMethod};

/// Persisted state for a single installed binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryState {
    pub version: String,
    pub path: String,
    pub installed_at: String,
    pub status: BinaryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Install status of a binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BinaryStatus {
    Installed,
    Installing,
    Failed,
}

/// Info returned by the list/get endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct BinaryInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
    #[serde(flatten)]
    pub state: Option<BinaryState>,
}

/// Manages installing, uninstalling, and tracking tool binaries.
#[derive(Clone)]
pub struct BinaryInstaller {
    install_dir: PathBuf,
    state_path: PathBuf,
    registry: Vec<BinaryDef>,
    lock: Arc<Mutex<()>>,
}

impl BinaryInstaller {
    pub fn new(install_dir: PathBuf, data_dir: &Path) -> Self {
        Self {
            install_dir,
            state_path: data_dir.join("installed-binaries.json"),
            registry: registry::builtin_registry(),
            lock: Arc::new(Mutex::new(())),
        }
    }

    /// Ensure the install directory exists.
    pub fn ensure_install_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.install_dir)
            .map_err(|e| SafeAgentError::Config(format!(
                "failed to create install dir {}: {e}",
                self.install_dir.display()
            )))?;
        Ok(())
    }

    /// List all known binaries with their install state.
    pub fn list(&self) -> Vec<BinaryInfo> {
        let states = self.load_state();
        self.registry.iter().map(|def| {
            BinaryInfo {
                name: def.name.clone(),
                display_name: def.display_name.clone(),
                description: def.description.clone(),
                state: states.get(&def.name).cloned(),
            }
        }).collect()
    }

    /// Get info for a single binary.
    pub fn get(&self, name: &str) -> Option<BinaryInfo> {
        let def = self.registry.iter().find(|d| d.name == name)?;
        let states = self.load_state();
        Some(BinaryInfo {
            name: def.name.clone(),
            display_name: def.display_name.clone(),
            description: def.description.clone(),
            state: states.get(name).cloned(),
        })
    }

    /// Install a binary by name.
    pub async fn install(&self, name: &str) -> Result<BinaryState> {
        let def = self.registry.iter().find(|d| d.name == name)
            .ok_or_else(|| SafeAgentError::Config(format!(
                "unknown binary: {name}"
            )))?
            .clone();

        // Check for concurrent install
        {
            let states = self.load_state();
            if let Some(s) = states.get(name) {
                if s.status == BinaryStatus::Installing {
                    return Err(SafeAgentError::Config(format!(
                        "{name} is already being installed"
                    )));
                }
            }
        }

        // Mark as installing
        self.update_state(name, BinaryState {
            version: String::new(),
            path: String::new(),
            installed_at: String::new(),
            status: BinaryStatus::Installing,
            error: None,
        });

        let _guard = self.lock.lock().await;
        self.ensure_install_dir()?;

        let result = match &def.install_method {
            InstallMethod::Download {
                url_template,
                archive_format,
                binary_name,
                version_args,
                ..
            } => {
                let arch = download::detect_arch();
                let url = url_template
                    .replace("{arch}", arch)
                    .replace("{version}", "latest");

                match download::fetch_url(&url).await {
                    Ok(data) => {
                        let dest = self.install_dir.join(binary_name);
                        match download::extract_binary(&data, *archive_format, binary_name, &dest) {
                            Ok(()) => {
                                let version = self.detect_version(&dest, version_args).await;
                                Ok(BinaryState {
                                    version,
                                    path: dest.to_string_lossy().to_string(),
                                    installed_at: chrono::Utc::now().to_rfc3339(),
                                    status: BinaryStatus::Installed,
                                    error: None,
                                })
                            }
                            Err(e) => Err(e),
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            InstallMethod::Npm { package, version_args } => {
                let prefix = self.install_dir.parent()
                    .unwrap_or(&self.install_dir)
                    .to_string_lossy()
                    .to_string();

                let output = Command::new("npm")
                    .arg("install").arg("-g")
                    .arg("--prefix").arg(&prefix)
                    .arg(package)
                    .output()
                    .await
                    .map_err(|e| SafeAgentError::Config(format!(
                        "npm install failed: {e}"
                    )))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(SafeAgentError::Config(format!(
                        "npm install failed: {stderr}"
                    )));
                }

                let bin_name = package.rsplit('/').next().unwrap_or(package);
                let dest = self.install_dir.join(bin_name);
                let version = self.detect_version(&dest, version_args).await;

                Ok(BinaryState {
                    version,
                    path: dest.to_string_lossy().to_string(),
                    installed_at: chrono::Utc::now().to_rfc3339(),
                    status: BinaryStatus::Installed,
                    error: None,
                })
            }
            InstallMethod::Pip { package, version_args } => {
                let output = Command::new("pip")
                    .arg("install")
                    .arg("--user")
                    .arg("--break-system-packages")
                    .arg(package)
                    .output()
                    .await
                    .map_err(|e| SafeAgentError::Config(format!(
                        "pip install failed: {e}"
                    )))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(SafeAgentError::Config(format!(
                        "pip install failed: {stderr}"
                    )));
                }

                // pip installs by the command name, not the package name
                // e.g. "aider-chat" installs as "aider"
                let bin_name = package.split('[').next().unwrap_or(package);
                let dest = self.install_dir.join(bin_name);
                let version = self.detect_version(&dest, version_args).await;

                Ok(BinaryState {
                    version,
                    path: dest.to_string_lossy().to_string(),
                    installed_at: chrono::Utc::now().to_rfc3339(),
                    status: BinaryStatus::Installed,
                    error: None,
                })
            }
        };

        match result {
            Ok(state) => {
                info!(name, version = %state.version, "binary installed successfully");
                self.update_state(name, state.clone());
                Ok(state)
            }
            Err(e) => {
                error!(name, err = %e, "binary install failed");
                self.update_state(name, BinaryState {
                    version: String::new(),
                    path: String::new(),
                    installed_at: String::new(),
                    status: BinaryStatus::Failed,
                    error: Some(e.to_string()),
                });
                Err(e)
            }
        }
    }

    /// Uninstall a binary by name.
    pub async fn uninstall(&self, name: &str) -> Result<()> {
        let def = self.registry.iter().find(|d| d.name == name)
            .ok_or_else(|| SafeAgentError::Config(format!(
                "unknown binary: {name}"
            )))?;

        let states = self.load_state();
        let state = states.get(name)
            .ok_or_else(|| SafeAgentError::Config(format!(
                "{name} is not installed"
            )))?;

        if state.status == BinaryStatus::Installing {
            return Err(SafeAgentError::Config(format!(
                "{name} is currently being installed"
            )));
        }

        match &def.install_method {
            InstallMethod::Download { binary_name, .. } => {
                let path = self.install_dir.join(binary_name);
                if path.exists() {
                    std::fs::remove_file(&path)
                        .map_err(|e| SafeAgentError::Config(format!(
                            "failed to remove {}: {e}", path.display()
                        )))?;
                }
            }
            InstallMethod::Npm { package, .. } => {
                let prefix = self.install_dir.parent()
                    .unwrap_or(&self.install_dir)
                    .to_string_lossy()
                    .to_string();

                let _ = Command::new("npm")
                    .arg("uninstall").arg("-g")
                    .arg("--prefix").arg(&prefix)
                    .arg(package)
                    .output()
                    .await;
            }
            InstallMethod::Pip { package, .. } => {
                let _ = Command::new("pip")
                    .arg("uninstall").arg("-y")
                    .arg("--break-system-packages")
                    .arg(package)
                    .output()
                    .await;
            }
        }

        self.remove_state(name);
        info!(name, "binary uninstalled");
        Ok(())
    }

    // ---------------------------------------------------------------
    // State file helpers
    // ---------------------------------------------------------------

    fn load_state(&self) -> HashMap<String, BinaryState> {
        match std::fs::read_to_string(&self.state_path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => HashMap::new(),
        }
    }

    fn save_state(&self, states: &HashMap<String, BinaryState>) {
        if let Ok(json) = serde_json::to_string_pretty(states) {
            let _ = std::fs::write(&self.state_path, json);
        }
    }

    fn update_state(&self, name: &str, state: BinaryState) {
        let mut states = self.load_state();
        states.insert(name.to_string(), state);
        self.save_state(&states);
    }

    fn remove_state(&self, name: &str) {
        let mut states = self.load_state();
        states.remove(name);
        self.save_state(&states);
    }

    // ---------------------------------------------------------------
    // Version detection
    // ---------------------------------------------------------------

    async fn detect_version(&self, bin_path: &Path, args: &[String]) -> String {
        let result = Command::new(bin_path)
            .args(args)
            .output()
            .await;

        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = if stdout.trim().is_empty() {
                    stderr.trim().to_string()
                } else {
                    stdout.trim().to_string()
                };
                combined.lines().next().unwrap_or("unknown").to_string()
            }
            Err(_) => "unknown".to_string(),
        }
    }
}
