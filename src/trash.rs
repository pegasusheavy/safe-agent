//! Trash system — intercepts file/directory deletions and moves them to a
//! recoverable trash directory instead of permanently deleting them.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::error::{Result, SafeAgentError};

/// Metadata for a single trashed item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashEntry {
    /// Unique identifier for this trash entry.
    pub id: String,
    /// Original absolute path of the file or directory.
    pub original_path: String,
    /// The filename or directory name (for display).
    pub name: String,
    /// ISO 8601 timestamp of when the item was trashed.
    pub trashed_at: String,
    /// Size in bytes (files only; 0 for directories).
    pub size_bytes: u64,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// Source of the deletion (e.g., "tool:exec", "tool:file", "rhai:skill-name").
    pub source: String,
}

/// Manages the trash directory and its contents.
#[derive(Debug, Clone)]
pub struct TrashManager {
    /// Where trashed files are stored: $DATA_DIR/trash/files/
    files_dir: PathBuf,
    /// Where metadata JSON files are stored: $DATA_DIR/trash/meta/
    meta_dir: PathBuf,
    /// Where rm/rmdir wrapper scripts live: $DATA_DIR/trash/bin/
    bin_dir: PathBuf,
}

impl TrashManager {
    /// Create a new TrashManager rooted at `data_dir/trash/`.
    pub fn new(data_dir: &Path) -> Result<Self> {
        let root = data_dir.join("trash");
        let files_dir = root.join("files");
        let meta_dir = root.join("meta");
        let bin_dir = root.join("bin");

        std::fs::create_dir_all(&files_dir)?;
        std::fs::create_dir_all(&meta_dir)?;
        std::fs::create_dir_all(&bin_dir)?;

        let mgr = Self {
            files_dir,
            meta_dir,
            bin_dir,
        };

        // Write/refresh the shell wrapper scripts
        mgr.write_shell_wrappers()?;

        Ok(mgr)
    }

    /// Path to the bin directory containing rm/rmdir wrappers.
    /// Prepend this to PATH for sandboxed command execution.
    pub fn bin_dir(&self) -> &Path {
        &self.bin_dir
    }

    /// Move a file or directory to the trash.
    ///
    /// Returns the `TrashEntry` metadata on success.
    pub fn trash(&self, path: &Path, source: &str) -> Result<TrashEntry> {
        if !path.exists() {
            return Err(SafeAgentError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("path does not exist: {}", path.display()),
            )));
        }

        let id = Uuid::new_v4().to_string();
        let is_dir = path.is_dir();
        let size_bytes = if is_dir {
            dir_size(path)
        } else {
            std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
        };

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let original_path = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string();

        let entry = TrashEntry {
            id: id.clone(),
            original_path,
            name,
            trashed_at: Utc::now().to_rfc3339(),
            size_bytes,
            is_dir,
            source: source.to_string(),
        };

        // Move the file/directory to the trash files directory
        let dest = self.files_dir.join(&id);
        if is_dir {
            copy_dir_recursive(path, &dest)?;
            std::fs::remove_dir_all(path)?;
        } else {
            // Try rename (fast, same filesystem) then fallback to copy+delete
            if std::fs::rename(path, &dest).is_err() {
                std::fs::copy(path, &dest)?;
                std::fs::remove_file(path)?;
            }
        }

        // Write metadata
        let meta_path = self.meta_dir.join(format!("{id}.json"));
        let meta_json = serde_json::to_string_pretty(&entry)
            .map_err(|e| SafeAgentError::Config(format!("serialize trash entry: {e}")))?;
        std::fs::write(&meta_path, meta_json)?;

        info!(
            id = %id,
            path = %entry.original_path,
            source = %source,
            size = size_bytes,
            "moved to trash"
        );

        Ok(entry)
    }

    /// List all items in the trash, sorted by most recently trashed first.
    pub fn list(&self) -> Vec<TrashEntry> {
        let mut entries = Vec::new();

        let Ok(dir) = std::fs::read_dir(&self.meta_dir) else {
            return entries;
        };

        for item in dir.flatten() {
            let path = item.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(entry) = serde_json::from_str::<TrashEntry>(&content) {
                    // Only include entries whose files still exist in trash
                    if self.files_dir.join(&entry.id).exists() {
                        entries.push(entry);
                    }
                }
            }
        }

        // Sort by trashed_at descending (newest first)
        entries.sort_by(|a, b| b.trashed_at.cmp(&a.trashed_at));
        entries
    }

    /// Restore a trashed item to its original location.
    pub fn restore(&self, id: &str) -> Result<TrashEntry> {
        let entry = self.get_entry(id)?;
        let src = self.files_dir.join(id);
        let dest = PathBuf::from(&entry.original_path);

        if !src.exists() {
            return Err(SafeAgentError::Config(format!(
                "trash file missing for id: {id}"
            )));
        }

        // Create parent directories if needed
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Don't overwrite existing files
        if dest.exists() {
            return Err(SafeAgentError::Config(format!(
                "cannot restore: destination already exists: {}",
                dest.display()
            )));
        }

        // Move back
        if entry.is_dir {
            copy_dir_recursive(&src, &dest)?;
            std::fs::remove_dir_all(&src)?;
        } else if std::fs::rename(&src, &dest).is_err() {
            std::fs::copy(&src, &dest)?;
            std::fs::remove_file(&src)?;
        }

        // Remove metadata
        let meta_path = self.meta_dir.join(format!("{id}.json"));
        let _ = std::fs::remove_file(&meta_path);

        info!(id = %id, path = %entry.original_path, "restored from trash");

        Ok(entry)
    }

    /// Permanently delete a single trashed item.
    pub fn permanent_delete(&self, id: &str) -> Result<TrashEntry> {
        let entry = self.get_entry(id)?;
        let file_path = self.files_dir.join(id);

        if file_path.exists() {
            if file_path.is_dir() {
                std::fs::remove_dir_all(&file_path)?;
            } else {
                std::fs::remove_file(&file_path)?;
            }
        }

        let meta_path = self.meta_dir.join(format!("{id}.json"));
        let _ = std::fs::remove_file(&meta_path);

        info!(id = %id, path = %entry.original_path, "permanently deleted from trash");

        Ok(entry)
    }

    /// Empty the entire trash.
    pub fn empty(&self) -> Result<usize> {
        let entries = self.list();
        let count = entries.len();

        // Remove all files
        if let Ok(dir) = std::fs::read_dir(&self.files_dir) {
            for item in dir.flatten() {
                let path = item.path();
                if path.is_dir() {
                    let _ = std::fs::remove_dir_all(&path);
                } else {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }

        // Remove all metadata
        if let Ok(dir) = std::fs::read_dir(&self.meta_dir) {
            for item in dir.flatten() {
                let _ = std::fs::remove_file(item.path());
            }
        }

        info!(count, "trash emptied");
        Ok(count)
    }

    /// Get total trash size and count.
    pub fn stats(&self) -> TrashStats {
        let entries = self.list();
        let total_bytes: u64 = entries.iter().map(|e| e.size_bytes).sum();
        TrashStats {
            count: entries.len(),
            total_bytes,
        }
    }

    /// Read a single entry's metadata.
    fn get_entry(&self, id: &str) -> Result<TrashEntry> {
        // Prevent path traversal
        if id.contains('/') || id.contains('\\') || id.contains("..") {
            return Err(SafeAgentError::SandboxViolation(
                "invalid trash ID".into(),
            ));
        }

        let meta_path = self.meta_dir.join(format!("{id}.json"));
        let content = std::fs::read_to_string(&meta_path).map_err(|_| {
            SafeAgentError::Config(format!("trash entry not found: {id}"))
        })?;

        serde_json::from_str(&content)
            .map_err(|e| SafeAgentError::Config(format!("corrupt trash entry: {e}")))
    }

    /// Write the shell wrapper scripts for rm and rmdir.
    fn write_shell_wrappers(&self) -> Result<()> {
        let trash_files_dir = self.files_dir.to_string_lossy();
        let trash_meta_dir = self.meta_dir.to_string_lossy();

        // The wrapper stores deleted files in the trash and writes metadata JSON.
        // It falls back to real deletion for special paths (e.g., /tmp, /dev).
        let rm_wrapper = format!(
            r#"#!/bin/sh
# safeclaw rm wrapper — moves files to trash instead of deleting
TRASH_FILES="{trash_files_dir}"
TRASH_META="{trash_meta_dir}"

# Parse flags: support -r/-R/-f/-rf/-fr and ignore them for move logic
RECURSIVE=0
FORCE=0
FILES=""
for arg in "$@"; do
    case "$arg" in
        -r|-R|--recursive) RECURSIVE=1 ;;
        -f|--force) FORCE=1 ;;
        -rf|-fr|-fR|-Rf) RECURSIVE=1; FORCE=1 ;;
        -d|--dir) ;; # ignored, we handle dirs
        --) ;; # end of flags
        -*) ;; # ignore other flags
        *) FILES="$FILES $arg" ;;
    esac
done

if [ -z "$FILES" ]; then
    exit 0
fi

for FILE in $FILES; do
    # Skip if doesn't exist
    if [ ! -e "$FILE" ] && [ ! -L "$FILE" ]; then
        if [ "$FORCE" = "0" ]; then
            echo "rm: cannot remove '$FILE': No such file or directory" >&2
        fi
        continue
    fi

    # Resolve to absolute path
    ABS="$(cd "$(dirname "$FILE")" 2>/dev/null && pwd)/$(basename "$FILE")"

    # Skip special paths — use real rm
    case "$ABS" in
        /tmp/*|/dev/*|/proc/*|/sys/*)
            /bin/rm "$@" 2>/dev/null || command rm "$@"
            exit $?
            ;;
    esac

    # Generate unique ID
    ID="$(date +%s)-$$-$(od -An -N4 -tx4 /dev/urandom 2>/dev/null | tr -d ' ' || echo $$)"
    DEST="$TRASH_FILES/$ID"

    # Move to trash
    mv "$FILE" "$DEST" 2>/dev/null
    if [ $? -ne 0 ]; then
        cp -a "$FILE" "$DEST" 2>/dev/null && rm -rf "$FILE" 2>/dev/null
        if [ $? -ne 0 ]; then
            echo "rm: failed to trash '$FILE'" >&2
            continue
        fi
    fi

    # Determine size and type
    IS_DIR="false"
    SIZE=0
    if [ -d "$DEST" ]; then
        IS_DIR="true"
        SIZE=$(du -sb "$DEST" 2>/dev/null | cut -f1 || echo 0)
    else
        SIZE=$(stat -c%s "$DEST" 2>/dev/null || stat -f%z "$DEST" 2>/dev/null || echo 0)
    fi

    NAME="$(basename "$FILE")"
    NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    # Write metadata
    cat > "$TRASH_META/$ID.json" <<METAEOF
{{
  "id": "$ID",
  "original_path": "$ABS",
  "name": "$NAME",
  "trashed_at": "$NOW",
  "size_bytes": $SIZE,
  "is_dir": $IS_DIR,
  "source": "shell:rm"
}}
METAEOF
done
"#
        );

        let rmdir_wrapper = format!(
            r#"#!/bin/sh
# safeclaw rmdir wrapper — moves directories to trash instead of deleting
TRASH_FILES="{trash_files_dir}"
TRASH_META="{trash_meta_dir}"

for DIR in "$@"; do
    case "$DIR" in
        -*) continue ;; # skip flags
    esac

    if [ ! -d "$DIR" ]; then
        echo "rmdir: failed to remove '$DIR': Not a directory" >&2
        continue
    fi

    ABS="$(cd "$(dirname "$DIR")" 2>/dev/null && pwd)/$(basename "$DIR")"

    ID="$(date +%s)-$$-$(od -An -N4 -tx4 /dev/urandom 2>/dev/null | tr -d ' ' || echo $$)"
    DEST="$TRASH_FILES/$ID"

    mv "$DIR" "$DEST" 2>/dev/null
    if [ $? -ne 0 ]; then
        cp -a "$DIR" "$DEST" 2>/dev/null && rm -rf "$DIR" 2>/dev/null
    fi

    SIZE=$(du -sb "$DEST" 2>/dev/null | cut -f1 || echo 0)
    NAME="$(basename "$DIR")"
    NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    cat > "$TRASH_META/$ID.json" <<METAEOF
{{
  "id": "$ID",
  "original_path": "$ABS",
  "name": "$NAME",
  "trashed_at": "$NOW",
  "size_bytes": $SIZE,
  "is_dir": true,
  "source": "shell:rmdir"
}}
METAEOF
done
"#
        );

        let rm_path = self.bin_dir.join("rm");
        let rmdir_path = self.bin_dir.join("rmdir");

        std::fs::write(&rm_path, rm_wrapper)?;
        std::fs::write(&rmdir_path, rmdir_wrapper)?;

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&rm_path, std::fs::Permissions::from_mode(0o755))?;
            std::fs::set_permissions(&rmdir_path, std::fs::Permissions::from_mode(0o755))?;
        }

        Ok(())
    }
}

/// Summary statistics for the trash.
#[derive(Debug, Serialize)]
pub struct TrashStats {
    pub count: usize,
    pub total_bytes: u64,
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }

    Ok(())
}

/// Calculate total size of a directory recursively.
fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = p.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_trash_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("sa-trash-test-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn test_trash_manager_new_creates_directories() {
        let base = temp_trash_root();
        std::fs::create_dir_all(&base).unwrap();

        let mgr = TrashManager::new(&base).unwrap();
        let trash_root = base.join("trash");
        assert!(trash_root.join("files").exists());
        assert!(trash_root.join("meta").exists());
        assert!(trash_root.join("bin").exists());
        assert!(mgr.bin_dir().join("rm").exists());
        assert!(mgr.bin_dir().join("rmdir").exists());

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_trash_file_moves_and_creates_metadata() {
        let base = temp_trash_root();
        std::fs::create_dir_all(&base).unwrap();
        let mgr = TrashManager::new(&base).unwrap();

        let test_file = base.join("data").join("test.txt");
        std::fs::create_dir_all(test_file.parent().unwrap()).unwrap();
        std::fs::write(&test_file, b"hello world").unwrap();

        let entry = mgr.trash(&test_file, "tool:file").unwrap();
        assert_eq!(entry.name, "test.txt");
        assert_eq!(entry.size_bytes, 11);
        assert!(!entry.is_dir);
        assert_eq!(entry.source, "tool:file");
        assert!(!test_file.exists());
        let trash_root = base.join("trash");
        assert!(trash_root.join("files").join(&entry.id).exists());
        assert!(trash_root.join("meta").join(format!("{}.json", entry.id)).exists());

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_trash_dir_moves_directory() {
        let base = temp_trash_root();
        std::fs::create_dir_all(&base).unwrap();
        let mgr = TrashManager::new(&base).unwrap();

        let test_dir = base.join("data").join("mydir");
        std::fs::create_dir_all(&test_dir).unwrap();
        std::fs::write(test_dir.join("a.txt"), b"a").unwrap();
        std::fs::write(test_dir.join("b.txt"), b"bb").unwrap();

        let entry = mgr.trash(&test_dir, "tool:exec").unwrap();
        assert_eq!(entry.name, "mydir");
        assert!(entry.is_dir);
        assert_eq!(entry.size_bytes, 3); // 1 + 2
        assert!(!test_dir.exists());
        assert!(base.join("trash").join("files").join(&entry.id).exists());

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_trash_list_returns_items() {
        let base = temp_trash_root();
        std::fs::create_dir_all(&base).unwrap();
        let mgr = TrashManager::new(&base).unwrap();

        let f1 = base.join("f1.txt");
        std::fs::write(&f1, b"x").unwrap();
        mgr.trash(&f1, "test").unwrap();

        let list = mgr.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "f1.txt");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_trash_restore_puts_file_back() {
        let base = temp_trash_root();
        std::fs::create_dir_all(&base).unwrap();
        let mgr = TrashManager::new(&base).unwrap();

        let original = base.join("restore_me.txt");
        std::fs::write(&original, b"restored content").unwrap();
        let entry = mgr.trash(&original, "test").unwrap();

        assert!(!original.exists());
        let restored = mgr.restore(&entry.id).unwrap();
        assert_eq!(restored.original_path, original.to_string_lossy());
        assert!(original.exists());
        assert_eq!(std::fs::read_to_string(&original).unwrap(), "restored content");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_trash_permanent_delete_removes_from_trash() {
        let base = temp_trash_root();
        std::fs::create_dir_all(&base).unwrap();
        let mgr = TrashManager::new(&base).unwrap();

        let f = base.join("delete_me.txt");
        std::fs::write(&f, b"x").unwrap();
        let entry = mgr.trash(&f, "test").unwrap();

        assert_eq!(mgr.list().len(), 1);
        mgr.permanent_delete(&entry.id).unwrap();
        assert_eq!(mgr.list().len(), 0);
        assert!(!base.join("trash").join("files").join(&entry.id).exists());

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_trash_empty_removes_all() {
        let base = temp_trash_root();
        std::fs::create_dir_all(&base).unwrap();
        let mgr = TrashManager::new(&base).unwrap();

        std::fs::write(base.join("a.txt"), b"a").unwrap();
        std::fs::write(base.join("b.txt"), b"b").unwrap();
        mgr.trash(&base.join("a.txt"), "test").unwrap();
        mgr.trash(&base.join("b.txt"), "test").unwrap();

        assert_eq!(mgr.list().len(), 2);
        let count = mgr.empty().unwrap();
        assert_eq!(count, 2);
        assert_eq!(mgr.list().len(), 0);

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_trash_stats_returns_counts_and_sizes() {
        let base = temp_trash_root();
        std::fs::create_dir_all(&base).unwrap();
        let mgr = TrashManager::new(&base).unwrap();

        let f1 = base.join("f1.txt");
        let f2 = base.join("f2.txt");
        std::fs::write(&f1, b"12345").unwrap();
        std::fs::write(&f2, b"1234567890").unwrap();
        mgr.trash(&f1, "test").unwrap();
        mgr.trash(&f2, "test").unwrap();

        let stats = mgr.stats();
        assert_eq!(stats.count, 2);
        assert_eq!(stats.total_bytes, 15);

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_trash_nonexistent_file_returns_error() {
        let base = temp_trash_root();
        std::fs::create_dir_all(&base).unwrap();
        let mgr = TrashManager::new(&base).unwrap();

        let nonexistent = base.join("does_not_exist.txt");
        let result = mgr.trash(&nonexistent, "test");
        assert!(result.is_err());

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_trash_restore_invalid_id_returns_error() {
        let base = temp_trash_root();
        std::fs::create_dir_all(&base).unwrap();
        let mgr = TrashManager::new(&base).unwrap();

        let result = mgr.restore("nonexistent-id-12345");
        assert!(result.is_err());

        std::fs::remove_dir_all(&base).ok();
    }
}
