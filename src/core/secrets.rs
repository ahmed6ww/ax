//! `axur secrets` — API keys and tokens, stored in the OS keychain instead of
//! a shell profile.
//!
//! Everything an MCP server needs is still declared as an `env` reference in
//! `axur.toml` (see [`crate::core::agent::env_var_reference`]) — this module
//! only changes *where the value lives*. Windows Credential Manager, macOS
//! Keychain, and the Linux Secret Service are the three backends the
//! `keyring` crate covers natively; axur never sees or stores the value
//! itself outside of them.
//!
//! The keychain has no cross-platform "list everything" API, so a small local
//! index at `~/.axur/secrets.toml` tracks *names only* — never values — purely
//! so `axur secrets` (no arguments) has something to enumerate. The index is
//! a cache, not the source of truth: [`get`] always reads the keychain
//! directly, and a name falling out of sync with the index (cleared by some
//! other tool) degrades to "not set" rather than erroring.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The keychain "service" every axur-managed secret is filed under.
const SERVICE: &str = "axur";

pub const INDEX_FILE: &str = "secrets.toml";
pub const INDEX_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Index {
    version: u32,
    #[serde(default)]
    names: Vec<String>,
}

impl Index {
    fn new() -> Self {
        Self {
            version: INDEX_VERSION,
            names: Vec::new(),
        }
    }

    fn path() -> Result<PathBuf> {
        Ok(crate::utils::paths::axur_config_dir()?.join(INDEX_FILE))
    }

    fn load() -> Result<Self> {
        Self::load_from(&Self::path()?)
    }

    fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        if content.trim().is_empty() {
            return Ok(Self::new());
        }
        let index: Self = toml::from_str(&content).with_context(|| {
            format!(
                "{} is not valid TOML. axur will not overwrite it — fix or \
                 move the file, then retry.",
                path.display()
            )
        })?;
        if index.version != INDEX_VERSION {
            anyhow::bail!(
                "{} was written by a different version of axur (version {}, expected {}).",
                path.display(),
                index.version,
                INDEX_VERSION
            );
        }
        Ok(index)
    }

    fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        let mut sorted = self.clone();
        sorted.names.sort();
        sorted.names.dedup();
        let header = "# Names of secrets axur has stored in the OS keychain.\n\
                      # No values live here — see `axur secrets`.\n\n";
        let rendered =
            toml::to_string_pretty(&sorted).context("Failed to serialize secrets.toml")?;
        crate::installers::common::write_atomic(path, (header.to_string() + &rendered).as_bytes())
    }
}

fn entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, name)
        .with_context(|| format!("Could not open the OS keychain for '{}'", name))
}

/// Store `value` for `name`, overwriting whatever was there.
pub fn set(name: &str, value: &str) -> Result<()> {
    entry(name)?
        .set_password(value)
        .with_context(|| format!("Failed to store '{}' in the OS keychain", name))?;

    let mut index = Index::load()?;
    if !index.names.iter().any(|n| n == name) {
        index.names.push(name.to_string());
        index.save()?;
    }
    Ok(())
}

/// Remove a stored secret. `Ok(false)` if nothing was there.
pub fn unset(name: &str) -> Result<bool> {
    let existed = match entry(name)?.delete_credential() {
        Ok(()) => true,
        Err(keyring::Error::NoEntry) => false,
        Err(e) => {
            return Err(e).with_context(|| format!("Failed to remove '{}' from the keychain", name))
        }
    };

    let mut index = Index::load()?;
    let before = index.names.len();
    index.names.retain(|n| n != name);
    if index.names.len() != before {
        index.save()?;
    }

    Ok(existed)
}

/// Read a secret's value, straight from the keychain. `None` if there is no
/// entry — including one the index believes exists but was removed by some
/// other means, which is treated as "not set" rather than an error.
pub fn get(name: &str) -> Result<Option<String>> {
    match entry(name)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).with_context(|| format!("Failed to read '{}' from the keychain", name)),
    }
}

/// Names currently recorded as stored, sorted. A listing, not a guarantee —
/// see the module doc.
pub fn list() -> Result<Vec<String>> {
    let mut names = Index::load()?.names;
    names.sort();
    Ok(names)
}

/// Fast, index-only check: would this name route through the keychain?
///
/// Used at sync time to decide whether an MCP server's command needs
/// wrapping — a plain keychain read per referenced name, every sync, would
/// otherwise cost a round trip to the OS credential store for names nobody
/// ever stored.
pub fn is_managed(name: &str) -> Result<bool> {
    Ok(Index::load()?.names.iter().any(|n| n == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The keychain itself (Entry::set_password/get_password/delete_credential)
    // is not exercised here: a headless Linux CI runner typically has no
    // Secret Service session bus, so a test that touches the real store would
    // be flaky on exactly the platform least likely to have someone watching
    // it fail. The index — the only part with meaningful logic of its own —
    // is plain file I/O and is fully covered below.

    #[test]
    fn a_missing_index_is_empty_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let index = Index::load_from(&dir.path().join("absent.toml")).unwrap();
        assert!(index.names.is_empty());
    }

    #[test]
    fn round_trips_names_through_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(INDEX_FILE);

        let mut index = Index::new();
        index.names.push("LINEAR_API_KEY".to_string());
        index.save_to(&path).unwrap();

        let loaded = Index::load_from(&path).unwrap();
        assert_eq!(loaded.names, vec!["LINEAR_API_KEY".to_string()]);
    }

    #[test]
    fn a_corrupt_index_is_never_silently_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(INDEX_FILE);
        std::fs::write(&path, "{{ not toml").unwrap();

        let err = Index::load_from(&path).unwrap_err().to_string();
        assert!(err.contains("not valid TOML"), "{}", err);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{{ not toml");
    }

    #[test]
    fn saving_sorts_and_dedupes_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(INDEX_FILE);

        let mut index = Index::new();
        index.names = vec!["B".to_string(), "A".to_string(), "A".to_string()];
        index.save_to(&path).unwrap();

        let loaded = Index::load_from(&path).unwrap();
        assert_eq!(loaded.names, vec!["A".to_string(), "B".to_string()]);
    }
}
