// Serial port abstraction with async support
// Wraps the serialport crate with tokio async functionality

use std::io;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

#[derive(Error, Debug)]
pub enum SerialError {
    #[error("Serial port error: {0}")]
    Port(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Timeout after {0:?}")]
    Timeout(Duration),

    #[error("Port not open")]
    NotOpen,

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

pub type Result<T> = std::result::Result<T, SerialError>;

/// Serial port configuration
#[derive(Debug, Clone)]
pub struct SerialConfig {
    /// Baud rate (e.g., 9600, 19200, 38400, 57600, 115200)
    pub baud_rate: u32,

    /// Data bits (5, 6, 7, 8)
    pub data_bits: serialport::DataBits,

    /// Stop bits
    pub stop_bits: serialport::StopBits,

    /// Parity
    pub parity: serialport::Parity,

    /// Flow control
    pub flow_control: serialport::FlowControl,

    /// Read/write timeout
    pub timeout: Duration,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            baud_rate: 9600,
            data_bits: serialport::DataBits::Eight,
            stop_bits: serialport::StopBits::One,
            parity: serialport::Parity::None,
            flow_control: serialport::FlowControl::None,
            timeout: Duration::from_secs(2),
        }
    }
}

impl SerialConfig {
    /// Create a new configuration with specified baud rate
    pub fn new(baud_rate: u32) -> Self {
        Self {
            baud_rate,
            ..Default::default()
        }
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set hardware flow control (RTS/CTS)
    pub fn with_hardware_flow(mut self) -> Self {
        self.flow_control = serialport::FlowControl::Hardware;
        self
    }
}

/// Async serial port wrapper
pub struct SerialPort {
    port: Option<Box<dyn serialport::SerialPort>>,
    config: SerialConfig,
    port_name: String,
}

impl SerialPort {
    /// Open a serial port with the given configuration
    pub fn open(port_name: &str, config: SerialConfig) -> Result<Self> {
        let mut port = serialport::new(port_name, config.baud_rate)
            .data_bits(config.data_bits)
            .stop_bits(config.stop_bits)
            .parity(config.parity)
            .flow_control(config.flow_control)
            .timeout(config.timeout)
            .open()
            .map_err(|e| SerialError::Port(e.to_string()))?;

        // Try to set DTR and RTS (common for radios)
        let _ = port.write_data_terminal_ready(true);
        let _ = port.write_request_to_send(true);

        Ok(Self {
            port: Some(port),
            config,
            port_name: port_name.to_string(),
        })
    }

    /// Get the port name
    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    /// Get the configuration
    pub fn config(&self) -> &SerialConfig {
        &self.config
    }

    /// Read exactly n bytes with timeout
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;

        timeout(self.config.timeout, async {
            let mut total_read = 0;
            while total_read < buf.len() {
                match port.read(&mut buf[total_read..]) {
                    Ok(0) => {
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Port closed"))
                    }
                    Ok(n) => total_read += n,
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| SerialError::Timeout(self.config.timeout))?
        .map_err(SerialError::Io)
    }

    /// Read up to n bytes with timeout
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;

        timeout(self.config.timeout, async {
            loop {
                match port.read(buf) {
                    Ok(n) => return Ok(n),
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
        })
        .await
        .map_err(|_| SerialError::Timeout(self.config.timeout))?
        .map_err(SerialError::Io)
    }

    /// Write all bytes with timeout
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;

        timeout(self.config.timeout, async {
            port.write_all(buf).map_err(SerialError::Io)
        })
        .await
        .map_err(|_| SerialError::Timeout(self.config.timeout))?
    }

    /// Write bytes and return number written
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;

        timeout(self.config.timeout, async {
            port.write(buf).map_err(SerialError::Io)
        })
        .await
        .map_err(|_| SerialError::Timeout(self.config.timeout))?
    }

    /// Flush the output buffer
    pub async fn flush(&mut self) -> Result<()> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        port.flush().map_err(SerialError::Io)
    }

    /// Clear input buffer
    pub fn clear_input(&mut self) -> Result<()> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        port.clear(serialport::ClearBuffer::Input)
            .map_err(|e| SerialError::Port(e.to_string()))
    }

    /// Clear output buffer
    pub fn clear_output(&mut self) -> Result<()> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        port.clear(serialport::ClearBuffer::Output)
            .map_err(|e| SerialError::Port(e.to_string()))
    }

    /// Clear both input and output buffers
    pub fn clear_all(&mut self) -> Result<()> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        port.clear(serialport::ClearBuffer::All)
            .map_err(|e| SerialError::Port(e.to_string()))
    }

    /// Set DTR (Data Terminal Ready)
    pub fn set_dtr(&mut self, level: bool) -> Result<()> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        port.write_data_terminal_ready(level)
            .map_err(|e| SerialError::Port(e.to_string()))
    }

    /// Set RTS (Request To Send)
    pub fn set_rts(&mut self, level: bool) -> Result<()> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        port.write_request_to_send(level)
            .map_err(|e| SerialError::Port(e.to_string()))
    }

    /// Change the baud rate
    pub fn set_baud_rate(&mut self, baud_rate: u32) -> Result<()> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        port.set_baud_rate(baud_rate).map_err(|e| {
            SerialError::Port(format!("Failed to set baud rate to {}: {}", baud_rate, e))
        })?;
        self.config.baud_rate = baud_rate;
        Ok(())
    }

    /// Get number of bytes available to read
    pub fn bytes_to_read(&mut self) -> Result<u32> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        port.bytes_to_read()
            .map_err(|e| SerialError::Port(e.to_string()))
    }

    /// Close the port
    pub fn close(mut self) -> Result<()> {
        self.port.take();
        Ok(())
    }
}

/// List available serial ports
pub fn list_ports() -> Result<Vec<String>> {
    serialport::available_ports()
        .map_err(|e| SerialError::Port(e.to_string()))?
        .into_iter()
        .map(|p| Ok(p.port_name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_config() {
        let config = SerialConfig::default();
        assert_eq!(config.baud_rate, 9600);
        assert_eq!(config.data_bits, serialport::DataBits::Eight);

        let config = SerialConfig::new(19200).with_timeout(Duration::from_secs(5));
        assert_eq!(config.baud_rate, 19200);
        assert_eq!(config.timeout, Duration::from_secs(5));
    }

    #[test]
    fn test_list_ports() {
        // This should not fail even if no ports are available
        let result = list_ports();
        assert!(result.is_ok());
    }
}
