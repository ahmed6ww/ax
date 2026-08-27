# axur

> **skills.sh helps you find a skill. axur makes sure your whole team is running the same ones.**

A package manager for AI coding-agent setups, targeting **Claude Code** and **Codex**.

Declare what your project's agent needs in `axur.toml`, commit it alongside `axur.lock`, and every teammate — and CI — gets a byte-identical setup from one command.

```bash
axur sync
```

[![CI](https://github.com/agenzalabs/ax/actions/workflows/ci.yml/badge.svg)](https://github.com/agenzalabs/ax/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/rust-stable-orange)
![License](https://img.shields.io/badge/license-MIT-green)

---

## The problem

Claude Code and Codex can be taught things: written instructions (skills), tools they can call (MCP servers), specialist subagents, slash commands, hooks that fire automatically, and permission rules.

Today everyone configures that by hand, per machine. Five developers on one team end up with five subtly different assistants, nobody can say what anyone has installed, and onboarding a new hire means reciting folklore.

## What axur does

```toml
# axur.toml — commit this
[targets]
agents = ["claude-code", "codex"]
scope  = "project"

[skills]
find-skills = { source = "vercel-labs/skills", path = "skills/find-skills" }

[mcp.context7]
command = "npx"
args    = ["-y", "@upstash/context7-mcp"]
```

```console
$ axur sync
== axur sync
- axur.toml
   Claude Code, Codex  ·  project scope
- Resolved 1 source
   ✓ find-skills                435076e  1 file
+ Claude Code   .claude/skills
   ✓ 1 skill
   ✓ 1 MCP server
+ Codex   .agents/skills
   ✓ 1 skill
   ✓ 1 MCP server
== In sync · axur.lock updated — commit it
```

Both targets are written to the locations their vendors actually document — `.claude/skills/` and `.agents/skills/`, not the lookalike directories that quietly never get loaded.

## Three things that make it more than a copier

### It pins exactly what you got

`axur.lock` records the resolved commit SHA and a digest of every file, so a teammate syncing next week gets what you got, not whatever the source has drifted to. Updating a pin is an explicit `axur sync --update`.

### It asks before anything runs on your machine

MCP servers are programs, and hooks are scripts that fire on their own. axur shows the actual command and waits:

```console
! 1 MCP server — each runs on your machine every time the agent starts
   ✗ context7
      npx -y @upstash/context7-mcp
      from axur.toml

  Allow these to run on your machine? [y/N]
```

A run that is not an interactive terminal never approves by default — it exits `7` and tells you to review and re-run with `--yes`.

Approve once and it stays quiet — but if that command ever *changes*, it asks again, so nobody can quietly swap what executes. Approvals live in `~/.axur/trust.toml` on your machine, never in the repo, so cloning a project inherits no one else's decisions. `axur audit` lists what is currently authorised; `axur audit --revoke <name>` withdraws it.

### It can fail a pull request

```yaml
- run: axur sync --check    # exit 2 if the tree has drifted
```

Drift stops being a suggestion.

## Bundles

A bundle ships a whole working environment as one pinned unit — skills, subagents, slash commands, MCP servers, hooks, and permission rules:

```toml
[bundles]
backend = { source = "acme/agent-bundles", path = "bundles/backend" }
```

Anything a target cannot accept is **named, never silently dropped**. Codex has no hooks or slash commands, so a sync says so rather than reporting a success that did not happen:

```
+ Codex   .agents/skills
   ✓ 3 skills
   ·  skipped, unsupported: 2 commands, 1 hook
```

## Installation

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/agenzalabs/ax/main/install.sh | sh

# Windows
irm https://raw.githubusercontent.com/agenzalabs/ax/main/install.ps1 | iex

# From source
cargo install axur
```

Both installers verify the download against the published `SHA256SUMS` and refuse to install on a mismatch. Releases carry build provenance attestation.

## Commands

| Command | What it does |
| --- | --- |
| `axur init` | Create `axur.toml`, detecting which agents you have |
| `axur install <owner/repo#path>` | Add a skill or bundle and sync |
| `axur sync` | Install everything the manifest declares, from the lock |
| `axur sync --check` | Verify only; exit 2 on drift. The CI gate |
| `axur sync --update` | Re-resolve every source and move the pins |
| `axur sync --offline` | Use only cached content; never touch the network |
| `axur uninstall <name>` | Remove an entry and sync |
| `axur list` | Show what this project has installed |
| `axur audit` | Show what is authorised to run on this machine |
| `axur cache` | Inspect or clear the content cache |

### Exit codes

Scriptable, and stable across releases:

| | | | |
| --- | --- | --- | --- |
| `0` ok | `1` failure | `2` drift | `3` not found |
| `4` network | `5` integrity | `6` invalid | `7` denied |

## How it compares

Vercel's [skills.sh](https://skills.sh) supports 76 agents. axur supports two — and that is the whole bet.

Supporting 76 tools means you can only install what all 76 understand: a text file. You cannot install an MCP server, a hook, or a permission rule, because most of them have no such concept. skills.sh also has no version pinning — its lockfile lives in `$HOME` and records what you happen to have, not what the project requires, so two people cannot reliably get the same thing.

|  | skills.sh | axur |
| --- | --- | --- |
| Agents supported | 76 | 2 |
| Skills | ✓ | ✓ |
| Version pinning | — | ✓ commit SHA + file digests |
| Project manifest, committed | — | ✓ |
| CI drift gate | — | ✓ |
| MCP servers | — | ✓ |
| Subagents, commands, hooks, permissions | — | ✓ |
| Consent before code runs | — | ✓ |

## Where files land

| | Claude Code | Codex |
| --- | --- | --- |
| Skills (project) | `.claude/skills/<name>/` | `.agents/skills/<name>/` |
| Skills (user) | `~/.claude/skills/<name>/` | `~/.agents/skills/<name>/` |
| Subagents | `.claude/agents/<name>.md` | *unsupported* |
| Commands | `.claude/commands/<name>.md` | *unsupported* |
| MCP | `.mcp.json` / `~/.claude.json` | `~/.codex/config.toml` |
| Hooks, permissions | `.claude/settings.json` | *unsupported* |

`CLAUDE_CONFIG_DIR` is honoured. Shared config files axur did not author keep one `.axur-bak` generation before any rewrite.

## Environment

| Variable | Effect |
| --- | --- |
| `GITHUB_TOKEN` | Raises the GitHub API rate limit. Recommended |
| `AXUR_HOME` | Override the home directory used for `~/.axur`, `~/.claude`, `~/.agents` |
| `AXUR_OUTPUT` | `rich`, `plain`, or `auto` |
| `AXUR_NO_CACHE` | Disable the content cache |

## Status

Working and tested — 104 tests, exercised end to end against real GitHub repositories.

Being straight about the gaps: it has been run in anger only on Windows so far (CI covers Windows, macOS and Linux, but has not yet had a green run), it is not published to crates.io or npm, and no team other than its author has used it. That last one, not the next feature, is the milestone that matters.

## License

MIT © [Ahmed](https://github.com/ahmed6ww)
