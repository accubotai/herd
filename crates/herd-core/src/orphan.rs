//! Orphan process tracking and cleanup.
//!
//! Persists PIDs to disk so that if Herd crashes, orphaned child
//! processes can be cleaned up on the next launch.
//! Linux-specific: uses `/proc/PID` to check liveness.

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrphanError {
    #[error("failed to read PID file: {0}")]
    Io(#[from] std::io::Error),
}

/// Tracks PIDs of managed processes to detect and clean up orphans on restart.
pub struct OrphanTracker {
    pid_file: PathBuf,
    pids: HashSet<u32>,
}

impl OrphanTracker {
    /// Create a new tracker. PID file stored at `~/.config/herd/pids/<project_id>.pids`.
    pub fn new(project_id: &str) -> Self {
        let pid_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("herd")
            .join("pids");

        Self {
            pid_file: pid_dir.join(format!("{project_id}.pids")),
            pids: HashSet::new(),
        }
    }

    /// Register a new PID.
    pub fn register(&mut self, pid: u32) {
        self.pids.insert(pid);
        if let Err(e) = self.save() {
            tracing::warn!(pid, error = %e, "Failed to persist PID file");
        }
    }

    /// Unregister a PID (process exited normally).
    pub fn unregister(&mut self, pid: u32) {
        self.pids.remove(&pid);
        if let Err(e) = self.save() {
            tracing::warn!(pid, error = %e, "Failed to update PID file");
        }
    }

    /// Check for and kill orphaned processes from a previous session.
    pub fn cleanup_orphans(&mut self) -> Vec<u32> {
        let mut killed = Vec::new();

        if let Ok(content) = fs::read_to_string(&self.pid_file) {
            for line in content.lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    if is_process_alive(pid) {
                        if let Some(nix_pid) = to_nix_pid(pid) {
                            tracing::warn!(pid, "Killing orphaned process");
                            if let Err(e) =
                                nix::sys::signal::kill(nix_pid, nix::sys::signal::Signal::SIGTERM)
                            {
                                tracing::warn!(pid, error = %e, "Failed to kill orphan");
                            } else {
                                killed.push(pid);
                            }
                        }
                    }
                }
            }
        }

        self.pids.clear();
        if let Err(e) = self.save() {
            tracing::warn!(error = %e, "Failed to clear PID file after cleanup");
        }

        killed
    }

    fn save(&self) -> Result<(), OrphanError> {
        if let Some(parent) = self.pid_file.parent() {
            fs::create_dir_all(parent)?;
            // Restrict directory to owner only
            let dir_perms = fs::Permissions::from_mode(0o700);
            let _ = fs::set_permissions(parent, dir_perms);
        }
        let content: String = self
            .pids
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&self.pid_file, &content)?;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&self.pid_file, perms)?;
        Ok(())
    }
}

/// Convert u32 PID to `nix::Pid`, rejecting invalid values (0, overflow).
fn to_nix_pid(pid: u32) -> Option<nix::unistd::Pid> {
    i32::try_from(pid)
        .ok()
        .filter(|&p| p > 0)
        .map(nix::unistd::Pid::from_raw)
}

/// Check if a process is alive (Linux-specific: checks `/proc/PID`).
fn is_process_alive(pid: u32) -> bool {
    PathBuf::from(format!("/proc/{pid}")).exists()
}

impl Drop for OrphanTracker {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.pid_file) {
            // Only warn if the file existed (ignore NotFound)
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, "Failed to remove PID file on exit");
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create an `OrphanTracker` that writes into a temp directory
    /// instead of the real config dir. We manually set `pid_file` via
    /// field access (fields are crate-private, and tests live in the
    /// same module).
    fn tracker_in(dir: &TempDir, name: &str) -> OrphanTracker {
        OrphanTracker {
            pid_file: dir.path().join(format!("{name}.pids")),
            pids: HashSet::new(),
        }
    }

    #[test]
    fn new_creates_tracker_with_correct_path() {
        let tracker = OrphanTracker::new("my-project");
        let path_str = tracker.pid_file.to_string_lossy();
        assert!(
            path_str.ends_with("herd/pids/my-project.pids"),
            "unexpected pid_file path: {path_str}"
        );
    }

    #[test]
    fn register_adds_pid_and_unregister_removes_it() {
        let dir = TempDir::new().unwrap();
        let mut tracker = tracker_in(&dir, "reg");

        tracker.register(1001);
        tracker.register(1002);
        assert!(tracker.pids.contains(&1001));
        assert!(tracker.pids.contains(&1002));

        tracker.unregister(1001);
        assert!(!tracker.pids.contains(&1001));
        assert!(tracker.pids.contains(&1002));
    }

    #[test]
    fn cleanup_orphans_on_empty_file_returns_empty_vec() {
        let dir = TempDir::new().unwrap();
        // Write an empty file so `read_to_string` succeeds but yields nothing
        let pid_path = dir.path().join("empty.pids");
        fs::write(&pid_path, "").unwrap();

        let mut tracker = OrphanTracker {
            pid_file: pid_path,
            pids: HashSet::new(),
        };

        let killed = tracker.cleanup_orphans();
        assert!(killed.is_empty());
    }

    #[test]
    fn cleanup_orphans_on_missing_file_returns_empty_vec() {
        let dir = TempDir::new().unwrap();
        let mut tracker = tracker_in(&dir, "missing");
        // File does not exist — cleanup should still succeed
        let killed = tracker.cleanup_orphans();
        assert!(killed.is_empty());
    }

    #[test]
    fn round_trip_register_save_load() {
        let dir = TempDir::new().unwrap();
        let mut tracker = tracker_in(&dir, "roundtrip");

        tracker.register(42);
        tracker.register(99);

        // Read raw file and verify both PIDs are present
        let content = fs::read_to_string(&tracker.pid_file).unwrap();
        let saved_pids: HashSet<u32> = content
            .lines()
            .filter_map(|l| l.trim().parse::<u32>().ok())
            .collect();
        assert!(saved_pids.contains(&42));
        assert!(saved_pids.contains(&99));
        assert_eq!(saved_pids.len(), 2);
    }

    #[test]
    fn save_persists_after_unregister() {
        let dir = TempDir::new().unwrap();
        let mut tracker = tracker_in(&dir, "persist");

        tracker.register(10);
        tracker.register(20);
        tracker.unregister(10);

        let content = fs::read_to_string(&tracker.pid_file).unwrap();
        let saved_pids: HashSet<u32> = content
            .lines()
            .filter_map(|l| l.trim().parse::<u32>().ok())
            .collect();
        assert!(!saved_pids.contains(&10));
        assert!(saved_pids.contains(&20));
    }

    #[test]
    fn is_process_alive_returns_true_for_current_pid() {
        let pid = std::process::id();
        assert!(is_process_alive(pid));
    }

    #[test]
    fn is_process_alive_returns_false_for_nonexistent_pid() {
        // PID 4_000_000_000 is far above the Linux pid_max (usually ≤ 4194304)
        assert!(!is_process_alive(4_000_000_000));
    }

    #[test]
    fn to_nix_pid_rejects_zero() {
        assert!(to_nix_pid(0).is_none());
    }

    #[test]
    fn to_nix_pid_accepts_valid_pid() {
        let result = to_nix_pid(123);
        assert!(result.is_some());
    }
}
