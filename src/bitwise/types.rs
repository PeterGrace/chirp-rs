// Common type definitions for binary parsing

use serde::{Deserialize, Serialize};

/// Endianness for multi-byte values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Endianness {
    Big,
    Little,
}

impl Endianness {
    pub fn is_big(&self) -> bool {
        matches!(self, Endianness::Big)
    }

    pub fn is_little(&self) -> bool {
        matches!(self, Endianness::Little)
    }
}

impl Default for Endianness {
    fn default() -> Self {
        Endianness::Big
    }
}

/// A trait for types that can be read from binary data
pub trait FromBytes: Sized {
    fn from_bytes_be(data: &[u8]) -> Result<Self, String>;
    fn from_bytes_le(data: &[u8]) -> Result<Self, String>;

    fn from_bytes(data: &[u8], endianness: Endianness) -> Result<Self, String> {
        match endianness {
            Endianness::Big => Self::from_bytes_be(data),
            Endianness::Little => Self::from_bytes_le(data),
        }
    }
}

/// A trait for types that can be written to binary data
pub trait ToBytes {
    fn to_bytes_be(&self) -> Vec<u8>;
    fn to_bytes_le(&self) -> Vec<u8>;

    fn to_bytes(&self, endianness: Endianness) -> Vec<u8> {
        match endianness {
            Endianness::Big => self.to_bytes_be(),
            Endianness::Little => self.to_bytes_le(),
        }
    }
}

// Implementations for common types

impl FromBytes for u8 {
    fn from_bytes_be(data: &[u8]) -> Result<Self, String> {
        data.get(0).copied().ok_or_else(|| "Insufficient data".to_string())
    }

    fn from_bytes_le(data: &[u8]) -> Result<Self, String> {
        Self::from_bytes_be(data)
    }
}

impl ToBytes for u8 {
    fn to_bytes_be(&self) -> Vec<u8> {
        vec![*self]
    }

    fn to_bytes_le(&self) -> Vec<u8> {
        self.to_bytes_be()
    }
}

impl FromBytes for u16 {
    fn from_bytes_be(data: &[u8]) -> Result<Self, String> {
        if data.len() < 2 {
            return Err("Insufficient data".to_string());
        }
        Ok(u16::from_be_bytes([data[0], data[1]]))
    }

    fn from_bytes_le(data: &[u8]) -> Result<Self, String> {
        if data.len() < 2 {
            return Err("Insufficient data".to_string());
        }
        Ok(u16::from_le_bytes([data[0], data[1]]))
    }
}

impl ToBytes for u16 {
    fn to_bytes_be(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }

    fn to_bytes_le(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

impl FromBytes for u32 {
    fn from_bytes_be(data: &[u8]) -> Result<Self, String> {
        if data.len() < 4 {
            return Err("Insufficient data".to_string());
        }
        Ok(u32::from_be_bytes([data[0], data[1], data[2], data[3]]))
    }

    fn from_bytes_le(data: &[u8]) -> Result<Self, String> {
        if data.len() < 4 {
            return Err("Insufficient data".to_string());
        }
        Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }
}

impl ToBytes for u32 {
    fn to_bytes_be(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }

    fn to_bytes_le(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_bytes() {
        let data = [0x12, 0x34];
        assert_eq!(u16::from_bytes_be(&data).unwrap(), 0x1234);
        assert_eq!(u16::from_bytes_le(&data).unwrap(), 0x3412);

        assert_eq!(
            u16::from_bytes(&data, Endianness::Big).unwrap(),
            0x1234
        );
        assert_eq!(
            u16::from_bytes(&data, Endianness::Little).unwrap(),
            0x3412
        );
    }

    #[test]
    fn test_to_bytes() {
        let value: u16 = 0x1234;
        assert_eq!(value.to_bytes_be(), vec![0x12, 0x34]);
        assert_eq!(value.to_bytes_le(), vec![0x34, 0x12]);

        assert_eq!(value.to_bytes(Endianness::Big), vec![0x12, 0x34]);
        assert_eq!(value.to_bytes(Endianness::Little), vec![0x34, 0x12]);
    }
}
