// .img file format handler compatible with Python CHIRP
// Reference: chirp/chirp_common.py lines 1560-1629

use super::metadata::Metadata;
use crate::memmap::MemoryMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ImgError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to decode metadata: {0}")]
    MetadataDecode(String),

    #[error("Failed to parse metadata JSON: {0}")]
    MetadataJson(#[from] serde_json::Error),

    #[error("Failed to decode base64 metadata: {0}")]
    Base64Decode(String),
}

pub type Result<T> = std::result::Result<T, ImgError>;

/// Magic bytes that separate binary data from metadata in .img files
/// This must match Python CHIRP exactly: b'\x00\xffchirp\xeeimg\x00\x01'
pub const MAGIC: &[u8] = b"\x00\xffchirp\xeeimg\x00\x01";

/// Load a .img file and return the memory map and metadata
pub fn load_img(filename: impl AsRef<Path>) -> Result<(MemoryMap, Metadata)> {
    let mut file = File::open(filename)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    // Check if the file contains metadata
    if let Some(idx) = find_magic(&data) {
        // Split at the magic boundary
        let binary_data = data[..idx].to_vec();
        let metadata_bytes = &data[idx + MAGIC.len()..];

        // Decode the base64-encoded JSON metadata
        let metadata = decode_metadata(metadata_bytes)?;

        Ok((MemoryMap::new(binary_data), metadata))
    } else {
        // No metadata, just raw binary data
        Ok((MemoryMap::new(data), Metadata::default()))
    }
}

/// Save a memory map and metadata to a .img file
pub fn save_img(filename: impl AsRef<Path>, mmap: &MemoryMap, metadata: &Metadata) -> Result<()> {
    let mut file = File::create(filename)?;

    // Write the binary data
    file.write_all(mmap.get_packed())?;

    // Write the magic separator
    file.write_all(MAGIC)?;

    // Encode metadata as base64-encoded JSON
    let metadata_json = metadata.to_json()?;
    let metadata_base64 = base64::encode(metadata_json.as_bytes());
    file.write_all(metadata_base64.as_bytes())?;

    Ok(())
}

/// Find the position of MAGIC in the data
fn find_magic(data: &[u8]) -> Option<usize> {
    data.windows(MAGIC.len()).position(|window| window == MAGIC)
}

/// Decode base64-encoded JSON metadata
fn decode_metadata(encoded: &[u8]) -> Result<Metadata> {
    // Decode from base64
    let decoded = base64::decode(encoded).map_err(|e| ImgError::Base64Decode(e.to_string()))?;

    // Parse JSON
    let json_str =
        String::from_utf8(decoded).map_err(|e| ImgError::MetadataDecode(e.to_string()))?;

    Metadata::from_json(&json_str).map_err(ImgError::MetadataJson)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_magic_finding() {
        let data = b"hello\x00\xffchirp\xeeimg\x00\x01world";
        assert_eq!(find_magic(data), Some(5));

        let data = b"no magic here";
        assert_eq!(find_magic(data), None);
    }

    #[test]
    fn test_save_load_img() -> Result<()> {
        let mut tempfile = NamedTempFile::new().unwrap();
        let path = tempfile.path().to_path_buf();

        // Create test data
        let mmap = MemoryMap::new(vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let mut metadata = Metadata::new("Kenwood", "TH-D75");
        metadata.rclass = "THD75Radio".to_string();

        // Save
        save_img(&path, &mmap, &metadata)?;

        // Load
        let (loaded_mmap, loaded_metadata) = load_img(&path)?;

        // Verify
        assert_eq!(loaded_mmap.get_packed(), mmap.get_packed());
        assert_eq!(loaded_metadata.vendor, "Kenwood");
        assert_eq!(loaded_metadata.model, "TH-D75");
        assert_eq!(loaded_metadata.rclass, "THD75Radio");

        Ok(())
    }

    #[test]
    fn test_load_raw_binary() -> Result<()> {
        let mut tempfile = NamedTempFile::new().unwrap();
        tempfile.write_all(&[1, 2, 3, 4, 5]).unwrap();
        let path = tempfile.path();

        let (mmap, metadata) = load_img(path)?;

        assert_eq!(mmap.get_packed(), &[1, 2, 3, 4, 5]);
        assert_eq!(metadata.vendor, ""); // Default empty metadata

        Ok(())
    }

    #[test]
    fn test_python_compatibility() -> Result<()> {
        // This tests that we can read files created by Python CHIRP
        // The format is: <binary_data><MAGIC><base64(json)>

        let mut tempfile = NamedTempFile::new().unwrap();

        // Write binary data
        tempfile.write_all(&[0xAA, 0xBB, 0xCC, 0xDD]).unwrap();

        // Write MAGIC
        tempfile.write_all(MAGIC).unwrap();

        // Write base64-encoded JSON metadata
        let metadata_json = r#"{"vendor":"Icom","model":"IC-9700","chirp_version":"0.1.0"}"#;
        let metadata_base64 = base64::encode(metadata_json.as_bytes());
        tempfile.write_all(metadata_base64.as_bytes()).unwrap();

        tempfile.flush().unwrap();
        let path = tempfile.path();

        // Load and verify
        let (mmap, metadata) = load_img(path)?;

        assert_eq!(mmap.get_packed(), &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(metadata.vendor, "Icom");
        assert_eq!(metadata.model, "IC-9700");

        Ok(())
    }
}
