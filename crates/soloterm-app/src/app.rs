use iced::widget::{column, container, row, text};
use iced::{Element, Length, Theme};

/// Main SoloTerm application state
#[derive(Default)]
pub struct SoloTermApp {
    /// Currently selected process index
    selected_process: usize,
    /// Status message
    status: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Select a process in the sidebar
    SelectProcess(usize),
}

impl SoloTermApp {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::SelectProcess(idx) => {
                self.selected_process = idx;
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let sidebar = container(
            column![
                text("SoloTerm").size(20),
                text("─────────────").size(14),
                text("No processes configured").size(12),
                text(""),
                text("Create a solo.toml to get started").size(11),
            ]
            .spacing(8)
            .padding(16),
        )
        .width(Length::Fixed(250.0))
        .height(Length::Fill);

        let terminal_area = container(
            column![
                text("Terminal").size(16),
                text("────────────────────────────").size(14),
                text(&self.status).size(13),
                text(""),
                text("Phase 1: GPU-rendered terminal coming next").size(12),
            ]
            .spacing(8)
            .padding(16),
        )
        .width(Length::Fill)
        .height(Length::Fill);

        let content = row![sidebar, terminal_area];

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }
}
