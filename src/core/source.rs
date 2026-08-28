//! Resolving skill sources to immutable content.
//!
//! A source is `owner/repo` on GitHub. Resolution happens in three steps:
//!
//! 1. A ref (branch, tag, or commit) resolves to a **commit SHA**. Everything
//!    after this point is pinned, which is what makes `axur.lock`
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
use std::path::Path;
use std::time::Duration;

use crate::core::cache::Cache;
use crate::core::lockfile::{digest, LockedFile};
use crate::core::manifest::LOCAL_SOURCE;

/// Largest file axur will pull into a skill directory.
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

/// Read a skill directly from this project's own working tree, rather than
/// fetching it from GitHub.
///
/// There is no ref to resolve and nothing to cache: the file on disk *is* the
/// pinned content, and `axur sync --check` catches drift on it exactly the
/// way it catches drift on a GitHub-sourced skill — by comparing the digest
/// recorded in `axur.lock`, not by re-resolving a ref.
pub fn read_local_skill(project_root: &Path, dir: &str) -> Result<ResolvedSkill> {
    let prefix = dir.trim_matches('/');
    if prefix.is_empty() {
        anyhow::bail!("A local skill's path cannot be the project root");
    }
    let root = project_root.join(prefix);
    if !root.is_dir() {
        return Err(crate::core::error::Error::NotFound(format!(
            "No directory at '{}' — declared with source = \"local\" but not \
             found in this project.",
            prefix
        ))
        .into());
    }

    let mut files = Vec::new();
    collect_local_files(&root, &root, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let Some(skill_md) = files.iter().find(|f| f.path == "SKILL.md") else {
        return Err(
            crate::core::error::Error::NotFound(format!("'{}' has no SKILL.md", prefix)).into(),
        );
    };
    validate_skill_md(&skill_md.bytes, prefix, LOCAL_SOURCE)?;

    Ok(ResolvedSkill {
        source: LOCAL_SOURCE.to_string(),
        source_url: format!("local:{}", prefix),
        path: prefix.to_string(),
        // No commit to pin to — the working tree is read fresh every sync,
        // and the lockfile's content digest is what catches drift.
        resolved: "working-tree".to_string(),
        files,
    })
}

/// Recursively collect every regular file under `dir`, as paths relative to
/// `root` with forward slashes — matching the shape a GitHub tree listing
/// already produces, so every downstream step treats the two the same way.
///
/// Symlinks are skipped rather than followed: this walks a directory inside
/// the developer's own project, and a symlink escaping it should not end up
/// silently swept into a lockfile digest and installed onto every machine
/// that syncs.
fn collect_local_files(root: &Path, dir: &Path, out: &mut Vec<FetchedFile>) -> Result<()> {
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("Failed to read {}", dir.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }

        let path = entry.path();
        if file_type.is_dir() {
            collect_local_files(root, &path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let bytes =
            std::fs::read(&path).with_context(|| format!("Failed to read {}", path.display()))?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            anyhow::bail!(
                "{} is over the {} byte limit",
                path.display(),
                MAX_FILE_BYTES
            );
        }

        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");

        out.push(FetchedFile {
            path: relative,
            bytes,
        });
    }
    Ok(())
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

/// What a source path turned out to hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Skill,
    Bundle,
}

/// Fetches skill content from GitHub.
pub struct SourceClient {
    http: reqwest::Client,
    token: Option<String>,
    cache: Cache,
    /// Refuse any network request. Everything pinned still resolves from cache.
    offline: bool,
}

impl SourceClient {
    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent(concat!("axur/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("Failed to build the HTTP client")?;

        // Raises the anonymous 60 requests/hour limit, which a manifest with a
        // handful of skills will otherwise reach quickly.
        let token = std::env::var("GITHUB_TOKEN")
            .or_else(|_| std::env::var("GH_TOKEN"))
            .ok()
            .filter(|t| !t.trim().is_empty());

        Ok(Self {
            http,
            token,
            cache: Cache::open(),
            offline: false,
        })
    }

    /// Serve only from cache; any request that would hit the network fails.
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    fn refuse_offline(&self, what: &str) -> Result<()> {
        if self.offline {
            anyhow::bail!(
                "Offline: {} is not cached.\n\
                 Sync once online to populate the cache, or drop --offline.",
                what
            );
        }
        Ok(())
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
                return Err(crate::core::error::Error::Network(format!(
                    "GitHub rate limit reached while {}. Set GITHUB_TOKEN to raise it.",
                    what
                ))
                .into());
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
        self.refuse_offline(&format!(
            "{}@{} (a ref must be resolved online)",
            source.slug(),
            rev
        ))?;
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
        // A commit's tree never changes, so this is cached indefinitely.
        let cache_key = Cache::tree_key(&source.owner, &source.repo, commit);
        let body = match self.cache.get(&cache_key) {
            Some(cached) => cached,
            None => {
                self.refuse_offline(&format!("the file listing for {}", source.slug()))?;

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

                let bytes = response
                    .bytes()
                    .await
                    .context("Failed to read the tree response")?
                    .to_vec();
                self.cache.put(&cache_key, &bytes);
                bytes
            }
        };

        let tree: TreeResponse =
            serde_json::from_slice(&body).context("Failed to parse the tree response")?;

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
            return Err(crate::core::error::Error::NotFound(format!(
                "Nothing to install at '{}' in {}. Check the `path` in the manifest.",
                prefix,
                source.slug()
            ))
            .into());
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
        // Pinned to a commit, so the content at this path is frozen.
        let cache_key = Cache::blob_key(&source.owner, &source.repo, commit, full_path);
        if let Some(cached) = self.cache.get(&cache_key) {
            return Ok(cached);
        }
        self.refuse_offline(full_path)?;

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

        let bytes = bytes.to_vec();
        self.cache.put(&cache_key, &bytes);
        Ok(bytes)
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
            return Err(crate::core::error::Error::NotFound(format!(
                "'{}' in {} has no SKILL.md",
                path.trim_matches('/'),
                source.slug()
            ))
            .into());
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

        // A skill whose frontmatter will not parse, or that has no description,
        // installs happily and is then never selected by the agent. Catch it
        // here rather than shipping something inert.
        if let Some(skill_md) = files.iter().find(|f| f.path == "SKILL.md") {
            validate_skill_md(&skill_md.bytes, prefix, &source.slug())?;
        }

        Ok(ResolvedSkill {
            source: source.slug(),
            source_url: source.clone_url(),
            path: prefix.to_string(),
            resolved: commit,
            files,
        })
    }
}

/// The frontmatter fields the Agent Skills standard requires.
///
/// Unknown fields are ignored: a skill may carry `allowed-tools`, `license` and
/// anything else the target understands, and axur has no business rejecting
/// keys it does not know about.
#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// Check that a fetched `SKILL.md` carries the frontmatter the standard needs.
///
/// This runs on content fetched from an arbitrary repository, so parsing is
/// budgeted: an adversarial document cannot expand aliases until the process
/// runs out of memory.
pub(crate) fn validate_skill_md(bytes: &[u8], path: &str, slug: &str) -> Result<()> {
    let text = std::str::from_utf8(bytes)
        .with_context(|| format!("SKILL.md in {} is not valid UTF-8", slug))?;

    let Some(rest) = text.strip_prefix("---") else {
        anyhow::bail!("'{}' in {} has no YAML frontmatter in SKILL.md", path, slug);
    };
    let Some(end) = rest.find(
        "
---",
    ) else {
        anyhow::bail!(
            "'{}' in {} has an unterminated frontmatter block in SKILL.md",
            path,
            slug
        );
    };

    let options = serde_saphyr::options! {
        budget: serde_saphyr::budget! {
            max_documents: 1,
            max_anchors: 64,
        },
        duplicate_keys: serde_saphyr::DuplicateKeyPolicy::FirstWins,
    };

    let frontmatter: SkillFrontmatter =
        serde_saphyr::from_str_with_options(rest[..end].trim(), options)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .with_context(|| {
                format!("Could not parse the frontmatter of '{}' in {}", path, slug)
            })?;

    for (field, value) in [
        ("name", &frontmatter.name),
        ("description", &frontmatter.description),
    ] {
        let present = value
            .as_deref()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        if !present {
            anyhow::bail!(
                "SKILL.md for '{}' in {} has no `{}`. Agents select skills by                  description, so one without it is never used.",
                path,
                slug,
                field
            );
        }
    }

    Ok(())
}

impl SourceClient {
    /// Resolve a ref and determine whether the path holds a bundle or a skill.
    ///
    /// A single cheap request: a bundle is a directory with `BUNDLE.toml`, and
    /// a 404 on that file means the path is a plain skill.
    pub async fn probe(
        &self,
        source: &GitHubSource,
        path: &str,
        rev: Option<&str>,
    ) -> Result<(String, SourceKind)> {
        use crate::core::bundle::BUNDLE_FILE;

        let commit = self.resolve_commit(source, rev).await?;
        let prefix = path.trim_matches('/');
        let manifest_path = if prefix.is_empty() {
            BUNDLE_FILE.to_string()
        } else {
            format!("{}/{}", prefix, BUNDLE_FILE)
        };

        if self
            .fetch_raw(source, &commit, &manifest_path)
            .await
            .is_ok()
        {
            return Ok((commit, SourceKind::Bundle));
        }

        // Not a bundle. Confirm it is a skill rather than assuming: a path that
        // holds neither used to be written into the manifest and only fail on
        // the sync that followed, leaving a broken entry behind.
        let skill_path = if prefix.is_empty() {
            "SKILL.md".to_string()
        } else {
            format!("{}/SKILL.md", prefix)
        };
        if self.fetch_raw(source, &commit, &skill_path).await.is_ok() {
            return Ok((commit, SourceKind::Skill));
        }

        Err(crate::core::error::Error::NotFound(format!(
            "'{}' in {} holds neither {} nor SKILL.md",
            if prefix.is_empty() { "<root>" } else { prefix },
            source.slug(),
            BUNDLE_FILE
        ))
        .into())
    }

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

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn reads_a_local_skill_from_the_working_tree() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join(".axur/skills/onboarding");
        write(
            &skill.join("SKILL.md"),
            "---\nname: onboarding\ndescription: How this team works\n---\n\nRead this first.",
        );
        write(&skill.join("references/notes.md"), "# Notes\n");

        let resolved = read_local_skill(dir.path(), ".axur/skills/onboarding").unwrap();
        assert_eq!(resolved.source, "local");
        assert_eq!(resolved.path, ".axur/skills/onboarding");
        assert_eq!(resolved.resolved, "working-tree");
        assert_eq!(resolved.files.len(), 2);
        assert!(resolved.skill_md().unwrap().bytes.starts_with(b"---"));
        assert!(resolved
            .files
            .iter()
            .any(|f| f.path == "references/notes.md"));
    }

    #[test]
    fn a_missing_local_directory_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_local_skill(dir.path(), "nowhere")
            .unwrap_err()
            .to_string();
        assert!(err.contains("No directory"), "{}", err);
    }

    #[test]
    fn a_local_skill_without_skill_md_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("empty/README.md"), "not a skill");
        let err = read_local_skill(dir.path(), "empty")
            .unwrap_err()
            .to_string();
        assert!(err.contains("SKILL.md"), "{}", err);
    }

    #[test]
    fn a_local_skill_with_no_description_is_rejected_same_as_a_fetched_one() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("bare/SKILL.md"),
            "---\nname: bare\n---\n\nBody.",
        );
        let err = read_local_skill(dir.path(), "bare")
            .unwrap_err()
            .to_string();
        assert!(err.contains("description"), "{}", err);
    }

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
