//! A content cache for immutable fetches.
//!
//! Once a ref resolves to a commit SHA, everything below it is frozen: the file
//! listing at that commit never changes, and neither does the content of a path
//! within it. So both are cached forever under a key derived from
//! `(owner, repo, commit, path)`, and a sync of already-pinned sources makes no
//! network requests at all.
//!
//! This matters beyond speed. Every sync previously refetched every file even
//! when fully pinned, which reaches GitHub's anonymous rate limit after a
//! handful of runs — the failure hit repeatedly while testing this project.
//!
//! Only immutable things are stored. Resolving a branch to a commit is a
//! network call every time, because a branch moves.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::core::lockfile::digest;

/// Cache entries larger than this are fetched but not stored, so one huge file
/// cannot dominate the directory.
const MAX_CACHED_BYTES: usize = 4 * 1024 * 1024;

pub struct Cache {
    root: Option<PathBuf>,
}

impl Cache {
    /// Open the cache at `~/.agentpm/cache`.
    ///
    /// `AGENTPM_NO_CACHE` disables it. A cache that cannot be opened is not an
    /// error: it degrades to fetching, which is what it was doing before.
    pub fn open() -> Self {
        if std::env::var_os("AGENTPM_NO_CACHE").is_some() {
            return Self { root: None };
        }

        let root = crate::utils::paths::agentpm_config_dir()
            .map(|dir| dir.join("cache"))
            .ok();

        Self { root }
    }

    /// A cache that never stores anything, for tests and `--no-cache`.
    pub fn disabled() -> Self {
        Self { root: None }
    }

    pub fn is_enabled(&self) -> bool {
        self.root.is_some()
    }

    /// Key for one file's contents at a commit.
    pub fn blob_key(owner: &str, repo: &str, commit: &str, path: &str) -> String {
        digest(
            format!(
                "blob\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
                owner, repo, commit, path
            )
            .as_bytes(),
        )
    }

    /// Key for the file listing of a commit.
    pub fn tree_key(owner: &str, repo: &str, commit: &str) -> String {
        digest(format!("tree\u{0}{}\u{0}{}\u{0}{}", owner, repo, commit).as_bytes())
    }

    /// Two levels of fan-out so a large cache does not put tens of thousands of
    /// entries in one directory.
    fn path_for(&self, key: &str) -> Option<PathBuf> {
        let hex = key.strip_prefix("sha256:").unwrap_or(key);
        if hex.len() < 4 {
            return None;
        }
        Some(
            self.root
                .as_ref()?
                .join(&hex[..2])
                .join(&hex[2..4])
                .join(hex),
        )
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let path = self.path_for(key)?;
        std::fs::read(path).ok()
    }

    /// Store `bytes`. Failure is silent: a cache miss is always survivable, and
    /// a full disk should not fail an install that would otherwise succeed.
    pub fn put(&self, key: &str, bytes: &[u8]) {
        if bytes.len() > MAX_CACHED_BYTES {
            return;
        }
        let Some(path) = self.path_for(key) else {
            return;
        };
        let _ = write_entry(&path, bytes);
    }

    /// Total size and entry count, for reporting.
    pub fn stats(&self) -> (u64, usize) {
        let Some(root) = &self.root else {
            return (0, 0);
        };
        let mut bytes = 0u64;
        let mut count = 0usize;
        visit(root, &mut |meta| {
            bytes += meta;
            count += 1;
        });
        (bytes, count)
    }

    /// Delete every entry.
    pub fn clear(&self) -> Result<usize> {
        let Some(root) = &self.root else {
            return Ok(0);
        };
        if !root.exists() {
            return Ok(0);
        }
        let (_, count) = self.stats();
        std::fs::remove_dir_all(root)
            .with_context(|| format!("Failed to clear {}", root.display()))?;
        Ok(count)
    }
}

fn write_entry(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let parent = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(parent)?;

    // A temp file keyed by process id avoids two concurrent syncs writing the
    // same partial entry, and the rename makes the visible state all-or-nothing.
    let tmp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("entry"),
        std::process::id()
    ));

    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    #[cfg(target_os = "windows")]
    if path.exists() {
        // The content is identical by construction, so an existing entry is
        // already correct; nothing is lost by leaving it.
        let _ = std::fs::remove_file(&tmp);
        return Ok(());
    }

    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn visit(dir: &Path, f: &mut impl FnMut(u64)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit(&path, f);
        } else if let Ok(meta) = entry.metadata() {
            f(meta.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_in(dir: &Path) -> Cache {
        Cache {
            root: Some(dir.join("cache")),
        }
    }

    #[test]
    fn keys_are_stable_and_distinguish_every_field() {
        let base = Cache::blob_key("o", "r", "abc123", "SKILL.md");
        assert_eq!(base, Cache::blob_key("o", "r", "abc123", "SKILL.md"));

        // Changing any component must change the key, or one commit's content
        // could be served for another's.
        assert_ne!(base, Cache::blob_key("other", "r", "abc123", "SKILL.md"));
        assert_ne!(base, Cache::blob_key("o", "other", "abc123", "SKILL.md"));
        assert_ne!(base, Cache::blob_key("o", "r", "def456", "SKILL.md"));
        assert_ne!(base, Cache::blob_key("o", "r", "abc123", "other.md"));
    }

    #[test]
    fn tree_and_blob_keys_never_collide() {
        assert_ne!(
            Cache::tree_key("o", "r", "abc"),
            Cache::blob_key("o", "r", "abc", "")
        );
    }

    #[test]
    fn round_trips_content() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_in(dir.path());
        let key = Cache::blob_key("o", "r", "abc", "SKILL.md");

        assert!(cache.get(&key).is_none());
        cache.put(&key, b"hello");
        assert_eq!(cache.get(&key).as_deref(), Some(b"hello".as_slice()));
    }

    #[test]
    fn a_disabled_cache_stores_nothing() {
        let cache = Cache::disabled();
        let key = Cache::blob_key("o", "r", "abc", "SKILL.md");
        cache.put(&key, b"hello");
        assert!(cache.get(&key).is_none());
        assert!(!cache.is_enabled());
    }

    #[test]
    fn oversized_entries_are_not_stored() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_in(dir.path());
        let key = Cache::blob_key("o", "r", "abc", "big");

        cache.put(&key, &vec![0u8; MAX_CACHED_BYTES + 1]);
        assert!(cache.get(&key).is_none(), "an oversized entry was stored");
    }

    #[test]
    fn stats_and_clear_account_for_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_in(dir.path());

        for i in 0..3 {
            cache.put(
                &Cache::blob_key("o", "r", "abc", &format!("f{}", i)),
                b"xxxx",
            );
        }

        let (bytes, count) = cache.stats();
        assert_eq!(count, 3);
        assert_eq!(bytes, 12);

        assert_eq!(cache.clear().unwrap(), 3);
        assert_eq!(cache.stats(), (0, 0));
    }

    #[test]
    fn no_partial_entries_are_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_in(dir.path());
        let key = Cache::blob_key("o", "r", "abc", "SKILL.md");
        cache.put(&key, b"hello");

        let mut temps = 0;
        visit_names(&dir.path().join("cache"), &mut |name| {
            if name.contains(".tmp") {
                temps += 1;
            }
        });
        assert_eq!(temps, 0);
    }

    fn visit_names(dir: &Path, f: &mut impl FnMut(&str)) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit_names(&path, f);
            } else {
                f(&entry.file_name().to_string_lossy());
            }
        }
    }
}
