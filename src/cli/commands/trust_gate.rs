//! The consent step before anything that will execute is installed.
//!
//! `sync` collects every MCP server and hook script it is about to write, drops
//! the ones already approved, and shows the rest. Nothing runs until a person
//! has seen the command line and agreed to it.
//!
//! The gate is silent when nothing has changed, which is what makes it
//! tolerable: approval is keyed by the digest of what actually runs, so a
//! routine sync of already-trusted content never interrupts.

use anyhow::Result;

use crate::core::trust::{Grant, Request, TrustStore};
use crate::utils::ui;

/// Outcome of showing the gate.
pub enum Decision {
    /// Nothing needed approval, or the user approved everything.
    Proceed,
    /// The user declined. Nothing should be installed.
    Declined,
}

/// Render one pending request as a review line.
fn render(request: &Request) -> String {
    format!(
        "{} {}\n   {}\n   {}",
        ui::bad("✗"),
        ui::bold(&request.label),
        request.detail,
        ui::dim(&format!("from {}", request.origin))
    )
}

/// Show anything not yet approved and record the answer.
///
/// `assume_yes` skips the prompt — required for non-interactive runs, which
/// must opt in explicitly rather than being approved by default.
pub fn review(requests: &[Request], assume_yes: bool) -> Result<Decision> {
    let mut store = TrustStore::load()?;
    let pending = store.outstanding(requests);

    if pending.is_empty() {
        return Ok(Decision::Proceed);
    }

    for kind in [Grant::Mcp, Grant::Hook] {
        let group: Vec<&Request> = pending.iter().filter(|r| r.kind == kind).collect();
        if group.is_empty() {
            continue;
        }
        ui::warning(&format!(
            "{} — each {}\n{}",
            ui::bold(&format!(
                "{} {}{}",
                group.len(),
                kind.noun(),
                if group.len() == 1 { "" } else { "s" }
            )),
            kind.consequence(),
            group
                .iter()
                .map(|r| render(r))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    if !assume_yes {
        if !ui::is_rich() {
            // A pipeline cannot answer, and defaulting to yes here would make
            // the gate decorative.
            anyhow::bail!(
                "{} item(s) need approval and this is not an interactive terminal.\n\
                 Review them, then re-run with --yes to record the approval.",
                pending.len()
            );
        }

        if !ui::confirm("Allow these to run on your machine?")? {
            return Ok(Decision::Declined);
        }
    }

    for request in &pending {
        store.approve(request);
    }
    store.save()?;

    ui::success(&format!(
        "Approved {} item(s) {}",
        pending.len(),
        ui::dim("· recorded in ~/.axur/trust.toml")
    ));

    Ok(Decision::Proceed)
}
