//! Asserts that AX writes where Claude Code and Codex actually read.
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

use ax_lib::core::agent::{AgentConfig, Identity, McpTool, Skill};
use ax_lib::installers::{get_installer, Target};
use ax_lib::utils::paths::Scope;

/// Run `f` with HOME and the working directory pointed at a temp project.
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
    let prev_ax_home = std::env::var_os("AX_HOME");
    let prev_claude_dir = std::env::var_os("CLAUDE_CONFIG_DIR");

    // AX_HOME, not HOME: dirs::home_dir() ignores HOME on Windows, so an
    // earlier version of this harness wrote into the real home directory.
    std::env::set_var("AX_HOME", home.path());
    std::env::remove_var("CLAUDE_CONFIG_DIR");
    std::env::set_current_dir(&project).unwrap();

    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&project, home.path())));

    std::env::set_current_dir(prev_cwd).ok();
    match prev_ax_home {
        Some(v) => std::env::set_var("AX_HOME", v),
        None => std::env::remove_var("AX_HOME"),
    }
    if let Some(v) = prev_claude_dir {
        std::env::set_var("CLAUDE_CONFIG_DIR", v);
    }

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn sample_agent() -> AgentConfig {
    AgentConfig {
        name: "rust-architect".to_string(),
        version: "1.0.0".to_string(),
        description: "Senior Rust systems engineer".to_string(),
        author: "ahmed6ww".to_string(),
        identity: Identity {
            model: Some("claude-3-5-sonnet-latest".to_string()),
            icon: Some("🦀".to_string()),
            system_prompt: "You are a Rust expert.".to_string(),
        },
        skills: vec![Skill {
            name: "tokio-patterns".to_string(),
            description: Some("Async patterns for Tokio".to_string()),
            content: "# Tokio\n\nUse JoinSet.".to_string(),
            ..Default::default()
        }],
        mcp: vec![McpTool {
            name: "context7".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@upstash/context7-mcp".to_string()],
            env: HashMap::new(),
            setup_url: None,
        }],
    }
}

#[test]
fn claude_project_scope_matches_documented_layout() {
    in_temp_project(|project, _home| {
        let agent = sample_agent();
        let installer = get_installer(Target::Claude, Scope::Project);
        installer.install_identity(&agent).unwrap();
        installer.install_skills(&agent).unwrap();
        installer.install_tools(&agent).unwrap();

        // Documented: .claude/skills/<name>/SKILL.md
        let skill = project.join(".claude/skills/tokio-patterns/SKILL.md");
        assert!(skill.is_file(), "missing {}", skill.display());
        let body = fs::read_to_string(&skill).unwrap();
        assert!(body.starts_with("---\n"));
        assert!(body.contains("name: tokio-patterns"));
        assert!(body.contains("description: Async patterns for Tokio"));

        // Documented: .claude/agents/<name>.md
        let subagent = project.join(".claude/agents/rust-architect.md");
        assert!(subagent.is_file(), "missing {}", subagent.display());
        assert!(fs::read_to_string(&subagent).unwrap().contains("model: sonnet"));

        // Documented: .mcp.json at the project root, shared via version control
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
        let agent = sample_agent();
        let installer = get_installer(Target::Claude, Scope::User);
        installer.install_skills(&agent).unwrap();

        // ~/.claude/skills on every platform — never Application Support or %APPDATA%.
        assert!(home.join(".claude/skills/tokio-patterns/SKILL.md").is_file());
        assert!(!home.join("Library").exists(), "wrote to a Desktop-style path");
        assert!(!home.join(".claude/skills/synced").exists());
    });
}

#[test]
fn codex_writes_to_dot_agents_not_dot_codex() {
    in_temp_project(|project, home| {
        let agent = sample_agent();
        let installer = get_installer(Target::Codex, Scope::Project);
        installer.install_skills(&agent).unwrap();
        installer.install_tools(&agent).unwrap();

        // Documented: .agents/skills/<name>/SKILL.md
        assert!(project.join(".agents/skills/tokio-patterns/SKILL.md").is_file());

        // Codex does not scan ~/.codex/skills — nothing may land there.
        assert!(
            !home.join(".codex/skills").exists(),
            "wrote skills to a directory Codex never reads"
        );

        // Codex has no subagent concept, so the identity ships as a skill.
        let identity = project.join(".agents/skills/rust-architect-identity/SKILL.md");
        assert!(identity.is_file(), "identity was dropped");
        assert!(fs::read_to_string(&identity).unwrap().contains("You are a Rust expert."));

        // MCP still belongs in ~/.codex/config.toml.
        let cfg = home.join(".codex/config.toml");
        assert!(cfg.is_file(), "missing {}", cfg.display());
        let doc: toml::Table = fs::read_to_string(&cfg).unwrap().parse().unwrap();
        assert_eq!(doc["mcp_servers"]["context7"]["command"].as_str(), Some("npx"));
    });
}

#[test]
fn codex_config_survives_a_hostile_command_string() {
    in_temp_project(|_project, home| {
        let mut agent = sample_agent();
        // Under the previous string-concatenation writer this escaped the TOML
        // string and injected an extra server table.
        agent.mcp[0].command = "npx\"\n[mcp_servers.injected]\ncommand = \"rm".to_string();

        let installer = get_installer(Target::Codex, Scope::User);
        installer.install_tools(&agent).unwrap();

        let cfg = home.join(".codex/config.toml");
        let doc: toml::Table = fs::read_to_string(&cfg).unwrap().parse().unwrap();
        let servers = doc["mcp_servers"].as_table().unwrap();
        assert!(!servers.contains_key("injected"), "TOML injection succeeded");
        assert_eq!(servers.len(), 1);
    });
}

#[test]
fn a_traversing_skill_name_is_refused() {
    in_temp_project(|project, _home| {
        let mut agent = sample_agent();
        agent.skills[0].name = "../../../../pwned".to_string();

        for target in Target::all() {
            let installer = get_installer(target, Scope::Project);
            assert!(
                installer.install_skills(&agent).is_err(),
                "{} accepted a traversing skill name",
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

        let installer = get_installer(Target::Claude, Scope::Project);
        let err = installer.install_tools(&sample_agent()).unwrap_err();
        assert!(format!("{:#}", err).contains("not valid JSON"));

        // The user's file is untouched rather than reset to {}.
        assert_eq!(fs::read_to_string(&mcp).unwrap(), "{ this is not json");
    });
}

#[test]
fn uninstall_removes_exactly_what_install_wrote() {
    in_temp_project(|project, _home| {
        let agent = sample_agent();

        for target in Target::all() {
            let installer = get_installer(target, Scope::Project);
            installer.install_skills(&agent).unwrap();
            installer.uninstall(&agent.name).unwrap();
        }

        assert!(!project
            .join(".agents/skills/rust-architect-identity")
            .exists());
    });
}
