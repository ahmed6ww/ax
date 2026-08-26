//! `agentpm install` — install an agent into Claude Code and/or Codex.

use anyhow::{Context, Result};

use crate::core::agent::AgentConfig;
use crate::core::registry::Registry;
use crate::installers::{get_installer, Target};
use crate::utils::paths::Scope;
use crate::utils::{ui, validation};

use super::super::TargetArg;

/// Resolve which targets to act on: the one named, or every detected target.
fn resolve_targets(target: Option<TargetArg>) -> Result<Vec<Target>> {
    if let Some(t) = target {
        return Ok(vec![t.into()]);
    }

    let detected: Vec<Target> = Target::all()
        .into_iter()
        .filter(|t| match t {
            Target::Claude => crate::utils::paths::claude_detected(),
            Target::Codex => crate::utils::paths::codex_detected(),
        })
        .collect();

    if detected.is_empty() {
        anyhow::bail!(
            "No supported agent detected. Install Claude Code or Codex, \
             or name a target explicitly with --target."
        );
    }

    Ok(detected)
}

pub async fn execute(agent_name: &str, target: Option<TargetArg>, global: bool) -> Result<()> {
    let targets = resolve_targets(target)?;
    let scope = Scope::from_global_flag(global);

    ui::intro(&format!("agentpm install {}", agent_name));

    let spinner = ui::Spinner::start("Fetching agent configuration…");
    let registry = Registry::new();
    let agent: AgentConfig = registry
        .fetch_agent(agent_name)
        .await
        .with_context(|| format!("Could not resolve agent '{}'", agent_name))?;
    spinner.stop(&format!(
        "{} {}",
        ui::bold(&agent.name),
        ui::dim(&format!("v{}", agent.version))
    ));

    let missing = validation::check_agent_dependencies(&agent);
    if missing.is_empty() {
        ui::step("All dependencies satisfied");
    } else {
        let detail = missing
            .iter()
            .map(|tool| {
                let hint = validation::get_install_hint(tool)
                    .map(|h| format!("  {}", ui::dim(h)))
                    .unwrap_or_default();
                format!("{} not on PATH{}", ui::bold(tool), hint)
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        ui::warning(&format!(
            "{} missing — those MCP servers will not start
{}",
            missing.len(),
            detail
        ));
    }

    let agent = super::prompt_for_api_keys(agent)?;

    for target in targets {
        let installer = get_installer(target, scope);
        let caps = installer.capabilities();
        let mut lines: Vec<String> = Vec::new();

        if caps.subagents {
            installer.install_identity(&agent)?;
            lines.push(format!("{} subagent", ui::good("✓")));
        } else {
            lines.push(ui::dim(
                "· no subagent support — identity installed as a skill",
            ));
        }

        if caps.skills {
            installer.install_skills(&agent)?;
            let count = agent.skills.len() + if caps.subagents { 0 } else { 1 };
            lines.push(format!(
                "{} {} skill{}",
                ui::good("✓"),
                count,
                if count == 1 { "" } else { "s" }
            ));
        }

        if !agent.mcp.is_empty() {
            if caps.mcp {
                installer.install_tools(&agent)?;
                lines.push(format!(
                    "{} {} MCP server{}",
                    ui::good("✓"),
                    agent.mcp.len(),
                    if agent.mcp.len() == 1 { "" } else { "s" }
                ));
            } else {
                lines.push(ui::dim(&format!(
                    "· {} MCP server(s) skipped — unsupported by {}",
                    agent.mcp.len(),
                    target.display_name()
                )));
            }
        }

        ui::success(&format!(
            "{}   {}
{}",
            ui::bold(target.display_name()),
            ui::dim(&installer.location()),
            lines.join(
                "
"
            )
        ));
    }

    ui::note(
        "Next",
        "Restart your agent to pick up the new configuration.",
    );
    ui::outro(&format!("{} installed", agent.name));

    Ok(())
}
