//! Codex Installer
//!
//! Installs agent configurations into Codex's native format.
//!
//! Output structure (per official docs):
//! - ~/.codex/skills/<skill-name>/SKILL.md - Skills as Markdown with YAML frontmatter
//!
//! Note: Codex only uses Skills. Agents and MCPs are not supported.
//! See: https://developers.openai.com/codex/skills

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use super::Installer;
use crate::core::agent::AgentConfig;
use crate::utils::paths;

/// Installer for Codex
pub struct CodexInstaller {
    /// Whether to install globally
    #[allow(dead_code)]
    global: bool,
}

impl CodexInstaller {
    pub fn new(global: bool) -> Self {
        Self { global }
    }

    /// Get the base directory for Codex configuration (~/.codex)
    fn get_base_dir(&self) -> Result<PathBuf> {
        paths::codex_config_dir()
            .context("Could not find Codex configuration directory")
    }

    /// Get the skills directory (~/.codex/skills)
    fn get_skills_dir(&self) -> Result<PathBuf> {
        Ok(self.get_base_dir()?.join("skills"))
    }

    /// Generate SKILL.md content per Agent Skills standard
    /// Format:
    /// ---
    /// name: skill-name
    /// description: Description that helps select the skill
    /// allowed-tools: (optional)
    /// ---
    /// Skill instructions...
    fn generate_skill_md(skill: &crate::core::agent::Skill, fallback_description: &str) -> String {
        let mut frontmatter = format!("---\nname: {}\n", skill.name);
        
        // Use skill's own description or fallback to agent description
        let description = skill.description.as_deref().unwrap_or(fallback_description);
        frontmatter.push_str(&format!("description: {}\n", description));
        
        // Add optional fields per Agent Skills spec
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

impl Installer for CodexInstaller {
    fn install_identity(&self, _agent: &AgentConfig) -> Result<()> {
        // Codex doesn't use agents in the same way as Claude Code.
        // The "identity" concept is handled through skills in Codex.
        // We skip this step for Codex.
        Ok(())
    }

    fn install_skills(&self, agent: &AgentConfig) -> Result<()> {
        if agent.skills.is_empty() {
            return Ok(());
        }

        let skills_dir = self.get_skills_dir()?;

        // Each skill goes in its own directory with a SKILL.md file
        // Format: ~/.codex/skills/<skill-name>/SKILL.md (Agent Skills standard)
        for skill in &agent.skills {
            let skill_folder = skills_dir.join(&skill.name);
            fs::create_dir_all(&skill_folder)?;

            let skill_file = skill_folder.join("SKILL.md");
            let skill_content = Self::generate_skill_md(skill, &agent.description);
            
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

        let config_path = self.get_base_dir()?.join("config.toml");
        
        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Read existing config or create new
        let mut existing_content = if config_path.exists() {
            fs::read_to_string(&config_path)?
        } else {
            String::new()
        };

        // Append MCP server configurations
        // Format per Codex docs:
        // [mcp_servers.<name>]
        // command = "..."
        // args = ["...", "..."]
        // [mcp_servers.<name>.env]
        // VAR = "value"
        for tool in &agent.mcp {
            // Check if this server already exists
            let server_header = format!("[mcp_servers.{}]", tool.name);
            if existing_content.contains(&server_header) {
                // Skip if already configured
                continue;
            }

            // Build the TOML section
            let mut section = format!("\n{}\n", server_header);
            section.push_str(&format!("command = \"{}\"\n", tool.command));
            
            if !tool.args.is_empty() {
                let args_str: Vec<String> = tool.args.iter()
                    .map(|a| format!("\"{}\"", a))
                    .collect();
                section.push_str(&format!("args = [{}]\n", args_str.join(", ")));
            }
            
            if !tool.env.is_empty() {
                section.push_str(&format!("\n[mcp_servers.{}.env]\n", tool.name));
                for (key, value) in &tool.env {
                    section.push_str(&format!("{} = \"{}\"\n", key, value));
                }
            }

            existing_content.push_str(&section);

            // Show setup URL if present
            if let Some(url) = &tool.setup_url {
                use colored::Colorize;
                println!("\n  {} Setup required for MCP tool '{}'", "ℹ".blue().bold(), tool.name.bold());
                println!("  {} Get your API key here: {}", "→".cyan(), url.underline().blue());
            }
        }

        // Write the updated config
        fs::write(&config_path, existing_content)?;

        Ok(())
    }

    fn uninstall(&self, agent_name: &str) -> Result<()> {
        // For Codex, we installed skills named after the skill, not the agent.
        // We need to track which skills belong to which agent, or just remove by skill name.
        // For now, try to remove a skill folder with the agent name (fallback)
        let skills_dir = self.get_skills_dir()?;
        let skill_folder = skills_dir.join(agent_name);
        
        if skill_folder.exists() {
            fs::remove_dir_all(&skill_folder)?;
        }

        Ok(())
    }
}
