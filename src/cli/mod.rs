//! CLI Module - Command-line interface definitions and handlers

pub mod commands;

use clap::{Parser, Subcommand};

/// axur - a package manager for AI coding-agent setups.
///
/// Declare what a project's agent needs in `axur.toml`, pin it in
/// `axur.lock`, and every machine that syncs gets the same setup.
#[derive(Parser, Debug)]
#[command(name = "axur")]
// Without this, clap takes the usage line from argv[0] and Windows users are
// told to run "axur.exe".
#[command(bin_name = "axur")]
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
    /// Initialize axur and detect installed editors
    Init,

    /// Show what this project has installed
    List,

    /// Inspect or clear the download cache
    Cache {
        /// Delete every cached entry
        #[arg(long, default_value = "false")]
        clear: bool,
    },

    /// Show what is authorised to run on this machine
    Audit {
        /// Withdraw approval for a name, so axur asks again
        #[arg(long, value_name = "NAME")]
        revoke: Option<String>,
    },

    /// Add a skill or bundle to axur.toml and sync
    Install {
        /// Source as owner/repo, optionally with #path/inside/repo
        source: String,

        /// Branch, tag or commit to pin. Defaults to the default branch.
        #[arg(long)]
        rev: Option<String>,

        /// Name to install under. Defaults to the last path segment.
        #[arg(long)]
        name: Option<String>,

        /// Approve anything that will run on your machine without prompting
        #[arg(short = 'y', long, default_value = "false")]
        yes: bool,
    },

    /// Install everything axur.toml declares, for the whole team
    Sync {
        /// Verify only: write nothing and exit non-zero if the tree has drifted
        #[arg(long, default_value = "false")]
        check: bool,

        /// Re-resolve every source to its latest commit and rewrite the lockfile
        #[arg(long, default_value = "false")]
        update: bool,

        /// Approve anything that will run on your machine without prompting
        #[arg(short = 'y', long, default_value = "false")]
        yes: bool,

        /// Use only cached content; fail rather than reach the network
        #[arg(long, default_value = "false")]
        offline: bool,

        /// Which agents to install for on this machine, comma-separated
        /// (e.g. claude-code,codex). Skips the prompt and remembers the
        /// choice for next time. Defaults to what was picked last, or asks.
        #[arg(long, value_name = "AGENTS")]
        agents: Option<String>,
    },

    /// Remove a skill or bundle from axur.toml and sync
    Uninstall {
        /// Name it was installed under
        agent: String,
    },

    /// Store API keys and tokens in the OS keychain instead of your shell profile
    Secrets {
        #[command(subcommand)]
        action: Option<SecretsAction>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SecretsAction {
    /// Store a secret, prompted — never pass the value on the command line
    Set {
        /// The env var name it's referenced as in axur.toml, e.g. LINEAR_API_KEY
        name: String,
    },

    /// Remove a stored secret
    Unset {
        /// The env var name
        name: String,
    },

    /// Resolve NAMEs from the keychain into the environment, then run the
    /// command. axur writes this into an MCP server's config itself — there
    /// is normally no reason to run it by hand.
    #[command(hide = true)]
    Exec {
        /// A secret name to resolve. Repeatable.
        #[arg(long = "name", value_name = "NAME")]
        names: Vec<String>,

        /// The real command and its arguments, after `--`
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
}
