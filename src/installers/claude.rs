//! Claude Code Installer
//!
//! Installs agent configurations into Claude Code's native format.
//!
//! Output structure:
//! - ~/.claude/agents/{name}.md - Agent as Markdown with YAML frontmatter
//! - claude_desktop_config.json - MCP tool configuration

use anyhow::{Context, Result};
use serde_json::{json, Value};
use colored::Colorize;
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

    /// Get the Claude Code config path for MCP servers
    /// On Linux: ~/.config/claude/config.json
    /// On macOS: ~/Library/Application Support/Claude/config.json
    fn get_mcp_config_path(&self) -> Result<PathBuf> {
        #[cfg(target_os = "linux")]
        {
            let config_dir = dirs::config_dir()
                .context("Could not find config directory")?;
            Ok(config_dir.join("claude").join("config.json"))
        }

        #[cfg(target_os = "macos")]
        {
            let home = dirs::home_dir()
                .context("Could not find home directory")?;
            Ok(home.join("Library/Application Support/Claude/config.json"))
        }

        #[cfg(target_os = "windows")]
        {
            let config_dir = dirs::config_dir()
                .context("Could not find config directory")?;
            Ok(config_dir.join("Claude").join("config.json"))
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let home = dirs::home_dir()
                .context("Could not find home directory")?;
            Ok(home.join(".claude.json"))
        }
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

        // Format skills list for frontmatter
        let skills_list = if !agent.skills.is_empty() {
            let names: Vec<String> = agent.skills.iter().map(|s| s.name.clone()).collect();
            format!("\nskills: {}", names.join(", "))
        } else {
            String::new()
        };

        format!(
            r#"---
name: {}
description: {}
model: {}
icon: {}{}
---

{}"#,
            agent.name,
            agent.description,
            model_short,
            icon,
            skills_list,
            agent.identity.system_prompt
        )
    }

    /// Generate SKILL.md content per Agent Skills standard
    /// Format:
    /// ---
    /// name: skill-name
    /// description: Description that helps select the skill
    /// allowed-tools: (optional)
    /// ---
    /// Skill instructions...
    fn generate_skill_md(skill: &crate::core::agent::Skill) -> String {
        let mut frontmatter = format!("---\nname: {}\n", skill.name);
        
        // Add description (required by Agent Skills spec)
        if let Some(desc) = &skill.description {
            frontmatter.push_str(&format!("description: {}\n", desc));
        }
        
        // Add optional fields
        if let Some(license) = &skill.license {
            frontmatter.push_str(&format!("license: {}\n", license));
        }
        
        if let Some(compat) = &skill.compatibility {
            frontmatter.push_str(&format!("compatibility: {}\n", compat));
        }
        
        if let Some(tools) = &skill.allowed_tools {
            frontmatter.push_str(&format!("allowed-tools: {}\n", tools));
        }
        
        if let Some(deps) = &skill.dependencies {
            frontmatter.push_str(&format!("dependencies: {}\n", deps));
        }
        
        // Add metadata if present
        if let Some(metadata) = &skill.metadata {
            frontmatter.push_str("metadata:\n");
            for (key, value) in metadata {
                frontmatter.push_str(&format!("  {}: {}\n", key, value));
            }
        }
        
        frontmatter.push_str("---\n\n");
        frontmatter.push_str(&skill.content);
        
        frontmatter
    }

    /// Copy scripts/, references/, and assets/ subdirectories from source to destination
    fn copy_skill_subdirectories(source_dir: &std::path::Path, dest_dir: &std::path::Path) -> Result<()> {
        let subdirs = ["scripts", "references", "assets"];
        
        for subdir in &subdirs {
            let source_subdir = source_dir.join(subdir);
            if source_subdir.exists() && source_subdir.is_dir() {
                let dest_subdir = dest_dir.join(subdir);
                Self::copy_dir_recursive(&source_subdir, &dest_subdir)?;
            }
        }
        
        Ok(())
    }

    /// Recursively copy a directory
    fn copy_dir_recursive(source: &std::path::Path, dest: &std::path::Path) -> Result<()> {
        fs::create_dir_all(dest)?;
        
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let path = entry.path();
            let dest_path = dest.join(entry.file_name());
            
            if path.is_dir() {
                Self::copy_dir_recursive(&path, &dest_path)?;
            } else {
                fs::copy(&path, &dest_path)?;
            }
        }
        
        Ok(())
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

    fn install_skills(&self, agent: &AgentConfig) -> Result<()> {
        if agent.skills.is_empty() {
            return Ok(());
        }

        let base_dir = self.get_base_dir()?;
        // Skills go in ~/.claude/skills/<skill-name>/SKILL.md (Agent Skills standard)
        let skills_dir = base_dir.join("skills");
        fs::create_dir_all(&skills_dir)?;

        for skill in &agent.skills {
            // Create skill directory: ~/.claude/skills/<skill-name>/
            let skill_folder = skills_dir.join(&skill.name);
            fs::create_dir_all(&skill_folder)?;

            // Generate SKILL.md with proper frontmatter
            let skill_content = Self::generate_skill_md(skill);
            let skill_file = skill_folder.join("SKILL.md");
            fs::write(&skill_file, skill_content)?;

            // Copy subdirectories (scripts, references, assets) if source_dir exists
            if let Some(source_dir) = &skill.source_dir {
                Self::copy_skill_subdirectories(source_dir, &skill_folder)?;
            }
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
            // Claude Code uses "type": "stdio" format
            let tool_config = json!({
                "type": "stdio",
                "command": tool.command,
                "args": tool.args,
                "env": tool.env
            });
            config["mcpServers"][&tool.name] = tool_config;

            // Check for setup URL (API key requirement)
            if let Some(url) = &tool.setup_url {
                println!("\n  {} Setup required for MCP tool '{}'", "ℹ".blue().bold(), tool.name.bold());
                println!("  {} Get your API key here: {}", "→".cyan(), url.underline().blue());
            }
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

        // Remove skills directory
        let skills_dir = self.get_base_dir()?.join("skills").join(agent_name);
        if skills_dir.exists() {
            fs::remove_dir_all(&skills_dir)?;
        }

        // Note: MCP tools are not removed as they might be used by other agents

        Ok(())
    }
}

