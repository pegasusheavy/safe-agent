use async_trait::async_trait;
use tracing::debug;

use super::{Tool, ToolContext, ToolOutput};
use crate::error::Result;

// -- ReadFile ------------------------------------------------------------

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a file from the sandboxed data directory. Returns the file contents as text."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path within the sandbox"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if path.is_empty() {
            return Ok(ToolOutput::error("path is required"));
        }

        let rel = std::path::Path::new(path);
        debug!(?rel, "reading file");

        match ctx.sandbox.read_to_string(rel) {
            Ok(contents) => Ok(ToolOutput::ok(contents)),
            Err(e) => Ok(ToolOutput::error(format!("failed to read: {e}"))),
        }
    }
}

// -- WriteFile -----------------------------------------------------------

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file in the sandboxed data directory. Creates parent directories as needed."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path within the sandbox"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if path.is_empty() {
            return Ok(ToolOutput::error("path is required"));
        }

        let rel = std::path::Path::new(path);
        debug!(?rel, bytes = content.len(), "writing file");

        match ctx.sandbox.write(rel, content.as_bytes()) {
            Ok(()) => Ok(ToolOutput::ok(format!("Wrote {} bytes to {path}", content.len()))),
            Err(e) => Ok(ToolOutput::error(format!("failed to write: {e}"))),
        }
    }
}

// -- EditFile ------------------------------------------------------------

pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Replace a specific string in a file within the sandbox. Use for targeted edits."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["path", "old_string", "new_string"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path within the sandbox"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact string to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement string"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or_default();
        let old = params.get("old_string").and_then(|v| v.as_str()).unwrap_or_default();
        let new = params.get("new_string").and_then(|v| v.as_str()).unwrap_or_default();

        if path.is_empty() || old.is_empty() {
            return Ok(ToolOutput::error("path and old_string are required"));
        }

        let rel = std::path::Path::new(path);
        let contents = match ctx.sandbox.read_to_string(rel) {
            Ok(c) => c,
            Err(e) => return Ok(ToolOutput::error(format!("failed to read: {e}"))),
        };

        let count = contents.matches(old).count();
        if count == 0 {
            return Ok(ToolOutput::error("old_string not found in file"));
        }

        let updated = contents.replacen(old, new, 1);
        match ctx.sandbox.write(rel, updated.as_bytes()) {
            Ok(()) => Ok(ToolOutput::ok(format!(
                "Replaced 1 of {count} occurrence(s) in {path}"
            ))),
            Err(e) => Ok(ToolOutput::error(format!("failed to write: {e}"))),
        }
    }
}

// -- ApplyPatch ----------------------------------------------------------

pub struct ApplyPatchTool;

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }

    fn description(&self) -> &str {
        "Apply a unified diff patch to files in the sandbox."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["patch"],
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "Unified diff patch content"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let patch = params
            .get("patch")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if patch.is_empty() {
            return Ok(ToolOutput::error("patch content is required"));
        }

        // Write patch to temp file and apply with `patch` command
        let patch_path = ctx.sandbox.resolve(std::path::Path::new(".tmp_patch"))?;
        std::fs::write(&patch_path, patch)?;

        let output = tokio::process::Command::new("patch")
            .arg("-p1")
            .arg("-i")
            .arg(&patch_path)
            .current_dir(ctx.sandbox.root())
            .output()
            .await;

        let _ = std::fs::remove_file(&patch_path);

        match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                let err = String::from_utf8_lossy(&out.stderr);
                if out.status.success() {
                    Ok(ToolOutput::ok(format!("{text}{err}")))
                } else {
                    Ok(ToolOutput::error(format!("patch failed: {text}{err}")))
                }
            }
            Err(e) => Ok(ToolOutput::error(format!("failed to run patch: {e}"))),
        }
    }
}
