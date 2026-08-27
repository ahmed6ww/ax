//! APM Configuration
//!
//! Manages the APM configuration file at ~/.apm/config.toml

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// APM Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default target for installations (claude-code, codex)
    pub default_target: String,

    /// Whether to show verbose output
    #[serde(default)]
    pub verbose: bool,
}

impl Config {
    /// Create a new configuration with default settings
    pub fn new(default_target: String) -> Self {
        Self {
            default_target,
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
        let path = crate::utils::paths::axur_config_path()?;
        if path.exists() {
            Self::load(&path)
        } else {
            Ok(Self::new("claude".to_string()))
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new("claude".to_string())
    }
}
