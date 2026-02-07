// Radio feature flags and capabilities
// Reference: chirp/chirp_common.py lines 891-1211

use super::constants::*;
use super::memory::Memory;
use super::power::PowerLevel;
use serde::{Deserialize, Serialize};

/// Radio feature flags describing what a radio supports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioFeatures {
    // General capability flags
    pub has_bank_index: bool,
    pub has_dtcs: bool,
    pub has_rx_dtcs: bool,
    pub has_dtcs_polarity: bool,
    pub has_mode: bool,
    pub has_offset: bool,
    pub has_name: bool,
    pub has_bank: bool,
    pub has_bank_names: bool,
    pub has_tuning_step: bool,
    pub has_ctone: bool,
    pub has_cross: bool,
    pub has_infinite_number: bool,
    pub has_nostep_tuning: bool,
    pub has_comment: bool,
    pub has_settings: bool,
    pub has_variable_power: bool,
    pub has_dynamic_subdevices: bool,
    pub has_sub_devices: bool,
    pub can_odd_split: bool,
    pub can_delete: bool,

    // D-STAR specific
    pub requires_call_lists: bool,
    pub has_implicit_calls: bool,

    // Valid values/lists
    pub valid_modes: Vec<String>,
    pub valid_tmodes: Vec<String>,
    pub valid_duplexes: Vec<String>,
    pub valid_tuning_steps: Vec<f32>,
    pub valid_bands: Vec<(u64, u64)>, // (low_hz, high_hz) pairs
    pub valid_skips: Vec<String>,
    pub valid_power_levels: Vec<PowerLevel>,
    pub valid_characters: String,
    pub valid_name_length: usize,
    pub valid_cross_modes: Vec<String>,
    pub valid_tones: Vec<f32>,
    pub valid_dtcs_pols: Vec<String>,
    pub valid_dtcs_codes: Vec<u16>,
    pub valid_special_chans: Vec<String>,

    /// Memory bounds (min, max)
    pub memory_bounds: (u32, u32),
}

impl Default for RadioFeatures {
    fn default() -> Self {
        Self {
            // Feature flags - default values from Python
            has_bank_index: false,
            has_dtcs: true,
            has_rx_dtcs: false,
            has_dtcs_polarity: true,
            has_mode: true,
            has_offset: true,
            has_name: true,
            has_bank: true,
            has_bank_names: false,
            has_tuning_step: true,
            has_ctone: true,
            has_cross: false,
            has_infinite_number: false,
            has_nostep_tuning: false,
            has_comment: false,
            has_settings: false,
            has_variable_power: false,
            has_dynamic_subdevices: false,
            has_sub_devices: false,
            can_odd_split: false,
            can_delete: true,
            requires_call_lists: true,
            has_implicit_calls: false,

            // Valid values - sensible defaults
            valid_modes: MODES.iter().map(|s| s.to_string()).collect(),
            valid_tmodes: Vec::new(),
            valid_duplexes: vec!["".to_string(), "+".to_string(), "-".to_string()],
            valid_tuning_steps: COMMON_TUNING_STEPS.to_vec(),
            valid_bands: Vec::new(),
            valid_skips: vec!["".to_string(), "S".to_string()],
            valid_power_levels: Vec::new(),
            valid_characters: CHARSET_UPPER_NUMERIC.to_string(),
            valid_name_length: 6,
            valid_cross_modes: CROSS_MODES.iter().map(|s| s.to_string()).collect(),
            valid_tones: TONES.to_vec(),
            valid_dtcs_pols: DTCS_POLARITIES.iter().map(|s| s.to_string()).collect(),
            valid_dtcs_codes: DTCS_CODES.to_vec(),
            valid_special_chans: Vec::new(),
            memory_bounds: (0, 1),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ValidationMessage {
    Warning(String),
    Error(String),
}

impl ValidationMessage {
    pub fn is_error(&self) -> bool {
        matches!(self, ValidationMessage::Error(_))
    }

    pub fn is_warning(&self) -> bool {
        matches!(self, ValidationMessage::Warning(_))
    }

    pub fn message(&self) -> &str {
        match self {
            ValidationMessage::Warning(msg) | ValidationMessage::Error(msg) => msg,
        }
    }
}

impl RadioFeatures {
    /// Create a new RadioFeatures with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a concise string representation of valid bands
    pub fn concise_bands(&self) -> String {
        self.valid_bands
            .iter()
            .map(|(lo, hi)| {
                format!(
                    "{}-{}MHz",
                    Memory::format_freq(*lo)
                        .trim_end_matches('0')
                        .trim_end_matches('.'),
                    Memory::format_freq(*hi)
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Validate a memory against this radio's features
    /// Returns a list of warnings and errors
    pub fn validate_memory(&self, mem: &Memory) -> Vec<ValidationMessage> {
        let mut msgs = Vec::new();

        // Check memory bounds
        let (lo, hi) = self.memory_bounds;
        if !self.has_infinite_number
            && (mem.number < lo || mem.number > hi)
            && !self.valid_special_chans.contains(&mem.extd_number)
        {
            msgs.push(ValidationMessage::Warning(format!(
                "Location {} is out of range",
                mem.number
            )));
        }

        // Check mode
        if !self.valid_modes.is_empty()
            && !self.valid_modes.contains(&mem.mode)
            && !mem.immutable.contains(&"mode".to_string())
            && mem.mode != "Auto"
        {
            msgs.push(ValidationMessage::Error(format!(
                "Mode {} not supported",
                mem.mode
            )));
        }

        // Check tone mode
        if !self.valid_tmodes.is_empty() && !self.valid_tmodes.contains(&mem.tmode) {
            msgs.push(ValidationMessage::Error(format!(
                "Tone mode {} not supported",
                mem.tmode
            )));
        } else if mem.tmode == "Cross" {
            // Check cross mode
            if !self.valid_cross_modes.is_empty()
                && !self.valid_cross_modes.contains(&mem.cross_mode)
            {
                msgs.push(ValidationMessage::Error(format!(
                    "Cross tone mode {} not supported",
                    mem.cross_mode
                )));
            }
        }

        // Check tones
        if !self.valid_tones.is_empty() {
            if !self.valid_tones.contains(&mem.rtone) {
                msgs.push(ValidationMessage::Error(format!(
                    "Tone {:.1} not supported",
                    mem.rtone
                )));
            }
            if !self.valid_tones.contains(&mem.ctone) {
                msgs.push(ValidationMessage::Error(format!(
                    "Tone {:.1} not supported",
                    mem.ctone
                )));
            }
        }

        // Check DTCS polarity
        if self.has_dtcs_polarity && !self.valid_dtcs_pols.contains(&mem.dtcs_polarity) {
            msgs.push(ValidationMessage::Error(format!(
                "DTCS Polarity {} not supported",
                mem.dtcs_polarity
            )));
        }

        // Check DTCS codes
        if !self.valid_dtcs_codes.is_empty() {
            if !self.valid_dtcs_codes.contains(&mem.dtcs) {
                msgs.push(ValidationMessage::Error(format!(
                    "DTCS Code {:03} not supported",
                    mem.dtcs
                )));
            }
            if !self.valid_dtcs_codes.contains(&mem.rx_dtcs) {
                msgs.push(ValidationMessage::Error(format!(
                    "DTCS Code {:03} not supported",
                    mem.rx_dtcs
                )));
            }
        }

        // Check duplex
        if !self.valid_duplexes.is_empty() && !self.valid_duplexes.contains(&mem.duplex) {
            msgs.push(ValidationMessage::Error(format!(
                "Duplex {} not supported",
                mem.duplex
            )));
        }

        // Check tuning step
        if !self.valid_tuning_steps.is_empty()
            && !self.valid_tuning_steps.contains(&mem.tuning_step)
            && !self.has_nostep_tuning
        {
            msgs.push(ValidationMessage::Error(format!(
                "Tuning step {:.2} not supported",
                mem.tuning_step
            )));
        }

        // Check frequency band
        if !self.valid_bands.is_empty() {
            let mut valid = false;
            for (lo, hi) in &self.valid_bands {
                if mem.freq >= *lo && mem.freq < *hi {
                    valid = true;
                    break;
                }
            }
            if !valid {
                msgs.push(ValidationMessage::Error(format!(
                    "Frequency {} is out of supported ranges {}",
                    Memory::format_freq(mem.freq),
                    self.concise_bands()
                )));
            }
        }

        // Check TX frequency (for split/offset)
        if !self.valid_bands.is_empty()
            && !self.valid_duplexes.is_empty()
            && (mem.duplex == "split" || mem.duplex == "-" || mem.duplex == "+")
        {
            let tx_freq = match mem.duplex.as_str() {
                "split" => mem.offset,
                "-" => mem.freq.saturating_sub(mem.offset),
                "+" => mem.freq + mem.offset,
                _ => mem.freq,
            };

            let mut valid = false;
            for (lo, hi) in &self.valid_bands {
                if tx_freq >= *lo && tx_freq < *hi {
                    valid = true;
                    break;
                }
            }
            if !valid {
                msgs.push(ValidationMessage::Error(format!(
                    "TX freq {} is out of supported range",
                    Memory::format_freq(tx_freq)
                )));
            }
        }

        // Check power level
        if let Some(ref power) = mem.power {
            if !self.valid_power_levels.is_empty() {
                if self.has_variable_power {
                    let min_power = self
                        .valid_power_levels
                        .iter()
                        .min_by(|a, b| a.partial_cmp(b).unwrap());
                    let max_power = self
                        .valid_power_levels
                        .iter()
                        .max_by(|a, b| a.partial_cmp(b).unwrap());

                    if let (Some(min), Some(max)) = (min_power, max_power) {
                        if power < min || power > max {
                            msgs.push(ValidationMessage::Warning(format!(
                                "Power level {} is out of radio's range",
                                power
                            )));
                        }
                    }
                } else if !self.valid_power_levels.contains(power) {
                    msgs.push(ValidationMessage::Warning(format!(
                        "Power level {} not supported",
                        power
                    )));
                }
            }
        }

        // Check name characters
        if !self.valid_characters.is_empty() {
            for ch in mem.name.chars() {
                if !self.valid_characters.contains(ch) {
                    msgs.push(ValidationMessage::Warning(format!(
                        "Name character '{}' not supported",
                        ch
                    )));
                    break;
                }
            }
        }

        msgs
    }

    /// Split validation messages into warnings and errors
    pub fn split_messages(msgs: &[ValidationMessage]) -> (Vec<String>, Vec<String>) {
        let warnings = msgs
            .iter()
            .filter_map(|m| match m {
                ValidationMessage::Warning(s) => Some(s.clone()),
                _ => None,
            })
            .collect();

        let errors = msgs
            .iter()
            .filter_map(|m| match m {
                ValidationMessage::Error(s) => Some(s.clone()),
                _ => None,
            })
            .collect();

        (warnings, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_features() {
        let rf = RadioFeatures::default();
        assert!(rf.has_dtcs);
        assert!(rf.has_mode);
        assert!(!rf.has_rx_dtcs);
        assert_eq!(rf.valid_name_length, 6);
    }

    #[test]
    fn test_validation() {
        let mut rf = RadioFeatures::default();
        rf.valid_modes = vec!["FM".to_string(), "AM".to_string()];
        rf.valid_tmodes = vec!["".to_string(), "Tone".to_string()];
        rf.memory_bounds = (1, 200);

        let mut mem = Memory::new(1);
        mem.freq = 146_520_000;
        mem.mode = "FM".to_string();
        mem.tmode = "Tone".to_string();

        let msgs = rf.validate_memory(&mem);
        assert!(msgs.is_empty());

        // Invalid mode
        mem.mode = "INVALID".to_string();
        let msgs = rf.validate_memory(&mem);
        assert!(!msgs.is_empty());
        assert!(msgs[0].is_error());
    }
}
