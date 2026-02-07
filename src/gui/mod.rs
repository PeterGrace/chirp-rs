// GUI module for CHIRP-RS using iced framework
// Architecture: Elm-like MVU (Model-View-Update)

pub mod app;
pub mod dialogs;
pub mod messages;
pub mod radio_ops;

pub use app::ChirpApp;
pub use messages::Message;
