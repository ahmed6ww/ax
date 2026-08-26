//! Claude Code Installer
//!
//! Writes into the locations Claude Code documents at
//! <https://code.claude.com/docs/en/skills>:
//!
//! - subagent:  `<scope>/agents/<name>.md`
//! - skills:    `<scope>/skills/<name>/SKILL.md`
//! - MCP:       `.mcp.json` (project) or `~/.claude.json` (user)
//!
//! where `<scope>` is `~/.claude` for user scope and `<repo>/.claude` for
//! project scope. Claude Desktop's Application Support directory is a different
//! product and is deliberately not used here.

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;

use super::common::{
    copy_skill_subdirectories, render_skill_md, skill_dir, write_atomic, write_atomic_preserving,
};
use super::{Capabilities, Installer};
use crate::core::agent::AgentConfig;
use crate::utils::paths::{self, Scope};

pub struct ClaudeInstaller {
    scope: Scope,
}

impl ClaudeInstaller {
    pub fn new(scope: Scope) -> Self {
        Self { scope }
    }

    fn skills_dir(&self) -> Result<PathBuf> {
        paths::claude_skills_dir(self.scope)
    }

    fn agents_dir(&self) -> Result<PathBuf> {
        paths::claude_agents_dir(self.scope)
    }

    /// Render the subagent file: YAML frontmatter plus the system prompt.
    fn render_subagent(agent: &AgentConfig) -> Result<String> {
        use serde_yaml::{Mapping, Value as Yaml};

        let mut fm = Mapping::new();
        fm.insert(Yaml::from("name"), Yaml::from(agent.name.as_str()));
        fm.insert(
            Yaml::from("description"),
            Yaml::from(agent.description.as_str()),
        );

        if let Some(model) = agent.identity.model.as_deref() {
            fm.insert(Yaml::from("model"), Yaml::from(short_model_name(model)));
        }

        let frontmatter = serde_yaml::to_string(&Yaml::Mapping(fm))
            .context("Failed to serialize subagent frontmatter")?;

        Ok(format!(
            "---\n{}---\n\n{}\n",
            frontmatter,
            agent.identity.system_prompt.trim_end()
        ))
    }
}

/// Claude Code accepts a model alias rather than a full model id.
fn short_model_name(model: &str) -> &str {
    for alias in ["opus", "sonnet", "haiku"] {
        if model.contains(alias) {
            return alias;
        }
    }
    model
}

impl Installer for ClaudeInstaller {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            subagents: true,
            skills: true,
            mcp: true,
            project_scope: true,
        }
    }

    fn skills_root(&self) -> Result<PathBuf> {
        self.skills_dir()
    }

    fn install_identity(&self, agent: &AgentConfig) -> Result<()> {
        let agents_dir = self.agents_dir()?;
        let file =
            skill_dir(&agents_dir, &format!("{}.md", agent.name)).context("Invalid agent name")?;
        write_atomic(&file, Self::render_subagent(agent)?.as_bytes())
    }

    fn install_skills(&self, agent: &AgentConfig) -> Result<()> {
        let skills_root = self.skills_dir()?;

        for skill in &agent.skills {
            let folder = skill_dir(&skills_root, &skill.name)?;
            let contents = render_skill_md(skill, &agent.description)?;
            write_atomic(&folder.join("SKILL.md"), contents.as_bytes())?;

            if let Some(source_dir) = &skill.source_dir {
                copy_skill_subdirectories(source_dir, &folder)?;
            }
        }

        Ok(())
    }

    fn install_mcp(&self, tools: &[crate::core::agent::McpTool]) -> Result<()> {
        if tools.is_empty() {
            return Ok(());
        }

        let config_path = paths::claude_mcp_config(self.scope)?;

        // Preserve everything already in the file. `~/.claude.json` in
        // particular holds unrelated Claude Code state, and a parse failure
        // means we refuse rather than replace — silently discarding a user's
        // configured servers is worse than failing the install.
        let mut config: Value = if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read {}", config_path.display()))?;
            if content.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(&content).with_context(|| {
                    format!(
                        "{} is not valid JSON. agentpm will not overwrite it — \
                         fix or move the file, then retry.",
                        config_path.display()
                    )
                })?
            }
        } else {
            json!({})
        };

        if !config.is_object() {
            anyhow::bail!("{} does not contain a JSON object", config_path.display());
        }

        let servers = config
            .as_object_mut()
            .unwrap()
            .entry("mcpServers")
            .or_insert_with(|| Value::Object(Map::new()));

        if !servers.is_object() {
            anyhow::bail!(
                "\"mcpServers\" in {} is not a JSON object",
                config_path.display()
            );
        }

        for tool in tools {
            servers[&tool.name] = json!({
                "type": "stdio",
                "command": tool.command,
                "args": tool.args,
                "env": tool.env,
            });
        }

        let rendered = serde_json::to_string_pretty(&config)? + "\n";
        write_atomic_preserving(&config_path, rendered.as_bytes())
    }

    fn uninstall(&self, agent_name: &str) -> Result<()> {
        let agent_file = skill_dir(&self.agents_dir()?, &format!("{}.md", agent_name))?;
        if agent_file.exists() {
            fs::remove_file(&agent_file)?;
        }

        let skill_folder = skill_dir(&self.skills_dir()?, agent_name)?;
        if skill_folder.exists() {
            fs::remove_dir_all(&skill_folder)?;
        }

        Ok(())
    }

    fn location(&self) -> String {
        match self.skills_dir() {
            Ok(p) => p.display().to_string(),
            Err(_) => "<unresolved>".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ids_collapse_to_claude_code_aliases() {
        assert_eq!(short_model_name("claude-3-5-sonnet-latest"), "sonnet");
        assert_eq!(short_model_name("claude-opus-4-6"), "opus");
        assert_eq!(short_model_name("haiku"), "haiku");
        assert_eq!(short_model_name("inherit"), "inherit");
    }
}
