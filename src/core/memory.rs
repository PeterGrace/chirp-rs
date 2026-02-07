// Memory structure representing a single radio memory channel
// Reference: chirp/chirp_common.py lines 280-645

use super::power::PowerLevel;
use crate::core::constants::*;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Invalid frequency: {0}")]
    InvalidFrequency(String),

    #[error("Invalid tone: {0}")]
    InvalidTone(f32),

    #[error("Invalid DTCS code: {0}")]
    InvalidDtcs(u16),

    #[error("Invalid mode: {0}")]
    InvalidMode(String),

    #[error("Invalid tone mode: {0}")]
    InvalidToneMode(String),

    #[error("Invalid duplex: {0}")]
    InvalidDuplex(String),

    #[error("Invalid skip value: {0}")]
    InvalidSkip(String),

    #[error("Field {0} is immutable")]
    ImmutableField(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;

/// Base structure for a single radio memory channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Memory channel number
    pub number: u32,

    /// Extended number (for radios with alphanumeric channel numbers)
    pub extd_number: String,

    /// Channel name
    pub name: String,

    /// Frequency in Hz
    pub freq: u64,

    /// VFO number (0 = VFO A, 1 = VFO B, etc.)
    pub vfo: u8,

    /// Transmit tone (CTCSS) in Hz
    pub rtone: f32,

    /// Receive tone (CTCSS) in Hz
    pub ctone: f32,

    /// DTCS code for transmit
    pub dtcs: u16,

    /// DTCS code for receive
    pub rx_dtcs: u16,

    /// Tone mode ("", "Tone", "TSQL", "DTCS", "DTCS-R", "TSQL-R", "Cross")
    pub tmode: String,

    /// Cross mode for complex tone setups
    pub cross_mode: String,

    /// DTCS polarity ("NN", "NR", "RN", "RR")
    pub dtcs_polarity: String,

    /// Skip flag ("", "S" for skip, "P" for priority)
    pub skip: String,

    /// Power level
    pub power: Option<PowerLevel>,

    /// Duplex ("", "+", "-", "split", "off")
    pub duplex: String,

    /// Offset frequency in Hz
    pub offset: u64,

    /// Mode (e.g., "FM", "NFM", "AM", "USB", "LSB", "DV")
    pub mode: String,

    /// Tuning step in kHz
    pub tuning_step: f32,

    /// Comment
    pub comment: String,

    /// Whether this memory is empty
    pub empty: bool,

    /// List of immutable fields
    pub immutable: Vec<String>,

    // D-STAR/DV fields (empty for non-DV memories)
    /// D-STAR URCALL (destination callsign)
    pub dv_urcall: String,

    /// D-STAR RPT1CALL (repeater 1)
    pub dv_rpt1call: String,

    /// D-STAR RPT2CALL (repeater 2)
    pub dv_rpt2call: String,

    /// D-STAR digital code
    pub dv_code: u8,
}

impl Default for Memory {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Memory {
    /// Create a new memory with default values
    pub fn new(number: u32) -> Self {
        Self {
            number,
            extd_number: String::new(),
            name: String::new(),
            freq: 0,
            vfo: 0,
            rtone: 88.5,
            ctone: 88.5,
            dtcs: 23,
            rx_dtcs: 23,
            tmode: String::new(),
            cross_mode: "Tone->Tone".to_string(),
            dtcs_polarity: "NN".to_string(),
            skip: String::new(),
            power: None,
            duplex: String::new(),
            offset: 600000, // 600 kHz default
            mode: "FM".to_string(),
            tuning_step: 5.0,
            comment: String::new(),
            empty: false,
            immutable: Vec::new(),
            dv_urcall: String::new(),
            dv_rpt1call: String::new(),
            dv_rpt2call: String::new(),
            dv_code: 0,
        }
    }

    /// Create an empty memory
    pub fn new_empty(number: u32) -> Self {
        let mut mem = Self::new(number);
        mem.empty = true;
        mem
    }

    /// Parse a frequency string and return Hz
    /// Supports formats: "146.520", "146.520 MHz", "146520 kHz"
    pub fn parse_freq(freqstr: &str) -> Result<u64> {
        let freqstr = freqstr.trim();

        if freqstr.is_empty() {
            return Ok(0);
        }

        // Handle MHz suffix
        if let Some(stripped) = freqstr.strip_suffix(" MHz") {
            return Self::parse_freq(stripped);
        }

        // Handle kHz suffix
        if let Some(stripped) = freqstr.strip_suffix(" kHz") {
            let khz: u64 = stripped
                .parse()
                .map_err(|_| MemoryError::InvalidFrequency(freqstr.to_string()))?;
            return Ok(khz * 1000);
        }

        // Parse decimal format (e.g., "146.520")
        if freqstr.contains('.') {
            let parts: Vec<&str> = freqstr.split('.').collect();
            if parts.len() != 2 {
                return Err(MemoryError::InvalidFrequency(freqstr.to_string()));
            }

            let mhz_str = if parts[0].is_empty() { "0" } else { parts[0] };
            let khz_str = format!("{:0<6}", parts[1]); // Left-pad to 6 digits

            if khz_str.len() > 6 {
                return Err(MemoryError::InvalidFrequency(format!(
                    "Invalid kHz value: {}",
                    parts[1]
                )));
            }

            let mhz: u64 = mhz_str
                .parse()
                .map_err(|_| MemoryError::InvalidFrequency(freqstr.to_string()))?;
            let khz: u64 = khz_str
                .parse()
                .map_err(|_| MemoryError::InvalidFrequency(freqstr.to_string()))?;

            Ok(mhz * 1_000_000 + khz)
        } else {
            // Integer MHz
            let mhz: u64 = freqstr
                .parse()
                .map_err(|_| MemoryError::InvalidFrequency(freqstr.to_string()))?;
            Ok(mhz * 1_000_000)
        }
    }

    /// Format frequency in Hz as a string (e.g., "146.520000")
    pub fn format_freq(freq: u64) -> String {
        format!("{}.{:06}", freq / 1_000_000, freq % 1_000_000)
    }

    /// Set frequency from a string
    pub fn set_freq_str(&mut self, freqstr: &str) -> Result<()> {
        self.freq = Self::parse_freq(freqstr)?;
        Ok(())
    }

    /// Get formatted frequency string
    pub fn freq_str(&self) -> String {
        Self::format_freq(self.freq)
    }

    /// Validate all fields
    pub fn validate(&self) -> Result<()> {
        // Validate tones
        if !self.tmode.is_empty() && !is_valid_tone_mode(&self.tmode) {
            return Err(MemoryError::InvalidToneMode(self.tmode.clone()));
        }

        if !is_valid_tone(self.rtone) {
            return Err(MemoryError::InvalidTone(self.rtone));
        }

        if !is_valid_tone(self.ctone) {
            return Err(MemoryError::InvalidTone(self.ctone));
        }

        // Validate DTCS
        if !is_valid_dtcs(self.dtcs) {
            return Err(MemoryError::InvalidDtcs(self.dtcs));
        }

        if !is_valid_dtcs(self.rx_dtcs) {
            return Err(MemoryError::InvalidDtcs(self.rx_dtcs));
        }

        // Validate mode
        if !is_valid_mode(&self.mode) {
            return Err(MemoryError::InvalidMode(self.mode.clone()));
        }

        // Validate duplex
        if !is_valid_duplex(&self.duplex) {
            return Err(MemoryError::InvalidDuplex(self.duplex.clone()));
        }

        // Validate skip
        if !is_valid_skip(&self.skip) {
            return Err(MemoryError::InvalidSkip(self.skip.clone()));
        }

        Ok(())
    }

    /// Clone this memory
    pub fn clone_mem(&self) -> Self {
        self.clone()
    }

    /// CSV header format
    pub const CSV_HEADER: &'static [&'static str] = &[
        "Location",
        "Name",
        "Frequency",
        "Duplex",
        "Offset",
        "Tone",
        "rToneFreq",
        "cToneFreq",
        "DtcsCode",
        "DtcsPolarity",
        "RxDtcsCode",
        "CrossMode",
        "Mode",
        "TStep",
        "Skip",
        "Power",
        "Comment",
        "URCALL",
        "RPT1CALL",
        "RPT2CALL",
        "DVCODE",
    ];

    /// Export to CSV row
    pub fn to_csv(&self) -> Vec<String> {
        vec![
            format!("{}", self.number),
            self.name.clone(),
            Self::format_freq(self.freq),
            self.duplex.clone(),
            Self::format_freq(self.offset),
            self.tmode.clone(),
            format!("{:.1}", self.rtone),
            format!("{:.1}", self.ctone),
            format!("{:03}", self.dtcs),
            self.dtcs_polarity.clone(),
            format!("{:03}", self.rx_dtcs),
            self.cross_mode.clone(),
            self.mode.clone(),
            format!("{:.2}", self.tuning_step),
            self.skip.clone(),
            self.power
                .as_ref()
                .map(|p| p.to_string())
                .unwrap_or_default(),
            self.comment.clone(),
            self.dv_urcall.clone(),
            self.dv_rpt1call.clone(),
            self.dv_rpt2call.clone(),
            format!("{}", self.dv_code),
        ]
    }
}

impl fmt::Display for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tenc = if self.tmode == "Tone" { "*" } else { " " };
        let tsql = if self.tmode == "TSQL" { "*" } else { " " };
        let dtcs = if self.tmode == "DTCS" { "*" } else { " " };
        let dup = if self.duplex.is_empty() {
            "/"
        } else {
            &self.duplex
        };

        write!(
            f,
            "Memory {}: {}{}{} {} ({}) r{:.1}{} c{:.1}{} d{:03}{}{} [{:.2}]",
            if self.extd_number.is_empty() {
                self.number.to_string()
            } else {
                self.extd_number.clone()
            },
            Self::format_freq(self.freq),
            dup,
            Self::format_freq(self.offset),
            self.mode,
            self.name,
            self.rtone,
            tenc,
            self.ctone,
            tsql,
            self.dtcs,
            dtcs,
            self.dtcs_polarity,
            self.tuning_step
        )
    }
}

/// D-STAR memory with additional fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DVMemory {
    /// Base memory fields
    #[serde(flatten)]
    pub base: Memory,

    /// D-STAR URCALL (destination callsign)
    pub dv_urcall: String,

    /// D-STAR RPT1CALL (repeater 1)
    pub dv_rpt1call: String,

    /// D-STAR RPT2CALL (repeater 2)
    pub dv_rpt2call: String,

    /// D-STAR digital code
    pub dv_code: u8,
}

impl Default for DVMemory {
    fn default() -> Self {
        Self::new(0)
    }
}

impl DVMemory {
    /// Create a new D-STAR memory
    pub fn new(number: u32) -> Self {
        Self {
            base: Memory::new(number),
            dv_urcall: "CQCQCQ".to_string(),
            dv_rpt1call: String::new(),
            dv_rpt2call: String::new(),
            dv_code: 0,
        }
    }

    /// Export to CSV row (includes D-STAR fields)
    pub fn to_csv(&self) -> Vec<String> {
        let mut csv = self.base.to_csv();
        // Replace the empty D-STAR fields
        csv[17] = self.dv_urcall.clone();
        csv[18] = self.dv_rpt1call.clone();
        csv[19] = self.dv_rpt2call.clone();
        csv[20] = format!("{}", self.dv_code);
        csv
    }
}

impl fmt::Display for DVMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} <{},{},{}>",
            self.base, self.dv_urcall, self.dv_rpt1call, self.dv_rpt2call
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_freq() {
        assert_eq!(Memory::parse_freq("146.520").unwrap(), 146_520_000);
        assert_eq!(Memory::parse_freq("146.520 MHz").unwrap(), 146_520_000);
        assert_eq!(Memory::parse_freq("146520 kHz").unwrap(), 146_520_000);
        assert_eq!(Memory::parse_freq("146").unwrap(), 146_000_000);
        assert_eq!(Memory::parse_freq(".520").unwrap(), 520_000);
        assert_eq!(Memory::parse_freq("").unwrap(), 0);
    }

    #[test]
    fn test_format_freq() {
        assert_eq!(Memory::format_freq(146_520_000), "146.520000");
        assert_eq!(Memory::format_freq(146_000_000), "146.000000");
        assert_eq!(Memory::format_freq(520_000), "0.520000");
    }

    #[test]
    fn test_memory_creation() {
        let mem = Memory::new(1);
        assert_eq!(mem.number, 1);
        assert_eq!(mem.mode, "FM");
        assert_eq!(mem.rtone, 88.5);
        assert!(!mem.empty);

        let empty = Memory::new_empty(2);
        assert!(empty.empty);
    }

    #[test]
    fn test_dv_memory() {
        let dv = DVMemory::new(10);
        assert_eq!(dv.base.number, 10);
        assert_eq!(dv.dv_urcall, "CQCQCQ");
    }

    #[test]
    fn test_validation() {
        let mut mem = Memory::new(1);
        mem.freq = 146_520_000;
        assert!(mem.validate().is_ok());

        mem.mode = "INVALID".to_string();
        assert!(mem.validate().is_err());
    }
}
