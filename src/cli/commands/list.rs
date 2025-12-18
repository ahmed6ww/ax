//! `apm list` Command
//!
//! Fetches and displays available agents from the registry.

use anyhow::Result;
use colored::Colorize;

use crate::core::agent::AgentInfo;
use crate::core::registry::Registry;
use crate::utils::ui;

/// Execute the list command
pub async fn execute() -> Result<()> {
    ui::print_header("Available Agents");

    let spinner = ui::create_spinner("Fetching registry...");

    let registry = Registry::new();
    let agents: Vec<AgentInfo> = registry.fetch_agents().await?;

    spinner.finish_and_clear();

    if agents.is_empty() {
        println!("  {} No agents found in registry.", "!".yellow().bold());
        return Ok(());
    }

    // Print table header
    println!(
        "  {:<20} {:<10} {:<40} {}",
        "NAME".bold().cyan(),
        "VERSION".bold().cyan(),
        "DESCRIPTION".bold().cyan(),
        "AUTHOR".bold().cyan()
    );
    println!("  {}", "─".repeat(85).dimmed());

    // Print agents
    for agent in &agents {
        let description = if agent.description.len() > 38 {
            format!("{}...", &agent.description[..35])
        } else {
            agent.description.clone()
        };

        println!(
            "  {:<20} {:<10} {:<40} {}",
            agent.name.green(),
            agent.version.dimmed(),
            description,
            agent.author.dimmed()
        );
    }

    println!();
    println!(
        "  {} {} agent(s) available",
        "→".cyan(),
        agents.len().to_string().bold()
    );
    println!(
        "  {} Install with: {}",
        "→".cyan(),
        "ax install <agent-name>".cyan().bold()
    );

    Ok(())
}

