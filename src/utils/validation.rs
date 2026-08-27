//! Validation Utilities
//!
//! Checks for required tool dependencies.

use crate::core::agent::McpTool;

/// Check if a tool is available in PATH
pub fn is_tool_available(name: &str) -> bool {
    which::which(name).is_ok()
}

/// Commands an MCP server needs that are not on PATH.
///
/// A server whose command is missing fails silently at agent startup, so it is
/// worth naming before the install completes.
pub fn missing_mcp_commands(tools: &[McpTool]) -> Vec<String> {
    let mut missing: Vec<String> = tools
        .iter()
        .filter(|tool| !is_tool_available(&tool.command))
        .map(|tool| tool.command.clone())
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

/// Get installation hints for common tools
pub fn get_install_hint(tool: &str) -> Option<&'static str> {
    match tool {
        "docker" => Some("Install Docker: https://docs.docker.com/get-docker/"),
        "cargo" => Some("Install Rust: https://rustup.rs/"),
        "npm" | "npx" => Some("Install Node.js: https://nodejs.org/"),
        "python" | "python3" => Some("Install Python: https://www.python.org/downloads/"),
        "go" => Some("Install Go: https://go.dev/dl/"),
        "uv" => Some("Install uv: pip install uv"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tool_available() {
        // These should exist on most systems
        assert!(is_tool_available("ls") || is_tool_available("dir"));
    }

    #[test]
    fn test_install_hints() {
        assert!(get_install_hint("docker").is_some());
        assert!(get_install_hint("unknown-tool").is_none());
    }
}
