// Memory map for storing radio's binary data
// Reference: chirp/memmap.py

use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryMapError {
    #[error("Index out of bounds: {0}")]
    IndexOutOfBounds(usize),

    #[error("Invalid value type")]
    InvalidValueType,
}

pub type Result<T> = std::result::Result<T, MemoryMapError>;

/// Memory map for storing radio binary data
/// This is a byte-oriented storage (unlike Python's string-based version)
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryMap {
    data: Vec<u8>,
}

impl MemoryMap {
    /// Create a new memory map from bytes
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Create a new memory map with a specific size, filled with zeros
    pub fn new_with_size(size: usize) -> Self {
        Self {
            data: vec![0u8; size],
        }
    }

    /// Create a new empty memory map
    pub fn new_empty() -> Self {
        Self { data: Vec::new() }
    }

    /// Get the size of the memory map
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the memory map is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get a chunk of memory from @start for @length bytes
    /// If length is None, returns all data from @start to end
    pub fn get(&self, start: usize, length: Option<usize>) -> Result<&[u8]> {
        if start > self.data.len() {
            return Err(MemoryMapError::IndexOutOfBounds(start));
        }

        match length {
            Some(len) => {
                let end = start + len;
                if end > self.data.len() {
                    return Err(MemoryMapError::IndexOutOfBounds(end));
                }
                Ok(&self.data[start..end])
            }
            None => Ok(&self.data[start..]),
        }
    }

    /// Get a mutable chunk of memory
    pub fn get_mut(&mut self, start: usize, length: Option<usize>) -> Result<&mut [u8]> {
        if start > self.data.len() {
            return Err(MemoryMapError::IndexOutOfBounds(start));
        }

        match length {
            Some(len) => {
                let end = start + len;
                if end > self.data.len() {
                    return Err(MemoryMapError::IndexOutOfBounds(end));
                }
                Ok(&mut self.data[start..end])
            }
            None => Ok(&mut self.data[start..]),
        }
    }

    /// Set a byte at position @pos to @value
    pub fn set_byte(&mut self, pos: usize, value: u8) -> Result<()> {
        if pos >= self.data.len() {
            return Err(MemoryMapError::IndexOutOfBounds(pos));
        }
        self.data[pos] = value;
        Ok(())
    }

    /// Set a chunk of bytes starting at @pos
    pub fn set_bytes(&mut self, pos: usize, bytes: &[u8]) -> Result<()> {
        let end = pos + bytes.len();
        if end > self.data.len() {
            return Err(MemoryMapError::IndexOutOfBounds(end));
        }
        self.data[pos..end].copy_from_slice(bytes);
        Ok(())
    }

    /// Get the entire memory map as raw bytes
    pub fn get_packed(&self) -> &[u8] {
        &self.data
    }

    /// Get the entire memory map as owned Vec<u8>
    pub fn to_vec(&self) -> Vec<u8> {
        self.data.clone()
    }

    /// Truncate the memory map to @size bytes
    pub fn truncate(&mut self, size: usize) {
        self.data.truncate(size);
    }

    /// Get a printable hex representation of the memory map
    pub fn printable(&self, start: Option<usize>, end: Option<usize>) -> String {
        let start = start.unwrap_or(0);
        let end = end.unwrap_or(self.data.len());

        let slice = &self.data[start..end];
        hexdump(slice)
    }
}

impl From<Vec<u8>> for MemoryMap {
    fn from(data: Vec<u8>) -> Self {
        Self::new(data)
    }
}

impl From<&[u8]> for MemoryMap {
    fn from(data: &[u8]) -> Self {
        Self::new(data.to_vec())
    }
}

impl AsRef<[u8]> for MemoryMap {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl fmt::Display for MemoryMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MemoryMap({} bytes)", self.data.len())
    }
}

/// Create a hex dump of bytes (similar to hexdump -C)
fn hexdump(data: &[u8]) -> String {
    let mut output = String::new();

    for (i, chunk) in data.chunks(16).enumerate() {
        // Offset
        output.push_str(&format!("{:08x}  ", i * 16));

        // Hex bytes
        for (j, byte) in chunk.iter().enumerate() {
            if j == 8 {
                output.push(' ');
            }
            output.push_str(&format!("{:02x} ", byte));
        }

        // Padding for incomplete lines
        if chunk.len() < 16 {
            for j in chunk.len()..16 {
                if j == 8 {
                    output.push(' ');
                }
                output.push_str("   ");
            }
        }

        // ASCII representation
        output.push_str(" |");
        for byte in chunk {
            if *byte >= 0x20 && *byte <= 0x7e {
                output.push(*byte as char);
            } else {
                output.push('.');
            }
        }
        output.push_str("|\n");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_map_creation() {
        let mmap = MemoryMap::new(vec![1, 2, 3, 4, 5]);
        assert_eq!(mmap.len(), 5);
        assert!(!mmap.is_empty());

        let empty = MemoryMap::new_empty();
        assert!(empty.is_empty());

        let sized = MemoryMap::new_with_size(10);
        assert_eq!(sized.len(), 10);
        assert_eq!(sized.get(0, Some(10)).unwrap(), &[0u8; 10]);
    }

    #[test]
    fn test_get_set() {
        let mut mmap = MemoryMap::new(vec![0; 10]);

        // Set a byte
        mmap.set_byte(5, 0x42).unwrap();
        assert_eq!(mmap.get(5, Some(1)).unwrap()[0], 0x42);

        // Set multiple bytes
        mmap.set_bytes(0, &[1, 2, 3]).unwrap();
        assert_eq!(mmap.get(0, Some(3)).unwrap(), &[1, 2, 3]);

        // Get to end
        assert_eq!(mmap.get(8, None).unwrap(), &[0, 0]);
    }

    #[test]
    fn test_bounds_checking() {
        let mmap = MemoryMap::new(vec![1, 2, 3]);

        // Out of bounds
        assert!(mmap.get(5, Some(1)).is_err());
        assert!(mmap.get(2, Some(5)).is_err());
    }

    #[test]
    fn test_truncate() {
        let mut mmap = MemoryMap::new(vec![1, 2, 3, 4, 5]);
        mmap.truncate(3);
        assert_eq!(mmap.len(), 3);
        assert_eq!(mmap.get_packed(), &[1, 2, 3]);
    }

    #[test]
    fn test_hexdump() {
        let data = vec![
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x41, 0x42, 0x43,
        ];
        let dump = hexdump(&data);
        assert!(dump.contains("00 01 02 03"));
        assert!(dump.contains("41 42 43"));
        assert!(dump.contains("|"));
    }
}
