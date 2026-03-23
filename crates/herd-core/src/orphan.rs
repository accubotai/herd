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
