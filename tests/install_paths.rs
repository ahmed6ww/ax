//! Asserts that agentpm writes where Claude Code and Codex actually read.
//!
//! These tests exist because the paths were wrong in every release up to 1.5.0:
//! Claude Code skills went to the Claude Desktop directory on macOS and
//! Windows, and Codex skills went to `~/.codex/skills`, which Codex never
//! scans. Both produced installs that reported success and were never loaded.
//!
//! The expected layouts below are transcribed from vendor documentation:
//!   Claude Code — https://code.claude.com/docs/en/skills
//!   Codex       — https://learn.chatgpt.com/docs/build-skills

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use agentpm_lib::core::agent::McpTool;
use agentpm_lib::installers::{get_installer, Target};
use agentpm_lib::utils::paths::Scope;

/// Run `f` with the home directory and working directory pointed at a sandbox.
///
/// Serialized because both are process-global.
fn in_temp_project<F: FnOnce(&Path, &Path)>(f: F) {
    use std::sync::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let home = tempfile::tempdir().unwrap();
    let project = home.path().join("work").join("repo");
    fs::create_dir_all(project.join(".git")).unwrap();

    let prev_cwd = std::env::current_dir().unwrap();
    let prev_home = std::env::var_os("AGENTPM_HOME");
    let prev_claude_dir = std::env::var_os("CLAUDE_CONFIG_DIR");

    // AGENTPM_HOME, not HOME: dirs::home_dir() ignores HOME on Windows, and an
    // earlier version of this harness wrote into the real ~/.claude and
    // ~/.codex as a result.
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
    if let Some(v) = prev_claude_dir {
        std::env::set_var("CLAUDE_CONFIG_DIR", v);
    }

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn skill_files() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "SKILL.md".to_string(),
            b"---\nname: tokio-patterns\ndescription: Async patterns\n---\n\nUse JoinSet.".to_vec(),
        ),
        ("references/notes.md".to_string(), b"# Notes\n".to_vec()),
    ]
}

fn context7() -> McpTool {
    McpTool {
        name: "context7".to_string(),
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "@upstash/context7-mcp".to_string()],
        env: HashMap::new(),
        setup_url: None,
    }
}

#[test]
fn claude_project_scope_matches_documented_layout() {
    in_temp_project(|project, _home| {
        let installer = get_installer(Target::Claude, Scope::Project);
        installer
            .install_files("tokio-patterns", &skill_files())
            .unwrap();
        installer
            .install_subagent("api-reviewer", b"---\nname: api-reviewer\n---\nReview.")
            .unwrap();
        installer.install_mcp(&[context7()]).unwrap();

        // Documented: .claude/skills/<name>/SKILL.md, with supporting files.
        let skill = project.join(".claude/skills/tokio-patterns/SKILL.md");
        assert!(skill.is_file(), "missing {}", skill.display());
        assert!(fs::read_to_string(&skill).unwrap().contains("Use JoinSet."));
        assert!(project
            .join(".claude/skills/tokio-patterns/references/notes.md")
            .is_file());

        // Documented: .claude/agents/<name>.md
        assert!(project.join(".claude/agents/api-reviewer.md").is_file());

        // Documented: .mcp.json at the project root, shared via version control.
        let mcp = project.join(".mcp.json");
        assert!(mcp.is_file(), "missing {}", mcp.display());
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&mcp).unwrap()).unwrap();
        assert_eq!(value["mcpServers"]["context7"]["command"], "npx");
    });
}

#[test]
fn claude_user_scope_uses_dot_claude_in_home() {
    in_temp_project(|_project, home| {
        let installer = get_installer(Target::Claude, Scope::User);
        installer
            .install_files("tokio-patterns", &skill_files())
            .unwrap();

        // ~/.claude/skills on every platform — never Application Support or %APPDATA%.
        assert!(home
            .join(".claude/skills/tokio-patterns/SKILL.md")
            .is_file());
        assert!(
            !home.join("Library").exists(),
            "wrote to a Claude Desktop path"
        );
        assert!(!home.join(".claude/skills/synced").exists());
    });
}

#[test]
fn codex_writes_to_dot_agents_not_dot_codex() {
    in_temp_project(|project, home| {
        let installer = get_installer(Target::Codex, Scope::Project);
        installer
            .install_files("tokio-patterns", &skill_files())
            .unwrap();
        installer.install_mcp(&[context7()]).unwrap();

        // Documented: .agents/skills/<name>/SKILL.md
        assert!(project
            .join(".agents/skills/tokio-patterns/SKILL.md")
            .is_file());

        // Codex does not scan ~/.codex/skills — nothing may land there.
        assert!(
            !home.join(".codex/skills").exists(),
            "wrote skills to a directory Codex never reads"
        );

        // MCP still belongs in ~/.codex/config.toml.
        let cfg = home.join(".codex/config.toml");
        assert!(cfg.is_file(), "missing {}", cfg.display());
        let doc: toml::Table = fs::read_to_string(&cfg).unwrap().parse().unwrap();
        assert_eq!(
            doc["mcp_servers"]["context7"]["command"].as_str(),
            Some("npx")
        );
    });
}

#[test]
fn codex_config_survives_a_hostile_command_string() {
    in_temp_project(|_project, home| {
        let mut tool = context7();
        // Under the previous string-concatenation writer this escaped the TOML
        // string and injected an extra server table.
        tool.command = "npx\"\n[mcp_servers.injected]\ncommand = \"rm".to_string();

        get_installer(Target::Codex, Scope::User)
            .install_mcp(&[tool])
            .unwrap();

        let cfg = home.join(".codex/config.toml");
        let doc: toml::Table = fs::read_to_string(&cfg).unwrap().parse().unwrap();
        let servers = doc["mcp_servers"].as_table().unwrap();
        assert!(
            !servers.contains_key("injected"),
            "TOML injection succeeded"
        );
        assert_eq!(servers.len(), 1);
    });
}

#[test]
fn a_traversing_name_is_refused_by_every_target() {
    in_temp_project(|project, _home| {
        for target in Target::all() {
            let installer = get_installer(target, Scope::Project);
            assert!(
                installer
                    .install_files("../../../../pwned", &skill_files())
                    .is_err(),
                "{} accepted a traversing name",
                target.display_name()
            );
            // A traversing path inside the payload must be refused too.
            assert!(
                installer
                    .install_files("ok", &[("../../escape.md".to_string(), b"x".to_vec())])
                    .is_err(),
                "{} accepted a traversing file path",
                target.display_name()
            );
        }
        assert!(!project.join("../../../../pwned").exists());
    });
}

#[test]
fn a_corrupt_existing_config_is_never_silently_replaced() {
    in_temp_project(|project, _home| {
        let mcp = project.join(".mcp.json");
        fs::write(&mcp, "{ this is not json").unwrap();

        let err = get_installer(Target::Claude, Scope::Project)
            .install_mcp(&[context7()])
            .unwrap_err();
        assert!(format!("{:#}", err).contains("not valid JSON"));

        // The user's file is untouched rather than reset to an empty object.
        assert_eq!(fs::read_to_string(&mcp).unwrap(), "{ this is not json");
    });
}

#[test]
fn pruning_removes_a_skill_the_manifest_dropped() {
    in_temp_project(|project, _home| {
        for target in Target::all() {
            let installer = get_installer(target, Scope::Project);
            installer
                .install_files("tokio-patterns", &skill_files())
                .unwrap();

            assert!(installer.remove_skill("tokio-patterns").unwrap());
            // Removing something already gone is not an error.
            assert!(!installer.remove_skill("tokio-patterns").unwrap());
        }

        assert!(!project.join(".claude/skills/tokio-patterns").exists());
        assert!(!project.join(".agents/skills/tokio-patterns").exists());
    });
}

#[test]
fn a_traversing_path_inside_a_bundle_stage_is_refused() {
    in_temp_project(|project, _home| {
        let installer = get_installer(Target::Claude, Scope::Project);
        assert!(installer
            .stage_bundle_files("b", &[("../../../out.sh".to_string(), b"x".to_vec())])
            .is_err());
        assert!(!project.join("../../../out.sh").exists());
    });
}

#[test]
fn removing_a_skill_leaves_its_siblings_alone() {
    in_temp_project(|project, _home| {
        let installer = get_installer(Target::Claude, Scope::Project);
        installer.install_files("keep-me", &skill_files()).unwrap();
        installer.install_files("drop-me", &skill_files()).unwrap();

        assert!(installer.remove_skill("drop-me").unwrap());

        assert!(project.join(".claude/skills/keep-me/SKILL.md").is_file());
        assert!(!project.join(".claude/skills/drop-me").exists());
    });
}
