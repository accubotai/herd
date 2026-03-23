use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::env_resolver;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("config file not found: {0}")]
    NotFound(PathBuf),
}

/// Root configuration structure for herd.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HerdConfig {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub process: Vec<ProcessConfig>,
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub auto_restart: bool,
    #[serde(default = "default_section")]
    pub section: String,
    #[serde(default)]
    pub lazy: bool,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub restart_delay_ms: Option<u64>,
    #[serde(default)]
    pub watch: Option<WatchConfig>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatchConfig {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiConfig {
    #[serde(default)]
    pub mcp_enabled: bool,
    #[serde(default = "default_mcp_transport")]
    pub mcp_transport: String,
    #[serde(default)]
    pub providers: HashMap<String, AiProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    pub api_key: String,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            font_family: default_font_family(),
            font_size: default_font_size(),
            sidebar_width: default_sidebar_width(),
        }
    }
}

fn default_section() -> String {
    "services".to_string()
}
fn default_mcp_transport() -> String {
    "stdio".to_string()
}
fn default_theme() -> String {
    "dark".to_string()
}
fn default_font_family() -> String {
    "monospace".to_string()
}
fn default_font_size() -> f32 {
    14.0
}
fn default_sidebar_width() -> u32 {
    280
}

impl HerdConfig {
    /// Load config from a herd.toml file path
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::NotFound(path.to_path_buf()));
        }
        let content = std::fs::read_to_string(path)?;
        let mut config: HerdConfig = toml::from_str(&content)?;

        // Resolve environment variables in process configs
        for process in &mut config.process {
            process.command = env_resolver::resolve(&process.command);
            for value in process.env.values_mut() {
                *value = env_resolver::resolve(value);
            }
        }

        // Resolve env vars in AI provider keys
        for provider in config.ai.providers.values_mut() {
            provider.api_key = env_resolver::resolve(&provider.api_key);
        }

        Ok(config)
    }

    /// Load config, searching upward from the given directory
    pub fn find_and_load(start_dir: &Path) -> Result<Self, ConfigError> {
        let mut dir = start_dir.to_path_buf();
        loop {
            let candidate = dir.join("herd.toml");
            if candidate.exists() {
                return Self::load(&candidate);
            }
            if !dir.pop() {
                return Err(ConfigError::NotFound("herd.toml".into()));
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let toml_str = r#"
[project]
name = "test-app"

[[process]]
name = "Server"
command = "npm run dev"
"#;
        let config: HerdConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.project.name, "test-app");
        assert_eq!(config.process.len(), 1);
        assert_eq!(config.process[0].name, "Server");
        assert_eq!(config.process[0].command, "npm run dev");
        assert_eq!(config.process[0].section, "services");
        assert!(!config.process[0].auto_restart);
        assert!(!config.process[0].lazy);
    }

    #[test]
    fn test_parse_full_config() {
        let toml_str = r#"
[project]
name = "my-app"

[[process]]
name = "Dev Server"
command = "npm run dev"
working_dir = "."
auto_restart = true
section = "services"

[[process]]
name = "Claude Code"
command = "claude"
section = "agents"
interactive = true
lazy = true

[ai]
mcp_enabled = true
mcp_transport = "stdio"

[ai.providers.anthropic]
api_key = "sk-test-key"
model = "claude-sonnet-4-20250514"

[ui]
theme = "dark"
font_family = "JetBrains Mono"
font_size = 14.0
sidebar_width = 300
"#;
        let config: HerdConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.process.len(), 2);
        assert!(config.process[0].auto_restart);
        assert!(config.process[1].interactive);
        assert!(config.process[1].lazy);
        assert!(config.ai.mcp_enabled);
        assert_eq!(config.ui.theme, "dark");
        assert_eq!(config.ui.sidebar_width, 300);
    }

    #[test]
    fn test_parse_process_with_watch_and_env() {
        let toml_str = r#"
[[process]]
name = "Worker"
command = "php artisan queue:work"
auto_restart = true
restart_delay_ms = 1000

[process.watch]
paths = ["app/", "config/"]
ignore = ["storage/"]

[process.env]
APP_ENV = "local"
DEBUG = "true"
"#;
        let config: HerdConfig = toml::from_str(toml_str).unwrap();
        let proc = &config.process[0];
        assert_eq!(proc.restart_delay_ms, Some(1000));
        let watch = proc.watch.as_ref().unwrap();
        assert_eq!(watch.paths, vec!["app/", "config/"]);
        assert_eq!(watch.ignore, vec!["storage/"]);
        assert_eq!(proc.env.get("APP_ENV").unwrap(), "local");
    }

    #[test]
    fn test_defaults() {
        let config: HerdConfig = toml::from_str("").unwrap();
        assert_eq!(config.ui.theme, "dark");
        assert!((config.ui.font_size - 14.0).abs() < f32::EPSILON);
        assert_eq!(config.ui.sidebar_width, 280);
        assert!(!config.ai.mcp_enabled);
    }
}
