// Radio operations for GUI - handles async communication with radio drivers

use crate::core::Memory;
use crate::drivers::{get_driver, CloneModeRadio, Radio, RadioError};
use crate::serial::{SerialConfig, SerialPort};
use std::sync::Arc;
use std::time::Duration;

/// Result type for radio operations
pub type RadioOpResult<T> = Result<T, String>;

/// Progress callback type for GUI updates
pub type ProgressFn = Arc<dyn Fn(usize, usize, String) + Send + Sync>;

/// Download memories from a radio
pub async fn download_from_radio(
    port_name: String,
    vendor: String,
    model: String,
    progress_fn: ProgressFn,
) -> RadioOpResult<Vec<Memory>> {
    tracing::debug!("download_from_radio called");
    tracing::debug!("  port: {}", port_name);
    tracing::debug!("  vendor: {}", vendor);
    tracing::debug!("  model: {}", model);

    // Get driver info to determine radio type
    let driver_info = get_driver(&vendor, &model)
        .ok_or_else(|| format!("Unknown radio: {} {}", vendor, model))?;

    tracing::debug!("Found driver: {} {} (clone_mode: {})",
              driver_info.vendor, driver_info.model, driver_info.is_clone_mode);

    // Open serial port with appropriate settings
    let serial_config = SerialConfig::new(9600)
        .with_timeout(Duration::from_secs(10))
        .with_hardware_flow();

    let mut port = SerialPort::open(&port_name, serial_config)
        .map_err(|e| format!("Failed to open port {}: {}", port_name, e))?;

    // Set DTR and RTS - required for Kenwood radios to enter programming mode
    tracing::debug!("Setting DTR/RTS");
    port.set_dtr(true).map_err(|e| format!("Failed to set DTR: {}", e))?;
    port.set_rts(false).map_err(|e| format!("Failed to set RTS: {}", e))?;

    // Clear buffers
    port.clear_all().map_err(|e| format!("Failed to clear buffers: {}", e))?;

    tracing::debug!("Opened serial port {}", port_name);

    // Download based on radio type
    let memories = if driver_info.is_clone_mode {
        // Clone mode radios (e.g., TH-D75)
        download_clone_mode(&mut port, &vendor, &model, progress_fn).await?
    } else {
        // Command-based radios (e.g., IC-9700)
        download_command_mode(&mut port, &vendor, &model, progress_fn).await?
    };

    tracing::debug!("Downloaded {} memories", memories.len());

    Ok(memories)
}

/// Download from a clone-mode radio (TH-D75, TH-D74)
async fn download_clone_mode(
    port: &mut SerialPort,
    _vendor: &str,
    _model: &str,
    progress_fn: ProgressFn,
) -> RadioOpResult<Vec<Memory>> {
    use crate::drivers::thd75::THD75Radio;

    // Create driver instance
    let mut driver = THD75Radio::new();

    // Create progress callback
    let status_callback = Some(Box::new(move |current: usize, total: usize, message: &str| {
        progress_fn(current, total, message.to_string());
    }) as Box<dyn Fn(usize, usize, &str) + Send + Sync>);

    // Download from radio
    let _mmap = driver
        .sync_in(port, status_callback)
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    // Parse memories from memmap (driver stores mmap internally)
    let memories = driver
        .get_memories()
        .map_err(|e| format!("Failed to parse memories: {}", e))?;

    Ok(memories)
}

/// Download from a command-based radio (IC-9700)
async fn download_command_mode(
    port: &mut SerialPort,
    _vendor: &str,
    _model: &str,
    progress_fn: ProgressFn,
) -> RadioOpResult<Vec<Memory>> {
    use crate::drivers::ic9700::IC9700Radio;

    // IC-9700 is multi-band - default to VHF (band 1) for now
    // TODO: Let user select band or download all bands
    let mut driver = IC9700Radio::new_band(1);

    // Create progress callback
    let status_callback = Some(Box::new(move |current: usize, total: usize, message: &str| {
        progress_fn(current, total, message.to_string());
    }) as Box<dyn Fn(usize, usize, &str) + Send + Sync>);

    // Download memories
    let memories = driver
        .download_memories(port, status_callback)
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    Ok(memories)
}

/// Upload memories to a radio
pub async fn upload_to_radio(
    port_name: String,
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
    let serial_config = SerialConfig::new(9600)
        .with_timeout(Duration::from_secs(10))
        .with_hardware_flow();

    let mut port = SerialPort::open(&port_name, serial_config)
        .map_err(|e| format!("Failed to open port {}: {}", port_name, e))?;

    tracing::debug!("Opened serial port {}", port_name);

    // Upload based on radio type
    if driver_info.is_clone_mode {
        upload_clone_mode(&mut port, &vendor, &model, memories, progress_fn).await?
    } else {
        upload_command_mode(&mut port, &vendor, &model, memories, progress_fn).await?
    };

    tracing::debug!("Upload complete");

    Ok(())
}

/// Upload to a clone-mode radio (TH-D75, TH-D74)
async fn upload_clone_mode(
    port: &mut SerialPort,
    _vendor: &str,
    _model: &str,
    _memories: Vec<Memory>,
    _progress_fn: ProgressFn,
) -> RadioOpResult<()> {
    // Upload not fully implemented yet for clone mode radios
    // set_memory() needs to be implemented in THD75Radio
    Err("Upload to clone-mode radios not yet fully implemented".to_string())
}

/// Upload to a command-based radio (IC-9700)
async fn upload_command_mode(
    port: &mut SerialPort,
    _vendor: &str,
    _model: &str,
    memories: Vec<Memory>,
    progress_fn: ProgressFn,
) -> RadioOpResult<()> {
    use crate::drivers::ic9700::IC9700Radio;

    // IC-9700 is multi-band - default to VHF (band 1)
    let mut driver = IC9700Radio::new_band(1);

    // Create progress callback
    let status_callback = Some(Box::new(move |current: usize, total: usize, message: &str| {
        progress_fn(current, total, message.to_string());
    }) as Box<dyn Fn(usize, usize, &str) + Send + Sync>);

    // Upload memories
    driver
        .upload_memories(port, &memories, status_callback)
        .await
        .map_err(|e| format!("Upload failed: {}", e))?;

    Ok(())
}
