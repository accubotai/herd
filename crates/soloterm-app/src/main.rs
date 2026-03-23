use anyhow::Result;

mod app;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_env_filter("info").init();

    tracing::info!("Starting SoloTerm");

    // Run the iced application
    iced::application("SoloTerm", app::SoloTermApp::update, app::SoloTermApp::view)
        .theme(app::SoloTermApp::theme)
        .window_size(iced::Size::new(1200.0, 800.0))
        .run()?;

    Ok(())
}
