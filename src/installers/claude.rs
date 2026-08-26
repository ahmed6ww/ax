//! Claude Code Installer
//!
//! Writes into the locations Claude Code documents at
//! <https://code.claude.com/docs/en/skills>:
//!
//! - subagent:  `<scope>/agents/<name>.md`
//! - skills:    `<scope>/skills/<name>/SKILL.md`
//! - MCP:       `.mcp.json` (project) or `~/.claude.json` (user)
//!
//! where `<scope>` is `~/.claude` for user scope and `<repo>/.claude` for
//! project scope. Claude Desktop's Application Support directory is a different
//! product and is deliberately not used here.

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;

use super::common::{safe_join, skill_dir, write_atomic, write_atomic_preserving};
use super::{Capabilities, Installer, SettingsContribution};
use crate::utils::paths::{self, Scope};

pub struct ClaudeInstaller {
    scope: Scope,
}

impl ClaudeInstaller {
    pub fn new(scope: Scope) -> Self {
        Self { scope }
    }

    fn skills_dir(&self) -> Result<PathBuf> {
        paths::claude_skills_dir(self.scope)
    }

    /// Slash commands: `<scope>/commands/<name>.md`.
    fn commands_dir(&self) -> Result<PathBuf> {
        Ok(paths::claude_dir(self.scope)?.join("commands"))
    }

    /// Shared project settings — the file a team commits.
    fn settings_path(&self) -> Result<PathBuf> {
        Ok(paths::claude_dir(self.scope)?.join("settings.json"))
    }

    /// Where a bundle's own scripts are staged, so `settings.json` can point at
    /// a stable path and removal stays exact.
    fn bundle_stage_dir(&self) -> Result<PathBuf> {
        Ok(paths::claude_dir(self.scope)?.join(crate::core::bundle::BUNDLE_STAGE_DIR))
    }

    fn agents_dir(&self) -> Result<PathBuf> {
        paths::claude_agents_dir(self.scope)
    }
}

impl Installer for ClaudeInstaller {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            subagents: true,
            skills: true,
            mcp: true,
            project_scope: true,
            commands: true,
            hooks: true,
            permissions: true,
        }
    }

    fn skills_root(&self) -> Result<PathBuf> {
        self.skills_dir()
    }

    fn install_subagent(&self, name: &str, contents: &[u8]) -> Result<()> {
        let file = safe_join(&self.agents_dir()?, &format!("{}.md", name))?;
        write_atomic(&file, contents)
    }

    fn install_command(&self, name: &str, contents: &[u8]) -> Result<()> {
        let file = safe_join(&self.commands_dir()?, &format!("{}.md", name))?;
        write_atomic(&file, contents)
    }

    fn stage_bundle_files(
        &self,
        bundle: &str,
        files: &[(String, Vec<u8>)],
    ) -> Result<Option<PathBuf>> {
        if files.is_empty() {
            return Ok(None);
        }

        let dir = safe_join(&self.bundle_stage_dir()?, bundle)?;
        for (relative, bytes) in files {
            write_atomic(&safe_join(&dir, relative)?, bytes)?;
        }
        Ok(Some(dir))
    }

    fn apply_settings(
        &self,
        contribution: &SettingsContribution,
        previous: &SettingsContribution,
    ) -> Result<()> {
        if contribution.is_empty() && previous.is_empty() {
            return Ok(());
        }

        let path = self.settings_path()?;
        let mut settings = read_json_object(&path)?;

        apply_permissions(
            &mut settings,
            &contribution.permissions,
            &previous.permissions,
        )?;
        apply_hooks(&mut settings, &contribution.hooks, &previous.hooks)?;

        let rendered = format!("{}\n", serde_json::to_string_pretty(&settings)?);
        write_atomic_preserving(&path, rendered.as_bytes())
    }

    fn install_mcp(&self, tools: &[crate::core::agent::McpTool]) -> Result<()> {
        if tools.is_empty() {
            return Ok(());
        }

        let config_path = paths::claude_mcp_config(self.scope)?;

        // Preserve everything already in the file. `~/.claude.json` in
        // particular holds unrelated Claude Code state, and a parse failure
        // means we refuse rather than replace — silently discarding a user's
        // configured servers is worse than failing the install.
        let mut config: Value = if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read {}", config_path.display()))?;
            if content.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(&content).with_context(|| {
                    format!(
                        "{} is not valid JSON. agentpm will not overwrite it — \
                         fix or move the file, then retry.",
                        config_path.display()
                    )
                })?
            }
        } else {
            json!({})
        };

        if !config.is_object() {
            anyhow::bail!("{} does not contain a JSON object", config_path.display());
        }

        let servers = config
            .as_object_mut()
            .unwrap()
            .entry("mcpServers")
            .or_insert_with(|| Value::Object(Map::new()));

        if !servers.is_object() {
            anyhow::bail!(
                "\"mcpServers\" in {} is not a JSON object",
                config_path.display()
            );
        }

        for tool in tools {
            servers[&tool.name] = json!({
                "type": "stdio",
                "command": tool.command,
                "args": tool.args,
                "env": tool.env,
            });
        }

        let rendered = serde_json::to_string_pretty(&config)? + "\n";
        write_atomic_preserving(&config_path, rendered.as_bytes())
    }

    fn uninstall(&self, agent_name: &str) -> Result<()> {
        let agent_file = skill_dir(&self.agents_dir()?, &format!("{}.md", agent_name))?;
        if agent_file.exists() {
            fs::remove_file(&agent_file)?;
        }

        let skill_folder = skill_dir(&self.skills_dir()?, agent_name)?;
        if skill_folder.exists() {
            fs::remove_dir_all(&skill_folder)?;
        }

        Ok(())
    }

    fn location(&self) -> String {
        match self.skills_dir() {
            Ok(path) => crate::utils::paths::display_relative(&path),
            Err(_) => "<unresolved>".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// settings.json merging
//
// agentpm shares this file with the user, so a sync must add its own entries,
// remove the ones a previous sync added, and leave everything else untouched.
// Tracking the previous contribution in the lockfile — rather than tagging
// entries in the file — keeps settings.json free of agentpm-specific keys that
// Claude Code would not recognize.
// ---------------------------------------------------------------------------

/// Read a JSON object, refusing to proceed on a parse error.
fn read_json_object(path: &std::path::Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }

    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(json!({}));
    }

    let value: Value = serde_json::from_str(&content).with_context(|| {
        format!(
            "{} is not valid JSON. agentpm will not overwrite it — fix or move \
             the file, then retry.",
            path.display()
        )
    })?;

    if !value.is_object() {
        anyhow::bail!("{} does not contain a JSON object", path.display());
    }

    Ok(value)
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn apply_permissions(
    settings: &mut Value,
    wanted: &crate::core::bundle::Permissions,
    previous: &crate::core::bundle::Permissions,
) -> Result<()> {
    if wanted.is_empty() && previous.is_empty() {
        return Ok(());
    }

    let root = settings
        .as_object_mut()
        .expect("checked by read_json_object");
    let permissions = root
        .entry("permissions")
        .or_insert_with(|| Value::Object(Map::new()));

    if !permissions.is_object() {
        anyhow::bail!("\"permissions\" in settings.json is not a JSON object");
    }

    for (key, want, prev) in [
        ("allow", &wanted.allow, &previous.allow),
        ("deny", &wanted.deny, &previous.deny),
        ("ask", &wanted.ask, &previous.ask),
    ] {
        let mut rules = string_list(permissions.get(key));

        // Drop what the last sync contributed, keep the user's own rules.
        rules.retain(|rule| !prev.contains(rule));
        for rule in want {
            if !rules.contains(rule) {
                rules.push(rule.clone());
            }
        }

        if rules.is_empty() {
            permissions.as_object_mut().unwrap().remove(key);
        } else {
            permissions[key] = json!(rules);
        }
    }

    if permissions
        .as_object()
        .map(|o| o.is_empty())
        .unwrap_or(false)
    {
        root.remove("permissions");
    }

    Ok(())
}

fn apply_hooks(
    settings: &mut Value,
    wanted: &[(crate::core::bundle::Hook, String)],
    previous: &[(crate::core::bundle::Hook, String)],
) -> Result<()> {
    if wanted.is_empty() && previous.is_empty() {
        return Ok(());
    }

    let root = settings
        .as_object_mut()
        .expect("checked by read_json_object");
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));

    if !hooks.is_object() {
        anyhow::bail!("\"hooks\" in settings.json is not a JSON object");
    }

    // Commands a previous sync installed, so its entries can be recognized.
    let stale: Vec<&str> = previous.iter().map(|(_, cmd)| cmd.as_str()).collect();

    let mut events: Vec<String> = wanted.iter().map(|(h, _)| h.event.clone()).collect();
    events.extend(previous.iter().map(|(h, _)| h.event.clone()));
    events.sort();
    events.dedup();

    for event in events {
        let mut matchers: Vec<Value> = hooks
            .get(&event)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Strip previously contributed handlers, and any matcher group left empty.
        for group in matchers.iter_mut() {
            if let Some(list) = group.get_mut("hooks").and_then(|v| v.as_array_mut()) {
                list.retain(|entry| {
                    entry
                        .get("command")
                        .and_then(|c| c.as_str())
                        .map(|c| !stale.contains(&c))
                        .unwrap_or(true)
                });
            }
        }
        matchers.retain(|group| {
            group
                .get("hooks")
                .and_then(|v| v.as_array())
                .map(|l| !l.is_empty())
                .unwrap_or(false)
        });

        for (hook, command) in wanted.iter().filter(|(h, _)| h.event == event) {
            let entry = hook.to_settings_entry(command);
            let matcher = hook.matcher.as_deref();

            // Reuse an existing group with the same matcher so the file stays flat.
            let existing = matchers
                .iter_mut()
                .find(|group| group.get("matcher").and_then(|m| m.as_str()) == matcher);

            match existing {
                Some(group) => {
                    if let Some(list) = group.get_mut("hooks").and_then(|v| v.as_array_mut()) {
                        list.push(entry);
                    }
                }
                None => {
                    let mut group = json!({ "hooks": [entry] });
                    if let Some(m) = matcher {
                        group["matcher"] = json!(m);
                    }
                    matchers.push(group);
                }
            }
        }

        if matchers.is_empty() {
            hooks.as_object_mut().unwrap().remove(&event);
        } else {
            hooks[&event] = Value::Array(matchers);
        }
    }

    if hooks.as_object().map(|o| o.is_empty()).unwrap_or(false) {
        root.remove("hooks");
    }

    Ok(())
}
