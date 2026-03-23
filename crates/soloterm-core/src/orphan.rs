use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrphanError {
    #[error("failed to read PID file: {0}")]
    Io(#[from] std::io::Error),
}

/// Tracks PIDs of managed processes to detect and clean up orphans on restart
pub struct OrphanTracker {
    pid_file: PathBuf,
    pids: HashSet<u32>,
}

impl OrphanTracker {
    /// Default path: ~/.config/soloterm/pids/<project-hash>
    pub fn new(project_id: &str) -> Self {
        let pid_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("soloterm")
            .join("pids");

        Self {
            pid_file: pid_dir.join(format!("{}.pids", project_id)),
            pids: HashSet::new(),
        }
    }

    /// Register a new PID
    pub fn register(&mut self, pid: u32) {
        self.pids.insert(pid);
        let _ = self.save();
    }

    /// Unregister a PID (process exited normally)
    pub fn unregister(&mut self, pid: u32) {
        self.pids.remove(&pid);
        let _ = self.save();
    }

    /// Check for and kill orphaned processes from a previous session
    pub fn cleanup_orphans(&mut self) -> Vec<u32> {
        let mut killed = Vec::new();

        if let Ok(content) = fs::read_to_string(&self.pid_file) {
            for line in content.lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    if is_process_alive(pid) {
                        tracing::warn!(pid, "Killing orphaned process");
                        let nix_pid = nix::unistd::Pid::from_raw(pid as i32);
                        let _ = nix::sys::signal::kill(nix_pid, nix::sys::signal::Signal::SIGTERM);
                        killed.push(pid);
                    }
                }
            }
        }

        // Clear the PID file after cleanup
        self.pids.clear();
        let _ = self.save();

        killed
    }

    fn save(&self) -> Result<(), OrphanError> {
        if let Some(parent) = self.pid_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let content: String = self
            .pids
            .iter()
            .map(|pid| pid.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&self.pid_file, content)?;
        Ok(())
    }
}

fn is_process_alive(pid: u32) -> bool {
    // Check if /proc/PID exists (Linux-specific)
    PathBuf::from(format!("/proc/{}", pid)).exists()
}

impl Drop for OrphanTracker {
    fn drop(&mut self) {
        // Clean up PID file on normal exit
        let _ = fs::remove_file(&self.pid_file);
    }
}
