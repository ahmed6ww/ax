//! Bundle installation across both targets.
//!
//! The settings merge is the risky part: `settings.json` is shared with the
//! user, so agentpm must add its own entries, remove the ones a previous sync
//! added, and never touch anything else. These tests pin that behaviour.
//!
//! Schemas are transcribed from vendor documentation:
//!   subagents   — https://code.claude.com/docs/en/sub-agents
//!   settings    — https://code.claude.com/docs/en/settings
//!   hooks       — https://code.claude.com/docs/en/hooks

use std::fs;
use std::path::Path;

use agentpm_lib::core::bundle::{BundleManifest, Hook, Permissions};
use agentpm_lib::installers::{get_installer, SettingsContribution, Target};
use agentpm_lib::utils::paths::Scope;

fn in_temp_project<F: FnOnce(&Path, &Path)>(f: F) {
    use std::sync::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let home = tempfile::tempdir().unwrap();
    let project = home.path().join("repo");
    fs::create_dir_all(project.join(".git")).unwrap();

    let prev_cwd = std::env::current_dir().unwrap();
    let prev_home = std::env::var_os("AGENTPM_HOME");
    let prev_claude = std::env::var_os("CLAUDE_CONFIG_DIR");

    std::env::set_var("AGENTPM_HOME", home.path());
    std::env::remove_var("CLAUDE_CONFIG_DIR");
    std::env::set_current_dir(&project).unwrap();

    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&project, home.path())));

    std::env::set_current_dir(prev_cwd).ok();
    match prev_home {
        Some(v) => std::env::set_var("AGENTPM_HOME", v),
        None => std::env::remove_var("AGENTPM_HOME"),
    }
    if let Some(v) = prev_claude {
        std::env::set_var("CLAUDE_CONFIG_DIR", v);
    }

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

const BUNDLE: &str = r#"
name = "backend"
description = "FastAPI working environment"

skills = ["skills/fastapi-tdd"]
agents = ["agents/api-reviewer.md"]
commands = ["commands/migrate.md"]
scripts = ["hooks/guard.sh"]

[mcp.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]

[permissions]
allow = ["Bash(pytest:*)", "Bash(ruff:*)"]
deny = ["Read(./.env)"]

[[hooks]]
event = "PreToolUse"
matcher = "Bash"
command = "hooks/guard.sh"
timeout = 30
"#;

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn contribution(manifest: &BundleManifest, script_path: &str) -> SettingsContribution {
    SettingsContribution {
        permissions: manifest.permissions.clone(),
        hooks: manifest
            .hooks
            .iter()
            .map(|h| (h.clone(), script_path.to_string()))
            .collect(),
    }
}

#[test]
fn claude_receives_every_surface_a_bundle_declares() {
    in_temp_project(|project, _home| {
        let manifest = BundleManifest::parse(BUNDLE).unwrap();
        let installer = get_installer(Target::Claude, Scope::Project);

        installer
            .install_files(
                "fastapi-tdd",
                &[(
                    "SKILL.md".to_string(),
                    b"---\nname: fastapi-tdd\n---\n".to_vec(),
                )],
            )
            .unwrap();
        installer
            .install_subagent(
                "api-reviewer",
                b"---\nname: api-reviewer\n---\nReview APIs.",
            )
            .unwrap();
        installer
            .install_command("migrate", b"Run the migration.")
            .unwrap();

        let staged = installer
            .stage_bundle_files(
                "backend",
                &[("hooks/guard.sh".to_string(), b"#!/bin/sh\n".to_vec())],
            )
            .unwrap()
            .expect("scripts were staged");
        let script = staged.join("hooks/guard.sh");
        assert!(script.is_file(), "hook script missing");

        installer
            .apply_settings(
                &contribution(&manifest, &script.display().to_string()),
                &SettingsContribution::default(),
            )
            .unwrap();
        installer
            .install_mcp(&[manifest.mcp["context7"].to_tool("context7")])
            .unwrap();

        // Documented locations, all under the project's .claude directory.
        assert!(project
            .join(".claude/skills/fastapi-tdd/SKILL.md")
            .is_file());
        assert!(project.join(".claude/agents/api-reviewer.md").is_file());
        assert!(project.join(".claude/commands/migrate.md").is_file());
        assert!(project.join(".mcp.json").is_file());

        let settings = read_json(&project.join(".claude/settings.json"));
        assert_eq!(settings["permissions"]["allow"][0], "Bash(pytest:*)");
        assert_eq!(settings["permissions"]["deny"][0], "Read(./.env)");

        let group = &settings["hooks"]["PreToolUse"][0];
        assert_eq!(group["matcher"], "Bash");
        assert_eq!(group["hooks"][0]["type"], "command");
        assert_eq!(group["hooks"][0]["timeout"], 30);
        assert_eq!(
            group["hooks"][0]["command"].as_str().unwrap(),
            script.display().to_string()
        );
    });
}

#[test]
fn re_applying_does_not_duplicate_entries() {
    in_temp_project(|project, _home| {
        let manifest = BundleManifest::parse(BUNDLE).unwrap();
        let installer = get_installer(Target::Claude, Scope::Project);
        let script = "/staged/guard.sh";
        let want = contribution(&manifest, script);

        installer
            .apply_settings(&want, &SettingsContribution::default())
            .unwrap();
        // A second sync passes the previous contribution so it is removed first.
        installer.apply_settings(&want, &want).unwrap();
        installer.apply_settings(&want, &want).unwrap();

        let settings = read_json(&project.join(".claude/settings.json"));
        assert_eq!(
            settings["permissions"]["allow"].as_array().unwrap().len(),
            2
        );
        assert_eq!(settings["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
        assert_eq!(
            settings["hooks"]["PreToolUse"][0]["hooks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    });
}

#[test]
fn removing_a_bundle_leaves_the_users_own_settings_intact() {
    in_temp_project(|project, _home| {
        let settings_path = project.join(".claude/settings.json");
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(
            &settings_path,
            r#"{
  "permissions": { "allow": ["Bash(git status)"], "deny": ["Read(./secrets)"] },
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [{ "type": "command", "command": "/mine/own.sh" }] }
    ]
  },
  "model": "opus"
}"#,
        )
        .unwrap();

        let manifest = BundleManifest::parse(BUNDLE).unwrap();
        let installer = get_installer(Target::Claude, Scope::Project);
        let want = contribution(&manifest, "/staged/guard.sh");

        installer
            .apply_settings(&want, &SettingsContribution::default())
            .unwrap();

        let merged = read_json(&settings_path);
        assert_eq!(merged["permissions"]["allow"].as_array().unwrap().len(), 3);
        assert_eq!(merged["model"], "opus", "unrelated keys must survive");

        // Now remove the bundle: pass its contribution as previous, nothing as wanted.
        installer
            .apply_settings(&SettingsContribution::default(), &want)
            .unwrap();

        let after = read_json(&settings_path);
        assert_eq!(
            after["permissions"]["allow"].as_array().unwrap(),
            &vec![serde_json::json!("Bash(git status)")],
            "the user's own allow rule must remain"
        );
        assert_eq!(after["permissions"]["deny"].as_array().unwrap().len(), 1);
        assert_eq!(after["model"], "opus");

        let hooks = after["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0]["hooks"][0]["command"], "/mine/own.sh");
    });
}

#[test]
fn codex_reports_what_it_cannot_take_rather_than_dropping_it() {
    in_temp_project(|_project, _home| {
        let manifest = BundleManifest::parse(BUNDLE).unwrap();
        let caps = get_installer(Target::Codex, Scope::Project).capabilities();

        assert!(caps.skills && caps.mcp);
        assert!(!caps.subagents && !caps.commands && !caps.hooks && !caps.permissions);

        let skipped = caps.unsupported(&manifest);
        let joined = skipped.join(", ");
        assert!(joined.contains("1 subagent"), "{}", joined);
        assert!(joined.contains("1 command"), "{}", joined);
        assert!(joined.contains("1 hook"), "{}", joined);
        assert!(joined.contains("3 permission rules"), "{}", joined);

        // Claude takes all of it, so nothing is reported as skipped.
        let claude = get_installer(Target::Claude, Scope::Project).capabilities();
        assert!(claude.unsupported(&manifest).is_empty());
    });
}

#[test]
fn a_corrupt_settings_file_is_never_silently_replaced() {
    in_temp_project(|project, _home| {
        let settings_path = project.join(".claude/settings.json");
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(&settings_path, "{ not json").unwrap();

        let installer = get_installer(Target::Claude, Scope::Project);
        let want = SettingsContribution {
            permissions: Permissions {
                allow: vec!["Bash(ls)".to_string()],
                ..Default::default()
            },
            hooks: Vec::new(),
        };

        let err = installer
            .apply_settings(&want, &SettingsContribution::default())
            .unwrap_err();
        assert!(format!("{:#}", err).contains("not valid JSON"));
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), "{ not json");
    });
}

#[test]
fn a_hook_script_cannot_escape_the_staging_directory() {
    in_temp_project(|project, _home| {
        let installer = get_installer(Target::Claude, Scope::Project);
        let escaped = installer.stage_bundle_files(
            "backend",
            &[("../../../../pwned.sh".to_string(), b"x".to_vec())],
        );
        assert!(escaped.is_err(), "traversal accepted");
        assert!(!project.join("../../../../pwned.sh").exists());
    });
}

#[test]
fn hooks_sharing_a_matcher_land_in_one_group() {
    in_temp_project(|project, _home| {
        let installer = get_installer(Target::Claude, Scope::Project);
        let hook = |command: &str| Hook {
            event: "PreToolUse".to_string(),
            matcher: Some("Bash".to_string()),
            command: command.to_string(),
            timeout: None,
            status_message: None,
        };

        let want = SettingsContribution {
            permissions: Permissions::default(),
            hooks: vec![
                (hook("a.sh"), "/staged/a.sh".to_string()),
                (hook("b.sh"), "/staged/b.sh".to_string()),
            ],
        };
        installer
            .apply_settings(&want, &SettingsContribution::default())
            .unwrap();

        let settings = read_json(&project.join(".claude/settings.json"));
        let groups = settings["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(
            groups.len(),
            1,
            "same matcher must not create a second group"
        );
        assert_eq!(groups[0]["hooks"].as_array().unwrap().len(), 2);
    });
}
