// Binary-Coded Decimal (BCD) encoding/decoding
// Reference: chirp/bitwise.py lines 707-759

use thiserror::Error;

#[derive(Error, Debug)]
pub enum BcdError {
    #[error("Invalid BCD digit: {0:#x}")]
    InvalidDigit(u8),

    #[error("BCD array too small: need {needed} bytes, have {available}")]
    ArrayTooSmall { needed: usize, available: usize },

    #[error("Value too large for BCD array: {0}")]
    ValueTooLarge(u64),
}

pub type Result<T> = std::result::Result<T, BcdError>;

/// Convert a BCD byte to its two decimal digits (tens, ones)
/// Example: 0x12 -> (1, 2), 0x95 -> (9, 5)
pub fn bcd_byte_to_digits(byte: u8) -> Result<(u8, u8)> {
    let tens = (byte & 0xF0) >> 4;
    let ones = byte & 0x0F;

    if tens > 9 || ones > 9 {
        return Err(BcdError::InvalidDigit(byte));
    }

    Ok((tens, ones))
}

/// Convert two decimal digits to a BCD byte
/// Example: (1, 2) -> 0x12, (9, 5) -> 0x95
pub fn digits_to_bcd_byte(tens: u8, ones: u8) -> Result<u8> {
    if tens > 9 || ones > 9 {
        return Err(BcdError::InvalidDigit((tens << 4) | ones));
    }

    Ok((tens << 4) | ones)
}

/// Convert a BCD array to an integer (big-endian)
/// Example: [0x12, 0x34, 0x56] -> 123456
pub fn bcd_to_int_be(bcd_array: &[u8]) -> Result<u64> {
    let mut value: u64 = 0;

    for &byte in bcd_array {
        let (tens, ones) = bcd_byte_to_digits(byte)?;
        value = value
            .checked_mul(100)
            .ok_or(BcdError::ValueTooLarge(value))?;
        value = value
            .checked_add((tens * 10 + ones) as u64)
            .ok_or(BcdError::ValueTooLarge(value))?;
    }

    Ok(value)
}

/// Convert a BCD array to an integer (little-endian)
/// Example: [0x56, 0x34, 0x12] -> 123456
pub fn bcd_to_int_le(bcd_array: &[u8]) -> Result<u64> {
    let mut value: u64 = 0;

    for &byte in bcd_array.iter().rev() {
        let (tens, ones) = bcd_byte_to_digits(byte)?;
        value = value
            .checked_mul(100)
            .ok_or(BcdError::ValueTooLarge(value))?;
        value = value
            .checked_add((tens * 10 + ones) as u64)
            .ok_or(BcdError::ValueTooLarge(value))?;
    }

    Ok(value)
}

/// Convert an integer to BCD array (big-endian)
/// Example: 123456 -> [0x12, 0x34, 0x56]
pub fn int_to_bcd_be(value: u64, num_bytes: usize) -> Result<Vec<u8>> {
    let mut result = vec![0u8; num_bytes];
    let mut remaining = value;

    for i in (0..num_bytes).rev() {
        let two_digits = (remaining % 100) as u8;
        remaining /= 100;
        result[i] = digits_to_bcd_byte(two_digits / 10, two_digits % 10)?;
    }

    if remaining > 0 {
        return Err(BcdError::ValueTooLarge(value));
    }

    Ok(result)
}

/// Convert an integer to BCD array (little-endian)
/// Example: 123456 -> [0x56, 0x34, 0x12]
pub fn int_to_bcd_le(value: u64, num_bytes: usize) -> Result<Vec<u8>> {
    let mut result = vec![0u8; num_bytes];
    let mut remaining = value;

    for i in 0..num_bytes {
        let two_digits = (remaining % 100) as u8;
        remaining /= 100;
        result[i] = digits_to_bcd_byte(two_digits / 10, two_digits % 10)?;
    }

    if remaining > 0 {
        return Err(BcdError::ValueTooLarge(value));
    }

    Ok(result)
}

/// Convert a BCD array to an integer (automatically detecting endianness)
/// This is a convenience wrapper
pub fn bcd_to_int(bcd_array: &[u8], little_endian: bool) -> Result<u64> {
    if little_endian {
        bcd_to_int_le(bcd_array)
    } else {
        bcd_to_int_be(bcd_array)
    }
}

/// Convert an integer to BCD array (automatically detecting endianness)
pub fn int_to_bcd(value: u64, num_bytes: usize, little_endian: bool) -> Result<Vec<u8>> {
    if little_endian {
        int_to_bcd_le(value, num_bytes)
    } else {
        int_to_bcd_be(value, num_bytes)
    }
}

/// A BCD array helper for common operations
#[derive(Debug, Clone, PartialEq)]
pub struct BcdArray {
    bytes: Vec<u8>,
    little_endian: bool,
}

impl BcdArray {
    /// Create a new BCD array from bytes
    pub fn new(bytes: Vec<u8>, little_endian: bool) -> Self {
        Self {
            bytes,
            little_endian,
        }
    }

    /// Create a new BCD array from an integer value
    pub fn from_int(value: u64, num_bytes: usize, little_endian: bool) -> Result<Self> {
        let bytes = int_to_bcd(value, num_bytes, little_endian)?;
        Ok(Self {
            bytes,
            little_endian,
        })
    }

    /// Get the integer value
    pub fn to_int(&self) -> Result<u64> {
        bcd_to_int(&self.bytes, self.little_endian)
    }

    /// Get the raw bytes
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Get mutable access to bytes
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    /// Set from an integer value
    pub fn set_int(&mut self, value: u64) -> Result<()> {
        self.bytes = int_to_bcd(value, self.bytes.len(), self.little_endian)?;
        Ok(())
    }

    /// Get the number of bytes
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl std::fmt::Display for BcdArray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.to_int() {
            Ok(val) => write!(f, "{}", val),
            Err(_) => write!(f, "[invalid BCD]"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bcd_byte_conversion() {
        assert_eq!(bcd_byte_to_digits(0x12).unwrap(), (1, 2));
        assert_eq!(bcd_byte_to_digits(0x95).unwrap(), (9, 5));
        assert_eq!(bcd_byte_to_digits(0x00).unwrap(), (0, 0));

        assert!(bcd_byte_to_digits(0xAB).is_err()); // Invalid BCD

        assert_eq!(digits_to_bcd_byte(1, 2).unwrap(), 0x12);
        assert_eq!(digits_to_bcd_byte(9, 5).unwrap(), 0x95);
    }

    #[test]
    fn test_bcd_to_int_be() {
        // Big-endian: [0x12, 0x34, 0x56] = 123456
        assert_eq!(bcd_to_int_be(&[0x12, 0x34, 0x56]).unwrap(), 123456);
        assert_eq!(bcd_to_int_be(&[0x01, 0x46, 0x52]).unwrap(), 14652); // 146.52 MHz * 100
    }

    #[test]
    fn test_bcd_to_int_le() {
        // Little-endian: [0x56, 0x34, 0x12] = 123456
        assert_eq!(bcd_to_int_le(&[0x56, 0x34, 0x12]).unwrap(), 123456);
    }

    #[test]
    fn test_int_to_bcd_be() {
        assert_eq!(int_to_bcd_be(123456, 3).unwrap(), vec![0x12, 0x34, 0x56]);
        assert_eq!(int_to_bcd_be(14652, 3).unwrap(), vec![0x01, 0x46, 0x52]);

        // Value too large
        assert!(int_to_bcd_be(1234567, 3).is_err());
    }

    #[test]
    fn test_int_to_bcd_le() {
        assert_eq!(int_to_bcd_le(123456, 3).unwrap(), vec![0x56, 0x34, 0x12]);
    }

    #[test]
    fn test_bcd_array() {
        let mut bcd = BcdArray::from_int(123456, 3, false).unwrap();
        assert_eq!(bcd.to_int().unwrap(), 123456);
        assert_eq!(bcd.bytes(), &[0x12, 0x34, 0x56]);

        bcd.set_int(654321).unwrap();
        assert_eq!(bcd.to_int().unwrap(), 654321);
        assert_eq!(bcd.bytes(), &[0x65, 0x43, 0x21]);
    }

    #[test]
    fn test_frequency_bcd() {
        // Common radio frequency encoding: 146.520 MHz = 146520000 Hz
        // Often stored as 14652 in BCD (dropping trailing zeros)
        let freq_bcd = int_to_bcd_be(14652, 3).unwrap();
        assert_eq!(freq_bcd, vec![0x01, 0x46, 0x52]);

        let freq = bcd_to_int_be(&freq_bcd).unwrap();
        assert_eq!(freq, 14652);

        // Convert back to Hz
        let freq_hz = freq * 10000; // Add back the 4 trailing zeros
        assert_eq!(freq_hz, 146520000);
    }
}
