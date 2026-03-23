use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use alacritty_terminal::event::{EventListener, WindowSize};
use alacritty_terminal::event_loop::EventLoop;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty;
use std::sync::Arc;

/// Default terminal dimensions
pub const DEFAULT_COLS: u16 = 80;
pub const DEFAULT_ROWS: u16 = 24;
pub const DEFAULT_CELL_WIDTH: u16 = 8;
pub const DEFAULT_CELL_HEIGHT: u16 = 16;

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
    _event_loop_handle: Option<
        std::thread::JoinHandle<(
            EventLoop<tty::Pty, EventProxy>,
            alacritty_terminal::event_loop::State,
        )>,
    >,
}

/// Proxy for alacritty_terminal events — forwards to our event system
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

pub fn default_window_size() -> WindowSize {
    WindowSize {
        num_lines: DEFAULT_ROWS,
        num_cols: DEFAULT_COLS,
        cell_width: DEFAULT_CELL_WIDTH,
        cell_height: DEFAULT_CELL_HEIGHT,
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
            cols: DEFAULT_COLS as usize,
            rows: DEFAULT_ROWS as usize,
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
            _event_loop_handle: None,
        }
    }

    /// Spawn the process with its own PTY
    pub fn spawn(&mut self) -> anyhow::Result<()> {
        let window_size = default_window_size();

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

    /// Send SIGTERM to stop the process
    pub fn stop(&mut self) {
        if let Some(pid) = self.pid {
            let pid = nix::unistd::Pid::from_raw(pid as i32);
            let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM);
            self.state = ProcessState::Stopped;
            tracing::info!(name = %self.info.name, "Process stopped");
        }
    }

    /// Write bytes to the PTY (keyboard input)
    pub fn write_to_pty(&self, _data: &[u8]) {
        // Will be implemented when we wire up PTY fd writing
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
