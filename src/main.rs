//! agentpm CLI entry point.

use clap::Parser;

use agentpm_lib::cli::{Cli, Commands};
use agentpm_lib::utils::ui;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
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
    };

    if let Err(error) = result {
        // Render failures through the same rail as everything else, so a run
        // that ends badly still reads as one piece rather than a bare panic
        // trailing off the end of a half-drawn diagram.
        let mut message = format!("{}", error);
        for cause in error.chain().skip(1) {
            message.push_str(&format!(
                "
{}",
                ui::dim(&format!("caused by: {}", cause))
            ));
        }
        ui::error(&message);
        ui::outro_cancel("Failed");
        std::process::exit(1);
    }
}
