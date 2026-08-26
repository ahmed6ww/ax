//! `ax uninstall` — remove an installed agent.

use anyhow::Result;
use colored::Colorize;

use crate::installers::{get_installer, Target};
use crate::utils::paths::Scope;
use crate::utils::ui;

use super::super::TargetArg;

pub async fn execute(agent_name: &str, target: Option<TargetArg>, global: bool) -> Result<()> {
    let targets: Vec<Target> = match target {
        Some(t) => vec![t.into()],
        None => Target::all().to_vec(),
    };
    let scope = Scope::from_global_flag(global);

    ui::print_header(&format!("Removing {}", agent_name));

    for target in targets {
        let installer = get_installer(target, scope);
        installer.uninstall(agent_name)?;
        println!(
            "  {} {} ({} scope) → {}",
            "✓".green(),
            target.display_name().bold(),
            scope.display_name(),
            installer.location().dimmed()
        );
    }

    println!();
    ui::print_success(&format!("{} removed.", agent_name));
    println!(
        "\n  {} MCP servers are left in place; they may be shared with other agents.",
        "·".dimmed()
    );

    Ok(())
}
