//! `source = "local"` — a skill authored in the project itself, not fetched.
//!
//! One developer commits the skill directory alongside `axur.toml`; every
//! other machine's `axur sync` installs it the same way it installs anything
//! else declared there — no GitHub round trip, no separate mechanism to learn.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

/// Serializes access to the process-global CWD and `AXUR_HOME`, and points
/// both at a sandbox for the duration of one test.
static LOCK: Mutex<()> = Mutex::new(());

struct Sandbox {
    project: PathBuf,
    prev_cwd: PathBuf,
    prev_home: Option<std::ffi::OsString>,
    prev_claude_dir: Option<std::ffi::OsString>,
    _home: tempfile::TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Sandbox {
    fn new() -> Self {
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("work").join("repo");
        fs::create_dir_all(project.join(".git")).unwrap();

        let prev_cwd = std::env::current_dir().unwrap();
        let prev_home = std::env::var_os("AXUR_HOME");
        let prev_claude_dir = std::env::var_os("CLAUDE_CONFIG_DIR");

        std::env::set_var("AXUR_HOME", home.path());
        std::env::remove_var("CLAUDE_CONFIG_DIR");
        std::env::set_current_dir(&project).unwrap();

        Self {
            project,
            prev_cwd,
            prev_home,
            prev_claude_dir,
            _home: home,
            _guard: guard,
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.prev_cwd).ok();
        match self.prev_home.take() {
            Some(v) => std::env::set_var("AXUR_HOME", v),
            None => std::env::remove_var("AXUR_HOME"),
        }
        if let Some(v) = self.prev_claude_dir.take() {
            std::env::set_var("CLAUDE_CONFIG_DIR", v);
        }
    }
}

fn write_skill(dir: &std::path::Path) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        "---\nname: onboarding\ndescription: How this team works\n---\n\nRead this first.",
    )
    .unwrap();
}

#[tokio::test]
async fn a_local_skill_installs_on_both_targets_without_a_network_call() {
    let sandbox = Sandbox::new();
    let project = &sandbox.project;

    write_skill(&project.join(".axur/skills/onboarding"));
    fs::write(
        project.join("axur.toml"),
        r#"[targets]
agents = ["claude-code", "codex"]
scope = "project"

[skills]
onboarding = { source = "local", path = ".axur/skills/onboarding" }
"#,
    )
    .unwrap();

    axur_lib::cli::commands::sync::execute(false, false, true, false, None)
        .await
        .unwrap();

    assert!(project.join(".claude/skills/onboarding/SKILL.md").is_file());
    assert!(project.join(".agents/skills/onboarding/SKILL.md").is_file());

    let lock = fs::read_to_string(project.join("axur.lock")).unwrap();
    assert!(lock.contains("source = \"local\""), "{}", lock);
}

#[tokio::test]
async fn a_local_skill_missing_from_disk_fails_sync_clearly() {
    let sandbox = Sandbox::new();
    let project = &sandbox.project;

    fs::write(
        project.join("axur.toml"),
        r#"[targets]
agents = ["codex"]
scope = "project"

[skills]
onboarding = { source = "local", path = ".axur/skills/onboarding" }
"#,
    )
    .unwrap();

    let err = axur_lib::cli::commands::sync::execute(false, false, true, false, None)
        .await
        .unwrap_err();
    assert!(format!("{:#}", err).contains("No directory"), "{:#}", err);
}
