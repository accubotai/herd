//! Process supervisor — manages lifecycle, orphan tracking, file watchers, and restart logic.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use herd_config::ProcessConfig;
use tokio::sync::mpsc;

use crate::orphan::OrphanTracker;
use crate::process::{ProcessEvent, ProcessHandle, ProcessInfo, ProcessState};
use crate::watcher::{FileChange, FileWatcher};

/// Manages multiple processes, handles lifecycle and restart logic.
pub struct Supervisor {
    processes: HashMap<String, ProcessHandle>,
    event_tx: mpsc::UnboundedSender<ProcessEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<ProcessEvent>>,
    orphan_tracker: OrphanTracker,
    file_change_tx: mpsc::UnboundedSender<FileChange>,
    file_change_rx: Option<mpsc::UnboundedReceiver<FileChange>>,
    /// Active file watchers (kept alive by holding the handle).
    #[allow(dead_code)]
    watchers: Vec<FileWatcher>,
    /// Processes scheduled for delayed restart: `(name, restart_at)`.
    pending_restarts: Vec<(String, Instant)>,
}

impl Supervisor {
    pub fn new(project_id: &str) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (file_tx, file_rx) = mpsc::unbounded_channel();
        let mut orphan_tracker = OrphanTracker::new(project_id);

        // Clean up any orphans from a previous crash
        let killed = orphan_tracker.cleanup_orphans();
        if !killed.is_empty() {
            tracing::info!(count = killed.len(), "Cleaned up orphaned processes");
        }

        Self {
            processes: HashMap::new(),
            event_tx,
            event_rx: Some(event_rx),
            orphan_tracker,
            file_change_tx: file_tx,
            file_change_rx: Some(file_rx),
            watchers: Vec::new(),
            pending_restarts: Vec::new(),
        }
    }

    /// Take the event receiver (can only be called once).
    pub fn take_event_rx(&mut self) -> Option<mpsc::UnboundedReceiver<ProcessEvent>> {
        self.event_rx.take()
    }

    /// Take the file change receiver (can only be called once).
    pub fn take_file_change_rx(&mut self) -> Option<mpsc::UnboundedReceiver<FileChange>> {
        self.file_change_rx.take()
    }

    /// Add a process from config without starting it.
    pub fn add_process(&mut self, config: &ProcessConfig) {
        let info = ProcessInfo {
            name: config.name.clone(),
            command: config.command.clone(),
            working_dir: config.working_dir.as_ref().map(Into::into),
            section: config.section.clone(),
            auto_restart: config.auto_restart,
            lazy: config.lazy,
            interactive: config.interactive,
            restart_delay_ms: config.restart_delay_ms,
            env: config.env.clone(),
        };

        let handle = ProcessHandle::new(info, self.event_tx.clone());
        self.processes.insert(config.name.clone(), handle);

        // Set up file watcher if configured
        if let Some(watch) = &config.watch {
            if !watch.paths.is_empty() {
                let watch_paths: Vec<PathBuf> = watch.paths.iter().map(PathBuf::from).collect();
                let ignore_paths: Vec<PathBuf> = watch.ignore.iter().map(PathBuf::from).collect();
                match FileWatcher::new(
                    config.name.clone(),
                    watch_paths,
                    ignore_paths,
                    self.file_change_tx.clone(),
                ) {
                    Ok(watcher) => {
                        self.watchers.push(watcher);
                        tracing::info!(name = %config.name, "File watcher started");
                    }
                    Err(e) => {
                        tracing::warn!(
                            name = %config.name, error = %e,
                            "Failed to create file watcher"
                        );
                    }
                }
            }
        }
    }

    /// Start a specific process by name.
    pub fn start_process(&mut self, name: &str) -> anyhow::Result<()> {
        let handle = self
            .processes
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Process not found: {name}"))?;

        handle.spawn()?;

        // Register PID with orphan tracker
        if let Some(pid) = handle.pid {
            self.orphan_tracker.register(pid);
        }

        Ok(())
    }

    /// Stop a specific process by name.
    pub fn stop_process(&mut self, name: &str) -> anyhow::Result<()> {
        let handle = self
            .processes
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Process not found: {name}"))?;

        // Unregister PID before stopping
        if let Some(pid) = handle.pid {
            self.orphan_tracker.unregister(pid);
        }

        handle.stop();
        Ok(())
    }

    /// Restart a process.
    pub fn restart_process(&mut self, name: &str) -> anyhow::Result<()> {
        self.stop_process(name)?;
        self.start_process(name)
    }

    /// Start all non-lazy processes.
    pub fn start_all(&mut self) -> Vec<anyhow::Error> {
        let names: Vec<String> = self
            .processes
            .iter()
            .filter(|(_, h)| !h.info.lazy && h.state == ProcessState::Pending)
            .map(|(name, _)| name.clone())
            .collect();

        let mut errors = Vec::new();
        for name in names {
            if let Err(e) = self.start_process(&name) {
                tracing::error!(name = %name, error = %e, "Failed to start process");
                errors.push(e);
            }
        }
        errors
    }

    /// Stop all running processes.
    pub fn stop_all(&mut self) {
        let names: Vec<String> = self
            .processes
            .iter()
            .filter(|(_, h)| h.is_running())
            .map(|(name, _)| name.clone())
            .collect();

        for name in names {
            if let Err(e) = self.stop_process(&name) {
                tracing::warn!(name = %name, error = %e, "Failed to stop process");
            }
        }
    }

    /// Get a reference to a process handle.
    pub fn get_process(&self, name: &str) -> Option<&ProcessHandle> {
        self.processes.get(name)
    }

    /// Get a mutable reference to a process handle.
    pub fn get_process_mut(&mut self, name: &str) -> Option<&mut ProcessHandle> {
        self.processes.get_mut(name)
    }

    /// List all processes with their current state.
    pub fn list_processes(&self) -> Vec<(&str, ProcessState, Option<u32>)> {
        self.processes
            .iter()
            .map(|(name, handle)| (name.as_str(), handle.state, handle.pid))
            .collect()
    }

    /// Get process names grouped by section.
    pub fn process_names_by_section(&self) -> HashMap<String, Vec<String>> {
        let mut sections: HashMap<String, Vec<String>> = HashMap::new();
        for (name, handle) in &self.processes {
            sections
                .entry(handle.info.section.clone())
                .or_default()
                .push(name.clone());
        }
        sections
    }

    /// Handle a process exit event — decide whether to restart.
    /// Returns `true` if the process crashed (for notification purposes).
    pub fn handle_exit(&mut self, name: &str, exit_code: Option<i32>) -> bool {
        let Some(handle) = self.processes.get_mut(name) else {
            return false;
        };

        // Unregister PID
        if let Some(pid) = handle.pid {
            self.orphan_tracker.unregister(pid);
        }

        let crashed = exit_code.is_none() || exit_code != Some(0);

        if crashed {
            handle.state = ProcessState::Crashed;
            tracing::warn!(name = %name, code = ?exit_code, "Process crashed");

            if handle.info.auto_restart {
                let delay_ms = handle.info.restart_delay_ms.unwrap_or(1000);
                let restart_at = Instant::now() + std::time::Duration::from_millis(delay_ms);
                handle.state = ProcessState::Restarting;
                self.pending_restarts.push((name.to_string(), restart_at));
                tracing::info!(name = %name, delay_ms, "Scheduled restart");
            }
        } else {
            handle.state = ProcessState::Exited;
            tracing::info!(name = %name, "Process exited cleanly");
        }

        crashed
    }

    /// Handle a file change event — restart the associated process.
    pub fn handle_file_change(&mut self, process_name: &str) {
        tracing::info!(name = %process_name, "File change detected, restarting");
        if let Err(e) = self.restart_process(process_name) {
            tracing::warn!(name = %process_name, error = %e, "Failed to restart after file change");
        }
    }

    /// Process any pending delayed restarts. Call this on each tick.
    pub fn process_pending_restarts(&mut self) {
        let now = Instant::now();
        let ready: Vec<String> = self
            .pending_restarts
            .iter()
            .filter(|(_, when)| now >= *when)
            .map(|(name, _)| name.clone())
            .collect();

        self.pending_restarts.retain(|(_, when)| now < *when);

        for name in ready {
            tracing::info!(name = %name, "Executing delayed restart");
            if let Err(e) = self.start_process(&name) {
                tracing::error!(name = %name, error = %e, "Delayed restart failed");
            }
        }
    }
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new("default")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use herd_config::ProcessConfig;
    use std::collections::HashMap;

    fn test_config(name: &str, lazy: bool) -> ProcessConfig {
        ProcessConfig {
            name: name.to_string(),
            command: "echo hello".to_string(),
            working_dir: None,
            auto_restart: false,
            section: "services".to_string(),
            lazy,
            interactive: false,
            restart_delay_ms: None,
            watch: None,
            env: HashMap::new(),
        }
    }

    #[test]
    fn test_add_process() {
        let mut sup = Supervisor::new("test");
        sup.add_process(&test_config("test", false));
        assert!(sup.get_process("test").is_some());
        assert!(sup.get_process("nonexistent").is_none());
    }

    #[test]
    fn test_list_processes() {
        let mut sup = Supervisor::new("test");
        sup.add_process(&test_config("a", false));
        sup.add_process(&test_config("b", true));
        let list = sup.list_processes();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_process_names_by_section() {
        let mut sup = Supervisor::new("test");
        sup.add_process(&test_config("a", false));
        sup.add_process(&test_config("b", false));
        let sections = sup.process_names_by_section();
        assert_eq!(sections.get("services").unwrap().len(), 2);
    }

    #[test]
    fn test_handle_exit_crash_with_auto_restart() {
        let mut sup = Supervisor::new("test");
        let mut config = test_config("svc", false);
        config.auto_restart = true;
        config.restart_delay_ms = Some(100);
        sup.add_process(&config);

        // Manually set state to Running
        sup.get_process_mut("svc").unwrap().state = ProcessState::Running;

        let crashed = sup.handle_exit("svc", Some(1));
        assert!(crashed);
        assert_eq!(
            sup.get_process("svc").unwrap().state,
            ProcessState::Restarting
        );
        assert_eq!(sup.pending_restarts.len(), 1);
    }

    #[test]
    fn test_handle_exit_clean() {
        let mut sup = Supervisor::new("test");
        sup.add_process(&test_config("svc", false));
        sup.get_process_mut("svc").unwrap().state = ProcessState::Running;

        let crashed = sup.handle_exit("svc", Some(0));
        assert!(!crashed);
        assert_eq!(sup.get_process("svc").unwrap().state, ProcessState::Exited);
        assert!(sup.pending_restarts.is_empty());
    }

    #[test]
    fn test_handle_exit_nonexistent_process() {
        let mut sup = Supervisor::new("test");
        let crashed = sup.handle_exit("nonexistent", Some(1));
        assert!(!crashed);
    }
}
