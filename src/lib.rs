// CHIRP-RS: Rust port of CHIRP radio programming software
// Copyright 2024 - Licensed under GPLv3

pub mod bitwise;
pub mod core;
pub mod drivers;
pub mod formats;
pub mod memmap;
pub mod serial;

#[cfg(feature = "gui")]
pub mod gui;

// Re-export commonly used types
pub use bitwise::{bcd_to_int, int_to_bcd, BcdArray};
pub use core::{
    constants::*, features::RadioFeatures, memory::{DVMemory, Memory}, power::PowerLevel,
    validation,
};
pub use drivers::{list_drivers, CloneModeRadio, Radio, RadioError};
pub use formats::{load_img, save_img, Metadata};
pub use memmap::MemoryMap;
pub use serial::{BlockProtocol, ProgressCallback, SerialConfig, SerialPort};

/// CHIRP version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
