use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum TrustError {
    #[error("failed to read trust file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse trust file: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize trust file: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Tracks which commands have been trusted per-project
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustStore {
    /// Map of project path → set of trusted command hashes
    #[serde(default)]
    pub projects: HashMap<String, ProjectTrust>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectTrust {
    /// SHA-256 hashes of trusted command strings
    #[serde(default)]
    pub trusted_commands: Vec<String>,
    /// Whether all commands in this project are trusted
    #[serde(default)]
    pub trust_all: bool,
}

impl TrustStore {
    /// Default path: ~/.config/soloterm/trust.toml
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("soloterm")
            .join("trust.toml")
    }

    pub fn load(path: &Path) -> Result<Self, TrustError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), TrustError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn is_trusted(&self, project_path: &str, command_hash: &str) -> bool {
        self.projects.get(project_path).is_some_and(|pt| {
            pt.trust_all || pt.trusted_commands.contains(&command_hash.to_string())
        })
    }

    pub fn trust_command(&mut self, project_path: &str, command_hash: String) {
        let project = self
            .projects
            .entry(project_path.to_string())
            .or_default();
        if !project.trusted_commands.contains(&command_hash) {
            project.trusted_commands.push(command_hash);
        }
    }

    pub fn trust_all(&mut self, project_path: &str) {
        let project = self
            .projects
            .entry(project_path.to_string())
            .or_default();
        project.trust_all = true;
    }
}

/// Compute a simple hash of a command string for trust comparison
pub fn hash_command(command: &str, working_dir: Option<&str>, env: &HashMap<String, String>) -> String {
    use std::collections::BTreeMap;
    // Sort env for deterministic hashing
    let sorted_env: BTreeMap<_, _> = env.iter().collect();
    let input = format!(
        "cmd:{}\ndir:{}\nenv:{:?}",
        command,
        working_dir.unwrap_or("."),
        sorted_env
    );
    // Simple hash — not cryptographic, just for identity
    format!("{:x}", fxhash(&input))
}

fn fxhash(s: &str) -> u64 {
    let mut hash: u64 = 0;
    for byte in s.bytes() {
        hash = hash.rotate_left(5) ^ (byte as u64);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trust_store_default_empty() {
        let store = TrustStore::default();
        assert!(!store.is_trusted("/project", "abc123"));
    }

    #[test]
    fn test_trust_command() {
        let mut store = TrustStore::default();
        store.trust_command("/project", "abc123".to_string());
        assert!(store.is_trusted("/project", "abc123"));
        assert!(!store.is_trusted("/project", "other"));
        assert!(!store.is_trusted("/other-project", "abc123"));
    }

    #[test]
    fn test_trust_all() {
        let mut store = TrustStore::default();
        store.trust_all("/project");
        assert!(store.is_trusted("/project", "anything"));
        assert!(!store.is_trusted("/other", "anything"));
    }

    #[test]
    fn test_hash_deterministic() {
        let env = HashMap::from([("KEY".to_string(), "val".to_string())]);
        let h1 = hash_command("npm run dev", Some("."), &env);
        let h2 = hash_command("npm run dev", Some("."), &env);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_differs_for_different_commands() {
        let env = HashMap::new();
        let h1 = hash_command("npm run dev", None, &env);
        let h2 = hash_command("npm run build", None, &env);
        assert_ne!(h1, h2);
    }
}
