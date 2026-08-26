//! `apm list` Command
//!
//! Fetches and displays available agents from the registry.

use anyhow::Result;
use colored::Colorize;

use crate::core::agent::AgentInfo;
use crate::core::registry::Registry;
use crate::utils::ui;

/// Truncate on a character boundary.
///
/// Slicing by byte offset panics when the cut falls inside a multi-byte
/// character, which any non-ASCII registry description would trigger.
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{}...", head.trim_end())
}

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
        let description = truncate(&agent.description, 38);

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


#[cfg(test)]
mod tests {
    use super::truncate;

    #[test]
    fn leaves_short_text_alone() {
        assert_eq!(truncate("short", 38), "short");
    }

    #[test]
    fn does_not_panic_on_multibyte_boundaries() {
        // Each of these panics under byte slicing at index 35.
        for text in [
            "Enforce “Two Hats” refactoring — strict cleanup for large repos",
            "🦀 Rust systems engineer optimized for Tokio and zero-cost abstractions",
            "ééééééééééééééééééééééééééééééééééééééééééé",
        ] {
            let out = truncate(text, 38);
            assert!(out.chars().count() <= 38, "{} too long", out);
        }
    }
}
