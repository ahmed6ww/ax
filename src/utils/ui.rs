//! Terminal presentation.
//!
//! Two modes. Attached to a terminal, output is drawn as a connected rail —
//! every step of a sync hangs off one vertical line, so a long run reads as a
//! single process rather than a wall of lines. Piped or redirected, the same
//! calls emit plain prefixed text, because CI logs and `| tee` should not carry
//! box-drawing characters.
//!
//! Every function here is infallible from the caller's side: a broken pipe
//! while printing progress must never fail an install that already succeeded.

use std::io::IsTerminal;
use std::sync::OnceLock;

use console::style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Connected rail, colour, live progress.
    Rich,
    /// One plain line per event.
    Plain,
}

/// `AXUR_OUTPUT` forces a mode: `rich`, `plain`, or `auto` (the default).
///
/// Forcing matters in both directions — a CI runner that renders ANSI can ask
/// for `rich`, and a terminal session piping into a pager can ask for `plain`.
fn mode() -> Mode {
    static MODE: OnceLock<Mode> = OnceLock::new();
    *MODE.get_or_init(|| {
        match std::env::var("AXUR_OUTPUT")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "rich" => Mode::Rich,
            "plain" => Mode::Plain,
            _ if std::io::stdout().is_terminal() => Mode::Rich,
            _ => Mode::Plain,
        }
    })
}

pub fn is_rich() -> bool {
    mode() == Mode::Rich
}

/// Print the body of a multi-line message with a plain-mode indent.
fn plain_block(prefix: &str, text: &str) {
    let mut lines = text.lines();
    if let Some(first) = lines.next() {
        println!("{} {}", prefix, first);
    }
    for line in lines {
        println!("{}   {}", " ".repeat(prefix.len().saturating_sub(1)), line);
    }
}

/// Open a run. Pairs with [`outro`] or [`outro_cancel`].
pub fn intro(title: &str) {
    match mode() {
        Mode::Rich => {
            let _ = cliclack::intro(format!(" {} ", style(title).bold().on_cyan().black()));
        }
        Mode::Plain => println!("== {}", title),
    }
}

/// Close a run successfully.
pub fn outro(message: &str) {
    match mode() {
        Mode::Rich => {
            let _ = cliclack::outro(message);
        }
        Mode::Plain => println!("== {}", message),
    }
}

/// Close a run that failed. Renders in the cancelled style.
pub fn outro_cancel(message: &str) {
    match mode() {
        Mode::Rich => {
            let _ = cliclack::outro_cancel(message);
        }
        Mode::Plain => eprintln!("== {}", message),
    }
}

/// A completed stage. Multi-line text continues along the rail.
pub fn step(message: &str) {
    match mode() {
        Mode::Rich => {
            let _ = cliclack::log::step(message);
        }
        Mode::Plain => plain_block("-", message),
    }
}

pub fn success(message: &str) {
    match mode() {
        Mode::Rich => {
            let _ = cliclack::log::success(message);
        }
        Mode::Plain => plain_block("+", message),
    }
}

pub fn info(message: &str) {
    match mode() {
        Mode::Rich => {
            let _ = cliclack::log::info(message);
        }
        Mode::Plain => plain_block("·", message),
    }
}

pub fn warning(message: &str) {
    match mode() {
        Mode::Rich => {
            let _ = cliclack::log::warning(message);
        }
        Mode::Plain => plain_block("!", message),
    }
}

pub fn error(message: &str) {
    match mode() {
        Mode::Rich => {
            let _ = cliclack::log::error(message);
        }
        Mode::Plain => eprintln!("x {}", message),
    }
}

/// A boxed aside, for guidance that is not part of the run's progress.
pub fn note(title: &str, body: &str) {
    match mode() {
        Mode::Rich => {
            let _ = cliclack::note(title, body);
        }
        Mode::Plain => {
            println!("-- {}", title);
            plain_block(" ", body);
        }
    }
}

// ---------------------------------------------------------------------------
// Text helpers
//
// Colour goes through `console::style`, which drops escapes when the stream is
// not a terminal, so these are safe to use in either mode.
// ---------------------------------------------------------------------------

pub fn dim(text: &str) -> String {
    style(text).dim().to_string()
}

pub fn bold(text: &str) -> String {
    style(text).bold().to_string()
}

pub fn accent(text: &str) -> String {
    style(text).cyan().to_string()
}

pub fn good(text: &str) -> String {
    style(text).green().to_string()
}

pub fn bad(text: &str) -> String {
    style(text).red().to_string()
}

/// A short commit, for the fixed-width column in a resolve listing.
pub fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

/// Truncate on a character boundary, appending an ellipsis.
pub fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}…", head.trim_end())
}

// ---------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------

/// A spinner that degrades to a single printed line when not on a terminal.
pub struct Spinner {
    inner: Option<cliclack::ProgressBar>,
}

impl Spinner {
    pub fn start(message: &str) -> Self {
        match mode() {
            Mode::Rich => {
                let bar = cliclack::spinner();
                bar.start(message);
                Self { inner: Some(bar) }
            }
            Mode::Plain => {
                println!("- {}", message);
                Self { inner: None }
            }
        }
    }

    pub fn stop(self, message: &str) {
        match self.inner {
            Some(bar) => bar.stop(message),
            None => println!("+ {}", message),
        }
    }

    /// Finish without leaving a line behind.
    pub fn clear(self) {
        if let Some(bar) = self.inner {
            bar.clear();
        }
    }
}

/// Several concurrent operations, each with its own line.
///
/// Used while skills resolve in parallel so the slow one is visible rather than
/// hidden behind a single spinner.
pub struct MultiProgress {
    inner: Option<cliclack::MultiProgress>,
}

impl MultiProgress {
    pub fn start(title: &str) -> Self {
        match mode() {
            Mode::Rich => Self {
                inner: Some(cliclack::multi_progress(title)),
            },
            Mode::Plain => {
                println!("- {}", title);
                Self { inner: None }
            }
        }
    }

    pub fn add(&self, label: &str) -> Spinner {
        match &self.inner {
            Some(multi) => {
                let bar = multi.add(cliclack::spinner());
                bar.start(label);
                Spinner { inner: Some(bar) }
            }
            None => Spinner { inner: None },
        }
    }

    pub fn stop(self) {
        if let Some(multi) = self.inner {
            multi.stop();
        }
    }
}

// ---------------------------------------------------------------------------
// Prompts
//
// Only reachable in Rich mode; callers branch on `is_rich()` so a piped or CI
// run never blocks on stdin.
// ---------------------------------------------------------------------------

use crate::installers::Target;
use crate::utils::paths::Scope;

/// Choose which agents to provision, pre-selecting the ones detected.
pub fn select_targets(detected: &[Target]) -> anyhow::Result<Vec<Target>> {
    select_targets_prompt(
        "Which agents should this project provision?",
        &Target::all(),
        detected,
    )
}

/// Choose which of the agents a project already declares to sync on this
/// machine — a checkmark per agent, not a numbered "both" choice, so picking
/// two is just checking two boxes rather than a third, separate option.
pub fn select_sync_targets(
    available: &[Target],
    detected: &[Target],
) -> anyhow::Result<Vec<Target>> {
    select_targets_prompt("Which agents do you use?", available, detected)
}

fn select_targets_prompt(
    question: &str,
    available: &[Target],
    detected: &[Target],
) -> anyhow::Result<Vec<Target>> {
    let mut prompt = cliclack::multiselect::<Target>(question).required(false);

    for target in available {
        let hint = if detected.contains(target) {
            "detected"
        } else {
            "not detected"
        };
        prompt = prompt.item(*target, target.display_name(), hint);
    }

    let initial: Vec<Target> = available
        .iter()
        .copied()
        .filter(|t| detected.contains(t))
        .collect();
    if !initial.is_empty() {
        prompt = prompt.initial_values(initial);
    }

    Ok(prompt.interact()?)
}

/// Choose project or user scope.
pub fn select_scope() -> anyhow::Result<Scope> {
    let scope = cliclack::select("Where should it install?")
        .item(
            Scope::Project,
            "This project",
            "committed with the repo — the team story",
        )
        .item(Scope::User, "This user", "every project on this machine")
        .initial_value(Scope::Project)
        .interact()?;
    Ok(scope)
}

/// Read a secret without echoing it.
pub fn secret(label: &str) -> anyhow::Result<String> {
    let value: String = cliclack::password(label).mask('•').interact()?;
    Ok(value)
}

/// Ask a yes/no question, defaulting to no.
pub fn confirm(question: &str) -> anyhow::Result<bool> {
    Ok(cliclack::confirm(question)
        .initial_value(false)
        .interact()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_respects_character_boundaries() {
        assert_eq!(truncate("short", 20), "short");
        for text in [
            "Enforce “Two Hats” refactoring — strict cleanup",
            "🦀 Rust systems engineer optimized for Tokio",
        ] {
            assert!(truncate(text, 20).chars().count() <= 20);
        }
    }

    #[test]
    fn short_sha_is_seven_characters() {
        assert_eq!(
            short_sha("9b23779e2309db5958097dd6bcd4a7671a74b9b3"),
            "9b23779"
        );
        assert_eq!(short_sha("abc"), "abc");
    }

    #[test]
    fn tests_run_in_plain_mode() {
        // Captured output is not a terminal, so the rail must not be drawn.
        assert_eq!(mode(), Mode::Plain);
    }

    #[test]
    fn plain_block_indents_continuation_lines() {
        // Sanity check on the shape used for every multi-line message.
        let text = "Header
first
second";
        assert_eq!(text.lines().count(), 3);
    }
}
