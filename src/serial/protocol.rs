// Protocol helpers for block-based radio communication
// Many radios transfer memory in fixed-size blocks

use super::comm::{SerialError, SerialPort};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Progress callback type
/// Arguments: (bytes_transferred, total_bytes, status_message)
pub type ProgressCallback = Arc<dyn Fn(usize, usize, &str) + Send + Sync>;

/// Block-based protocol helper
pub struct BlockProtocol {
    block_size: usize,
    total_size: usize,
}

impl BlockProtocol {
    /// Create a new block protocol
    pub fn new(block_size: usize, total_size: usize) -> Self {
        Self {
            block_size,
            total_size,
        }
    }

    /// Calculate number of blocks needed
    pub fn num_blocks(&self) -> usize {
        (self.total_size + self.block_size - 1) / self.block_size
    }

    /// Get the size of a specific block (last block may be smaller)
    pub fn block_size(&self, block_index: usize) -> usize {
        let remaining = self.total_size - (block_index * self.block_size);
        remaining.min(self.block_size)
    }

    /// Calculate progress percentage
    pub fn progress_percent(&self, bytes_transferred: usize) -> f32 {
        if self.total_size == 0 {
            return 100.0;
        }
        (bytes_transferred as f32 / self.total_size as f32) * 100.0
    }

    /// Download blocks from radio
    pub async fn download<F>(
        &self,
        port: &mut SerialPort,
        request_block: F,
        progress: Option<ProgressCallback>,
    ) -> Result<Vec<u8>, SerialError>
    where
        F: Fn(usize) -> Vec<u8>, // Function to generate request for block N
    {
        let mut data = Vec::with_capacity(self.total_size);
        let num_blocks = self.num_blocks();

        for block_idx in 0..num_blocks {
            // Send request for this block
            let request = request_block(block_idx);
            port.write_all(&request).await?;

            // Read block
            let block_len = self.block_size(block_idx);
            let mut block = vec![0u8; block_len];
            port.read_exact(&mut block).await?;

            data.extend_from_slice(&block);

            // Report progress
            if let Some(ref callback) = progress {
                let bytes = data.len();
                let percent = self.progress_percent(bytes);
                let msg = format!(
                    "Downloaded block {}/{} ({:.1}%)",
                    block_idx + 1,
                    num_blocks,
                    percent
                );
                callback(bytes, self.total_size, &msg);
            }
        }

        Ok(data)
    }

    /// Upload blocks to radio
    pub async fn upload<F>(
        &self,
        port: &mut SerialPort,
        data: &[u8],
        send_block: F,
        progress: Option<ProgressCallback>,
    ) -> Result<(), SerialError>
    where
        F: Fn(usize, &[u8]) -> Vec<u8>, // Function to format block N with data
    {
        if data.len() != self.total_size {
            return Err(SerialError::InvalidConfig(format!(
                "Data size {} doesn't match expected size {}",
                data.len(),
                self.total_size
            )));
        }

        let num_blocks = self.num_blocks();

        for block_idx in 0..num_blocks {
            let start = block_idx * self.block_size;
            let end = start + self.block_size(block_idx);
            let block_data = &data[start..end];

            // Format and send block
            let message = send_block(block_idx, block_data);
            port.write_all(&message).await?;

            // Some radios send ACK, read it
            let mut ack = [0u8; 1];
            let _ = port.read(&mut ack).await; // Ignore errors for radios without ACK

            // Report progress
            if let Some(ref callback) = progress {
                let bytes = end;
                let percent = self.progress_percent(bytes);
                let msg = format!(
                    "Uploaded block {}/{} ({:.1}%)",
                    block_idx + 1,
                    num_blocks,
                    percent
                );
                callback(bytes, self.total_size, &msg);
            }
        }

        Ok(())
    }

    /// Download with automatic block request (simple sequential protocol)
    pub async fn download_simple(
        &self,
        port: &mut SerialPort,
        init_command: &[u8],
        progress: Option<ProgressCallback>,
    ) -> Result<Vec<u8>, SerialError> {
        // Send initialization command
        port.write_all(init_command).await?;

        // Read all data
        let mut data = vec![0u8; self.total_size];
        let num_blocks = self.num_blocks();

        for block_idx in 0..num_blocks {
            let start = block_idx * self.block_size;
            let block_len = self.block_size(block_idx);
            let end = start + block_len;

            port.read_exact(&mut data[start..end]).await?;

            // Report progress
            if let Some(ref callback) = progress {
                let percent = self.progress_percent(end);
                let msg = format!(
                    "Downloaded {}/{} bytes ({:.1}%)",
                    end, self.total_size, percent
                );
                callback(end, self.total_size, &msg);
            }
        }

        Ok(data)
    }

    /// Upload with simple protocol (send all data at once)
    pub async fn upload_simple(
        &self,
        port: &mut SerialPort,
        data: &[u8],
        init_command: &[u8],
        progress: Option<ProgressCallback>,
    ) -> Result<(), SerialError> {
        if data.len() != self.total_size {
            return Err(SerialError::InvalidConfig(format!(
                "Data size {} doesn't match expected size {}",
                data.len(),
                self.total_size
            )));
        }

        // Send initialization command
        port.write_all(init_command).await?;

        // Send data in blocks for progress reporting
        let num_blocks = self.num_blocks();

        for block_idx in 0..num_blocks {
            let start = block_idx * self.block_size;
            let block_len = self.block_size(block_idx);
            let end = start + block_len;

            port.write_all(&data[start..end]).await?;

            // Report progress
            if let Some(ref callback) = progress {
                let percent = self.progress_percent(end);
                let msg = format!(
                    "Uploaded {}/{} bytes ({:.1}%)",
                    end, self.total_size, percent
                );
                callback(end, self.total_size, &msg);
            }
        }

        port.flush().await?;
        Ok(())
    }
}

/// Helper to read until a specific byte is encountered
pub async fn read_until(
    port: &mut SerialPort,
    delimiter: u8,
    max_len: usize,
) -> Result<Vec<u8>, SerialError> {
    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];

    while buffer.len() < max_len {
        port.read_exact(&mut byte).await?;
        buffer.push(byte[0]);

        if byte[0] == delimiter {
            break;
        }
    }

    Ok(buffer)
}

/// Helper to read a specific response pattern
pub async fn expect_response(port: &mut SerialPort, expected: &[u8]) -> Result<(), SerialError> {
    let mut response = vec![0u8; expected.len()];
    port.read_exact(&mut response).await?;

    if response != expected {
        return Err(SerialError::Port(format!(
            "Unexpected response: expected {:?}, got {:?}",
            expected, response
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_protocol() {
        let protocol = BlockProtocol::new(64, 256);
        assert_eq!(protocol.num_blocks(), 4);
        assert_eq!(protocol.block_size(0), 64);
        assert_eq!(protocol.block_size(3), 64);

        // Test with non-aligned size
        let protocol = BlockProtocol::new(64, 200);
        assert_eq!(protocol.num_blocks(), 4);
        assert_eq!(protocol.block_size(0), 64);
        assert_eq!(protocol.block_size(3), 8); // Last block is smaller
    }

    #[test]
    fn test_progress_calculation() {
        let protocol = BlockProtocol::new(64, 256);
        assert_eq!(protocol.progress_percent(0), 0.0);
        assert_eq!(protocol.progress_percent(128), 50.0);
        assert_eq!(protocol.progress_percent(256), 100.0);
    }
}
