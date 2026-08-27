//! Bundles — a whole agent environment as one versioned unit.
//!
//! A skill is a file. A bundle is everything a team needs to work on a
//! codebase: skills, subagents, slash commands, MCP servers, hooks, and tool
//! permissions, pinned together and installed by one command.
//!
//! This is the payoff of targeting Claude Code and Codex specifically. A tool
//! covering 76 agents can only install the lowest common denominator, because
//! 74 of them have no concept of a hook or a subagent. Everything below is
//! capability-gated: Claude Code takes all of it, Codex takes the parts it
//! understands and the rest is reported as skipped rather than silently
//! dropped.
//!
//! A bundle is a directory in a repository containing `BUNDLE.toml`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const BUNDLE_FILE: &str = "BUNDLE.toml";

/// Where a bundle's own files are staged, relative to the target's config dir.
///
/// Hook scripts need a stable on-disk home that `settings.json` can reference,
/// and keeping them under one directory makes removal exact.
pub const BUNDLE_STAGE_DIR: &str = "axur";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    pub name: String,

    #[serde(default)]
    pub description: Option<String>,

    /// Directories, relative to the bundle root, each holding a `SKILL.md`.
    #[serde(default)]
    pub skills: Vec<String>,

    /// Claude Code subagent files, relative to the bundle root.
    #[serde(default)]
    pub agents: Vec<String>,

    /// Slash command files, relative to the bundle root.
    #[serde(default)]
    pub commands: Vec<String>,

    /// Scripts the hooks below invoke, relative to the bundle root.
    #[serde(default)]
    pub scripts: Vec<String>,

    #[serde(default)]
    pub mcp: BTreeMap<String, super::manifest::McpSpec>,

    #[serde(default)]
    pub permissions: Permissions,

    #[serde(default)]
    pub hooks: Vec<Hook>,
}

/// Tool permission rules, matching the `permissions` key in `settings.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ask: Vec<String>,
}

impl Permissions {
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty() && self.ask.is_empty()
    }
}

/// A lifecycle hook.
///
/// Flat here because a bundle author should not have to hand-write the nested
/// `settings.json` shape; [`Hook::to_settings_entry`] expands it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    /// Event name, e.g. `PreToolUse`. Validated against the documented set.
    pub event: String,

    /// Tool pattern this fires for, e.g. `Bash|Write`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,

    /// Script to run, relative to the bundle root.
    pub command: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
}

/// Hook events Claude Code documents.
///
/// A typo in an event name would otherwise install a hook that never fires and
/// reports no error, which is the failure mode this project keeps hitting.
pub const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "Setup",
    "UserPromptSubmit",
    "Stop",
    "StopFailure",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "PermissionDenied",
    "PostToolBatch",
    "SubagentStart",
    "SubagentStop",
    "TaskCreated",
    "TaskCompleted",
    "PreCompact",
    "PostCompact",
    "ConfigChange",
    "InstructionsLoaded",
    "CwdChanged",
    "DirectoryAdded",
    "FileChanged",
    "WorktreeCreate",
    "WorktreeRemove",
    "UserPromptExpansion",
    "Notification",
    "MessageDisplay",
    "TeammateIdle",
    "Elicitation",
    "ElicitationResult",
];

impl Hook {
    /// Stable identity, so a re-sync can remove exactly what it added before.
    pub fn key(&self) -> String {
        format!(
            "{}\u{0}{}\u{0}{}",
            self.event,
            self.matcher.as_deref().unwrap_or(""),
            self.command
        )
    }

    /// Expand to the `{ type, command, timeout }` object `settings.json` wants.
    ///
    /// `resolved_command` is the installed path of the script.
    pub fn to_settings_entry(&self, resolved_command: &str) -> serde_json::Value {
        use serde_json::json;
        let mut entry = json!({
            "type": "command",
            "command": resolved_command,
        });
        if let Some(timeout) = self.timeout {
            entry["timeout"] = json!(timeout);
        }
        if let Some(message) = &self.status_message {
            entry["statusMessage"] = json!(message);
        }
        entry
    }
}

impl BundleManifest {
    pub fn parse(contents: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(contents).context("Failed to parse BUNDLE.toml")?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            anyhow::bail!("BUNDLE.toml has no name");
        }

        for hook in &self.hooks {
            if !HOOK_EVENTS.contains(&hook.event.as_str()) {
                anyhow::bail!(
                    "Unknown hook event '{}' in bundle '{}'. A hook on an \
                     unrecognized event would install and never fire.",
                    hook.event,
                    self.name
                );
            }
            if !self.scripts.iter().any(|s| s == &hook.command) {
                anyhow::bail!(
                    "Hook in bundle '{}' runs '{}', which is not listed under \
                     `scripts`. Add it so the file is installed.",
                    self.name,
                    hook.command
                );
            }
        }

        for path in self
            .skills
            .iter()
            .chain(&self.agents)
            .chain(&self.commands)
            .chain(&self.scripts)
        {
            if path.starts_with('/') || path.contains("..") {
                anyhow::bail!(
                    "Path '{}' in bundle '{}' must stay inside the bundle",
                    path,
                    self.name
                );
            }
        }

        Ok(())
    }

    /// Everything this bundle contributes, for reporting before install.
    pub fn summary(&self) -> Vec<(&'static str, usize)> {
        let mut parts = Vec::new();
        let mut push = |label, n: usize| {
            if n > 0 {
                parts.push((label, n));
            }
        };
        push("skill", self.skills.len());
        push("subagent", self.agents.len());
        push("command", self.commands.len());
        push("MCP server", self.mcp.len());
        push("hook", self.hooks.len());
        push(
            "permission rule",
            self.permissions.allow.len() + self.permissions.deny.len() + self.permissions.ask.len(),
        );
        parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
name = "backend"
description = "FastAPI working environment"

skills = ["skills/fastapi-best-practices"]
agents = ["agents/api-reviewer.md"]
commands = ["commands/migrate.md"]
scripts = ["hooks/guard.sh"]

[mcp.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]

[permissions]
allow = ["Bash(pytest:*)", "Bash(ruff:*)"]
deny = ["Read(./.env)"]

[[hooks]]
event = "PreToolUse"
matcher = "Bash"
command = "hooks/guard.sh"
timeout = 30
"#;

    #[test]
    fn parses_a_complete_bundle() {
        let b = BundleManifest::parse(SAMPLE).unwrap();
        assert_eq!(b.name, "backend");
        assert_eq!(b.skills.len(), 1);
        assert_eq!(b.agents.len(), 1);
        assert_eq!(b.commands.len(), 1);
        assert_eq!(b.mcp.len(), 1);
        assert_eq!(b.permissions.allow.len(), 2);
        assert_eq!(b.hooks[0].event, "PreToolUse");
        assert_eq!(b.hooks[0].matcher.as_deref(), Some("Bash"));
    }

    #[test]
    fn rejects_an_unknown_hook_event() {
        let bad = SAMPLE.replace("event = \"PreToolUse\"", "event = \"PreToolUze\"");
        let err = BundleManifest::parse(&bad).unwrap_err().to_string();
        assert!(err.contains("PreToolUze"), "{}", err);
    }

    #[test]
    fn rejects_a_hook_whose_script_is_not_shipped() {
        let bad = SAMPLE.replace("scripts = [\"hooks/guard.sh\"]", "scripts = []");
        let err = BundleManifest::parse(&bad).unwrap_err().to_string();
        assert!(err.contains("not listed under"), "{}", err);
    }

    #[test]
    fn rejects_paths_escaping_the_bundle() {
        for bad_path in ["../../etc/passwd", "/etc/passwd"] {
            let bad = SAMPLE.replace("agents/api-reviewer.md", bad_path);
            assert!(
                BundleManifest::parse(&bad).is_err(),
                "accepted {}",
                bad_path
            );
        }
    }

    #[test]
    fn rejects_an_unnamed_bundle() {
        assert!(BundleManifest::parse("name = \"\"").is_err());
    }

    #[test]
    fn hook_expands_to_the_settings_shape() {
        let b = BundleManifest::parse(SAMPLE).unwrap();
        let entry = b.hooks[0].to_settings_entry("${CLAUDE_PROJECT_DIR}/.claude/x/guard.sh");
        assert_eq!(entry["type"], "command");
        assert_eq!(entry["command"], "${CLAUDE_PROJECT_DIR}/.claude/x/guard.sh");
        assert_eq!(entry["timeout"], 30);
    }

    #[test]
    fn hook_keys_distinguish_different_hooks() {
        let b = BundleManifest::parse(SAMPLE).unwrap();
        let mut other = b.hooks[0].clone();
        other.matcher = Some("Write".to_string());
        assert_ne!(b.hooks[0].key(), other.key());
        assert_eq!(b.hooks[0].key(), b.hooks[0].clone().key());
    }

    #[test]
    fn summarises_contents() {
        let b = BundleManifest::parse(SAMPLE).unwrap();
        let summary = b.summary();
        assert!(summary.contains(&("subagent", 1)));
        assert!(summary.contains(&("permission rule", 3)));
    }
}
