//! agentpm CLI Entry Point

use anyhow::Result;
use clap::Parser;

use agentpm_lib::cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => agentpm_lib::cli::commands::init::execute().await,
        Commands::List => agentpm_lib::cli::commands::list::execute().await,
        Commands::Install {
            agent,
            target,
            global,
        } => agentpm_lib::cli::commands::install::execute(&agent, target, global).await,
        Commands::Sync { check, update } => {
            agentpm_lib::cli::commands::sync::execute(check, update).await
        }
        Commands::Uninstall {
            agent,
            target,
            global,
        } => agentpm_lib::cli::commands::uninstall::execute(&agent, target, global).await,
    }
}
