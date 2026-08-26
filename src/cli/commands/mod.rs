//! CLI Commands Module

pub mod audit;
pub mod init;
pub mod install;
pub mod list;
pub mod sync;
pub mod trust_gate;
pub mod uninstall;

use anyhow::Result;

use crate::core::agent::McpTool;
use crate::utils::ui;

/// Prompt for any API key an MCP server declares via `setup_url`.
///
/// Only the placeholder whose value is a `${...}` reference is replaced, so one
/// key cannot be sprayed into unrelated environment variables. Input is masked,
/// because an echoed key ends up in scrollback and screen recordings.
pub fn prompt_for_api_keys(tools: &mut [McpTool]) -> Result<()> {
    for tool in tools.iter_mut() {
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

        ui::info(&format!(
            "{} needs an API key
{}",
            ui::bold(&tool.name),
            ui::dim(&format!("Get one at {}", url))
        ));

        for key in placeholders {
            // Non-interactive runs leave the environment reference in place
            // rather than blocking on stdin.
            if !ui::is_rich() {
                continue;
            }

            let entered = ui::secret(&format!("{} (Enter to skip)", key))?;
            let entered = entered.trim();

            if entered.is_empty() {
                ui::step(&ui::dim(&format!(
                    "{} left as ${{{}}} — export it in your shell",
                    key, key
                )));
            } else {
                tool.env.insert(key.clone(), entered.to_string());
                ui::step(&format!("{} set", key));
            }
        }
    }

    Ok(())
}
