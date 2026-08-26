//! `agentpm sync` — reconcile the working tree with `agentpm.toml`.
//!
//! Three modes, mirroring the distinction between `npm install` and `npm ci`:
//!
//! - `sync` installs from the lock when one exists, so every machine gets the
//!   same commits, and resolves and locks anything new.
//! - `sync --update` re-resolves every ref to its current commit and rewrites
//!   the lock. The only mode that moves a pinned version.
//! - `sync --check` resolves and compares, writes nothing, and exits non-zero
//!   on drift. This is the CI gate.

use anyhow::{Context, Result};
use colored::Colorize;

use crate::core::lockfile::{tree_digest, digest, LockedMcp, LockedSkill, Lockfile};
use crate::core::manifest::Manifest;
use crate::core::source::{GitHubSource, SourceClient};
use crate::installers::{get_installer, Target};
use crate::utils::paths::Scope;
use crate::utils::ui;

/// A skill's files, ready to write: `(relative path, bytes)`.
type SkillPayload = (String, Vec<(String, Vec<u8>)>);

/// Exit code returned by `--check` when the tree has drifted.
pub const DRIFT_EXIT_CODE: i32 = 2;

pub async fn execute(check: bool, update: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let manifest_path = Manifest::find(&cwd).with_context(|| {
        format!(
            "No {} found in this directory or any parent. Run `agentpm init` to create one.",
            crate::core::manifest::MANIFEST_FILE
        )
    })?;
    let project_root = manifest_path
        .parent()
        .unwrap_or(&cwd)
        .to_path_buf();

    let manifest = Manifest::load(&manifest_path)?;
    let (targets, scope) = manifest.targets.resolve()?;

    ui::print_header(if check { "Checking agent setup" } else { "Syncing agent setup" });
    println!(
        "  {} {}",
        "manifest".dimmed(),
        manifest_path.display().to_string().cyan()
    );
    println!(
        "  {} {}  {} {} scope\n",
        "targets".dimmed(),
        targets
            .iter()
            .map(|t| t.display_name())
            .collect::<Vec<_>>()
            .join(", "),
        "·".dimmed(),
        scope.display_name()
    );

    if manifest.is_empty() {
        ui::print_warning("Nothing declared yet.");
        println!(
            "  {} Add skills under [skills] in {}.",
            "→".cyan(),
            crate::core::manifest::MANIFEST_FILE
        );
        return Ok(());
    }

    let lock_path = Lockfile::path_for(&project_root);
    let existing = if lock_path.exists() {
        Some(Lockfile::load(&lock_path)?)
    } else {
        None
    };

    if check && existing.is_none() {
        anyhow::bail!(
            "No {} to check against. Run `agentpm sync` and commit the result.",
            crate::core::lockfile::LOCKFILE
        );
    }

    let client = SourceClient::new()?;
    let mut resolved_lock = Lockfile::new();
    let mut installed: Vec<SkillPayload> = Vec::new();

    // ---- skills -----------------------------------------------------------
    for (name, spec) in &manifest.skills {
        let source = GitHubSource::parse(spec.source())?;
        let path = spec.path(name);

        // Reuse the locked commit unless asked to update. This is what makes a
        // second machine reproduce the first rather than pick up a new HEAD.
        let pinned = if update {
            None
        } else {
            existing
                .as_ref()
                .and_then(|l| l.skill(name))
                .filter(|l| l.source == source.slug() && l.path == path.trim_matches('/'))
                .map(|l| l.resolved.clone())
        };

        let spinner = ui::create_spinner(&format!("Resolving {}...", name));
        let fetched = client
            .fetch_skill(&source, path, spec.rev(), pinned.as_deref())
            .await
            .with_context(|| format!("Failed to resolve skill '{}'", name))?;
        spinner.finish_and_clear();

        let files: Vec<_> = fetched.files.iter().map(|f| f.locked()).collect();
        let locked = LockedSkill {
            name: name.clone(),
            source: fetched.source.clone(),
            source_url: fetched.source_url.clone(),
            path: fetched.path.clone(),
            resolved: fetched.resolved.clone(),
            digest: tree_digest(&files),
            targets: targets.iter().map(|t| t.slug().to_string()).collect(),
            files,
        };

        // A pinned commit is immutable, so identical bytes are guaranteed. A
        // mismatch means the lock and the source disagree about history.
        if let (Some(prev), Some(_)) = (existing.as_ref().and_then(|l| l.skill(name)), &pinned) {
            if prev.digest != locked.digest {
                anyhow::bail!(
                    "Integrity check failed for '{}'.\n  \
                     {} is pinned to {} but its contents no longer match the lockfile.\n  \
                     The source may have been rewritten. Inspect it, then run \
                     `agentpm sync --update` if the change is expected.",
                    name,
                    fetched.source,
                    &fetched.resolved[..12.min(fetched.resolved.len())]
                );
            }
        }

        let short = &locked.resolved[..7.min(locked.resolved.len())];
        println!(
            "  {} {:<28} {} {} file(s)",
            "✓".green(),
            name.bold(),
            short.dimmed(),
            locked.files.len()
        );

        installed.push((
            name.clone(),
            fetched
                .files
                .into_iter()
                .map(|f| (f.path, f.bytes))
                .collect(),
        ));
        resolved_lock.skills.push(locked);
    }

    // ---- MCP servers ------------------------------------------------------
    let tools: Vec<_> = manifest
        .mcp
        .iter()
        .map(|(name, spec)| spec.to_tool(name))
        .collect();

    for tool in &tools {
        // Environment values are excluded from the digest: they hold API keys,
        // and the lockfile is committed.
        let material = format!("{}\u{0}{}\u{0}{}", tool.name, tool.command, tool.args.join("\u{0}"));
        resolved_lock.mcp.push(LockedMcp {
            name: tool.name.clone(),
            command: tool.command.clone(),
            args: tool.args.clone(),
            digest: digest(material.as_bytes()),
        });
    }

    // ---- check mode -------------------------------------------------------
    if check {
        let previous = existing.expect("checked above");
        let drift = previous.diff(&resolved_lock);

        println!();
        if drift.is_empty() {
            ui::print_success("In sync. No drift.");
            return Ok(());
        }

        ui::print_error(&format!("{} change(s) not reflected in the lockfile:", drift.len()));
        println!();
        for item in &drift {
            println!("    {}", item);
        }
        println!();
        println!(
            "  {} Run {} and commit {}.",
            "→".cyan(),
            "agentpm sync".cyan().bold(),
            crate::core::lockfile::LOCKFILE
        );
        std::process::exit(DRIFT_EXIT_CODE);
    }

    // ---- install ----------------------------------------------------------
    println!();
    for target in &targets {
        let installer = get_installer(*target, scope);
        let caps = installer.capabilities();

        println!(
            "  {} {} → {}",
            "▸".cyan().bold(),
            target.display_name().bold(),
            installer.location().dimmed()
        );

        for (name, files) in &installed {
            installer
                .install_files(name, files)
                .with_context(|| format!("Failed to install '{}' for {}", name, target.display_name()))?;
        }
        println!("    {} {} skill(s)", "✓".green(), installed.len());

        if !tools.is_empty() {
            if caps.mcp {
                installer.install_mcp(&tools)?;
                println!("    {} {} MCP server(s)", "✓".green(), tools.len());
            } else {
                println!(
                    "    {} {} MCP server(s) skipped — unsupported by {}",
                    "·".dimmed(),
                    tools.len(),
                    target.display_name()
                );
            }
        }
    }

    // ---- write the lock ---------------------------------------------------
    let changed = existing
        .as_ref()
        .map(|prev| !prev.diff(&resolved_lock).is_empty())
        .unwrap_or(true);

    resolved_lock.save(&lock_path)?;

    println!();
    ui::print_success("In sync.");
    if changed {
        println!(
            "  {} {} updated — commit it so your team resolves the same commits.",
            "→".cyan(),
            crate::core::lockfile::LOCKFILE.cyan().bold()
        );
    }

    Ok(())
}

/// Targets a manifest declares, for `agentpm init` reporting.
pub fn declared_targets(manifest: &Manifest) -> Vec<Target> {
    manifest
        .targets
        .resolve()
        .map(|(t, _)| t)
        .unwrap_or_else(|_| Target::all().to_vec())
}

/// Scope a manifest declares.
pub fn declared_scope(manifest: &Manifest) -> Scope {
    manifest
        .targets
        .resolve()
        .map(|(_, s)| s)
        .unwrap_or(Scope::Project)
}
