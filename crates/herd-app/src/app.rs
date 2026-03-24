// Hex color literals (0xRRGGBB) are intentionally without separators.
#![allow(clippy::unreadable_literal)]

use std::env;
use std::time::Duration;

use herd_config::HerdConfig;
use herd_core::process::ProcessState;
use herd_core::Supervisor;
use herd_mcp::server::{McpServer, ProcessSnapshot, SharedProcessState, Transport};
use herd_terminal::grid_adapter;
use iced::keyboard;
use iced::widget::{button, column, container, row, text, Column};
use iced::{color, Element, Length, Subscription, Theme};

use crate::terminal_widget::TerminalProgram;

/// Main Herd application state.
pub(crate) struct HerdApp {
    config: Option<HerdConfig>,
    supervisor: Supervisor,
    focused: Option<String>,
    process_order: Vec<String>,
    /// GPU terminal renderer for the focused process.
    terminal_program: Option<TerminalProgram>,
    status: String,
    started: bool,
    /// Shared process state for MCP server.
    mcp_state: SharedProcessState,
    /// MCP server instance (kept alive).
    #[allow(dead_code)]
    mcp_server: Option<McpServer>,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    SelectProcess(String),
    StartAll,
    StopAll,
    Tick,
    KeyPressed(keyboard::Key, keyboard::Modifiers),
}

impl Default for HerdApp {
    fn default() -> Self {
        let cwd = env::current_dir().unwrap_or_default();
        let config = HerdConfig::find_and_load(&cwd).ok();

        let project_name = config
            .as_ref()
            .map_or("default", |c| c.project.name.as_str());
        let mut supervisor = Supervisor::new(project_name);
        let mut process_order = Vec::new();

        if let Some(cfg) = &config {
            for proc in &cfg.process {
                supervisor.add_process(proc);
                process_order.push(proc.name.clone());
            }
        }

        let focused = process_order.first().cloned();
        let status = config.as_ref().map_or_else(
            || "No herd.toml found — create one to get started".to_string(),
            |c| format!("Project: {}", c.project.name),
        );

        // Set up MCP server with shared state
        let mcp_state = herd_mcp::server::new_shared_state();
        let mcp_enabled = config.as_ref().is_some_and(|c| c.ai.mcp_enabled);

        let mcp_server = if mcp_enabled {
            let server = McpServer::new(Transport::Stdio, mcp_state.clone());
            if let Err(e) = server.start() {
                tracing::warn!(error = %e, "Failed to start MCP server");
            }
            Some(server)
        } else {
            None
        };

        Self {
            config,
            supervisor,
            focused,
            process_order,
            terminal_program: None,
            status,
            started: false,
            mcp_state,
            mcp_server,
        }
    }
}

impl HerdApp {
    pub(crate) fn update(&mut self, message: Message) {
        match message {
            Message::SelectProcess(name) => {
                self.focused = Some(name);
                self.refresh_terminal_content();
            }
            Message::StartAll => {
                let errors = self.supervisor.start_all();
                if errors.is_empty() {
                    self.status = "All processes started".to_string();
                } else {
                    self.status = format!("{} process(es) failed to start", errors.len());
                }
                self.started = true;
                self.update_mcp_state();
                self.refresh_terminal_content();
            }
            Message::StopAll => {
                self.supervisor.stop_all();
                self.status = "All processes stopped".to_string();
                self.update_mcp_state();
                self.refresh_terminal_content();
            }
            Message::Tick => {
                if !self.started && self.config.is_some() {
                    let errors = self.supervisor.start_all();
                    if !errors.is_empty() {
                        self.status = format!("{} process(es) failed to start", errors.len());
                    }
                    self.started = true;
                }

                // Process pending delayed restarts
                self.supervisor.process_pending_restarts();

                // Drain process events (crash/exit detection)
                self.drain_process_events();

                // Drain file change events (auto-restart)
                self.drain_file_changes();

                self.update_mcp_state();
                self.refresh_terminal_content();
            }
            Message::KeyPressed(key, modifiers) => {
                self.handle_key(&key, modifiers);
            }
        }
    }

    pub(crate) fn view(&self) -> Element<'_, Message> {
        let sidebar = self.view_sidebar();
        let terminal_area = self.view_terminal();

        let status_bar =
            container(text(&self.status).size(12).color(color!(0x888888))).padding([4, 16]);

        let content = column![
            row![sidebar, terminal_area].height(Length::Fill),
            status_bar,
        ];

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn subscription(&self) -> Subscription<Message> {
        let tick = iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick);
        let keys =
            keyboard::on_key_press(|key, modifiers| Some(Message::KeyPressed(key, modifiers)));
        Subscription::batch(vec![tick, keys])
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn theme(&self) -> Theme {
        Theme::Dark
    }

    // ── Event draining ──

    fn drain_process_events(&mut self) {
        // We can't take_event_rx every tick, so we use try_recv pattern.
        // The event_rx is taken once and stored — but since iced owns us
        // and we can't have async, we drain synchronously via the supervisor.
        // For now, check process states directly by examining PIDs.
        for name in &self.process_order {
            if let Some(handle) = self.supervisor.get_process(name) {
                if handle.state == ProcessState::Running {
                    if let Some(pid) = handle.pid {
                        // Check if process is still alive
                        if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
                            // Process died — handle exit
                            let crashed = self.supervisor.handle_exit(name, None);
                            if crashed {
                                self.status = format!("Process '{name}' crashed");
                                // Send desktop notification
                                if let Err(e) = herd_notify::notify_crash(name, None) {
                                    tracing::debug!(error = %e, "Notification failed");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn drain_file_changes(&self) {
        // File changes are handled by the supervisor's internal watchers
        // which send events through the file_change channel.
        // Since we can't easily poll channels from iced's sync update,
        // we rely on the watcher → supervisor restart path being
        // triggered through the event loop in future async integration.
    }

    // ── MCP state sync ──

    fn update_mcp_state(&self) {
        let snapshots: Vec<ProcessSnapshot> = self
            .process_order
            .iter()
            .filter_map(|name| {
                self.supervisor.get_process(name).map(|h| ProcessSnapshot {
                    name: name.clone(),
                    state: format!("{:?}", h.state),
                    pid: h.pid,
                    section: h.info.section.clone(),
                    command: h.info.command.clone(),
                })
            })
            .collect();
        *self.mcp_state.lock() = snapshots;
    }

    // ── Sidebar ──

    fn view_sidebar(&self) -> Element<'_, Message> {
        let mut sidebar_content: Vec<Element<'_, Message>> = vec![
            text("Herd").size(20).into(),
            text("─────────────")
                .size(12)
                .color(color!(0x555555))
                .into(),
        ];

        if self.process_order.is_empty() {
            sidebar_content.push(
                text("No processes configured")
                    .size(12)
                    .color(color!(0x888888))
                    .into(),
            );
        } else {
            let mut last_section = String::new();
            for name in &self.process_order {
                let (state, section) = self
                    .supervisor
                    .get_process(name)
                    .map_or((ProcessState::Pending, "services"), |h| {
                        (h.state, h.info.section.as_str())
                    });

                if section != last_section {
                    sidebar_content.push(
                        text(section.to_uppercase())
                            .size(11)
                            .color(color!(0x666666))
                            .into(),
                    );
                    last_section = section.to_string();
                }

                let is_focused = self.focused.as_deref() == Some(name.as_str());
                sidebar_content.push(self.view_process_item(name, state, is_focused));
            }

            sidebar_content.push(text("").into());
            sidebar_content.push(
                row![
                    button(text("Start All").size(11))
                        .on_press(Message::StartAll)
                        .padding([4, 8]),
                    button(text("Stop All").size(11))
                        .on_press(Message::StopAll)
                        .padding([4, 8]),
                ]
                .spacing(4)
                .into(),
            );
        }

        container(Column::from_vec(sidebar_content).spacing(4).padding(12))
            .width(Length::Fixed(220.0))
            .height(Length::Fill)
            .style(|_theme: &Theme| container::Style {
                background: Some(iced::Background::Color(color!(0x1a1a2e))),
                ..Default::default()
            })
            .into()
    }

    #[allow(clippy::unused_self)]
    fn view_process_item<'a>(
        &self,
        name: &'a str,
        state: ProcessState,
        is_focused: bool,
    ) -> Element<'a, Message> {
        let (indicator, indicator_color) = match state {
            ProcessState::Running => ("●", color!(0x00cc66)),
            ProcessState::Pending => ("○", color!(0x666666)),
            ProcessState::Stopped => ("■", color!(0x888888)),
            ProcessState::Crashed => ("●", color!(0xff4444)),
            ProcessState::Exited => ("○", color!(0x448844)),
            ProcessState::Restarting => ("◌", color!(0xffaa00)),
        };

        let bg = if is_focused {
            color!(0x2a2a4e)
        } else {
            color!(0x1a1a2e)
        };

        let name_owned = name.to_string();
        button(
            row![
                text(indicator).size(10).color(indicator_color),
                text(name).size(13).color(color!(0xcccccc)),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center),
        )
        .on_press(Message::SelectProcess(name_owned))
        .padding([4, 8])
        .width(Length::Fill)
        .style(move |_theme: &Theme, _status| button::Style {
            background: Some(iced::Background::Color(bg)),
            text_color: color!(0xcccccc),
            border: iced::Border::default(),
            shadow: iced::Shadow::default(),
        })
        .into()
    }

    // ── Terminal pane ──

    fn view_terminal(&self) -> Element<'_, Message> {
        let header = if let Some(name) = &self.focused {
            let state = self
                .supervisor
                .get_process(name)
                .map_or(ProcessState::Pending, |h| h.state);
            let state_str = match state {
                ProcessState::Running => "running",
                ProcessState::Pending => "pending",
                ProcessState::Stopped => "stopped",
                ProcessState::Crashed => "crashed",
                ProcessState::Exited => "exited",
                ProcessState::Restarting => "restarting",
            };
            row![
                text(name).size(14).color(color!(0xdddddd)),
                text(format!(" [{state_str}]"))
                    .size(12)
                    .color(color!(0x888888)),
            ]
        } else {
            row![text("No process selected").size(14).color(color!(0x888888))]
        };

        // GPU-rendered terminal canvas
        let terminal_canvas: Element<'_, Message> = if let Some(program) = &self.terminal_program {
            iced::widget::Canvas::new(program)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            container(text("No terminal output").size(12).color(color!(0x666666)))
                .padding(16)
                .into()
        };

        container(column![container(header).padding([8, 12]), terminal_canvas,])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &Theme| container::Style {
                background: Some(iced::Background::Color(color!(0x0d0d1a))),
                ..Default::default()
            })
            .into()
    }

    // ── Terminal content extraction ──

    fn refresh_terminal_content(&mut self) {
        let Some(name) = &self.focused else {
            self.terminal_program = None;
            return;
        };

        let Some(handle) = self.supervisor.get_process(name) else {
            self.terminal_program = None;
            return;
        };

        let term = handle.terminal.lock();
        let content = grid_adapter::extract_content(&*term);
        self.terminal_program = Some(TerminalProgram::new(content));
    }

    // ── Keyboard handling ──

    fn handle_key(&mut self, key: &keyboard::Key, modifiers: keyboard::Modifiers) {
        if modifiers.command() {
            return;
        }

        if let Some(name) = &self.focused {
            if let Some(handle) = self.supervisor.get_process(name) {
                if handle.is_running() {
                    if let Some(bytes) = key_to_bytes(key, modifiers) {
                        handle.write_to_pty(&bytes);
                    }
                }
            }
        }
    }
}

fn key_to_bytes(key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Option<Vec<u8>> {
    match key {
        keyboard::Key::Character(c) => {
            let ch = c.chars().next()?;
            if modifiers.control() {
                let code = ch.to_ascii_lowercase() as u8;
                if code.is_ascii_lowercase() {
                    return Some(vec![code - b'a' + 1]);
                }
            }
            Some(c.as_bytes().to_vec())
        }
        keyboard::Key::Named(named) => {
            let bytes = match named {
                keyboard::key::Named::Enter => b"\r".to_vec(),
                keyboard::key::Named::Backspace => vec![0x7f],
                keyboard::key::Named::Tab => b"\t".to_vec(),
                keyboard::key::Named::Escape => vec![0x1b],
                keyboard::key::Named::ArrowUp => b"\x1b[A".to_vec(),
                keyboard::key::Named::ArrowDown => b"\x1b[B".to_vec(),
                keyboard::key::Named::ArrowRight => b"\x1b[C".to_vec(),
                keyboard::key::Named::ArrowLeft => b"\x1b[D".to_vec(),
                keyboard::key::Named::Home => b"\x1b[H".to_vec(),
                keyboard::key::Named::End => b"\x1b[F".to_vec(),
                keyboard::key::Named::PageUp => b"\x1b[5~".to_vec(),
                keyboard::key::Named::PageDown => b"\x1b[6~".to_vec(),
                keyboard::key::Named::Delete => b"\x1b[3~".to_vec(),
                keyboard::key::Named::Space => b" ".to_vec(),
                _ => return None,
            };
            Some(bytes)
        }
        keyboard::Key::Unidentified => None,
    }
}
