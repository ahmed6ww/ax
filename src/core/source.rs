//! Resolving skill sources to immutable content.
//!
//! A source is `owner/repo` on GitHub. Resolution happens in three steps:
//!
//! 1. A ref (branch, tag, or commit) resolves to a **commit SHA**. Everything
//!    after this point is pinned, which is what makes `agentpm.lock`
//!    reproducible rather than a record of what HEAD happened to be.
//! 2. The git tree API returns the **actual file list** under the skill's
//!    directory. Earlier versions guessed twelve filenames and ignored 404s, so
//!    any skill not using those exact names installed with its scripts and
//!    references missing and no error.
//! 3. Files are fetched from `raw.githubusercontent.com` at that SHA, which is
//!    immutable and therefore safe to cache and to verify against a digest.

use anyhow::{Context, Result};
use futures::stream::{StreamExt, TryStreamExt};
use serde::Deserialize;
use std::time::Duration;

use crate::core::lockfile::{digest, LockedFile};

/// Largest file agentpm will pull into a skill directory.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Concurrent file fetches per skill.
const FETCH_CONCURRENCY: usize = 8;

/// A file fetched from a source.
#[derive(Debug, Clone)]
pub struct FetchedFile {
    /// Path relative to the skill directory, e.g. `SKILL.md`, `scripts/lint.py`.
    pub path: String,
    pub bytes: Vec<u8>,
}

impl FetchedFile {
    pub fn locked(&self) -> LockedFile {
        LockedFile {
            path: self.path.clone(),
            digest: digest(&self.bytes),
            size: self.bytes.len() as u64,
        }
    }
}

/// Everything a source yielded for one skill, pinned to a commit.
#[derive(Debug, Clone)]
pub struct ResolvedSkill {
    pub source: String,
    pub source_url: String,
    pub path: String,
    pub resolved: String,
    pub files: Vec<FetchedFile>,
}

impl ResolvedSkill {
    pub fn skill_md(&self) -> Option<&FetchedFile> {
        self.files.iter().find(|f| f.path == "SKILL.md")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubSource {
    pub owner: String,
    pub repo: String,
}

impl GitHubSource {
    /// Parse `owner/repo`, a full GitHub URL, or a `.git` clone URL.
    pub fn parse(source: &str) -> Result<Self> {
        let trimmed = source.trim().trim_end_matches('/');

        let slug = trimmed
            .strip_prefix("https://github.com/")
            .or_else(|| trimmed.strip_prefix("http://github.com/"))
            .or_else(|| trimmed.strip_prefix("git@github.com:"))
            .unwrap_or(trimmed)
            .trim_end_matches(".git");

        let mut parts = slug.split('/');
        let owner = parts.next().unwrap_or_default();
        let repo = parts.next().unwrap_or_default();

        if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
            anyhow::bail!(
                "'{}' is not a GitHub source. Use owner/repo, or a github.com URL.",
                source
            );
        }

        let valid = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if !valid(owner) || !valid(repo) {
            anyhow::bail!(
                "'{}' contains characters not valid in a GitHub path",
                source
            );
        }

        Ok(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }

    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    pub fn clone_url(&self) -> String {
        format!("https://github.com/{}/{}.git", self.owner, self.repo)
    }
}

#[derive(Deserialize)]
struct CommitResponse {
    sha: String,
}

#[derive(Deserialize)]
struct TreeResponse {
    #[serde(default)]
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    size: Option<u64>,
}

/// Fetches skill content from GitHub.
pub struct SourceClient {
    http: reqwest::Client,
    token: Option<String>,
}

impl SourceClient {
    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent(concat!("agentpm/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("Failed to build the HTTP client")?;

        // Raises the anonymous 60 requests/hour limit, which a manifest with a
        // handful of skills will otherwise reach quickly.
        let token = std::env::var("GITHUB_TOKEN")
            .or_else(|_| std::env::var("GH_TOKEN"))
            .ok()
            .filter(|t| !t.trim().is_empty());

        Ok(Self { http, token })
    }

    fn api(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self
            .http
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        req
    }

    async fn check(&self, response: reqwest::Response, what: &str) -> Result<reqwest::Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        if status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            let remaining = response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("?");
            if remaining == "0" {
                anyhow::bail!(
                    "GitHub rate limit reached while {}. Set GITHUB_TOKEN to raise it.",
                    what
                );
            }
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!("Not found while {} (HTTP 404)", what);
        }

        anyhow::bail!("HTTP {} while {}", status.as_u16(), what)
    }

    /// Resolve a branch, tag or commit to an immutable commit SHA.
    pub async fn resolve_commit(&self, source: &GitHubSource, rev: Option<&str>) -> Result<String> {
        let rev = rev.unwrap_or("HEAD");
        let url = format!(
            "https://api.github.com/repos/{}/{}/commits/{}",
            source.owner, source.repo, rev
        );

        let response = self
            .api(&url)
            .send()
            .await
            .with_context(|| format!("Failed to reach GitHub for {}", source.slug()))?;
        let response = self
            .check(response, &format!("resolving {}@{}", source.slug(), rev))
            .await?;

        let commit: CommitResponse = response
            .json()
            .await
            .context("Failed to parse the commit response")?;
        Ok(commit.sha)
    }

    /// List the files under `path` at `commit`.
    ///
    /// This is the real directory listing that replaces guessing filenames.
    async fn list_files(
        &self,
        source: &GitHubSource,
        commit: &str,
        path: &str,
        keep: &dyn Fn(&str) -> bool,
    ) -> Result<Vec<(String, u64)>> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
            source.owner, source.repo, commit
        );

        let response = self.api(&url).send().await.with_context(|| {
            format!(
                "Failed to list files in {} at {}",
                source.slug(),
                &commit[..7.min(commit.len())]
            )
        })?;
        let response = self
            .check(response, &format!("listing {}", source.slug()))
            .await?;

        let tree: TreeResponse = response
            .json()
            .await
            .context("Failed to parse the tree response")?;

        if tree.truncated {
            anyhow::bail!(
                "{} is too large for the git tree API to list in one response",
                source.slug()
            );
        }

        let prefix = path.trim_matches('/');
        let mut files = Vec::new();

        for entry in tree.tree {
            if entry.kind != "blob" {
                continue;
            }
            let Some(relative) = strip_dir_prefix(&entry.path, prefix) else {
                continue;
            };
            if !keep(relative) {
                continue;
            }

            let size = entry.size.unwrap_or(0);
            if size > MAX_FILE_BYTES {
                anyhow::bail!(
                    "{}/{} is {} bytes, over the {} byte limit",
                    prefix,
                    relative,
                    size,
                    MAX_FILE_BYTES
                );
            }

            files.push((relative.to_string(), size));
        }

        if files.is_empty() {
            anyhow::bail!(
                "Nothing to install at '{}' in {}. Check the `path` in the manifest.",
                prefix,
                source.slug()
            );
        }

        files.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(files)
    }

    async fn fetch_raw(
        &self,
        source: &GitHubSource,
        commit: &str,
        full_path: &str,
    ) -> Result<Vec<u8>> {
        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            source.owner, source.repo, commit, full_path
        );

        let mut req = self.http.get(&url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }

        let response = req
            .send()
            .await
            .with_context(|| format!("Failed to download {}", full_path))?;
        let response = self
            .check(response, &format!("downloading {}", full_path))
            .await?;

        let bytes = response
            .bytes()
            .await
            .with_context(|| format!("Failed to read {}", full_path))?;

        if bytes.len() as u64 > MAX_FILE_BYTES {
            anyhow::bail!("{} exceeds the {} byte limit", full_path, MAX_FILE_BYTES);
        }

        Ok(bytes.to_vec())
    }

    /// Resolve and download one skill, pinned to `rev` or to a known commit.
    pub async fn fetch_skill(
        &self,
        source: &GitHubSource,
        path: &str,
        rev: Option<&str>,
        pinned: Option<&str>,
    ) -> Result<ResolvedSkill> {
        let commit = match pinned {
            Some(sha) => sha.to_string(),
            None => self.resolve_commit(source, rev).await?,
        };

        let listing = self
            .list_files(source, &commit, path, &|relative: &str| {
                relative == "SKILL.md"
                    || relative.starts_with("scripts/")
                    || relative.starts_with("references/")
                    || relative.starts_with("assets/")
            })
            .await?;

        if !listing.iter().any(|(p, _)| p == "SKILL.md") {
            anyhow::bail!(
                "'{}' in {} has no SKILL.md",
                path.trim_matches('/'),
                source.slug()
            );
        }

        let prefix = path.trim_matches('/');

        let commit_ref = commit.as_str();
        let mut files: Vec<FetchedFile> =
            futures::stream::iter(listing.into_iter().map(|(relative, _)| async move {
                let full = if prefix.is_empty() {
                    relative.clone()
                } else {
                    format!("{}/{}", prefix, relative)
                };
                let bytes = self.fetch_raw(source, commit_ref, &full).await?;
                Ok::<_, anyhow::Error>(FetchedFile {
                    path: relative,
                    bytes,
                })
            }))
            .buffer_unordered(FETCH_CONCURRENCY)
            .try_collect()
            .await?;

        files.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(ResolvedSkill {
            source: source.slug(),
            source_url: source.clone_url(),
            path: prefix.to_string(),
            resolved: commit,
            files,
        })
    }
}

impl SourceClient {
    /// Resolve and download a bundle: its `BUNDLE.toml` and every file the
    /// manifest names.
    pub async fn fetch_bundle(
        &self,
        source: &GitHubSource,
        path: &str,
        rev: Option<&str>,
        pinned: Option<&str>,
    ) -> Result<(crate::core::bundle::BundleManifest, ResolvedSkill)> {
        use crate::core::bundle::BUNDLE_FILE;

        let commit = match pinned {
            Some(sha) => sha.to_string(),
            None => self.resolve_commit(source, rev).await?,
        };
        let prefix = path.trim_matches('/');

        // Read the manifest first; it declares exactly which files to pull, so
        // nothing is guessed and nothing unrelated to the bundle is installed.
        let manifest_path = if prefix.is_empty() {
            BUNDLE_FILE.to_string()
        } else {
            format!("{}/{}", prefix, BUNDLE_FILE)
        };
        let manifest_bytes = self
            .fetch_raw(source, &commit, &manifest_path)
            .await
            .with_context(|| format!("No {} at '{}' in {}", BUNDLE_FILE, prefix, source.slug()))?;
        let manifest = crate::core::bundle::BundleManifest::parse(
            std::str::from_utf8(&manifest_bytes).context("BUNDLE.toml is not valid UTF-8")?,
        )?;

        // Skill entries are directories; the rest are individual files.
        let declared_dirs: Vec<String> = manifest.skills.clone();
        let declared_files: Vec<String> = manifest
            .agents
            .iter()
            .chain(&manifest.commands)
            .chain(&manifest.scripts)
            .cloned()
            .collect();

        let listing = self
            .list_files(source, &commit, path, &|relative: &str| {
                declared_files.iter().any(|f| f == relative)
                    || declared_dirs
                        .iter()
                        .any(|d| relative.starts_with(&format!("{}/", d)))
            })
            .await?;

        for declared in &declared_files {
            if !listing.iter().any(|(p, _)| p == declared) {
                anyhow::bail!(
                    "Bundle '{}' declares '{}' but the file is not in the repository",
                    manifest.name,
                    declared
                );
            }
        }
        for dir in &declared_dirs {
            let skill_md = format!("{}/SKILL.md", dir);
            if !listing.iter().any(|(p, _)| *p == skill_md) {
                anyhow::bail!(
                    "Bundle '{}' declares skill '{}' but there is no {}",
                    manifest.name,
                    dir,
                    skill_md
                );
            }
        }

        let mut files = self.download_all(source, &commit, prefix, listing).await?;
        files.push(FetchedFile {
            path: BUNDLE_FILE.to_string(),
            bytes: manifest_bytes,
        });
        files.sort_by(|a, b| a.path.cmp(&b.path));

        Ok((
            manifest,
            ResolvedSkill {
                source: source.slug(),
                source_url: source.clone_url(),
                path: prefix.to_string(),
                resolved: commit,
                files,
            },
        ))
    }

    async fn download_all(
        &self,
        source: &GitHubSource,
        commit: &str,
        prefix: &str,
        listing: Vec<(String, u64)>,
    ) -> Result<Vec<FetchedFile>> {
        futures::stream::iter(listing.into_iter().map(|(relative, _)| async move {
            let full = if prefix.is_empty() {
                relative.clone()
            } else {
                format!("{}/{}", prefix, relative)
            };
            let bytes = self.fetch_raw(source, commit, &full).await?;
            Ok::<_, anyhow::Error>(FetchedFile {
                path: relative,
                bytes,
            })
        }))
        .buffer_unordered(FETCH_CONCURRENCY)
        .try_collect()
        .await
    }
}

/// Path of `entry` relative to `prefix`, or `None` if it is not below it.
///
/// An empty prefix means the repository root.
fn strip_dir_prefix<'a>(entry: &'a str, prefix: &str) -> Option<&'a str> {
    if prefix.is_empty() {
        return Some(entry);
    }
    entry
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_accepted_source_spelling() {
        let expected = GitHubSource {
            owner: "vercel-labs".to_string(),
            repo: "agent-skills".to_string(),
        };
        for input in [
            "vercel-labs/agent-skills",
            "https://github.com/vercel-labs/agent-skills",
            "https://github.com/vercel-labs/agent-skills.git",
            "https://github.com/vercel-labs/agent-skills/",
            "git@github.com:vercel-labs/agent-skills.git",
        ] {
            assert_eq!(GitHubSource::parse(input).unwrap(), expected, "{}", input);
        }
    }

    #[test]
    fn rejects_malformed_sources() {
        for input in [
            "",
            "owner",
            "owner/repo/extra",
            "owner/",
            "/repo",
            "own er/repo",
        ] {
            assert!(GitHubSource::parse(input).is_err(), "accepted {:?}", input);
        }
    }

    #[test]
    fn builds_a_clone_url() {
        let s = GitHubSource::parse("ahmed6ww/ax-agents").unwrap();
        assert_eq!(s.clone_url(), "https://github.com/ahmed6ww/ax-agents.git");
        assert_eq!(s.slug(), "ahmed6ww/ax-agents");
    }

    #[test]
    fn strips_directory_prefixes() {
        assert_eq!(
            strip_dir_prefix("skills/nextjs/SKILL.md", "skills/nextjs"),
            Some("SKILL.md")
        );
        assert_eq!(
            strip_dir_prefix("skills/nextjs/scripts/a.py", "skills/nextjs"),
            Some("scripts/a.py")
        );
        assert_eq!(strip_dir_prefix("SKILL.md", ""), Some("SKILL.md"));
        // A sibling directory that merely shares a name prefix must not match.
        assert_eq!(
            strip_dir_prefix("skills/nextjs-old/SKILL.md", "skills/nextjs"),
            None
        );
        assert_eq!(strip_dir_prefix("other/SKILL.md", "skills/nextjs"), None);
    }

    #[test]
    fn fetched_files_hash_their_contents() {
        let f = FetchedFile {
            path: "SKILL.md".to_string(),
            bytes: b"hello".to_vec(),
        };
        let locked = f.locked();
        assert_eq!(locked.size, 5);
        assert_eq!(locked.digest, digest(b"hello"));
    }
}
