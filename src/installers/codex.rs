//! Codex Installer
//!
//! Writes into the locations Codex documents at
//! <https://learn.chatgpt.com/docs/build-skills>:
//!
//! - skills: `.agents/skills/<name>/SKILL.md` (project, searched up to the repo
//!   root) and `$HOME/.agents/skills/<name>/SKILL.md` (user)
//! - MCP:    `~/.codex/config.toml`
//!
//! `~/.codex/skills` is **not** a location Codex scans; `~/.codex` holds
//! configuration only.
//!
//! Codex has no subagent concept, so an agent's system prompt is installed as a
//! skill rather than silently dropped.

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use super::common::{skill_dir, write_atomic_preserving};
use super::{Capabilities, Installer};
use crate::utils::paths::{self, Scope};

pub struct CodexInstaller {
    scope: Scope,
}

impl CodexInstaller {
    pub fn new(scope: Scope) -> Self {
        Self { scope }
    }

    fn skills_dir(&self) -> Result<PathBuf> {
        paths::codex_skills_dir(self.scope)
    }
}

impl Installer for CodexInstaller {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // Represented as a skill rather than a native subagent.
            subagents: false,
            skills: true,
            mcp: true,
            project_scope: true,
            // Codex has no subagent, slash command, hook or permission
            // surface. Declaring that here is what makes sync report these as
            // skipped instead of appearing to install them.
            commands: false,
            hooks: false,
            permissions: false,
        }
    }

    fn skills_root(&self) -> Result<PathBuf> {
        self.skills_dir()
    }

    fn install_mcp(&self, tools: &[crate::core::agent::McpTool]) -> Result<()> {
        if tools.is_empty() {
            return Ok(());
        }

        let config_path = paths::codex_mcp_config()?;

        // Parse and re-serialize through the toml crate. The previous
        // implementation concatenated strings, so a quote or newline in a
        // registry-supplied value injected arbitrary TOML.
        let mut doc: toml::Table = if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read {}", config_path.display()))?;
            if content.trim().is_empty() {
                toml::Table::new()
            } else {
                content.parse().with_context(|| {
                    format!(
                        "{} is not valid TOML. agentpm will not overwrite it — \
                         fix or move the file, then retry.",
                        config_path.display()
                    )
                })?
            }
        } else {
            toml::Table::new()
        };

        let servers = doc
            .entry("mcp_servers".to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));

        let servers = servers.as_table_mut().with_context(|| {
            format!(
                "\"mcp_servers\" in {} is not a TOML table",
                config_path.display()
            )
        })?;

        for tool in tools {
            let mut entry = toml::Table::new();
            entry.insert(
                "command".to_string(),
                toml::Value::String(tool.command.clone()),
            );
            if !tool.args.is_empty() {
                entry.insert(
                    "args".to_string(),
                    toml::Value::Array(
                        tool.args
                            .iter()
                            .map(|a| toml::Value::String(a.clone()))
                            .collect(),
                    ),
                );
            }
            if !tool.env.is_empty() {
                let mut env = toml::Table::new();
                let mut keys: Vec<_> = tool.env.keys().collect();
                keys.sort();
                for key in keys {
                    env.insert(key.clone(), toml::Value::String(tool.env[key].clone()));
                }
                entry.insert("env".to_string(), toml::Value::Table(env));
            }

            servers.insert(tool.name.clone(), toml::Value::Table(entry));
        }

        let rendered =
            toml::to_string_pretty(&doc).context("Failed to serialize Codex configuration")?;
        write_atomic_preserving(&config_path, rendered.as_bytes())
    }

    fn uninstall(&self, agent_name: &str) -> Result<()> {
        let skills_root = self.skills_dir()?;

        for name in [format!("{}-identity", agent_name), agent_name.to_string()] {
            let folder = skill_dir(&skills_root, &name)?;
            if folder.exists() {
                crate::core::tx::remove_dir_all(&folder)?;
            }
        }

        Ok(())
    }

    fn location(&self) -> String {
        match self.skills_dir() {
            Ok(path) => crate::utils::paths::display_relative(&path),
            Err(_) => "<unresolved>".to_string(),
        }
    }
}
