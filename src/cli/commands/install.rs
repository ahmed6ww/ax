//! `agentpm install` — install an agent into Claude Code and/or Codex.

use anyhow::{Context, Result};
use colored::Colorize;

use crate::core::agent::AgentConfig;
use crate::core::registry::Registry;
use crate::installers::{get_installer, Target};
use crate::utils::paths::Scope;
use crate::utils::{ui, validation};

use super::super::TargetArg;

/// Resolve which targets to act on: the one named, or every detected target.
fn resolve_targets(target: Option<TargetArg>) -> Result<Vec<Target>> {
    if let Some(t) = target {
        return Ok(vec![t.into()]);
    }

    let detected: Vec<Target> = Target::all()
        .into_iter()
        .filter(|t| match t {
            Target::Claude => crate::utils::paths::claude_detected(),
            Target::Codex => crate::utils::paths::codex_detected(),
        })
        .collect();

    if detected.is_empty() {
        anyhow::bail!(
            "No supported agent detected. Install Claude Code or Codex, \
             or name a target explicitly with --target."
        );
    }

    Ok(detected)
}

pub async fn execute(agent_name: &str, target: Option<TargetArg>, global: bool) -> Result<()> {
    let targets = resolve_targets(target)?;
    let scope = Scope::from_global_flag(global);

    ui::print_header(&format!("Installing {}", agent_name));

    let spinner = ui::create_spinner("Fetching agent configuration...");
    let registry = Registry::new();
    let agent: AgentConfig = registry
        .fetch_agent(agent_name)
        .await
        .with_context(|| format!("Could not resolve agent '{}'", agent_name))?;
    spinner.finish_with_message(format!(
        "{} Found {} v{}",
        "✓".green(),
        agent.name,
        agent.version
    ));

    println!("\n{} Checking dependencies...", "→".cyan());
    let missing = validation::check_agent_dependencies(&agent);
    if missing.is_empty() {
        println!("  {} All dependencies satisfied", "✓".green());
    } else {
        println!();
        for tool in &missing {
            println!(
                "  {} {} is required but not found in PATH",
                "⚠".yellow().bold(),
                tool.as_str().bold()
            );
            if let Some(hint) = validation::get_install_hint(tool) {
                println!("    {}", hint.dimmed());
            }
        }
        println!();
    }

    let agent = super::prompt_for_api_keys(agent)?;

    for target in targets {
        let installer = get_installer(target, scope);
        let caps = installer.capabilities();

        println!(
            "\n  {} {} ({} scope) → {}",
            "▸".cyan().bold(),
            target.display_name().bold(),
            scope.display_name(),
            installer.location().dimmed()
        );

        if caps.subagents {
            installer.install_identity(&agent)?;
            println!("    {} Subagent installed", "✓".green());
        } else {
            println!(
                "    {} No subagent support — identity installed as a skill",
                "·".dimmed()
            );
        }

        if caps.skills {
            installer.install_skills(&agent)?;
            let count = agent.skills.len() + if caps.subagents { 0 } else { 1 };
            println!("    {} {} skill(s) installed", "✓".green(), count);
        }

        if !agent.mcp.is_empty() {
            if caps.mcp {
                installer.install_tools(&agent)?;
                println!(
                    "    {} {} MCP server(s) configured",
                    "✓".green(),
                    agent.mcp.len()
                );
            } else {
                println!(
                    "    {} {} MCP server(s) skipped — not supported by {}",
                    "·".dimmed(),
                    agent.mcp.len(),
                    target.display_name()
                );
            }
        }
    }

    println!();
    ui::print_success(&format!("{} installed.", agent.name));
    println!(
        "\n  {} Restart your agent to pick up the new configuration.",
        "→".cyan()
    );

    Ok(())
}
