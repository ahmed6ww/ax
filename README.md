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

`[targets].agents` is the set a project has content for — which of those *you* actually run is your own choice, not whoever committed `axur.toml` first. The first `axur sync` on a new machine asks:

```console
◆  Which agents do you use?
│  ◻ Claude Code  (detected)
│  ◻ Codex
```

Check both, and both get installed; check one, and only that one does. The answer is remembered per project in `~/.axur/projects.toml`, on your machine only — never in the repo, so cloning a project never inherits whoever synced first. `axur sync --agents claude-code,codex` sets it without asking, which is what CI should pass so it never hits the prompt.

## Four things that make it more than a copier

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

### It never writes a secret to a file it commits

An MCP server that needs an API key is declared by *name*, not value:

```toml
[mcp.linear]
command = "npx"
args    = ["-y", "mcp-remote", "https://mcp.linear.app/sse"]
env     = { LINEAR_API_KEY = "${LINEAR_API_KEY}" }
```

`${LINEAR_API_KEY}` is a reference, resolved from your own shell — axur never reads or stores the value. Claude Code and Codex expect that differently, so axur compiles the same declaration into what each one actually understands: Claude Code's `.mcp.json` keeps the `${VAR}` form, which it expands itself; Codex's `config.toml` has no such expansion, so the name goes into `env_vars` instead, forwarded from Codex's own process environment. Either way, the committed file never holds more than a name. If the variable isn't set when you sync, axur says so before writing anything, rather than shipping a config that silently never authenticates.

`${LINEAR_API_KEY}` still has to come from *somewhere* — usually a line in a shell profile everyone forgets is there. `axur secrets set LINEAR_API_KEY` stores it in the OS keychain instead (Windows Credential Manager, macOS Keychain, the Linux Secret Service), prompted, masked, never a command-line argument. Nothing about `axur.toml` changes — the same `${LINEAR_API_KEY}` reference now resolves from there instead. Since neither target can read a keychain itself, axur points that server's `command` at itself:

```console
$ axur secrets set LINEAR_API_KEY
? Value for LINEAR_API_KEY: ••••••••••••••••
✓ LINEAR_API_KEY stored in the OS keychain
```

```json
"linear": {
  "command": "axur",
  "args": ["secrets", "exec", "--name", "LINEAR_API_KEY", "--", "npx", "-y", "mcp-remote", "https://mcp.linear.app/sse"]
}
```

which resolves the value and becomes the real command — the consent prompt shows this exact line, since it's what will actually run. `axur secrets` lists what's stored; `axur secrets unset <NAME>` removes it.

### It can fail a pull request

```yaml
- run: axur sync --check    # exit 2 if the tree has drifted
```

Drift stops being a suggestion.

## Skills your team wrote, not GitHub's

A skill doesn't have to come from a public repository. `source = "local"` points at a directory in *this* project instead — the way to share something specific to your team without publishing it anywhere:

```toml
[skills]
onboarding = { source = "local", path = ".axur/skills/onboarding" }
```

One developer authors `.axur/skills/onboarding/SKILL.md`, commits it next to `axur.toml`, and everyone else's `axur sync` installs it exactly like a GitHub-sourced skill — same lockfile digest, same drift check, no network round trip. There's no ref to pin: the file in the working tree *is* the pinned content.

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

### Org-level defaults

A bundle is also how a company keeps one golden-path agent setup instead of N slightly different copies. Publish it once:

```bash
axur install your-org/your-bundles#team-defaults
```

and every repo that runs this gets the same permission rules, guard hooks, and subagents — pinned to a commit, same as anything else axur installs. Rolling out a change to the whole org is a commit in the bundle repo, then `axur sync --update` in each project; nobody hand-edits `settings.json` per repo. `[bundles]` isn't limited to one entry, so a repo can combine the org default with something of its own:

```toml
[bundles]
team-defaults = { source = "your-org/your-bundles", path = "team-defaults" }
backend       = { source = "your-org/your-bundles", path = "backend" }
```

[**agenzalabs/ax-bundles**](https://github.com/agenzalabs/ax-bundles) is a working example — a `team-defaults` bundle meant to be forked and adapted, plus two content bundles (Next.js App Router conventions, Rust CLI patterns) showing the non-org-defaults case.

## Installation

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/agenzalabs/ax/main/install.sh | sh

# Windows
irm https://raw.githubusercontent.com/agenzalabs/ax/main/install.ps1 | iex

# npm
npm install -g @axur/cli

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
| `axur sync --agents <list>` | Set which agents to install for on this machine, e.g. `claude-code,codex` |
| `axur uninstall <name>` | Remove an entry and sync |
| `axur list` | Show what this project has installed |
| `axur audit` | Show what is authorised to run on this machine |
| `axur secrets set <NAME>` | Store an API key or token in the OS keychain, prompted |
| `axur secrets unset <NAME>` | Remove a stored secret |
| `axur secrets` | List what's stored |
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
