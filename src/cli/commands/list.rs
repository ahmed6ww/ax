//! `agentpm list` — show what this project has installed.
//!
//! Reads `agentpm.lock`, not a remote catalogue. The answer to "what is
//! installed" has to come from the lockfile, because that is the only record
//! of which commits are actually on disk.

use anyhow::Result;
use comfy_table::{
    presets::UTF8_HORIZONTAL_ONLY, Attribute, Cell, Color, ContentArrangement, Table,
};

use crate::core::lockfile::Lockfile;
use crate::core::manifest::{Manifest, MANIFEST_FILE};
use crate::utils::{paths, ui};

pub async fn execute() -> Result<()> {
    ui::intro("agentpm list");

    let project_root = paths::project_root()?;
    let Some(manifest_path) = Manifest::find(&project_root) else {
        ui::warning(&format!("No {} in this project", MANIFEST_FILE));
        ui::note(
            "Get started",
            "agentpm init                       set up the manifest\n\
             agentpm install <owner>/<repo>#<path>   add a skill or bundle",
        );
        ui::outro("Nothing installed");
        return Ok(());
    };

    let lock_path = Lockfile::path_for(manifest_path.parent().unwrap_or(&project_root));
    if !lock_path.exists() {
        ui::warning("Declared but never synced");
        ui::note("Next", "Run agentpm sync to install and lock.");
        ui::outro("Nothing installed");
        return Ok(());
    }

    let lock = Lockfile::load(&lock_path)?;
    if lock.skills.is_empty() && lock.bundles.is_empty() {
        ui::warning("The lockfile is empty");
        ui::outro("Nothing installed");
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_style(UTF8_HORIZONTAL_ONLY)
        // Adapts to the real terminal width rather than a fixed column count.
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("NAME").add_attribute(Attribute::Bold),
            Cell::new("KIND").add_attribute(Attribute::Bold),
            Cell::new("SOURCE").add_attribute(Attribute::Bold),
            Cell::new("PINNED").add_attribute(Attribute::Bold),
            Cell::new("FILES").add_attribute(Attribute::Bold),
        ]);

    for bundle in &lock.bundles {
        table.add_row(vec![
            Cell::new(&bundle.name).fg(Color::Green),
            Cell::new("bundle").fg(Color::Cyan),
            Cell::new(format!("{}#{}", bundle.source, bundle.path)),
            Cell::new(ui::short_sha(&bundle.resolved)).fg(Color::DarkGrey),
            Cell::new(bundle.files.len()),
        ]);
    }
    for skill in &lock.skills {
        table.add_row(vec![
            Cell::new(&skill.name).fg(Color::Green),
            Cell::new("skill").fg(Color::DarkGrey),
            Cell::new(format!("{}#{}", skill.source, skill.path)),
            Cell::new(ui::short_sha(&skill.resolved)).fg(Color::DarkGrey),
            Cell::new(skill.files.len()),
        ]);
    }

    ui::step(&table.to_string());

    if !lock.mcp.is_empty() {
        ui::step(&format!(
            "MCP servers\n{}",
            lock.mcp
                .iter()
                .map(|m| format!(
                    "{}  {}",
                    m.name,
                    ui::dim(&format!("{} {}", m.command, m.args.join(" ")))
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    let total = lock.skills.len() + lock.bundles.len();
    ui::outro(&format!(
        "{} installed {} pinned in {}",
        total,
        ui::dim("·"),
        ui::accent(crate::core::lockfile::LOCKFILE)
    ));

    Ok(())
}
