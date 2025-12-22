//! CLI Module - Command-line interface definitions and handlers

pub mod commands;

use clap::{Parser, Subcommand, ValueEnum};

use crate::installers::Target;

/// AX (Agent Package Manager) - The npm of the Agentic AI era
///
/// Install AI agent configurations into Claude Code, Cursor, and more.
#[derive(Parser, Debug)]
#[command(name = "ax")]
#[command(author = "ahmed6ww")]
#[command(version = "1.2.1")]
#[command(about = "Write Once, Run on Claude, Cursor, or Codex", long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize AX and detect installed editors
    Init,

    /// List available agents from the registry
    List,

    /// Install an agent configuration
    Install {
        /// Name of the agent to install
        agent: String,

        /// Target editor (claude, cursor)
        #[arg(short, long, value_enum, default_value = "claude")]
        target: TargetArg,

        /// Install globally (applies to all projects)
        #[arg(short, long, default_value = "false")]
        global: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TargetArg {
    Claude,
    Cursor,
    Codex,
}

impl From<TargetArg> for Target {
    fn from(arg: TargetArg) -> Self {
        match arg {
            TargetArg::Claude => Target::Claude,
            TargetArg::Cursor => Target::Cursor,
            TargetArg::Codex => Target::Codex,
        }
    }
}
