//! `agentpm install` — add a source to the manifest and sync.
//!
//! Modelled on `npm install <pkg>`: the command records the dependency in
//! `agentpm.toml` and then reconciles, rather than performing a one-off install
//! that nothing remembers. That is what keeps the manifest the single
//! description of what a project needs.

use anyhow::{Context, Result};

use crate::core::manifest::{Manifest, SkillSpec, MANIFEST_FILE};
use crate::core::source::{GitHubSource, SourceClient, SourceKind};
use crate::utils::{paths, ui};

/// Split `owner/repo#path/inside/repo` into its parts.
fn split_source(spec: &str) -> (&str, Option<&str>) {
    match spec.split_once('#') {
        Some((source, path)) if !path.trim().is_empty() => (source, Some(path.trim())),
        _ => (spec, None),
    }
}

/// The name an entry installs under: the last path segment, else the repo.
fn derive_name(source: &GitHubSource, path: Option<&str>) -> String {
    path.and_then(|p| p.trim_matches('/').rsplit('/').next())
        .filter(|s| !s.is_empty())
        .unwrap_or(&source.repo)
        .to_string()
}

pub async fn execute(spec: &str, rev: Option<String>, name_override: Option<String>) -> Result<()> {
    ui::intro(&format!("agentpm install {}", spec));

    let (source_str, path) = split_source(spec);
    let source = GitHubSource::parse(source_str)?;
    let name = name_override.unwrap_or_else(|| derive_name(&source, path));

    // The manifest is created on demand so `install` works in a fresh repo
    // without a separate `init` step.
    let project_root = paths::project_root()?;
    let manifest_path =
        Manifest::find(&project_root).unwrap_or_else(|| project_root.join(MANIFEST_FILE));
    let mut manifest = if manifest_path.exists() {
        Manifest::load(&manifest_path)?
    } else {
        Manifest::default()
    };

    let spinner = ui::Spinner::start(&format!("Resolving {}…", source.slug()));
    let client = SourceClient::new()?;
    let (commit, kind) = client
        .probe(&source, path.unwrap_or(&name), rev.as_deref())
        .await
        .with_context(|| format!("Could not resolve {}", spec))?;
    spinner.stop(&format!(
        "{} {}  {}",
        match kind {
            SourceKind::Bundle => "bundle",
            SourceKind::Skill => "skill",
        },
        ui::bold(&name),
        ui::dim(&ui::short_sha(&commit))
    ));

    let entry = SkillSpec::Detailed {
        source: source.slug(),
        path: path.map(str::to_string),
        rev: rev.clone(),
    };

    let table = match kind {
        SourceKind::Bundle => &mut manifest.bundles,
        SourceKind::Skill => &mut manifest.skills,
    };
    let replaced = table.insert(name.clone(), entry).is_some();

    manifest.save(&manifest_path)?;
    ui::success(&format!(
        "{} {} in {}",
        if replaced { "Updated" } else { "Added" },
        ui::bold(&name),
        ui::accent(MANIFEST_FILE)
    ));

    // Reconcile so the entry is actually on disk and in the lockfile. The
    // rail stays open: sync closes it.
    super::sync::reconcile().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_source_from_its_path() {
        assert_eq!(
            split_source("vercel-labs/skills#skills/find-skills"),
            ("vercel-labs/skills", Some("skills/find-skills"))
        );
        assert_eq!(
            split_source("ahmed6ww/ax-agents"),
            ("ahmed6ww/ax-agents", None)
        );
        assert_eq!(split_source("owner/repo#"), ("owner/repo#", None));
    }

    #[test]
    fn derives_the_name_from_the_last_segment() {
        let source = GitHubSource::parse("vercel-labs/skills").unwrap();
        assert_eq!(
            derive_name(&source, Some("skills/find-skills")),
            "find-skills"
        );
        assert_eq!(derive_name(&source, Some("skills/web-perf/")), "web-perf");
        // With no path, the repository name is the sensible default.
        assert_eq!(derive_name(&source, None), "skills");
    }
}
