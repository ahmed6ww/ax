//! Shared filesystem primitives for both installers.
//!
//! Claude Code and Codex consume the same Agent Skills layout, so the only
//! difference between them is the destination root. Path safety and durable
//! writes live here rather than being duplicated per target.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::paths;

/// Join `relative` onto `base`, refusing anything that escapes it.
///
/// Every path component that originates from a fetched manifest passes through
/// here, so a `../..` in a skill name cannot reach outside the install root.
pub fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    if relative.trim().is_empty() {
        anyhow::bail!("Empty path component");
    }

    let path = Path::new(relative);
    if !paths::is_contained(base, path) {
        anyhow::bail!("'{}' would write outside {}", relative, base.display());
    }

    Ok(base.join(path))
}

/// Resolve the directory a skill installs into, refusing any name that would
/// escape `skills_root` or collide with a reserved directory.
pub fn skill_dir(skills_root: &Path, skill_name: &str) -> Result<PathBuf> {
    if paths::is_reserved_skill_name(skill_name) {
        anyhow::bail!(
            "Skill name '{}' is reserved by Claude Code for skills synced from claude.ai",
            skill_name
        );
    }

    safe_join(skills_root, skill_name)
}

/// Write `contents` to `path` atomically, replacing it outright.
///
/// For files agentpm owns: skill files it authored and will author again.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    write_inner(path, contents, false)
}

/// Write atomically, first keeping one `.agentpm-bak` generation.
///
/// For files agentpm shares with the user — an editor's MCP configuration —
/// where a rewrite is lossy and must stay recoverable.
pub fn write_atomic_preserving(path: &Path, contents: &[u8]) -> Result<()> {
    write_inner(path, contents, true)
}

/// Write a sibling temp file, flush it, then rename over the target so an
/// interrupted run cannot leave a truncated file where the agent expects valid
/// configuration.
fn write_inner(path: &Path, contents: &[u8], backup: bool) -> Result<()> {
    use std::io::Write;

    let parent = path
        .parent()
        .context("Refusing to write to a path with no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("Failed to create {}", parent.display()))?;

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("staged");

    if backup {
        // Keep one generation of whatever was already here. Only for files
        // agentpm does not own: rewriting a user's `.mcp.json` or Codex
        // `config.toml` through a parser drops comments and reorders keys, so
        // the previous contents have to stay recoverable.
        //
        // Files agentpm owns outright — everything under a skill directory —
        // are never backed up. The agents scan those directories, and for a
        // project-scoped install the stray files would be committed.
        if path.exists() {
            let backup_path = parent.join(format!("{}.agentpm-bak", name));
            fs::copy(path, &backup_path).with_context(|| {
                format!("Failed to back up {} before rewriting it", path.display())
            })?;
        }
    }

    let tmp = parent.join(format!(".{}.agentpm-tmp", name));

    {
        let mut file = fs::File::create(&tmp)
            .with_context(|| format!("Failed to create {}", tmp.display()))?;
        file.write_all(contents)?;
        file.sync_all()?;
    }

    // Windows rejects a rename onto an existing file.
    #[cfg(target_os = "windows")]
    if path.exists() {
        fs::remove_file(path).ok();
    }

    fs::rename(&tmp, path)
        .with_context(|| format!("Failed to move staged file into {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_dir_rejects_traversal() {
        let root = Path::new("/home/u/.claude/skills");
        assert!(skill_dir(root, "code-cleaner").is_ok());
        assert!(skill_dir(root, "../../.bashrc").is_err());
        assert!(skill_dir(root, "/etc/passwd").is_err());
        assert!(skill_dir(root, "").is_err());
    }

    #[test]
    fn skill_dir_rejects_the_reserved_synced_directory() {
        let root = Path::new("/home/u/.claude/skills");
        assert!(skill_dir(root, "synced").is_err());
        assert!(skill_dir(root, "SYNCED").is_err());
    }

    #[test]
    fn write_atomic_leaves_no_backup_for_files_agentpm_owns() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("SKILL.md");
        write_atomic(&target, b"first").unwrap();
        write_atomic(&target, b"second").unwrap();

        // A skill directory is scanned by the agent and committed with the
        // repo, so a stray .agentpm-bak must never appear in it.
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["SKILL.md".to_string()]);
    }

    #[test]
    fn preserving_write_keeps_one_generation() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".mcp.json");
        write_atomic_preserving(&target, b"original").unwrap();
        write_atomic_preserving(&target, b"rewritten").unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "rewritten");
        let backup = dir.path().join(".mcp.json.agentpm-bak");
        assert_eq!(fs::read_to_string(&backup).unwrap(), "original");
    }

    #[test]
    fn write_atomic_replaces_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nested").join("SKILL.md");
        write_atomic(&target, b"first").unwrap();
        write_atomic(&target, b"second").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "second");
        // No staging files left behind.
        let leftovers: Vec<_> = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("agentpm-tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
