//! CLI Module - Command-line interface definitions and handlers

pub mod commands;

use clap::{Parser, Subcommand, ValueEnum};

use crate::installers::Target;

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

    /// List available agents from the registry
    List,

    /// Install an agent configuration
    Install {
        /// Name of the agent to install
        agent: String,

        /// Target editor (claude, codex). Omit to install to every detected target.
        #[arg(short, long, value_enum)]
        target: Option<TargetArg>,

        /// Install for the user instead of this project
        #[arg(short, long, default_value = "false")]
        global: bool,
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

    /// Remove an installed agent
    Uninstall {
        /// Name of the agent to remove
        agent: String,

        /// Target editor (claude, codex). Omit to remove from every target.
        #[arg(short, long, value_enum)]
        target: Option<TargetArg>,

        /// Remove from user scope instead of this project
        #[arg(short, long, default_value = "false")]
        global: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TargetArg {
    Claude,
    Codex,
}

impl From<TargetArg> for Target {
    fn from(arg: TargetArg) -> Self {
        match arg {
            TargetArg::Claude => Target::Claude,
            TargetArg::Codex => Target::Codex,
        }
    }
}
