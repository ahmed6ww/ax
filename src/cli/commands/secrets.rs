//! `axur secrets` — set, unset, list, and (internally) resolve-and-exec.
//!
//! `set` and `unset` talk to the OS keychain via [`crate::core::secrets`].
//! `exec` is the piece that makes a stored secret actually reach an MCP
//! server: `axur sync` rewrites a server's `command` to `axur secrets exec
//! --name VAR -- <real command> <real args>` whenever one of its declared env
//! references is keychain-managed, so Claude Code and Codex — neither of
//! which can read an OS keychain themselves — just launch axur, which
//! resolves the value into its own environment and then becomes the real
//! command.

use anyhow::{Context, Result};

use crate::cli::SecretsAction;
use crate::core::secrets;
use crate::utils::ui;

pub async fn execute(action: Option<SecretsAction>) -> Result<()> {
    match action {
        None => list(),
        Some(SecretsAction::Set { name }) => set(&name),
        Some(SecretsAction::Unset { name }) => unset(&name),
        Some(SecretsAction::Exec { names, command }) => exec(&names, &command),
    }
}

fn list() -> Result<()> {
    ui::intro("axur secrets");
    let names = secrets::list()?;
    if names.is_empty() {
        ui::note(
            "Nothing stored",
            "axur secrets set <NAME> stores a value in your OS keychain — \
             Windows Credential Manager, macOS Keychain, or the Linux Secret \
             Service — instead of your shell profile.",
        );
    } else {
        ui::step(&format!(
            "{} in the OS keychain\n{}",
            super::sync::plural(names.len(), "secret"),
            names
                .iter()
                .map(|n| format!("{} {}", ui::good("✓"), n))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    ui::outro("Done");
    Ok(())
}

fn set(name: &str) -> Result<()> {
    ui::intro(&format!("axur secrets set {}", name));

    if !ui::is_rich() {
        ui::outro_cancel("Not an interactive terminal");
        anyhow::bail!(
            "'{}' needs a value entered at a prompt, so it never touches shell \
             history or a script's argument list. Run this in an interactive terminal.",
            name
        );
    }

    let value = ui::secret(&format!("Value for {}", name))?;
    if value.trim().is_empty() {
        ui::outro_cancel("No value entered");
        anyhow::bail!("Nothing stored — the value was empty");
    }

    secrets::set(name, value.trim())?;
    ui::success(&format!(
        "{} stored in the OS keychain\n{}",
        name,
        ui::dim("A sync using ${...} for this name in axur.toml now resolves it from here.")
    ));
    ui::outro("Done");
    Ok(())
}

fn unset(name: &str) -> Result<()> {
    ui::intro(&format!("axur secrets unset {}", name));
    if secrets::unset(name)? {
        ui::success(&format!("{} removed", name));
    } else {
        ui::info(&format!("{} was not stored", name));
    }
    ui::outro("Done");
    Ok(())
}

/// Resolve `names` from the keychain into this process's environment, then
/// become `command`. Never printed to — this runs as the actual MCP server
/// process, and anything written to stdout/stderr would corrupt its protocol.
fn exec(names: &[String], command: &[String]) -> Result<()> {
    let (program, args) = command
        .split_first()
        .context("axur secrets exec: no command given after --")?;

    let mut cmd = std::process::Command::new(program);
    cmd.args(args);

    for name in names {
        // Not found: leave whatever this process already inherited (e.g. a
        // plain shell-exported value) rather than failing — the keychain
        // supplements a real environment variable, it doesn't replace it.
        if let Some(value) = secrets::get(name)? {
            cmd.env(name, value);
        }
    }

    exec_replace(cmd)
}

#[cfg(unix)]
fn exec_replace(mut cmd: std::process::Command) -> Result<()> {
    use std::os::unix::process::CommandExt;
    // On success this never returns — the process image is replaced in
    // place, so the real server inherits this process's stdio and PID
    // directly rather than running as a child axur has to babysit.
    let err = cmd.exec();
    Err(err).context("Failed to exec the wrapped command")
}

#[cfg(windows)]
fn exec_replace(mut cmd: std::process::Command) -> Result<()> {
    // Windows has no exec-replace; run as a child, inheriting stdio, and
    // forward its exit code so the caller cannot tell the difference.
    let status = cmd.status().context("Failed to run the wrapped command")?;
    std::process::exit(status.code().unwrap_or(1));
}
