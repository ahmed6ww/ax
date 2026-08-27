//! Undo journal for a multi-target install.
//!
//! `axur sync` writes to every configured target in turn, and until now a
//! failure partway through left the run split in half: Claude Code fully
//! written, Codex untouched, and no lockfile — because the lock is only saved
//! once every target succeeds. The next run then had no record of what was on
//! disk, so nothing pruned it.
//!
//! This records each mutation before it happens and can put the tree back. It
//! is deliberately not a general transaction system: it covers the writes
//! axur itself performs during one install, on one thread, and nothing else.
//! Concurrent modification by another process is out of scope — the window is
//! milliseconds and the alternative is a lock file nobody would clean up.
//!
//! The journal lives in a thread-local so the write helpers can record without
//! every installer method growing an extra parameter. Installs are sequential
//! within a run, and a thread-local keeps parallel tests independent.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

thread_local! {
    static JOURNAL: RefCell<Option<Vec<Entry>>> = const { RefCell::new(None) };
}

/// Suffix for a directory moved aside rather than deleted outright.
const TRASH_SUFFIX: &str = ".axur-trash";

#[derive(Debug)]
enum Entry {
    /// The file did not exist beforehand: delete it to undo.
    Created(PathBuf),
    /// The file existed: restore these bytes to undo.
    Replaced(PathBuf, Vec<u8>),
    /// The directory did not exist beforehand: remove it to undo. Only ever
    /// removed when empty, so a rollback cannot take anything with it.
    Dir(PathBuf),
    /// A directory tree renamed aside. Undone by renaming it back; committed
    /// by deleting it. Deletion is deferred precisely so it can be undone.
    Trashed { original: PathBuf, holding: PathBuf },
}

/// An open journal. Rolls back when dropped unless [`Guard::commit`] is called.
///
/// Drop-based rollback is what makes this safe against the `?` operator: every
/// early return in the install path unwinds through here.
pub struct Guard {
    /// False when a journal was already open on this thread: the guard then
    /// defers entirely to the outer one and does nothing itself.
    owns: bool,
    committed: bool,
}

/// Begin recording.
///
/// Nesting is a programming error - axur opens exactly one journal per run.
/// Rather than clobber the outer journal and lose its undo information, an
/// inner scope becomes a no-op and the outer one stays in charge.
pub fn begin() -> Guard {
    let owns = JOURNAL.with(|j| {
        let mut slot = j.borrow_mut();
        if slot.is_some() {
            debug_assert!(false, "a journal is already open on this thread");
            return false;
        }
        *slot = Some(Vec::new());
        true
    });
    Guard {
        owns,
        committed: false,
    }
}

impl Guard {
    /// Accept every recorded change: drop the undo information and delete the
    /// trees that were moved aside.
    pub fn commit(mut self) {
        self.committed = true;
        if !self.owns {
            return;
        }
        if let Some(entries) = JOURNAL.with(|j| j.borrow_mut().take()) {
            for entry in entries {
                if let Entry::Trashed { holding, .. } = entry {
                    // Best effort: the install already succeeded, and failing
                    // the run over an undeletable holding directory would be
                    // worse than leaving it. It is named so it is obvious.
                    let _ = fs::remove_dir_all(&holding);
                }
            }
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.committed || !self.owns {
            return;
        }
        if let Some(entries) = JOURNAL.with(|j| j.borrow_mut().take()) {
            rollback(entries);
        }
    }
}

/// Undo in reverse order, so a file restored into a directory is put back
/// before that directory is considered for removal.
///
/// Every step is best effort. A rollback runs while something has already gone
/// wrong; failing it would replace one reported error with a less useful one.
fn rollback(entries: Vec<Entry>) {
    for entry in entries.into_iter().rev() {
        match entry {
            Entry::Created(path) => {
                let _ = fs::remove_file(&path);
            }
            Entry::Replaced(path, bytes) => {
                let _ = fs::write(&path, &bytes);
            }
            Entry::Dir(path) => {
                // remove_dir, never remove_dir_all: if anything landed here
                // that the journal does not know about, it stays.
                let _ = fs::remove_dir(&path);
            }
            Entry::Trashed { original, holding } => {
                let _ = fs::rename(&holding, &original);
            }
        }
    }
}

fn active() -> bool {
    JOURNAL.with(|j| j.borrow().is_some())
}

fn push(entry: Entry) {
    JOURNAL.with(|j| {
        if let Some(entries) = j.borrow_mut().as_mut() {
            entries.push(entry);
        }
    });
}

/// True when `path` already has an entry, so only its original state is kept.
///
/// A skill written twice in one run must roll back to what was there before
/// the *first* write, not to the intermediate state.
fn already_recorded(path: &Path) -> bool {
    JOURNAL.with(|j| {
        j.borrow()
            .as_ref()
            .map(|entries| {
                entries.iter().any(|e| match e {
                    Entry::Created(p) | Entry::Replaced(p, _) => p == path,
                    _ => false,
                })
            })
            .unwrap_or(false)
    })
}

/// Record a file's current state before it is written.
///
/// Call this immediately before the write. A no-op when no journal is open, so
/// the write helpers stay usable outside an install.
pub fn record_write(path: &Path) {
    if !active() || already_recorded(path) {
        return;
    }
    match fs::read(path) {
        Ok(previous) => push(Entry::Replaced(path.to_path_buf(), previous)),
        // Unreadable is treated as absent: the undo then deletes it, which is
        // the right outcome for a path axur is about to create.
        Err(_) => push(Entry::Created(path.to_path_buf())),
    }
}

/// Create `dir` and every missing ancestor, recording which ones were new.
///
/// Ancestors that already existed are left out, so a rollback never tries to
/// remove a directory it did not create.
pub fn create_dir_all(dir: &Path) -> Result<()> {
    if dir.exists() {
        return Ok(());
    }

    if active() {
        let mut missing = Vec::new();
        let mut cursor = Some(dir);
        while let Some(path) = cursor {
            if path.exists() {
                break;
            }
            missing.push(path.to_path_buf());
            cursor = path.parent();
        }
        // Recorded outermost-first so the reversed rollback removes the
        // deepest directory first.
        for path in missing.into_iter().rev() {
            push(Entry::Dir(path));
        }
    }

    fs::create_dir_all(dir).with_context(|| format!("Failed to create {}", dir.display()))
}

/// Remove a directory tree, deferring the deletion while a journal is open.
///
/// Under a journal the tree is renamed to a sibling holding directory and only
/// deleted on commit, so a later failure in the same run can put it back.
pub fn remove_dir_all(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    if !active() {
        return fs::remove_dir_all(dir)
            .with_context(|| format!("Failed to remove {}", dir.display()));
    }

    let holding = holding_path(dir)?;
    fs::rename(dir, &holding)
        .with_context(|| format!("Failed to move {} aside before removing it", dir.display()))?;
    push(Entry::Trashed {
        original: dir.to_path_buf(),
        holding,
    });
    Ok(())
}

/// A sibling path to hold a removed tree, distinct from anything present.
fn holding_path(dir: &Path) -> Result<PathBuf> {
    let parent = dir
        .parent()
        .context("Refusing to remove a path with no parent directory")?;
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .context("Refusing to remove a path with no file name")?;

    for n in 0..1_000 {
        let candidate = if n == 0 {
            parent.join(format!(".{}{}", name, TRASH_SUFFIX))
        } else {
            parent.join(format!(".{}{}.{}", name, TRASH_SUFFIX, n))
        };
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "Could not find a free holding path beside {}",
        dir.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    #[test]
    fn a_dropped_guard_deletes_files_it_created() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("SKILL.md");

        {
            let _guard = begin();
            record_write(&file);
            fs::write(&file, "new").unwrap();
            assert!(file.exists());
        }

        assert!(!file.exists(), "a rolled-back create should be undone");
    }

    #[test]
    fn a_dropped_guard_restores_replaced_content() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.toml");
        write(&file, "original");

        {
            let _guard = begin();
            record_write(&file);
            fs::write(&file, "rewritten").unwrap();
        }

        assert_eq!(fs::read_to_string(&file).unwrap(), "original");
    }

    #[test]
    fn only_the_first_state_of_a_path_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("SKILL.md");
        write(&file, "original");

        {
            let _guard = begin();
            record_write(&file);
            fs::write(&file, "second").unwrap();
            record_write(&file);
            fs::write(&file, "third").unwrap();
        }

        assert_eq!(fs::read_to_string(&file).unwrap(), "original");
    }

    #[test]
    fn a_committed_guard_keeps_everything() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("SKILL.md");

        let guard = begin();
        record_write(&file);
        fs::write(&file, "kept").unwrap();
        guard.commit();

        assert_eq!(fs::read_to_string(&file).unwrap(), "kept");
    }

    #[test]
    fn rollback_removes_directories_it_created_but_not_ones_it_found() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("skills");
        fs::create_dir_all(&existing).unwrap();
        let nested = existing.join("a").join("b");

        {
            let _guard = begin();
            create_dir_all(&nested).unwrap();
            assert!(nested.exists());
        }

        assert!(!existing.join("a").exists(), "created dirs should be gone");
        assert!(existing.exists(), "a pre-existing dir must survive");
    }

    #[test]
    fn a_removed_tree_comes_back_on_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join("old-skill");
        write(&skill.join("SKILL.md"), "body");

        {
            let _guard = begin();
            remove_dir_all(&skill).unwrap();
            assert!(!skill.exists(), "removal is visible inside the scope");
        }

        assert_eq!(
            fs::read_to_string(skill.join("SKILL.md")).unwrap(),
            "body",
            "a rolled-back prune should restore the skill"
        );
    }

    #[test]
    fn a_committed_removal_leaves_no_holding_directory() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join("old-skill");
        write(&skill.join("SKILL.md"), "body");

        let guard = begin();
        remove_dir_all(&skill).unwrap();
        guard.commit();

        assert!(!skill.exists());
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("axur-trash"))
            .collect();
        assert!(leftovers.is_empty(), "commit must clear the holding dir");
    }

    #[test]
    fn the_helpers_work_with_no_journal_open() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("x").join("y");
        create_dir_all(&nested).unwrap();
        assert!(nested.exists());

        record_write(&nested.join("f"));
        fs::write(nested.join("f"), "kept").unwrap();

        remove_dir_all(&nested).unwrap();
        assert!(!nested.exists(), "removal is immediate without a journal");
    }
}
