//! Shared configuration types.
//!
//! What remains after the agent.yaml format was retired in favour of
//! `axur.toml` plus `BUNDLE.toml`. An "agent" in the old sense — an identity
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
}

/// If `value` is written as a bare `${NAME}` or `${NAME:-default}` reference
/// and nothing else, return `NAME`.
///
/// Distinguishes "read this from the developer's own environment at launch" —
/// how Claude Code and Codex both expect a secret to be supplied — from a
/// literal static value someone wants baked into the config verbatim, such as
/// `NODE_ENV = "production"`. Neither target's installer ever writes the
/// resolved value itself; axur only ever sees and stores the reference.
pub fn env_var_reference(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("${")?.strip_suffix('}')?;
    let name = inner.split(":-").next().unwrap_or(inner);
    let valid = !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_a_bare_reference() {
        assert_eq!(
            env_var_reference("${LINEAR_API_KEY}"),
            Some("LINEAR_API_KEY")
        );
    }

    #[test]
    fn recognises_a_reference_with_a_default() {
        assert_eq!(
            env_var_reference("${API_BASE_URL:-https://api.example.com}"),
            Some("API_BASE_URL")
        );
    }

    #[test]
    fn rejects_a_literal_value() {
        assert_eq!(env_var_reference("production"), None);
    }

    #[test]
    fn rejects_text_around_a_reference() {
        // Not a bare reference: Codex has no way to expand this, and writing
        // the literal text into env_vars would forward the wrong name.
        assert_eq!(env_var_reference("Bearer ${TOKEN}"), None);
    }

    #[test]
    fn rejects_an_invalid_variable_name() {
        assert_eq!(env_var_reference("${1NVALID}"), None);
        assert_eq!(env_var_reference("${}"), None);
    }
}
