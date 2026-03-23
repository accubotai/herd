use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use std::borrow::Cow;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty;
use std::sync::Arc;

use crate::pty;

/// Current state of a managed process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Not yet started
    Pending,
    /// Currently running
    Running,
    /// Stopped by user
    Stopped,
    /// Crashed (exited with non-zero or signal)
    Crashed,
    /// Exited cleanly
    Exited,
    /// Waiting before auto-restart
    Restarting,
}

/// Static info about a configured process
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub name: String,
    pub command: String,
    pub working_dir: Option<PathBuf>,
    pub section: String,
    pub auto_restart: bool,
    pub lazy: bool,
    pub interactive: bool,
    pub restart_delay_ms: Option<u64>,
    pub env: HashMap<String, String>,
}

/// A running process handle with its terminal state
pub struct ProcessHandle {
    pub info: ProcessInfo,
    pub state: ProcessState,
    pub pid: Option<u32>,
    pub terminal: Arc<FairMutex<Term<EventProxy>>>,
    pub started_at: Option<Instant>,
    event_sender: tokio::sync::mpsc::UnboundedSender<ProcessEvent>,
    pty_sender: Option<EventLoopSender>,
    #[allow(clippy::used_underscore_binding)]
    _event_loop_handle: Option<
        std::thread::JoinHandle<(
            EventLoop<tty::Pty, EventProxy>,
            alacritty_terminal::event_loop::State,
        )>,
    >,
}

/// Proxy for `alacritty_terminal` events — forwards to our event system
#[derive(Clone)]
pub struct EventProxy {
    pub process_name: String,
    pub sender: tokio::sync::mpsc::UnboundedSender<ProcessEvent>,
}

/// Events emitted by managed processes
#[derive(Debug, Clone)]
pub enum ProcessEvent {
    /// Terminal content changed (needs re-render)
    Render(String),
    /// Process exited with a code
    Exited { name: String, code: Option<i32> },
    /// Bell character received
    Bell(String),
    /// Title changed
    TitleChanged { name: String, title: String },
}

impl EventListener for EventProxy {
    fn send_event(&self, event: alacritty_terminal::event::Event) {
        use alacritty_terminal::event::Event;
        match event {
            Event::Wakeup => {
                let _ = self
                    .sender
                    .send(ProcessEvent::Render(self.process_name.clone()));
            }
            Event::Bell => {
                let _ = self
                    .sender
                    .send(ProcessEvent::Bell(self.process_name.clone()));
            }
            Event::Title(title) => {
                let _ = self.sender.send(ProcessEvent::TitleChanged {
                    name: self.process_name.clone(),
                    title,
                });
            }
            _ => {}
        }
    }
}

/// Simple struct implementing Dimensions for Term creation
struct TermSize {
    cols: usize,
    rows: usize,
    history: usize,
}

impl alacritty_terminal::grid::Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows + self.history
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

impl ProcessHandle {
    pub fn new(
        info: ProcessInfo,
        sender: tokio::sync::mpsc::UnboundedSender<ProcessEvent>,
    ) -> Self {
        let event_proxy = EventProxy {
            process_name: info.name.clone(),
            sender: sender.clone(),
        };

        // Create terminal with default size
        let term_config = alacritty_terminal::term::Config::default();
        let dimensions = TermSize {
            cols: pty::DEFAULT_COLS as usize,
            rows: pty::DEFAULT_ROWS as usize,
            history: 10000,
        };
        let terminal = Term::new(term_config, &dimensions, event_proxy);
        let terminal = Arc::new(FairMutex::new(terminal));

        Self {
            info,
            state: ProcessState::Pending,
            pid: None,
            terminal,
            started_at: None,
            event_sender: sender,
            pty_sender: None,
            _event_loop_handle: None,
        }
    }

    /// Spawn the process with its own PTY
    pub fn spawn(&mut self) -> anyhow::Result<()> {
        let window_size = pty::default_window_size();

        // Build PTY config
        let pty_config = tty::Options {
            shell: Some(tty::Shell::new(
                "/bin/sh".to_string(),
                vec!["-c".to_string(), self.info.command.clone()],
            )),
            working_directory: self.info.working_dir.clone(),
            drain_on_exit: false,
            env: self.info.env.clone(),
        };

        // Create PTY
        let pty = tty::new(&pty_config, window_size, 0)?;
        self.pid = Some(pty.child().id());

        // Create event proxy for the event loop
        let event_proxy = EventProxy {
            process_name: self.info.name.clone(),
            sender: self.event_sender.clone(),
        };

        // EventLoop::new(terminal, event_proxy, pty, drain_on_exit, ref_test)
        let event_loop = EventLoop::new(
            Arc::clone(&self.terminal),
            event_proxy,
            pty,
            false, // drain_on_exit
            false, // ref_test
        )?;

        // Capture PTY write channel before spawning (moves event_loop)
        self.pty_sender = Some(event_loop.channel());
        let loop_handle = event_loop.spawn();
        self._event_loop_handle = Some(loop_handle);
        self.state = ProcessState::Running;
        self.started_at = Some(Instant::now());

        tracing::info!(
            name = %self.info.name,
            pid = ?self.pid,
            "Process spawned"
        );

        Ok(())
    }

    /// Send SIGTERM to stop the process.
    pub fn stop(&mut self) {
        if let Some(pid) = self.pid {
            match i32::try_from(pid) {
                Ok(raw_pid) if raw_pid > 0 => {
                    let nix_pid = nix::unistd::Pid::from_raw(raw_pid);
                    if let Err(e) =
                        nix::sys::signal::kill(nix_pid, nix::sys::signal::Signal::SIGTERM)
                    {
                        tracing::warn!(
                            name = %self.info.name, pid, error = %e,
                            "Failed to send SIGTERM"
                        );
                    }
                }
                _ => {
                    tracing::error!(
                        name = %self.info.name, pid,
                        "Invalid PID — cannot send signal"
                    );
                }
            }
            self.state = ProcessState::Stopped;
            tracing::info!(name = %self.info.name, "Process stopped");
        }
    }

    /// Write bytes to the PTY (keyboard input).
    pub fn write_to_pty(&self, data: &[u8]) {
        if let Some(sender) = &self.pty_sender {
            let _ = sender.send(Msg::Input(Cow::Owned(data.to_vec())));
        }
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let dimensions = TermSize {
            cols: cols as usize,
            rows: rows as usize,
            history: 10000,
        };
        self.terminal.lock().resize(dimensions);
    }

    /// Check if the process is alive
    pub fn is_running(&self) -> bool {
        self.state == ProcessState::Running
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample_info() -> ProcessInfo {
        ProcessInfo {
            name: "test-proc".to_string(),
            command: "echo hello".to_string(),
            working_dir: None,
            section: "default".to_string(),
            auto_restart: false,
            lazy: false,
            interactive: false,
            restart_delay_ms: None,
            env: HashMap::new(),
        }
    }

    // --- ProcessState tests ---

    #[test]
    fn process_state_enum_values_are_distinct() {
        let states = [
            ProcessState::Pending,
            ProcessState::Running,
            ProcessState::Stopped,
            ProcessState::Crashed,
            ProcessState::Exited,
            ProcessState::Restarting,
        ];
        // Every pair should be unequal
        for (i, a) in states.iter().enumerate() {
            for (j, b) in states.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b, "states at index {i} and {j} should differ");
                }
            }
        }
    }

    #[test]
    fn process_state_is_copy_and_clone() {
        let s = ProcessState::Running;
        let s2 = s; // Copy
        let s3 = s; // Copy (same as Clone for Copy types)
        assert_eq!(s, s2);
        assert_eq!(s, s3);
    }

    #[test]
    fn process_state_debug_format() {
        // Ensure Debug is derived and produces reasonable output
        let dbg = format!("{:?}", ProcessState::Pending);
        assert_eq!(dbg, "Pending");
    }

    // --- ProcessHandle tests ---

    #[test]
    fn new_creates_handle_in_pending_state() {
        let (sender, _rx) = tokio::sync::mpsc::unbounded_channel();
        let handle = ProcessHandle::new(sample_info(), sender);
        assert_eq!(handle.state, ProcessState::Pending);
        assert!(handle.pid.is_none());
        assert!(handle.started_at.is_none());
    }

    #[test]
    fn new_creates_terminal_with_correct_dimensions() {
        use alacritty_terminal::grid::Dimensions;
        let (sender, _rx) = tokio::sync::mpsc::unbounded_channel();
        let handle = ProcessHandle::new(sample_info(), sender);
        let term = handle.terminal.lock();
        assert_eq!(term.columns(), pty::DEFAULT_COLS as usize);
        assert_eq!(term.screen_lines(), pty::DEFAULT_ROWS as usize);
    }

    #[test]
    fn is_running_returns_false_when_pending() {
        let (sender, _rx) = tokio::sync::mpsc::unbounded_channel();
        let handle = ProcessHandle::new(sample_info(), sender);
        assert!(!handle.is_running());
    }

    #[test]
    fn is_running_reflects_state() {
        let (sender, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut handle = ProcessHandle::new(sample_info(), sender);

        // Manually set state to Running (without spawning a real process)
        handle.state = ProcessState::Running;
        assert!(handle.is_running());

        handle.state = ProcessState::Stopped;
        assert!(!handle.is_running());

        handle.state = ProcessState::Crashed;
        assert!(!handle.is_running());

        handle.state = ProcessState::Exited;
        assert!(!handle.is_running());

        handle.state = ProcessState::Restarting;
        assert!(!handle.is_running());
    }

    #[test]
    fn process_info_stores_fields_correctly() {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());

        let info = ProcessInfo {
            name: "web".to_string(),
            command: "npm start".to_string(),
            working_dir: Some(PathBuf::from("/tmp")),
            section: "services".to_string(),
            auto_restart: true,
            lazy: true,
            interactive: false,
            restart_delay_ms: Some(500),
            env: env.clone(),
        };

        assert_eq!(info.name, "web");
        assert_eq!(info.command, "npm start");
        assert_eq!(info.working_dir, Some(PathBuf::from("/tmp")));
        assert_eq!(info.section, "services");
        assert!(info.auto_restart);
        assert!(info.lazy);
        assert!(!info.interactive);
        assert_eq!(info.restart_delay_ms, Some(500));
        assert_eq!(info.env.get("FOO").unwrap(), "bar");
    }

    #[test]
    fn event_proxy_clone() {
        let (sender, _rx) = tokio::sync::mpsc::unbounded_channel();
        let proxy = EventProxy {
            process_name: "test".to_string(),
            sender,
        };
        let proxy2 = proxy.clone();
        assert_eq!(proxy.process_name, proxy2.process_name);
    }
}
