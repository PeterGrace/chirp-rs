// Serial communication module for radio I/O
pub mod civ_protocol;
pub mod comm;
pub mod protocol;

#[cfg(test)]
pub mod mock;

pub use civ_protocol::{CivFrame, CivProtocol};
pub use comm::{SerialPort, SerialConfig, SerialError};
pub use protocol::{BlockProtocol, ProgressCallback};
