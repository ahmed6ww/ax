//! CLI Commands Module

pub mod init;
pub mod install;
pub mod list;
pub mod uninstall;

use anyhow::Result;
use colored::Colorize;

use crate::core::agent::AgentConfig;

/// Prompt for any API key an MCP server declares via `setup_url`.
///
/// Only the placeholder whose value is a `${...}` reference is replaced, so one
/// key cannot be sprayed into unrelated environment variables.
pub fn prompt_for_api_keys(mut agent: AgentConfig) -> Result<AgentConfig> {
    use std::io::Write;

    for tool in &mut agent.mcp {
        let Some(url) = tool.setup_url.clone() else {
            continue;
        };

        let placeholders: Vec<String> = tool
            .env
            .iter()
            .filter(|(_, v)| v.starts_with("${") && v.ends_with('}'))
            .map(|(k, _)| k.clone())
            .collect();

        if placeholders.is_empty() {
            continue;
        }

        println!();
        println!(
            "  {} MCP server '{}' needs an API key",
            "ℹ".blue().bold(),
            tool.name.bold()
        );
        println!("  {} Get one at: {}", "→".cyan(), url.underline().blue());

        for key in placeholders {
            print!(
                "  {} {} (Enter to leave as an environment reference): ",
                "?".yellow().bold(),
                key.bold()
            );
            std::io::stdout().flush().ok();

            let entered = rpassword::prompt_password("").unwrap_or_default();
            let entered = entered.trim();

            if entered.is_empty() {
                println!("  {} Left as ${{{}}} — export it in your shell", "·".dimmed(), key);
            } else {
                tool.env.insert(key.clone(), entered.to_string());
                println!("  {} {} set", "✓".green(), key);
            }
        }
    }

    Ok(agent)
}
