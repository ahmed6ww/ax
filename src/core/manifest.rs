//! `axur.toml` — the project manifest.
//!
//! Committed to the repository. It states what agent configuration this project
//! requires; `axur.lock` records what that resolved to. Together they are the pair
//! the competing tools omit: an install ledger in `$HOME` says what one machine
//! happens to have, while a manifest plus lock says what every machine must get.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::core::agent::McpTool;
use crate::installers::Target;
use crate::utils::paths::Scope;

pub const MANIFEST_FILE: &str = "axur.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default)]
    pub targets: Targets,

    /// Skills this project requires, keyed by the name they install under.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skills: BTreeMap<String, SkillSpec>,

    /// Bundles this project requires. A bundle brings skills, subagents,
    /// commands, MCP servers, hooks and permissions as one pinned unit.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bundles: BTreeMap<String, SkillSpec>,

    /// MCP servers this project requires.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp: BTreeMap<String, McpSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Targets {
    /// Which agents to provision. Defaults to both supported targets.
    #[serde(default = "default_agents")]
    pub agents: Vec<String>,

    /// `project` (committed alongside the repo) or `user`.
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_agents() -> Vec<String> {
    vec!["claude-code".to_string(), "codex".to_string()]
}

fn default_scope() -> String {
    "project".to_string()
}

impl Default for Targets {
    fn default() -> Self {
        Self {
            agents: default_agents(),
            scope: default_scope(),
        }
    }
}

impl Targets {
    pub fn resolve(&self) -> Result<(Vec<Target>, Scope)> {
        let mut targets = Vec::new();
        for name in &self.agents {
            let target = Target::from_slug(name).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown agent '{}' in {}. Supported: claude-code, codex.",
                    name,
                    MANIFEST_FILE
                )
            })?;
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        if targets.is_empty() {
            anyhow::bail!("[targets].agents is empty in {}", MANIFEST_FILE);
        }

        let scope = match self.scope.as_str() {
            "project" => Scope::Project,
            "user" => Scope::User,
            other => anyhow::bail!(
                "Unknown scope '{}' in {}. Use \"project\" or \"user\".",
                other,
                MANIFEST_FILE
            ),
        };

        Ok((targets, scope))
    }
}

/// Where a skill comes from.
///
/// Accepts a shorthand string or a table:
///
/// ```toml
/// [skills]
/// rust-architect = "ahmed6ww/ax-agents"
/// nextjs = { source = "vercel-labs/agent-skills", path = "skills/nextjs", rev = "main" }
///
/// # Authored by the team, not fetched from anywhere — `path` is a directory
/// # in this repository, committed alongside axur.toml.
/// onboarding = { source = "local", path = ".axur/skills/onboarding" }
/// ```
pub const LOCAL_SOURCE: &str = "local";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SkillSpec {
    Shorthand(String),
    Detailed {
        /// `owner/repo` on GitHub, or the literal `"local"`.
        source: String,
        /// Directory holding `SKILL.md` — within the repository for a GitHub
        /// source, or within *this* project for `"local"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        /// Branch, tag or commit to resolve. Defaults to the default branch.
        /// Not valid with `source = "local"`, which has nothing to pin.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rev: Option<String>,
    },
}

impl SkillSpec {
    pub fn source(&self) -> &str {
        match self {
            SkillSpec::Shorthand(s) => s,
            SkillSpec::Detailed { source, .. } => source,
        }
    }

    /// Directory inside the repository. Defaults to the skill's own name, which
    /// matches how both `ax-agents` and the Vercel collections are laid out.
    pub fn path<'a>(&'a self, skill_name: &'a str) -> &'a str {
        match self {
            SkillSpec::Shorthand(_) => skill_name,
            SkillSpec::Detailed { path, .. } => path.as_deref().unwrap_or(skill_name),
        }
    }

    pub fn rev(&self) -> Option<&str> {
        match self {
            SkillSpec::Shorthand(_) => None,
            SkillSpec::Detailed { rev, .. } => rev.as_deref(),
        }
    }

    /// Content the team authored directly in this repository, rather than
    /// fetched from GitHub.
    pub fn is_local(&self) -> bool {
        self.source() == LOCAL_SOURCE
    }

    /// The directory a local entry reads from, enforcing what a GitHub entry
    /// leaves optional: a local skill has no repository of its own to default
    /// into, and no ref to pin.
    pub fn local_dir(&self, name: &str) -> Result<&str> {
        match self {
            SkillSpec::Shorthand(_) => anyhow::bail!(
                "'{}' has source = \"local\" but no `path` — say which directory \
                 in this project holds it.",
                name
            ),
            SkillSpec::Detailed { path, rev, .. } => {
                if rev.is_some() {
                    anyhow::bail!(
                        "'{}' has source = \"local\": a local skill has no ref to \
                         pin, so `rev` is not allowed.",
                        name
                    );
                }
                path.as_deref().map(|p| p.trim_matches('/')).ok_or_else(|| {
                    anyhow::anyhow!(
                        "'{}' has source = \"local\" but no `path` — say which \
                         directory in this project holds it.",
                        name
                    )
                })
            }
        }
    }
}

/// An MCP server declared by the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpSpec {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl McpSpec {
    pub fn to_tool(&self, name: &str) -> McpTool {
        McpTool {
            name: name.to_string(),
            command: self.command.clone(),
            args: self.args.clone(),
            env: self.env.clone().into_iter().collect(),
        }
    }
}

impl Manifest {
    /// Locate `axur.toml` by walking up from `start` to the filesystem root.
    pub fn find(start: &Path) -> Option<PathBuf> {
        let mut dir = Some(start);
        while let Some(d) = dir {
            let candidate = d.join(MANIFEST_FILE);
            if candidate.is_file() {
                return Some(candidate);
            }
            dir = d.parent();
        }
        None
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let rendered = toml::to_string_pretty(self).context("Failed to serialize the manifest")?;
        crate::installers::common::write_atomic(path, rendered.as_bytes())
    }

    /// A starter manifest for `axur init`.
    pub fn starter(detected: &[Target]) -> Self {
        let agents = if detected.is_empty() {
            default_agents()
        } else {
            detected.iter().map(|t| t.slug().to_string()).collect()
        };

        Self {
            targets: Targets {
                agents,
                scope: default_scope(),
            },
            skills: BTreeMap::new(),
            bundles: BTreeMap::new(),
            mcp: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty() && self.mcp.is_empty() && self.bundles.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_skill_resolves_its_directory() {
        let spec = SkillSpec::Detailed {
            source: "local".to_string(),
            path: Some("/.axur/skills/onboarding/".to_string()),
            rev: None,
        };
        assert!(spec.is_local());
        assert_eq!(
            spec.local_dir("onboarding").unwrap(),
            ".axur/skills/onboarding"
        );
    }

    #[test]
    fn a_local_skill_with_no_path_is_an_error() {
        let spec = SkillSpec::Detailed {
            source: "local".to_string(),
            path: None,
            rev: None,
        };
        let err = spec.local_dir("onboarding").unwrap_err().to_string();
        assert!(err.contains("no `path`"), "{}", err);
    }

    #[test]
    fn a_local_skill_with_a_rev_is_an_error() {
        let spec = SkillSpec::Detailed {
            source: "local".to_string(),
            path: Some("onboarding".to_string()),
            rev: Some("main".to_string()),
        };
        let err = spec.local_dir("onboarding").unwrap_err().to_string();
        assert!(err.contains("no ref to"), "{}", err);
    }

    #[test]
    fn a_local_shorthand_has_no_path_to_default_to() {
        // `onboarding = "local"` names a source with no way to say which
        // directory holds it — the table form is required.
        let spec = SkillSpec::Shorthand("local".to_string());
        assert!(spec.is_local());
        assert!(spec.local_dir("onboarding").is_err());
    }

    const SAMPLE: &str = r#"
[targets]
agents = ["claude-code", "codex"]
scope = "project"

[skills]
rust-architect = "ahmed6ww/ax-agents"
nextjs = { source = "vercel-labs/agent-skills", path = "skills/nextjs", rev = "main" }

[mcp.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]
env = { CONTEXT7_API_KEY = "${CONTEXT7_API_KEY}" }
"#;

    #[test]
    fn parses_both_skill_spellings() {
        let m: Manifest = toml::from_str(SAMPLE).unwrap();
        assert_eq!(m.skills.len(), 2);

        let short = &m.skills["rust-architect"];
        assert_eq!(short.source(), "ahmed6ww/ax-agents");
        assert_eq!(short.path("rust-architect"), "rust-architect");
        assert_eq!(short.rev(), None);

        let long = &m.skills["nextjs"];
        assert_eq!(long.source(), "vercel-labs/agent-skills");
        assert_eq!(long.path("nextjs"), "skills/nextjs");
        assert_eq!(long.rev(), Some("main"));
    }

    #[test]
    fn resolves_targets_and_scope() {
        let m: Manifest = toml::from_str(SAMPLE).unwrap();
        let (targets, scope) = m.targets.resolve().unwrap();
        assert_eq!(targets, vec![Target::Claude, Target::Codex]);
        assert_eq!(scope, Scope::Project);
    }

    #[test]
    fn rejects_an_unknown_agent() {
        let m: Manifest = toml::from_str(
            r#"[targets]
agents = ["cursor"]
"#,
        )
        .unwrap();
        let err = m.targets.resolve().unwrap_err().to_string();
        assert!(err.contains("cursor"), "{}", err);
    }

    #[test]
    fn defaults_to_both_targets_at_project_scope() {
        let m: Manifest = toml::from_str("").unwrap();
        let (targets, scope) = m.targets.resolve().unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(scope, Scope::Project);
    }

    #[test]
    fn mcp_spec_becomes_a_tool() {
        let m: Manifest = toml::from_str(SAMPLE).unwrap();
        let tool = m.mcp["context7"].to_tool("context7");
        assert_eq!(tool.command, "npx");
        assert_eq!(tool.args, vec!["-y", "@upstash/context7-mcp"]);
        assert_eq!(tool.env["CONTEXT7_API_KEY"], "${CONTEXT7_API_KEY}");
    }

    #[test]
    fn a_misplaced_key_is_an_error_not_a_silent_no_op() {
        // Appending a skill after an [mcp.*] table puts it inside that table.
        // Without deny_unknown_fields this parsed happily and the skill vanished.
        let bad = r#"
[mcp.context7]
command = "npx"
web-perf = { source = "cloudflare/skills" }
"#;
        assert!(toml::from_str::<Manifest>(bad).is_err());
    }

    #[test]
    fn round_trips_through_toml() {
        let m: Manifest = toml::from_str(SAMPLE).unwrap();
        let rendered = toml::to_string_pretty(&m).unwrap();
        let again: Manifest = toml::from_str(&rendered).unwrap();
        assert_eq!(again.skills.len(), m.skills.len());
        assert_eq!(again.mcp.len(), m.mcp.len());
    }
}
