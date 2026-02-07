// CHIRP-RS main entry point
// Launches the iced GUI application

use chirp_rs::gui::ChirpApp;
use iced::{Application, Settings};

fn main() -> iced::Result {
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

    // Run the application
    ChirpApp::run(settings)
}
