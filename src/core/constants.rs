// Constants used across CHIRP - tones, DTCS codes, modes, etc.
// Reference: chirp/chirp_common.py lines 30-110

/// 50 standard CTCSS tones (in Hz)
pub const TONES: [f32; 50] = [
    67.0, 69.3, 71.9, 74.4, 77.0, 79.7, 82.5, 85.4, 88.5, 91.5, 94.8, 97.4, 100.0, 103.5, 107.2,
    110.9, 114.8, 118.8, 123.0, 127.3, 131.8, 136.5, 141.3, 146.2, 151.4, 156.7, 159.8, 162.2,
    165.5, 167.9, 171.3, 173.8, 177.3, 179.9, 183.5, 186.2, 189.9, 192.8, 196.6, 199.5, 203.5,
    206.5, 210.7, 218.1, 225.7, 229.1, 233.6, 241.8, 250.3, 254.1,
];

/// 104 standard DTCS codes
pub const DTCS_CODES: [u16; 104] = [
    23, 25, 26, 31, 32, 36, 43, 47, 51, 53, 54, 65, 71, 72, 73, 74, 114, 115, 116, 122, 125, 131,
    132, 134, 143, 145, 152, 155, 156, 162, 165, 172, 174, 205, 212, 223, 225, 226, 243, 244, 245,
    246, 251, 252, 255, 261, 263, 265, 266, 271, 274, 306, 311, 315, 325, 331, 332, 343, 346, 351,
    356, 364, 365, 371, 411, 412, 413, 423, 431, 432, 445, 446, 452, 454, 455, 462, 464, 465, 466,
    503, 506, 516, 523, 526, 532, 546, 565, 606, 612, 624, 627, 631, 632, 654, 662, 664, 703, 712,
    723, 731, 732, 734, 743, 754,
];

/// All 512 possible DTCS codes (octal 000-777)
pub const ALL_DTCS_CODES: [u16; 512] = {
    let mut codes = [0u16; 512];
    let mut i = 0;
    let mut a = 0;
    while a < 8 {
        let mut b = 0;
        while b < 8 {
            let mut c = 0;
            while c < 8 {
                codes[i] = (a * 100 + b * 10 + c) as u16;
                i += 1;
                c += 1;
            }
            b += 1;
        }
        a += 1;
    }
    codes
};

/// Radio modes - master list that should remain stable
pub const MODES: &[&str] = &[
    "WFM", "FM", "NFM", "AM", "NAM", "DV", "USB", "LSB", "CW", "RTTY", "DIG", "PKT", "NCW", "NCWR",
    "CWR", "P25", "Auto", "RTTYR", "FSK", "FSKR", "DMR", "DN",
];

/// Tone modes
pub const TONE_MODES: &[&str] = &["", "Tone", "TSQL", "DTCS", "DTCS-R", "TSQL-R", "Cross"];

/// Cross-mode combinations
pub const CROSS_MODES: &[&str] = &[
    "Tone->Tone",
    "DTCS->",
    "->DTCS",
    "Tone->DTCS",
    "DTCS->Tone",
    "->Tone",
    "DTCS->DTCS",
    "Tone->",
];

/// Tuning steps (in kHz)
pub const TUNING_STEPS: &[f32] = &[
    5.0, 6.25, 10.0, 12.5, 15.0, 20.0, 25.0, 30.0, 50.0, 100.0, 125.0, 200.0, 9.0, 1.0, 2.5,
];

/// Common tuning steps (default for RadioFeatures)
pub const COMMON_TUNING_STEPS: &[f32] = &[5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 50.0, 100.0];

/// Skip values
pub const SKIP_VALUES: &[&str] = &["", "S", "P"];

/// Duplex modes
pub const DUPLEX_MODES: &[&str] = &["", "+", "-", "split", "off"];

/// DTCS polarity combinations
pub const DTCS_POLARITIES: &[&str] = &["NN", "NR", "RN", "RR"];

/// Validate a tone value
pub fn is_valid_tone(tone: f32) -> bool {
    tone > 50.0 && tone < 300.0
}

/// Validate a DTCS code
pub fn is_valid_dtcs(code: u16) -> bool {
    ALL_DTCS_CODES.contains(&code)
}

/// Validate a mode
pub fn is_valid_mode(mode: &str) -> bool {
    MODES.contains(&mode)
}

/// Validate a tone mode
pub fn is_valid_tone_mode(tmode: &str) -> bool {
    TONE_MODES.contains(&tmode)
}

/// Validate duplex
pub fn is_valid_duplex(duplex: &str) -> bool {
    DUPLEX_MODES.contains(&duplex)
}

/// Validate skip value
pub fn is_valid_skip(skip: &str) -> bool {
    SKIP_VALUES.contains(&skip)
}

/// Character sets for name validation
pub const CHARSET_UPPER_NUMERIC: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ 1234567890";
pub const CHARSET_ALPHANUMERIC: &str =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz 1234567890";
