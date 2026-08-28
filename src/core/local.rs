//! Per-machine, per-project agent preference.
//!
//! Which agent(s) a developer runs is a personal choice, not a project-wide
//! one — two teammates on the same repository may run different editors, or
//! only one of the two `axur.toml` declares as available. Recording it in
//! `~/.axur/projects.toml`, never inside the project directory, is the same
//! reasoning [`crate::core::trust`] uses for `trust.toml`: a file a clone
//! could pick up would let whoever ran `sync` first impose their choice on
//! everyone else.
//!
//! Keyed by the project's canonical root, so the same clone resolves to the
//! same entry regardless of which directory a command was run from.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::installers::Target;

pub const PROJECTS_FILE: &str = "projects.toml";
pub const PROJECTS_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectsStore {
    pub version: u32,
    #[serde(default, rename = "project")]
    entries: Vec<ProjectPrefs>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProjectPrefs {
    root: String,
    agents: Vec<String>,
}

/// The join key: canonicalized so `.` and a full path resolve to the same
/// entry. Falls back to the given path verbatim if canonicalization fails —
/// still stable within one run, and a project root is never expected to be
/// missing since it was just used to locate `axur.toml`.
fn key(root: &Path) -> String {
    std::fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .display()
        .to_string()
}

impl ProjectsStore {
    pub fn new() -> Self {
        Self {
            version: PROJECTS_VERSION,
            entries: Vec::new(),
        }
    }

    pub fn path() -> Result<PathBuf> {
        Ok(crate::utils::paths::axur_config_dir()?.join(PROJECTS_FILE))
    }

    pub fn load() -> Result<Self> {
        Self::load_from(&Self::path()?)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        if content.trim().is_empty() {
            return Ok(Self::new());
        }

        let store: Self = toml::from_str(&content).with_context(|| {
            format!(
                "{} is not valid TOML. axur will not overwrite it — fix or \
                 move the file, then retry.",
                path.display()
            )
        })?;

        if store.version != PROJECTS_VERSION {
            anyhow::bail!(
                "{} was written by a different version of axur (version {}, expected {}).",
                path.display(),
                store.version,
                PROJECTS_VERSION
            );
        }

        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        let mut sorted = self.clone();
        sorted.entries.sort_by(|a, b| a.root.cmp(&b.root));

        let header = "# Which agent(s) axur syncs on this machine, per project.\n\
                      # Delete a project's entry to be asked again.\n\n";
        let rendered =
            toml::to_string_pretty(&sorted).context("Failed to serialize projects.toml")?;
        crate::installers::common::write_atomic(path, (header.to_string() + &rendered).as_bytes())
    }

    /// The cached choice for a project, if one exists. Unrecognized agent
    /// slugs (a manifest edited by a newer axur, or a hand-corrupted file)
    /// are dropped rather than failing the load.
    pub fn agents_for(&self, root: &Path) -> Option<Vec<Target>> {
        let key = key(root);
        self.entries.iter().find(|e| e.root == key).map(|e| {
            e.agents
                .iter()
                .filter_map(|s| Target::from_slug(s))
                .collect()
        })
    }

    /// Record a project's choice, replacing any previous one.
    pub fn set_agents_for(&mut self, root: &Path, agents: &[Target]) {
        let key = key(root);
        let agents: Vec<String> = agents.iter().map(|t| t.slug().to_string()).collect();
        match self.entries.iter_mut().find(|e| e.root == key) {
            Some(entry) => entry.agents = agents,
            None => self.entries.push(ProjectPrefs { root: key, agents }),
        }
    }
}

impl Default for ProjectsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_store_is_empty_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectsStore::load_from(&dir.path().join("absent.toml")).unwrap();
        assert!(store.agents_for(dir.path()).is_none());
    }

    #[test]
    fn round_trips_a_projects_choice() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(PROJECTS_FILE);
        let project = dir.path().join("repo");
        std::fs::create_dir_all(&project).unwrap();

        let mut store = ProjectsStore::new();
        store.set_agents_for(&project, &[Target::Claude]);
        store.save_to(&store_path).unwrap();

        let loaded = ProjectsStore::load_from(&store_path).unwrap();
        assert_eq!(loaded.agents_for(&project), Some(vec![Target::Claude]));
    }

    #[test]
    fn a_second_choice_replaces_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("repo");
        std::fs::create_dir_all(&project).unwrap();

        let mut store = ProjectsStore::new();
        store.set_agents_for(&project, &[Target::Claude]);
        store.set_agents_for(&project, &[Target::Codex]);
        assert_eq!(store.agents_for(&project), Some(vec![Target::Codex]));
    }

    #[test]
    fn different_projects_keep_separate_choices() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();

        let mut store = ProjectsStore::new();
        store.set_agents_for(&a, &[Target::Claude]);
        store.set_agents_for(&b, &[Target::Codex]);

        assert_eq!(store.agents_for(&a), Some(vec![Target::Claude]));
        assert_eq!(store.agents_for(&b), Some(vec![Target::Codex]));
    }

    #[test]
    fn a_corrupt_store_is_never_silently_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PROJECTS_FILE);
        std::fs::write(&path, "{{ not toml").unwrap();

        let err = ProjectsStore::load_from(&path).unwrap_err();
        assert!(format!("{:#}", err).contains("not valid TOML"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{{ not toml");
    }
}
