// Kenwood TH-D75 / TH-D74 radio driver
// Reference: chirp/drivers/thd74.py

use super::traits::{CloneModeRadio, Radio, RadioError, RadioResult, Status, StatusCallback};
use crate::bitwise::{read_u32_le, write_u32_le};
use crate::core::{Memory, RadioFeatures, DTCS_CODES, TONES};
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

/// Duplex mode enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Duplex {
    Simplex = 0, // 0b00 - split/simplex mode
    Plus = 1,    // 0b01 - positive offset
    Minus = 2,   // 0b10 - negative offset
}

impl Duplex {
    fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0 => Duplex::Simplex,
            1 => Duplex::Plus,
            2 => Duplex::Minus,
            _ => Duplex::Simplex, // 0b11 shouldn't occur, treat as simplex
        }
    }

    fn to_bits(self) -> u8 {
        self as u8
    }

    fn as_str(self) -> &'static str {
        match self {
            Duplex::Simplex => "",
            Duplex::Plus => "+",
            Duplex::Minus => "-",
        }
    }

    fn all() -> &'static [&'static str] {
        &["", "+", "-"]
    }
}

impl std::fmt::Display for Duplex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Tuning steps (kHz)
const TUNE_STEPS: &[f32] = &[
    5.0, 6.25, 8.33, 9.0, 10.0, 12.5, 15.0, 20.0, 25.0, 30.0, 50.0, 100.0,
];

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

/// Raw memory structure (40 bytes at 0x4000)
/// Note: The radio stores memories in 40-byte chunks, not 80 bytes as documented
/// in some sources. The structure contains the essential fields (freq, offset, tones)
/// and D-STAR call signs, with additional data potentially stored elsewhere.
#[derive(Debug, Clone)]
struct RawMemory {
    freq: u32,   // Frequency in Hz
    offset: u32, // Offset in Hz
    tuning_step: u8,
    mode: u8,
    narrow: bool,
    tone_mode: u8,
    ctcss_mode: u8,
    dtcs_mode: u8,
    cross_mode: u8,
    split: bool,
    duplex: Duplex,
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
    /// Size of memory structure in bytes (actual size in radio is 40, not 80)
    const SIZE: usize = 40;

    fn from_bytes(data: &[u8]) -> RadioResult<Self> {
        if data.len() < Self::SIZE {
            return Err(RadioError::Radio(format!(
                "Insufficient data for memory: {} bytes",
                data.len()
            )));
        }

        // Log first 40 bytes of raw memory data for debugging
        tracing::debug!(
            "RawMemory bytes[0..40]: {:02X?}",
            &data[0..40.min(data.len())]
        );

        let freq = read_u32_le(&data[0..4]).unwrap();
        let offset = read_u32_le(&data[4..8]).unwrap();

        let tuning_step = data[8] & 0x0F;
        let mode_bits = (data[9] >> 1) & 0x07;
        let narrow_flag = (data[9] & 0x10) != 0; // Bit 4

        // For TH-D75, the "narrow" flag (bit 4) actually indicates DV mode
        // If set, this is a D-STAR/DV memory; otherwise use mode_bits
        let mode = if narrow_flag { 1 } else { mode_bits }; // 1 = DV
        let narrow = (data[9] & 0x08) != 0; // Actual narrow flag is bit 3

        // Byte 10 bit layout (actual layout differs from some documentation):
        // Bits 0-1: duplex (01='+', 10='-', 00=split/simplex)
        // Bit 2: dtcs_mode
        // Bit 3: cross_mode
        // Bit 5: split
        // Bits 6-7: tone_mode/ctcss_mode flags
        let duplex = Duplex::from_bits(data[10]); // Bits 0-1
        let dtcs_mode = (data[10] >> 2) & 0x01;
        let cross_mode = (data[10] >> 3) & 0x01;
        let split = (data[10] & 0x20) != 0;
        let tone_mode = (data[10] >> 6) & 0x01; // Bit 6
        let ctcss_mode = (data[10] >> 7) & 0x01; // Bit 7

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

        // Byte 10: duplex in bits 0-1, other flags in higher bits
        bytes[10] = self.duplex.to_bits() // Bits 0-1: duplex
            | ((self.dtcs_mode & 0x01) << 2)
            | ((self.cross_mode & 0x01) << 3)
            | (if self.split { 0x20 } else { 0x00 })
            | ((self.tone_mode & 0x01) << 6)
            | ((self.ctcss_mode & 0x01) << 7);

        bytes[11] = self.rtone;
        bytes[12] = self.ctone & 0x3F;
        bytes[13] = self.dtcs_code & 0x7F;
        bytes[14] = self.dig_squelch & 0x03;

        // D-STAR calls (bytes 15-39)
        bytes[15..23].copy_from_slice(&self.dv_urcall);
        bytes[23..31].copy_from_slice(&self.dv_rpt1call);
        bytes[31..39].copy_from_slice(&self.dv_rpt2call);
        bytes[39] = self.dv_code & 0x7F;

        // Note: Total size is 40 bytes. The remaining bytes (if SIZE > 40) are left as zeros.

        bytes
    }
}

/// Kenwood TH-D75 radio driver
pub struct THD75Radio {
    pub mmap: Option<MemoryMap>,
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

    /// Get all non-empty memories from the radio
    pub fn get_memories(&mut self) -> RadioResult<Vec<Memory>> {
        tracing::info!(
            "Decoding {} memory channels from downloaded data",
            NUM_MEMORIES
        );
        let mut memories = Vec::new();

        for channel in 0..NUM_MEMORIES {
            if let Some(mem) = self.get_memory(channel)? {
                if !mem.empty {
                    memories.push(mem);
                }
            }
        }

        tracing::info!(
            "Found {} non-empty memories out of {} channels",
            memories.len(),
            NUM_MEMORIES
        );
        Ok(memories)
    }

    /// Encode all memories to a MemoryMap (reverse of get_memories)
    pub fn encode_memories(&self, memories: &[Memory]) -> RadioResult<MemoryMap> {
        // Create empty memory map filled with 0xFF (empty memory pattern)
        let mut data = vec![0xFFu8; MEMSIZE];

        // Initialize padding regions with 0x00
        for i in 0..0x2000 {
            data[i] = 0x00;
        }

        // Process each memory
        for mem in memories {
            let channel = mem.number;
            if channel >= NUM_MEMORIES {
                return Err(RadioError::InvalidMemory(channel));
            }

            // Encode memory to raw format
            let (raw, flags) = self.encode_memory(mem)?;

            // Write raw memory data
            let mem_off = self.memory_offset(channel);
            let raw_bytes = raw.to_bytes();
            data[mem_off..mem_off + RawMemory::SIZE].copy_from_slice(&raw_bytes);

            // Write flags
            let flags_off = self.flags_offset(channel);
            let flags_bytes = flags.to_bytes();
            data[flags_off..flags_off + 4].copy_from_slice(&flags_bytes);

            // Write name (pad to 16 bytes with zeros)
            let name_off = self.name_offset(channel);
            // Clear name area first
            for i in 0..16 {
                data[name_off + i] = 0x00;
            }
            // Then write name bytes
            let name_bytes = mem.name.as_bytes();
            let name_len = name_bytes.len().min(16);
            data[name_off..name_off + name_len].copy_from_slice(&name_bytes[..name_len]);
        }

        Ok(MemoryMap::new(data))
    }

    /// Calculate memory offset for a given channel number
    fn memory_offset(&self, number: u32) -> usize {
        // Memories are organized in groups of 6, with 16-byte padding after each group
        // Each memory is 40 bytes, not 80 as documented in some older sources
        // Formula: base + (group * (6*40 + 16)) + (index * 40)
        const GROUP_SIZE: u32 = 6;
        let group = (number / GROUP_SIZE) as usize;
        let index = (number % GROUP_SIZE) as usize;
        MEMORY_OFFSET
            + (group * (GROUP_SIZE as usize * RawMemory::SIZE + 16))
            + (index * RawMemory::SIZE)
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
    async fn read_block(&self, port: &mut SerialPort, block: u16) -> RadioResult<Vec<u8>> {
        // Send read command: "R" + block number (big-endian u16) + 0x0000
        let mut cmd = vec![b'R'];
        cmd.extend_from_slice(&block.to_be_bytes());
        cmd.extend_from_slice(&[0x00, 0x00]);

        if block == 0 {
            tracing::debug!("read_block 0 - sending command: {:02X?}", cmd);
        }

        port.write_all(&cmd).await.map_err(|e| {
            tracing::debug!("read_block {} - write failed: {}", block, e);
            RadioError::Serial(e.to_string())
        })?;

        port.flush().await.ok();

        // Read response header: "W" + block number + 0x0000
        if block == 0 {
            tracing::debug!("read_block 0 - waiting for header (5 bytes)");
        }
        let mut header = [0u8; 5];
        port.read_exact(&mut header).await.map_err(|e| {
            tracing::debug!("read_block {} - read_exact header failed: {}", block, e);
            RadioError::Serial(e.to_string())
        })?;

        if block == 0 {
            tracing::debug!("read_block 0 - got header: {:02X?}", header);
        }

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
    async fn write_block(&self, port: &mut SerialPort, block: u16, data: &[u8]) -> RadioResult<()> {
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
    async fn command(&self, port: &mut SerialPort, cmd: &str) -> RadioResult<String> {
        // Clear any stale data before sending command
        port.clear_input().ok();

        let cmd_bytes = format!("{}\r", cmd);
        tracing::debug!("command - sending: {:?}", cmd);
        port.write_all(cmd_bytes.as_bytes())
            .await
            .map_err(|e| RadioError::Serial(e.to_string()))?;

        // Flush to ensure command is sent
        port.flush().await.ok();

        // Read until \r - use a small buffer and read more efficiently
        let mut response = Vec::new();
        let mut buffer = [0u8; 64];
        let start = Instant::now();

        while start.elapsed() < Duration::from_secs(2) {
            match port.read(&mut buffer).await {
                Ok(n) => {
                    if n > 0 {
                        for i in 0..n {
                            response.push(buffer[i]);
                            if buffer[i] == b'\r' {
                                // Found terminator
                                let result = String::from_utf8(response)
                                    .map(|s| s.trim().to_string())
                                    .map_err(|_| {
                                        RadioError::InvalidResponse("Invalid UTF-8".to_string())
                                    })?;
                                tracing::debug!("command - received: {:?}", result);
                                return Ok(result);
                            }
                        }
                    } else {
                        // No data yet, small delay before retry
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        }

        // Timeout - return what we got
        let result = String::from_utf8(response)
            .map(|s| s.trim().to_string())
            .map_err(|_| RadioError::InvalidResponse("Invalid UTF-8".to_string()))?;

        if result.is_empty() {
            Err(RadioError::NoResponse)
        } else {
            tracing::debug!("command - received (incomplete): {:?}", result);
            Ok(result)
        }
    }

    /// Get radio ID
    async fn get_id(&self, port: &mut SerialPort) -> RadioResult<String> {
        tracing::debug!("get_id - sending ID command");

        // Try up to 3 times if we get garbage
        for attempt in 1..=3 {
            let response = self.command(port, "ID").await.map_err(|e| {
                tracing::debug!("get_id - command failed on attempt {}: {}", attempt, e);
                e
            })?;
            tracing::debug!(
                "get_id - got response on attempt {}: {:?}",
                attempt,
                response
            );

            if response.starts_with("ID ") {
                return Ok(response.split_whitespace().nth(1).unwrap_or("").to_string());
            } else if response == "?" && attempt < 3 {
                // Radio confused, clear and retry
                tracing::debug!("get_id - got '?', clearing and retrying");
                port.clear_all().ok();
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        }

        tracing::debug!("get_id - all attempts failed");
        Err(RadioError::NoResponse)
    }

    /// Detect baud rate
    async fn detect_baud(&self, port: &mut SerialPort) -> RadioResult<String> {
        // Note: serialport doesn't support runtime baud rate changes easily
        // For now, we'll assume 9600 is set correctly at port opening
        tracing::debug!("detect_baud - clearing input buffer");
        port.clear_input()
            .map_err(|e| RadioError::Serial(format!("Failed to clear buffer: {}", e)))?;

        tracing::debug!("detect_baud - sending wake-up CRs");
        port.write_all(b"\r\r")
            .await
            .map_err(|e| RadioError::Serial(format!("Failed to send wake-up: {}", e)))?;

        // Wait a bit for radio to wake up
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Clear any pending data
        let mut buf = [0u8; 32];
        let _ = port.read(&mut buf).await;

        tracing::debug!("detect_baud - getting ID");
        self.get_id(port).await
    }

    /// Find tone index from frequency value
    fn find_tone_index(tone: f32) -> RadioResult<u8> {
        TONES
            .iter()
            .position(|&t| (t - tone).abs() < 0.1)
            .map(|idx| idx as u8)
            .ok_or_else(|| RadioError::Radio(format!("Invalid tone: {}", tone)))
    }

    /// Find DTCS code index from code value
    fn find_dtcs_index(dtcs: u16) -> RadioResult<u8> {
        DTCS_CODES
            .iter()
            .position(|&d| d == dtcs)
            .map(|idx| idx as u8)
            .ok_or_else(|| RadioError::Radio(format!("Invalid DTCS code: {}", dtcs)))
    }

    /// Find tuning step index from kHz value
    fn find_tuning_step_index(step: f32) -> RadioResult<u8> {
        TUNE_STEPS
            .iter()
            .position(|&s| (s - step).abs() < 0.01)
            .map(|idx| idx as u8)
            .ok_or_else(|| RadioError::Radio(format!("Invalid tuning step: {}", step)))
    }

    /// Find mode index from mode string
    fn find_mode_index(mode: &str) -> RadioResult<u8> {
        THD75_MODES
            .iter()
            .position(|&m| m == mode)
            .map(|idx| idx as u8)
            .ok_or_else(|| RadioError::Radio(format!("Invalid mode: {}", mode)))
    }

    /// Convert duplex string to Duplex enum
    fn parse_duplex(duplex: &str) -> RadioResult<Duplex> {
        match duplex {
            "" => Ok(Duplex::Simplex),
            "+" => Ok(Duplex::Plus),
            "-" => Ok(Duplex::Minus),
            _ => Err(RadioError::Radio(format!("Invalid duplex: {}", duplex))),
        }
    }

    /// Convert Memory struct to RawMemory (reverse of decode_memory)
    fn encode_memory(&self, mem: &Memory) -> RadioResult<(RawMemory, MemoryFlags)> {
        // Convert frequency and offset (u64 → u32)
        let freq = mem.freq as u32;
        let offset = mem.offset as u32;

        // Find indexes
        let tuning_step = Self::find_tuning_step_index(mem.tuning_step)?;
        let mode = Self::find_mode_index(&mem.mode)?;
        let duplex = Self::parse_duplex(&mem.duplex)?;

        // Determine narrow flag based on mode
        let narrow = mem.mode == "NFM";

        // Handle tones/DTCS
        let rtone = if mem.rtone > 0.0 {
            Self::find_tone_index(mem.rtone)?
        } else {
            0
        };

        let ctone = if mem.ctone > 0.0 {
            Self::find_tone_index(mem.ctone)?
        } else {
            0
        };

        let dtcs_code = if mem.dtcs > 0 {
            Self::find_dtcs_index(mem.dtcs)?
        } else {
            0
        };

        // Parse tone mode flags
        let (tone_mode, ctcss_mode, dtcs_mode, cross_mode) = match mem.tmode.as_str() {
            "Tone" => (1, 0, 0, 0),
            "TSQL" => (0, 1, 0, 0),
            "DTCS" => (0, 0, 1, 0),
            "Cross" => (0, 0, 0, 1),
            _ => (0, 0, 0, 0),
        };

        // Handle D-STAR fields (for DV mode only)
        let mut dv_urcall = [0u8; 8];
        let mut dv_rpt1call = [0u8; 8];
        let mut dv_rpt2call = [0u8; 8];
        let dv_code = mem.dv_code;

        if mode == 1 {
            // DV mode
            // Copy D-STAR call signs, padding with zeros
            let urcall_bytes = mem.dv_urcall.as_bytes();
            let rpt1_bytes = mem.dv_rpt1call.as_bytes();
            let rpt2_bytes = mem.dv_rpt2call.as_bytes();

            dv_urcall[..urcall_bytes.len().min(8)]
                .copy_from_slice(&urcall_bytes[..urcall_bytes.len().min(8)]);
            dv_rpt1call[..rpt1_bytes.len().min(8)]
                .copy_from_slice(&rpt1_bytes[..rpt1_bytes.len().min(8)]);
            dv_rpt2call[..rpt2_bytes.len().min(8)]
                .copy_from_slice(&rpt2_bytes[..rpt2_bytes.len().min(8)]);
        }

        let raw = RawMemory {
            freq,
            offset,
            tuning_step,
            mode,
            narrow,
            tone_mode,
            ctcss_mode,
            dtcs_mode,
            cross_mode,
            split: false, // Not used in basic memories
            duplex,
            rtone,
            ctone,
            dtcs_code,
            dig_squelch: 0, // Default
            dv_urcall,
            dv_rpt1call,
            dv_rpt2call,
            dv_code,
        };

        let flags = MemoryFlags {
            used: 0x00, // 0x00 = used, 0xFF = empty
            lockout: mem.skip == "S",
            group: 0, // Default group
        };

        Ok((raw, flags))
    }

    /// Convert raw memory to Memory struct
    fn decode_memory(
        &self,
        number: u32,
        raw: &RawMemory,
        name: &str,
        flags: &MemoryFlags,
    ) -> RadioResult<Memory> {
        let mut mem = Memory::new(number);

        mem.number = number;
        mem.name = name.to_string();
        mem.freq = raw.freq as u64;
        mem.offset = raw.offset as u64;

        // Populate D-STAR fields if this is a DV memory
        if raw.mode == 1 {
            mem.dv_urcall = String::from_utf8_lossy(&raw.dv_urcall)
                .trim_end_matches('\0')
                .to_string();
            mem.dv_rpt1call = String::from_utf8_lossy(&raw.dv_rpt1call)
                .trim_end_matches('\0')
                .to_string();
            mem.dv_rpt2call = String::from_utf8_lossy(&raw.dv_rpt2call)
                .trim_end_matches('\0')
                .to_string();
            mem.dv_code = raw.dv_code;
        }

        // Mode
        if (raw.mode as usize) < THD75_MODES.len() {
            mem.mode = THD75_MODES[raw.mode as usize].to_string();
        }

        // Duplex
        mem.duplex = raw.duplex.as_str().to_string();

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

        // Tone mode (but not for DV/D-STAR mode - those use digital squelch instead)
        // Mode 1 = DV/D-STAR, skip tone settings for those
        if raw.mode != 1 {
            if raw.tone_mode != 0 {
                mem.tmode = "Tone".to_string();
            } else if raw.ctcss_mode != 0 {
                mem.tmode = "TSQL".to_string();
            } else if raw.dtcs_mode != 0 {
                mem.tmode = "DTCS".to_string();
            } else if raw.cross_mode != 0 {
                mem.tmode = "Cross".to_string();
            }
        }

        // Skip (lockout)
        if flags.lockout {
            mem.skip = "S".to_string();
        }

        // Log the decoded memory for debugging
        tracing::debug!(
            "Decoded Memory #{}: name=\"{}\" freq={} offset={} mode=\"{}\" duplex=\"{}\" tmode=\"{}\" rtone={} ctone={} dtcs={} skip=\"{}\" tuning_step={}",
            mem.number,
            mem.name,
            mem.freq,
            mem.offset,
            mem.mode,
            mem.duplex,
            mem.tmode,
            mem.rtone,
            mem.ctone,
            mem.dtcs,
            mem.skip,
            mem.tuning_step
        );

        // Also log the raw memory data for comparison
        tracing::debug!(
            "  Raw data: freq={} offset={} mode={} duplex={} rtone={} ctone={} dtcs_code={} tone_mode={} ctcss_mode={} dtcs_mode={} cross_mode={} tuning_step={}",
            raw.freq,
            raw.offset,
            raw.mode,
            raw.duplex,
            raw.rtone,
            raw.ctone,
            raw.dtcs_code,
            raw.tone_mode,
            raw.ctcss_mode,
            raw.dtcs_mode,
            raw.cross_mode,
            raw.tuning_step
        );

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
        features.valid_duplexes = Duplex::all().iter().map(|s| s.to_string()).collect();
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

        let mmap = self
            .mmap
            .as_ref()
            .ok_or(RadioError::Radio("Memory map not loaded".to_string()))?;

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
        tracing::debug!(
            "Reading memory #{} from offset 0x{:04X} (decimal {})",
            number,
            mem_off,
            mem_off
        );
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

    fn set_memory(&mut self, memory: &Memory) -> RadioResult<()> {
        // Encode memory first (before borrowing mmap)
        let (raw, flags) = self.encode_memory(memory)?;

        // Calculate offsets
        let mem_off = self.memory_offset(memory.number);
        let flags_off = self.flags_offset(memory.number);
        let name_off = self.name_offset(memory.number);

        // Prepare data
        let raw_bytes = raw.to_bytes();
        let flags_bytes = flags.to_bytes();
        let mut name_bytes = [0u8; 16];
        let name_slice = memory.name.as_bytes();
        let name_len = name_slice.len().min(16);
        name_bytes[..name_len].copy_from_slice(&name_slice[..name_len]);

        // Now borrow mmap mutably and write
        let mmap = self
            .mmap
            .as_mut()
            .ok_or(RadioError::Radio("Memory map not loaded".to_string()))?;

        mmap.set_bytes(mem_off, &raw_bytes)
            .map_err(|e| RadioError::Radio(e.to_string()))?;
        mmap.set_bytes(flags_off, &flags_bytes)
            .map_err(|e| RadioError::Radio(e.to_string()))?;
        mmap.set_bytes(name_off, &name_bytes)
            .map_err(|e| RadioError::Radio(e.to_string()))?;

        Ok(())
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

        tracing::debug!("Entering programming mode");
        let response = self.command(port, "0M PROGRAM").await?;
        tracing::debug!("Got response: {:?}", response);
        if response != "0M" {
            return Err(RadioError::NoResponse);
        }

        // Radio is now in programming mode and expecting us to switch to high speed
        // DO NOT read anything else - immediately switch baud rates
        tracing::debug!("Switching to 57600 baud immediately");

        port.set_baud_rate(57600)
            .map_err(|e| RadioError::Serial(format!("Failed to change baud rate: {}", e)))?;

        // Brief pause for both PC and radio to stabilize at new baud rate
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Clear buffers to start clean communication at new baud rate
        tracing::debug!("Clearing buffers at 57600 baud");
        port.clear_all().ok();

        let num_blocks = MEMSIZE / BLOCK_SIZE;
        let mut data = Vec::with_capacity(MEMSIZE);

        tracing::debug!("Starting block download ({} blocks)", num_blocks);
        for block in 0..num_blocks {
            if block % 100 == 0 {
                tracing::debug!("Reading block {}/{}", block, num_blocks);
            }
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
        tracing::debug!("Block download complete");

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

        // Test channel 6 (next group) - groups of 6 with 40-byte memories + 16-byte padding
        assert_eq!(radio.flags_offset(6), FLAGS_OFFSET + 24);
        assert_eq!(radio.memory_offset(6), MEMORY_OFFSET + (6 * 40 + 16));

        // Test memory within a group
        assert_eq!(radio.memory_offset(1), MEMORY_OFFSET + 40);
        assert_eq!(radio.memory_offset(5), MEMORY_OFFSET + (5 * 40));

        // Test memory 32 (previously problematic)
        // Group 5 (32/6=5), index 2 (32%6=2)
        // Offset = 0x4000 + (5 * (6*40 + 16)) + (2 * 40) = 0x4000 + (5 * 256) + 80 = 0x4550
        assert_eq!(radio.memory_offset(32), 0x4550);

        // Test memory 40 (previously problematic)
        // Group 6 (40/6=6), index 4 (40%6=4)
        // Offset = 0x4000 + (6 * 256) + (4 * 40) = 0x4000 + 1536 + 160 = 0x46A0
        assert_eq!(radio.memory_offset(40), 0x46A0);
    }

    #[test]
    fn test_parse_real_memories() {
        // Load the actual radio dump for testing
        let dump_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data/radio_dump.bin");

        // Skip test if dump file doesn't exist (e.g., in CI)
        if !dump_path.exists() {
            eprintln!("Skipping test: radio_dump.bin not found");
            return;
        }

        let data = std::fs::read(&dump_path).expect("Failed to read radio_dump.bin");
        let mmap = crate::memmap::MemoryMap::new(data);
        let mut radio = THD75Radio::new();
        radio
            .process_mmap(&mmap)
            .expect("Failed to process memory map");

        // Test memory #0 - APRS
        let mem0 = radio.get_memory(0).expect("Failed to get memory 0");
        assert!(mem0.is_some());
        let mem0 = mem0.unwrap();
        assert_eq!(mem0.number, 0);
        assert_eq!(mem0.name, "APRS");
        assert_eq!(mem0.freq, 144_390_000);
        assert_eq!(mem0.offset, 600_000);
        assert_eq!(mem0.mode, "FM");

        // Test memory #3 - PhilMont W3QV (Tone mode)
        let mem3 = radio.get_memory(3).expect("Failed to get memory 3");
        assert!(mem3.is_some());
        let mem3 = mem3.unwrap();
        assert_eq!(mem3.number, 3);
        assert_eq!(mem3.name, "PhilMont W3QV");
        assert_eq!(mem3.freq, 147_030_000);
        assert_eq!(mem3.offset, 600_000);
        assert_eq!(mem3.duplex, "+");
        assert_eq!(mem3.tmode, "Tone");
        assert_eq!(mem3.rtone, 88.5);
        assert_eq!(mem3.ctone, 91.5);

        // Test memory #32 - N3CB (previously problematic)
        let mem32 = radio.get_memory(32).expect("Failed to get memory 32");
        assert!(mem32.is_some());
        let mem32 = mem32.unwrap();
        assert_eq!(mem32.number, 32);
        assert_eq!(mem32.name, "N3CB");
        assert_eq!(mem32.freq, 448_675_000);
        assert_eq!(mem32.offset, 5_000_000);
        assert_eq!(mem32.mode, "FM");
        assert_eq!(mem32.duplex, "-");

        // Test memory #40 - W3EOC (previously problematic)
        let mem40 = radio.get_memory(40).expect("Failed to get memory 40");
        assert!(mem40.is_some());
        let mem40 = mem40.unwrap();
        assert_eq!(mem40.number, 40);
        assert_eq!(mem40.name, "W3EOC");
        assert_eq!(mem40.freq, 441_950_000);
        assert_eq!(mem40.offset, 5_000_000);
        assert_eq!(mem40.duplex, "+");
        assert_eq!(mem40.tmode, "Tone");
        assert_eq!(mem40.ctone, 100.0);

        // Test memory #50 - KB3AJF
        let mem50 = radio.get_memory(50).expect("Failed to get memory 50");
        assert!(mem50.is_some());
        let mem50 = mem50.unwrap();
        assert_eq!(mem50.number, 50);
        assert_eq!(mem50.name, "KB3AJF");
        assert_eq!(mem50.freq, 447_975_000);
        assert_eq!(mem50.offset, 5_000_000);
        assert_eq!(mem50.duplex, "-");

        // Test that we find the expected number of non-empty memories
        let memories = radio.get_memories().expect("Failed to get memories");
        assert_eq!(memories.len(), 91, "Expected 91 non-empty memories");
    }

    #[test]
    fn test_empty_memory() {
        let dump_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data/radio_dump.bin");

        if !dump_path.exists() {
            eprintln!("Skipping test: radio_dump.bin not found");
            return;
        }

        let data = std::fs::read(&dump_path).expect("Failed to read radio_dump.bin");
        let mmap = crate::memmap::MemoryMap::new(data);
        let mut radio = THD75Radio::new();
        radio
            .process_mmap(&mmap)
            .expect("Failed to process memory map");

        // Test that empty memories return None
        // Memory #63 should be empty (based on the CSV data)
        let mem63 = radio.get_memory(63).expect("Failed to get memory 63");
        assert!(mem63.is_none(), "Memory #63 should be empty");
    }

    #[test]
    fn test_dv_memories() {
        let dump_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data/radio_dump.bin");

        if !dump_path.exists() {
            eprintln!("Skipping test: radio_dump.bin not found");
            return;
        }

        let data = std::fs::read(&dump_path).expect("Failed to read radio_dump.bin");
        let mmap = crate::memmap::MemoryMap::new(data);
        let mut radio = THD75Radio::new();
        radio
            .process_mmap(&mmap)
            .expect("Failed to process memory map");

        // Test memory #1 - Eaglevi CQCQCQ (DV memory with empty D-STAR fields)
        let mem1 = radio.get_memory(1).expect("Failed to get memory 1");
        assert!(mem1.is_some());
        let mem1 = mem1.unwrap();
        assert_eq!(mem1.number, 1);
        assert_eq!(mem1.name, "Eaglevi CQCQCQ");
        assert_eq!(mem1.freq, 445_018_750);
        assert_eq!(mem1.offset, 5_000_000);
        assert_eq!(mem1.mode, "DV");
        assert_eq!(mem1.duplex, "-");
        // D-STAR fields are empty for this memory
        assert_eq!(mem1.dv_urcall, "");
        assert_eq!(mem1.dv_rpt1call, "");
        assert_eq!(mem1.dv_rpt2call, "");
        assert_eq!(mem1.dv_code, 0);
        // Verify that DV memories have no tone mode
        assert_eq!(mem1.tmode, "");

        // Test memory #102 - dmr clear (DV memory with populated D-STAR fields)
        let mem102 = radio.get_memory(102).expect("Failed to get memory 102");
        assert!(mem102.is_some());
        let mem102 = mem102.unwrap();
        assert_eq!(mem102.number, 102);
        assert_eq!(mem102.name, "dmr clear");
        assert_eq!(mem102.freq, 438_287_500);
        assert_eq!(mem102.mode, "DV");
        // D-STAR fields should be populated (CHIRP CSV export bug didn't include these)
        assert_eq!(mem102.dv_urcall, "4000");
        assert_eq!(mem102.dv_rpt1call, "W3POG");
        // RPT2CALL might be empty or populated - just verify it's been parsed
        assert_eq!(mem102.tmode, "");
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        // Load real radio dump
        let dump_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data/radio_dump.bin");

        if !dump_path.exists() {
            eprintln!("Skipping test: radio_dump.bin not found");
            return;
        }

        let data = std::fs::read(&dump_path).expect("Failed to read radio_dump.bin");
        let mmap = MemoryMap::new(data);
        let mut radio = THD75Radio::new();
        radio
            .process_mmap(&mmap)
            .expect("Failed to process memory map");

        // Decode memory #3
        let mem3 = radio
            .get_memory(3)
            .expect("Failed to get memory 3")
            .unwrap();

        // Encode it back
        let (raw, flags) = radio.encode_memory(&mem3).expect("Failed to encode memory");

        // Verify critical fields match original
        assert_eq!(raw.freq, 147_030_000);
        assert_eq!(raw.offset, 600_000);
        assert_eq!(raw.duplex, Duplex::Plus);
        assert_eq!(flags.used, 0x00);
        assert_eq!(raw.tone_mode, 1); // Tone mode
        assert_eq!(raw.rtone, 8); // 88.5 Hz is index 8
    }

    #[test]
    fn test_encode_memories_full() {
        let dump_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data/radio_dump.bin");

        if !dump_path.exists() {
            eprintln!("Skipping test: radio_dump.bin not found");
            return;
        }

        let data = std::fs::read(&dump_path).expect("Failed to read radio_dump.bin");
        let mmap = MemoryMap::new(data);
        let mut radio = THD75Radio::new();
        radio
            .process_mmap(&mmap)
            .expect("Failed to process memory map");

        // Get all memories
        let memories = radio.get_memories().expect("Failed to get memories");

        // Encode them back
        let encoded_mmap = radio
            .encode_memories(&memories)
            .expect("Failed to encode memories");

        // Decode again
        let mut radio2 = THD75Radio::new();
        radio2
            .process_mmap(&encoded_mmap)
            .expect("Failed to process encoded memory map");
        let memories2 = radio2
            .get_memories()
            .expect("Failed to get memories from encoded map");

        // Should have same count
        assert_eq!(memories.len(), memories2.len());

        // Spot check a few memories
        let mem0 = &memories[0];
        let mem0_2 = memories2.iter().find(|m| m.number == 0).unwrap();
        assert_eq!(mem0.freq, mem0_2.freq);
        assert_eq!(mem0.name, mem0_2.name);
        assert_eq!(mem0.mode, mem0_2.mode);

        // Check memory #3 in detail
        let mem3 = memories.iter().find(|m| m.number == 3).unwrap();
        let mem3_2 = memories2.iter().find(|m| m.number == 3).unwrap();
        assert_eq!(mem3.freq, mem3_2.freq);
        assert_eq!(mem3.offset, mem3_2.offset);
        assert_eq!(mem3.duplex, mem3_2.duplex);
        assert_eq!(mem3.tmode, mem3_2.tmode);
        assert_eq!(mem3.rtone, mem3_2.rtone);
    }

    #[test]
    fn test_encode_dv_memory() {
        // Test encoding a D-STAR memory with call signs
        let mut mem = Memory::new(100);
        mem.freq = 438_287_500;
        mem.mode = "DV".to_string();
        mem.duplex = "-".to_string();
        mem.offset = 5_000_000;
        mem.dv_urcall = "4000".to_string();
        mem.dv_rpt1call = "W3POG".to_string();
        mem.dv_rpt2call = "".to_string();
        mem.dv_code = 0;

        let radio = THD75Radio::new();
        let (raw, _) = radio
            .encode_memory(&mem)
            .expect("Failed to encode DV memory");

        assert_eq!(raw.mode, 1); // DV mode
        assert_eq!(
            String::from_utf8_lossy(&raw.dv_urcall).trim_end_matches('\0'),
            "4000"
        );
        assert_eq!(
            String::from_utf8_lossy(&raw.dv_rpt1call).trim_end_matches('\0'),
            "W3POG"
        );
    }

    #[test]
    fn test_helper_functions() {
        // Test tone lookup
        assert_eq!(THD75Radio::find_tone_index(88.5).unwrap(), 8);
        assert_eq!(THD75Radio::find_tone_index(100.0).unwrap(), 12);
        assert!(THD75Radio::find_tone_index(999.9).is_err());

        // Test DTCS lookup
        assert_eq!(THD75Radio::find_dtcs_index(23).unwrap(), 0);
        assert_eq!(THD75Radio::find_dtcs_index(754).unwrap(), 103);
        assert!(THD75Radio::find_dtcs_index(9999).is_err());

        // Test tuning step lookup
        assert_eq!(THD75Radio::find_tuning_step_index(5.0).unwrap(), 0);
        assert_eq!(THD75Radio::find_tuning_step_index(12.5).unwrap(), 5);
        assert!(THD75Radio::find_tuning_step_index(7.5).is_err());

        // Test mode lookup
        assert_eq!(THD75Radio::find_mode_index("FM").unwrap(), 0);
        assert_eq!(THD75Radio::find_mode_index("DV").unwrap(), 1);
        assert_eq!(THD75Radio::find_mode_index("NFM").unwrap(), 6);
        assert!(THD75Radio::find_mode_index("INVALID").is_err());

        // Test duplex parsing
        assert_eq!(THD75Radio::parse_duplex("").unwrap(), Duplex::Simplex);
        assert_eq!(THD75Radio::parse_duplex("+").unwrap(), Duplex::Plus);
        assert_eq!(THD75Radio::parse_duplex("-").unwrap(), Duplex::Minus);
        assert!(THD75Radio::parse_duplex("invalid").is_err());
    }
}
