//! APM Configuration
//!
//! Manages the APM configuration file at ~/.apm/config.toml

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// APM Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApmConfig {
    /// Default target for installations (claude, cursor)
    pub default_target: String,

    /// Registry URL (defaults to GitHub)
    #[serde(default = "default_registry_url")]
    pub registry_url: String,

    /// Whether to show verbose output
    #[serde(default)]
    pub verbose: bool,
}

fn default_registry_url() -> String {
    "https://raw.githubusercontent.com/ahmed6ww/ax-agents/main".to_string()
}

impl ApmConfig {
    /// Create a new configuration with default settings
    pub fn new(default_target: String) -> Self {
        Self {
            default_target,
            registry_url: default_registry_url(),
            verbose: false,
        }
    }

    /// Load configuration from a file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to a file
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Load configuration or create default
    pub fn load_or_default() -> Result<Self> {
        let path = crate::utils::paths::ax_config_path()?;
        if path.exists() {
            Self::load(&path)
        } else {
            Ok(Self::new("claude".to_string()))
        }
    }
}

impl Default for ApmConfig {
    fn default() -> Self {
        Self::new("claude".to_string())
    }
}
