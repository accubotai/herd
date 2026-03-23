use std::collections::HashMap;
use tokio::sync::mpsc;

use herd_config::ProcessConfig;

use crate::process::{ProcessEvent, ProcessHandle, ProcessInfo, ProcessState};

/// Manages multiple processes, handles lifecycle and restart logic
pub struct Supervisor {
    processes: HashMap<String, ProcessHandle>,
    event_tx: mpsc::UnboundedSender<ProcessEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<ProcessEvent>>,
}

impl Supervisor {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            processes: HashMap::new(),
            event_tx: tx,
            event_rx: Some(rx),
        }
    }

    /// Take the event receiver (can only be called once)
    pub fn take_event_rx(&mut self) -> Option<mpsc::UnboundedReceiver<ProcessEvent>> {
        self.event_rx.take()
    }

    /// Get the event sender for cloning
    pub fn event_tx(&self) -> mpsc::UnboundedSender<ProcessEvent> {
        self.event_tx.clone()
    }

    /// Add a process from config without starting it
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
    }

    /// Start a specific process by name
    pub fn start_process(&mut self, name: &str) -> anyhow::Result<()> {
        let handle = self
            .processes
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Process not found: {name}"))?;

        handle.spawn()?;
        Ok(())
    }

    /// Stop a specific process by name
    pub fn stop_process(&mut self, name: &str) -> anyhow::Result<()> {
        let handle = self
            .processes
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Process not found: {name}"))?;

        handle.stop();
        Ok(())
    }

    /// Restart a process
    pub fn restart_process(&mut self, name: &str) -> anyhow::Result<()> {
        self.stop_process(name)?;
        self.start_process(name)
    }

    /// Start all non-lazy processes
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

    /// Stop all running processes
    pub fn stop_all(&mut self) {
        let names: Vec<String> = self
            .processes
            .iter()
            .filter(|(_, h)| h.is_running())
            .map(|(name, _)| name.clone())
            .collect();

        for name in names {
            if let Err(e) = self.stop_process(&name) {
                tracing::warn!(name = %name, error = %e, "Failed to stop process during stop_all");
            }
        }
    }

    /// Get a reference to a process handle
    pub fn get_process(&self, name: &str) -> Option<&ProcessHandle> {
        self.processes.get(name)
    }

    /// Get a mutable reference to a process handle
    pub fn get_process_mut(&mut self, name: &str) -> Option<&mut ProcessHandle> {
        self.processes.get_mut(name)
    }

    /// List all processes with their current state
    pub fn list_processes(&self) -> Vec<(&str, ProcessState, Option<u32>)> {
        self.processes
            .iter()
            .map(|(name, handle)| (name.as_str(), handle.state, handle.pid))
            .collect()
    }

    /// Get process names ordered by section
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

    /// Handle a process exit event — decide whether to restart
    pub fn handle_exit(&mut self, name: &str, exit_code: Option<i32>) {
        if let Some(handle) = self.processes.get_mut(name) {
            let crashed = exit_code.is_none() || exit_code != Some(0);

            if crashed {
                handle.state = ProcessState::Crashed;
                tracing::warn!(name = %name, code = ?exit_code, "Process crashed");

                if handle.info.auto_restart {
                    handle.state = ProcessState::Restarting;
                    tracing::info!(name = %name, "Scheduling restart");
                    // Actual restart will be triggered by the event loop
                    // after restart_delay_ms
                }
            } else {
                handle.state = ProcessState::Exited;
                tracing::info!(name = %name, "Process exited cleanly");
            }
        }
    }
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
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
        let mut sup = Supervisor::new();
        sup.add_process(&test_config("test", false));
        assert!(sup.get_process("test").is_some());
        assert!(sup.get_process("nonexistent").is_none());
    }

    #[test]
    fn test_list_processes() {
        let mut sup = Supervisor::new();
        sup.add_process(&test_config("a", false));
        sup.add_process(&test_config("b", true));
        let list = sup.list_processes();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_process_names_by_section() {
        let mut sup = Supervisor::new();
        sup.add_process(&test_config("a", false));
        sup.add_process(&test_config("b", false));
        let sections = sup.process_names_by_section();
        assert_eq!(sections.get("services").unwrap().len(), 2);
    }
}
