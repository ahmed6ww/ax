//! Claude Code Installer
//!
//! Installs agent configurations into Claude Code's native format.
//!
//! Output structure:
//! - ~/.claude/agents/{name}.md - Agent as Markdown with YAML frontmatter
//! - claude_desktop_config.json - MCP tool configuration

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

use super::Installer;
use crate::core::agent::AgentConfig;
use crate::utils::paths;

/// Installer for Claude Code
pub struct ClaudeInstaller {
    /// Whether to install globally
    global: bool,
}

impl ClaudeInstaller {
    pub fn new(global: bool) -> Self {
        Self { global }
    }

    /// Get the base directory for Claude configuration
    fn get_base_dir(&self) -> Result<PathBuf> {
        paths::claude_config_dir()
            .context("Could not find Claude configuration directory")
    }

    /// Get the agents directory
    fn get_agents_dir(&self) -> Result<PathBuf> {
        Ok(self.get_base_dir()?.join("agents"))
    }

    /// Get the Claude Desktop config path
    fn get_desktop_config_path(&self) -> Result<PathBuf> {
        Ok(self.get_base_dir()?.join("claude_desktop_config.json"))
    }

    /// Generate the markdown content with YAML frontmatter
    fn generate_agent_markdown(agent: &AgentConfig) -> String {
        let icon = agent.identity.icon.as_deref().unwrap_or("🤖");
        let model = agent.identity.model.as_deref().unwrap_or("sonnet");
        
        // Extract just the model name (e.g., "sonnet" from "claude-3-5-sonnet-latest")
        let model_short = if model.contains("sonnet") {
            "sonnet"
        } else if model.contains("opus") {
            "opus"
        } else if model.contains("haiku") {
            "haiku"
        } else {
            model
        };

        // Build skills section if present
        let skills_section = if !agent.skills.is_empty() {
            let skills_content: String = agent.skills.iter()
                .map(|s| format!("\n## {}\n\n{}", s.name, s.content))
                .collect();
            format!("\n\n---\n# Knowledge Base\n{}", skills_content)
        } else {
            String::new()
        };

        format!(
            r#"---
name: {}
description: {}
model: {}
icon: {}
---

{}
{}"#,
            agent.name,
            agent.description,
            model_short,
            icon,
            agent.identity.system_prompt,
            skills_section
        )
    }
}

impl Installer for ClaudeInstaller {
    fn install_identity(&self, agent: &AgentConfig) -> Result<()> {
        let agents_dir = self.get_agents_dir()?;
        fs::create_dir_all(&agents_dir)?;

        // Create the agent markdown file (Claude Code format)
        let agent_file = agents_dir.join(format!("{}.md", agent.name));
        let markdown_content = Self::generate_agent_markdown(agent);
        
        fs::write(&agent_file, markdown_content)?;

        Ok(())
    }

    fn install_skills(&self, _agent: &AgentConfig) -> Result<()> {
        // Skills are embedded in the agent markdown file for Claude Code
        // Already handled in install_identity
        Ok(())
    }

    fn install_tools(&self, agent: &AgentConfig) -> Result<()> {
        if agent.mcp.is_empty() {
            return Ok(());
        }

        let config_path = self.get_desktop_config_path()?;

        // Load existing config or create new one
        let mut config: Value = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
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
        // Remove agent file
        let agent_file = self.get_agents_dir()?.join(format!("{}.md", agent_name));
        if agent_file.exists() {
            fs::remove_file(&agent_file)?;
        }

        // Note: MCP tools are not removed as they might be used by other agents

        Ok(())
    }
}

