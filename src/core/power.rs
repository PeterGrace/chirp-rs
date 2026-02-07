// Power level abstraction
// Reference: chirp/chirp_common.py lines 178-241

use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PowerError {
    #[error("Invalid power specification: {0}")]
    InvalidFormat(String),
}

/// Represents a power level supported by a radio
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PowerLevel {
    /// Display label (e.g., "High", "5W", "50W")
    label: String,
    /// Power in dBm
    dbm: f32,
}

impl PowerLevel {
    /// Create a new power level from watts
    pub fn from_watts(label: impl Into<String>, watts: f32) -> Self {
        let dbm = watts_to_dbm(watts);
        Self {
            label: label.into(),
            dbm,
        }
    }

    /// Create a new power level from dBm
    pub fn from_dbm(label: impl Into<String>, dbm: f32) -> Self {
        Self {
            label: label.into(),
            dbm,
        }
    }

    /// Create an auto-named power level from watts (e.g., "5W", "0.5W")
    pub fn auto_named(watts: f32) -> Self {
        let label = if watts >= 10.0 {
            format!("{}W", watts as i32)
        } else {
            format!("{:.1}W", watts)
        };
        Self::from_watts(label, watts)
    }

    /// Get the power in dBm
    pub fn dbm(&self) -> f32 {
        self.dbm
    }

    /// Get the power in watts
    pub fn watts(&self) -> f32 {
        dbm_to_watts(self.dbm)
    }

    /// Get the display label
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Parse a power string (e.g., "5", "5W", "0.5W")
    pub fn parse(powerstr: &str) -> Result<Self, PowerError> {
        let powerstr = powerstr.trim();

        // Try simple integer first
        if let Ok(watts) = powerstr.parse::<i32>() {
            return Ok(Self::auto_named(watts as f32));
        }

        // Try regex-like parsing
        let re = regex::Regex::new(r"^\s*([0-9.]+)\s*([Ww]?)\s*$").unwrap();
        if let Some(caps) = re.captures(powerstr) {
            let value: f32 = caps[1]
                .parse()
                .map_err(|_| PowerError::InvalidFormat(powerstr.to_string()))?;

            let unit = caps.get(2).map_or("", |m| m.as_str());
            if unit.is_empty() || unit.eq_ignore_ascii_case("w") {
                return Ok(Self::auto_named(value));
            }
        }

        Err(PowerError::InvalidFormat(powerstr.to_string()))
    }
}

impl fmt::Display for PowerLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label)
    }
}

impl PartialOrd for PowerLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.dbm.partial_cmp(&other.dbm)
    }
}

/// Convert watts to dBm
pub fn watts_to_dbm(watts: f32) -> f32 {
    10.0 * watts.log10() + 30.0
}

/// Convert dBm to watts
pub fn dbm_to_watts(dbm: f32) -> f32 {
    (10.0_f32.powf(dbm / 10.0) / 1000.0 * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_conversions() {
        assert_eq!(watts_to_dbm(1.0), 30.0);
        assert_eq!(watts_to_dbm(5.0), 36.98970004336019);
        assert!((dbm_to_watts(30.0) - 1.0).abs() < 0.01);
        assert!((dbm_to_watts(37.0) - 5.0).abs() < 0.1);
    }

    #[test]
    fn test_power_level_creation() {
        let p = PowerLevel::from_watts("High", 5.0);
        assert_eq!(p.label(), "High");
        assert!((p.watts() - 5.0).abs() < 0.1);

        let p = PowerLevel::auto_named(5.0);
        assert_eq!(p.label(), "5.0W");

        let p = PowerLevel::auto_named(10.0);
        assert_eq!(p.label(), "10W");

        let p = PowerLevel::auto_named(0.5);
        assert_eq!(p.label(), "0.5W");
    }

    #[test]
    fn test_power_parse() {
        let p = PowerLevel::parse("5").unwrap();
        assert_eq!(p.label(), "5.0W");

        let p = PowerLevel::parse("10").unwrap();
        assert_eq!(p.label(), "10W");

        let p = PowerLevel::parse("5W").unwrap();
        assert_eq!(p.label(), "5.0W");

        let p = PowerLevel::parse("0.5W").unwrap();
        assert_eq!(p.label(), "0.5W");
    }
}
