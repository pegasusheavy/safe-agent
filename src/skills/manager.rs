use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::{Child, Command};
use tracing::{error, info, warn};

use crate::error::{Result, SafeAgentError};

/// Manifest describing a skill, read from `skill.toml` in the skill directory.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SkillManifest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// "daemon" (long-running) or "oneshot" (run once and exit).
    #[serde(default = "default_skill_type")]
    pub skill_type: String,
    /// Whether the skill should be started automatically.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Entry point relative to the skill directory (default: "main.py").
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
    /// Extra environment variables to pass to the skill process.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Credentials this skill requires. Each entry declares a credential
    /// by name with a human-readable description and whether it's required.
    #[serde(default)]
    pub credentials: Vec<CredentialSpec>,
}

/// Declares a credential that a skill needs.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct CredentialSpec {
    /// Environment variable name the credential is passed as.
    pub name: String,
    /// Human-readable label shown in the dashboard.
    #[serde(default)]
    pub label: String,
    /// Description / help text.
    #[serde(default)]
    pub description: String,
    /// Whether the skill cannot function without this credential.
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_skill_type() -> String {
    "daemon".to_string()
}
fn default_true() -> bool {
    true
}
fn default_entrypoint() -> String {
    "main.py".to_string()
}

/// Tracks a running skill process.
struct RunningSkill {
    manifest: SkillManifest,
    child: Child,
    dir: PathBuf,
}

/// Manages skill lifecycle: discovery, start, stop, restart, credentials.
pub struct SkillManager {
    skills_dir: PathBuf,
    running: HashMap<String, RunningSkill>,
    telegram_bot_token: Option<String>,
    telegram_chat_id: Option<i64>,
    /// Stored credentials: skill_name -> { env_var_name -> value }
    credentials: HashMap<String, HashMap<String, String>>,
    credentials_path: PathBuf,
}

impl SkillManager {
    pub fn new(
        skills_dir: PathBuf,
        telegram_bot_token: Option<String>,
        telegram_chat_id: Option<i64>,
    ) -> Self {
        if let Err(e) = std::fs::create_dir_all(&skills_dir) {
            warn!(path = %skills_dir.display(), err = %e, "failed to create skills directory");
        }

        let credentials_path = skills_dir.join("credentials.json");
        let credentials = Self::load_credentials(&credentials_path);

        info!(
            path = %skills_dir.display(),
            stored_credentials = credentials.len(),
            "skill manager initialized"
        );

        Self {
            skills_dir,
            running: HashMap::new(),
            telegram_bot_token,
            telegram_chat_id,
            credentials,
            credentials_path,
        }
    }

    fn load_credentials(path: &Path) -> HashMap<String, HashMap<String, String>> {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => HashMap::new(),
        }
    }

    fn save_credentials(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.credentials)
            .map_err(|e| SafeAgentError::Config(format!("serialize credentials: {e}")))?;
        std::fs::write(&self.credentials_path, json)
            .map_err(|e| SafeAgentError::Io(e))?;
        Ok(())
    }

    /// Get stored credentials for a skill.
    pub fn get_credentials(&self, skill_name: &str) -> HashMap<String, String> {
        self.credentials.get(skill_name).cloned().unwrap_or_default()
    }

    /// Set a credential value for a skill and persist to disk.
    pub fn set_credential(&mut self, skill_name: &str, key: &str, value: &str) -> Result<()> {
        self.credentials
            .entry(skill_name.to_string())
            .or_default()
            .insert(key.to_string(), value.to_string());
        self.save_credentials()
    }

    /// Delete a credential value for a skill and persist to disk.
    pub fn delete_credential(&mut self, skill_name: &str, key: &str) -> Result<()> {
        if let Some(creds) = self.credentials.get_mut(skill_name) {
            creds.remove(key);
            if creds.is_empty() {
                self.credentials.remove(skill_name);
            }
        }
        self.save_credentials()
    }

    /// Scan the skills directory, start new enabled skills, restart crashed ones,
    /// and stop skills whose directories have been deleted.
    ///
    /// Called every tick from the agent loop.
    pub async fn reconcile(&mut self) -> Result<()> {
        // Reap finished processes first
        self.reap_finished().await;

        // Scan for skill directories
        let entries = match std::fs::read_dir(&self.skills_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(err = %e, "failed to read skills directory");
                return Ok(());
            }
        };

        // Collect the names of skills that still exist on disk so we can
        // detect deletions after the scan.
        let mut on_disk: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("skill.toml");
            if !manifest_path.exists() {
                continue;
            }

            let manifest = match self.read_manifest(&manifest_path) {
                Ok(m) => m,
                Err(e) => {
                    warn!(path = %manifest_path.display(), err = %e, "bad skill manifest");
                    continue;
                }
            };

            on_disk.insert(manifest.name.clone());

            if !manifest.enabled {
                // If it's running but now disabled, stop it
                if self.running.contains_key(&manifest.name) {
                    info!(skill = %manifest.name, "stopping disabled skill");
                    self.stop_skill(&manifest.name).await;
                }
                continue;
            }

            // If not already running, start it
            if !self.running.contains_key(&manifest.name) {
                self.start_skill(manifest, path).await;
            }
        }

        // Stop any running skills whose directories were deleted
        let orphaned: Vec<String> = self
            .running
            .keys()
            .filter(|name| !on_disk.contains(name.as_str()))
            .cloned()
            .collect();

        for name in orphaned {
            info!(skill = %name, "skill directory removed, stopping orphaned process");
            self.stop_skill(&name).await;
        }

        Ok(())
    }

    /// Start a skill process.
    async fn start_skill(&mut self, manifest: SkillManifest, dir: PathBuf) {
        let entrypoint = dir.join(&manifest.entrypoint);
        if !entrypoint.exists() {
            warn!(
                skill = %manifest.name,
                entrypoint = %entrypoint.display(),
                "skill entrypoint not found"
            );
            return;
        }

        // Install requirements if present
        let requirements = dir.join("requirements.txt");
        if requirements.exists() {
            info!(skill = %manifest.name, "installing skill requirements");
            let install = Command::new("pip3")
                .args(["install", "--no-cache-dir", "--break-system-packages", "-r"])
                .arg(&requirements)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .status()
                .await;

            match install {
                Ok(s) if s.success() => {
                    info!(skill = %manifest.name, "requirements installed");
                }
                Ok(s) => {
                    warn!(skill = %manifest.name, status = %s, "pip install failed");
                }
                Err(e) => {
                    warn!(skill = %manifest.name, err = %e, "pip install error");
                }
            }
        }

        // Determine the interpreter from the entrypoint extension
        let interpreter = if manifest.entrypoint.ends_with(".py") {
            "python3"
        } else {
            "sh"
        };

        let log_path = dir.join("skill.log");

        let log_file = match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(f) => f,
            Err(e) => {
                error!(skill = %manifest.name, err = %e, "failed to open skill log");
                return;
            }
        };
        let stderr_log = match log_file.try_clone() {
            Ok(f) => f,
            Err(e) => {
                error!(skill = %manifest.name, err = %e, "failed to clone log file handle");
                return;
            }
        };

        let mut cmd = Command::new(interpreter);
        cmd.arg(&entrypoint)
            .current_dir(&dir)
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(stderr_log))
            .env("SKILL_NAME", &manifest.name)
            .env("SKILL_DIR", &dir)
            .env("SKILL_DATA_DIR", dir.join("data"))
            .env("SKILLS_DIR", &self.skills_dir);

        // Put the skill in its own process group so we can kill the entire
        // group (including any child processes) on stop.
        #[cfg(unix)]
        #[allow(unused_imports)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        if let Some(ref token) = self.telegram_bot_token {
            cmd.env("TELEGRAM_BOT_TOKEN", token);
        }
        if let Some(chat_id) = self.telegram_chat_id {
            cmd.env("TELEGRAM_CHAT_ID", chat_id.to_string());
        }

        // Pass any extra env vars from the manifest
        for (k, v) in &manifest.env {
            cmd.env(k, v);
        }

        // Inject stored credentials
        if let Some(creds) = self.credentials.get(&manifest.name) {
            for (k, v) in creds {
                cmd.env(k, v);
            }
        }

        // Create skill data directory
        let _ = std::fs::create_dir_all(dir.join("data"));

        match cmd.spawn() {
            Ok(child) => {
                info!(
                    skill = %manifest.name,
                    pid = ?child.id(),
                    entrypoint = %manifest.entrypoint,
                    "skill started"
                );
                self.running.insert(
                    manifest.name.clone(),
                    RunningSkill {
                        manifest,
                        child,
                        dir,
                    },
                );
            }
            Err(e) => {
                error!(skill = %manifest.name, err = %e, "failed to start skill");
            }
        }
    }

    /// Stop a running skill by name, killing the entire process group.
    pub async fn stop_skill(&mut self, name: &str) {
        if let Some(mut skill) = self.running.remove(name) {
            if let Some(pid) = skill.child.id() {
                info!(skill = %name, pid, "stopping skill (killing process group)");
                // Kill the entire process group so child processes don't linger.
                // On Unix, negate the PID to target the process group.
                #[cfg(unix)]
                {
                    unsafe {
                        libc::kill(-(pid as i32), libc::SIGTERM);
                    }
                    // Give the process a moment to exit gracefully, then force-kill.
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    unsafe {
                        libc::kill(-(pid as i32), libc::SIGKILL);
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = skill.child.kill().await;
                }
            } else {
                info!(skill = %name, "stopping skill");
                let _ = skill.child.kill().await;
            }
            // Ensure we reap the child
            let _ = skill.child.wait().await;
        }
    }

    /// Check running skills for any that have exited, and remove them so
    /// they can be restarted on the next reconcile.
    async fn reap_finished(&mut self) {
        let mut finished = Vec::new();

        for (name, skill) in &mut self.running {
            match skill.child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() && skill.manifest.skill_type == "oneshot" {
                        info!(skill = %name, "oneshot skill completed");
                    } else if status.success() {
                        info!(skill = %name, "daemon skill exited (will restart)");
                    } else {
                        warn!(
                            skill = %name,
                            status = %status,
                            "skill exited with error (will restart)"
                        );
                    }
                    finished.push(name.clone());
                }
                Ok(None) => {} // still running
                Err(e) => {
                    warn!(skill = %name, err = %e, "error checking skill status");
                    finished.push(name.clone());
                }
            }
        }

        for name in &finished {
            self.running.remove(name);
        }
    }

    fn read_manifest(&self, path: &Path) -> Result<SkillManifest> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| SafeAgentError::Config(format!("read skill manifest: {e}")))?;
        toml::from_str(&contents)
            .map_err(|e| SafeAgentError::Config(format!("parse skill manifest: {e}")))
    }

    /// List all skills (running and discovered).
    pub fn list(&self) -> Vec<SkillStatus> {
        let mut result = Vec::new();

        let entries = match std::fs::read_dir(&self.skills_dir) {
            Ok(e) => e,
            Err(_) => return result,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("skill.toml");
            if !manifest_path.exists() {
                continue;
            }

            if let Ok(manifest) = self.read_manifest(&manifest_path) {
                let name = manifest.name.clone();
                let running = self.running.contains_key(&name);
                let pid = self
                    .running
                    .get(&name)
                    .and_then(|s| s.child.id());

                let stored = self.get_credentials(&name);
                let credential_status: Vec<CredentialStatus> = manifest
                    .credentials
                    .iter()
                    .map(|spec| {
                        let configured = stored.contains_key(&spec.name);
                        CredentialStatus {
                            name: spec.name.clone(),
                            label: if spec.label.is_empty() {
                                spec.name.clone()
                            } else {
                                spec.label.clone()
                            },
                            description: spec.description.clone(),
                            required: spec.required,
                            configured,
                        }
                    })
                    .collect();

                result.push(SkillStatus {
                    name,
                    description: manifest.description,
                    skill_type: manifest.skill_type,
                    enabled: manifest.enabled,
                    running,
                    pid,
                    credentials: credential_status,
                });
            }
        }

        result
    }

    /// Stop all running skills (called on shutdown).
    pub async fn shutdown(&mut self) {
        let names: Vec<String> = self.running.keys().cloned().collect();
        for name in names {
            self.stop_skill(&name).await;
        }
        info!("all skills stopped");
    }
}

#[derive(Debug, serde::Serialize)]
pub struct SkillStatus {
    pub name: String,
    pub description: String,
    pub skill_type: String,
    pub enabled: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub credentials: Vec<CredentialStatus>,
}

#[derive(Debug, serde::Serialize)]
pub struct CredentialStatus {
    pub name: String,
    pub label: String,
    pub description: String,
    pub required: bool,
    pub configured: bool,
}
