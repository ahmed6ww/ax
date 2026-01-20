# AX (Agent Package Manager)

> **The npm of the Agentic AI era.**
>
> "Write Once, Run on Claude, Cursor, or Codex."

![Version](https://img.shields.io/badge/version-1.0.0-blue)
![Rust](https://img.shields.io/badge/rust-stable-orange)
![License](https://img.shields.io/badge/license-MIT-green)

## 🚀 What is AX?

AX is a Rust-based CLI tool that solves **Configuration Fatigue** in AI-assisted development. Instead of manually configuring System Prompts, MCP Servers, and Documentation files for every new project, developers simply run:

```bash
ax install code-cleaner
```

AX acts as a **Transpiler**: It reads a universal **Agent Skill Standard** definition and compiles it into the native format of your environment—whether that's Claude Code or Cursor.

## ✨ Features

- 🔄 **Universal Standard**: Directory-based "Skill" format for rich agent definitions
- 🦀 **Rust-Powered**: Single binary, blazing fast
- 🎯 **Multi-Target**: Install to Claude Code or Cursor
- 🛠️ **MCP Support**: Automatic tool configuration
- 📚 **Knowledge Graph**: Static reference files and deterministic scripts
- 🎨 **Beautiful CLI**: Progress bars and colored output

## 📦 Installation

### Pre-built Binary

```bash
curl -fsSL https://raw.githubusercontent.com/ahmed6ww/ax/main/install.sh | sh
```

### From Source

```bash
git clone https://github.com/ahmed6ww/ax
cd ax
cargo build --release
sudo cp target/release/ax /usr/local/bin/
```

## 🎮 Quick Start

### 1. Initialize AX

```bash
ax init
```

This detects your installed editors and creates `~/.ax/config.toml`.

### 2. Browse Available Agents

```bash
ax list
```

Output:
```
  ▶ Available Agents

  NAME                         VERSION    DESCRIPTION
  ──────────────────────────────────────────────────────────────────────────────────────
  code-cleaner                 1.0.0      Enforce "Two Hats" refactoring & strict cleanup
  enterprise-code-architect    2.0.0      Scalable patterns (Hexagonal, Monorepo decisions)
  fastapi-code-cleaner         1.0.0      Pydantic V2 migration & dead code elimination
  fastapi-code-structure       2.0.0      Enterprise dispatch-style project layout
  fastapi-tdd                  1.0.0      "The Quads" testing strategy for Async Python
  nextjs-code-structure        1.0.0      Feature-sliced design for Scalable Next.js
  
  → 6 agent(s) available
  → Install with: ax install <agent-name>
```

### 3. Install an Agent

```bash
# Install to Claude Code (default)
ax install code-cleaner

# Install to Cursor
ax install code-cleaner --target cursor

# Install globally
ax install code-cleaner --global
```

## 📐 The Agent Skill Standard

Agents are no longer single files. They are full directories following the **Skill Standard**:

```
my-agent/
├── SKILL.md          # The Source of Truth (Metadata + Prompt)
├── scripts/          # Python/Bash scripts for deterministic actions
└── references/       # Static knowledge files (MD) for the agent to read
```

### Example: `SKILL.md`

```markdown
---
name: code-cleaner
description: Refactor code to enforce SOLID principles.
version: 1.0.0
allowed-tools: "Read,Write,Bash"
---

# Code Cleaner Identity

You are a Principal Software Engineer acting as the "Code Janitor."
You must strictly adhere to the "Two Hats" metaphor.

## Execution Workflow

1. Run Auto-Linter: `python {baseDir}/scripts/run_ruff.py`
2. Tree Shake: `Read({baseDir}/references/cleanup_rules.md)`
```

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                          AX                             │
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
150: └─────────────────────────────────────────────────────────┘
```

## 🛣️ Roadmap

- [x] Core CLI (init, list, install)
- [x] Claude Code support
- [x] Cursor support
- [x] **Agent Skill Standard (v2)**
- [ ] VS Code extension
- [ ] Private registries
- [ ] `ax create` template generator
- [ ] `ax publish` for community agents

## 📄 License

MIT © [Ahmed](https://github.com/ahmed6ww)

---

<p align="center">
  <strong>Built with 🦀 Rust for the Agentic AI era</strong>
</p>
