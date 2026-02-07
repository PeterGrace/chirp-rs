// Metadata for radio image files
// Reference: chirp/chirp_common.py lines 1582-1596

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata stored in .img files
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    /// Radio class name
    #[serde(default)]
    pub rclass: String,

    /// Vendor name
    #[serde(default)]
    pub vendor: String,

    /// Model name
    #[serde(default)]
    pub model: String,

    /// Model variant
    #[serde(default)]
    pub variant: String,

    /// CHIRP version that created the file
    #[serde(default)]
    pub chirp_version: String,

    /// Additional properties
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Metadata {
    /// Create new metadata
    pub fn new(vendor: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            vendor: vendor.into(),
            model: model.into(),
            chirp_version: crate::VERSION.to_string(),
            ..Default::default()
        }
    }

    /// Create metadata with all fields
    pub fn with_details(
        rclass: impl Into<String>,
        vendor: impl Into<String>,
        model: impl Into<String>,
        variant: impl Into<String>,
    ) -> Self {
        Self {
            rclass: rclass.into(),
            vendor: vendor.into(),
            model: model.into(),
            variant: variant.into(),
            chirp_version: crate::VERSION.to_string(),
            extra: HashMap::new(),
        }
    }

    /// Set an extra property
    pub fn set_extra(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.extra.insert(key.into(), value);
    }

    /// Get an extra property
    pub fn get_extra(&self, key: &str) -> Option<&serde_json::Value> {
        self.extra.get(key)
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_creation() {
        let meta = Metadata::new("Kenwood", "TH-D75");
        assert_eq!(meta.vendor, "Kenwood");
        assert_eq!(meta.model, "TH-D75");
        assert!(!meta.chirp_version.is_empty());
    }

    #[test]
    fn test_metadata_serialization() {
        let mut meta = Metadata::new("Icom", "IC-9700");
        meta.set_extra("test_key".to_string(), serde_json::json!("test_value"));

        let json = meta.to_json().unwrap();
        let meta2 = Metadata::from_json(&json).unwrap();

        assert_eq!(meta2.vendor, "Icom");
        assert_eq!(meta2.model, "IC-9700");
        assert_eq!(
            meta2.get_extra("test_key"),
            Some(&serde_json::json!("test_value"))
        );
    }
}
