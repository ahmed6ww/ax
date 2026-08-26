//! Shared skill rendering and materialization.
//!
//! Claude Code and Codex both consume the Agent Skills standard, so the
//! `SKILL.md` they receive is byte-identical and only the destination root
//! differs. Everything that used to be duplicated across the two installers
//! lives here.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::agent::Skill;
use crate::utils::paths;

/// Render a skill to the Agent Skills `SKILL.md` format.
///
/// `name` and `description` are the two fields the standard requires; the rest
/// are emitted only when present. Values are serialized through `serde_yaml` so
/// a colon, quote or newline in a registry-supplied field cannot break out of
/// the frontmatter.
pub fn render_skill_md(skill: &Skill, fallback_description: &str) -> Result<String> {
    use serde_yaml::{Mapping, Value};

    let mut fm = Mapping::new();
    fm.insert(Value::from("name"), Value::from(skill.name.as_str()));

    let description = skill
        .description
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or(fallback_description);
    fm.insert(Value::from("description"), Value::from(description));

    let mut put = |key: &str, value: &Option<String>| {
        if let Some(v) = value {
            fm.insert(Value::from(key), Value::from(v.as_str()));
        }
    };
    put("license", &skill.license);
    put("compatibility", &skill.compatibility);
    put("allowed-tools", &skill.allowed_tools);
    put("dependencies", &skill.dependencies);

    if let Some(metadata) = &skill.metadata {
        if !metadata.is_empty() {
            let mut meta = Mapping::new();
            // BTreeMap ordering keeps the rendered file stable across runs, so a
            // reinstall produces no spurious diff.
            let mut keys: Vec<_> = metadata.keys().collect();
            keys.sort();
            for key in keys {
                meta.insert(
                    Value::from(key.as_str()),
                    Value::from(metadata[key].as_str()),
                );
            }
            fm.insert(Value::from("metadata"), Value::Mapping(meta));
        }
    }

    let frontmatter = serde_yaml::to_string(&Value::Mapping(fm))
        .context("Failed to serialize skill frontmatter")?;

    let body = skill.content.trim_end();
    Ok(format!("---\n{}---\n\n{}\n", frontmatter, body))
}

/// Resolve the directory a skill installs into, refusing any name that would
/// escape `skills_root` or collide with a reserved directory.
pub fn skill_dir(skills_root: &Path, skill_name: &str) -> Result<PathBuf> {
    if skill_name.trim().is_empty() {
        anyhow::bail!("Skill name is empty");
    }

    if paths::is_reserved_skill_name(skill_name) {
        anyhow::bail!(
            "Skill name '{}' is reserved by Claude Code for skills synced from claude.ai",
            skill_name
        );
    }

    let relative = Path::new(skill_name);
    if !paths::is_contained(skills_root, relative) {
        anyhow::bail!(
            "Skill name '{}' would install outside {}",
            skill_name,
            skills_root.display()
        );
    }

    Ok(skills_root.join(relative))
}

/// Write `contents` to `path` atomically: write a sibling temp file, flush it,
/// then rename over the target so an interrupted run cannot leave a truncated
/// file where the agent expects valid configuration.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;

    let parent = path
        .parent()
        .context("Refusing to write to a path with no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create {}", parent.display()))?;

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("staged");

    // Keep one generation of whatever was already here. AX rewrites files it
    // does not own — a user's `.mcp.json` or Codex `config.toml` — and
    // serializing through a parser drops comments and reorders keys, so the
    // previous contents have to remain recoverable.
    if path.exists() {
        let backup = parent.join(format!("{}.ax-bak", name));
        fs::copy(path, &backup).with_context(|| {
            format!("Failed to back up {} before rewriting it", path.display())
        })?;
    }

    let tmp = parent.join(format!(".{}.ax-tmp", name));

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

    fs::rename(&tmp, path).with_context(|| {
        format!("Failed to move staged file into {}", path.display())
    })?;

    Ok(())
}

/// Copy `scripts/`, `references/` and `assets/` from a local skill directory.
pub fn copy_skill_subdirectories(source_dir: &Path, dest_dir: &Path) -> Result<()> {
    for subdir in ["scripts", "references", "assets"] {
        let source = source_dir.join(subdir);
        if source.is_dir() {
            copy_dir_recursive(&source, &dest_dir.join(subdir))?;
        }
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn skill(name: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: Some("Does a thing".to_string()),
            content: "# Body\n\nInstructions.".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn renders_required_frontmatter_fields() {
        let md = render_skill_md(&skill("code-cleaner"), "fallback").unwrap();
        assert!(md.starts_with("---\n"));
        assert!(md.contains("name: code-cleaner"));
        assert!(md.contains("description: Does a thing"));
        assert!(md.trim_end().ends_with("Instructions."));
    }

    #[test]
    fn falls_back_to_the_agent_description() {
        let mut s = skill("x");
        s.description = None;
        let md = render_skill_md(&s, "agent-level description").unwrap();
        assert!(md.contains("description: agent-level description"));
    }

    #[test]
    fn frontmatter_survives_a_description_containing_yaml_syntax() {
        let mut s = skill("x");
        s.description = Some("Reads: a file\nthen: writes # one".to_string());
        let md = render_skill_md(&s, "fallback").unwrap();

        // The body must still begin after exactly one closing delimiter.
        let after = md.strip_prefix("---\n").unwrap();
        let end = after.find("\n---\n").expect("closing delimiter");
        let frontmatter = &after[..end];
        let parsed: serde_yaml::Value = serde_yaml::from_str(frontmatter).unwrap();
        assert_eq!(
            parsed["description"].as_str(),
            Some("Reads: a file\nthen: writes # one")
        );
    }

    #[test]
    fn metadata_ordering_is_stable() {
        let mut s = skill("x");
        let mut m = HashMap::new();
        m.insert("zeta".to_string(), "1".to_string());
        m.insert("alpha".to_string(), "2".to_string());
        s.metadata = Some(m);
        let a = render_skill_md(&s, "f").unwrap();
        let b = render_skill_md(&s, "f").unwrap();
        assert_eq!(a, b);
        assert!(a.find("alpha").unwrap() < a.find("zeta").unwrap());
    }

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
            .filter(|e| e.file_name().to_string_lossy().contains("ax-tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
