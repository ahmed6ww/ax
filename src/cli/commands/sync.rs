//! `axur sync` — reconcile the working tree with `axur.toml`.
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
use crate::core::trust::Request;
use crate::installers::{get_installer, Installer, SettingsContribution, Target};
use crate::utils::ui;

/// Sources resolved at once. Enough to hide latency, low enough to stay well
/// inside GitHub's rate limit on an unauthenticated run.
const RESOLVE_CONCURRENCY: usize = 4;

/// Exit code returned by `--check` when the tree has drifted.
pub const DRIFT_EXIT_CODE: i32 = crate::core::error::exit::DRIFT as i32;

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

pub(crate) fn plural(n: usize, word: &str) -> String {
    format!("{} {}{}", n, word, if n == 1 { "" } else { "s" })
}

/// One source to resolve, with everything it needs owned so the fetch can run
/// concurrently with the others.
///
/// `source` is `None` for a locally-authored entry: it has nothing to resolve
/// over the network, so it never enters the concurrent fetch stream below and
/// this field is never read for one.
struct Plan {
    name: String,
    source: Option<GitHubSource>,
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
            source: Some(source),
            path,
            rev: spec.rev().map(str::to_string),
            pinned,
        })
    }

    /// A local entry: no source to resolve, no ref, and never pinned — the
    /// working tree is read fresh every sync and drift is caught by digest.
    fn local(name: &str, dir: &str) -> Self {
        Self {
            name: name.to_string(),
            source: None,
            path: dir.to_string(),
            rev: None,
            pinned: None,
        }
    }
}

pub async fn execute(
    check: bool,
    update: bool,
    assume_yes: bool,
    offline: bool,
    agents: Option<String>,
) -> Result<()> {
    run(check, update, true, assume_yes, offline, agents).await
}

/// Reconcile as a continuation of another command, without opening a second
/// rail. `install` and `uninstall` announce themselves and then hand over.
pub(crate) async fn reconcile(assume_yes: bool) -> Result<()> {
    run(false, false, false, assume_yes, false, None).await
}

async fn run(
    check: bool,
    update: bool,
    announce: bool,
    assume_yes: bool,
    offline: bool,
    agents_flag: Option<String>,
) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let manifest_path = Manifest::find(&cwd).ok_or_else(|| {
        crate::core::error::Error::NotFound(format!(
            "No {} in this directory or any parent. Run `axur init` to create one.",
            crate::core::manifest::MANIFEST_FILE
        ))
    })?;
    let project_root = manifest_path.parent().unwrap_or(&cwd).to_path_buf();

    let manifest = Manifest::load(&manifest_path)?;
    let (declared_targets, scope) = manifest.targets.resolve()?;

    if announce {
        ui::intro(if check { "axur check" } else { "axur sync" });
    }

    // `--check` is the CI gate: it always verifies against everything the
    // manifest declares, never a developer's local subset, and never prompts.
    let targets = if check {
        declared_targets.clone()
    } else {
        resolve_effective_targets(&declared_targets, &project_root, agents_flag.as_deref())?
    };

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
            // Same rule as a normal sync: if the second target refuses, the
            // first one is put back.
            let journal = crate::core::tx::begin();
            let mut reports: Vec<String> = Vec::new();
            let outcome: Result<()> = (|| {
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
                    reports.push(format!(
                        "{}   {}\n{} {} removed",
                        ui::bold(target.display_name()),
                        ui::dim(&installer.location()),
                        ui::dim("-"),
                        plural(pruned, "skill")
                    ));
                }

                Lockfile::new().save(&lock_path)?;
                Ok(())
            })();
            outcome.map_err(rolled_back)?;
            journal.commit();
            for report in &reports {
                ui::success(report);
            }

            ui::outro("Nothing declared - everything removed");
            return Ok(());
        }

        ui::warning("Nothing declared yet");
        ui::note(
            "Add a skill",
            &format!(
                "axur install vercel-labs/skills#skills/find-skills\n\nor edit {} by hand",
                crate::core::manifest::MANIFEST_FILE
            ),
        );
        ui::outro("Nothing to do");
        return Ok(());
    }

    if check && existing.is_none() {
        ui::outro_cancel("Nothing to check against");
        anyhow::bail!(
            "No {} in this project. Run `axur sync` and commit the result.",
            crate::core::lockfile::LOCKFILE
        );
    }

    let client = SourceClient::new()?.offline(offline);
    let mut resolved_lock = Lockfile::new();

    // ---- local skills -------------------------------------------------------
    //
    // Read synchronously, before the concurrent GitHub fetches below: there is
    // no network round trip, so there is nothing to gain by interleaving it,
    // and every downstream step (locking, digesting, installing) treats the
    // result exactly like a GitHub-sourced skill from here on.
    let mut skill_results: Vec<(Plan, ResolvedSkill)> = Vec::new();
    let mut local_lines: Vec<String> = Vec::new();
    for (name, spec) in manifest.skills.iter().filter(|(_, spec)| spec.is_local()) {
        let dir = spec.local_dir(name)?;
        let fetched = crate::core::source::read_local_skill(&project_root, dir)
            .with_context(|| format!("Failed to read local skill '{}'", name))?;
        local_lines.push(format!(
            "{} {:<26} {}  {}",
            ui::good("✓"),
            name,
            ui::dim("local"),
            ui::dim(&plural(fetched.files.len(), "file"))
        ));
        skill_results.push((Plan::local(name, dir), fetched));
    }
    let local_count = skill_results.len();

    // ---- resolve, concurrently --------------------------------------------
    let skill_plans: Vec<Plan> = manifest
        .skills
        .iter()
        .filter(|(_, spec)| !spec.is_local())
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

    let total = local_count + skill_plans.len() + bundle_plans.len();
    let multi = ui::MultiProgress::start("Resolving…");

    // Progress bars are transient: a stopped bar may be cleared from the
    // screen. Every resolved source also records a line here so the listing
    // survives as part of the run's permanent output.
    let resolved_lines = std::sync::Mutex::new(local_lines);

    skill_results.extend(
        futures::stream::iter(skill_plans.into_iter().map(|plan| {
            let bar = multi.add(&plan.name);
            let client = &client;
            let lines = &resolved_lines;
            async move {
                let source = plan
                    .source
                    .as_ref()
                    .expect("only GitHub-sourced plans enter this stream");
                let result = client
                    .fetch_skill(
                        source,
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
        .try_collect::<Vec<(Plan, ResolvedSkill)>>()
        .await?,
    );

    let bundle_results: Vec<(Plan, BundleManifest, ResolvedSkill)> =
        futures::stream::iter(bundle_plans.into_iter().map(|plan| {
            let bar = multi.add(&plan.name);
            let client = &client;
            let lines = &resolved_lines;
            async move {
                let source = plan
                    .source
                    .as_ref()
                    .expect("bundles have no local source yet");
                let result = client
                    .fetch_bundle(
                        source,
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

    // ---- keychain-backed secrets --------------------------------------------
    //
    // `axur secrets set NAME` stores a value neither target can reach on its
    // own: Claude Code's ${VAR} expansion and Codex's env_vars both only
    // forward what is already in *this* process's environment, and neither
    // can read an OS keychain. For a server referencing a keychain-managed
    // name, the fix is the same on both targets: point command at axur
    // itself, which resolves the value and then becomes the real command.
    for tool in &mut tools {
        let mut managed: Vec<String> = Vec::new();
        for value in tool.env.values() {
            if let Some(name) = crate::core::agent::env_var_reference(value) {
                if crate::core::secrets::is_managed(name)? && !managed.iter().any(|n| n == name) {
                    managed.push(name.to_string());
                }
            }
        }
        if managed.is_empty() {
            continue;
        }
        managed.sort();

        let mut args = vec!["secrets".to_string(), "exec".to_string()];
        for name in &managed {
            args.push("--name".to_string());
            args.push(name.clone());
        }
        args.push("--".to_string());
        args.push(tool.command.clone());
        args.extend(tool.args.clone());

        tool.command = "axur".to_string();
        tool.args = args;
        // These names are now resolved by the wrapper, not by the target's
        // own expansion — leaving the ${VAR} reference in `env` too would be
        // redundant at best and, on a target with no expansion, wrong.
        tool.env.retain(|_, value| {
            crate::core::agent::env_var_reference(value)
                .map(|name| !managed.iter().any(|m| m == name))
                .unwrap_or(true)
        });
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

    // ---- secrets not yet set ------------------------------------------------
    //
    // A ${VAR} reference axur cannot see set is not a hard failure — CI may
    // inject it later in the pipeline, or the server may go unused this run —
    // but writing a config that references it silently is how "the MCP server
    // never authenticates" turns into a debugging session days later.
    let mut missing_env: Vec<(String, String)> = Vec::new();
    for tool in &tools {
        for value in tool.env.values() {
            if let Some(name) = crate::core::agent::env_var_reference(value) {
                if std::env::var_os(name).is_none()
                    && !missing_env
                        .iter()
                        .any(|(_, n): &(String, String)| n == name)
                {
                    missing_env.push((tool.name.clone(), name.to_string()));
                }
            }
        }
    }
    if !missing_env.is_empty() {
        ui::warning(&format!(
            "{} not set in this environment\n{}",
            plural(missing_env.len(), "environment variable"),
            missing_env
                .iter()
                .map(|(server, var)| format!("{}  ·  needed by {}", var, server))
                .collect::<Vec<_>>()
                .join("\n")
        ));
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
            "Out of sync — run `axur sync` and commit {}",
            crate::core::lockfile::LOCKFILE
        ));
        std::process::exit(DRIFT_EXIT_CODE);
    }

    // ---- consent ----------------------------------------------------------
    //
    // Everything below this point writes something that will execute. Collect
    // it, show whatever has not been approved before, and stop if the answer
    // is no.
    let mut requests: Vec<Request> = tools
        .iter()
        .map(|t| {
            let origin = bundles
                .iter()
                .find(|(_, p)| p.manifest.mcp.contains_key(&t.name))
                .map(|(name, p)| format!("bundle {} ({})", name, p.manifest.name))
                .unwrap_or_else(|| crate::core::manifest::MANIFEST_FILE.to_string());
            Request::mcp(&t.name, &t.command, &t.args, &origin)
        })
        .collect();

    for (bundle_name, payload) in &bundles {
        for hook in &payload.manifest.hooks {
            let body = payload.file(&hook.command).cloned().unwrap_or_default();
            requests.push(Request::hook(
                &hook.command,
                &hook.event,
                hook.matcher.as_deref(),
                &body,
                &format!("bundle {}", bundle_name),
            ));
        }
    }

    if !requests.is_empty() {
        match super::trust_gate::review(&requests, assume_yes)? {
            super::trust_gate::Decision::Proceed => {}
            super::trust_gate::Decision::Declined => {
                ui::outro_cancel("Declined — nothing installed");
                return Ok(());
            }
        }
    }

    // ---- prune what the manifest no longer declares -----------------------
    //
    // A skill removed from axur.toml has to leave the disk too, or the
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

    // ---- preflight --------------------------------------------------------
    //
    // Resolve every destination, and check every file a bundle points at, while
    // the disk is still untouched. Both used to fail partway through the write
    // loop, after an earlier target had already been changed.
    let installers: Vec<(Target, Box<dyn Installer>)> = targets
        .iter()
        .map(|target| {
            let installer = get_installer(*target, scope);
            installer.skills_root().with_context(|| {
                format!(
                    "Cannot determine where {} keeps its skills",
                    target.display_name()
                )
            })?;
            Ok((*target, installer))
        })
        .collect::<Result<_>>()?;

    for (bundle_name, payload) in &bundles {
        let bundle = &payload.manifest;
        for path in bundle
            .agents
            .iter()
            .chain(&bundle.commands)
            .chain(&bundle.scripts)
        {
            if payload.file(path).is_none() {
                anyhow::bail!(
                    "Bundle '{}' declares {} but the source does not contain it.",
                    bundle_name,
                    path
                );
            }
        }
    }

    // ---- install ----------------------------------------------------------
    //
    // Everything past this point writes to disk. The journal records each
    // change so a failure on the second target undoes the first, rather than
    // leaving the machine half-configured with no lockfile describing it.
    let journal = crate::core::tx::begin();

    // Per-target reports are held until the whole run commits. Announcing a
    // target as done while a later one can still roll it back reads as a
    // contradiction: the run says the skill was installed, then says nothing
    // was changed.
    let mut reports: Vec<String> = Vec::new();

    let outcome: Result<()> = (|| {
        for (target, installer) in &installers {
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
                        let bytes = payload.file(path).with_context(|| {
                            format!("Bundle '{}' is missing {}", bundle_name, path)
                        })?;
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
                        let bytes = payload.file(path).with_context(|| {
                            format!("Bundle '{}' is missing {}", bundle_name, path)
                        })?;
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

            reports.push(format!(
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

        // The lockfile is written inside the journal: if it cannot be saved, the
        // installs it would have described are rolled back too, so the disk and the
        // lock can never disagree.
        resolved_lock.save(&lock_path)?;
        Ok(())
    })();

    outcome.map_err(rolled_back)?;
    journal.commit();

    // ---- report -----------------------------------------------------------
    for report in &reports {
        ui::success(report);
    }

    let changed = existing
        .as_ref()
        .map(|prev| !prev.diff(&resolved_lock).is_empty())
        .unwrap_or(true);

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

/// Which agents to install for on this machine.
///
/// A project's `[targets].agents` is the superset it has content for; which of
/// those a given developer actually runs is a personal choice — recorded in
/// `~/.axur/projects.toml` (see [`crate::core::local`]), never in the repo, so
/// cloning a project never inherits whoever synced first.
///
/// Resolution order: `--agents` (and it updates the cache), then the cached
/// choice for this project, then — only on a terminal, and only when there is
/// more than one to choose from — a prompt. A non-interactive run with no
/// cache and no flag (CI, a piped shell) falls back to everything declared,
/// which matches what `sync` did before per-developer selection existed.
fn resolve_effective_targets(
    declared: &[Target],
    project_root: &std::path::Path,
    agents_flag: Option<&str>,
) -> Result<Vec<Target>> {
    use crate::core::local::ProjectsStore;

    let mut store = ProjectsStore::load()?;

    if let Some(raw) = agents_flag {
        let chosen = parse_agents_flag(raw, declared)?;
        store.set_agents_for(project_root, &chosen);
        store.save()?;
        return Ok(chosen);
    }

    if let Some(cached) = store.agents_for(project_root) {
        let filtered: Vec<Target> = cached
            .into_iter()
            .filter(|t| declared.contains(t))
            .collect();
        if !filtered.is_empty() {
            return Ok(filtered);
        }
        // Everything cached has since dropped out of [targets].agents — fall
        // through and ask again rather than silently installing nothing.
    }

    if declared.len() > 1 && ui::is_rich() {
        let detected: Vec<Target> = declared
            .iter()
            .copied()
            .filter(|t| t.is_detected())
            .collect();
        let chosen = ui::select_sync_targets(declared, &detected)?;
        if chosen.is_empty() {
            anyhow::bail!(
                "No agents selected. Run `axur sync` again and pick at least one, \
                 or `axur sync --agents claude-code,codex` to skip the prompt."
            );
        }
        store.set_agents_for(project_root, &chosen);
        store.save()?;
        return Ok(chosen);
    }

    Ok(declared.to_vec())
}

/// Parse `--agents claude-code,codex`, rejecting anything the manifest does
/// not declare — a machine-local choice cannot ask for content the project
/// never resolved.
fn parse_agents_flag(raw: &str, declared: &[Target]) -> Result<Vec<Target>> {
    let mut chosen = Vec::new();
    for part in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let target = Target::from_slug(part).with_context(|| {
            format!(
                "Unknown agent '{}' in --agents. Supported: claude-code, codex.",
                part
            )
        })?;
        if !declared.contains(&target) {
            anyhow::bail!(
                "--agents requested '{}', but {} does not declare it in [targets].agents.",
                part,
                crate::core::manifest::MANIFEST_FILE
            );
        }
        if !chosen.contains(&target) {
            chosen.push(target);
        }
    }
    if chosen.is_empty() {
        anyhow::bail!("--agents was empty");
    }
    Ok(chosen)
}

/// Name the rollback at the top of the error chain.
///
/// The underlying failure stays visible as the cause; this only answers the
/// question a half-finished install would otherwise leave open, which is
/// whether the machine has been left in some in-between state.
fn rolled_back(err: anyhow::Error) -> anyhow::Error {
    err.context("Nothing was changed - every target was rolled back to its previous state")
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
         `axur sync --update` if the change is expected.",
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
