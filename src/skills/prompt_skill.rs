use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::{Result, SafeAgentError};

/// A prompt-based skill loaded from a SKILL.md file.
///
/// Unlike subprocess skills (skill.toml), prompt skills inject their
/// markdown body into the LLM system prompt to augment agent behavior
/// without running external processes.
#[derive(Debug, Clone)]
pub struct PromptSkill {
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub triggers: Vec<String>,
    pub body: String,
    pub references: HashMap<String, String>,
}

/// Intermediate struct for deserializing YAML frontmatter.
#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    triggers: Vec<String>,
}

fn default_enabled() -> bool {
    true
}

impl PromptSkill {
    /// Load a prompt skill from a directory containing a SKILL.md file.
    ///
    /// The SKILL.md must have YAML frontmatter delimited by `---` lines,
    /// followed by a markdown body. If a `references/` subdirectory exists,
    /// all `.md` files within it are loaded into the references map.
    pub fn load(dir: &Path) -> Result<Self> {
        let skill_path = dir.join("SKILL.md");
        let content = std::fs::read_to_string(&skill_path).map_err(|e| {
            SafeAgentError::Plugin(format!(
                "failed to read SKILL.md in {}: {}",
                dir.display(),
                e
            ))
        })?;

        let (frontmatter, body) = Self::parse_frontmatter(&content, dir)?;

        let references = Self::load_references(dir)?;

        Ok(PromptSkill {
            name: frontmatter.name,
            description: frontmatter.description,
            enabled: frontmatter.enabled,
            triggers: frontmatter.triggers,
            body,
            references,
        })
    }

    /// Check if any trigger phrase matches the given input (case-insensitive).
    pub fn matches_trigger(&self, input: &str) -> bool {
        let input_lower = input.to_lowercase();
        self.triggers
            .iter()
            .any(|trigger| input_lower.contains(&trigger.to_lowercase()))
    }

    /// Parse YAML frontmatter delimited by `---` lines and extract the body.
    fn parse_frontmatter(content: &str, dir: &Path) -> Result<(Frontmatter, String)> {
        let trimmed = content.trim_start();

        if !trimmed.starts_with("---") {
            return Err(SafeAgentError::Plugin(format!(
                "SKILL.md in {} missing frontmatter delimiters (---)",
                dir.display()
            )));
        }

        // Find the closing ---
        let after_opening = &trimmed[3..];
        let closing_pos = after_opening.find("\n---").ok_or_else(|| {
            SafeAgentError::Plugin(format!(
                "SKILL.md in {} missing closing frontmatter delimiter (---)",
                dir.display()
            ))
        })?;

        let yaml_str = &after_opening[..closing_pos];
        let body_start = closing_pos + 4; // skip "\n---"
        let body = if body_start < after_opening.len() {
            after_opening[body_start..].trim().to_string()
        } else {
            String::new()
        };

        let frontmatter: Frontmatter = serde_yaml::from_str(yaml_str).map_err(|e| {
            SafeAgentError::Plugin(format!(
                "invalid YAML frontmatter in {}: {}",
                dir.display(),
                e
            ))
        })?;

        Ok((frontmatter, body))
    }

    /// Load all `.md` files from the `references/` subdirectory, if it exists.
    fn load_references(dir: &Path) -> Result<HashMap<String, String>> {
        let refs_dir = dir.join("references");
        let mut refs = HashMap::new();

        if !refs_dir.is_dir() {
            return Ok(refs);
        }

        let entries = std::fs::read_dir(&refs_dir).map_err(|e| {
            SafeAgentError::Plugin(format!(
                "failed to read references/ in {}: {}",
                dir.display(),
                e
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                SafeAgentError::Plugin(format!("failed to read reference entry: {}", e))
            })?;
            let path = entry.path();

            if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();

                let content = std::fs::read_to_string(&path).map_err(|e| {
                    SafeAgentError::Plugin(format!(
                        "failed to read reference file {}: {}",
                        path.display(),
                        e
                    ))
                })?;

                refs.insert(filename, content);
            }
        }

        Ok(refs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a SKILL.md in a temp directory with the given content.
    fn write_skill_md(dir: &Path, content: &str) {
        std::fs::write(dir.join("SKILL.md"), content).unwrap();
    }

    #[test]
    fn parse_minimal_skill_md() {
        let tmp = TempDir::new().unwrap();
        write_skill_md(
            tmp.path(),
            "---\nname: minimal\ndescription: A minimal skill\n---\n\nHello world.",
        );

        let skill = PromptSkill::load(tmp.path()).unwrap();
        assert_eq!(skill.name, "minimal");
        assert_eq!(skill.description, "A minimal skill");
        assert!(skill.enabled);
        assert!(skill.triggers.is_empty());
        assert_eq!(skill.body, "Hello world.");
        assert!(skill.references.is_empty());
    }

    #[test]
    fn parse_full_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let content = r#"---
name: full-skill
description: A full skill with all fields
version: 1.2.3
enabled: true
tools:
  - exec
  - web_search
triggers:
  - simplify code
  - clean up
---

# Full Skill

This is the body with **markdown**."#;

        write_skill_md(tmp.path(), content);

        let skill = PromptSkill::load(tmp.path()).unwrap();
        assert_eq!(skill.name, "full-skill");
        assert_eq!(skill.description, "A full skill with all fields");
        assert!(skill.enabled);
        assert_eq!(skill.triggers, vec!["simplify code", "clean up"]);
        assert!(skill.body.contains("# Full Skill"));
        assert!(skill.body.contains("**markdown**"));
    }

    #[test]
    fn load_references() {
        let tmp = TempDir::new().unwrap();
        write_skill_md(
            tmp.path(),
            "---\nname: with-refs\ndescription: Has references\n---\n\nBody.",
        );

        let refs_dir = tmp.path().join("references");
        std::fs::create_dir(&refs_dir).unwrap();
        std::fs::write(refs_dir.join("style-guide.md"), "# Style Guide\nUse snake_case.").unwrap();
        std::fs::write(refs_dir.join("patterns.md"), "# Patterns\nUse builder pattern.").unwrap();
        // Non-.md file should be ignored
        std::fs::write(refs_dir.join("notes.txt"), "not included").unwrap();

        let skill = PromptSkill::load(tmp.path()).unwrap();
        assert_eq!(skill.references.len(), 2);
        assert_eq!(
            skill.references.get("style-guide.md").unwrap(),
            "# Style Guide\nUse snake_case."
        );
        assert_eq!(
            skill.references.get("patterns.md").unwrap(),
            "# Patterns\nUse builder pattern."
        );
    }

    #[test]
    fn missing_skill_md_returns_error() {
        let tmp = TempDir::new().unwrap();
        let result = PromptSkill::load(tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("failed to read SKILL.md"), "got: {}", err);
    }

    #[test]
    fn missing_frontmatter_returns_error() {
        let tmp = TempDir::new().unwrap();
        write_skill_md(tmp.path(), "# No Frontmatter\n\nJust markdown.");

        let result = PromptSkill::load(tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("missing frontmatter delimiters"),
            "got: {}",
            err
        );
    }

    #[test]
    fn disabled_skill() {
        let tmp = TempDir::new().unwrap();
        write_skill_md(
            tmp.path(),
            "---\nname: off\ndescription: Disabled skill\nenabled: false\n---\n\nBody.",
        );

        let skill = PromptSkill::load(tmp.path()).unwrap();
        assert_eq!(skill.name, "off");
        assert!(!skill.enabled);
    }

    #[test]
    fn matches_trigger_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        let content = "---\nname: t\ndescription: d\ntriggers:\n  - simplify code\n---\n\nBody.";
        write_skill_md(tmp.path(), content);

        let skill = PromptSkill::load(tmp.path()).unwrap();
        assert!(skill.matches_trigger("Please SIMPLIFY CODE for me"));
        assert!(skill.matches_trigger("simplify code"));
        assert!(!skill.matches_trigger("optimize code"));
    }
}
