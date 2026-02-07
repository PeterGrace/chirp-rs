// Driver registry for managing radio drivers

use std::collections::HashMap;
use std::sync::Mutex;

/// Information about a radio driver
#[derive(Debug, Clone)]
pub struct DriverInfo {
    pub vendor: String,
    pub model: String,
    pub description: String,
    pub is_clone_mode: bool,
}

impl DriverInfo {
    pub fn new(
        vendor: impl Into<String>,
        model: impl Into<String>,
        description: impl Into<String>,
        is_clone_mode: bool,
    ) -> Self {
        Self {
            vendor: vendor.into(),
            model: model.into(),
            description: description.into(),
            is_clone_mode,
        }
    }

    pub fn full_name(&self) -> String {
        format!("{} {}", self.vendor, self.model)
    }
}

/// Global driver registry
lazy_static::lazy_static! {
    static ref DRIVER_REGISTRY: Mutex<HashMap<String, DriverInfo>> = Mutex::new(HashMap::new());
}

/// Register a driver in the global registry
pub fn register_driver(info: DriverInfo) {
    let key = format!("{}::{}", info.vendor, info.model);
    DRIVER_REGISTRY.lock().unwrap().insert(key, info);
}

/// Get information about a specific driver
pub fn get_driver(vendor: &str, model: &str) -> Option<DriverInfo> {
    let key = format!("{}::{}", vendor, model);
    DRIVER_REGISTRY.lock().unwrap().get(&key).cloned()
}

/// List all registered drivers
pub fn list_drivers() -> Vec<DriverInfo> {
    DRIVER_REGISTRY.lock().unwrap().values().cloned().collect()
}

/// List drivers grouped by vendor
pub fn list_drivers_by_vendor() -> HashMap<String, Vec<DriverInfo>> {
    let mut by_vendor: HashMap<String, Vec<DriverInfo>> = HashMap::new();

    for info in list_drivers() {
        by_vendor
            .entry(info.vendor.clone())
            .or_insert_with(Vec::new)
            .push(info);
    }

    // Sort within each vendor
    for drivers in by_vendor.values_mut() {
        drivers.sort_by(|a, b| a.model.cmp(&b.model));
    }

    by_vendor
}

/// Helper macro to register a driver
#[macro_export]
macro_rules! register_radio_driver {
    ($driver:ty, $vendor:expr, $model:expr, $description:expr, $is_clone:expr) => {
        inventory::submit! {
            $crate::drivers::registry::DriverInfo::new(
                $vendor,
                $model,
                $description,
                $is_clone
            )
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_info() {
        let info = DriverInfo::new("Kenwood", "TH-D75", "Dual-band HT with D-STAR", true);
        assert_eq!(info.vendor, "Kenwood");
        assert_eq!(info.model, "TH-D75");
        assert_eq!(info.full_name(), "Kenwood TH-D75");
        assert!(info.is_clone_mode);
    }

    #[test]
    fn test_registry() {
        let info = DriverInfo::new("Test", "Radio-1", "Test radio", false);
        register_driver(info.clone());

        let retrieved = get_driver("Test", "Radio-1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().model, "Radio-1");

        let all = list_drivers();
        assert!(!all.is_empty());
    }

    #[test]
    fn test_list_by_vendor() {
        register_driver(DriverInfo::new("Kenwood", "TH-D75", "Test", true));
        register_driver(DriverInfo::new("Kenwood", "TH-D74", "Test", true));
        register_driver(DriverInfo::new("Icom", "IC-9700", "Test", false));

        let by_vendor = list_drivers_by_vendor();
        assert!(by_vendor.contains_key("Kenwood"));
        assert!(by_vendor.contains_key("Icom"));

        let kenwood = &by_vendor["Kenwood"];
        assert!(kenwood.len() >= 2);
    }
}
