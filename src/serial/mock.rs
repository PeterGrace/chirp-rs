// Mock serial port for testing without hardware

use super::comm::{SerialConfig, SerialError};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Mock serial port for testing
#[derive(Clone)]
pub struct MockSerialPort {
    /// Data to be read (simulates radio responses)
    read_buffer: Arc<Mutex<VecDeque<u8>>>,

    /// Data that was written (simulates commands sent to radio)
    write_buffer: Arc<Mutex<Vec<u8>>>,

    /// Configuration
    config: SerialConfig,

    /// Simulated delay for read/write operations (in ms)
    delay_ms: u64,
}

impl MockSerialPort {
    /// Create a new mock serial port
    pub fn new() -> Self {
        Self {
            read_buffer: Arc::new(Mutex::new(VecDeque::new())),
            write_buffer: Arc::new(Mutex::new(Vec::new())),
            config: SerialConfig::default(),
            delay_ms: 0,
        }
    }

    /// Create with specific configuration
    pub fn with_config(config: SerialConfig) -> Self {
        Self {
            config,
            ..Self::new()
        }
    }

    /// Set simulated delay for operations
    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Push data to be read (simulates radio sending data)
    pub fn push_read_data(&mut self, data: &[u8]) {
        let mut buffer = self.read_buffer.lock().unwrap();
        for &byte in data {
            buffer.push_back(byte);
        }
    }

    /// Get data that was written (simulates reading commands sent to radio)
    pub fn get_written_data(&self) -> Vec<u8> {
        self.write_buffer.lock().unwrap().clone()
    }

    /// Clear written data
    pub fn clear_written_data(&mut self) {
        self.write_buffer.lock().unwrap().clear();
    }

    /// Check if a specific command was written
    pub fn was_written(&self, expected: &[u8]) -> bool {
        let buffer = self.write_buffer.lock().unwrap();
        buffer
            .windows(expected.len())
            .any(|window| window == expected)
    }

    /// Get number of bytes available to read
    pub fn bytes_available(&self) -> usize {
        self.read_buffer.lock().unwrap().len()
    }

    /// Simulate reading bytes
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        if self.delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        }

        let mut buffer = self.read_buffer.lock().unwrap();
        let mut count = 0;

        for item in buf.iter_mut() {
            if let Some(byte) = buffer.pop_front() {
                *item = byte;
                count += 1;
            } else {
                break;
            }
        }

        if count == 0 {
            Err(SerialError::Timeout(self.config.timeout))
        } else {
            Ok(count)
        }
    }

    /// Simulate reading exact number of bytes
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), SerialError> {
        if self.delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        }

        let mut buffer = self.read_buffer.lock().unwrap();

        if buffer.len() < buf.len() {
            return Err(SerialError::Timeout(self.config.timeout));
        }

        for item in buf.iter_mut() {
            *item = buffer.pop_front().unwrap();
        }

        Ok(())
    }

    /// Simulate writing bytes
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, SerialError> {
        if self.delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        }

        let mut buffer = self.write_buffer.lock().unwrap();
        buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    /// Simulate writing all bytes
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<(), SerialError> {
        self.write(buf).await?;
        Ok(())
    }

    /// Simulate flush (no-op for mock)
    pub async fn flush(&mut self) -> Result<(), SerialError> {
        Ok(())
    }
}

impl Default for MockSerialPort {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create a mock port with pre-loaded response data
pub fn mock_port_with_response(response: &[u8]) -> MockSerialPort {
    let mut port = MockSerialPort::new();
    port.push_read_data(response);
    port
}

/// Helper to create a mock port that simulates a radio download
/// Responds to each block request with the corresponding block of data
pub fn mock_clone_mode_radio(memory_data: Vec<u8>, block_size: usize) -> MockSerialPort {
    let mut port = MockSerialPort::new();

    // Pre-load all blocks
    // In a real implementation, this would respond to specific commands
    // For testing, we just provide all the data sequentially
    port.push_read_data(&memory_data);

    port
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_serial_basic() {
        let mut port = MockSerialPort::new();

        // Push data to read
        port.push_read_data(b"Hello");

        // Read it back
        let mut buf = [0u8; 5];
        port.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"Hello");

        // Write data
        port.write_all(b"World").await.unwrap();
        assert_eq!(port.get_written_data(), b"World");
    }

    #[tokio::test]
    async fn test_mock_serial_timeout() {
        let mut port = MockSerialPort::new();

        // Try to read when no data available
        let mut buf = [0u8; 5];
        let result = port.read_exact(&mut buf).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_serial_partial_read() {
        let mut port = MockSerialPort::new();

        // Push less data than requested
        port.push_read_data(b"Hi");

        // Read more than available
        let mut buf = [0u8; 5];
        let n = port.read(&mut buf).await.unwrap();
        assert_eq!(n, 2);
        assert_eq!(&buf[..2], b"Hi");
    }

    #[tokio::test]
    async fn test_mock_was_written() {
        let mut port = MockSerialPort::new();

        port.write_all(b"COMMAND123").await.unwrap();

        assert!(port.was_written(b"COMMAND"));
        assert!(port.was_written(b"123"));
        assert!(!port.was_written(b"NOTFOUND"));
    }

    #[tokio::test]
    async fn test_mock_with_delay() {
        let mut port = MockSerialPort::new().with_delay(10);
        port.push_read_data(b"Test");

        let start = std::time::Instant::now();
        let mut buf = [0u8; 4];
        port.read_exact(&mut buf).await.unwrap();
        let elapsed = start.elapsed();

        // Should take at least 10ms
        assert!(elapsed.as_millis() >= 10);
    }
}
