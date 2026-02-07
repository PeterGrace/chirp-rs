// CHIRP-RS main entry point
// Launches the Qt GUI application

use chirp_rs::gui::run_qt_app;
use console_subscriber as tokio_console_subscriber;
use tracing_subscriber::{EnvFilter, Registry, prelude::*};
use tracing_subscriber::fmt::{format::FmtSpan, time::SystemTime};

fn main() -> std::process::ExitCode {
    // Initialize tracing with timestamps
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

    tracing::info!("Starting CHIRP-RS with Qt GUI...");

    // Run the Qt application
    let exit_code = run_qt_app();

    std::process::ExitCode::from(exit_code as u8)
}
