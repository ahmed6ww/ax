//! agentpm CLI entry point.

use clap::Parser;

use agentpm_lib::cli::{Cli, Commands};
use agentpm_lib::core::error;
use agentpm_lib::utils::ui;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init => agentpm_lib::cli::commands::init::execute().await,
        Commands::List => agentpm_lib::cli::commands::list::execute().await,
        Commands::Audit { revoke } => agentpm_lib::cli::commands::audit::execute(revoke).await,
        Commands::Cache { clear } => agentpm_lib::cli::commands::cache::execute(clear).await,
        Commands::Install {
            source,
            rev,
            name,
            yes,
        } => agentpm_lib::cli::commands::install::execute(&source, rev, name, yes).await,
        Commands::Sync {
            check,
            update,
            yes,
            offline,
        } => agentpm_lib::cli::commands::sync::execute(check, update, yes, offline).await,
        Commands::Uninstall { agent } => {
            agentpm_lib::cli::commands::uninstall::execute(&agent).await
        }
    };

    match result {
        Ok(()) => error::status(error::exit::OK),
        Err(err) => {
            // Render failures through the same rail as everything else, so a
            // run that ends badly still reads as one piece rather than a bare
            // error trailing off a half-drawn diagram.
            let mut message = format!("{}", err);
            for cause in err.chain().skip(1) {
                message.push('\n');
                message.push_str(&ui::dim(&format!("caused by: {}", cause)));
            }
            ui::error(&message);
            ui::outro_cancel("Failed");

            // A distinct code per failure kind, so a script can tell a drifted
            // lockfile from an unreachable network.
            error::status(error::exit_code_for(&err))
        }
    }
}
