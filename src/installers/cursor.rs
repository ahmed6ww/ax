//! Cursor Installer
//!
//! Installs agent configurations into Cursor's native format.
//!
//! Output structure:
//! - .cursor/rules/{name}-identity.mdc - Agent identity as MDC rule
//! - .cursor/rules/{skill}.mdc - Agent skills as MDC rules
//! - .cursor/mcp.json - MCP tool configuration

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

use super::Installer;
use crate::core::agent::AgentConfig;
use crate::utils::paths;

/// Installer for Cursor
pub struct CursorInstaller {
    /// Whether to install globally
    global: bool,
}

impl CursorInstaller {
    pub fn new(global: bool) -> Self {
        Self { global }
    }

    /// Get the base directory for Cursor configuration
    fn get_base_dir(&self) -> Result<PathBuf> {
        if self.global {
            paths::cursor_config_dir()
                .context("Could not find Cursor configuration directory")
        } else {
            // For non-global, use the current project's .cursor directory
            Ok(PathBuf::from(".cursor"))
        }
    }

    /// Get the rules directory
    fn get_rules_dir(&self) -> Result<PathBuf> {
        Ok(self.get_base_dir()?.join("rules"))
    }

    /// Get the MCP config path
    fn get_mcp_config_path(&self) -> Result<PathBuf> {
        Ok(self.get_base_dir()?.join("mcp.json"))
    }

    /// Generate MDC frontmatter for a rule file
    fn generate_mdc_content(name: &str, description: &str, content: &str) -> String {
        format!(
            r#"---
description: {}
globs: 
alwaysApply: true
---

# {}

{}
"#,
            description, name, content
        )
    }
}

impl Installer for CursorInstaller {
    fn install_identity(&self, agent: &AgentConfig) -> Result<()> {
        let rules_dir = self.get_rules_dir()?;
        fs::create_dir_all(&rules_dir)?;

        // Create the identity MDC file
        let identity_file = rules_dir.join(format!("{}-identity.mdc", agent.name));

        let icon = agent.identity.icon.as_deref().unwrap_or("🤖");
        let mdc_content = Self::generate_mdc_content(
            &format!("{} {} Agent", icon, agent.name),
            &agent.description,
            &agent.identity.system_prompt,
        );

        fs::write(&identity_file, mdc_content)?;

        Ok(())
    }

    fn install_skills(&self, agent: &AgentConfig) -> Result<()> {
        if agent.skills.is_empty() {
            return Ok(());
        }

        let rules_dir = self.get_rules_dir()?;
        fs::create_dir_all(&rules_dir)?;

        for skill in &agent.skills {
            let skill_file = rules_dir.join(format!("{}-{}.mdc", agent.name, skill.name));

            let mdc_content = Self::generate_mdc_content(
                &format!("{} - {}", agent.name, skill.name),
                &format!("Knowledge base for {} agent", agent.name),
                &skill.content,
            );

            fs::write(&skill_file, mdc_content)?;
        }

        Ok(())
    }

    fn install_tools(&self, agent: &AgentConfig) -> Result<()> {
        if agent.mcp.is_empty() {
            return Ok(());
        }

        let config_path = self.get_mcp_config_path()?;

        // Load existing config or create new one
        let mut config: Value = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            serde_json::from_str(&content).unwrap_or_else(|_| json!({"mcpServers": {}}))
        } else {
            json!({"mcpServers": {}})
        };

        // Ensure mcpServers object exists
        if config.get("mcpServers").is_none() {
            config["mcpServers"] = json!({});
        }

        // Add each MCP tool
        for tool in &agent.mcp {
            let tool_config = json!({
                "command": tool.command,
                "args": tool.args,
                "env": tool.env
            });
            config["mcpServers"][&tool.name] = tool_config;
        }

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write the updated config
        fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

        Ok(())
    }

    fn uninstall(&self, agent_name: &str) -> Result<()> {
        let rules_dir = self.get_rules_dir()?;

        // Remove identity file
        let identity_file = rules_dir.join(format!("{}-identity.mdc", agent_name));
        if identity_file.exists() {
            fs::remove_file(&identity_file)?;
        }

        // Remove all skill files for this agent
        if rules_dir.exists() {
            for entry in fs::read_dir(&rules_dir)? {
                let entry = entry?;
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();

                if file_name.starts_with(&format!("{}-", agent_name))
                    && file_name.ends_with(".mdc")
                {
                    fs::remove_file(entry.path())?;
                }
            }
        }

        // Note: MCP tools are not removed as they might be used by other agents

        Ok(())
    }
}
