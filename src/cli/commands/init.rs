//! `axur init` — detect installed agents and scaffold the project manifest.
//!
//! Interactive on a terminal: detected agents are pre-selected, the scope is a
//! choice, and the manifest is written from what you pick. Piped or in CI it
//! takes the detected agents at project scope without prompting, so the command
//! stays scriptable.

use anyhow::Result;

use crate::core::config::Config;
use crate::core::manifest::{Manifest, Targets, MANIFEST_FILE};
use crate::installers::Target;
use crate::utils::paths::{self, Scope};
use crate::utils::ui;

fn detect(target: Target) -> bool {
    match target {
        Target::Claude => paths::claude_detected(),
        Target::Codex => paths::codex_detected(),
    }
}

fn user_skills_dir(target: Target) -> String {
    let path = match target {
        Target::Claude => paths::claude_skills_dir(Scope::User),
        Target::Codex => paths::codex_skills_dir(Scope::User),
    };
    path.map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unresolved".to_string())
}

pub async fn execute() -> Result<()> {
    ui::intro("axur init");

    let detected: Vec<Target> = Target::all().into_iter().filter(|t| detect(*t)).collect();

    let report = Target::all()
        .iter()
        .map(|t| {
            if detected.contains(t) {
                format!(
                    "{} {}  {}",
                    ui::good("✓"),
                    ui::bold(t.display_name()),
                    ui::dim(&user_skills_dir(*t))
                )
            } else {
                ui::dim(&format!("· {}  not found", t.display_name()))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    ui::step(&format!("Detected agents\n{}", report));

    if detected.is_empty() {
        ui::warning("No supported agent found on this machine");
        ui::note(
            "axur targets Claude Code and Codex",
            "Install one, then run axur init again.\nYou can still create a manifest now and sync later.",
        );
    }

    // Choose targets and scope. Non-interactive runs take the detected set.
    let (chosen, scope) = if ui::is_rich() {
        let selected = ui::select_targets(&detected)?;
        if selected.is_empty() {
            ui::outro_cancel("No targets selected — nothing written");
            return Ok(());
        }
        let scope = ui::select_scope()?;
        (selected, scope)
    } else {
        let fallback = if detected.is_empty() {
            Target::all().to_vec()
        } else {
            detected.clone()
        };
        (fallback, Scope::Project)
    };

    // The project manifest is the file a team commits, so never overwrite one.
    let manifest_path = paths::project_root()?.join(MANIFEST_FILE);
    if manifest_path.exists() {
        ui::info(&format!(
            "{} already exists — left untouched",
            ui::accent(MANIFEST_FILE)
        ));
    } else {
        let manifest = Manifest {
            targets: Targets {
                agents: chosen.iter().map(|t| t.slug().to_string()).collect(),
                scope: scope.display_name().to_string(),
            },
            ..Manifest::starter(&chosen)
        };
        manifest.save(&manifest_path)?;
        ui::success(&format!("Created {}", ui::accent(MANIFEST_FILE)));
    }

    // Preserve an existing configuration; re-running init previously reset a
    // customized registry URL back to the default.
    let config_path = paths::axur_config_path()?;
    let mut config = Config::load_or_default()?;
    if let Some(first) = chosen.first() {
        config.default_target = first.slug().to_string();
    }
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    config.save(&config_path)?;

    ui::note(
        "Next",
        &format!(
            "1. Add skills under [skills] in {}\n\
             2. Run axur sync to install them\n\
             3. Commit {} and axur.lock so your team resolves the same commits",
            MANIFEST_FILE, MANIFEST_FILE
        ),
    );

    ui::outro(&format!(
        "Ready {} {} {} {} scope",
        ui::dim("·"),
        chosen
            .iter()
            .map(|t| t.display_name())
            .collect::<Vec<_>>()
            .join(", "),
        ui::dim("·"),
        scope.display_name()
    ));

    Ok(())
}
