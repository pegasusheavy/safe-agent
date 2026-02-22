use serde::{Deserialize, Serialize};

/// How to install a binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InstallMethod {
    /// Download a pre-built binary or archive from a URL.
    Download {
        /// URL template with `{arch}` and `{version}` placeholders.
        url_template: String,
        /// Archive format (if any).
        archive_format: ArchiveFormat,
        /// Filename of the binary inside the archive (or the downloaded file).
        binary_name: String,
        /// URL that returns the latest version string (optional).
        latest_version_url: Option<String>,
        /// Command + args to detect installed version, e.g. ["--version"].
        version_args: Vec<String>,
    },
    /// Install via npm global with --prefix.
    Npm {
        package: String,
        version_args: Vec<String>,
    },
    /// Install via pip --user.
    Pip {
        package: String,
        version_args: Vec<String>,
    },
}

/// Archive format for downloaded binaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchiveFormat {
    /// Raw binary, no archive.
    None,
    /// .tar.gz archive.
    TarGz,
    /// .zip archive.
    Zip,
}

/// Definition of an installable binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryDef {
    /// Short machine name, e.g. "ngrok".
    pub name: String,
    /// Human-readable name, e.g. "Ngrok".
    pub display_name: String,
    /// One-line description.
    pub description: String,
    /// How to install it.
    pub install_method: InstallMethod,
}

/// Return the built-in registry of known installable binaries.
pub fn builtin_registry() -> Vec<BinaryDef> {
    vec![
        BinaryDef {
            name: "ngrok".into(),
            display_name: "Ngrok".into(),
            description: "Secure tunnel to localhost".into(),
            install_method: InstallMethod::Download {
                url_template: "https://bin.equinox.io/c/bNyj1mQVY4c/ngrok-v3-stable-linux-{arch}.zip".into(),
                archive_format: ArchiveFormat::Zip,
                binary_name: "ngrok".into(),
                latest_version_url: None,
                version_args: vec!["version".into()],
            },
        },
        BinaryDef {
            name: "cloudflared".into(),
            display_name: "Cloudflare Tunnel".into(),
            description: "Cloudflare Tunnel client".into(),
            install_method: InstallMethod::Download {
                url_template: "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-{arch}".into(),
                archive_format: ArchiveFormat::None,
                binary_name: "cloudflared".into(),
                latest_version_url: None,
                version_args: vec!["version".into()],
            },
        },
        BinaryDef {
            name: "tailscale".into(),
            display_name: "Tailscale".into(),
            description: "Tailscale VPN / Funnel / Serve".into(),
            install_method: InstallMethod::Download {
                url_template: "https://pkgs.tailscale.com/stable/tailscale_latest_{arch}.tgz".into(),
                archive_format: ArchiveFormat::TarGz,
                binary_name: "tailscale".into(),
                latest_version_url: None,
                version_args: vec!["version".into()],
            },
        },
        BinaryDef {
            name: "claude".into(),
            display_name: "Claude Code CLI".into(),
            description: "Anthropic Claude Code command-line tool".into(),
            install_method: InstallMethod::Npm {
                package: "@anthropic-ai/claude-code".into(),
                version_args: vec!["--version".into()],
            },
        },
        BinaryDef {
            name: "aider".into(),
            display_name: "Aider".into(),
            description: "AI pair programming in the terminal".into(),
            install_method: InstallMethod::Pip {
                package: "aider-chat".into(),
                version_args: vec!["--version".into()],
            },
        },
        BinaryDef {
            name: "codex".into(),
            display_name: "OpenAI Codex CLI".into(),
            description: "OpenAI coding agent for the terminal".into(),
            install_method: InstallMethod::Npm {
                package: "@openai/codex".into(),
                version_args: vec!["--version".into()],
            },
        },
        BinaryDef {
            name: "gemini".into(),
            display_name: "Google Gemini CLI".into(),
            description: "Google Gemini AI agent for the terminal".into(),
            install_method: InstallMethod::Npm {
                package: "@google/gemini-cli".into(),
                version_args: vec!["--version".into()],
            },
        },
        BinaryDef {
            name: "ollama".into(),
            display_name: "Ollama".into(),
            description: "Run large language models locally".into(),
            install_method: InstallMethod::Download {
                url_template: "https://ollama.com/download/ollama-linux-{arch}.tgz".into(),
                archive_format: ArchiveFormat::TarGz,
                binary_name: "ollama".into(),
                latest_version_url: None,
                version_args: vec!["--version".into()],
            },
        },
    ]
}
