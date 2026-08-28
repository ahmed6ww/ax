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
                        "{} is not valid TOML. axur will not overwrite it — \
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
            // Codex has no `${VAR}` expansion inside a static `env` table —
            // unlike Claude Code's .mcp.json, a value written that way is
            // passed to the server literally, unexpanded. A bare `${NAME}`
            // reference is instead forwarded by name in `env_vars`, which
            // Codex fills from its own process environment at launch; only a
            // genuine literal (e.g. NODE_ENV = "production") goes in `env`.
            let mut env_vars: Vec<String> = Vec::new();
            let mut literal: Vec<(&String, &String)> = Vec::new();
            for (key, value) in &tool.env {
                match crate::core::agent::env_var_reference(value) {
                    Some(name) => env_vars.push(name.to_string()),
                    None => literal.push((key, value)),
                }
            }

            if !literal.is_empty() {
                literal.sort_by_key(|(k, _)| (*k).clone());
                let mut env = toml::Table::new();
                for (key, value) in literal {
                    env.insert(key.clone(), toml::Value::String(value.clone()));
                }
                entry.insert("env".to_string(), toml::Value::Table(env));
            }
            if !env_vars.is_empty() {
                env_vars.sort();
                env_vars.dedup();
                entry.insert(
                    "env_vars".to_string(),
                    toml::Value::Array(env_vars.into_iter().map(toml::Value::String).collect()),
                );
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
