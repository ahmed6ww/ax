//! AX (Agent Package Manager) - The npm of the Agentic AI era
//!
//! A CLI tool that installs AI agent configurations into Claude Code and Codex.

pub mod cli;
pub mod core;
pub mod installers;
pub mod utils;

pub use core::agent::AgentConfig;
pub use core::config::ApmConfig;
pub use core::registry::Registry;
pub use installers::{get_installer, Capabilities, Installer, Target};
