// Icom IC-9700 Driver
// Reference: chirp/drivers/icomciv.py lines 145-169 (memory formats)
//            chirp/drivers/icomciv.py lines 1337-1720 (IC-9700 implementation)

use crate::bitwise::bcd;
use crate::core::{DVMemory, Memory, RadioFeatures};
use crate::drivers::traits::StatusCallback;
use crate::drivers::{Radio, RadioError, RadioResult};
use crate::serial::{CivProtocol, SerialPort};

// IC-9700 CI-V model code
const MODEL_CODE: u8 = 0xA2;
const CONTROLLER_ADDR: u8 = 0xE0;

// IC-9700 supports these modes
// Matches Python CHIRP _MODES array (29 entries, indices 0-28)
const MODES: &[Option<&str>] = &[
    Some("LSB"),    // 0
    Some("USB"),    // 1
    Some("AM"),     // 2
    Some("CW"),     // 3
    Some("RTTY"),   // 4
    Some("FM"),     // 5
    Some("CWR"),    // 6
    Some("RTTY-R"), // 7
    None,           // 8
    None,           // 9
    None,           // 10
    None,           // 11
    None,           // 12
    None,           // 13
    None,           // 14
    None,           // 15
    None,           // 16
    Some("DV"),     // 17
    None,           // 18
    None,           // 19
    None,           // 20
    None,           // 21
    Some("DD"),     // 22
    None,           // 23
    None,           // 24
    None,           // 25
    None,           // 26
    None,           // 27
    None,           // 28
];

// Cross-mode tone support
const CROSS_MODES: &[(u8, &str)] = &[
    (4, "DTCS->"),
    (5, "Tone->DTCS"),
    (6, "DTCS->Tone"),
    (7, "Tone->Tone"),
];

// Band definitions (MHz)
const BANDS: &[(u32, u32)] = &[
    (144, 148),   // VHF
    (430, 450),   // UHF
    (1240, 1300), // 1.2 GHz
];

/// IC-9700 Memory Format (67 bytes total)
/// Reference: MEM_IC9700_FORMAT in icomciv.py lines 145-167
///
/// Byte layout:
/// 0: bank (1 byte)
/// 1-2: number BCD (2 bytes)
/// 3: select_memory (1 byte)
/// 4-8: freq BCD LE (5 bytes)
/// 9: mode (1 byte)
/// 10: filter (1 byte)
/// 11: data_mode (1 byte)
/// 12: duplex(low 4) | tmode(high 4) (1 byte)
/// 13: dig_sql (1 byte)
/// 14-16: rtone BCD (3 bytes)
/// 17-19: ctone BCD (3 bytes)
/// 20: dtcs_polarity (1 byte)
/// 21-22: dtcs BCD (2 bytes)
/// 23: dig_code (1 byte)
/// 24-26: duplex_offset BCD LE (3 bytes)
/// 27-34: urcall (8 bytes)
/// 35-42: rpt1call (8 bytes)
/// 43-50: rpt2call (8 bytes)
/// 51-66: name (16 bytes)
#[derive(Debug)]
#[allow(dead_code)]
struct RawMemory {
    bank: u8,
    number: u16, // BCD
    select_memory: u8,
    freq: u64, // BCD, little-endian, 5 bytes (10 digits)
    mode: u8,  // BCD
    filter: u8,
    data_mode: u8, // BCD
    duplex: u8,    // 4 bits
    tmode: u8,     // 4 bits
    dig_sql: u8,   // 4 bits
    rtone: u16,    // BCD, 3 bytes
    ctone: u16,    // BCD, 3 bytes
    dtcs_polarity: u8,
    dtcs: u16, // BCD, 2 bytes
    dig_code: u8,
    duplex_offset: u32, // BCD, little-endian, 3 bytes
    urcall: [u8; 8],
    rpt1call: [u8; 8],
    rpt2call: [u8; 8],
    name: [u8; 16],
}

impl RawMemory {
    /// Parse IC-9700 memory from raw bytes
    /// Memory format is 67 bytes total
    fn from_bytes(data: &[u8]) -> RadioResult<Self> {
        if data.len() < 67 {
            return Err(RadioError::InvalidResponse(format!(
                "Memory data too short: {} bytes (expected 67)",
                data.len()
            )));
        }

        let bank = data[0];
        let number = bcd::bcd_to_int_be(&data[1..3])? as u16;
        let select_memory = data[3];
        let freq = bcd::bcd_to_int_le(&data[4..9])?;
        // Mode is BCD encoded (bbcd in Python format)
        let mode = bcd::bcd_to_int_be(&data[9..10])? as u8;
        let filter = data[10];
        let data_mode = data[11];

        // Bitfields in byte 12: duplex (low 4 bits), tmode (high 4 bits)
        let duplex = data[12] & 0x0F;
        let tmode = (data[12] >> 4) & 0x0F;

        // Bitfields in byte 13: dig_sql (low 4 bits)
        let dig_sql = data[13] & 0x0F;

        // Tones are BCD, 3 bytes each (6 digits)
        let rtone = bcd::bcd_to_int_be(&data[14..17])? as u16;
        let ctone = bcd::bcd_to_int_be(&data[17..20])? as u16;

        let dtcs_polarity = data[20];
        let dtcs = bcd::bcd_to_int_be(&data[21..23])? as u16;
        let dig_code = data[23];

        // Duplex offset is little-endian BCD, 3 bytes
        let duplex_offset = bcd::bcd_to_int_le(&data[24..27])? as u32;

        // D-STAR call signs
        let mut urcall = [0u8; 8];
        let mut rpt1call = [0u8; 8];
        let mut rpt2call = [0u8; 8];
        let mut name = [0u8; 16];

        urcall.copy_from_slice(&data[27..35]);
        rpt1call.copy_from_slice(&data[35..43]);
        rpt2call.copy_from_slice(&data[43..51]);
        name.copy_from_slice(&data[51..67]);

        Ok(Self {
            bank,
            number,
            select_memory,
            freq,
            mode,
            filter,
            data_mode,
            duplex,
            tmode,
            dig_sql,
            rtone,
            ctone,
            dtcs_polarity,
            dtcs,
            dig_code,
            duplex_offset,
            urcall,
            rpt1call,
            rpt2call,
            name,
        })
    }

    /// Convert to Memory struct
    fn to_memory(&self, number: u32) -> RadioResult<Memory> {
        let mode_idx = self.mode as usize;
        tracing::debug!("Parsing memory {}: mode byte = {}", number, self.mode);
        let mode = MODES.get(mode_idx).and_then(|m| *m).ok_or_else(|| {
            let err = format!("Invalid mode: {} (index out of bounds or None)", self.mode);
            tracing::error!("{}", err);
            RadioError::InvalidResponse(err)
        })?;

        let mut mem = if mode == "DV" {
            // D-STAR mode - create DVMemory and populate both base and DV fields
            let mut dv = DVMemory::new(number);

            // Set base Memory DV fields (not DVMemory's duplicate fields)
            // These will be preserved when we return dv.base below
            dv.base.dv_urcall = String::from_utf8_lossy(&self.urcall).trim_end().to_string();
            dv.base.dv_rpt1call = String::from_utf8_lossy(&self.rpt1call)
                .trim_end()
                .to_string();
            dv.base.dv_rpt2call = String::from_utf8_lossy(&self.rpt2call)
                .trim_end()
                .to_string();
            dv.base.dv_code = self.dig_code;

            // Set other base fields
            dv.base.freq = self.freq;
            dv.base.name = String::from_utf8_lossy(&self.name).trim_end().to_string();
            dv.base.mode = mode.to_string();

            // Return base memory (Radio trait expects Memory, not DVMemory)
            // The base.dv_* fields are now populated, so they'll be available
            dv.base.clone()
        } else {
            Memory::new(number)
        };

        // Set fields directly (for non-DV) or update cloned base (for DV)
        if mode != "DV" {
            mem.freq = self.freq;
            mem.name = String::from_utf8_lossy(&self.name).trim_end().to_string();
            mem.mode = mode.to_string();
        }

        // Tone mode
        if let Some((_, cross_mode)) = CROSS_MODES.iter().find(|(code, _)| *code == self.tmode) {
            mem.tmode = "Cross".to_string();
            mem.cross_mode = cross_mode.to_string();
        } else {
            let tmode = match self.tmode {
                0 => "",
                1 => "Tone",
                2 => "TSQL",
                3 => "DTCS",
                _ => "",
            };
            mem.tmode = tmode.to_string();
        };

        // Tones
        mem.rtone = (self.rtone as f32) / 10.0;
        mem.ctone = (self.ctone as f32) / 10.0;
        mem.dtcs = self.dtcs;

        // DTCS polarity
        let dtcs_pol = match self.dtcs_polarity {
            0x11 => "RR",
            0x10 => "RN",
            0x01 => "NR",
            _ => "NN",
        };
        mem.dtcs_polarity = dtcs_pol.to_string();

        // Duplex
        let duplex = match self.duplex {
            0 => "",
            1 => "+",
            2 => "-",
            _ => "",
        };
        mem.duplex = duplex.to_string();
        mem.offset = (self.duplex_offset as u64) * 100;

        Ok(mem)
    }

    /// Convert from Memory struct
    fn from_memory(mem: &Memory, bank: u8) -> RadioResult<Vec<u8>> {
        let mut data = Vec::with_capacity(64);

        // NOTE: Bank and channel are sent in the CI-V command header,
        // so we don't include them in the memory data itself

        // Select memory (0 for now)
        data.push(0);

        // Frequency (BCD, little-endian, 5 bytes)
        let freq_bcd = bcd::int_to_bcd_le(mem.freq, 5)?;
        data.extend_from_slice(&freq_bcd);

        // Mode (BCD encoded)
        let mode_idx = MODES
            .iter()
            .position(|m| m.map(|s| s == mem.mode.as_str()).unwrap_or(false))
            .ok_or_else(|| RadioError::Unsupported(format!("Mode not supported: {}", mem.mode)))?;
        let mode_bcd = bcd::int_to_bcd_be(mode_idx as u64, 1)?;
        data.push(mode_bcd[0]);

        // Filter (1 = FIL1, which is default for most modes)
        data.push(1);

        // Data mode (0 for now)
        data.push(0);

        // Duplex and tmode (bitfield byte)
        let duplex = match mem.duplex.as_str() {
            "" => 0,
            "+" => 1,
            "-" => 2,
            _ => 0,
        };

        let tmode = if mem.tmode == "Cross" {
            // Find cross mode code
            CROSS_MODES
                .iter()
                .find(|(_, mode)| *mode == mem.cross_mode.as_str())
                .map(|(code, _)| *code)
                .unwrap_or(0)
        } else {
            match mem.tmode.as_str() {
                "" => 0,
                "Tone" => 1,
                "TSQL" => 2,
                "DTCS" => 3,
                _ => 0,
            }
        };

        data.push(duplex | (tmode << 4));

        // Dig_sql (0 for now) + unused
        data.push(0);

        // Tones (BCD, 3 bytes each)
        let rtone = (mem.rtone * 10.0) as u16;
        let ctone = (mem.ctone * 10.0) as u16;
        let rtone_bcd = bcd::int_to_bcd_be(rtone as u64, 3)?;
        let ctone_bcd = bcd::int_to_bcd_be(ctone as u64, 3)?;
        data.extend_from_slice(&rtone_bcd);
        data.extend_from_slice(&ctone_bcd);

        // DTCS polarity
        let dtcs_pol = match mem.dtcs_polarity.as_str() {
            "RR" => 0x11,
            "RN" => 0x10,
            "NR" => 0x01,
            _ => 0x00,
        };
        data.push(dtcs_pol);

        // DTCS code (BCD, 2 bytes)
        let dtcs_bcd = bcd::int_to_bcd_be(mem.dtcs as u64, 2)?;
        data.extend_from_slice(&dtcs_bcd);

        // Digital code (dv_code field from Memory struct)
        data.push(mem.dv_code);

        // Duplex offset (BCD, little-endian, 3 bytes)
        let offset = (mem.offset / 100) as u32;
        let offset_bcd = bcd::int_to_bcd_le(offset as u64, 3)?;
        data.extend_from_slice(&offset_bcd);

        // D-STAR call signs
        // For non-DV memories, IC-9700 expects "CQCQCQ" as default
        // For DV memories, these would come from DVMemory
        let urcall_padded = if mem.dv_urcall.is_empty() {
            *b"CQCQCQ  " // Default for non-DV
        } else {
            let mut buf = [b' '; 8];
            buf[..mem.dv_urcall.len().min(8)]
                .copy_from_slice(&mem.dv_urcall.as_bytes()[..mem.dv_urcall.len().min(8)]);
            buf
        };

        let rpt1call_padded = if mem.dv_rpt1call.is_empty() {
            [b' '; 8]
        } else {
            let mut buf = [b' '; 8];
            buf[..mem.dv_rpt1call.len().min(8)]
                .copy_from_slice(&mem.dv_rpt1call.as_bytes()[..mem.dv_rpt1call.len().min(8)]);
            buf
        };

        let rpt2call_padded = if mem.dv_rpt2call.is_empty() {
            [b' '; 8]
        } else {
            let mut buf = [b' '; 8];
            buf[..mem.dv_rpt2call.len().min(8)]
                .copy_from_slice(&mem.dv_rpt2call.as_bytes()[..mem.dv_rpt2call.len().min(8)]);
            buf
        };

        data.extend_from_slice(&urcall_padded);
        data.extend_from_slice(&rpt1call_padded);
        data.extend_from_slice(&rpt2call_padded);

        // Name (pad to 16 bytes)
        let name = &mem.name;
        let mut name_padded = [b' '; 16];
        name_padded[..name.len().min(16)].copy_from_slice(&name.as_bytes()[..name.len().min(16)]);
        data.extend_from_slice(&name_padded);

        Ok(data)
    }
}

/// IC-9700 Radio Driver (base class)
pub struct IC9700Radio {
    protocol: CivProtocol,
    band: Option<u8>,
}

impl IC9700Radio {
    pub fn new() -> Self {
        Self {
            protocol: CivProtocol::new(MODEL_CODE, CONTROLLER_ADDR),
            band: None,
        }
    }

    pub fn new_band(band: u8) -> Self {
        Self {
            protocol: CivProtocol::new(MODEL_CODE, CONTROLLER_ADDR),
            band: Some(band),
        }
    }
}

impl Radio for IC9700Radio {
    fn vendor(&self) -> &str {
        "Icom"
    }

    fn model(&self) -> &str {
        if let Some(band) = self.band {
            match band {
                1 => "IC-9700 (VHF)",
                2 => "IC-9700 (UHF)",
                3 => "IC-9700 (1.2GHz)",
                _ => "IC-9700",
            }
        } else {
            "IC-9700"
        }
    }

    fn get_features(&self) -> RadioFeatures {
        let mut features = RadioFeatures::new();
        features.memory_bounds = (1, 99);
        features.has_name = true;
        features.valid_name_length = 16;
        features.has_dtcs = true;
        features.has_dtcs_polarity = true;
        features.has_bank = true;

        // Valid modes depend on band
        if let Some(3) = self.band {
            // 1.2GHz band doesn't support DD mode
            features.valid_modes = MODES
                .iter()
                .filter_map(|m| *m)
                .filter(|m| *m != "DD")
                .map(|s| s.to_string())
                .collect();
        } else {
            features.valid_modes = MODES
                .iter()
                .filter_map(|m| *m)
                .map(|s| s.to_string())
                .collect();
        }

        features.valid_tmodes = vec![
            "".to_string(),
            "Tone".to_string(),
            "TSQL".to_string(),
            "DTCS".to_string(),
            "Cross".to_string(),
        ];

        features.valid_duplexes = vec!["".to_string(), "+".to_string(), "-".to_string()];

        // Set valid bands based on band number
        if let Some(band_num) = self.band {
            if let Some(&(low, high)) = BANDS.get((band_num - 1) as usize) {
                features.valid_bands = vec![(low as u64 * 1_000_000, high as u64 * 1_000_000)];
            }
        } else {
            features.valid_bands = BANDS
                .iter()
                .map(|(low, high)| (*low as u64 * 1_000_000, *high as u64 * 1_000_000))
                .collect();
        }

        features
    }

    fn get_memory(&mut self, _number: u32) -> RadioResult<Option<Memory>> {
        // This requires a serial port connection
        Err(RadioError::Unsupported(
            "get_memory requires serial port (use get_memory_from_port)".to_string(),
        ))
    }

    fn set_memory(&mut self, _memory: &Memory) -> RadioResult<()> {
        // This requires a serial port connection
        Err(RadioError::Unsupported(
            "set_memory requires serial port (use set_memory_to_port)".to_string(),
        ))
    }
}

impl IC9700Radio {
    /// Detect if the CI-V interface echoes commands
    /// This MUST be called before any other radio operations
    pub async fn detect_echo(&mut self, port: &mut SerialPort) -> RadioResult<bool> {
        tracing::debug!("IC-9700: Detecting CI-V echo...");
        let has_echo = self.protocol.detect_echo(port).await?;
        tracing::debug!("IC-9700: Interface echo detected: {}", has_echo);
        Ok(has_echo)
    }

    /// Get a memory from the radio via serial port
    pub async fn get_memory_from_port(
        &mut self,
        port: &mut SerialPort,
        number: u32,
    ) -> RadioResult<Option<Memory>> {
        let bank = self.band.unwrap_or(1);

        // Read memory via CI-V protocol
        let data = self.protocol.read_memory(port, bank, number as u16).await?;

        // Check if empty
        if data.is_empty() {
            return Ok(None);
        }

        // Parse memory
        let raw = RawMemory::from_bytes(&data)?;
        let mem = raw.to_memory(number)?;
        Ok(Some(mem))
    }

    /// Set a memory in the radio via serial port
    pub async fn set_memory_to_port(
        &mut self,
        port: &mut SerialPort,
        memory: &Memory,
    ) -> RadioResult<()> {
        let bank = self.band.unwrap_or(1);

        tracing::debug!(
            "set_memory_to_port: bank={}, ch={}, empty={}, freq={}, name='{}', mode='{}'",
            bank,
            memory.number,
            memory.empty,
            memory.freq,
            memory.name,
            memory.mode
        );

        if memory.empty {
            // Erase memory
            tracing::debug!("Erasing memory: bank={}, ch={}", bank, memory.number);
            self.protocol
                .erase_memory(port, bank, memory.number as u16)
                .await?;
        } else {
            // Write memory
            let data = RawMemory::from_memory(memory, bank)?;
            tracing::debug!(
                "Writing memory: bank={}, ch={}, data_len={}",
                bank,
                memory.number,
                data.len()
            );
            self.protocol
                .write_memory(port, bank, memory.number as u16, &data)
                .await?;
        }

        Ok(())
    }

    /// Download all memories from the radio
    pub async fn download_memories(
        &mut self,
        port: &mut SerialPort,
        status_fn: Option<StatusCallback>,
    ) -> RadioResult<Vec<Memory>> {
        let features = self.get_features();
        let (start, end) = features.memory_bounds;
        let mut memories = Vec::new();

        for i in start..=end {
            if let Some(callback) = &status_fn {
                callback(
                    (i - start) as usize,
                    (end - start + 1) as usize,
                    &format!("Reading memory {}", i),
                );
            }

            // Get memory from port, or create empty placeholder if slot is empty
            let mem = if let Some(mem) = self.get_memory_from_port(port, i).await? {
                mem
            } else {
                // Create empty memory for this slot so users can paste into it
                Memory::new_empty(i)
            };

            memories.push(mem);
        }

        if let Some(callback) = &status_fn {
            callback(
                (end - start + 1) as usize,
                (end - start + 1) as usize,
                "Download complete",
            );
        }

        Ok(memories)
    }

    /// Upload memories to the radio
    /// Note: Empty memories will be erased on the radio via erase_memory command
    pub async fn upload_memories(
        &mut self,
        port: &mut SerialPort,
        memories: &[Memory],
        status_fn: Option<StatusCallback>,
    ) -> RadioResult<()> {
        tracing::debug!("Uploading {} memories to IC-9700", memories.len());

        for (i, mem) in memories.iter().enumerate() {
            if let Some(callback) = &status_fn {
                let action = if mem.empty { "Erasing" } else { "Writing" };
                callback(
                    i,
                    memories.len(),
                    &format!("{} memory {}", action, mem.number),
                );
            }

            self.set_memory_to_port(port, mem).await?;
        }

        if let Some(callback) = &status_fn {
            callback(memories.len(), memories.len(), "Upload complete");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ic9700_radio_creation() {
        let radio = IC9700Radio::new();
        assert_eq!(radio.vendor(), "Icom");
        assert_eq!(radio.model(), "IC-9700");
    }

    #[test]
    fn test_ic9700_radio_band() {
        let radio = IC9700Radio::new_band(1);
        assert_eq!(radio.model(), "IC-9700 (VHF)");

        let radio = IC9700Radio::new_band(2);
        assert_eq!(radio.model(), "IC-9700 (UHF)");

        let radio = IC9700Radio::new_band(3);
        assert_eq!(radio.model(), "IC-9700 (1.2GHz)");
    }

    #[test]
    fn test_ic9700_features() {
        let radio = IC9700Radio::new_band(1);
        let features = radio.get_features();

        assert_eq!(features.memory_bounds, (1, 99));
        assert!(features.has_name);
        assert_eq!(features.valid_name_length, 16);
        assert!(features.has_dtcs);
        assert!(features.has_dtcs_polarity);
        assert!(features.has_bank);
        assert!(features.valid_modes.contains(&"DV".to_string()));
        assert!(features.valid_modes.contains(&"FM".to_string()));
    }

    #[test]
    fn test_ic9700_band3_no_dd() {
        let radio = IC9700Radio::new_band(3);
        let features = radio.get_features();

        assert!(!features.valid_modes.contains(&"DD".to_string()));
    }
}
