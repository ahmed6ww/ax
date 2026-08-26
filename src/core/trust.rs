//! What the user has agreed to let run on their machine.
//!
//! Installing a bundle authorises code: an MCP server is a command the editor
//! launches at startup, and a hook is a script Claude Code runs on an event.
//! Nothing in the fetch path makes that visible, so a compromised source, a
//! hijacked account, or a careless copy-paste becomes execution with no moment
//! where a person could have said no.
//!
//! Approval is recorded per user, at `~/.agentpm/trust.toml`, and deliberately
//! **not** in `agentpm.lock`. The lockfile is committed: if it carried
//! approvals, cloning a repository would silently inherit the decisions of
//! whoever ran sync first, which is the property being defended against.
//!
//! Entries are keyed by digest, so editing a command or a script's contents
//! revokes the approval and asks again.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::core::lockfile::digest;

pub const TRUST_FILE: &str = "trust.toml";
pub const TRUST_VERSION: u32 = 1;

/// The kind of thing being authorised, for reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grant {
    /// A command the editor launches as an MCP server.
    Mcp,
    /// A script the agent runs on a lifecycle event.
    Hook,
}

impl Grant {
    pub fn noun(&self) -> &'static str {
        match self {
            Grant::Mcp => "MCP server",
            Grant::Hook => "hook script",
        }
    }

    /// Why this needs consent, in the user's terms.
    pub fn consequence(&self) -> &'static str {
        match self {
            Grant::Mcp => "runs on your machine every time the agent starts",
            Grant::Hook => "runs on your machine when the agent hits this event",
        }
    }
}

/// Something that will execute, presented for approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub kind: Grant,
    /// Short name: the server name, or the script's path.
    pub label: String,
    /// The command line, or the event and matcher.
    pub detail: String,
    /// Where it came from, for attribution.
    pub origin: String,
    /// Identity. Changing what runs changes this, which revokes approval.
    pub digest: String,
}

impl Request {
    /// An MCP server. The digest covers name, command and arguments.
    ///
    /// Environment values are excluded: they hold API keys, and a key rotating
    /// should not re-prompt for a command that has not changed.
    pub fn mcp(name: &str, command: &str, args: &[String], origin: &str) -> Self {
        let material = format!(
            "mcp\u{0}{}\u{0}{}\u{0}{}",
            name,
            command,
            args.join("\u{0}")
        );
        Self {
            kind: Grant::Mcp,
            label: name.to_string(),
            detail: if args.is_empty() {
                command.to_string()
            } else {
                format!("{} {}", command, args.join(" "))
            },
            origin: origin.to_string(),
            digest: digest(material.as_bytes()),
        }
    }

    /// A hook script. The digest covers the script's contents, so an edited
    /// script asks again even at the same path.
    pub fn hook(
        script: &str,
        event: &str,
        matcher: Option<&str>,
        body: &[u8],
        origin: &str,
    ) -> Self {
        let mut material = Vec::from(b"hook\0".as_slice());
        material.extend_from_slice(event.as_bytes());
        material.push(0);
        material.extend_from_slice(matcher.unwrap_or("").as_bytes());
        material.push(0);
        material.extend_from_slice(body);

        Self {
            kind: Grant::Hook,
            label: script.to_string(),
            detail: match matcher {
                Some(m) => format!("{} · {}", event, m),
                None => event.to_string(),
            },
            origin: origin.to_string(),
            digest: digest(&material),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustStore {
    pub version: u32,
    #[serde(default, rename = "approved")]
    pub entries: Vec<Approval>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    pub digest: String,
    pub kind: Grant,
    pub label: String,
    pub detail: String,
    #[serde(default)]
    pub origin: String,
}

impl TrustStore {
    pub fn new() -> Self {
        Self {
            version: TRUST_VERSION,
            entries: Vec::new(),
        }
    }

    pub fn path() -> Result<PathBuf> {
        Ok(crate::utils::paths::agentpm_config_dir()?.join(TRUST_FILE))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        Self::load_from(&path)
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
                "{} is not valid TOML. agentpm will not overwrite it — fix or \
                 move the file, then retry.",
                path.display()
            )
        })?;

        if store.version != TRUST_VERSION {
            anyhow::bail!(
                "{} was written by a different version of agentpm (version {}, expected {}).",
                path.display(),
                store.version,
                TRUST_VERSION
            );
        }

        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        let mut sorted = self.clone();
        sorted.entries.sort_by(|a, b| a.label.cmp(&b.label));
        sorted.entries.dedup_by(|a, b| a.digest == b.digest);

        let header = "# Records what you have allowed agentpm to install that runs code.\n\
                      # Delete an entry to be asked about it again.\n\n";
        let rendered =
            toml::to_string_pretty(&sorted).context("Failed to serialize the trust store")?;
        crate::installers::common::write_atomic(path, (header.to_string() + &rendered).as_bytes())
    }

    pub fn is_approved(&self, request: &Request) -> bool {
        self.entries.iter().any(|e| e.digest == request.digest)
    }

    pub fn approve(&mut self, request: &Request) {
        if self.is_approved(request) {
            return;
        }
        self.entries.push(Approval {
            digest: request.digest.clone(),
            kind: request.kind,
            label: request.label.clone(),
            detail: request.detail.clone(),
            origin: request.origin.clone(),
        });
    }

    /// Requests that have not been approved yet, in presentation order.
    pub fn outstanding(&self, requests: &[Request]) -> Vec<Request> {
        let mut pending: Vec<Request> = requests
            .iter()
            .filter(|r| !self.is_approved(r))
            .cloned()
            .collect();
        // Deduplicate: the same server declared for two targets is one decision.
        pending.sort_by(|a, b| a.label.cmp(&b.label));
        pending.dedup_by(|a, b| a.digest == b.digest);
        pending
    }

    /// Drop an approval by label. Returns how many entries were removed.
    pub fn revoke(&mut self, label: &str) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| e.label != label);
        before - self.entries.len()
    }
}

impl Default for TrustStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mcp() -> Request {
        Request::mcp(
            "context7",
            "npx",
            &["-y".to_string(), "@upstash/context7-mcp".to_string()],
            "agentpm.toml",
        )
    }

    #[test]
    fn identical_servers_share_a_digest() {
        assert_eq!(mcp().digest, mcp().digest);
    }

    #[test]
    fn changing_the_command_revokes_approval() {
        let mut store = TrustStore::new();
        store.approve(&mcp());
        assert!(store.is_approved(&mcp()));

        let tampered = Request::mcp("context7", "curl", &["evil.sh".to_string()], "agentpm.toml");
        assert!(
            !store.is_approved(&tampered),
            "a different command must not inherit approval"
        );
    }

    #[test]
    fn changing_an_argument_revokes_approval() {
        let mut store = TrustStore::new();
        store.approve(&mcp());
        let tampered = Request::mcp(
            "context7",
            "npx",
            &["-y".to_string(), "@evil/mcp".to_string()],
            "agentpm.toml",
        );
        assert!(!store.is_approved(&tampered));
    }

    #[test]
    fn editing_a_hook_script_revokes_approval() {
        let mut store = TrustStore::new();
        let original = Request::hook(
            "guard.sh",
            "PreToolUse",
            Some("Bash"),
            b"#!/bin/sh\n",
            "bundle backend",
        );
        store.approve(&original);
        assert!(store.is_approved(&original));

        let edited = Request::hook(
            "guard.sh",
            "PreToolUse",
            Some("Bash"),
            b"#!/bin/sh\nrm -rf /\n",
            "bundle backend",
        );
        assert!(
            !store.is_approved(&edited),
            "an edited script at the same path must be re-reviewed"
        );
    }

    #[test]
    fn outstanding_deduplicates_the_same_decision() {
        let store = TrustStore::new();
        // The same server declared for both targets is one decision, not two.
        let pending = store.outstanding(&[mcp(), mcp()]);
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn outstanding_hides_what_is_already_approved() {
        let mut store = TrustStore::new();
        store.approve(&mcp());
        assert!(store.outstanding(&[mcp()]).is_empty());
    }

    #[test]
    fn approving_twice_records_one_entry() {
        let mut store = TrustStore::new();
        store.approve(&mcp());
        store.approve(&mcp());
        assert_eq!(store.entries.len(), 1);
    }

    #[test]
    fn revoking_asks_again() {
        let mut store = TrustStore::new();
        store.approve(&mcp());
        assert_eq!(store.revoke("context7"), 1);
        assert!(!store.is_approved(&mcp()));
        assert_eq!(store.revoke("context7"), 0);
    }

    #[test]
    fn round_trips_through_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(TRUST_FILE);

        let mut store = TrustStore::new();
        store.approve(&mcp());
        store.save_to(&path).unwrap();

        let loaded = TrustStore::load_from(&path).unwrap();
        assert_eq!(loaded.entries, store.entries);
        assert!(loaded.is_approved(&mcp()));
    }

    #[test]
    fn a_missing_store_is_empty_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = TrustStore::load_from(&dir.path().join("absent.toml")).unwrap();
        assert!(store.entries.is_empty());
    }

    #[test]
    fn a_corrupt_store_is_never_silently_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(TRUST_FILE);
        std::fs::write(&path, "{{ not toml").unwrap();

        let err = TrustStore::load_from(&path).unwrap_err();
        assert!(format!("{:#}", err).contains("not valid TOML"));
        // Refusing means the user's approvals are not quietly discarded.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{{ not toml");
    }
}
