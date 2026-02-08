// Radio operations for GUI - handles async communication with radio drivers

use crate::core::Memory;
use crate::drivers::{get_driver, CloneModeRadio, Radio};
use crate::serial::{SerialConfig, SerialPort};
use std::sync::Arc;
use std::time::Duration;

/// Result type for radio operations
pub type RadioOpResult<T> = Result<T, String>;

/// Progress callback type for GUI updates
pub type ProgressFn = Arc<dyn Fn(usize, usize, String) + Send + Sync>;

/// Download memories from a radio
/// Returns (memories, mmap) so the mmap can be stored and used for uploads
pub async fn download_from_radio(
    port_name: String,
    vendor: String,
    model: String,
    progress_fn: ProgressFn,
) -> RadioOpResult<(Vec<Memory>, crate::memmap::MemoryMap)> {
    tracing::debug!("download_from_radio called");
    tracing::debug!("  port: {}", port_name);
    tracing::debug!("  vendor: {}", vendor);
    tracing::debug!("  model: {}", model);

    // Get driver info to determine radio type
    let driver_info = get_driver(&vendor, &model)
        .ok_or_else(|| format!("Unknown radio: {} {}", vendor, model))?;

    tracing::debug!(
        "Found driver: {} {} (clone_mode: {})",
        driver_info.vendor,
        driver_info.model,
        driver_info.is_clone_mode
    );

    // Open serial port with appropriate settings
    // Kenwood radios need hardware flow control (RTS/CTS)
    // Icom CI-V radios do NOT use flow control (RTS high = transmit)
    // IC-9700 uses 19200 baud by default
    let baud_rate = if vendor.to_lowercase() == "icom" && model.contains("9700") {
        19200
    } else {
        9600
    };
    // Use shorter timeout for CI-V radios (Icom) since responses are fast
    // Kenwood clone mode needs longer timeout for large block transfers
    let timeout = if vendor.to_lowercase() == "icom" {
        Duration::from_secs(2)  // CI-V responses should be quick
    } else {
        Duration::from_secs(10) // Clone mode block transfers can be slow
    };
    let mut serial_config = SerialConfig::new(baud_rate).with_timeout(timeout);

    if vendor.to_lowercase() == "kenwood" {
        serial_config = serial_config.with_hardware_flow();
        tracing::debug!("Using hardware flow control for Kenwood radio");
    } else {
        tracing::debug!("No hardware flow control (vendor: {})", vendor);
    }

    let mut port = SerialPort::open(&port_name, serial_config)
        .map_err(|e| format!("Failed to open port {}: {}", port_name, e))?;

    // Set DTR and RTS based on radio vendor
    // Kenwood radios need DTR=true, RTS=false to enter programming mode
    // Icom radios need DTR=false, RTS=false (RTS high = transmitting!)
    if vendor.to_lowercase() == "kenwood" {
        tracing::debug!("Setting DTR=true, RTS=false for Kenwood radio");
        port.set_dtr(true)
            .map_err(|e| format!("Failed to set DTR: {}", e))?;
        port.set_rts(false)
            .map_err(|e| format!("Failed to set RTS: {}", e))?;
        tracing::debug!("Kenwood: DTR/RTS configured");
    } else if vendor.to_lowercase() == "icom" {
        tracing::debug!("Setting DTR=true, RTS=false for Icom radio");
        // CRITICAL: Icom CI-V radios need DTR=true (for interface power/signaling)
        // but RTS=false (RTS high = transmit)
        port.set_dtr(true)
            .map_err(|e| format!("Failed to set DTR: {}", e))?;
        port.set_rts(false)
            .map_err(|e| format!("Failed to set RTS: {}", e))?;
        tracing::debug!("Icom: DTR=true, RTS=false confirmed");
    }

    // Clear buffers
    port.clear_all()
        .map_err(|e| format!("Failed to clear buffers: {}", e))?;

    tracing::debug!("Opened serial port {}", port_name);

    // Download based on radio type
    let (memories, mmap) = if driver_info.is_clone_mode {
        // Clone mode radios (e.g., TH-D75)
        download_clone_mode(&mut port, &vendor, &model, progress_fn).await?
    } else {
        // Command-based radios (e.g., IC-9700)
        download_command_mode(&mut port, &vendor, &model, progress_fn).await?
    };

    tracing::debug!("Downloaded {} memories", memories.len());

    Ok((memories, mmap))
}

/// Download from a clone-mode radio (TH-D75, TH-D74)
async fn download_clone_mode(
    port: &mut SerialPort,
    _vendor: &str,
    _model: &str,
    progress_fn: ProgressFn,
) -> RadioOpResult<(Vec<Memory>, crate::memmap::MemoryMap)> {
    use crate::drivers::thd75::THD75Radio;

    // Create driver instance
    let mut driver = THD75Radio::new();

    // Create progress callback
    let status_callback = Some(
        Box::new(move |current: usize, total: usize, message: &str| {
            progress_fn(current, total, message.to_string());
        }) as Box<dyn Fn(usize, usize, &str) + Send + Sync>,
    );

    // Download from radio
    let mmap = driver
        .sync_in(port, status_callback)
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    // Parse memories from memmap (driver stores mmap internally)
    let memories = driver
        .get_memories()
        .map_err(|e| format!("Failed to parse memories: {}", e))?;

    Ok((memories, mmap))
}

/// Download from a command-based radio (IC-9700)
async fn download_command_mode(
    port: &mut SerialPort,
    _vendor: &str,
    model: &str,
    progress_fn: ProgressFn,
) -> RadioOpResult<(Vec<Memory>, crate::memmap::MemoryMap)> {
    use crate::drivers::ic9700::IC9700Radio;

    // Check if this is IC-9700 (multi-band radio)
    let is_ic9700 = model.contains("9700");
    let bands = if is_ic9700 {
        vec![1, 2, 3] // VHF, UHF, 1.2GHz
    } else {
        vec![1] // Single band for other command-mode radios
    };

    let mut all_memories = Vec::new();
    let total_bands = bands.len();

    // Download all bands
    for (band_idx, band_num) in bands.iter().enumerate() {
        tracing::info!("Downloading Band {} of {}", band_idx + 1, total_bands);

        let mut driver = IC9700Radio::new_band(*band_num);

        // CRITICAL: Detect if interface echoes commands before any operations
        driver
            .detect_echo(port)
            .await
            .map_err(|e| format!("Failed to detect echo: {}", e))?;

        // Create progress callback for this band
        let band_name = match band_num {
            1 => "VHF (144 MHz)",
            2 => "UHF (430 MHz)",
            3 => "1.2 GHz (1240 MHz)",
            _ => "Unknown",
        };

        let progress_fn_clone = progress_fn.clone();
        let status_callback = Some(
            Box::new(move |current: usize, total: usize, message: &str| {
                let band_message = format!("{} - {}", band_name, message);
                progress_fn_clone(current, total, band_message);
            }) as Box<dyn Fn(usize, usize, &str) + Send + Sync>,
        );

        // Download memories for this band
        let mut band_memories = driver
            .download_memories(port, status_callback)
            .await
            .map_err(|e| format!("Download failed for band {}: {}", band_num, e))?;

        // Tag memories with band number for multi-band radios
        if is_ic9700 {
            for mem in &mut band_memories {
                mem.band = Some(*band_num);
            }
        }

        all_memories.extend(band_memories);
    }

    tracing::info!("Downloaded total of {} memories from {} band(s)", all_memories.len(), total_bands);

    // IC-9700 doesn't use clone mode, so create empty mmap
    // Upload will use command-based protocol
    let mmap = crate::memmap::MemoryMap::new(vec![]);

    Ok((all_memories, mmap))
}

/// Upload memories to a radio
/// Requires the mmap from the original download to preserve all radio settings
pub async fn upload_to_radio(
    port_name: String,
    mmap: crate::memmap::MemoryMap,
    memories: Vec<Memory>,
    vendor: String,
    model: String,
    progress_fn: ProgressFn,
) -> RadioOpResult<()> {
    tracing::debug!("upload_to_radio called");
    tracing::debug!("  port: {}", port_name);
    tracing::debug!("  vendor: {}", vendor);
    tracing::debug!("  model: {}", model);
    tracing::debug!("  memories: {}", memories.len());

    // Get driver info
    let driver_info = get_driver(&vendor, &model)
        .ok_or_else(|| format!("Unknown radio: {} {}", vendor, model))?;

    // Open serial port
    // Kenwood radios need hardware flow control (RTS/CTS)
    // Icom CI-V radios do NOT use flow control (RTS high = transmit)
    // IC-9700 uses 19200 baud by default
    let baud_rate = if vendor.to_lowercase() == "icom" && model.contains("9700") {
        19200
    } else {
        9600
    };
    // Use shorter timeout for CI-V radios (Icom) since responses are fast
    // Kenwood clone mode needs longer timeout for large block transfers
    let timeout = if vendor.to_lowercase() == "icom" {
        Duration::from_secs(2)  // CI-V responses should be quick
    } else {
        Duration::from_secs(10) // Clone mode block transfers can be slow
    };
    let mut serial_config = SerialConfig::new(baud_rate).with_timeout(timeout);

    if vendor.to_lowercase() == "kenwood" {
        serial_config = serial_config.with_hardware_flow();
        tracing::debug!("Using hardware flow control for Kenwood radio");
    } else {
        tracing::debug!("No hardware flow control (vendor: {})", vendor);
    }

    let mut port = SerialPort::open(&port_name, serial_config)
        .map_err(|e| format!("Failed to open port {}: {}", port_name, e))?;

    // Set DTR and RTS based on radio vendor
    // Kenwood radios need DTR=true, RTS=false to enter programming mode
    // Icom radios need DTR=false, RTS=false (RTS high = transmitting!)
    if vendor.to_lowercase() == "kenwood" {
        tracing::debug!("Setting DTR=true, RTS=false for Kenwood radio");
        port.set_dtr(true)
            .map_err(|e| format!("Failed to set DTR: {}", e))?;
        port.set_rts(false)
            .map_err(|e| format!("Failed to set RTS: {}", e))?;
        tracing::debug!("Kenwood: DTR/RTS configured");
    } else if vendor.to_lowercase() == "icom" {
        tracing::debug!("Setting DTR=true, RTS=false for Icom radio");
        // CRITICAL: Icom CI-V radios need DTR=true (for interface power/signaling)
        // but RTS=false (RTS high = transmit)
        port.set_dtr(true)
            .map_err(|e| format!("Failed to set DTR: {}", e))?;
        port.set_rts(false)
            .map_err(|e| format!("Failed to set RTS: {}", e))?;
        tracing::debug!("Icom: DTR=true, RTS=false confirmed");
    }

    // Clear buffers
    port.clear_all()
        .map_err(|e| format!("Failed to clear buffers: {}", e))?;

    tracing::debug!("Opened serial port {}", port_name);

    // Upload based on radio type
    if driver_info.is_clone_mode {
        upload_clone_mode(&mut port, &vendor, &model, mmap, memories, progress_fn).await?
    } else {
        upload_command_mode(&mut port, &vendor, &model, memories, progress_fn).await?
    };

    tracing::debug!("Upload complete");

    Ok(())
}

/// Upload to a clone-mode radio (TH-D75, TH-D74)
/// Uses the mmap from the original download and updates it with the edited memories
async fn upload_clone_mode(
    port: &mut SerialPort,
    _vendor: &str,
    _model: &str,
    mmap: crate::memmap::MemoryMap,
    memories: Vec<Memory>,
    progress_fn: ProgressFn,
) -> RadioOpResult<()> {
    use crate::drivers::thd75::THD75Radio;

    // Create driver instance and load the mmap
    let mut driver = THD75Radio::new();
    driver
        .process_mmap(&mmap)
        .map_err(|e| format!("Failed to process mmap: {}", e))?;

    tracing::info!("Updating memories in mmap...");

    // Update only non-empty memory channels in the mmap
    // Empty memories should be left as-is in the original mmap
    for mem in &memories {
        if mem.number >= 1200 {
            continue; // Skip invalid memory numbers
        }

        // Only update non-empty memories to preserve existing data
        if !mem.empty {
            driver
                .set_memory(mem)
                .map_err(|e| format!("Failed to update memory #{}: {}", mem.number, e))?;
        }
    }

    // Get the modified memory map
    let modified_mmap = driver
        .mmap
        .clone()
        .ok_or_else(|| "Memory map not available after update".to_string())?;

    tracing::info!("Uploading to radio...");

    // DTR/RTS already set in upload_to_radio() based on vendor
    // Clear buffers before upload
    port.clear_all()
        .map_err(|e| format!("Failed to clear buffers: {}", e))?;

    // Create progress callback for upload
    let status_callback = Some(
        Box::new(move |current: usize, total: usize, message: &str| {
            progress_fn(current, total, message.to_string());
        }) as Box<dyn Fn(usize, usize, &str) + Send + Sync>,
    );

    // Upload the modified memory map to radio
    driver
        .sync_out(port, &modified_mmap, status_callback)
        .await
        .map_err(|e| format!("Upload failed: {}", e))?;

    Ok(())
}

/// Upload to a command-based radio (IC-9700)
async fn upload_command_mode(
    port: &mut SerialPort,
    _vendor: &str,
    model: &str,
    memories: Vec<Memory>,
    progress_fn: ProgressFn,
) -> RadioOpResult<()> {
    use crate::drivers::ic9700::IC9700Radio;
    use std::collections::HashMap;

    // Check if this is IC-9700 (multi-band radio)
    let is_ic9700 = model.contains("9700");

    // Filter to only modified memories for efficient upload
    let modified_memories: Vec<&Memory> = memories
        .iter()
        .filter(|m| m.modified)
        .collect();

    tracing::info!(
        "Upload: {} modified out of {} total memories",
        modified_memories.len(),
        memories.len()
    );

    if modified_memories.is_empty() {
        tracing::info!("No modified memories to upload");
        return Ok(());
    }

    // Group modified memories by band
    let mut bands: HashMap<u8, Vec<Memory>> = HashMap::new();
    for mem in modified_memories {
        let band = mem.band.unwrap_or(1); // Default to band 1 if not specified
        bands.entry(band).or_default().push(mem.clone());
    }

    // Upload each band separately
    for (band_num, band_mems) in bands {
        tracing::info!("Uploading Band {} ({} memories)", band_num, band_mems.len());

        let mut driver = IC9700Radio::new_band(band_num);

        // CRITICAL: Detect if interface echoes commands before any operations
        driver
            .detect_echo(port)
            .await
            .map_err(|e| format!("Failed to detect echo: {}", e))?;

        // Create progress callback for this band
        let band_name = if is_ic9700 {
            match band_num {
                1 => "VHF (144 MHz)",
                2 => "UHF (430 MHz)",
                3 => "1.2 GHz (1240 MHz)",
                _ => "Unknown",
            }
        } else {
            "Band"
        };

        let progress_fn_clone = progress_fn.clone();
        let status_callback = Some(
            Box::new(move |current: usize, total: usize, message: &str| {
                let band_message = format!("{} - {}", band_name, message);
                progress_fn_clone(current, total, band_message);
            }) as Box<dyn Fn(usize, usize, &str) + Send + Sync>,
        );

        // Upload memories for this band
        driver
            .upload_memories(port, &band_mems, status_callback)
            .await
            .map_err(|e| format!("Upload failed for band {}: {}", band_num, e))?;
    }

    tracing::info!("Upload complete for all bands");

    Ok(())
}
