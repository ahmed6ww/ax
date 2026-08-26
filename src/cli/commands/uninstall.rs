//! `agentpm uninstall` — remove an installed agent.

use anyhow::Result;

use crate::installers::{get_installer, Target};
use crate::utils::paths::Scope;
use crate::utils::ui;

use super::super::TargetArg;

pub async fn execute(agent_name: &str, target: Option<TargetArg>, global: bool) -> Result<()> {
    let targets: Vec<Target> = match target {
        Some(t) => vec![t.into()],
        None => Target::all().to_vec(),
    };
    let scope = Scope::from_global_flag(global);

    ui::intro(&format!("agentpm uninstall {}", agent_name));

    for target in targets {
        let installer = get_installer(target, scope);
        installer.uninstall(agent_name)?;
        ui::success(&format!(
            "{}   {}
{} {} scope",
            ui::bold(target.display_name()),
            ui::dim(&installer.location()),
            ui::dim("·"),
            scope.display_name()
        ));
    }

    ui::info("MCP servers are left in place — they may be shared with other agents");
    ui::outro(&format!("{} removed", agent_name));

    Ok(())
}
