//! Configuration module for Sashiki
//!
//! Handles loading and saving of configuration from TOML files.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse config file: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("Failed to serialize config: {0}")]
    SerializeError(#[from] toml::ser::Error),
    #[error("Could not determine config directory")]
    NoConfigDir,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
}

impl Default for Theme {
    fn default() -> Self {
        Self::Dark
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "monospace".to_string(),
            size: 14.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    pub shell_path: Option<String>,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self { shell_path: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    pub sidebar_width: f32,
    pub terminal_height: f32,
    pub terminal_visible: bool,
    pub split_horizontal: bool,
    pub split_ratio: f32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            sidebar_width: 200.0,
            terminal_height: 200.0,
            terminal_visible: true,
            split_horizontal: true,
            split_ratio: 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub theme: Theme,
    pub font: FontConfig,
    pub terminal: TerminalConfig,
    #[serde(default)]
    pub layout: LayoutConfig,
}

impl Config {
    pub fn config_dir() -> Result<PathBuf, ConfigError> {
        ProjectDirs::from("com", "secchanu", "sashiki")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .ok_or(ConfigError::NoConfigDir)
    }

    pub fn config_path() -> Result<PathBuf, ConfigError> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.theme, Theme::Dark);
        assert_eq!(config.font.family, "monospace");
        assert_eq!(config.font.size, 14.0);
        assert!(config.terminal.shell_path.is_none());
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.theme, config.theme);
        assert_eq!(parsed.font.family, config.font.family);
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
            theme = "light"

            [font]
            family = "Fira Code"
            size = 16.0

            [terminal]
            shell_path = "/bin/zsh"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.theme, Theme::Light);
        assert_eq!(config.font.family, "Fira Code");
        assert_eq!(config.font.size, 16.0);
        assert_eq!(config.terminal.shell_path, Some("/bin/zsh".to_string()));
    }
}
