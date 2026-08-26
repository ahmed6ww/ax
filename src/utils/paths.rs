//! Path Utilities
//!
//! Resolves the directories Claude Code and Codex actually read.
//!
//! These paths are load-bearing: writing to the wrong one produces an install
//! that reports success and is never seen by the agent. Each function below
//! cites the documentation it implements, and `tests/paths.rs` asserts the
//! layout so a refactor cannot silently regress it.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Where an agent's configuration is installed.
///
/// `User` applies across every project; `Project` is committed with the repo
/// and is the scope the team-sync workflow builds on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    User,
    Project,
}

impl Scope {
    pub fn from_global_flag(global: bool) -> Self {
        if global {
            Scope::User
        } else {
            Scope::Project
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Scope::User => "user",
            Scope::Project => "project",
        }
    }
}

/// Directory name reserved by Claude Code for skills synced from claude.ai.
///
/// Claude Code overwrites this directory on sync and skips any skill authored
/// there, so AX must never install into it.
pub const CLAUDE_RESERVED_SKILL_DIR: &str = "synced";

/// The home directory every agent path is resolved against.
///
/// `AX_HOME` overrides it. This exists because `dirs::home_dir()` on Windows
/// reads the profile through the shell API and ignores `HOME`/`USERPROFILE`,
/// which means tests cannot redirect it — without this override an integration
/// test writes into the developer's real configuration.
fn home() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("AX_HOME") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    dirs::home_dir().context("Could not determine the home directory")
}

/// The root of the current project.
///
/// Walks up from the working directory looking for a `.git` entry, matching how
/// both agents resolve project configuration, and falls back to the working
/// directory outside a repository.
pub fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("Could not determine the working directory")?;

    let mut dir = cwd.as_path();
    loop {
        if dir.join(".git").exists() {
            return Ok(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    Ok(cwd)
}

// ---------------------------------------------------------------------------
// Claude Code
//
// https://code.claude.com/docs/en/skills
//   personal: ~/.claude/skills/<name>/SKILL.md
//   project:  .claude/skills/<name>/SKILL.md
//
// `~/.claude` is the location on every platform. The Application Support and
// %APPDATA% directories belong to Claude Desktop, which is a different product.
// ---------------------------------------------------------------------------

/// Claude Code's user configuration directory (`~/.claude`).
///
/// Honours `CLAUDE_CONFIG_DIR` when set, which is how Claude Code itself allows
/// the location to be relocated.
pub fn claude_user_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    Ok(home()?.join(".claude"))
}

/// Claude Code's project configuration directory (`<root>/.claude`).
pub fn claude_project_dir() -> Result<PathBuf> {
    Ok(project_root()?.join(".claude"))
}

/// The Claude Code configuration directory for a scope.
pub fn claude_dir(scope: Scope) -> Result<PathBuf> {
    match scope {
        Scope::User => claude_user_dir(),
        Scope::Project => claude_project_dir(),
    }
}

/// Where Claude Code discovers skills.
pub fn claude_skills_dir(scope: Scope) -> Result<PathBuf> {
    Ok(claude_dir(scope)?.join("skills"))
}

/// Where Claude Code discovers subagents.
pub fn claude_agents_dir(scope: Scope) -> Result<PathBuf> {
    Ok(claude_dir(scope)?.join("agents"))
}

/// Where Claude Code reads MCP server definitions.
///
/// Project scope is `.mcp.json` at the repository root — the file intended to be
/// committed and shared. User scope lives in `~/.claude.json`, which also holds
/// unrelated Claude Code state and must therefore be merged, never replaced.
pub fn claude_mcp_config(scope: Scope) -> Result<PathBuf> {
    match scope {
        Scope::Project => Ok(project_root()?.join(".mcp.json")),
        Scope::User => Ok(home()?.join(".claude.json")),
    }
}

/// Whether Claude Code is present for this user.
pub fn claude_detected() -> bool {
    claude_user_dir().map(|p| p.exists()).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Codex
//
// https://learn.chatgpt.com/docs/build-skills
//   Codex scans `.agents/skills` from the working directory up to the repo
//   root, then `$HOME/.agents/skills`, then `/etc/codex/skills`.
//
// `~/.codex` holds `config.toml` only; it is not a skill location.
// ---------------------------------------------------------------------------

/// The `.agents` directory for a scope.
pub fn codex_agents_root(scope: Scope) -> Result<PathBuf> {
    match scope {
        Scope::User => Ok(home()?.join(".agents")),
        Scope::Project => Ok(project_root()?.join(".agents")),
    }
}

/// Where Codex discovers skills.
pub fn codex_skills_dir(scope: Scope) -> Result<PathBuf> {
    Ok(codex_agents_root(scope)?.join("skills"))
}

/// Codex's configuration directory (`~/.codex`), which is user-scoped only.
pub fn codex_config_dir() -> Result<PathBuf> {
    Ok(home()?.join(".codex"))
}

/// Where Codex reads MCP server definitions (`~/.codex/config.toml`).
pub fn codex_mcp_config() -> Result<PathBuf> {
    Ok(codex_config_dir()?.join("config.toml"))
}

/// Whether Codex is present for this user.
pub fn codex_detected() -> bool {
    let by_config = codex_config_dir().map(|p| p.exists()).unwrap_or(false);
    let by_skills = codex_skills_dir(Scope::User)
        .map(|p| p.exists())
        .unwrap_or(false);
    by_config || by_skills || which::which("codex").is_ok()
}

// ---------------------------------------------------------------------------
// AX itself
// ---------------------------------------------------------------------------

/// The AX configuration directory (`~/.ax`).
pub fn ax_config_dir() -> Result<PathBuf> {
    Ok(home()?.join(".ax"))
}

/// The AX configuration file (`~/.ax/config.toml`).
pub fn ax_config_path() -> Result<PathBuf> {
    Ok(ax_config_dir()?.join("config.toml"))
}

/// Rejects a skill directory name that Claude Code reserves for its own use.
pub fn is_reserved_skill_name(name: &str) -> bool {
    name.eq_ignore_ascii_case(CLAUDE_RESERVED_SKILL_DIR)
}

/// True when `child` stays inside `base` after normalization.
///
/// Guards the join of a registry-supplied name onto an install root.
pub fn is_contained(base: &Path, child: &Path) -> bool {
    let mut depth = 0i32;
    for component in child.components() {
        use std::path::Component;
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            // An absolute path or a Windows prefix escapes `base` outright.
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    let _ = base;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_maps_from_global_flag() {
        assert_eq!(Scope::from_global_flag(true), Scope::User);
        assert_eq!(Scope::from_global_flag(false), Scope::Project);
    }

    #[test]
    fn synced_is_reserved_in_any_capitalization() {
        assert!(is_reserved_skill_name("synced"));
        assert!(is_reserved_skill_name("Synced"));
        assert!(is_reserved_skill_name("SYNCED"));
        assert!(!is_reserved_skill_name("sync"));
    }

    #[test]
    fn containment_rejects_traversal_and_absolute_paths() {
        let base = Path::new("/home/u/.claude/skills");
        assert!(is_contained(base, Path::new("code-cleaner")));
        assert!(is_contained(base, Path::new("a/b/c")));
        assert!(!is_contained(base, Path::new("../../.bashrc")));
        assert!(!is_contained(base, Path::new("a/../../..")));
        assert!(!is_contained(base, Path::new("/etc/passwd")));
    }
}
