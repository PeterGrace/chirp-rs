// Radio driver framework
pub mod registry;
pub mod traits;

// Drivers
pub mod ic9700;
pub mod thd75;
pub mod uv5r;

pub use registry::{get_driver, list_drivers, register_driver, DriverInfo};
pub use traits::{CloneModeRadio, Radio, RadioError, RadioResult};

/// Initialize and register all available radio drivers
///
/// This function must be called once at application startup to populate
/// the driver registry with all available radio drivers.
pub fn init_drivers() {
    // Register Kenwood TH-D75 (CloneModeRadio)
    register_driver(DriverInfo::new(
        "Kenwood",
        "TH-D75",
        "Dual-band HT with D-STAR support (VHF/UHF)",
        true, // is_clone_mode
    ));

    // Register Kenwood TH-D74 (same driver as TH-D75)
    register_driver(DriverInfo::new(
        "Kenwood",
        "TH-D74",
        "Dual-band HT with D-STAR support (VHF/UHF)",
        true, // is_clone_mode
    ));

    // Register Icom IC-9700 (CI-V command-based)
    register_driver(DriverInfo::new(
        "Icom",
        "IC-9700",
        "Tri-band transceiver with D-STAR (VHF/UHF/1.2GHz)",
        false, // not clone mode - uses CI-V protocol
    ));

    // Register Baofeng UV-5R (CloneModeRadio)
    register_driver(DriverInfo::new(
        "Baofeng",
        "UV-5R",
        "Dual-band handheld (VHF/UHF, FM only)",
        true, // is_clone_mode
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_drivers() {
        // Clear registry first (for testing)
        init_drivers();

        // Verify all drivers are registered
        let drivers = list_drivers();
        assert!(!drivers.is_empty(), "No drivers registered");
        assert!(drivers.len() >= 4, "Expected at least 4 drivers");

        // Verify specific drivers
        assert!(
            get_driver("Kenwood", "TH-D75").is_some(),
            "TH-D75 not found"
        );
        assert!(
            get_driver("Kenwood", "TH-D74").is_some(),
            "TH-D74 not found"
        );
        assert!(get_driver("Icom", "IC-9700").is_some(), "IC-9700 not found");

        // Verify specific drivers
        assert!(get_driver("Baofeng", "UV-5R").is_some(), "UV-5R not found");

        // Verify vendors
        let vendors: std::collections::HashSet<String> =
            drivers.iter().map(|d| d.vendor.clone()).collect();
        assert!(vendors.contains("Kenwood"), "Kenwood vendor not found");
        assert!(vendors.contains("Icom"), "Icom vendor not found");
        assert!(vendors.contains("Baofeng"), "Baofeng vendor not found");
    }
}
