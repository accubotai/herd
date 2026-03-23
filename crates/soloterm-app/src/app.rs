use iced::widget::{column, container, row, text};
use iced::{Element, Length, Theme};

/// Main `SoloTerm` application state.
#[derive(Default)]
pub(crate) struct SoloTermApp {
    /// Status message
    status: String,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    // Placeholder — real messages will be added in Phase 2
}

impl SoloTermApp {
    #[allow(clippy::unused_self)]
    pub(crate) fn update(&mut self, _message: Message) {
        // Will handle real messages in Phase 2
    }

    pub(crate) fn view(&self) -> Element<'_, Message> {
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

    #[allow(clippy::unused_self)]
    pub(crate) fn theme(&self) -> Theme {
        Theme::Dark
    }
}
