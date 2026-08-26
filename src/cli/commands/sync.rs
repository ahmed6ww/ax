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
use futures::stream::{StreamExt, TryStreamExt};

use crate::core::bundle::BundleManifest;
use crate::core::lockfile::{digest, tree_digest, LockedBundle, LockedMcp, LockedSkill, Lockfile};
use crate::core::manifest::{Manifest, SkillSpec};
use crate::core::source::{GitHubSource, ResolvedSkill, SourceClient};
use crate::installers::{get_installer, SettingsContribution, Target};
use crate::utils::ui;

/// Sources resolved at once. Enough to hide latency, low enough to stay well
/// inside GitHub's rate limit on an unauthenticated run.
const RESOLVE_CONCURRENCY: usize = 4;

/// Exit code returned by `--check` when the tree has drifted.
pub const DRIFT_EXIT_CODE: i32 = 2;

/// A skill's files, ready to write: `(relative path, bytes)`.
type SkillPayload = (String, Vec<(String, Vec<u8>)>);

/// A resolved bundle: its manifest and every file it ships.
struct BundlePayload {
    manifest: BundleManifest,
    files: Vec<(String, Vec<u8>)>,
}

impl BundlePayload {
    fn file(&self, path: &str) -> Option<&Vec<u8>> {
        self.files.iter().find(|(p, _)| p == path).map(|(_, b)| b)
    }

    /// Files under a declared skill directory, rebased to that directory.
    fn skill_files(&self, dir: &str) -> Vec<(String, Vec<u8>)> {
        let prefix = format!("{}/", dir);
        self.files
            .iter()
            .filter_map(|(p, b)| {
                p.strip_prefix(&prefix)
                    .map(|rest| (rest.to_string(), b.clone()))
            })
            .collect()
    }
}

/// The final path component, without extension.
fn stem(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .split('.')
        .next()
        .unwrap_or(path)
}

fn plural(n: usize, word: &str) -> String {
    format!("{} {}{}", n, word, if n == 1 { "" } else { "s" })
}

/// One source to resolve, with everything it needs owned so the fetch can run
/// concurrently with the others.
struct Plan {
    name: String,
    source: GitHubSource,
    path: String,
    rev: Option<String>,
    pinned: Option<String>,
}

impl Plan {
    fn build(
        name: &str,
        spec: &SkillSpec,
        locked: Option<(&str, &str, &str)>,
        update: bool,
    ) -> Result<Self> {
        let source = GitHubSource::parse(spec.source())?;
        let path = spec.path(name).trim_matches('/').to_string();

        // Reuse the locked commit unless asked to update. This is what makes a
        // second machine reproduce the first rather than pick up a new HEAD.
        let pinned = match locked {
            Some((locked_source, locked_path, resolved))
                if !update && locked_source == source.slug() && locked_path == path =>
            {
                Some(resolved.to_string())
            }
            _ => None,
        };

        Ok(Self {
            name: name.to_string(),
            source,
            path,
            rev: spec.rev().map(str::to_string),
            pinned,
        })
    }
}

pub async fn execute(check: bool, update: bool) -> Result<()> {
    run(check, update, true).await
}

/// Reconcile as a continuation of another command, without opening a second
/// rail. `install` and `uninstall` announce themselves and then hand over.
pub(crate) async fn reconcile() -> Result<()> {
    run(false, false, false).await
}

async fn run(check: bool, update: bool, announce: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let manifest_path = Manifest::find(&cwd).with_context(|| {
        format!(
            "No {} found in this directory or any parent. Run `agentpm init` to create one.",
            crate::core::manifest::MANIFEST_FILE
        )
    })?;
    let project_root = manifest_path.parent().unwrap_or(&cwd).to_path_buf();

    let manifest = Manifest::load(&manifest_path)?;
    let (targets, scope) = manifest.targets.resolve()?;

    if announce {
        ui::intro(if check {
            "agentpm check"
        } else {
            "agentpm sync"
        });
    }

    ui::step(&format!(
        "{}\n{}  {}  {} scope",
        ui::dim(
            &manifest_path
                .strip_prefix(&project_root)
                .unwrap_or(&manifest_path)
                .display()
                .to_string()
        ),
        targets
            .iter()
            .map(|t| ui::bold(t.display_name()))
            .collect::<Vec<_>>()
            .join(", "),
        ui::dim("·"),
        scope.display_name()
    ));

    let lock_path = Lockfile::path_for(&project_root);
    let existing = if lock_path.exists() {
        Some(Lockfile::load(&lock_path)?)
    } else {
        None
    };

    if manifest.is_empty() {
        // An empty manifest still has work to do when the lockfile records
        // something installed: removing the last entry has to take its files
        // with it. Returning early here left them on disk.
        let names: Vec<String> = existing
            .as_ref()
            .map(|l| {
                l.skills
                    .iter()
                    .map(|s| s.name.clone())
                    .chain(l.bundles.iter().map(|b| b.name.clone()))
                    .collect()
            })
            .unwrap_or_default();

        if !names.is_empty() && !check {
            for target in &targets {
                let installer = get_installer(*target, scope);
                let mut pruned = 0usize;
                for name in &names {
                    if installer.remove_skill(name)? {
                        pruned += 1;
                    }
                }
                // A bundle's settings contributions have to come back out too.
                let previous = previous_contribution(existing.as_ref(), *target);
                if !previous.is_empty() {
                    installer.apply_settings(&SettingsContribution::default(), &previous)?;
                }
                ui::success(&format!(
                    "{}   {}\n{} {} removed",
                    ui::bold(target.display_name()),
                    ui::dim(&installer.location()),
                    ui::dim("-"),
                    plural(pruned, "skill")
                ));
            }

            Lockfile::new().save(&lock_path)?;
            ui::outro("Nothing declared - everything removed");
            return Ok(());
        }

        ui::warning("Nothing declared yet");
        ui::note(
            "Add a skill",
            &format!(
                "agentpm install vercel-labs/skills#skills/find-skills\n\nor edit {} by hand",
                crate::core::manifest::MANIFEST_FILE
            ),
        );
        ui::outro("Nothing to do");
        return Ok(());
    }

    if check && existing.is_none() {
        ui::outro_cancel("Nothing to check against");
        anyhow::bail!(
            "No {} in this project. Run `agentpm sync` and commit the result.",
            crate::core::lockfile::LOCKFILE
        );
    }

    let client = SourceClient::new()?;
    let mut resolved_lock = Lockfile::new();

    // ---- resolve, concurrently --------------------------------------------
    let skill_plans: Vec<Plan> = manifest
        .skills
        .iter()
        .map(|(name, spec)| {
            let locked = existing
                .as_ref()
                .and_then(|l| l.skill(name))
                .map(|l| (l.source.as_str(), l.path.as_str(), l.resolved.as_str()));
            Plan::build(name, spec, locked, update)
        })
        .collect::<Result<_>>()?;

    let bundle_plans: Vec<Plan> = manifest
        .bundles
        .iter()
        .map(|(name, spec)| {
            let locked = existing
                .as_ref()
                .and_then(|l| l.bundle(name))
                .map(|l| (l.source.as_str(), l.path.as_str(), l.resolved.as_str()));
            Plan::build(name, spec, locked, update)
        })
        .collect::<Result<_>>()?;

    let total = skill_plans.len() + bundle_plans.len();
    let multi = ui::MultiProgress::start("Resolving…");

    // Progress bars are transient: a stopped bar may be cleared from the
    // screen. Every resolved source also records a line here so the listing
    // survives as part of the run's permanent output.
    let resolved_lines = std::sync::Mutex::new(Vec::<String>::new());

    let skill_results: Vec<(Plan, ResolvedSkill)> =
        futures::stream::iter(skill_plans.into_iter().map(|plan| {
            let bar = multi.add(&plan.name);
            let client = &client;
            let lines = &resolved_lines;
            async move {
                let result = client
                    .fetch_skill(
                        &plan.source,
                        &plan.path,
                        plan.rev.as_deref(),
                        plan.pinned.as_deref(),
                    )
                    .await;
                match &result {
                    Ok(fetched) => {
                        let line = format!(
                            "{} {:<26} {}  {}",
                            ui::good("✓"),
                            plan.name,
                            ui::dim(&ui::short_sha(&fetched.resolved)),
                            ui::dim(&plural(fetched.files.len(), "file"))
                        );
                        bar.stop(&line);
                        lines.lock().unwrap().push(line);
                    }
                    Err(_) => bar.stop(&ui::bad(&format!("{}  unresolved", plan.name))),
                }
                let name = plan.name.clone();
                result
                    .map(|fetched| (plan, fetched))
                    .with_context(|| format!("Failed to resolve skill '{}'", name))
            }
        }))
        .buffer_unordered(RESOLVE_CONCURRENCY)
        .try_collect()
        .await?;

    let bundle_results: Vec<(Plan, BundleManifest, ResolvedSkill)> =
        futures::stream::iter(bundle_plans.into_iter().map(|plan| {
            let bar = multi.add(&plan.name);
            let client = &client;
            let lines = &resolved_lines;
            async move {
                let result = client
                    .fetch_bundle(
                        &plan.source,
                        &plan.path,
                        plan.rev.as_deref(),
                        plan.pinned.as_deref(),
                    )
                    .await;
                match &result {
                    Ok((bundle, fetched)) => {
                        let line = format!(
                            "{} {:<26} {}  {}",
                            ui::good("✓"),
                            plan.name,
                            ui::dim(&ui::short_sha(&fetched.resolved)),
                            ui::dim(&format!(
                                "bundle · {}",
                                bundle
                                    .summary()
                                    .iter()
                                    .map(|(label, n)| plural(*n, label))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ))
                        );
                        bar.stop(&line);
                        lines.lock().unwrap().push(line);
                    }
                    Err(_) => bar.stop(&ui::bad(&format!("{}  unresolved", plan.name))),
                }
                let name = plan.name.clone();
                result
                    .map(|(bundle, fetched)| (plan, bundle, fetched))
                    .with_context(|| format!("Failed to resolve bundle '{}'", name))
            }
        }))
        .buffer_unordered(RESOLVE_CONCURRENCY)
        .try_collect()
        .await?;

    multi.stop();

    let mut lines = resolved_lines.into_inner().unwrap();
    lines.sort();
    ui::step(&format!(
        "Resolved {}
{}",
        plural(total, "source"),
        lines.join(
            "
"
        )
    ));

    // ---- lock entries ------------------------------------------------------
    let mut installed: Vec<SkillPayload> = Vec::new();

    for (plan, fetched) in skill_results {
        let files: Vec<_> = fetched.files.iter().map(|f| f.locked()).collect();
        let locked = LockedSkill {
            name: plan.name.clone(),
            source: fetched.source.clone(),
            source_url: fetched.source_url.clone(),
            path: fetched.path.clone(),
            resolved: fetched.resolved.clone(),
            digest: tree_digest(&files),
            targets: targets.iter().map(|t| t.slug().to_string()).collect(),
            files,
        };

        verify_pin(
            existing
                .as_ref()
                .and_then(|l| l.skill(&plan.name))
                .map(|l| l.digest.as_str()),
            &plan,
            &locked.digest,
            &fetched.resolved,
        )?;

        installed.push((
            plan.name.clone(),
            fetched
                .files
                .into_iter()
                .map(|f| (f.path, f.bytes))
                .collect(),
        ));
        resolved_lock.skills.push(locked);
    }

    let mut bundles: Vec<(String, BundlePayload)> = Vec::new();

    for (plan, bundle_manifest, fetched) in bundle_results {
        let files: Vec<_> = fetched.files.iter().map(|f| f.locked()).collect();
        let locked = LockedBundle {
            name: plan.name.clone(),
            source: fetched.source.clone(),
            source_url: fetched.source_url.clone(),
            path: fetched.path.clone(),
            resolved: fetched.resolved.clone(),
            digest: tree_digest(&files),
            targets: targets.iter().map(|t| t.slug().to_string()).collect(),
            permissions_allow: bundle_manifest.permissions.allow.clone(),
            permissions_deny: bundle_manifest.permissions.deny.clone(),
            permissions_ask: bundle_manifest.permissions.ask.clone(),
            hook_commands: Vec::new(),
            files,
        };

        verify_pin(
            existing
                .as_ref()
                .and_then(|l| l.bundle(&plan.name))
                .map(|l| l.digest.as_str()),
            &plan,
            &locked.digest,
            &fetched.resolved,
        )?;

        bundles.push((
            plan.name.clone(),
            BundlePayload {
                manifest: bundle_manifest,
                files: fetched
                    .files
                    .into_iter()
                    .map(|f| (f.path, f.bytes))
                    .collect(),
            },
        ));
        resolved_lock.bundles.push(locked);
    }

    // ---- MCP servers ------------------------------------------------------
    let mut tools: Vec<_> = manifest
        .mcp
        .iter()
        .map(|(name, spec)| spec.to_tool(name))
        .collect();

    // Bundles bring their own servers. A project-level [mcp] entry of the same
    // name wins, so a team can override what a bundle ships.
    for (_, payload) in &bundles {
        for (name, spec) in &payload.manifest.mcp {
            if !tools.iter().any(|t| &t.name == name) {
                tools.push(spec.to_tool(name));
            }
        }
    }

    for tool in &tools {
        // Environment values are excluded from the digest: they hold API keys,
        // and the lockfile is committed.
        let material = format!(
            "{}\u{0}{}\u{0}{}",
            tool.name,
            tool.command,
            tool.args.join("\u{0}")
        );
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

        if drift.is_empty() {
            ui::success("No drift — every machine resolves these commits");
            ui::outro("In sync");
            return Ok(());
        }

        ui::error(&format!(
            "{} not reflected in {}\n{}",
            plural(drift.len(), "change"),
            crate::core::lockfile::LOCKFILE,
            drift
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        ));
        ui::outro_cancel(&format!(
            "Out of sync — run `agentpm sync` and commit {}",
            crate::core::lockfile::LOCKFILE
        ));
        std::process::exit(DRIFT_EXIT_CODE);
    }

    // ---- prune what the manifest no longer declares -----------------------
    //
    // A skill removed from agentpm.toml has to leave the disk too, or the
    // agent keeps loading something the project dropped.
    let stale: Vec<String> = existing
        .as_ref()
        .map(|prev| {
            let mut names: Vec<String> = prev
                .skills
                .iter()
                .filter(|s| resolved_lock.skill(&s.name).is_none())
                .map(|s| s.name.clone())
                .collect();
            for bundle in &prev.bundles {
                if resolved_lock.bundle(&bundle.name).is_none() {
                    names.push(bundle.name.clone());
                }
            }
            names
        })
        .unwrap_or_default();

    // ---- install ----------------------------------------------------------
    for target in &targets {
        let installer = get_installer(*target, scope);
        let caps = installer.capabilities();
        let mut lines: Vec<String> = Vec::new();
        let mut contribution = SettingsContribution::default();

        for (name, files) in &installed {
            installer.install_files(name, files).with_context(|| {
                format!("Failed to install '{}' for {}", name, target.display_name())
            })?;
        }
        if !installed.is_empty() {
            lines.push(format!(
                "{} {}",
                ui::good("✓"),
                plural(installed.len(), "skill")
            ));
        }

        for (bundle_name, payload) in &bundles {
            let bundle = &payload.manifest;

            for dir in &bundle.skills {
                installer
                    .install_files(stem(dir), &payload.skill_files(dir))
                    .with_context(|| format!("Failed to install skill '{}'", dir))?;
            }
            if !bundle.skills.is_empty() {
                lines.push(format!(
                    "{} {} {}",
                    ui::good("✓"),
                    plural(bundle.skills.len(), "skill"),
                    ui::dim(&format!("from {}", bundle_name))
                ));
            }

            if caps.subagents && !bundle.agents.is_empty() {
                for path in &bundle.agents {
                    let bytes = payload
                        .file(path)
                        .with_context(|| format!("Bundle '{}' is missing {}", bundle_name, path))?;
                    installer.install_subagent(stem(path), bytes)?;
                }
                lines.push(format!(
                    "{} {}",
                    ui::good("✓"),
                    plural(bundle.agents.len(), "subagent")
                ));
            }

            if caps.commands && !bundle.commands.is_empty() {
                for path in &bundle.commands {
                    let bytes = payload
                        .file(path)
                        .with_context(|| format!("Bundle '{}' is missing {}", bundle_name, path))?;
                    installer.install_command(stem(path), bytes)?;
                }
                lines.push(format!(
                    "{} {}",
                    ui::good("✓"),
                    plural(bundle.commands.len(), "command")
                ));
            }

            if caps.hooks && !bundle.hooks.is_empty() {
                let scripts: Vec<(String, Vec<u8>)> = bundle
                    .scripts
                    .iter()
                    .filter_map(|p| payload.file(p).map(|b| (p.clone(), b.clone())))
                    .collect();

                if let Some(dir) = installer.stage_bundle_files(bundle_name, &scripts)? {
                    for hook in &bundle.hooks {
                        // An absolute path keeps the hook working regardless of
                        // the directory Claude Code is started from.
                        let resolved = dir.join(&hook.command).display().to_string();
                        contribution.hooks.push((hook.clone(), resolved));
                    }
                }
            }

            if caps.permissions {
                contribution
                    .permissions
                    .allow
                    .extend(bundle.permissions.allow.clone());
                contribution
                    .permissions
                    .deny
                    .extend(bundle.permissions.deny.clone());
                contribution
                    .permissions
                    .ask
                    .extend(bundle.permissions.ask.clone());
            }
        }

        if !tools.is_empty() && caps.mcp {
            installer.install_mcp(&tools)?;
            lines.push(format!(
                "{} {}",
                ui::good("✓"),
                plural(tools.len(), "MCP server")
            ));
        }

        // Remove what the previous sync contributed, then apply the current
        // set, so a shared settings file never accumulates duplicates.
        let previous = previous_contribution(existing.as_ref(), *target);
        if !contribution.is_empty() || !previous.is_empty() {
            installer.apply_settings(&contribution, &previous)?;
            if !contribution.hooks.is_empty() {
                lines.push(format!(
                    "{} {}",
                    ui::good("✓"),
                    plural(contribution.hooks.len(), "hook")
                ));
            }
            let rules = contribution.permissions.allow.len()
                + contribution.permissions.deny.len()
                + contribution.permissions.ask.len();
            if rules > 0 {
                lines.push(format!(
                    "{} {}",
                    ui::good("✓"),
                    plural(rules, "permission rule")
                ));
            }
        }

        let mut pruned = 0usize;
        for name in &stale {
            if installer.remove_skill(name)? {
                pruned += 1;
            }
        }
        if pruned > 0 {
            lines.push(format!(
                "{} {} removed",
                ui::dim("−"),
                plural(pruned, "skill")
            ));
        }

        // Anything this target cannot take is named, never silently dropped.
        let skipped: Vec<String> = bundles
            .iter()
            .flat_map(|(_, p)| caps.unsupported(&p.manifest))
            .collect();
        if !skipped.is_empty() {
            lines.push(ui::dim(&format!(
                "·  skipped, unsupported: {}",
                skipped.join(", ")
            )));
        }

        ui::success(&format!(
            "{}   {}\n{}",
            ui::bold(target.display_name()),
            ui::dim(&installer.location()),
            lines.join("\n")
        ));

        for (bundle_name, _) in &bundles {
            if let Some(locked) = resolved_lock
                .bundles
                .iter_mut()
                .find(|b| &b.name == bundle_name)
            {
                for (_, command) in &contribution.hooks {
                    if !locked.hook_commands.contains(command) {
                        locked.hook_commands.push(command.clone());
                    }
                }
            }
        }
    }

    // ---- write the lock ---------------------------------------------------
    let changed = existing
        .as_ref()
        .map(|prev| !prev.diff(&resolved_lock).is_empty())
        .unwrap_or(true);

    resolved_lock.save(&lock_path)?;

    ui::outro(&if changed {
        format!(
            "In sync {} {} updated — commit it",
            ui::dim("·"),
            ui::accent(crate::core::lockfile::LOCKFILE)
        )
    } else {
        format!("In sync {} no changes", ui::dim("·"))
    });

    Ok(())
}

/// A pinned commit is immutable, so identical bytes are guaranteed. A mismatch
/// means the lock and the source disagree about history.
fn verify_pin(
    previous_digest: Option<&str>,
    plan: &Plan,
    current_digest: &str,
    resolved: &str,
) -> Result<()> {
    if plan.pinned.is_none() {
        return Ok(());
    }
    let Some(previous) = previous_digest else {
        return Ok(());
    };
    if previous == current_digest {
        return Ok(());
    }

    ui::outro_cancel("Integrity check failed");
    anyhow::bail!(
        "'{}' is pinned to {} in {}, but its contents no longer match.\n\
         The source may have been rewritten. Inspect it, then run \
         `agentpm sync --update` if the change is expected.",
        plan.name,
        &resolved[..12.min(resolved.len())],
        crate::core::lockfile::LOCKFILE
    )
}

/// Rebuild what a previous sync wrote into a target's settings.
///
/// Hooks are matched by their resolved command path, which is all that removal
/// needs; the event and matcher are irrelevant to it.
fn previous_contribution(lock: Option<&Lockfile>, target: Target) -> SettingsContribution {
    use crate::core::bundle::Hook;

    let mut contribution = SettingsContribution::default();
    let Some(lock) = lock else {
        return contribution;
    };

    for bundle in &lock.bundles {
        if !bundle.targets.iter().any(|t| t == target.slug()) {
            continue;
        }
        contribution
            .permissions
            .allow
            .extend(bundle.permissions_allow.clone());
        contribution
            .permissions
            .deny
            .extend(bundle.permissions_deny.clone());
        contribution
            .permissions
            .ask
            .extend(bundle.permissions_ask.clone());

        for command in &bundle.hook_commands {
            contribution.hooks.push((
                Hook {
                    event: String::new(),
                    matcher: None,
                    command: command.clone(),
                    timeout: None,
                    status_message: None,
                },
                command.clone(),
            ));
        }
    }

    contribution
}
