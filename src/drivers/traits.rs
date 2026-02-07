// Radio driver traits
// Reference: chirp/chirp_common.py lines 1240-1500

use crate::core::{Memory, RadioFeatures};
use crate::memmap::MemoryMap;
use crate::serial::SerialPort;
use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RadioError {
    #[error("Serial communication error: {0}")]
    Serial(String),

    #[error("Invalid memory location: {0}")]
    InvalidMemory(u32),

    #[error("Radio did not respond")]
    NoResponse,

    #[error("Invalid response from radio: {0}")]
    InvalidResponse(String),

    #[error("Radio NAK'd operation")]
    Nak,

    #[error("Timeout waiting for radio")]
    Timeout,

    #[error("Unsupported operation: {0}")]
    Unsupported(String),

    #[error("Radio error: {0}")]
    Radio(String),
}

impl From<crate::serial::SerialError> for RadioError {
    fn from(err: crate::serial::SerialError) -> Self {
        RadioError::Serial(err.to_string())
    }
}

impl From<crate::bitwise::bcd::BcdError> for RadioError {
    fn from(err: crate::bitwise::bcd::BcdError) -> Self {
        RadioError::InvalidResponse(format!("BCD decode error: {}", err))
    }
}

pub type RadioResult<T> = std::result::Result<T, RadioError>;

/// Progress callback for download/upload operations
pub type StatusCallback = Box<dyn Fn(usize, usize, &str) + Send + Sync>;

/// Base trait for all radio drivers
pub trait Radio: Send {
    /// Get the radio vendor name
    fn vendor(&self) -> &str;

    /// Get the radio model name
    fn model(&self) -> &str;

    /// Get the radio's feature set
    fn get_features(&self) -> RadioFeatures;

    /// Get a printable name for this radio
    fn get_name(&self) -> String {
        format!("{} {}", self.vendor(), self.model())
    }

    /// Get a memory from the radio
    /// Returns None if the memory is empty
    fn get_memory(&mut self, number: u32) -> RadioResult<Option<Memory>>;

    /// Set a memory in the radio
    fn set_memory(&mut self, memory: &Memory) -> RadioResult<()>;

    /// Delete a memory (mark as empty)
    fn delete_memory(&mut self, number: u32) -> RadioResult<()> {
        let mut mem = Memory::new_empty(number);
        self.set_memory(&mem)
    }

    /// Get all memories from the radio
    fn get_memories(&mut self) -> RadioResult<Vec<Memory>> {
        let features = self.get_features();
        let (start, end) = features.memory_bounds;
        let mut memories = Vec::new();

        for i in start..=end {
            if let Some(mem) = self.get_memory(i)? {
                memories.push(mem);
            }
        }

        Ok(memories)
    }
}

/// Trait for radios that support clone mode (full memory dump)
/// Reference: chirp/chirp_common.py lines 1498-1641
pub trait CloneModeRadio: Radio {
    /// Get the size of the radio's memory map in bytes
    fn get_memsize(&self) -> usize;

    /// Download the radio's memory map
    /// This initiates a radio-to-PC clone operation
    async fn sync_in(
        &mut self,
        port: &mut SerialPort,
        status_fn: Option<StatusCallback>,
    ) -> RadioResult<MemoryMap>;

    /// Upload a memory map to the radio
    /// This initiates a PC-to-radio clone operation
    async fn sync_out(
        &mut self,
        port: &mut SerialPort,
        mmap: &MemoryMap,
        status_fn: Option<StatusCallback>,
    ) -> RadioResult<()>;

    /// Process the memory map after loading from file
    fn process_mmap(&mut self, mmap: &MemoryMap) -> RadioResult<()>;

    /// Check if this driver matches a given file
    fn match_model(data: &[u8], filename: &str) -> bool
    where
        Self: Sized;
}

/// Status information for progress reporting
#[derive(Debug, Clone)]
pub struct Status {
    pub current: usize,
    pub max: usize,
    pub message: String,
}

impl Status {
    pub fn new(current: usize, max: usize, message: impl Into<String>) -> Self {
        Self {
            current,
            max,
            message: message.into(),
        }
    }

    pub fn percent(&self) -> f32 {
        if self.max == 0 {
            return 100.0;
        }
        (self.current as f32 / self.max as f32) * 100.0
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}/{}  - {:.1}%)",
            self.message,
            self.current,
            self.max,
            self.percent()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status() {
        let status = Status::new(50, 100, "Downloading");
        assert_eq!(status.percent(), 50.0);
        assert_eq!(status.current, 50);
        assert_eq!(status.max, 100);
    }
}
