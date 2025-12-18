//! Agent Configuration Models
//!
//! Defines the universal schema for agent.yaml files.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The main agent configuration matching the universal agent.yaml schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent name (e.g., "rust-architect")
    pub name: String,

    /// Semantic version (e.g., "1.0.0")
    pub version: String,

    /// Human-readable description
    pub description: String,

    /// Author or organization
    pub author: String,

    /// Identity configuration (the brain)
    pub identity: Identity,

    /// Skills/knowledge base (optional)
    #[serde(default)]
    pub skills: Vec<Skill>,

    /// MCP tool configurations (optional)
    #[serde(default)]
    pub mcp: Vec<McpTool>,
}

/// Identity configuration - becomes the system prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    /// Preferred model (e.g., "claude-3-5-sonnet-latest")
    #[serde(default)]
    pub model: Option<String>,

    /// Emoji icon for the agent
    #[serde(default)]
    pub icon: Option<String>,

    /// The system prompt that defines the agent's behavior
    pub system_prompt: String,
}

/// Skill definition - becomes markdown files or context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Skill name (used for filename)
    pub name: String,

    /// Skill content (markdown or plain text)
    pub content: String,
}

/// MCP Tool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Tool name
    pub name: String,

    /// Command to execute
    pub command: String,

    /// Command arguments
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Minimal agent info for registry listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
}

impl From<&AgentConfig> for AgentInfo {
    fn from(config: &AgentConfig) -> Self {
        Self {
            name: config.name.clone(),
            version: config.version.clone(),
            description: config.description.clone(),
            author: config.author.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_agent_yaml() {
        let yaml = r#"
name: "test-agent"
version: "1.0.0"
description: "A test agent"
author: "test-author"
identity:
  model: "claude-3-5-sonnet-latest"
  icon: "🧪"
  system_prompt: |
    You are a test agent.
skills:
  - name: "test-skill"
    content: |
      # Test Skill
      This is a test skill.
mcp:
  - name: "test-mcp"
    command: "echo"
    args: ["hello"]
    env:
      TEST_VAR: "value"
"#;

        let agent: AgentConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(agent.name, "test-agent");
        assert_eq!(agent.version, "1.0.0");
        assert_eq!(agent.skills.len(), 1);
        assert_eq!(agent.mcp.len(), 1);
    }
}
