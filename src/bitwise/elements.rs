// Data element reading functions for various integer types
// Reference: chirp/bitwise.py lines 500-683

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ElementError {
    #[error("Insufficient data: expected {expected} bytes, got {actual}")]
    InsufficientData { expected: usize, actual: usize },

    #[error("Invalid value: {0}")]
    InvalidValue(String),
}

pub type Result<T> = std::result::Result<T, ElementError>;

/// Read a u16 in big-endian format
pub fn read_u16_be(data: &[u8]) -> Result<u16> {
    if data.len() < 2 {
        return Err(ElementError::InsufficientData {
            expected: 2,
            actual: data.len(),
        });
    }
    Ok(u16::from_be_bytes([data[0], data[1]]))
}

/// Read a u16 in little-endian format
pub fn read_u16_le(data: &[u8]) -> Result<u16> {
    if data.len() < 2 {
        return Err(ElementError::InsufficientData {
            expected: 2,
            actual: data.len(),
        });
    }
    Ok(u16::from_le_bytes([data[0], data[1]]))
}

/// Read a u24 (3 bytes) in big-endian format
pub fn read_u24_be(data: &[u8]) -> Result<u32> {
    if data.len() < 3 {
        return Err(ElementError::InsufficientData {
            expected: 3,
            actual: data.len(),
        });
    }
    Ok(u32::from_be_bytes([0, data[0], data[1], data[2]]))
}

/// Read a u24 (3 bytes) in little-endian format
pub fn read_u24_le(data: &[u8]) -> Result<u32> {
    if data.len() < 3 {
        return Err(ElementError::InsufficientData {
            expected: 3,
            actual: data.len(),
        });
    }
    Ok(u32::from_le_bytes([data[0], data[1], data[2], 0]))
}

/// Read a u32 in big-endian format
pub fn read_u32_be(data: &[u8]) -> Result<u32> {
    if data.len() < 4 {
        return Err(ElementError::InsufficientData {
            expected: 4,
            actual: data.len(),
        });
    }
    Ok(u32::from_be_bytes([data[0], data[1], data[2], data[3]]))
}

/// Read a u32 in little-endian format
pub fn read_u32_le(data: &[u8]) -> Result<u32> {
    if data.len() < 4 {
        return Err(ElementError::InsufficientData {
            expected: 4,
            actual: data.len(),
        });
    }
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Read a i16 in big-endian format
pub fn read_i16_be(data: &[u8]) -> Result<i16> {
    if data.len() < 2 {
        return Err(ElementError::InsufficientData {
            expected: 2,
            actual: data.len(),
        });
    }
    Ok(i16::from_be_bytes([data[0], data[1]]))
}

/// Read a i16 in little-endian format
pub fn read_i16_le(data: &[u8]) -> Result<i16> {
    if data.len() < 2 {
        return Err(ElementError::InsufficientData {
            expected: 2,
            actual: data.len(),
        });
    }
    Ok(i16::from_le_bytes([data[0], data[1]]))
}

/// Read a i24 (3 bytes) in big-endian format
pub fn read_i24_be(data: &[u8]) -> Result<i32> {
    if data.len() < 3 {
        return Err(ElementError::InsufficientData {
            expected: 3,
            actual: data.len(),
        });
    }
    // Sign extend from 24 bits to 32 bits
    let value = i32::from_be_bytes([0, data[0], data[1], data[2]]);
    // Check if sign bit (bit 23) is set
    if (data[0] & 0x80) != 0 {
        // Sign extend by setting upper 8 bits to 1
        Ok(value | 0xFF000000_u32 as i32)
    } else {
        Ok(value)
    }
}

/// Read a i24 (3 bytes) in little-endian format
pub fn read_i24_le(data: &[u8]) -> Result<i32> {
    if data.len() < 3 {
        return Err(ElementError::InsufficientData {
            expected: 3,
            actual: data.len(),
        });
    }
    // Sign extend from 24 bits to 32 bits
    let value = i32::from_le_bytes([data[0], data[1], data[2], 0]);
    // Check if sign bit (bit 23) is set
    if (data[2] & 0x80) != 0 {
        // Sign extend by setting upper 8 bits to 1
        Ok(value | 0xFF000000_u32 as i32)
    } else {
        Ok(value)
    }
}

/// Read a i32 in big-endian format
pub fn read_i32_be(data: &[u8]) -> Result<i32> {
    if data.len() < 4 {
        return Err(ElementError::InsufficientData {
            expected: 4,
            actual: data.len(),
        });
    }
    Ok(i32::from_be_bytes([data[0], data[1], data[2], data[3]]))
}

/// Read a i32 in little-endian format
pub fn read_i32_le(data: &[u8]) -> Result<i32> {
    if data.len() < 4 {
        return Err(ElementError::InsufficientData {
            expected: 4,
            actual: data.len(),
        });
    }
    Ok(i32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Write a u16 in big-endian format
pub fn write_u16_be(value: u16) -> [u8; 2] {
    value.to_be_bytes()
}

/// Write a u16 in little-endian format
pub fn write_u16_le(value: u16) -> [u8; 2] {
    value.to_le_bytes()
}

/// Write a u24 in big-endian format
pub fn write_u24_be(value: u32) -> [u8; 3] {
    let bytes = value.to_be_bytes();
    [bytes[1], bytes[2], bytes[3]]
}

/// Write a u24 in little-endian format
pub fn write_u24_le(value: u32) -> [u8; 3] {
    let bytes = value.to_le_bytes();
    [bytes[0], bytes[1], bytes[2]]
}

/// Write a u32 in big-endian format
pub fn write_u32_be(value: u32) -> [u8; 4] {
    value.to_be_bytes()
}

/// Write a u32 in little-endian format
pub fn write_u32_le(value: u32) -> [u8; 4] {
    value.to_le_bytes()
}

/// Write a i16 in big-endian format
pub fn write_i16_be(value: i16) -> [u8; 2] {
    value.to_be_bytes()
}

/// Write a i16 in little-endian format
pub fn write_i16_le(value: i16) -> [u8; 2] {
    value.to_le_bytes()
}

/// Write a i24 in big-endian format
pub fn write_i24_be(value: i32) -> [u8; 3] {
    let bytes = value.to_be_bytes();
    [bytes[1], bytes[2], bytes[3]]
}

/// Write a i24 in little-endian format
pub fn write_i24_le(value: i32) -> [u8; 3] {
    let bytes = value.to_le_bytes();
    [bytes[0], bytes[1], bytes[2]]
}

/// Write a i32 in big-endian format
pub fn write_i32_be(value: i32) -> [u8; 4] {
    value.to_be_bytes()
}

/// Write a i32 in little-endian format
pub fn write_i32_le(value: i32) -> [u8; 4] {
    value.to_le_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u16_read_write() {
        let data_be = [0x12, 0x34];
        assert_eq!(read_u16_be(&data_be).unwrap(), 0x1234);
        assert_eq!(write_u16_be(0x1234), data_be);

        let data_le = [0x34, 0x12];
        assert_eq!(read_u16_le(&data_le).unwrap(), 0x1234);
        assert_eq!(write_u16_le(0x1234), data_le);
    }

    #[test]
    fn test_u24_read_write() {
        let data_be = [0x12, 0x34, 0x56];
        assert_eq!(read_u24_be(&data_be).unwrap(), 0x123456);
        assert_eq!(write_u24_be(0x123456), data_be);

        let data_le = [0x56, 0x34, 0x12];
        assert_eq!(read_u24_le(&data_le).unwrap(), 0x123456);
        assert_eq!(write_u24_le(0x123456), data_le);
    }

    #[test]
    fn test_u32_read_write() {
        let data_be = [0x12, 0x34, 0x56, 0x78];
        assert_eq!(read_u32_be(&data_be).unwrap(), 0x12345678);
        assert_eq!(write_u32_be(0x12345678), data_be);

        let data_le = [0x78, 0x56, 0x34, 0x12];
        assert_eq!(read_u32_le(&data_le).unwrap(), 0x12345678);
        assert_eq!(write_u32_le(0x12345678), data_le);
    }

    #[test]
    fn test_i16_read_write() {
        // Positive number
        let data = [0x12, 0x34];
        assert_eq!(read_i16_be(&data).unwrap(), 0x1234);

        // Negative number
        let data = [0xFF, 0xFE];
        assert_eq!(read_i16_be(&data).unwrap(), -2);
        assert_eq!(write_i16_be(-2), data);
    }

    #[test]
    fn test_i24_read_write() {
        // Positive number
        let data = [0x12, 0x34, 0x56];
        assert_eq!(read_i24_be(&data).unwrap(), 0x123456);
        assert_eq!(write_i24_be(0x123456), data);

        // Negative number (sign bit set)
        let data = [0xFF, 0xFF, 0xFE];
        assert_eq!(read_i24_be(&data).unwrap(), -2);
    }

    #[test]
    fn test_insufficient_data() {
        let data = [0x12];
        assert!(read_u16_be(&data).is_err());
        assert!(read_u24_be(&data).is_err());
        assert!(read_u32_be(&data).is_err());
    }
}
