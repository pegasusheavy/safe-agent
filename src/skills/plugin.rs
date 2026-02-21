use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::{info, warn};

use crate::error::{Result, SafeAgentError};

use super::prompt_skill::PromptSkill;

/// Parsed representation of a `plugin.json` manifest file.
///
/// Explicit plugins include a `plugin.json` at their root that declares
/// metadata (name, version, author) and lists skill subdirectories via
/// glob patterns. Implicit plugins (bare SKILL.md or skill.toml dirs)
/// get a synthetic manifest generated at load time.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PluginManifest {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

/// A fully loaded plugin with its manifest and discovered skills.
#[derive(Debug)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub dir: PathBuf,
    pub prompt_skills: Vec<PromptSkill>,
    pub subprocess_skill_dirs: Vec<PathBuf>,
}

/// Registry that discovers, loads, and indexes plugins from the filesystem.
///
/// Supports two discovery modes:
/// - Explicit: directory contains `plugin.json` with skill globs
/// - Implicit: directory contains `SKILL.md` or `skill.toml` directly
///
/// Disabled plugin names are skipped during scanning.
pub struct PluginRegistry {
    plugins: Vec<LoadedPlugin>,
    disabled: Vec<String>,
}

impl PluginRegistry {
    /// Create a new empty registry with the given list of disabled plugin names.
    pub fn new(disabled: Vec<String>) -> Self {
        Self {
            plugins: Vec::new(),
            disabled,
        }
    }

    /// Scan a directory for plugins and load them.
    ///
    /// Each immediate child directory of `dir` is examined as a potential
    /// plugin. Returns the number of plugins successfully loaded, or 0 if
    /// the directory does not exist or is empty.
    pub fn scan_dir(&mut self, dir: &Path) -> Result<usize> {
        if !dir.is_dir() {
            warn!("plugin directory does not exist: {}", dir.display());
            return Ok(0);
        }

        let entries = std::fs::read_dir(dir).map_err(|e| {
            SafeAgentError::Plugin(format!(
                "failed to read plugin directory {}: {}",
                dir.display(),
                e
            ))
        })?;

        let mut count = 0;

        for entry in entries {
            let entry = entry.map_err(|e| {
                SafeAgentError::Plugin(format!("failed to read directory entry: {}", e))
            })?;

            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            // Derive plugin name from directory name for disabled check
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();

            if self.disabled.contains(&dir_name.to_string()) {
                info!("skipping disabled plugin: {}", dir_name);
                continue;
            }

            match self.load_plugin(&path) {
                Ok(plugin) => {
                    // Also check the manifest name against disabled list
                    if self.disabled.contains(&plugin.manifest.name) {
                        info!(
                            "skipping disabled plugin (by manifest name): {}",
                            plugin.manifest.name
                        );
                        continue;
                    }
                    info!("loaded plugin: {} from {}", plugin.manifest.name, path.display());
                    self.plugins.push(plugin);
                    count += 1;
                }
                Err(e) => {
                    warn!("skipping directory {}: {}", path.display(), e);
                }
            }
        }

        Ok(count)
    }

    /// Load a single plugin from a directory.
    ///
    /// Determines whether the directory is an explicit plugin (has
    /// `plugin.json`) or an implicit single-skill plugin (has `SKILL.md`
    /// or `skill.toml` directly).
    fn load_plugin(&self, dir: &Path) -> Result<LoadedPlugin> {
        let manifest_path = dir.join("plugin.json");

        if manifest_path.is_file() {
            // Explicit plugin with plugin.json
            self.load_explicit_plugin(dir, &manifest_path)
        } else if dir.join("SKILL.md").is_file() || dir.join("skill.toml").is_file() {
            // Implicit single-skill plugin
            self.load_implicit_plugin(dir)
        } else {
            Err(SafeAgentError::Plugin(format!(
                "no plugin.json, SKILL.md, or skill.toml found in {}",
                dir.display()
            )))
        }
    }

    /// Load an explicit plugin that has a `plugin.json` manifest.
    fn load_explicit_plugin(&self, dir: &Path, manifest_path: &Path) -> Result<LoadedPlugin> {
        let content = std::fs::read_to_string(manifest_path).map_err(|e| {
            SafeAgentError::Plugin(format!(
                "failed to read plugin.json in {}: {}",
                dir.display(),
                e
            ))
        })?;

        let manifest: PluginManifest = serde_json::from_str(&content).map_err(|e| {
            SafeAgentError::Plugin(format!(
                "invalid plugin.json in {}: {}",
                dir.display(),
                e
            ))
        })?;

        let mut prompt_skills = Vec::new();
        let mut subprocess_skill_dirs = Vec::new();

        // If skills list is empty, scan the skills/ subdirectory
        let skill_dirs = if manifest.skills.is_empty() {
            self.find_skill_dirs_in(&dir.join("skills"))?
        } else {
            self.resolve_skill_globs(dir, &manifest.skills)?
        };

        for skill_dir in skill_dirs {
            self.classify_skill_dir(&skill_dir, &mut prompt_skills, &mut subprocess_skill_dirs)?;
        }

        Ok(LoadedPlugin {
            manifest,
            dir: dir.to_path_buf(),
            prompt_skills,
            subprocess_skill_dirs,
        })
    }

    /// Load an implicit single-skill plugin (no plugin.json).
    fn load_implicit_plugin(&self, dir: &Path) -> Result<LoadedPlugin> {
        let dir_name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let manifest = PluginManifest {
            name: dir_name,
            version: String::new(),
            description: String::new(),
            author: String::new(),
            skills: Vec::new(),
            hooks: Vec::new(),
            config: HashMap::new(),
        };

        let mut prompt_skills = Vec::new();
        let mut subprocess_skill_dirs = Vec::new();

        self.classify_skill_dir(dir, &mut prompt_skills, &mut subprocess_skill_dirs)?;

        Ok(LoadedPlugin {
            manifest,
            dir: dir.to_path_buf(),
            prompt_skills,
            subprocess_skill_dirs,
        })
    }

    /// Classify a single skill directory by the files it contains.
    ///
    /// - Has `SKILL.md` -> load as PromptSkill
    /// - Has `skill.toml` -> add dir to subprocess_skill_dirs
    /// - Has both -> hybrid (add to both lists)
    fn classify_skill_dir(
        &self,
        dir: &Path,
        prompt_skills: &mut Vec<PromptSkill>,
        subprocess_skill_dirs: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let has_skill_md = dir.join("SKILL.md").is_file();
        let has_skill_toml = dir.join("skill.toml").is_file();

        if has_skill_md {
            let skill = PromptSkill::load(dir)?;
            prompt_skills.push(skill);
        }

        if has_skill_toml {
            subprocess_skill_dirs.push(dir.to_path_buf());
        }

        if !has_skill_md && !has_skill_toml {
            warn!(
                "skill directory {} has neither SKILL.md nor skill.toml",
                dir.display()
            );
        }

        Ok(())
    }

    /// Find all immediate subdirectories of a given path.
    fn find_skill_dirs_in(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        if !dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut dirs = Vec::new();
        let entries = std::fs::read_dir(dir).map_err(|e| {
            SafeAgentError::Plugin(format!(
                "failed to read skills directory {}: {}",
                dir.display(),
                e
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                SafeAgentError::Plugin(format!("failed to read skill entry: {}", e))
            })?;
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            }
        }

        Ok(dirs)
    }

    /// Resolve skill glob patterns relative to the plugin directory.
    ///
    /// Each pattern in the manifest's `skills` list is treated as a
    /// relative path (possibly with `*` wildcards) under the plugin dir.
    /// Simple glob matching: `*` matches any single directory component.
    fn resolve_skill_globs(&self, plugin_dir: &Path, patterns: &[String]) -> Result<Vec<PathBuf>> {
        let mut dirs = Vec::new();

        for pattern in patterns {
            let full_pattern_path = plugin_dir.join(pattern);

            // If the pattern contains a wildcard, enumerate the parent
            if pattern.contains('*') {
                let parent = full_pattern_path
                    .parent()
                    .unwrap_or(plugin_dir);

                // For a pattern like "skills/*", parent is "skills/" and
                // we match all immediate subdirectories.
                if parent.is_dir() {
                    let entries = std::fs::read_dir(parent).map_err(|e| {
                        SafeAgentError::Plugin(format!(
                            "failed to read directory {} for glob {}: {}",
                            parent.display(),
                            pattern,
                            e
                        ))
                    })?;
                    for entry in entries {
                        let entry = entry.map_err(|e| {
                            SafeAgentError::Plugin(format!(
                                "failed to read entry for glob: {}",
                                e
                            ))
                        })?;
                        let path = entry.path();
                        if path.is_dir() {
                            dirs.push(path);
                        }
                    }
                }
            } else {
                // Literal path
                let resolved = plugin_dir.join(pattern);
                if resolved.is_dir() {
                    dirs.push(resolved);
                } else {
                    warn!(
                        "skill path {} does not exist in plugin {}",
                        pattern,
                        plugin_dir.display()
                    );
                }
            }
        }

        Ok(dirs)
    }

    /// Return all prompt skills across all loaded plugins.
    pub fn all_prompt_skills(&self) -> Vec<&PromptSkill> {
        self.plugins
            .iter()
            .flat_map(|p| p.prompt_skills.iter())
            .collect()
    }

    /// Return all subprocess skill directories across all loaded plugins.
    pub fn all_subprocess_skill_dirs(&self) -> Vec<&Path> {
        self.plugins
            .iter()
            .flat_map(|p| p.subprocess_skill_dirs.iter().map(|d| d.as_path()))
            .collect()
    }

    /// Return the names of all loaded plugins.
    pub fn plugin_names(&self) -> Vec<&str> {
        self.plugins
            .iter()
            .map(|p| p.manifest.name.as_str())
            .collect()
    }

    /// Return the number of loaded plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Return true if no plugins are loaded.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a minimal SKILL.md in a directory.
    fn write_skill_md(dir: &Path, name: &str) {
        let content = format!(
            "---\nname: {}\ndescription: Test skill {}\n---\n\nBody of {}.",
            name, name, name
        );
        std::fs::write(dir.join("SKILL.md"), content).unwrap();
    }

    /// Helper: create a minimal skill.toml in a directory.
    fn write_skill_toml(dir: &Path, name: &str) {
        let content = format!(
            "name = \"{}\"\ndescription = \"Subprocess skill {}\"\n",
            name, name
        );
        std::fs::write(dir.join("skill.toml"), content).unwrap();
    }

    /// Helper: create a minimal plugin.json in a directory.
    fn write_plugin_json(dir: &Path, name: &str, skills: &[&str]) {
        let skills_json: Vec<String> = skills.iter().map(|s| format!("\"{}\"", s)).collect();
        let content = format!(
            r#"{{"name":"{}","version":"1.0.0","description":"Test plugin","skills":[{}]}}"#,
            name,
            skills_json.join(",")
        );
        std::fs::write(dir.join("plugin.json"), content).unwrap();
    }

    #[test]
    fn load_implicit_prompt_skill_plugin() {
        // A directory with just SKILL.md should be loaded as an implicit
        // plugin containing one prompt skill and no subprocess skills.
        let tmp = TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        std::fs::create_dir(&plugins_dir).unwrap();

        let skill_dir = plugins_dir.join("my-prompt-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        write_skill_md(&skill_dir, "my-prompt-skill");

        let mut registry = PluginRegistry::new(vec![]);
        let count = registry.scan_dir(&plugins_dir).unwrap();

        assert_eq!(count, 1);
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let prompt_skills = registry.all_prompt_skills();
        assert_eq!(prompt_skills.len(), 1);
        assert_eq!(prompt_skills[0].name, "my-prompt-skill");

        let subprocess_dirs = registry.all_subprocess_skill_dirs();
        assert!(subprocess_dirs.is_empty());

        let names = registry.plugin_names();
        assert_eq!(names, vec!["my-prompt-skill"]);
    }

    #[test]
    fn load_implicit_subprocess_plugin() {
        // A directory with just skill.toml + main.py should be loaded as
        // an implicit plugin containing one subprocess skill dir.
        let tmp = TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        std::fs::create_dir(&plugins_dir).unwrap();

        let skill_dir = plugins_dir.join("my-subprocess-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        write_skill_toml(&skill_dir, "my-subprocess-skill");
        std::fs::write(skill_dir.join("main.py"), "print('hello')").unwrap();

        let mut registry = PluginRegistry::new(vec![]);
        let count = registry.scan_dir(&plugins_dir).unwrap();

        assert_eq!(count, 1);

        let prompt_skills = registry.all_prompt_skills();
        assert!(prompt_skills.is_empty());

        let subprocess_dirs = registry.all_subprocess_skill_dirs();
        assert_eq!(subprocess_dirs.len(), 1);
        assert_eq!(subprocess_dirs[0], skill_dir);
    }

    #[test]
    fn load_explicit_plugin_with_manifest() {
        // A plugin.json listing skill subdirectories. The skills/
        // subdirectory contains one prompt skill and one subprocess skill.
        let tmp = TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        std::fs::create_dir(&plugins_dir).unwrap();

        let plugin_dir = plugins_dir.join("my-plugin");
        std::fs::create_dir(&plugin_dir).unwrap();

        // Create plugin.json pointing to skills/*
        write_plugin_json(&plugin_dir, "my-plugin", &["skills/*"]);

        // Create skills subdirectory with two skills
        let skills_dir = plugin_dir.join("skills");
        std::fs::create_dir(&skills_dir).unwrap();

        let prompt_dir = skills_dir.join("prompt-one");
        std::fs::create_dir(&prompt_dir).unwrap();
        write_skill_md(&prompt_dir, "prompt-one");

        let subprocess_dir = skills_dir.join("subprocess-one");
        std::fs::create_dir(&subprocess_dir).unwrap();
        write_skill_toml(&subprocess_dir, "subprocess-one");
        std::fs::write(subprocess_dir.join("main.py"), "pass").unwrap();

        let mut registry = PluginRegistry::new(vec![]);
        let count = registry.scan_dir(&plugins_dir).unwrap();

        assert_eq!(count, 1);
        assert_eq!(registry.plugin_names(), vec!["my-plugin"]);

        let prompt_skills = registry.all_prompt_skills();
        assert_eq!(prompt_skills.len(), 1);
        assert_eq!(prompt_skills[0].name, "prompt-one");

        let subprocess_dirs = registry.all_subprocess_skill_dirs();
        assert_eq!(subprocess_dirs.len(), 1);
        assert_eq!(subprocess_dirs[0], subprocess_dir);
    }

    #[test]
    fn disabled_plugins_are_skipped() {
        // Two plugins in a directory; one is disabled by name.
        let tmp = TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        std::fs::create_dir(&plugins_dir).unwrap();

        let enabled_dir = plugins_dir.join("enabled-skill");
        std::fs::create_dir(&enabled_dir).unwrap();
        write_skill_md(&enabled_dir, "enabled-skill");

        let disabled_dir = plugins_dir.join("disabled-skill");
        std::fs::create_dir(&disabled_dir).unwrap();
        write_skill_md(&disabled_dir, "disabled-skill");

        let mut registry =
            PluginRegistry::new(vec!["disabled-skill".to_string()]);
        let count = registry.scan_dir(&plugins_dir).unwrap();

        assert_eq!(count, 1);
        assert_eq!(registry.plugin_names(), vec!["enabled-skill"]);
    }

    #[test]
    fn empty_directory_loads_nothing() {
        let tmp = TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        std::fs::create_dir(&plugins_dir).unwrap();

        let mut registry = PluginRegistry::new(vec![]);
        let count = registry.scan_dir(&plugins_dir).unwrap();

        assert_eq!(count, 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn nonexistent_directory_returns_zero() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("does-not-exist");

        let mut registry = PluginRegistry::new(vec![]);
        let count = registry.scan_dir(&nonexistent).unwrap();

        assert_eq!(count, 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn hybrid_skill_appears_in_both_lists() {
        // A skill directory with both SKILL.md and skill.toml should
        // appear in both prompt_skills and subprocess_skill_dirs.
        let tmp = TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        std::fs::create_dir(&plugins_dir).unwrap();

        let hybrid_dir = plugins_dir.join("hybrid-skill");
        std::fs::create_dir(&hybrid_dir).unwrap();
        write_skill_md(&hybrid_dir, "hybrid-skill");
        write_skill_toml(&hybrid_dir, "hybrid-skill");

        let mut registry = PluginRegistry::new(vec![]);
        let count = registry.scan_dir(&plugins_dir).unwrap();

        assert_eq!(count, 1);

        let prompt_skills = registry.all_prompt_skills();
        assert_eq!(prompt_skills.len(), 1);
        assert_eq!(prompt_skills[0].name, "hybrid-skill");

        let subprocess_dirs = registry.all_subprocess_skill_dirs();
        assert_eq!(subprocess_dirs.len(), 1);
        assert_eq!(subprocess_dirs[0], hybrid_dir);
    }
}
