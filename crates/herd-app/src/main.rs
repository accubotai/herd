use anyhow::Result;

pub(crate) mod app;
mod terminal_widget;

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    tracing::info!("Starting Herd");

    iced::application("Herd", app::HerdApp::update, app::HerdApp::view)
        .subscription(app::HerdApp::subscription)
        .theme(app::HerdApp::theme)
        .window_size(iced::Size::new(1200.0, 800.0))
        .run()?;

    Ok(())
}
