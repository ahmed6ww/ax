//! Installer Module
//!
//! Adapter per target editor. agentpm targets Claude Code and Codex only: both speak
//! the Agent Skills standard, which lets them share one skill renderer and
//! differ only in where files land and which surfaces they support.

mod claude;
mod codex;
pub mod common;

use anyhow::Result;

pub use claude::ClaudeInstaller;
pub use codex::CodexInstaller;

use crate::core::agent::AgentConfig;
use crate::utils::paths::Scope;

/// Target editor for installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

/// Installer trait — the adapter for a target editor.
pub trait Installer: Send + Sync {
    /// What this target supports.
    fn capabilities(&self) -> Capabilities;

    /// Install the agent's identity (system prompt) as a subagent.
    ///
    /// Only called when `capabilities().subagents` is true.
    fn install_identity(&self, agent: &AgentConfig) -> Result<()>;

    /// Install the agent's skills.
    fn install_skills(&self, agent: &AgentConfig) -> Result<()>;

    /// Install the agent's MCP servers.
    ///
    /// Only called when `capabilities().mcp` is true.
    fn install_tools(&self, agent: &AgentConfig) -> Result<()>;

    /// Remove an agent by name.
    fn uninstall(&self, agent_name: &str) -> Result<()>;

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
