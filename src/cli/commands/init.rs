//! `apm init` Command
//!
//! Detects installed editors and creates APM configuration.

use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;

use crate::core::config::ApmConfig;
use crate::utils::paths;
use crate::utils::ui;

/// Execute the init command
pub async fn execute() -> Result<()> {
    ui::print_header("APM Initialization");

    // Detect installed editors
    println!("{} Detecting installed editors...\n", "→".cyan());

    let claude_installed = detect_claude();
    let cursor_installed = detect_cursor();
    let vscode_installed = detect_vscode();

    // Print detection results
    print_editor_status("Claude Code", claude_installed, paths::claude_config_dir());
    print_editor_status("Cursor", cursor_installed, paths::cursor_config_dir());
    print_editor_status("VS Code", vscode_installed, None);

    println!();

    // Determine default target
    let default_target = if claude_installed {
        "claude"
    } else if cursor_installed {
        "cursor"
    } else {
        "claude" // Default to claude even if not detected
    };

    // Create config
    let config = ApmConfig::new(default_target.to_string());
    let config_path = paths::apm_config_path()?;

    // Ensure config directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write config
    config.save(&config_path)?;

    println!(
        "{} Created configuration at {}",
        "✓".green().bold(),
        config_path.display().to_string().cyan()
    );
    println!(
        "{} Default target set to: {}",
        "✓".green().bold(),
        default_target.cyan().bold()
    );

    println!();
    ui::print_success("APM initialized successfully!");
    println!(
        "\n  Run {} to see available agents.",
        "apm list".cyan().bold()
    );

    Ok(())
}

fn detect_claude() -> bool {
    paths::claude_config_dir()
        .map(|path| path.exists())
        .unwrap_or(false)
}

fn detect_cursor() -> bool {
    paths::cursor_config_dir()
        .map(|path| path.exists())
        .unwrap_or_else(|| {
            // Also check for .cursor in current directory
            PathBuf::from(".cursor").exists()
        })
}

fn detect_vscode() -> bool {
    // Check if code command exists
    which::which("code").is_ok()
}

fn print_editor_status(name: &str, installed: bool, path: Option<PathBuf>) {
    let status = if installed {
        "✓".green().bold()
    } else {
        "✗".red().bold()
    };

    let status_text = if installed {
        "detected".green()
    } else {
        "not found".dimmed()
    };

    print!("  {} {} - {}", status, name.bold(), status_text);

    if installed {
        if let Some(p) = path {
            print!(" ({})", p.display().to_string().dimmed());
        }
    }

    println!();
}

