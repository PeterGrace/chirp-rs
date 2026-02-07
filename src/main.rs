// CHIRP-RS main entry point
// Launches the iced GUI application

use chirp_rs::gui::ChirpApp;
use iced::{Application, Settings};
use console_subscriber as tokio_console_subscriber;
use tracing_subscriber::{EnvFilter, Registry, prelude::*};
use tracing_subscriber::fmt::{format::FmtSpan, time::SystemTime};

fn main() -> iced::Result {
    // Initialize tracing with timestamps
    //region console logging
    let console_layer = tokio_console_subscriber::spawn();
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("debug"))
        .unwrap();
    let format_layer = tracing_subscriber::fmt::layer()
        .event_format(
            tracing_subscriber::fmt::format()
                .with_file(true)
                .with_timer(SystemTime)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_line_number(true),
        )
        .with_span_events(FmtSpan::NONE);


    let subscriber = Registry::default()
        .with(console_layer)
        .with(filter_layer)
        .with(format_layer);
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
    //endregion

    tracing::info!("Starting CHIRP-RS...");

    // Configure application settings
    let settings = Settings {
        window: iced::window::Settings {
            size: iced::Size::new(1200.0, 800.0),
            position: iced::window::Position::Centered,
            min_size: Some(iced::Size::new(800.0, 600.0)),
            ..Default::default()
        },
        ..Default::default()
    };

    tracing::info!("Launching iced application...");
    // Run the application
    ChirpApp::run(settings)
}
