//! What went wrong, in a form a caller can act on.
//!
//! `thiserror` was declared as a dependency from the first commit and never
//! used: everything was `anyhow`, so nothing could distinguish "that skill does
//! not exist" from "the network is down" from "you declined". Every failure
//! exited 1, which makes the tool unscriptable — a CI job cannot tell a real
//! problem from a drifted lockfile.
//!
//! The variants below exist to be matched on and mapped to exit codes. The
//! binary still uses `anyhow` at the edge for context chaining; this is the
//! typed layer underneath it.

use std::process::ExitCode;

/// Exit codes agentpm promises. These are part of the interface: a script that
/// branches on them should keep working across releases.
pub mod exit {
    /// Everything succeeded.
    pub const OK: u8 = 0;
    /// Something went wrong that has no more specific code.
    pub const FAILURE: u8 = 1;
    /// `sync --check` found drift between the manifest and the lockfile.
    pub const DRIFT: u8 = 2;
    /// A source, skill, or bundle could not be found.
    pub const NOT_FOUND: u8 = 3;
    /// The network was unreachable, timed out, or rate limited.
    pub const NETWORK: u8 = 4;
    /// Content did not match the digest recorded in the lockfile.
    pub const INTEGRITY: u8 = 5;
    /// A manifest, lockfile, or fetched file could not be parsed.
    pub const INVALID: u8 = 6;
    /// The user declined to authorise something, or approval is required and
    /// the run is not interactive.
    pub const DENIED: u8 = 7;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Network(String),

    #[error("{0}")]
    Integrity(String),

    #[error("{0}")]
    Invalid(String),

    #[error("{0}")]
    Denied(String),

    #[error("{0}")]
    Drift(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::NotFound(_) => exit::NOT_FOUND,
            Error::Network(_) => exit::NETWORK,
            Error::Integrity(_) => exit::INTEGRITY,
            Error::Invalid(_) => exit::INVALID,
            Error::Denied(_) => exit::DENIED,
            Error::Drift(_) => exit::DRIFT,
            Error::Io(_) => exit::FAILURE,
        }
    }
}

/// Derive an exit code from an `anyhow` chain.
///
/// Commands build errors with `anyhow` for its context chaining, so the typed
/// error is usually somewhere inside rather than at the top. Look for it, and
/// fall back to classifying by message when a lower layer produced a plain
/// error.
pub fn exit_code_for(error: &anyhow::Error) -> u8 {
    for cause in error.chain() {
        if let Some(typed) = cause.downcast_ref::<Error>() {
            return typed.exit_code();
        }
    }

    // Fallback classification. Message matching is unlovely, but a wrong exit
    // code is better than collapsing every failure onto 1, and the strings
    // below are ones agentpm itself produces.
    let text = format!("{:#}", error).to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| text.contains(n));

    if has(&["need approval", "declined"]) {
        exit::DENIED
    } else if has(&["integrity check failed", "no longer match the lockfile"]) {
        exit::INTEGRITY
    } else if has(&[
        "rate limit",
        "failed to reach",
        "offline:",
        "timed out",
        "failed to connect",
    ]) {
        exit::NETWORK
    } else if has(&[
        "not found",
        "http 404",
        "has no skill.md",
        "is not declared",
    ]) {
        exit::NOT_FOUND
    } else if has(&[
        "not valid json",
        "not valid toml",
        "failed to parse",
        "is not a github source",
    ]) {
        exit::INVALID
    } else {
        exit::FAILURE
    }
}

/// Convert a code into the process's exit status.
pub fn status(code: u8) -> ExitCode {
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_errors_map_to_their_codes() {
        assert_eq!(Error::NotFound("x".into()).exit_code(), exit::NOT_FOUND);
        assert_eq!(Error::Network("x".into()).exit_code(), exit::NETWORK);
        assert_eq!(Error::Integrity("x".into()).exit_code(), exit::INTEGRITY);
        assert_eq!(Error::Invalid("x".into()).exit_code(), exit::INVALID);
        assert_eq!(Error::Denied("x".into()).exit_code(), exit::DENIED);
        assert_eq!(Error::Drift("x".into()).exit_code(), exit::DRIFT);
    }

    #[test]
    fn a_typed_error_is_found_through_anyhow_context() {
        let err = anyhow::Error::from(Error::Network("unreachable".into()))
            .context("Failed to resolve skill 'x'")
            .context("while syncing");
        assert_eq!(exit_code_for(&err), exit::NETWORK);
    }

    #[test]
    fn real_messages_classify_correctly() {
        let cases = [
            (
                "GitHub rate limit reached while resolving a/b@HEAD",
                exit::NETWORK,
            ),
            ("Offline: a/b is not cached.", exit::NETWORK),
            ("Not found while downloading x (HTTP 404)", exit::NOT_FOUND),
            (
                "2 item(s) need approval and this is not an interactive terminal",
                exit::DENIED,
            ),
            ("Integrity check failed for 'x'", exit::INTEGRITY),
            (".mcp.json is not valid JSON.", exit::INVALID),
            ("'owner' is not a GitHub source.", exit::INVALID),
        ];
        for (message, expected) in cases {
            let err = anyhow::anyhow!("{}", message);
            assert_eq!(exit_code_for(&err), expected, "misclassified: {}", message);
        }
    }

    #[test]
    fn an_unrecognised_failure_is_a_plain_one() {
        assert_eq!(
            exit_code_for(&anyhow::anyhow!("something odd")),
            exit::FAILURE
        );
    }

    #[test]
    fn context_wrapping_does_not_change_the_classification() {
        let err = anyhow::anyhow!("GitHub rate limit reached")
            .context("Failed to resolve skill 'find-skills'");
        assert_eq!(exit_code_for(&err), exit::NETWORK);
    }
}
