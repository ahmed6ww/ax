//! Shared configuration types.
//!
//! What remains after the agent.yaml format was retired in favour of
//! `agentpm.toml` plus `BUNDLE.toml`. An "agent" in the old sense — an identity
//! with skills and servers — is now expressed as a bundle.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    /// Optional URL for setup instructions (e.g. API key generation)
    #[serde(default)]
    pub setup_url: Option<String>,
}
