use anyhow::Result;

mod app;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_env_filter("info").init();

    tracing::info!("Starting Herd");

    // Run the iced application
    iced::application("Herd", app::HerdApp::update, app::HerdApp::view)
        .theme(app::HerdApp::theme)
        .window_size(iced::Size::new(1200.0, 800.0))
        .run()?;

    Ok(())
}
