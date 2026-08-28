//! Installer Module
//!
//! Adapter per target editor. axur targets Claude Code and Codex only: both speak
//! the Agent Skills standard, which lets them share one skill renderer and
//! differ only in where files land and which surfaces they support.

mod claude;
mod codex;
pub mod common;

use anyhow::Result;
use std::path::PathBuf;

pub use claude::ClaudeInstaller;
pub use codex::CodexInstaller;

use crate::core::bundle::{Hook, Permissions};
use crate::utils::paths::Scope;

/// What axur has written into a target's settings file.
///
/// Recorded in the lockfile so a later sync removes exactly these entries
/// rather than merging blindly and growing the file, and so uninstall can undo
/// them without touching anything the user added by hand.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsContribution {
    pub permissions: Permissions,
    /// `(hook, resolved command path)` pairs.
    pub hooks: Vec<(Hook, String)>,
}

impl SettingsContribution {
    pub fn is_empty(&self) -> bool {
        self.permissions.is_empty() && self.hooks.is_empty()
    }
}

/// Target editor for installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    Claude,
    Codex,
}

impl Target {
    pub fn display_name(&self) -> &'static str {
        match self {
            Target::Claude => "Claude Code",
            Target::Codex => "Codex",
        }
    }

    pub fn slug(&self) -> &'static str {
        match self {
            Target::Claude => "claude-code",
            Target::Codex => "codex",
        }
    }

    pub fn all() -> [Target; 2] {
        [Target::Claude, Target::Codex]
    }

    /// Parse a manifest/CLI agent slug. Accepts `claude`, `claude-code`'s
    /// short spelling too, since both read naturally in `--agents`.
    pub fn from_slug(slug: &str) -> Option<Target> {
        match slug {
            "claude-code" | "claude" => Some(Target::Claude),
            "codex" => Some(Target::Codex),
            _ => None,
        }
    }

    /// Whether this agent looks installed on the current machine.
    pub fn is_detected(&self) -> bool {
        match self {
            Target::Claude => crate::utils::paths::claude_detected(),
            Target::Codex => crate::utils::paths::codex_detected(),
        }
    }
}

/// What a target can actually be given.
///
/// Replaces the previous pattern of implementing every step on every target and
/// returning `Ok(())` from the ones that do nothing, which reported success for
/// work that never happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Dedicated subagent files with their own system prompt.
    pub subagents: bool,
    /// Agent Skills directories.
    pub skills: bool,
    /// MCP server configuration.
    pub mcp: bool,
    /// Installation into a project directory as well as the user's home.
    pub project_scope: bool,
    /// Slash commands.
    pub commands: bool,
    /// Lifecycle hooks.
    pub hooks: bool,
    /// Tool permission rules.
    pub permissions: bool,
}

impl Capabilities {
    /// Parts of a bundle this target cannot take, for honest reporting.
    pub fn unsupported(&self, bundle: &crate::core::bundle::BundleManifest) -> Vec<String> {
        let mut skipped = Vec::new();
        let mut note = |supported: bool, n: usize, label: &str| {
            if !supported && n > 0 {
                skipped.push(format!("{} {}{}", n, label, if n == 1 { "" } else { "s" }));
            }
        };
        note(self.subagents, bundle.agents.len(), "subagent");
        note(self.commands, bundle.commands.len(), "command");
        note(self.hooks, bundle.hooks.len(), "hook");
        note(self.mcp, bundle.mcp.len(), "MCP server");
        note(
            self.permissions,
            bundle.permissions.allow.len()
                + bundle.permissions.deny.len()
                + bundle.permissions.ask.len(),
            "permission rule",
        );
        skipped
    }
}

/// Installer trait — the adapter for a target editor.
pub trait Installer: Send + Sync {
    /// What this target supports.
    fn capabilities(&self) -> Capabilities;

    /// Where this target discovers skills, for the configured scope.
    fn skills_root(&self) -> Result<PathBuf>;

    /// Install already-fetched files verbatim under `skill_name`.
    ///
    /// `axur sync` uses this rather than re-rendering: the bytes on disk then
    /// match the digests recorded in the lockfile, so drift is detectable
    /// without refetching, and the author's own frontmatter is preserved.
    fn install_files(&self, skill_name: &str, files: &[(String, Vec<u8>)]) -> Result<PathBuf> {
        let root = self.skills_root()?;
        let dir = common::skill_dir(&root, skill_name)?;

        for (relative, bytes) in files {
            let target = common::safe_join(&dir, relative)?;
            common::write_atomic(&target, bytes)?;
        }

        Ok(dir)
    }

    /// Configure MCP servers.
    fn install_mcp(&self, tools: &[crate::core::agent::McpTool]) -> Result<()>;

    /// Install a subagent file verbatim.
    ///
    /// Only called when `capabilities().subagents` is true.
    fn install_subagent(&self, _name: &str, _contents: &[u8]) -> Result<()> {
        Ok(())
    }

    /// Install a slash command file verbatim.
    ///
    /// Only called when `capabilities().commands` is true.
    fn install_command(&self, _name: &str, _contents: &[u8]) -> Result<()> {
        Ok(())
    }

    /// Stage a bundle's own files (hook scripts) and return the directory.
    fn stage_bundle_files(
        &self,
        _bundle: &str,
        _files: &[(String, Vec<u8>)],
    ) -> Result<Option<PathBuf>> {
        Ok(None)
    }

    /// Merge hooks and permissions into the target's settings.
    ///
    /// `previous` names what a prior sync contributed, so those entries are
    /// removed before the current set is applied and the file does not grow on
    /// every run. Only called when `capabilities().hooks` or
    /// `capabilities().permissions` is true.
    fn apply_settings(
        &self,
        _contribution: &SettingsContribution,
        _previous: &SettingsContribution,
    ) -> Result<()> {
        Ok(())
    }

    /// Remove an agent by name.
    fn uninstall(&self, agent_name: &str) -> Result<()>;

    /// Remove a single installed skill directory.
    ///
    /// Used to prune what a manifest no longer declares; without it, deleting
    /// an entry from the manifest left the files on disk and the agent kept
    /// loading a skill the project had dropped.
    fn remove_skill(&self, name: &str) -> Result<bool> {
        let dir = common::skill_dir(&self.skills_root()?, name)?;
        if dir.exists() {
            crate::core::tx::remove_dir_all(&dir)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Human-readable description of where this installer writes.
    fn location(&self) -> String;
}

/// Get the installer for a target and scope.
pub fn get_installer(target: Target, scope: Scope) -> Box<dyn Installer> {
    match target {
        Target::Claude => Box::new(ClaudeInstaller::new(scope)),
        Target::Codex => Box::new(CodexInstaller::new(scope)),
    }
}
