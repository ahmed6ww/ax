//! APM CLI Entry Point

use anyhow::Result;
use clap::Parser;

use apm_lib::cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => apm_lib::cli::commands::init::execute().await,
        Commands::List => apm_lib::cli::commands::list::execute().await,
        Commands::Install { agent, target, global } => {
            apm_lib::cli::commands::install::execute(&agent, target, global).await
        }
    }
}

