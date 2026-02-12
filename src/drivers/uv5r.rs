// Baofeng UV-5R radio driver
// Reference: chirp/drivers/uv5r.py

use super::traits::{CloneModeRadio, Radio, RadioError, RadioResult, StatusCallback};
use crate::bitwise::{bcd_to_int, int_to_bcd};
use crate::core::{Memory, PowerLevel, RadioFeatures, DTCS_CODES, TONES};
use crate::memmap::MemoryMap;
use crate::serial::SerialPort;
use std::time::Duration;
use tokio::time::timeout;

/// UV-5R memory map size: 6152 bytes (base) up to 8192 bytes with aux block
const MEMSIZE: usize = 0x1808; // 6152 bytes

/// Number of memory channels
const NUM_MEMORIES: u32 = 128;

/// Memory block base address
const MEMORY_BASE: usize = 0x0008;

/// Memory size (16 bytes per channel)
const MEMORY_SIZE: usize = 16;

/// Names block base address (128 names × 16 bytes, only first 7 chars used)
const NAME_BASE: usize = 0x1008;

/// Name storage size (16 bytes per name, only first 7 used)
const NAME_SIZE: usize = 16;

/// Block size for reading (64 bytes)
const BLOCK_SIZE: usize = 0x40;

/// Block size for writing (16 bytes)
const WRITE_BLOCK_SIZE: usize = 0x10;

/// Model identification magic bytes (UV-5R variant 291)
const UV5R_MODEL_291: &[u8] = b"\x50\xBB\xFF\x20\x12\x07\x25";

/// Model identification magic bytes (original UV-5R)
const UV5R_MODEL_ORIG: &[u8] = b"\x50\xBB\xFF\x01\x25\x98\x4D";

/// CTCSS tone encoding threshold (values >= this are CTCSS, < this are DTCS)
const TONE_CTCSS_THRESHOLD: u16 = 0x0258;

/// Power levels: High (4W), Low (1W)
const POWER_LEVELS: &[(&str, f32)] = &[("High", 4.0), ("Low", 1.0)];

/// Valid character set for channel names
const UV5R_CHARSET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()+-=[]:\";'<>?,./";

/// Memory skip ranges during upload (these ranges should not be written)
/// (start, end) pairs
const UPLOAD_SKIP_RANGES: &[(usize, usize)] = &[
    (0x0CF8, 0x0D08), // Skip range 1
    (0x0DF8, 0x0E08), // Skip range 2
];

/// Raw memory structure (16 bytes per channel)
///
/// Byte layout:
/// - Bytes 0-3:   rxfreq (4-byte little-endian BCD)
/// - Bytes 4-7:   txfreq (4-byte little-endian BCD) or 0xFFFFFFFF if TX inhibited
/// - Bytes 8-9:   rxtone (u16 little-endian): 0=off, >= 0x0258=CTCSS (value/10.0), <= 0x0258=DTCS
/// - Bytes 10-11: txtone (u16 little-endian): Same encoding as rxtone
/// - Byte 12:     Bitfield 1 (isuhf:1, scode:4, unused1:3)
/// - Byte 13:     Bitfield 2 (txtoneicon:1, unknown1:7)
/// - Byte 14:     Bitfield 3 (lowpower:2, mailicon:3, unknown2:3)
/// - Byte 15:     Bitfield 4 (pttid:2, scan:1, bcl:1, unknown4:2, wide:1, unknown3:1)
#[derive(Debug, Clone)]
struct RawMemory {
    rxfreq: u32,  // BCD encoded frequency (divide by 10 to get Hz)
    txfreq: u32,  // BCD encoded frequency or 0xFFFFFFFF
    rxtone: u16,  // 0, CTCSS (>=0x258), or DTCS index
    txtone: u16,  // Same as rxtone
    isuhf: bool,  // Band indicator (VHF=false, UHF=true)
    scode: u8,    // PTT ID code (0-15)
    lowpower: u8, // Power level (0=High, 1=Low, 2=Mid on tri-power variants, stored in bits 0-1 of byte 14)
    wide: bool,   // Bandwidth: true=FM (25kHz), false=NFM (12.5kHz)
    bcl: bool,    // Busy channel lockout
    scan: bool,   // Scan enable
    pttid: u8,    // PTT ID setting (0-3, stored in bits 0-1 of byte 15)
}

impl RawMemory {
    const SIZE: usize = 16;

    /// Parse raw memory from 16-byte slice
    fn from_bytes(data: &[u8]) -> RadioResult<Self> {
        if data.len() < Self::SIZE {
            return Err(RadioError::Radio(format!(
                "Insufficient data for memory: {} bytes",
                data.len()
            )));
        }

        // Parse BCD frequencies (little-endian u32)
        let rxfreq = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let txfreq = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

        // Parse tone values (little-endian u16)
        let rxtone = u16::from_le_bytes([data[8], data[9]]);
        let txtone = u16::from_le_bytes([data[10], data[11]]);

        // Byte 12: unused1 (bits 0-2), isuhf (bit 3), scode (bits 4-7)
        let isuhf = (data[12] & 0x08) != 0; // bit 3
        let scode = (data[12] >> 4) & 0x0F; // bits 4-7

        // Byte 14: lowpower (bits 0-1)
        let lowpower = data[14] & 0x03;

        // Byte 15: pttid (bits 0-1), scan (bit 2), bcl (bit 3), wide (bit 6)
        let pttid = data[15] & 0x03;
        let scan = (data[15] & 0x04) != 0;
        let bcl = (data[15] & 0x08) != 0;
        let wide = (data[15] & 0x40) != 0;

        Ok(Self {
            rxfreq,
            txfreq,
            rxtone,
            txtone,
            isuhf,
            scode,
            lowpower,
            wide,
            bcl,
            scan,
            pttid,
        })
    }

    /// Encode raw memory to 16-byte array
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; Self::SIZE];

        // Frequencies (little-endian u32)
        bytes[0..4].copy_from_slice(&self.rxfreq.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.txfreq.to_le_bytes());

        // Tones (little-endian u16)
        bytes[8..10].copy_from_slice(&self.rxtone.to_le_bytes());
        bytes[10..12].copy_from_slice(&self.txtone.to_le_bytes());

        // Byte 12: unused1 (bits 0-2), isuhf (bit 3), scode (bits 4-7)
        bytes[12] = (if self.isuhf { 0x08 } else { 0x00 }) | ((self.scode & 0x0F) << 4);

        // Byte 13: txtoneicon (not used for now)
        bytes[13] = 0x00;

        // Byte 14: lowpower (bits 0-1)
        bytes[14] = self.lowpower & 0x03;

        // Byte 15: pttid (bits 0-1), scan (bit 2), bcl (bit 3), wide (bit 6)
        bytes[15] = (self.pttid & 0x03)
            | (if self.scan { 0x04 } else { 0x00 })
            | (if self.bcl { 0x08 } else { 0x00 })
            | (if self.wide { 0x40 } else { 0x00 });

        bytes
    }

    /// Check if this memory is empty (frequency == 0xFFFFFFFF)
    fn is_empty(&self) -> bool {
        self.rxfreq == 0xFFFFFFFF
    }
}

/// BCD frequency conversion helpers
///
/// UV-5R stores frequencies as BCD (Binary-Coded Decimal) where each nibble
/// represents a decimal digit. The frequency in Hz is divided by 10 before
/// encoding.
///
/// # Example
/// ```text
/// 146.520 MHz = 146520000 Hz
///   → 14652000 (divide by 10)
///   → BCD encoding: 0x01 0x46 0x52 0x00 (little-endian)
/// ```

/// Convert BCD-encoded u32 to frequency in Hz
fn bcd_to_freq(bcd: u32) -> u64 {
    // Convert BCD to integer (treats u32 as 4-byte BCD array)
    let bytes = bcd.to_le_bytes();

    // Use existing BCD utilities (little-endian = true)
    match bcd_to_int(&bytes, true) {
        Ok(value) => value * 10, // Multiply by 10 to get Hz
        Err(e) => {
            tracing::warn!("Invalid BCD frequency {:08X}: {}", bcd, e);
            0
        }
    }
}

/// Convert frequency in Hz to BCD-encoded u32
fn freq_to_bcd(freq: u64) -> u32 {
    // Divide by 10 to get the value to encode
    let value = freq / 10;

    // Convert to BCD (4 bytes, little-endian)
    match int_to_bcd(value, 4, true) {
        Ok(bytes) => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        Err(e) => {
            tracing::warn!("Failed to encode frequency {}: {}", freq, e);
            0xFFFFFFFF
        }
    }
}

/// Validate BCD encoding (each nibble must be 0-9)
fn is_valid_bcd(bcd: u32) -> bool {
    let bytes = bcd.to_le_bytes();
    for &byte in &bytes {
        let hi = (byte >> 4) & 0x0F;
        let lo = byte & 0x0F;
        if hi > 9 || lo > 9 {
            return false;
        }
    }
    true
}

/// Tone encoding helpers
///
/// Convert CTCSS frequency (Hz) to u16 encoding
/// The radio stores CTCSS tones as frequency * 10
/// Example: 88.5 Hz → 885 → 0x0375
fn tone_to_u16(tone: f32) -> u16 {
    (tone * 10.0) as u16
}

/// Decode u16 to CTCSS frequency (Hz)
/// Returns None if value is not a valid CTCSS tone (< threshold)
fn u16_to_tone(value: u16) -> Option<f32> {
    if value >= TONE_CTCSS_THRESHOLD {
        Some(value as f32 / 10.0)
    } else {
        None
    }
}

/// Convert DTCS code and polarity to u16 encoding
/// DTCS codes are stored as index into DTCS_CODES array
/// - Normal polarity: index + 1
/// - Reversed polarity: index + 1 + 0x69
fn dtcs_to_u16(code: u16, polarity: char) -> RadioResult<u16> {
    // Find index in DTCS_CODES
    let index = DTCS_CODES
        .iter()
        .position(|&c| c == code)
        .ok_or_else(|| RadioError::Radio(format!("Invalid DTCS code: {}", code)))?;

    let encoded = (index + 1) as u16;

    if polarity == 'R' {
        Ok(encoded + 0x69)
    } else {
        Ok(encoded)
    }
}

/// Decode u16 to DTCS code and polarity
/// Returns (code, polarity) where polarity is 'N' (normal) or 'R' (reversed)
fn u16_to_dtcs(value: u16) -> RadioResult<(u16, char)> {
    if value == 0 || value >= TONE_CTCSS_THRESHOLD {
        return Err(RadioError::Radio(format!(
            "Invalid DTCS encoding: {}",
            value
        )));
    }

    let (index, polarity) = if value > 0x69 {
        ((value - 0x6A) as usize, 'R')
    } else {
        ((value - 1) as usize, 'N')
    };

    if index >= DTCS_CODES.len() {
        return Err(RadioError::Radio(format!(
            "DTCS index out of range: {}",
            index
        )));
    }

    Ok((DTCS_CODES[index], polarity))
}

/// UV-5R Radio Driver
pub struct UV5RRadio {
    pub mmap: Option<MemoryMap>,
    vendor: String,
    model: String,
}

impl UV5RRadio {
    /// Create a new UV-5R radio instance
    pub fn new() -> Self {
        Self {
            mmap: None,
            vendor: "Baofeng".to_string(),
            model: "UV-5R".to_string(),
        }
    }

    /// Calculate memory offset for a given channel number
    fn memory_offset(&self, number: u32) -> usize {
        MEMORY_BASE + (number as usize * MEMORY_SIZE)
    }

    /// Calculate name offset for a given channel number
    fn name_offset(&self, number: u32) -> usize {
        NAME_BASE + (number as usize * NAME_SIZE)
    }

    /// Read raw memory from memory map
    fn read_raw_memory(&self, number: u32) -> RadioResult<RawMemory> {
        let mmap = self
            .mmap
            .as_ref()
            .ok_or(RadioError::Radio("No memory map loaded".to_string()))?;

        let offset = self.memory_offset(number);
        let data = mmap.get(offset, Some(MEMORY_SIZE)).map_err(|e| {
            RadioError::Radio(format!("Failed to read memory at offset {}: {}", offset, e))
        })?;
        RawMemory::from_bytes(data)
    }

    /// Write raw memory to memory map
    fn write_raw_memory(&mut self, number: u32, raw: &RawMemory) -> RadioResult<()> {
        let offset = self.memory_offset(number);
        let data = raw.to_bytes();

        let mmap = self
            .mmap
            .as_mut()
            .ok_or(RadioError::Radio("No memory map loaded".to_string()))?;

        mmap.set_bytes(offset, &data).map_err(|e| {
            RadioError::Radio(format!(
                "Failed to write memory at offset {}: {}",
                offset, e
            ))
        })?;
        Ok(())
    }

    /// Read channel name from memory map
    fn read_name(&self, number: u32) -> RadioResult<String> {
        let mmap = self
            .mmap
            .as_ref()
            .ok_or(RadioError::Radio("No memory map loaded".to_string()))?;

        let offset = self.name_offset(number);
        let data = mmap.get(offset, Some(7)).map_err(|e| {
            RadioError::Radio(format!("Failed to read name at offset {}: {}", offset, e))
        })?; // Only first 7 bytes

        // Convert to string, handling 0xFF padding
        let name = data
            .iter()
            .take_while(|&&b| b != 0xFF && b != 0x00)
            .map(|&b| {
                // Convert to char, replace invalid characters with space
                if UV5R_CHARSET.contains(b as char) {
                    b as char
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .trim_end()
            .to_string();

        Ok(name)
    }

    /// Write channel name to memory map
    fn write_name(&mut self, number: u32, name: &str) -> RadioResult<()> {
        let offset = self.name_offset(number);
        let mut name_bytes = [0xFFu8; 7];

        // Copy name characters (up to 7, converting to uppercase)
        for (i, c) in name.chars().take(7).enumerate() {
            name_bytes[i] = c.to_ascii_uppercase() as u8;
        }

        let mmap = self
            .mmap
            .as_mut()
            .ok_or(RadioError::Radio("No memory map loaded".to_string()))?;

        mmap.set_bytes(offset, &name_bytes).map_err(|e| {
            RadioError::Radio(format!("Failed to write name at offset {}: {}", offset, e))
        })?;
        Ok(())
    }
}

impl Default for UV5RRadio {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode raw memory structure to Memory
fn decode_memory(number: u32, raw: &RawMemory, name: &str) -> RadioResult<Memory> {
    let mut mem = Memory::new(number);

    // Decode frequency
    mem.freq = bcd_to_freq(raw.rxfreq);

    // Decode duplex and offset
    if raw.txfreq == 0xFFFFFFFF {
        // TX inhibited
        mem.duplex = "off".to_string();
        mem.offset = 0;
    } else {
        let tx_freq = bcd_to_freq(raw.txfreq);

        if tx_freq == mem.freq {
            // Simplex
            mem.duplex = String::new();
            mem.offset = 0;
        } else {
            // Calculate offset
            let diff = tx_freq.abs_diff(mem.freq);

            // Check if this is split (large frequency difference)
            if diff > 70_000_000 {
                // Split operation (TX freq stored directly)
                mem.duplex = "split".to_string();
                mem.offset = tx_freq;
            } else if tx_freq > mem.freq {
                // Positive offset
                mem.duplex = "+".to_string();
                mem.offset = diff;
            } else {
                // Negative offset
                mem.duplex = "-".to_string();
                mem.offset = diff;
            }
        }
    }

    // Mode: FM or NFM based on wide flag
    mem.mode = if raw.wide {
        "FM".to_string()
    } else {
        "NFM".to_string()
    };

    // Power level (lowpower: 0=High, 1=Low, 2=Mid on tri-power variants)
    // Standard UV-5R uses: 0=High (4W), 1=Low (1W)
    let power_index = match raw.lowpower {
        0 => 0, // High
        1 => 1, // Low
        2 => 1, // Mid (treat as Low on standard UV-5R, only exists on tri-power variants)
        _ => 0, // Invalid, default to High
    };
    mem.power = Some(PowerLevel::from_watts(
        POWER_LEVELS[power_index].0,
        POWER_LEVELS[power_index].1,
    ));

    // Decode tone modes
    decode_tone_mode(raw, &mut mem)?;

    // Skip flag (scan enabled = don't skip, scan disabled = skip)
    mem.skip = if raw.scan {
        String::new()
    } else {
        "S".to_string()
    };

    // Name
    mem.name = name.to_string();

    Ok(mem)
}

/// Decode tone mode from RawMemory into Memory
fn decode_tone_mode(raw: &RawMemory, mem: &mut Memory) -> RadioResult<()> {
    // Decode TX tone
    let tx_tone_type = if raw.txtone == 0 || raw.txtone == 0xFFFF {
        ToneType::None
    } else if raw.txtone >= TONE_CTCSS_THRESHOLD {
        ToneType::Ctcss(u16_to_tone(raw.txtone).unwrap())
    } else {
        let (code, polarity) = u16_to_dtcs(raw.txtone)?;
        ToneType::Dtcs(code, polarity)
    };

    // Decode RX tone
    let rx_tone_type = if raw.rxtone == 0 || raw.rxtone == 0xFFFF {
        ToneType::None
    } else if raw.rxtone >= TONE_CTCSS_THRESHOLD {
        ToneType::Ctcss(u16_to_tone(raw.rxtone).unwrap())
    } else {
        let (code, polarity) = u16_to_dtcs(raw.rxtone)?;
        ToneType::Dtcs(code, polarity)
    };

    // Determine tone mode based on TX and RX tones
    match (&tx_tone_type, &rx_tone_type) {
        (ToneType::None, ToneType::None) => {
            // No tones
            mem.tmode = String::new();
        }
        (ToneType::Ctcss(freq), ToneType::None) => {
            // TX CTCSS only (Tone mode)
            mem.tmode = "Tone".to_string();
            mem.rtone = *freq;
        }
        (ToneType::Ctcss(freq), ToneType::Ctcss(_)) => {
            // TX and RX CTCSS (TSQL mode)
            mem.tmode = "TSQL".to_string();
            mem.rtone = *freq;
            mem.ctone = *freq; // In TSQL, both use same tone
        }
        (ToneType::Dtcs(code, pol), ToneType::None)
        | (ToneType::Dtcs(code, pol), ToneType::Dtcs(_, _)) => {
            // DTCS mode
            mem.tmode = "DTCS".to_string();
            mem.dtcs = *code;
            mem.rx_dtcs = *code;

            // Set polarity
            mem.dtcs_polarity = match pol {
                'N' => "NN".to_string(),
                'R' => "RN".to_string(), // TX reversed
                _ => "NN".to_string(),
            };
        }
        (ToneType::None, ToneType::Ctcss(freq)) => {
            // RX CTCSS only (TSQL-R mode)
            mem.tmode = "TSQL-R".to_string();
            mem.ctone = *freq;
        }
        (ToneType::None, ToneType::Dtcs(code, _)) => {
            // RX DTCS only (DTCS-R mode)
            mem.tmode = "DTCS-R".to_string();
            mem.rx_dtcs = *code;
        }
        (ToneType::Ctcss(_), ToneType::Dtcs(_, _)) | (ToneType::Dtcs(_, _), ToneType::Ctcss(_)) => {
            // Cross mode
            mem.tmode = "Cross".to_string();

            if matches!(tx_tone_type, ToneType::Ctcss(_)) {
                mem.rtone = if let ToneType::Ctcss(f) = tx_tone_type {
                    f
                } else {
                    88.5
                };
                mem.cross_mode = "Tone->DTCS".to_string();
                mem.rx_dtcs = if let ToneType::Dtcs(c, _) = rx_tone_type {
                    c
                } else {
                    23
                };
            } else {
                mem.dtcs = if let ToneType::Dtcs(c, _) = tx_tone_type {
                    c
                } else {
                    23
                };
                mem.cross_mode = "DTCS->Tone".to_string();
                mem.ctone = if let ToneType::Ctcss(f) = rx_tone_type {
                    f
                } else {
                    88.5
                };
            }
        }
    }

    Ok(())
}

/// Helper enum for tone type detection
#[derive(Debug, Clone)]
enum ToneType {
    None,
    Ctcss(f32),
    Dtcs(u16, char),
}

/// Encode Memory structure to RawMemory
fn encode_memory(mem: &Memory) -> RadioResult<RawMemory> {
    // Validate frequency is in valid bands
    if !((136_000_000..=174_000_000).contains(&mem.freq)
        || (400_000_000..=520_000_000).contains(&mem.freq))
    {
        return Err(RadioError::Radio(format!(
            "Frequency {} Hz is outside valid bands (136-174 MHz or 400-520 MHz)",
            mem.freq
        )));
    }

    // Encode RX frequency
    let rxfreq = freq_to_bcd(mem.freq);

    // Encode TX frequency based on duplex
    let txfreq = match mem.duplex.as_str() {
        "off" => 0xFFFFFFFF, // TX inhibited
        "" => rxfreq,        // Simplex
        "+" => freq_to_bcd(mem.freq + mem.offset),
        "-" => freq_to_bcd(mem.freq.saturating_sub(mem.offset)),
        "split" => freq_to_bcd(mem.offset), // In split mode, offset is the TX freq
        _ => {
            return Err(RadioError::Radio(format!(
                "Invalid duplex mode: {}",
                mem.duplex
            )))
        }
    };

    // Encode tone modes
    let (txtone, rxtone) = encode_tone_mode(mem)?;

    // Determine band (VHF or UHF)
    let isuhf = mem.freq >= 300_000_000;

    // Power level
    let lowpower = if let Some(ref power) = mem.power {
        // Match power level (High=0, Low=1)
        // Note: Some UV-5R variants support 3 levels (High=0, Mid=1, Low=2)
        // but standard UV-5R only has 2 levels (High=0, Low=1)
        if power.watts() < 2.5 {
            1 // Low (1W)
        } else {
            0 // High (4W)
        }
    } else {
        0 // Default to high
    };

    // Mode (wide = FM, narrow = NFM)
    let wide = mem.mode == "FM";

    // Scan flag (empty skip = scan enabled)
    let scan = mem.skip.is_empty();

    Ok(RawMemory {
        rxfreq,
        txfreq,
        rxtone,
        txtone,
        isuhf,
        scode: 0, // PTT ID code (default 0)
        lowpower,
        wide,
        bcl: false, // Busy channel lockout (default off)
        scan,
        pttid: 0, // PTT ID setting (default 0)
    })
}

/// Encode tone mode from Memory to raw u16 values
fn encode_tone_mode(mem: &Memory) -> RadioResult<(u16, u16)> {
    let txtone: u16;
    let rxtone: u16;

    match mem.tmode.as_str() {
        "" => {
            // No tones
            txtone = 0;
            rxtone = 0;
        }
        "Tone" => {
            // TX CTCSS only
            txtone = tone_to_u16(mem.rtone);
            rxtone = 0;
        }
        "TSQL" => {
            // TX and RX CTCSS (same tone)
            // Note: For TSQL, both use ctone (not rtone)
            txtone = tone_to_u16(mem.ctone);
            rxtone = tone_to_u16(mem.ctone);
        }
        "TSQL-R" => {
            // RX CTCSS only
            txtone = 0;
            rxtone = tone_to_u16(mem.ctone);
        }
        "DTCS" => {
            // DTCS mode
            // Parse polarity (e.g., "NN", "NR", "RN", "RR")
            let tx_polarity = mem.dtcs_polarity.chars().next().unwrap_or('N');
            let rx_polarity = mem.dtcs_polarity.chars().nth(1).unwrap_or('N');

            txtone = dtcs_to_u16(mem.dtcs, tx_polarity)?;
            rxtone = dtcs_to_u16(mem.rx_dtcs, rx_polarity)?;
        }
        "DTCS-R" => {
            // RX DTCS only
            let rx_polarity = mem.dtcs_polarity.chars().nth(1).unwrap_or('N');
            txtone = 0;
            rxtone = dtcs_to_u16(mem.rx_dtcs, rx_polarity)?;
        }
        "Cross" => {
            // Cross mode
            match mem.cross_mode.as_str() {
                "Tone->DTCS" => {
                    txtone = tone_to_u16(mem.rtone);
                    let rx_polarity = mem.dtcs_polarity.chars().nth(1).unwrap_or('N');
                    rxtone = dtcs_to_u16(mem.rx_dtcs, rx_polarity)?;
                }
                "DTCS->Tone" => {
                    let tx_polarity = mem.dtcs_polarity.chars().next().unwrap_or('N');
                    txtone = dtcs_to_u16(mem.dtcs, tx_polarity)?;
                    rxtone = tone_to_u16(mem.ctone);
                }
                "Tone->Tone" => {
                    txtone = tone_to_u16(mem.rtone);
                    rxtone = tone_to_u16(mem.ctone);
                }
                "DTCS->" => {
                    let tx_polarity = mem.dtcs_polarity.chars().next().unwrap_or('N');
                    txtone = dtcs_to_u16(mem.dtcs, tx_polarity)?;
                    rxtone = 0;
                }
                "->DTCS" => {
                    txtone = 0;
                    let rx_polarity = mem.dtcs_polarity.chars().nth(1).unwrap_or('N');
                    rxtone = dtcs_to_u16(mem.rx_dtcs, rx_polarity)?;
                }
                "->Tone" => {
                    txtone = 0;
                    rxtone = tone_to_u16(mem.ctone);
                }
                _ => {
                    return Err(RadioError::Radio(format!(
                        "Unsupported cross mode: {}",
                        mem.cross_mode
                    )))
                }
            }
        }
        _ => {
            return Err(RadioError::Radio(format!(
                "Unsupported tone mode: {}",
                mem.tmode
            )))
        }
    }

    Ok((txtone, rxtone))
}

/// Implementation of Radio trait for UV-5R
impl Radio for UV5RRadio {
    fn vendor(&self) -> &str {
        &self.vendor
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn get_features(&self) -> RadioFeatures {
        RadioFeatures {
            memory_bounds: (0, NUM_MEMORIES - 1),
            valid_modes: vec!["FM".to_string(), "NFM".to_string()],
            valid_tmodes: vec![
                String::new(),
                "Tone".to_string(),
                "TSQL".to_string(),
                "DTCS".to_string(),
                "Cross".to_string(),
            ],
            valid_duplexes: vec![
                String::new(),
                "+".to_string(),
                "-".to_string(),
                "split".to_string(),
                "off".to_string(),
            ],
            valid_tuning_steps: vec![2.5, 5.0, 6.25, 10.0, 12.5, 20.0, 25.0, 50.0],
            valid_tones: TONES.to_vec(),
            valid_dtcs_codes: DTCS_CODES.to_vec(),
            valid_name_length: 7,
            valid_bands: vec![
                (136_000_000, 174_000_000), // VHF: 136-174 MHz
                (400_000_000, 520_000_000), // UHF: 400-520 MHz
            ],
            has_bank: false,
            has_dtcs: true,
            has_ctone: true,
            has_cross: true,
            has_tuning_step: true,
            has_mode: true,
            has_offset: true,
            has_name: true,
            has_comment: false,
            has_variable_power: true,
            valid_characters: UV5R_CHARSET.to_string(),
            valid_power_levels: POWER_LEVELS
                .iter()
                .map(|(label, watts)| PowerLevel::from_watts(*label, *watts))
                .collect(),
            ..Default::default()
        }
    }

    fn get_memory(&mut self, number: u32) -> RadioResult<Option<Memory>> {
        if number >= NUM_MEMORIES {
            return Err(RadioError::InvalidMemory(number));
        }

        let raw = self.read_raw_memory(number)?;

        // Check if memory is empty (0xFF pattern)
        if raw.is_empty() {
            return Ok(None);
        }

        let name = self.read_name(number)?;
        let mem = decode_memory(number, &raw, &name)?;

        // Also check if frequency is 0 (invalid BCD decode) - treat as empty
        if mem.freq == 0 {
            tracing::debug!(
                "Memory #{} has invalid frequency (0 Hz), treating as empty",
                number
            );
            return Ok(None);
        }

        Ok(Some(mem))
    }

    fn set_memory(&mut self, memory: &Memory) -> RadioResult<()> {
        if memory.number >= NUM_MEMORIES {
            return Err(RadioError::InvalidMemory(memory.number));
        }

        let raw = encode_memory(memory)?;
        self.write_raw_memory(memory.number, &raw)?;
        self.write_name(memory.number, &memory.name)?;
        Ok(())
    }

    fn delete_memory(&mut self, number: u32) -> RadioResult<()> {
        if number >= NUM_MEMORIES {
            return Err(RadioError::InvalidMemory(number));
        }

        // Set memory to empty (all 0xFF)
        let offset = self.memory_offset(number);
        let name_offset = self.name_offset(number);

        let mmap = self
            .mmap
            .as_mut()
            .ok_or(RadioError::Radio("No memory map loaded".to_string()))?;

        let empty = vec![0xFFu8; MEMORY_SIZE];
        mmap.set_bytes(offset, &empty).map_err(|e| {
            RadioError::Radio(format!(
                "Failed to erase memory at offset {}: {}",
                offset, e
            ))
        })?;

        // Clear name too
        let empty_name = vec![0xFFu8; 7];
        mmap.set_bytes(name_offset, &empty_name).map_err(|e| {
            RadioError::Radio(format!(
                "Failed to erase name at offset {}: {}",
                name_offset, e
            ))
        })?;

        Ok(())
    }

    fn get_memories(&mut self) -> RadioResult<Vec<Memory>> {
        let mut memories = Vec::new();
        for i in 0..NUM_MEMORIES {
            if let Some(mem) = self.get_memory(i)? {
                memories.push(mem);
            }
        }
        Ok(memories)
    }
}

/// Implementation of CloneModeRadio trait for UV-5R
impl CloneModeRadio for UV5RRadio {
    fn get_memsize(&self) -> usize {
        MEMSIZE
    }

    async fn sync_in(
        &mut self,
        port: &mut SerialPort,
        status_fn: Option<StatusCallback>,
    ) -> RadioResult<MemoryMap> {
        // Perform handshake to establish communication
        let ident = self.do_handshake(port).await?;

        // CHIRP file format: ident bytes go at the BEGINNING as an 8-byte header
        // The radio sends them during handshake, and CHIRP places them at 0x0000-0x0007
        let mut data = Vec::with_capacity(MEMSIZE);

        // Prepend ident as 8-byte header (matching CHIRP format)
        // Take only first 8 bytes if ident is longer
        if ident.len() >= 8 {
            data.extend_from_slice(&ident[0..8]);
        } else {
            data.extend_from_slice(&ident);
            // Pad to 8 bytes if ident is shorter
            while data.len() < 8 {
                data.push(0xFF);
            }
        }

        // Progress callback for header
        if let Some(ref cb) = status_fn {
            cb(0, 0x1808, "Downloading from radio");
        }

        // Read ALL memory from 0x0000 to 0x1800
        // This goes into file offsets 0x0008-0x1808 (after the 8-byte header)
        let start_addr = 0x0000;
        let end_addr = 0x1800;

        for addr in (start_addr..end_addr).step_by(BLOCK_SIZE) {
            let size = BLOCK_SIZE.min(end_addr - addr);
            let block = self.read_block(port, addr as u16, size as u8).await?;
            data.extend_from_slice(&block);

            // Progress callback
            if let Some(ref cb) = status_fn {
                cb(addr + 8, end_addr + 8, "Downloading from radio");
            }
        }

        // Pad to MEMSIZE if needed
        while data.len() < MEMSIZE {
            data.push(0xFF);
        }

        // DEBUG: Save downloaded data to file for inspection
        if let Err(e) = std::fs::write("/tmp/uv5r_download_raw.bin", &data) {
            tracing::warn!("Failed to save debug file: {}", e);
        } else {
            tracing::debug!("Saved downloaded data to /tmp/uv5r_download_raw.bin");
        }

        Ok(MemoryMap::new(data))
    }

    async fn sync_out(
        &mut self,
        port: &mut SerialPort,
        mmap: &MemoryMap,
        status_fn: Option<StatusCallback>,
    ) -> RadioResult<()> {
        // Perform handshake
        self.do_handshake(port).await?;

        // Upload memory blocks, skipping certain ranges
        let end_addr = 0x1808;

        // Calculate ranges to upload (excluding skip ranges)
        let mut ranges = Vec::new();
        let mut current_start = 0x0008;

        for &(skip_start, skip_end) in UPLOAD_SKIP_RANGES {
            if current_start < skip_start {
                ranges.push((current_start, skip_start));
            }
            current_start = skip_end;
        }

        // Add final range
        if current_start < end_addr {
            ranges.push((current_start, end_addr));
        }

        // Upload each range
        for (start, end) in ranges {
            for file_offset in (start..end).step_by(WRITE_BLOCK_SIZE) {
                let size = WRITE_BLOCK_SIZE.min(end - file_offset);
                let data = mmap
                    .get(file_offset, Some(size))
                    .map_err(|e| RadioError::Radio(format!("Failed to read memory map: {}", e)))?;

                // Convert file offset to radio address
                // File has 8-byte header (0x0000-0x0007), so memory data starts at 0x0008
                // Radio memory starts at address 0x0000
                let radio_addr = (file_offset - 8) as u16;

                self.write_block(port, radio_addr, data).await?;

                // Progress callback
                if let Some(ref cb) = status_fn {
                    cb(file_offset, end_addr, "Uploading to radio");
                }
            }
        }

        Ok(())
    }

    fn process_mmap(&mut self, mmap: &MemoryMap) -> RadioResult<()> {
        // Validate size
        if mmap.len() < MEMSIZE {
            return Err(RadioError::Radio(format!(
                "Memory map too small: expected at least {} bytes, got {}",
                MEMSIZE,
                mmap.len()
            )));
        }

        self.mmap = Some(mmap.clone());
        Ok(())
    }

    fn match_model(data: &[u8], filename: &str) -> bool {
        // Check file extension
        if !filename.ends_with(".dat") && !filename.ends_with(".uv5") {
            return false;
        }

        // Check size
        if data.len() < 0x1808 || data.len() > 0x2000 {
            return false;
        }

        // Heuristic: check if first memory has valid VHF/UHF frequency
        if data.len() >= MEMORY_BASE + 4 {
            let freq_bcd = u32::from_le_bytes([
                data[MEMORY_BASE],
                data[MEMORY_BASE + 1],
                data[MEMORY_BASE + 2],
                data[MEMORY_BASE + 3],
            ]);

            // BCD validation: each nibble must be 0-9
            if is_valid_bcd(freq_bcd) {
                let freq = bcd_to_freq(freq_bcd);

                // Check if frequency is in valid bands (with some margin)
                return (130_000_000..=180_000_000).contains(&freq)
                    || (390_000_000..=530_000_000).contains(&freq);
            }
        }

        false
    }
}

/// UV-5R protocol helper methods
impl UV5RRadio {
    /// Perform handshake with radio
    ///
    /// The handshake sequence is:
    /// 1. Send magic bytes (try both variants)
    /// 2. Wait for ACK (0x06)
    /// 3. Send 0x02
    /// 4. Read ident (8 bytes ending with 0xDD)
    /// 5. Send ACK (0x06)
    /// 6. Wait for ACK (0x06)
    async fn do_handshake(&self, port: &mut SerialPort) -> RadioResult<Vec<u8>> {
        // Try both magic sequences
        let magics = [UV5R_MODEL_291, UV5R_MODEL_ORIG];

        for magic in magics.iter() {
            tracing::debug!("Trying magic sequence: {:02X?}", magic);

            // Send magic byte-by-byte with delay
            for &byte in *magic {
                port.write(&[byte]).await?;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            // Wait for ACK (0x06)
            let mut ack_buf = [0u8; 1];
            match timeout(Duration::from_secs(1), port.read_exact(&mut ack_buf)).await {
                Ok(Ok(())) if ack_buf[0] == 0x06 => {
                    tracing::debug!("Received ACK after magic");

                    // Send 0x02
                    port.write(&[0x02]).await?;

                    // Read ident (up to 12 bytes, ending with 0xDD)
                    let mut ident = Vec::new();
                    for _ in 0..12 {
                        let mut byte_buf = [0u8; 1];
                        match timeout(Duration::from_secs(1), port.read_exact(&mut byte_buf)).await
                        {
                            Ok(Ok(())) => {
                                ident.push(byte_buf[0]);
                                if byte_buf[0] == 0xDD {
                                    break;
                                }
                            }
                            _ => break,
                        }
                    }

                    tracing::debug!("Received ident: {:02X?}", ident);

                    // Validate ident length (8 or 12 bytes)
                    if ident.len() == 8 || ident.len() == 12 {
                        // Send ACK
                        port.write(&[0x06]).await?;

                        // Wait for final ACK
                        let mut ack2_buf = [0u8; 1];
                        match timeout(Duration::from_secs(1), port.read_exact(&mut ack2_buf)).await
                        {
                            Ok(Ok(())) if ack2_buf[0] == 0x06 => {
                                tracing::info!(
                                    "Handshake successful with {} magic",
                                    if magic == &UV5R_MODEL_291 {
                                        "291"
                                    } else {
                                        "ORIG"
                                    }
                                );
                                return Ok(ident);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }

            // Wait before trying next magic
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        Err(RadioError::NoResponse)
    }

    /// Read a block of data from the radio
    ///
    /// Protocol:
    /// - Send: "S" + addr (u16 BE) + size (u8)
    /// - Receive: "X" + addr (u16 BE) + size (u8) + data
    /// - Send: ACK (0x06)
    async fn read_block(&self, port: &mut SerialPort, addr: u16, size: u8) -> RadioResult<Vec<u8>> {
        // Send read command
        let cmd = [b'S', (addr >> 8) as u8, (addr & 0xFF) as u8, size];
        port.write(&cmd).await?;

        tracing::debug!("Sent read command: addr={:04X} size={:02X}", addr, size);

        // Small delay to let radio process command
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Read response header (4 bytes)
        // Note: Some cheap clone cables echo transmitted data, which can cause
        // the first byte to be our ACK (0x06) instead of 'X'. We handle this
        // by checking and skipping the echo if detected.
        let mut hdr = [0u8; 4];
        timeout(Duration::from_secs(2), port.read_exact(&mut hdr))
            .await
            .map_err(|_| RadioError::Timeout)??;

        // Check if first byte is an echoed ACK (0x06) from previous operation
        if hdr[0] == 0x06 {
            tracing::debug!("Detected echoed ACK, reading actual header");
            // Shift bytes left and read one more byte
            hdr[0] = hdr[1];
            hdr[1] = hdr[2];
            hdr[2] = hdr[3];
            // Read the 4th byte
            let mut last_byte = [0u8; 1];
            timeout(Duration::from_secs(2), port.read_exact(&mut last_byte))
                .await
                .map_err(|_| RadioError::Timeout)??;
            hdr[3] = last_byte[0];
            tracing::debug!("Corrected header: {:02X?}", hdr);
        }

        let rcmd = hdr[0];
        let raddr = u16::from_be_bytes([hdr[1], hdr[2]]);
        let rsize = hdr[3];

        if rcmd != b'X' || raddr != addr || rsize != size {
            return Err(RadioError::InvalidResponse(format!(
                "Response mismatch: cmd={:02X} addr={:04X} size={:02X} (expected X {:04X} {:02X})",
                rcmd, raddr, rsize, addr, size
            )));
        }

        // Read data
        let mut data = vec![0u8; size as usize];
        timeout(Duration::from_secs(2), port.read_exact(&mut data))
            .await
            .map_err(|_| RadioError::Timeout)??;

        // Send ACK
        port.write(&[0x06]).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;

        Ok(data)
    }

    /// Write a block of data to the radio
    ///
    /// Protocol:
    /// - Send: "X" + addr (u16 BE) + size (u8) + data
    /// - Receive: ACK (0x06)
    async fn write_block(&self, port: &mut SerialPort, addr: u16, data: &[u8]) -> RadioResult<()> {
        // Send write command
        let mut cmd = vec![
            b'X',
            (addr >> 8) as u8,
            (addr & 0xFF) as u8,
            data.len() as u8,
        ];
        cmd.extend_from_slice(data);
        port.write(&cmd).await?;

        tracing::debug!(
            "Sent write command: addr={:04X} size={:02X}",
            addr,
            data.len()
        );

        tokio::time::sleep(Duration::from_millis(50)).await;

        // Wait for ACK
        let mut ack_buf = [0u8; 1];
        timeout(Duration::from_secs(2), port.read_exact(&mut ack_buf))
            .await
            .map_err(|_| RadioError::Timeout)??;

        if ack_buf[0] != 0x06 {
            return Err(RadioError::InvalidResponse(format!(
                "Write rejected: received {:02X} instead of ACK",
                ack_buf[0]
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bcd_encoding() {
        // Test frequency encoding/decoding
        let freq = 146_520_000u64;
        let bcd = freq_to_bcd(freq);
        let decoded = bcd_to_freq(bcd);
        assert_eq!(freq, decoded);

        // Test UHF frequency
        let freq_uhf = 446_000_000u64;
        let bcd_uhf = freq_to_bcd(freq_uhf);
        let decoded_uhf = bcd_to_freq(bcd_uhf);
        assert_eq!(freq_uhf, decoded_uhf);
    }

    #[test]
    fn test_bcd_validation() {
        // Valid BCD
        let freq_bcd = freq_to_bcd(146_520_000);
        assert!(is_valid_bcd(freq_bcd));

        // Invalid BCD (contains 0xA nibble)
        assert!(!is_valid_bcd(0xABCD1234));
    }

    #[test]
    fn test_tone_encoding() {
        // CTCSS tone 88.5 Hz
        let tone_freq = 88.5f32;
        let encoded = tone_to_u16(tone_freq);
        assert_eq!(encoded, 885);

        let decoded = u16_to_tone(encoded).unwrap();
        assert_eq!(decoded, 88.5);

        // Verify threshold
        let below_threshold = 100u16;
        assert!(u16_to_tone(below_threshold).is_none());
    }

    #[test]
    fn test_dtcs_encoding() {
        // DTCS code 023, normal polarity
        let encoded = dtcs_to_u16(23, 'N').unwrap();
        assert_eq!(encoded, 1); // index 0 + 1

        let (code, polarity) = u16_to_dtcs(encoded).unwrap();
        assert_eq!(code, 23);
        assert_eq!(polarity, 'N');

        // DTCS code 023, reversed polarity
        let encoded_r = dtcs_to_u16(23, 'R').unwrap();
        assert_eq!(encoded_r, 0x69 + 1);

        let (code_r, polarity_r) = u16_to_dtcs(encoded_r).unwrap();
        assert_eq!(code_r, 23);
        assert_eq!(polarity_r, 'R');
    }

    #[test]
    fn test_raw_memory_roundtrip() {
        // Create a test memory
        let raw = RawMemory {
            rxfreq: freq_to_bcd(146_520_000),
            txfreq: freq_to_bcd(146_520_000 + 600_000),
            rxtone: 885, // 88.5 Hz
            txtone: 885,
            isuhf: false,
            scode: 0,
            lowpower: 0,
            wide: true,
            bcl: false,
            scan: true,
            pttid: 0,
        };

        // Encode and decode
        let bytes = raw.to_bytes();
        assert_eq!(bytes.len(), 16);

        let decoded = RawMemory::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.rxfreq, raw.rxfreq);
        assert_eq!(decoded.txfreq, raw.txfreq);
        assert_eq!(decoded.txtone, raw.txtone);
        assert_eq!(decoded.rxtone, raw.rxtone);
        assert_eq!(decoded.wide, raw.wide);
        assert_eq!(decoded.scan, raw.scan);
    }

    #[test]
    fn test_memory_conversion_simplex() {
        // Test Memory → RawMemory → Memory roundtrip for simplex
        let mut mem = Memory::new(0);
        mem.freq = 146_520_000;
        mem.offset = 0;
        mem.duplex = String::new();
        mem.mode = "FM".to_string();
        mem.rtone = 88.5;
        mem.tmode = "Tone".to_string();
        mem.power = Some(PowerLevel::from_watts("High", 4.0));
        mem.name = "TEST".to_string();

        let raw = encode_memory(&mem).unwrap();
        let decoded = decode_memory(0, &raw, "TEST").unwrap();

        assert_eq!(decoded.freq, mem.freq);
        assert_eq!(decoded.duplex, mem.duplex);
        assert_eq!(decoded.offset, mem.offset);
        assert_eq!(decoded.mode, mem.mode);
        assert_eq!(decoded.tmode, mem.tmode);
        assert_eq!(decoded.rtone, mem.rtone);
    }

    #[test]
    fn test_memory_conversion_plus_offset() {
        // Test with positive offset
        let mut mem = Memory::new(1);
        mem.freq = 146_520_000;
        mem.offset = 600_000;
        mem.duplex = "+".to_string();
        mem.mode = "NFM".to_string();
        mem.power = Some(PowerLevel::from_watts("Low", 1.0));
        mem.name = "RPTR".to_string();

        let raw = encode_memory(&mem).unwrap();
        let decoded = decode_memory(1, &raw, "RPTR").unwrap();

        assert_eq!(decoded.freq, mem.freq);
        assert_eq!(decoded.duplex, mem.duplex);
        assert_eq!(decoded.offset, mem.offset);
        assert_eq!(decoded.mode, mem.mode);

        // Check power level
        assert!(decoded.power.is_some());
        let power = decoded.power.unwrap();
        assert!((power.watts() - 1.0).abs() < 0.1);
    }

    #[test]
    fn test_memory_conversion_dtcs() {
        // Test DTCS tone mode
        let mut mem = Memory::new(2);
        mem.freq = 446_000_000;
        mem.duplex = String::new();
        mem.mode = "FM".to_string();
        mem.tmode = "DTCS".to_string();
        mem.dtcs = 23;
        mem.rx_dtcs = 23;
        mem.dtcs_polarity = "NN".to_string();

        let raw = encode_memory(&mem).unwrap();
        let decoded = decode_memory(2, &raw, "").unwrap();

        assert_eq!(decoded.tmode, "DTCS");
        assert_eq!(decoded.dtcs, 23);
        assert_eq!(decoded.rx_dtcs, 23);
    }

    #[test]
    fn test_memory_conversion_tx_inhibit() {
        // Test TX inhibit (duplex = "off")
        let mut mem = Memory::new(3);
        mem.freq = 146_520_000;
        mem.duplex = "off".to_string();
        mem.mode = "FM".to_string();

        let raw = encode_memory(&mem).unwrap();
        assert_eq!(raw.txfreq, 0xFFFFFFFF);

        let decoded = decode_memory(3, &raw, "").unwrap();
        assert_eq!(decoded.duplex, "off");
    }

    #[test]
    fn test_match_model_valid() {
        // Create test data with valid UV-5R memory
        let mut data = vec![0xFFu8; 0x1808];

        // Set first memory to valid VHF frequency (146.520 MHz)
        let freq_bcd = freq_to_bcd(146_520_000);
        data[MEMORY_BASE..MEMORY_BASE + 4].copy_from_slice(&freq_bcd.to_le_bytes());

        assert!(UV5RRadio::match_model(&data, "test.dat"));
        assert!(UV5RRadio::match_model(&data, "radio.uv5"));
    }

    #[test]
    fn test_match_model_invalid() {
        let mut data = vec![0xFFu8; 0x1808];

        // Wrong file extension
        assert!(!UV5RRadio::match_model(&data, "test.img"));

        // Invalid frequency (invalid BCD)
        data[MEMORY_BASE] = 0xAB; // Invalid BCD digit
        assert!(!UV5RRadio::match_model(&data, "test.dat"));

        // Wrong size
        let small_data = vec![0u8; 100];
        assert!(!UV5RRadio::match_model(&small_data, "test.dat"));
    }

    #[test]
    fn test_memory_offset() {
        let radio = UV5RRadio::new();
        assert_eq!(radio.memory_offset(0), MEMORY_BASE);
        assert_eq!(radio.memory_offset(1), MEMORY_BASE + 16);
        assert_eq!(radio.memory_offset(127), MEMORY_BASE + 127 * 16);
    }

    #[test]
    fn test_name_offset() {
        let radio = UV5RRadio::new();
        assert_eq!(radio.name_offset(0), NAME_BASE);
        assert_eq!(radio.name_offset(1), NAME_BASE + 16);
        assert_eq!(radio.name_offset(127), NAME_BASE + 127 * 16);
    }

    #[test]
    fn test_features() {
        let radio = UV5RRadio::new();
        let features = radio.get_features();

        assert_eq!(features.memory_bounds, (0, 127));
        assert_eq!(features.valid_name_length, 7);
        assert_eq!(features.valid_modes.len(), 2); // FM, NFM
        assert!(!features.has_bank);
        assert!(features.has_dtcs);
        assert!(features.has_ctone);
        assert!(features.has_cross);

        // Check valid bands
        assert_eq!(features.valid_bands.len(), 2);
        assert_eq!(features.valid_bands[0], (136_000_000, 174_000_000));
        assert_eq!(features.valid_bands[1], (400_000_000, 520_000_000));

        // Check power levels
        assert_eq!(features.valid_power_levels.len(), 2);
    }

    #[test]
    fn test_get_memsize() {
        let radio = UV5RRadio::new();
        assert_eq!(radio.get_memsize(), MEMSIZE);
    }

    #[test]
    fn test_empty_memory_detection() {
        let raw = RawMemory {
            rxfreq: 0xFFFFFFFF,
            txfreq: 0xFFFFFFFF,
            rxtone: 0,
            txtone: 0,
            isuhf: false,
            scode: 0,
            lowpower: 0,
            wide: true,
            bcl: false,
            scan: true,
            pttid: 0,
        };

        assert!(raw.is_empty());
    }

    #[test]
    fn test_cross_mode_tone_to_dtcs() {
        // Test Cross mode: Tone->DTCS
        let mut mem = Memory::new(10);
        mem.freq = 146_520_000;
        mem.mode = "FM".to_string();
        mem.tmode = "Cross".to_string();
        mem.cross_mode = "Tone->DTCS".to_string();
        mem.rtone = 100.0;
        mem.rx_dtcs = 23;
        mem.dtcs_polarity = "NN".to_string();

        let raw = encode_memory(&mem).unwrap();
        let decoded = decode_memory(10, &raw, "").unwrap();

        assert_eq!(decoded.tmode, "Cross");
        assert_eq!(decoded.cross_mode, "Tone->DTCS");
        assert_eq!(decoded.rtone, 100.0);
        assert_eq!(decoded.rx_dtcs, 23);
    }

    #[test]
    fn test_tsql_mode() {
        // Test TSQL (TX and RX use same tone)
        let mut mem = Memory::new(11);
        mem.freq = 146_520_000;
        mem.mode = "FM".to_string();
        mem.tmode = "TSQL".to_string();
        mem.rtone = 123.0;
        mem.ctone = 123.0;

        let raw = encode_memory(&mem).unwrap();

        // In TSQL mode, both TX and RX should use the same tone
        assert_eq!(raw.txtone, tone_to_u16(123.0));
        assert_eq!(raw.rxtone, tone_to_u16(123.0));

        let decoded = decode_memory(11, &raw, "").unwrap();
        assert_eq!(decoded.tmode, "TSQL");
        assert_eq!(decoded.rtone, 123.0);
    }

    #[test]
    fn test_frequency_validation() {
        // Test invalid frequency (out of band)
        let mut mem = Memory::new(0);
        mem.freq = 200_000_000; // Invalid (between VHF and UHF)
        mem.mode = "FM".to_string();

        let result = encode_memory(&mem);
        assert!(result.is_err());
    }

    #[test]
    fn test_name_truncation() {
        // Names should be truncated to 7 characters
        let mut radio = UV5RRadio::new();

        // Create a memory map for testing
        let mmap = MemoryMap::new(vec![0xFFu8; MEMSIZE]);
        radio.process_mmap(&mmap).unwrap();

        // Write a long name
        let long_name = "VERYLONGNAME";
        radio.write_name(0, long_name).unwrap();

        // Read it back (should be truncated to 7 chars)
        let read_name = radio.read_name(0).unwrap();
        assert!(read_name.len() <= 7);
    }

    #[test]
    fn test_split_mode() {
        // Test split operation (large TX/RX difference)
        let mut mem = Memory::new(5);
        mem.freq = 146_520_000; // RX
        mem.offset = 446_000_000; // TX (stored in offset for split mode)
        mem.duplex = "split".to_string();
        mem.mode = "FM".to_string();

        let raw = encode_memory(&mem).unwrap();
        assert_eq!(bcd_to_freq(raw.rxfreq), 146_520_000);
        assert_eq!(bcd_to_freq(raw.txfreq), 446_000_000);

        let decoded = decode_memory(5, &raw, "").unwrap();
        assert_eq!(decoded.duplex, "split");
        assert_eq!(decoded.freq, 146_520_000);
        assert_eq!(decoded.offset, 446_000_000);
    }
}
