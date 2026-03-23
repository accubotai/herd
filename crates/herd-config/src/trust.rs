use std::collections::{HashMap, HashSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

/// Tracks which commands have been trusted per-project.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustStore {
    /// Map of project path to trusted command set.
    #[serde(default)]
    pub projects: HashMap<String, ProjectTrust>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectTrust {
    /// SHA-256 hashes of trusted command strings.
    #[serde(default)]
    pub trusted_commands: HashSet<String>,
    /// Whether all commands are trusted — scoped to a specific config hash.
    /// When the config file changes, this is invalidated.
    #[serde(default)]
    pub trust_all_config_hash: Option<String>,
}

impl TrustStore {
    /// Default path: `~/.config/herd/trust.toml`
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("herd")
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
        std::fs::write(path, &content)?;
        // Restrict permissions to owner only (0600)
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)?;
        Ok(())
    }

    /// Check if a command is trusted for a given project.
    ///
    /// `config_hash` is the SHA-256 of the current config file content.
    /// If `trust_all` was set for a different config hash, it is invalidated.
    pub fn is_trusted(&self, project_path: &str, command_hash: &str, config_hash: &str) -> bool {
        self.projects.get(project_path).is_some_and(|pt| {
            let trust_all_valid = pt
                .trust_all_config_hash
                .as_ref()
                .is_some_and(|h| h == config_hash);
            trust_all_valid || pt.trusted_commands.contains(command_hash)
        })
    }

    pub fn trust_command(&mut self, project_path: &str, command_hash: String) {
        let project = self.projects.entry(project_path.to_string()).or_default();
        project.trusted_commands.insert(command_hash);
    }

    /// Trust all commands for a project, scoped to the current config file hash.
    /// If the config file changes, this trust is automatically invalidated.
    pub fn trust_all(&mut self, project_path: &str, config_hash: String) {
        let project = self.projects.entry(project_path.to_string()).or_default();
        project.trust_all_config_hash = Some(config_hash);
    }
}

/// Compute SHA-256 hash of a command with its context for trust verification.
pub fn hash_command<S: std::hash::BuildHasher>(
    command: &str,
    working_dir: Option<&str>,
    env: &HashMap<String, String, S>,
) -> String {
    use std::collections::BTreeMap;
    let sorted_env: BTreeMap<_, _> = env.iter().collect();
    let input = format!(
        "cmd:{}\ndir:{}\nenv:{:?}",
        command,
        working_dir.unwrap_or("."),
        sorted_env
    );
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Compute SHA-256 hash of a config file's content.
pub fn hash_config_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_trust_store_default_empty() {
        let store = TrustStore::default();
        assert!(!store.is_trusted("/project", "abc123", "confighash"));
    }

    #[test]
    fn test_trust_command() {
        let mut store = TrustStore::default();
        store.trust_command("/project", "abc123".to_string());
        assert!(store.is_trusted("/project", "abc123", "any"));
        assert!(!store.is_trusted("/project", "other", "any"));
        assert!(!store.is_trusted("/other-project", "abc123", "any"));
    }

    #[test]
    fn test_trust_all_valid_config() {
        let mut store = TrustStore::default();
        store.trust_all("/project", "config_v1".to_string());
        // Trusted when config hash matches
        assert!(store.is_trusted("/project", "anything", "config_v1"));
        // Not trusted when config hash differs (config changed)
        assert!(!store.is_trusted("/project", "anything", "config_v2"));
        // Not trusted for other projects
        assert!(!store.is_trusted("/other", "anything", "config_v1"));
    }

    #[test]
    fn test_trust_all_invalidated_on_config_change() {
        let mut store = TrustStore::default();
        store.trust_all("/project", "old_hash".to_string());
        assert!(store.is_trusted("/project", "cmd", "old_hash"));
        // Config changed — trust_all no longer valid
        assert!(!store.is_trusted("/project", "cmd", "new_hash"));
        // But individually trusted commands still work
        store.trust_command("/project", "cmd".to_string());
        assert!(store.is_trusted("/project", "cmd", "new_hash"));
    }

    #[test]
    fn test_hash_is_sha256() {
        let env = HashMap::new();
        let h = hash_command("npm run dev", None, &env);
        // SHA-256 produces 64 hex chars
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
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

    #[test]
    fn test_config_hash() {
        let h1 = hash_config_content("[project]\nname = \"app\"");
        let h2 = hash_config_content("[project]\nname = \"app\"");
        let h3 = hash_config_content("[project]\nname = \"changed\"");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn test_trusted_commands_uses_hashset() {
        let mut store = TrustStore::default();
        // Insert same hash twice — should not duplicate
        store.trust_command("/project", "abc".to_string());
        store.trust_command("/project", "abc".to_string());
        assert_eq!(store.projects["/project"].trusted_commands.len(), 1);
    }
}
