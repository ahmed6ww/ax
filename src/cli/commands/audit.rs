//! `axur audit` — what is authorised to run on this machine.
//!
//! Read-only. Answers the question a security review actually asks: not "what
//! skills are installed" but "what code did installing them authorise, where
//! did it come from, and when did someone agree to it".

use anyhow::Result;
use comfy_table::{
    presets::UTF8_HORIZONTAL_ONLY, Attribute, Cell, Color, ContentArrangement, Table,
};

use crate::core::lockfile::Lockfile;
use crate::core::manifest::Manifest;
use crate::core::trust::{Grant, TrustStore};
use crate::utils::{paths, ui};

pub async fn execute(revoke: Option<String>) -> Result<()> {
    ui::intro("axur audit");

    let mut store = TrustStore::load()?;

    if let Some(label) = revoke {
        let removed = store.revoke(&label);
        if removed == 0 {
            ui::warning(&format!("Nothing approved under '{}'", label));
            ui::outro_cancel("Nothing revoked");
            return Ok(());
        }
        store.save()?;
        ui::success(&format!(
            "Revoked {} ({} entries)",
            ui::bold(&label),
            removed
        ));
        ui::outro("axur will ask again next sync");
        return Ok(());
    }

    if store.entries.is_empty() {
        ui::step("Nothing has been authorised to run on this machine");
        // A fresh machine is exactly when the declared-but-unapproved list
        // matters most: it is what the next sync will ask about.
        let pending = report_unapproved(&store)?;
        ui::outro(if pending > 0 {
            "Clean — but this project declares code that is not approved here"
        } else {
            "Clean"
        });
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_style(UTF8_HORIZONTAL_ONLY)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("NAME").add_attribute(Attribute::Bold),
            Cell::new("KIND").add_attribute(Attribute::Bold),
            Cell::new("RUNS").add_attribute(Attribute::Bold),
            Cell::new("FROM").add_attribute(Attribute::Bold),
        ]);

    for entry in &store.entries {
        table.add_row(vec![
            Cell::new(&entry.label).fg(Color::Green),
            Cell::new(match entry.kind {
                Grant::Mcp => "mcp",
                Grant::Hook => "hook",
            })
            .fg(Color::Cyan),
            Cell::new(&entry.detail),
            Cell::new(&entry.origin).fg(Color::DarkGrey),
        ]);
    }

    ui::step(&table.to_string());

    report_unapproved(&store)?;

    ui::outro(&format!(
        "{} authorised {} revoke with {}",
        store.entries.len(),
        ui::dim("·"),
        ui::accent("axur audit --revoke <name>")
    ));

    Ok(())
}

/// Warn about anything this project declares that this machine has not
/// approved. Returns how many were found.
fn report_unapproved(store: &TrustStore) -> Result<usize> {
    let Some(manifest_path) = Manifest::find(&paths::project_root()?) else {
        return Ok(0);
    };
    let Some(root) = manifest_path.parent() else {
        return Ok(0);
    };
    let lock_path = Lockfile::path_for(root);
    if !lock_path.exists() {
        return Ok(0);
    }

    let lock = Lockfile::load(&lock_path)?;
    let pending: Vec<String> = lock
        .mcp
        .iter()
        .filter(|m| {
            let request =
                crate::core::trust::Request::mcp(&m.name, &m.command, &m.args, "axur.toml");
            !store.is_approved(&request)
        })
        .map(|m| {
            format!(
                "{} {}
   {}",
                ui::bad("✗"),
                ui::bold(&m.name),
                format_args!("{} {}", m.command, m.args.join(" "))
            )
        })
        .collect();

    if pending.is_empty() {
        return Ok(0);
    }

    ui::warning(&format!(
        "Declared by this project, not approved on this machine
{}
{}",
        pending.join(
            "
"
        ),
        ui::dim("axur sync will ask before installing these")
    ));

    Ok(pending.len())
}
