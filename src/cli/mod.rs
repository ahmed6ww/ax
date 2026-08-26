//! CLI Module - Command-line interface definitions and handlers

pub mod commands;

use clap::{Parser, Subcommand};

/// agentpm (Agent Package Manager) - The npm of the Agentic AI era
///
/// Install AI agent configurations into Claude Code and Codex.
#[derive(Parser, Debug)]
#[command(name = "agentpm")]
#[command(author = "ahmed6ww")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Keep your team running the same agent setup on Claude Code and Codex", long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize agentpm and detect installed editors
    Init,

    /// Show what this project has installed
    List,

    /// Add a skill or bundle to agentpm.toml and sync
    Install {
        /// Source as owner/repo, optionally with #path/inside/repo
        source: String,

        /// Branch, tag or commit to pin. Defaults to the default branch.
        #[arg(long)]
        rev: Option<String>,

        /// Name to install under. Defaults to the last path segment.
        #[arg(long)]
        name: Option<String>,
    },

    /// Install everything agentpm.toml declares, for the whole team
    Sync {
        /// Verify only: write nothing and exit non-zero if the tree has drifted
        #[arg(long, default_value = "false")]
        check: bool,

        /// Re-resolve every source to its latest commit and rewrite the lockfile
        #[arg(long, default_value = "false")]
        update: bool,
    },

    /// Remove a skill or bundle from agentpm.toml and sync
    Uninstall {
        /// Name it was installed under
        agent: String,
    },
}
