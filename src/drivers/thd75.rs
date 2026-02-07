// Kenwood TH-D75 / TH-D74 radio driver
// Reference: chirp/drivers/thd74.py

use super::traits::{CloneModeRadio, Radio, RadioError, RadioResult, Status, StatusCallback};
use crate::bitwise::{read_u32_le, write_u32_le};
use crate::core::{DVMemory, Memory, RadioFeatures, DTCS_CODES, TONES};
use crate::memmap::MemoryMap;
use crate::serial::SerialPort;
use std::time::{Duration, Instant};

/// TH-D74/D75 memory size: 500KB
const MEMSIZE: usize = 0x7A300;

/// Block size for download/upload
const BLOCK_SIZE: usize = 256;

/// File header for .d74 files
const D74_FILE_HEADER: &[u8] = b"MCP-D74\xFFV1.03\xFF\xFF\xFFTH-D74\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x00\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF";

/// Memory offsets
const FLAGS_OFFSET: usize = 0x2000;
const MEMORY_OFFSET: usize = 0x4000;
const NAMES_OFFSET: usize = 0x10000;
const GROUP_NAME_OFFSET: usize = 1152;

/// Number of memories
const NUM_MEMORIES: u32 = 1200;

/// Duplex modes
const DUPLEX: &[&str] = &["", "+", "-"];

/// Tuning steps (kHz)
const TUNE_STEPS: &[f32] = &[5.0, 6.25, 8.33, 9.0, 10.0, 12.5, 15.0, 20.0, 25.0, 30.0, 50.0, 100.0];

/// Cross modes
const CROSS_MODES: &[&str] = &["DTCS->", "Tone->DTCS", "DTCS->Tone", "Tone->Tone"];

/// TH-D75 modes
const THD75_MODES: &[&str] = &["FM", "DV", "AM", "LSB", "USB", "CW", "NFM", "DV"];

/// Memory flags structure (4 bytes per memory at 0x2000)
#[derive(Debug, Clone, Copy)]
struct MemoryFlags {
    used: u8,
    lockout: bool,
    group: u8,
}

impl MemoryFlags {
    fn from_bytes(data: &[u8]) -> Self {
        Self {
            used: data[0],
            lockout: (data[1] & 0x80) != 0,
            group: data[2],
        }
    }

    fn to_bytes(&self) -> [u8; 4] {
        [
            self.used,
            if self.lockout { 0x80 } else { 0x00 },
            self.group,
            0xFF,
        ]
    }
}

/// Raw memory structure (80 bytes at 0x4000)
#[derive(Debug, Clone)]
struct RawMemory {
    freq: u32,           // Frequency in Hz
    offset: u32,         // Offset in Hz
    tuning_step: u8,
    mode: u8,
    narrow: bool,
    tone_mode: u8,
    ctcss_mode: u8,
    dtcs_mode: u8,
    cross_mode: u8,
    split: bool,
    duplex: u8,
    rtone: u8,
    ctone: u8,
    dtcs_code: u8,
    dig_squelch: u8,
    dv_urcall: [u8; 8],
    dv_rpt1call: [u8; 8],
    dv_rpt2call: [u8; 8],
    dv_code: u8,
}

impl RawMemory {
    /// Size of memory structure in bytes
    const SIZE: usize = 80;

    fn from_bytes(data: &[u8]) -> RadioResult<Self> {
        if data.len() < Self::SIZE {
            return Err(RadioError::Radio(format!(
                "Insufficient data for memory: {} bytes",
                data.len()
            )));
        }

        let freq = read_u32_le(&data[0..4]).unwrap();
        let offset = read_u32_le(&data[4..8]).unwrap();

        let tuning_step = data[8] & 0x0F;
        let mode = (data[9] >> 1) & 0x07;
        let narrow = (data[9] & 0x08) != 0;

        let tone_mode = data[10] & 0x01;
        let ctcss_mode = (data[10] >> 1) & 0x01;
        let dtcs_mode = (data[10] >> 2) & 0x01;
        let cross_mode = (data[10] >> 3) & 0x01;
        let split = (data[10] & 0x20) != 0;
        let duplex = (data[10] >> 6) & 0x03;

        let rtone = data[11];
        let ctone = data[12] & 0x3F;
        let dtcs_code = data[13] & 0x7F;
        let dig_squelch = data[14] & 0x03;

        let mut dv_urcall = [0u8; 8];
        let mut dv_rpt1call = [0u8; 8];
        let mut dv_rpt2call = [0u8; 8];

        dv_urcall.copy_from_slice(&data[15..23]);
        dv_rpt1call.copy_from_slice(&data[23..31]);
        dv_rpt2call.copy_from_slice(&data[31..39]);

        let dv_code = data[39] & 0x7F;

        Ok(Self {
            freq,
            offset,
            tuning_step,
            mode,
            narrow,
            tone_mode,
            ctcss_mode,
            dtcs_mode,
            cross_mode,
            split,
            duplex,
            rtone,
            ctone,
            dtcs_code,
            dig_squelch,
            dv_urcall,
            dv_rpt1call,
            dv_rpt2call,
            dv_code,
        })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; Self::SIZE];

        // Frequency and offset (little-endian u32)
        bytes[0..4].copy_from_slice(&write_u32_le(self.freq));
        bytes[4..8].copy_from_slice(&write_u32_le(self.offset));

        // Tuning step and mode
        bytes[8] = self.tuning_step & 0x0F;
        bytes[9] = ((self.mode & 0x07) << 1) | if self.narrow { 0x08 } else { 0x00 };

        // Tone settings
        bytes[10] = (self.tone_mode & 0x01)
            | ((self.ctcss_mode & 0x01) << 1)
            | ((self.dtcs_mode & 0x01) << 2)
            | ((self.cross_mode & 0x01) << 3)
            | (if self.split { 0x20 } else { 0x00 })
            | ((self.duplex & 0x03) << 6);

        bytes[11] = self.rtone;
        bytes[12] = self.ctone & 0x3F;
        bytes[13] = self.dtcs_code & 0x7F;
        bytes[14] = self.dig_squelch & 0x03;

        // D-STAR calls
        bytes[15..23].copy_from_slice(&self.dv_urcall);
        bytes[23..31].copy_from_slice(&self.dv_rpt1call);
        bytes[31..39].copy_from_slice(&self.dv_rpt2call);
        bytes[39] = self.dv_code & 0x7F;

        bytes
    }
}

/// Kenwood TH-D75 radio driver
pub struct THD75Radio {
    mmap: Option<MemoryMap>,
    vendor: String,
    model: String,
}

impl THD75Radio {
    pub fn new() -> Self {
        Self {
            mmap: None,
            vendor: "Kenwood".to_string(),
            model: "TH-D75".to_string(),
        }
    }

    /// Calculate memory offset for a given channel number
    fn memory_offset(&self, number: u32) -> usize {
        // Memories are organized in groups of 6, with 16-byte padding
        let group = (number / 6) as usize;
        let index = (number % 6) as usize;
        MEMORY_OFFSET + (group * (6 * RawMemory::SIZE + 16)) + (index * RawMemory::SIZE)
    }

    /// Calculate flags offset for a given channel number
    fn flags_offset(&self, number: u32) -> usize {
        FLAGS_OFFSET + (number as usize * 4)
    }

    /// Calculate name offset for a given channel number
    fn name_offset(&self, number: u32) -> usize {
        NAMES_OFFSET + (number as usize * 16)
    }

    /// Read a block from the radio
    async fn read_block(
        &self,
        port: &mut SerialPort,
        block: u16,
    ) -> RadioResult<Vec<u8>> {
        // Send read command: "R" + block number (big-endian u16) + 0x0000
        let mut cmd = vec![b'R'];
        cmd.extend_from_slice(&block.to_be_bytes());
        cmd.extend_from_slice(&[0x00, 0x00]);

        port.write_all(&cmd)
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        // Read response header: "W" + block number + 0x0000
        let mut header = [0u8; 5];
        port.read_exact(&mut header)
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        if header[0] != b'W' {
            return Err(RadioError::InvalidResponse(format!(
                "Expected 'W', got '{}'",
                header[0] as char
            )));
        }

        let response_block = u16::from_be_bytes([header[1], header[2]]);
        if response_block != block {
            return Err(RadioError::InvalidResponse(format!(
                "Block mismatch: expected {}, got {}",
                block, response_block
            )));
        }

        // Read block data
        let mut data = vec![0u8; BLOCK_SIZE];
        port.read_exact(&mut data)
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        // Send ACK
        port.write_all(&[0x06])
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        // Wait for ACK response
        let mut ack = [0u8; 1];
        port.read_exact(&mut ack)
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        if ack[0] != 0x06 {
            return Err(RadioError::Nak);
        }

        Ok(data)
    }

    /// Write a block to the radio
    async fn write_block(
        &self,
        port: &mut SerialPort,
        block: u16,
        data: &[u8],
    ) -> RadioResult<()> {
        // Send write command: "W" + block number + size + data
        let mut cmd = vec![b'W'];
        cmd.extend_from_slice(&block.to_be_bytes());

        let size = if data.len() < BLOCK_SIZE {
            data.len() as u16
        } else {
            0
        };
        cmd.extend_from_slice(&size.to_be_bytes());
        cmd.extend_from_slice(data);

        port.write_all(&cmd)
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        port.flush()
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        // Wait for ACK
        let mut ack = [0u8; 1];
        port.read_exact(&mut ack)
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        if ack[0] != 0x06 {
            return Err(RadioError::Nak);
        }

        Ok(())
    }

    /// Send a command and get response
    async fn command(
        &self,
        port: &mut SerialPort,
        cmd: &str,
    ) -> RadioResult<String> {
        let cmd_bytes = format!("{}\r", cmd);
        port.write_all(cmd_bytes.as_bytes())
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        // Read until \r
        let mut response = Vec::new();
        let mut byte = [0u8; 1];
        let start = Instant::now();

        while start.elapsed() < Duration::from_secs(2) {
            match port.read(&mut byte).await {
                Ok(1) => {
                    response.push(byte[0]);
                    if byte[0] == b'\r' {
                        break;
                    }
                }
                _ => continue,
            }
        }

        String::from_utf8(response)
            .map(|s| s.trim().to_string())
            .map_err(|_| RadioError::InvalidResponse("Invalid UTF-8".to_string()))
    }

    /// Get radio ID
    async fn get_id(&self, port: &mut SerialPort) -> RadioResult<String> {
        let response = self.command(port, "ID").await?;
        if response.starts_with("ID ") {
            Ok(response.split_whitespace().nth(1).unwrap_or("").to_string())
        } else {
            Err(RadioError::NoResponse)
        }
    }

    /// Detect baud rate
    async fn detect_baud(&self, port: &mut SerialPort) -> RadioResult<String> {
        // Note: serialport doesn't support runtime baud rate changes easily
        // For now, we'll assume 9600 is set correctly at port opening
        port.write_all(b"\r\r")
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        // Clear any pending data
        let mut buf = [0u8; 32];
        let _ = port.read(&mut buf).await;

        self.get_id(port).await
    }

    /// Convert raw memory to Memory struct
    fn decode_memory(&self, number: u32, raw: &RawMemory, name: &str, flags: &MemoryFlags) -> RadioResult<Memory> {
        let mut mem = if raw.mode == 1 {
            // D-STAR mode - use DVMemory
            let mut dv = DVMemory::new(number);
            dv.base.freq = raw.freq as u64;
            dv.base.offset = raw.offset as u64;
            dv.base.mode = "DV".to_string();

            // Decode D-STAR calls
            dv.dv_urcall = String::from_utf8_lossy(&raw.dv_urcall)
                .trim_end_matches('\0')
                .to_string();
            dv.dv_rpt1call = String::from_utf8_lossy(&raw.dv_rpt1call)
                .trim_end_matches('\0')
                .to_string();
            dv.dv_rpt2call = String::from_utf8_lossy(&raw.dv_rpt2call)
                .trim_end_matches('\0')
                .to_string();
            dv.dv_code = raw.dv_code;

            // For now, return base memory
            // TODO: Handle DVMemory properly
            dv.base.clone()
        } else {
            Memory::new(number)
        };

        mem.number = number;
        mem.name = name.to_string();
        mem.freq = raw.freq as u64;
        mem.offset = raw.offset as u64;

        // Mode
        if (raw.mode as usize) < THD75_MODES.len() {
            mem.mode = THD75_MODES[raw.mode as usize].to_string();
        }

        // Duplex
        if (raw.duplex as usize) < DUPLEX.len() {
            mem.duplex = DUPLEX[raw.duplex as usize].to_string();
        }

        // Tuning step
        if (raw.tuning_step as usize) < TUNE_STEPS.len() {
            mem.tuning_step = TUNE_STEPS[raw.tuning_step as usize];
        }

        // Tones
        if (raw.rtone as usize) < TONES.len() {
            mem.rtone = TONES[raw.rtone as usize];
        }
        if (raw.ctone as usize) < TONES.len() {
            mem.ctone = TONES[raw.ctone as usize];
        }

        // DTCS
        if (raw.dtcs_code as usize) < DTCS_CODES.len() {
            mem.dtcs = DTCS_CODES[raw.dtcs_code as usize];
            mem.rx_dtcs = DTCS_CODES[raw.dtcs_code as usize];
        }

        // Tone mode
        if raw.tone_mode != 0 {
            mem.tmode = "Tone".to_string();
        } else if raw.ctcss_mode != 0 {
            mem.tmode = "TSQL".to_string();
        } else if raw.dtcs_mode != 0 {
            mem.tmode = "DTCS".to_string();
        } else if raw.cross_mode != 0 {
            mem.tmode = "Cross".to_string();
        }

        // Skip (lockout)
        if flags.lockout {
            mem.skip = "S".to_string();
        }

        Ok(mem)
    }
}

impl Default for THD75Radio {
    fn default() -> Self {
        Self::new()
    }
}

impl Radio for THD75Radio {
    fn vendor(&self) -> &str {
        &self.vendor
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn get_features(&self) -> RadioFeatures {
        let mut features = RadioFeatures::default();
        features.memory_bounds = (0, NUM_MEMORIES - 1);
        features.valid_modes = THD75_MODES.iter().map(|s| s.to_string()).collect();
        features.valid_tmodes = vec![
            "".to_string(),
            "Tone".to_string(),
            "TSQL".to_string(),
            "DTCS".to_string(),
            "Cross".to_string(),
        ];
        features.valid_duplexes = DUPLEX.iter().map(|s| s.to_string()).collect();
        features.valid_tuning_steps = TUNE_STEPS.to_vec();
        features.valid_tones = TONES.to_vec();
        features.valid_dtcs_codes = DTCS_CODES.to_vec();
        features.valid_name_length = 16;
        features.has_bank = true;
        features.has_dtcs = true;
        features.has_ctone = true;
        features.has_cross = true;
        features
    }

    fn get_memory(&mut self, number: u32) -> RadioResult<Option<Memory>> {
        if number >= NUM_MEMORIES {
            return Err(RadioError::InvalidMemory(number));
        }

        let mmap = self.mmap.as_ref().ok_or(RadioError::Radio(
            "Memory map not loaded".to_string(),
        ))?;

        // Read flags
        let flags_off = self.flags_offset(number);
        let flags_data = mmap
            .get(flags_off, Some(4))
            .map_err(|e| RadioError::Radio(e.to_string()))?;
        let flags = MemoryFlags::from_bytes(flags_data);

        // Check if memory is used
        if flags.used == 0xFF {
            return Ok(None); // Empty memory
        }

        // Read raw memory
        let mem_off = self.memory_offset(number);
        let mem_data = mmap
            .get(mem_off, Some(RawMemory::SIZE))
            .map_err(|e| RadioError::Radio(e.to_string()))?;
        let raw = RawMemory::from_bytes(mem_data)?;

        // Read name
        let name_off = self.name_offset(number);
        let name_data = mmap
            .get(name_off, Some(16))
            .map_err(|e| RadioError::Radio(e.to_string()))?;
        let name = String::from_utf8_lossy(name_data)
            .trim_end_matches('\0')
            .trim()
            .to_string();

        // Decode and return
        let mem = self.decode_memory(number, &raw, &name, &flags)?;
        Ok(Some(mem))
    }

    fn set_memory(&mut self, _memory: &Memory) -> RadioResult<()> {
        // TODO: Implement memory encoding
        Err(RadioError::Unsupported("set_memory not yet implemented".to_string()))
    }
}

impl CloneModeRadio for THD75Radio {
    fn get_memsize(&self) -> usize {
        MEMSIZE
    }

    async fn sync_in(
        &mut self,
        port: &mut SerialPort,
        status_fn: Option<StatusCallback>,
    ) -> RadioResult<MemoryMap> {
        // Detect baud and enter programming mode
        self.detect_baud(port).await?;

        let response = self.command(port, "0M PROGRAM").await?;
        if response != "0M" {
            return Err(RadioError::NoResponse);
        }

        // Switch to high speed (Note: requires port reconfiguration in real implementation)
        // port.set_baud_rate(57600)?;

        // Read one byte (ACK)
        let mut ack = [0u8; 1];
        let _ = port.read(&mut ack).await;

        let num_blocks = MEMSIZE / BLOCK_SIZE;
        let mut data = Vec::with_capacity(MEMSIZE);

        for block in 0..num_blocks {
            let block_data = self.read_block(port, block as u16).await?;
            data.extend_from_slice(&block_data);

            if let Some(ref callback) = status_fn {
                let status = Status::new(
                    block + 1,
                    num_blocks,
                    format!("Downloading block {}/{}", block + 1, num_blocks),
                );
                callback(status.current, status.max, &status.message);
            }
        }

        // End programming mode
        port.write_all(b"E")
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        let mmap = MemoryMap::new(data);
        self.mmap = Some(mmap.clone());
        Ok(mmap)
    }

    async fn sync_out(
        &mut self,
        port: &mut SerialPort,
        mmap: &MemoryMap,
        status_fn: Option<StatusCallback>,
    ) -> RadioResult<()> {
        // Detect baud and enter programming mode
        self.detect_baud(port).await?;

        let response = self.command(port, "0M PROGRAM").await?;
        if response != "0M" {
            return Err(RadioError::NoResponse);
        }

        // Read one byte (ACK)
        let mut ack = [0u8; 1];
        let _ = port.read(&mut ack).await;

        let num_blocks = (MEMSIZE / BLOCK_SIZE) - 2; // Don't write last 2 blocks

        for block in 0..num_blocks {
            let start = block * BLOCK_SIZE;
            let block_data = mmap
                .get(start, Some(BLOCK_SIZE))
                .map_err(|e| RadioError::Radio(e.to_string()))?;

            self.write_block(port, block as u16, block_data).await?;

            if let Some(ref callback) = status_fn {
                let status = Status::new(
                    block + 1,
                    num_blocks,
                    format!("Uploading block {}/{}", block + 1, num_blocks),
                );
                callback(status.current, status.max, &status.message);
            }
        }

        // End programming mode
        port.write_all(b"E")
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        Ok(())
    }

    fn process_mmap(&mut self, mmap: &MemoryMap) -> RadioResult<()> {
        self.mmap = Some(mmap.clone());
        Ok(())
    }

    fn match_model(data: &[u8], filename: &str) -> bool {
        if filename.ends_with(".d74") || filename.ends_with(".d75") {
            return true;
        }
        // Check for file header
        if data.len() >= D74_FILE_HEADER.len() {
            return data[..D74_FILE_HEADER.len()].starts_with(D74_FILE_HEADER);
        }
        false
    }
}

// Register the driver
lazy_static::lazy_static! {
    static ref THD75_REGISTERED: () = {
        crate::drivers::register_driver(
            crate::drivers::DriverInfo::new(
                "Kenwood",
                "TH-D75",
                "Dual-band HT with D-STAR support",
                true,
            )
        );
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thd75_creation() {
        let radio = THD75Radio::new();
        assert_eq!(radio.vendor(), "Kenwood");
        assert_eq!(radio.model(), "TH-D75");
        assert_eq!(radio.get_memsize(), MEMSIZE);
    }

    #[test]
    fn test_thd75_features() {
        let radio = THD75Radio::new();
        let features = radio.get_features();
        assert_eq!(features.memory_bounds, (0, NUM_MEMORIES - 1));
        assert_eq!(features.valid_name_length, 16);
        assert!(features.has_bank);
        assert!(features.has_dtcs);
    }

    #[test]
    fn test_match_model() {
        assert!(THD75Radio::match_model(&[], "test.d74"));
        assert!(THD75Radio::match_model(&[], "test.d75"));
        assert!(!THD75Radio::match_model(&[], "test.img"));
    }

    #[test]
    fn test_memory_offsets() {
        let radio = THD75Radio::new();

        // Test channel 0
        assert_eq!(radio.flags_offset(0), FLAGS_OFFSET);
        assert_eq!(radio.name_offset(0), NAMES_OFFSET);
        assert_eq!(radio.memory_offset(0), MEMORY_OFFSET);

        // Test channel 6 (next group)
        assert_eq!(radio.flags_offset(6), FLAGS_OFFSET + 24);
        assert_eq!(radio.memory_offset(6), MEMORY_OFFSET + (6 * 80 + 16));
    }
}
