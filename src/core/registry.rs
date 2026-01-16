//! Registry Client
//!
//! Fetches agent configurations from the GitHub registry.

use anyhow::{Context, Result};
use reqwest::Client;

use super::agent::{AgentConfig, AgentInfo};
use super::config::ApmConfig;

/// Registry client for fetching agents
pub struct Registry {
    client: Client,
    base_url: String,
}

impl Registry {
    /// Create a new registry client
    pub fn new() -> Self {
        let config = ApmConfig::load_or_default().unwrap_or_default();
        Self {
            client: Client::new(),
            base_url: config.registry_url,
        }
    }

    /// Create a registry client with a custom base URL
    pub fn with_url(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    /// Fetch the list of available agents
    pub async fn fetch_agents(&self) -> Result<Vec<AgentInfo>> {
        let url = format!("{}/registry.json", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to registry")?;

        if !response.status().is_success() {
            // Return sample agents for demo purposes
            return Ok(self.get_builtin_agents());
        }

        let agents: Vec<AgentInfo> = response
            .json()
            .await
            .context("Failed to parse registry response")?;

        Ok(agents)
    }

    /// Fetch a specific agent configuration
    pub async fn fetch_agent(&self, name: &str) -> Result<AgentConfig> {
        let url = format!("{}/agents/{}.yaml", self.base_url, name);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to registry")?;

        if !response.status().is_success() {
            // Try to load from builtin agents
            if let Some(agent) = self.get_builtin_agent(name) {
                return Ok(agent);
            }
            anyhow::bail!("Agent '{}' not found in registry", name);
        }

        let yaml = response
            .text()
            .await
            .context("Failed to read agent configuration")?;

        let agent: AgentConfig =
            serde_yaml::from_str(&yaml).context("Failed to parse agent configuration")?;

        Ok(agent)
    }

    /// Get builtin/demo agents
    fn get_builtin_agents(&self) -> Vec<AgentInfo> {
        vec![
            AgentInfo {
                name: "rust-architect".to_string(),
                version: "1.0.0".to_string(),
                description: "Senior Rust Systems Engineer optimized for Tokio & zero-cost abstractions".to_string(),
                author: "ahmed6ww".to_string(),
            },
            AgentInfo {
                name: "fullstack-next".to_string(),
                version: "1.0.0".to_string(),
                description: "Next.js 15 + FastAPI + ShadcnUI full-stack expert".to_string(),
                author: "ahmed6ww".to_string(),
            },
            AgentInfo {
                name: "qa-testing-squad".to_string(),
                version: "1.0.0".to_string(),
                description: "Playwright + Jest testing configuration specialist".to_string(),
                author: "ahmed6ww".to_string(),
            },
        ]
    }

    /// Get a specific builtin agent
    fn get_builtin_agent(&self, name: &str) -> Option<AgentConfig> {
        match name {
            "rust-architect" => Some(self.rust_architect_agent()),
            "fullstack-next" => Some(self.fullstack_next_agent()),
            "qa-testing-squad" => Some(self.qa_testing_squad_agent()),
            _ => None,
        }
    }

    fn rust_architect_agent(&self) -> AgentConfig {
        use super::agent::{Identity, McpTool, Skill};
        use std::collections::HashMap;

        AgentConfig {
            name: "rust-architect".to_string(),
            version: "1.0.0".to_string(),
            description: "Senior Rust Systems Engineer optimized for Tokio & zero-cost abstractions".to_string(),
            author: "ahmed6ww".to_string(),
            identity: Identity {
                model: Some("claude-3-5-sonnet-latest".to_string()),
                icon: Some("🦀".to_string()),
                system_prompt: r#"You are a specialized Rust subagent with deep expertise in systems programming.

## Core Principles
- You prefer composition over inheritance
- You use `anyhow` for applications and `thiserror` for libraries
- You strictly follow borrow checker patterns
- You leverage zero-cost abstractions whenever possible
- You write idiomatic Rust that compiles on stable

## Error Handling
- Use `Result<T, E>` for recoverable errors
- Use `panic!` only for unrecoverable bugs
- Provide context with `.context()` from anyhow
- Create custom error types with thiserror for libraries

## Async Patterns
- Use Tokio as the default async runtime
- Prefer `tokio::spawn` for concurrent tasks
- Use `task::spawn_blocking` for CPU-intensive work
- Never block the async runtime

## Memory & Performance
- Minimize allocations where possible
- Use `Cow<str>` for flexible string handling
- Leverage the type system for compile-time guarantees
- Profile before optimizing"#.to_string(),
            },
            skills: vec![
                Skill {
                    name: "tokio-patterns".to_string(),
                    description: Some("Best practices for async programming with Tokio runtime".to_string()),
                    content: r#"# Tokio Best Practices

## Task Management
- Use `tokio::spawn` for fire-and-forget async tasks
- Use `tokio::spawn_blocking` for CPU-heavy synchronous work
- Use `JoinSet` for managing multiple concurrent tasks

## Channels
- Use `mpsc` for multi-producer, single-consumer scenarios
- Use `broadcast` for pub/sub patterns
- Use `oneshot` for request-response patterns

## Timeouts & Cancellation
- Always set timeouts with `tokio::time::timeout`
- Use `CancellationToken` for graceful shutdown
- Handle `JoinError` for task panics"#.to_string(),
                    ..Default::default()
                },
                Skill {
                    name: "error-handling".to_string(),
                    description: Some("Rust error handling patterns using anyhow and thiserror".to_string()),
                    content: r#"# Rust Error Handling Patterns

## For Applications (use anyhow)
```rust
use anyhow::{Context, Result};

fn main() -> Result<()> {
    let config = load_config()
        .context("Failed to load configuration")?;
    Ok(())
}
```

## For Libraries (use thiserror)
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MyError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid input: {message}")]
    InvalidInput { message: String },
}
```"#.to_string(),
                    ..Default::default()
                },
            ],
            mcp: vec![
                McpTool {
                    name: "context7".to_string(),
                    command: "npx".to_string(),
                    args: vec!["-y".to_string(), "@upstash/context7-mcp".to_string()],
                    env: {
                        let mut env = HashMap::new();
                        env.insert("CONTEXT7_API_KEY".to_string(), "${CONTEXT7_API_KEY}".to_string());
                        env
                    },
                    setup_url: Some("https://context7.com/dashboard".to_string()),
                },
            ],
        }
    }

    fn fullstack_next_agent(&self) -> AgentConfig {
        use super::agent::{Identity, McpTool, Skill};
        use std::collections::HashMap;

        AgentConfig {
            name: "fullstack-next".to_string(),
            version: "1.0.0".to_string(),
            description: "Next.js 15 + FastAPI + ShadcnUI full-stack expert".to_string(),
            author: "ahmed6ww".to_string(),
            identity: Identity {
                model: Some("claude-3-5-sonnet-latest".to_string()),
                icon: Some("⚡".to_string()),
                system_prompt: r#"You are a full-stack development expert specializing in modern web applications.

## Tech Stack Expertise
- **Frontend**: Next.js 15 with App Router, React 19, TypeScript
- **UI**: ShadcnUI, Tailwind CSS, Radix UI primitives
- **Backend**: FastAPI (Python), SQLAlchemy, Pydantic
- **Database**: PostgreSQL, Redis for caching

## Next.js 15 Patterns
- Use Server Components by default
- Use 'use client' directive only when needed
- Leverage Server Actions for mutations
- Use Suspense for loading states

## API Design
- RESTful endpoints with FastAPI
- Pydantic models for validation
- Proper HTTP status codes
- OpenAPI documentation

## Best Practices
- TypeScript strict mode
- Zod for runtime validation
- React Query for data fetching
- Proper error boundaries"#.to_string(),
            },
            skills: vec![
                Skill {
                    name: "nextjs-patterns".to_string(),
                    description: Some("Next.js 15 patterns for Server Components, Client Components, and Server Actions".to_string()),
                    content: r#"# Next.js 15 Patterns

## Server Components (Default)
```tsx
// app/users/page.tsx
async function UsersPage() {
  const users = await fetchUsers();
  return <UserList users={users} />;
}
```

## Client Components
```tsx
'use client';
import { useState } from 'react';

export function Counter() {
  const [count, setCount] = useState(0);
  return <button onClick={() => setCount(c => c + 1)}>{count}</button>;
}
```

## Server Actions
```tsx
'use server';
export async function createUser(formData: FormData) {
  // Runs on the server
}
```"#.to_string(),
                    ..Default::default()
                },
            ],
            mcp: vec![
                McpTool {
                    name: "context7".to_string(),
                    command: "npx".to_string(),
                    args: vec!["-y".to_string(), "@upstash/context7-mcp".to_string()],
                    env: {
                        let mut env = HashMap::new();
                        env.insert("CONTEXT7_API_KEY".to_string(), "${CONTEXT7_API_KEY}".to_string());
                        env
                    },
                    setup_url: Some("https://context7.com/dashboard".to_string()),
                },
            ],
        }
    }

    fn qa_testing_squad_agent(&self) -> AgentConfig {
        use super::agent::{Identity, McpTool, Skill};
        use std::collections::HashMap;

        AgentConfig {
            name: "qa-testing-squad".to_string(),
            version: "1.0.0".to_string(),
            description: "Playwright + Jest testing configuration specialist".to_string(),
            author: "ahmed6ww".to_string(),
            identity: Identity {
                model: Some("claude-3-5-sonnet-latest".to_string()),
                icon: Some("🧪".to_string()),
                system_prompt: r#"You are a QA and testing specialist focused on automated testing.

## Testing Expertise
- **E2E Testing**: Playwright for browser automation
- **Unit Testing**: Jest + React Testing Library
- **API Testing**: Supertest, pytest
- **Performance**: Lighthouse, k6

## Testing Principles
- Write tests that provide confidence, not coverage
- Follow the Testing Trophy (more integration tests)
- Use Page Object Model for E2E tests
- Mock at the network boundary

## Playwright Best Practices
- Use locators that are resilient to change
- Prefer user-visible locators (role, text, label)
- Use fixtures for test setup
- Run tests in parallel

## Jest Patterns
- Test behavior, not implementation
- Use describe blocks for organization
- Mock external dependencies only
- Keep tests focused and fast"#.to_string(),
            },
            skills: vec![
                Skill {
                    name: "playwright-setup".to_string(),
                    description: Some("Playwright configuration and Page Object Model patterns for E2E testing".to_string()),
                    content: r#"# Playwright Configuration

## playwright.config.ts
```typescript
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  retries: process.env.CI ? 2 : 0,
  reporter: 'html',
  use: {
    baseURL: 'http://localhost:3000',
    trace: 'on-first-retry',
  },
});
```

## Page Object Example
```typescript
export class LoginPage {
  constructor(private page: Page) {}

  async login(email: string, password: string) {
    await this.page.getByLabel('Email').fill(email);
    await this.page.getByLabel('Password').fill(password);
    await this.page.getByRole('button', { name: 'Sign in' }).click();
  }
}
```"#.to_string(),
                    ..Default::default()
                },
            ],
            mcp: vec![
                McpTool {
                    name: "context7".to_string(),
                    command: "npx".to_string(),
                    args: vec!["-y".to_string(), "@upstash/context7-mcp".to_string()],
                    env: {
                        let mut env = HashMap::new();
                        env.insert("CONTEXT7_API_KEY".to_string(), "${CONTEXT7_API_KEY}".to_string());
                        env
                    },
                    setup_url: Some("https://context7.com/dashboard".to_string()),
                },
            ],
        }
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}
