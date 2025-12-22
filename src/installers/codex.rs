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

    /// Generate SKILL.md content per official Codex format
    /// Format:
    /// ---
    /// name: skill-name
    /// description: Description that helps Codex select the skill
    /// metadata:
    ///   short-description: Optional user-facing description
    /// ---
    /// Skill instructions...
    fn generate_skill_md(skill_name: &str, content: &str, agent_description: &str) -> String {
        // Extract a short description from the first line or use agent description
        let short_desc = content.lines()
            .find(|line| !line.trim().is_empty() && !line.starts_with('#'))
            .map(|s| s.trim().chars().take(100).collect::<String>())
            .unwrap_or_else(|| agent_description.chars().take(100).collect());

        format!(
            r#"---
name: {}
description: {}
metadata:
  short-description: {}
---

{}"#,
            skill_name,
            agent_description.chars().take(500).collect::<String>(),
            short_desc,
            content
        )
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
        // Format: ~/.codex/skills/<skill-name>/SKILL.md
        for skill in &agent.skills {
            let skill_folder = skills_dir.join(&skill.name);
            fs::create_dir_all(&skill_folder)?;

            let skill_file = skill_folder.join("SKILL.md");
            let skill_content = Self::generate_skill_md(
                &skill.name,
                &skill.content,
                &agent.description
            );
            
            fs::write(&skill_file, skill_content)?;
        }

        Ok(())
    }

    fn install_tools(&self, _agent: &AgentConfig) -> Result<()> {
        // Codex doesn't support MCP tools in the same way.
        // MCPs are not part of the Codex skill system.
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
