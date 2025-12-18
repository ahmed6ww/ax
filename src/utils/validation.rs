//! Validation Utilities
//!
//! Checks for required tool dependencies.

use crate::core::agent::AgentConfig;

/// Check if a tool is available in PATH
pub fn is_tool_available(name: &str) -> bool {
    which::which(name).is_ok()
}

/// Check agent dependencies and return missing tools
pub fn check_agent_dependencies(agent: &AgentConfig) -> Vec<String> {
    let mut missing = Vec::new();

    for tool in &agent.mcp {
        // Check if the command exists
        if !is_tool_available(&tool.command) {
            missing.push(tool.command.clone());
        }
    }

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
