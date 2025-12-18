# APM (Agent Package Manager)

> **The npm of the Agentic AI era.**
>
> "Write Once, Run on Claude, Cursor, or Codex."

![Version](https://img.shields.io/badge/version-1.0.0-blue)
![Rust](https://img.shields.io/badge/rust-stable-orange)
![License](https://img.shields.io/badge/license-MIT-green)

## 🚀 What is APM?

APM is a Rust-based CLI tool that solves **Configuration Fatigue** in AI-assisted development. Instead of manually configuring System Prompts, MCP Servers, and Documentation files for every new project, developers simply run:

```bash
apm install rust-architect
```

APM acts as a **Transpiler**: It reads a universal `agent.yaml` definition and compiles it into the native format of your environment—whether that's Claude Code or Cursor.

## ✨ Features

- 🔄 **Universal Schema**: One `agent.yaml` to rule them all
- 🦀 **Rust-Powered**: Single binary, blazing fast
- 🎯 **Multi-Target**: Install to Claude Code or Cursor
- 🛠️ **MCP Support**: Automatic tool configuration
- 📚 **Skills System**: Knowledge base as markdown files
- 🎨 **Beautiful CLI**: Progress bars and colored output

## 📦 Installation

### Pre-built Binary

```bash
curl -fsSL https://raw.githubusercontent.com/ahmed6ww/apm/main/install.sh | sh
```

### From Source

```bash
git clone https://github.com/ahmed6ww/apm
cd apm
cargo build --release
sudo cp target/release/apm /usr/local/bin/
```

## 🎮 Quick Start

### 1. Initialize APM

```bash
apm init
```

This detects your installed editors and creates `~/.apm/config.toml`.

### 2. Browse Available Agents

```bash
apm list
```

Output:
```
  ▶ Available Agents

  NAME                 VERSION    DESCRIPTION                              AUTHOR
  ─────────────────────────────────────────────────────────────────────────────────
  rust-architect       1.0.0      Senior Rust Systems Engineer...         ahmed6ww
  fullstack-next       1.0.0      Next.js 15 + FastAPI + ShadcnUI...      ahmed6ww
  qa-testing-squad     1.0.0      Playwright + Jest testing...            ahmed6ww

  → 3 agent(s) available
  → Install with: apm install <agent-name>
```

### 3. Install an Agent

```bash
# Install to Claude Code (default)
apm install rust-architect

# Install to Cursor
apm install rust-architect --target cursor

# Install globally
apm install rust-architect --global
```

## 📐 The Universal Schema

All agents follow the `agent.yaml` schema:

```yaml
# agent.yaml - The Source of Truth
name: "rust-architect"
version: "1.0.0"
description: "Senior Rust Systems Engineer"
author: "ahmed6ww"

# 1. Identity (The Brain) - Becomes the System Prompt
identity:
  model: "claude-3-5-sonnet-latest"
  icon: "🦀"
  system_prompt: |
    You are a specialized Rust subagent.
    - You prefer composition over inheritance.
    - You use `anyhow` for apps and `thiserror` for libs.

# 2. Skills (The Knowledge) - Becomes Markdown files
skills:
  - name: "tokio-patterns"
    content: |
      # Tokio Best Practices
      - Use `tokio::spawn` for async tasks.
      - Use `task::spawn_blocking` for CPU-heavy work.

# 3. Tools (The Hands) - Becomes MCP Server configs
mcp:
  - name: "cargo-mcp"
    command: "cargo"
    args: ["mcp-server"]
    env:
      RUST_LOG: "info"
```

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                          APM                            │
├─────────────────────────────────────────────────────────┤
│  ┌─────────┐    ┌─────────────┐    ┌─────────────────┐ │
│  │  init   │    │    list     │    │     install     │ │
│  └────┬────┘    └──────┬──────┘    └────────┬────────┘ │
│       │                │                    │          │
│       v                v                    v          │
│  ┌─────────────────────────────────────────────────┐   │
│  │                   Registry                      │   │
│  │            (GitHub Raw Content)                 │   │
│  └─────────────────────────────────────────────────┘   │
│                          │                             │
│                          v                             │
│  ┌─────────────────────────────────────────────────┐   │
│  │              Installer Trait                    │   │
│  ├─────────────────────┬───────────────────────────┤   │
│  │   ClaudeInstaller   │     CursorInstaller      │   │
│  │   ~/.claude/*       │     .cursor/rules/*       │   │
│  └─────────────────────┴───────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## 📁 Output Structure

### Claude Code

```
~/.claude/
├── agents/
│   └── rust-architect.json     # Identity (system prompt)
├── skills/
│   └── rust-architect/
│       ├── tokio-patterns.md   # Skill 1
│       └── error-handling.md   # Skill 2
└── claude_desktop_config.json  # MCP tools (patched)
```

### Cursor

```
.cursor/
├── rules/
│   ├── rust-architect-identity.mdc    # Identity (MDC format)
│   ├── rust-architect-tokio-patterns.mdc
│   └── rust-architect-error-handling.mdc
└── mcp.json                           # MCP tools
```

## 🛣️ Roadmap

- [x] Core CLI (init, list, install)
- [x] Claude Code support
- [x] Cursor support
- [ ] VS Code extension
- [ ] Agent versioning & updates
- [ ] Private registries
- [ ] `apm create` template generator
- [ ] `apm publish` for community agents

## 📄 License

MIT © [Ahmed](https://github.com/ahmed6ww)

---

<p align="center">
  <strong>Built with 🦀 Rust for the Agentic AI era</strong>
</p>
