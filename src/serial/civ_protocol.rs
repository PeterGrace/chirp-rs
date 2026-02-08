// Icom CI-V protocol implementation
// Reference: chirp/drivers/icomciv.py

use crate::drivers::{RadioError, RadioResult};
use crate::serial::SerialPort;

/// CI-V frame structure: 0xFE 0xFE <dst> <src> <cmd> [sub] [data...] 0xFD
pub struct CivFrame {
    cmd: u8,
    sub: Option<u8>,
    data: Vec<u8>,
}

impl CivFrame {
    /// Create a new CI-V frame
    pub fn new(cmd: u8, sub: Option<u8>) -> Self {
        Self {
            cmd,
            sub,
            data: Vec::new(),
        }
    }

    /// Set the frame data payload
    pub fn set_data(&mut self, data: impl Into<Vec<u8>>) {
        self.data = data.into();
    }

    /// Get the frame data payload
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Take ownership of the frame data
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    /// Send the frame over the serial port
    pub async fn send(
        &self,
        port: &mut SerialPort,
        src: u8,
        dst: u8,
        expect_echo: bool,
    ) -> RadioResult<()> {
        // Build frame: 0xFE 0xFE <dst> <src> <cmd> [sub] [data...] 0xFD
        let mut frame = vec![0xFE, 0xFE, dst, src, self.cmd];

        // Add subcommand if present
        if let Some(sub) = self.sub {
            frame.push(sub);
        }

        // Add data payload
        frame.extend_from_slice(&self.data);

        // Add end marker
        frame.push(0xFD);

        // Send frame
        port.write_all(&frame).await?;

        // Read echo if expected
        if expect_echo {
            let mut echo = vec![0u8; frame.len()];
            port.read_exact(&mut echo).await?;

            if echo != frame {
                return Err(RadioError::InvalidResponse(
                    "Echo didn't match sent frame".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Receive a frame from the serial port
    pub async fn receive(port: &mut SerialPort) -> RadioResult<Self> {
        let mut data = Vec::new();

        // Read until we get 0xFD end marker
        loop {
            let mut byte = [0u8; 1];
            port.read_exact(&mut byte).await?;
            data.push(byte[0]);

            if byte[0] == 0xFD {
                break;
            }

            // Prevent infinite loops
            if data.len() > 1024 {
                return Err(RadioError::InvalidResponse(
                    "Frame too large (>1024 bytes)".to_string(),
                ));
            }
        }

        // Check for error response (single 0xFD)
        if data.len() == 1 && data[0] == 0xFD {
            return Err(RadioError::Radio("Radio reported error (0xFD)".to_string()));
        }

        // Validate frame structure
        if data.len() < 6 {
            return Err(RadioError::InvalidResponse(format!(
                "Frame too short: {} bytes",
                data.len()
            )));
        }

        if data[0] != 0xFE || data[1] != 0xFE {
            return Err(RadioError::InvalidResponse(
                "Invalid frame header (expected 0xFE 0xFE)".to_string(),
            ));
        }

        if data[data.len() - 1] != 0xFD {
            return Err(RadioError::InvalidResponse(
                "Invalid frame terminator (expected 0xFD)".to_string(),
            ));
        }

        // Extract frame components
        // data[2] = dst, data[3] = src
        let cmd = data[4];

        // Determine if there's a subcommand (heuristic: if data length suggests it)
        // For memory operations (0x1A), there's always a subcommand
        let (sub, data_start) = if cmd == 0x1A && data.len() > 6 {
            (Some(data[5]), 6)
        } else if data.len() > 6 {
            // Other commands may have subcommands
            (Some(data[5]), 6)
        } else {
            (None, 5)
        };

        // Extract payload (everything between header and 0xFD)
        let payload = data[data_start..data.len() - 1].to_vec();

        Ok(Self {
            cmd,
            sub,
            data: payload,
        })
    }

    /// Check if the frame indicates an empty memory
    pub fn is_empty_memory(&self) -> bool {
        // Empty memory is indicated by 0xFF at the end of data
        !self.data.is_empty() && self.data[self.data.len() - 1] == 0xFF
    }
}

/// CI-V protocol helper for Icom radios
pub struct CivProtocol {
    model_code: u8,
    controller_addr: u8,
    expect_echo: bool,
}

impl CivProtocol {
    /// Create a new CI-V protocol handler
    ///
    /// # Arguments
    /// * `model_code` - Radio model code (e.g., 0xA2 for IC-9700)
    /// * `controller_addr` - Controller address (typically 0xE0)
    pub fn new(model_code: u8, controller_addr: u8) -> Self {
        Self {
            model_code,
            controller_addr,
            expect_echo: false,
        }
    }

    /// Detect if the serial interface echoes frames
    pub async fn detect_echo(&mut self, port: &mut SerialPort) -> RadioResult<bool> {
        // Send a simple test frame
        let test_frame = vec![0xFE, 0xFE, 0xE0, 0xE0, 0xFA, 0xFD];
        port.write_all(&test_frame).await?;

        // Try to read echo with short timeout
        let mut echo = vec![0u8; test_frame.len()];
        match tokio::time::timeout(
            std::time::Duration::from_millis(100),
            port.read_exact(&mut echo),
        )
        .await
        {
            Ok(Ok(())) if echo == test_frame => {
                self.expect_echo = true;
                Ok(true)
            }
            _ => {
                self.expect_echo = false;
                Ok(false)
            }
        }
    }

    /// Send a CI-V frame and receive response
    pub async fn send_command(
        &self,
        port: &mut SerialPort,
        cmd: u8,
        sub: Option<u8>,
        data: &[u8],
    ) -> RadioResult<CivFrame> {
        tracing::debug!("CI-V send_command: cmd=0x{:02X}, sub={:?}, data_len={}", cmd, sub, data.len());

        let mut frame = CivFrame::new(cmd, sub);
        frame.set_data(data);

        tracing::debug!("CI-V sending frame...");
        frame
            .send(
                port,
                self.controller_addr,
                self.model_code,
                self.expect_echo,
            )
            .await?;

        tracing::debug!("CI-V waiting for response...");
        let response = CivFrame::receive(port).await?;
        tracing::debug!("CI-V response received");
        Ok(response)
    }

    /// Read a memory from the radio
    pub async fn read_memory(
        &self,
        port: &mut SerialPort,
        bank: u8,
        channel: u16,
    ) -> RadioResult<Vec<u8>> {
        tracing::debug!("CI-V read_memory: bank={}, channel={}", bank, channel);

        // Build data payload: bank (BCD) + channel (BCD, big-endian, 2 bytes)
        let channel_bcd = format!("{:04}", channel);
        let channel_bytes = u16::from_str_radix(&channel_bcd, 16)
            .map_err(|_| RadioError::InvalidMemory(channel as u32))?
            .to_be_bytes();

        let data = vec![bank, channel_bytes[0], channel_bytes[1]];
        tracing::debug!("CI-V sending command 0x1A/0x00 with data: {:02X?}", data);

        // Command 0x1A, subcommand 0x00 = read memory
        let response = self.send_command(port, 0x1A, Some(0x00), &data).await?;
        tracing::debug!("CI-V received response: {} bytes", response.data().len());

        // Check if memory is empty
        if response.is_empty_memory() {
            return Ok(Vec::new());
        }

        // Check for error
        if response.data().is_empty() {
            return Err(RadioError::Radio("Radio reported error".to_string()));
        }

        // Log hex dump of data received from radio
        let data = response.data();
        let hex_dump: String = data
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ");
        tracing::debug!("read_memory received (hex): bank={}, ch={}, {} bytes: {}", bank, channel, data.len(), hex_dump);

        Ok(response.into_data())
    }

    /// Write a memory to the radio
    pub async fn write_memory(
        &self,
        port: &mut SerialPort,
        bank: u8,
        channel: u16,
        memory_data: &[u8],
    ) -> RadioResult<()> {
        // Build data payload: bank (BCD) + channel (BCD, 2 bytes) + memory data
        let channel_bcd = format!("{:04}", channel);
        let channel_bytes = u16::from_str_radix(&channel_bcd, 16)
            .map_err(|_| RadioError::InvalidMemory(channel as u32))?
            .to_be_bytes();

        let mut data = vec![bank, channel_bytes[0], channel_bytes[1]];
        data.extend_from_slice(memory_data);

        // Log hex dump of data being sent
        let hex_dump: String = data
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ");
        tracing::debug!("write_memory data (hex): {}", hex_dump);

        // Command 0x1A, subcommand 0x00 = write memory
        let response = self.send_command(port, 0x1A, Some(0x00), &data).await?;

        // Log response for debugging
        let response_hex: String = response.data()
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ");
        tracing::debug!("write_memory response: {} bytes: {}", response.data().len(), response_hex);

        // Check for error (NAK = 0xFA) or success (ACK = 0xFB or empty)
        // For write operations, the radio typically returns an ACK with just 0xFB
        if !response.data().is_empty() && response.data()[0] == 0xFA {
            tracing::error!(
                "Radio rejected write_memory (NAK): bank={}, ch={}, data_len={}",
                bank,
                channel,
                memory_data.len()
            );
            return Err(RadioError::Radio("Radio reported error (NAK)".to_string()));
        }

        Ok(())
    }

    /// Erase a memory (mark as empty)
    pub async fn erase_memory(
        &self,
        port: &mut SerialPort,
        bank: u8,
        channel: u16,
    ) -> RadioResult<()> {
        // Build data payload: bank + channel + 0xFF (empty marker)
        let channel_bcd = format!("{:04}", channel);
        let channel_bytes = u16::from_str_radix(&channel_bcd, 16)
            .map_err(|_| RadioError::InvalidMemory(channel as u32))?
            .to_be_bytes();

        let data = vec![bank, channel_bytes[0], channel_bytes[1], 0xFF];

        // Command 0x1A, subcommand 0x00 = write memory (erase with 0xFF)
        let response = self.send_command(port, 0x1A, Some(0x00), &data).await?;

        // Log response for debugging
        let response_hex: String = response.data()
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ");
        tracing::debug!("erase_memory response: {} bytes: {}", response.data().len(), response_hex);

        // Check for error (NAK = 0xFA) or success (ACK = 0xFB or empty)
        // Same logic as write_memory
        if !response.data().is_empty() && response.data()[0] == 0xFA {
            tracing::error!(
                "Radio rejected erase_memory (NAK): bank={}, ch={}",
                bank,
                channel
            );
            return Err(RadioError::Radio("Radio reported error (NAK)".to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_civ_frame_new() {
        let frame = CivFrame::new(0x1A, Some(0x00));
        assert_eq!(frame.cmd, 0x1A);
        assert_eq!(frame.sub, Some(0x00));
        assert!(frame.data.is_empty());
    }

    #[test]
    fn test_civ_frame_set_data() {
        let mut frame = CivFrame::new(0x1A, Some(0x00));
        frame.set_data(vec![0x01, 0x02, 0x03]);
        assert_eq!(frame.data(), &[0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_is_empty_memory() {
        let mut frame = CivFrame::new(0x1A, Some(0x00));
        frame.set_data(vec![0x01, 0x02, 0xFF]);
        assert!(frame.is_empty_memory());

        let mut frame2 = CivFrame::new(0x1A, Some(0x00));
        frame2.set_data(vec![0x01, 0x02, 0x03]);
        assert!(!frame2.is_empty_memory());
    }
}
