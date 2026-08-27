//! axur - a package manager for AI coding-agent setups.
//!
//! A CLI tool that installs AI agent configurations into Claude Code and Codex.

pub mod cli;
pub mod core;
pub mod installers;
pub mod utils;

pub use core::config::Config;
pub use installers::{get_installer, Capabilities, Installer, Target};
