//! `apm install` Command
//!
//! Installs an agent configuration into the target editor.

use anyhow::{Context, Result};
use colored::Colorize;

use crate::core::agent::AgentConfig;
use crate::core::registry::Registry;
use crate::installers::{get_installer, Target};
use crate::utils::{ui, validation};

use super::super::TargetArg;

/// Execute the install command
pub async fn execute(agent_name: &str, target: TargetArg, global: bool) -> Result<()> {
    let target: Target = target.into();

    ui::print_header(&format!("Installing {}", agent_name));

    // Step 1: Fetch agent from registry
    let spinner = ui::create_spinner("Fetching agent configuration...");

    let registry = Registry::new();
    let agent: AgentConfig = registry
        .fetch_agent(agent_name)
        .await
        .context(format!("Agent '{}' not found in registry", agent_name))?;

    spinner.finish_with_message(format!("{} Found {} v{}", "✓".green(), agent.name, agent.version));

    // Step 2: Validate required tools
    println!("\n{} Checking dependencies...", "→".cyan());

    let missing_tools = validation::check_agent_dependencies(&agent);
    if !missing_tools.is_empty() {
        println!();
        for tool in &missing_tools {
            println!(
                "  {} {} is required but not found in PATH",
                "⚠".yellow().bold(),
                tool.as_str().bold()
            );
        }
        println!();
        println!(
            "  {} Some MCP tools may not work without these dependencies.",
            "!".yellow()
        );
        println!(
            "  {} Install missing tools and try again, or continue anyway.",
            "→".cyan()
        );
        println!();
    } else {
        println!("  {} All dependencies satisfied", "✓".green());
    }

    // Step 3: Get the appropriate installer
    let installer = get_installer(target, global);

    // Step 4: Install identity
    let spinner = ui::create_spinner("Installing identity (system prompt)...");
    installer.install_identity(&agent)?;
    spinner.finish_with_message(format!("{} Identity installed", "✓".green()));

    // Step 5: Install skills
    if !agent.skills.is_empty() {
        let spinner = ui::create_spinner(&format!("Installing {} skill(s)...", agent.skills.len()));
        installer.install_skills(&agent)?;
        spinner.finish_with_message(format!(
            "{} {} skill(s) installed",
            "✓".green(),
            agent.skills.len()
        ));
    }

    // Step 6: Install MCP tools
    if !agent.mcp.is_empty() {
        let spinner = ui::create_spinner(&format!("Configuring {} MCP tool(s)...", agent.mcp.len()));
        installer.install_tools(&agent)?;
        spinner.finish_with_message(format!(
            "{} {} MCP tool(s) configured",
            "✓".green(),
            agent.mcp.len()
        ));
    }

    // Success message
    println!();
    ui::print_success(&format!(
        "{} installed successfully to {}!",
        agent.name,
        target.display_name()
    ));

    // Print next steps
    println!("\n  {} Next steps:", "→".cyan());
    match target {
        Target::Claude => {
            println!("    1. Restart Claude Code to load the new agent");
            println!("    2. The agent will be available in your conversations");
        }
        Target::Cursor => {
            println!("    1. Restart Cursor to load the new rules");
            println!("    2. The agent context will be available in Composer");
        }
    }

    Ok(())
}

