//! `axur uninstall` — drop an entry from the manifest and reconcile.
//!
//! The inverse of `install`: the manifest is the record of what a project
//! needs, so removal edits it and lets `sync` prune the files. That keeps the
//! lockfile honest instead of leaving orphaned directories behind.

use anyhow::Result;

use crate::core::manifest::{Manifest, MANIFEST_FILE};
use crate::utils::{paths, ui};

pub async fn execute(name: &str) -> Result<()> {
    ui::intro(&format!("axur uninstall {}", name));

    let project_root = paths::project_root()?;
    let Some(manifest_path) = Manifest::find(&project_root) else {
        ui::outro_cancel(&format!("No {} in this project", MANIFEST_FILE));
        return Ok(());
    };

    let mut manifest = Manifest::load(&manifest_path)?;
    let removed_skill = manifest.skills.remove(name).is_some();
    let removed_bundle = manifest.bundles.remove(name).is_some();

    if !removed_skill && !removed_bundle {
        ui::warning(&format!("'{}' is not declared in {}", name, MANIFEST_FILE));
        let declared: Vec<&String> = manifest
            .skills
            .keys()
            .chain(manifest.bundles.keys())
            .collect();
        if !declared.is_empty() {
            ui::note(
                "Declared here",
                &declared
                    .iter()
                    .map(|d| d.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        ui::outro_cancel("Nothing removed");
        return Ok(());
    }

    manifest.save(&manifest_path)?;
    ui::success(&format!(
        "Removed {} from {}",
        ui::bold(name),
        ui::accent(MANIFEST_FILE)
    ));
    // sync prunes anything the manifest no longer declares and closes the rail.
    super::sync::reconcile(false).await
}
