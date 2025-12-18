//! Path Utilities
//!
//! Cross-platform path resolution for configuration directories.

use anyhow::Result;
use std::path::PathBuf;

/// Get the APM configuration directory (~/.apm)
pub fn apm_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    Ok(home.join(".apm"))
}

/// Get the APM configuration file path (~/.apm/config.toml)
pub fn apm_config_path() -> Result<PathBuf> {
    Ok(apm_config_dir()?.join("config.toml"))
}

/// Get the Claude configuration directory
/// 
/// On macOS: ~/Library/Application Support/Claude
/// On Linux: ~/.config/claude or ~/.claude
/// On Windows: %APPDATA%/Claude
pub fn claude_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library/Application Support/Claude"))
    }

    #[cfg(target_os = "linux")]
    {
        // Try XDG config first, then fallback to ~/.claude
        if let Some(config) = dirs::config_dir() {
            let claude_dir = config.join("claude");
            if claude_dir.exists() {
                return Some(claude_dir);
            }
        }
        dirs::home_dir().map(|h| h.join(".claude"))
    }

    #[cfg(target_os = "windows")]
    {
        dirs::config_dir().map(|d| d.join("Claude"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        dirs::home_dir().map(|h| h.join(".claude"))
    }
}

/// Get the Cursor global configuration directory
///
/// On macOS: ~/Library/Application Support/Cursor
/// On Linux: ~/.config/Cursor
/// On Windows: %APPDATA%/Cursor
pub fn cursor_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library/Application Support/Cursor"))
    }

    #[cfg(target_os = "linux")]
    {
        dirs::config_dir().map(|d| d.join("Cursor"))
    }

    #[cfg(target_os = "windows")]
    {
        dirs::config_dir().map(|d| d.join("Cursor"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        dirs::home_dir().map(|h| h.join(".cursor"))
    }
}

/// Get the VS Code configuration directory
pub fn vscode_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library/Application Support/Code"))
    }

    #[cfg(target_os = "linux")]
    {
        dirs::config_dir().map(|d| d.join("Code"))
    }

    #[cfg(target_os = "windows")]
    {
        dirs::config_dir().map(|d| d.join("Code"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}
