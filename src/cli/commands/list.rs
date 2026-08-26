//! `agentpm list` — show what the registry offers.

use anyhow::Result;
use comfy_table::{
    presets::UTF8_HORIZONTAL_ONLY, Attribute, Cell, Color, ContentArrangement, Table,
};

use crate::core::agent::AgentInfo;
use crate::core::registry::Registry;
use crate::utils::ui;

pub async fn execute() -> Result<()> {
    ui::intro("agentpm list");

    let spinner = ui::Spinner::start("Fetching registry…");
    let registry = Registry::new();
    let agents: Vec<AgentInfo> = registry.fetch_agents().await?;
    spinner.clear();

    if agents.is_empty() {
        ui::warning("The registry returned no agents");
        ui::outro("Nothing to show");
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_style(UTF8_HORIZONTAL_ONLY)
        // Adapts to the real terminal width instead of a hardcoded 85 columns,
        // wrapping descriptions rather than cutting them off.
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("NAME").add_attribute(Attribute::Bold),
            Cell::new("VERSION").add_attribute(Attribute::Bold),
            Cell::new("DESCRIPTION").add_attribute(Attribute::Bold),
            Cell::new("AUTHOR").add_attribute(Attribute::Bold),
        ]);

    for agent in &agents {
        table.add_row(vec![
            Cell::new(&agent.name).fg(Color::Green),
            Cell::new(&agent.version).fg(Color::DarkGrey),
            Cell::new(&agent.description),
            Cell::new(&agent.author).fg(Color::DarkGrey),
        ]);
    }

    ui::step(&table.to_string());

    ui::outro(&format!(
        "{} available {} install with {}",
        agents.len(),
        ui::dim("·"),
        ui::accent("agentpm install <name>")
    ));

    Ok(())
}
