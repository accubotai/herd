use std::path::Path;

use crate::solo_config::{ConfigError, SoloConfig};

/// Merge a `.solo.local` file into an existing config.
///
/// Local processes are appended to the process list.
/// Local UI overrides replace existing values.
pub fn merge_local(config: &mut SoloConfig, local_path: &Path) -> Result<(), ConfigError> {
    if !local_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(local_path)?;
    let local: SoloConfig = toml::from_str(&content)?;

    // Append local processes
    for proc in local.process {
        config.process.push(proc);
    }

    // Override UI settings if specified in local config (non-default values)
    if local.ui.theme != "dark" {
        config.ui.theme = local.ui.theme;
    }
    #[allow(clippy::float_cmp)]
    if local.ui.font_size != 14.0 {
        config.ui.font_size = local.ui.font_size;
    }

    Ok(())
}

/// Find the `.solo.local` file next to a `solo.toml`.
pub fn local_path_for(config_path: &Path) -> Option<std::path::PathBuf> {
    config_path.parent().map(|dir| dir.join(".solo.local"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_missing_local_is_noop() {
        let mut config = SoloConfig::default();
        let result = merge_local(&mut config, Path::new("/nonexistent/.solo.local"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_local_path_for() {
        let path = local_path_for(Path::new("/home/user/project/solo.toml"));
        assert_eq!(path.unwrap(), Path::new("/home/user/project/.solo.local"));
    }
}
