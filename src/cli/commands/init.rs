//! `agentpm init` — detect installed agents and write agentpm configuration.

use anyhow::Result;
use colored::Colorize;

use crate::core::config::Config;
use crate::core::manifest::{Manifest, MANIFEST_FILE};
use crate::installers::Target;
use crate::utils::paths::{self, Scope};
use crate::utils::ui;

pub async fn execute() -> Result<()> {
    ui::print_header("agentpm Initialization");

    println!("{} Detecting agents...\n", "→".cyan());

    let mut detected: Vec<Target> = Vec::new();

    for target in Target::all() {
        let (found, location) = match target {
            Target::Claude => (
                paths::claude_detected(),
                paths::claude_skills_dir(Scope::User).ok(),
            ),
            Target::Codex => (
                paths::codex_detected(),
                paths::codex_skills_dir(Scope::User).ok(),
            ),
        };

        if found {
            detected.push(target);
        }

        let mark = if found {
            "✓".green().bold()
        } else {
            "✗".red().bold()
        };
        let status = if found {
            "detected".green()
        } else {
            "not found".dimmed()
        };

        print!("  {} {} - {}", mark, target.display_name().bold(), status);
        if found {
            if let Some(path) = location {
                print!(" ({})", path.display().to_string().dimmed());
            }
        }
        println!();
    }

    println!();

    if detected.is_empty() {
        ui::print_warning("No supported agent detected.");
        println!(
            "  {} agentpm targets Claude Code and Codex. Install one, then re-run {}.",
            "→".cyan(),
            "agentpm init".cyan().bold()
        );
    }

    // Preserve an existing configuration. Re-running init previously reset a
    // customized registry URL back to the default.
    let config_path = paths::agentpm_config_path()?;
    let existed = config_path.exists();

    let mut config = Config::load_or_default()?;
    if let Some(first) = detected.first() {
        config.default_target = first.slug().to_string();
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    config.save(&config_path)?;

    println!(
        "{} {} {}",
        "✓".green().bold(),
        if existed { "Updated" } else { "Created" },
        config_path.display().to_string().cyan()
    );
    println!(
        "{} Default target: {}",
        "✓".green().bold(),
        config.default_target.cyan().bold()
    );
    if existed {
        println!(
            "  {} Existing registry URL kept: {}",
            "·".dimmed(),
            config.registry_url.dimmed()
        );
    }

    // Scaffold the project manifest. This is the file a team commits, so init
    // must never overwrite one that already exists.
    let manifest_path = paths::project_root()?.join(MANIFEST_FILE);
    if manifest_path.exists() {
        println!(
            "{} {} already exists — left untouched",
            "·".dimmed(),
            manifest_path.display().to_string().dimmed()
        );
    } else {
        Manifest::starter(&detected).save(&manifest_path)?;
        println!(
            "{} Created {}",
            "✓".green().bold(),
            manifest_path.display().to_string().cyan()
        );
    }

    println!();
    ui::print_success("agentpm initialized.");
    println!("\n  Next:");
    println!(
        "    1. Add skills under {} in {}",
        "[skills]".bold(),
        MANIFEST_FILE.cyan()
    );
    println!("    2. Run {} to install them", "agentpm sync".cyan().bold());
    println!(
        "    3. Commit {} and {} so your team resolves the same commits",
        MANIFEST_FILE.cyan(),
        crate::core::lockfile::LOCKFILE.cyan()
    );

    Ok(())
}
