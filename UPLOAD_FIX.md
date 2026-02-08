# Upload to Radio Fix - Critical Bug Resolution

## Problem
When uploading memories to the TH-D75 radio, the entire radio configuration was being wiped and the device would hard-reset. This caused:
- Loss of all configuration settings
- Loss of band settings
- Loss of scan settings
- Complete radio reset

## Root Cause
The original `upload_clone_mode()` function in `src/gui/radio_ops.rs` was calling `encode_memories()`, which created a **brand new memory map** filled with 0xFF (empty memory pattern). This new map only contained the memory channels being uploaded, with no other radio settings, effectively wiping everything on the radio.

## Solution
Implemented a proper **read-modify-write** pattern in `upload_clone_mode()` (lines 176-248):

### Step 1: Download Current State
```rust
// CRITICAL: Download current radio state first to preserve settings!
tracing::info!("Downloading current radio state to preserve settings...");
progress_fn(0, 100, "Downloading current state...".to_string());

let download_callback = Some(
    Box::new(|current: usize, total: usize, message: &str| {
        tracing::debug!("Download: {}/{} - {}", current, total, message);
    }) as Box<dyn Fn(usize, usize, &str) + Send + Sync>,
);

let mut current_mmap = driver
    .sync_in(port, download_callback)
    .await
    .map_err(|e| format!("Failed to download current state: {}", e))?;
```

### Step 2: Modify Only Memory Channels
```rust
tracing::info!("Download complete, now updating memories...");

// Update only the memory channels in the downloaded data
for mem in &memories {
    if mem.number >= 1200 {
        continue; // Skip invalid memory numbers
    }

    driver
        .set_memory(mem)
        .map_err(|e| format!("Failed to update memory #{}: {}", mem.number, e))?;
}
```

### Step 3: Upload Modified State
```rust
// Get the modified memory map from the driver
current_mmap = driver
    .mmap
    .clone()
    .ok_or_else(|| "Memory map not available after update".to_string())?;

tracing::info!("Memories updated, now uploading to radio...");

// Create progress callback for upload
let status_callback = Some(
    Box::new(move |current: usize, total: usize, message: &str| {
        progress_fn(current, total, message.to_string());
    }) as Box<dyn Fn(usize, usize, &str) + Send + Sync>,
);

// Upload the modified memory map back to radio
driver
    .sync_out(port, &current_mmap, status_callback)
    .await
    .map_err(|e| format!("Upload failed: {}", e))?;
```

## Key Functions Involved

### `set_memory()` (src/drivers/thd75.rs:969-1000)
Updates a single memory channel in the memory map:
- Encodes the Memory struct to raw binary format
- Calculates offsets for memory data, flags, and name
- Writes the data at the correct positions in the memory map
- Updates in place without creating new data structures

### `sync_in()` (src/drivers/thd75.rs:1008+)
Downloads the complete radio state:
- Enters programming mode
- Switches to 57600 baud
- Downloads all memory blocks
- Returns complete MemoryMap with all radio settings

### `sync_out()` (src/drivers/thd75.rs:1078+)
Uploads the complete radio state:
- Enters programming mode
- Switches to 57600 baud
- Uploads all memory blocks
- Preserves all radio settings

## Additional Fix: Baud Rate Synchronization

### Problem
After the initial fix, upload would fail with "Broken pipe" error. The issue was:
- `sync_in()` exits programming mode, which returns radio to 9600 baud
- PC-side serial port remained at 57600 baud
- `sync_out()` tried to communicate but baud rates were mismatched

### Solution
Both `sync_in()` and `sync_out()` now switch the PC back to 9600 baud after exiting programming mode:

```rust
// End programming mode
port.write_all(b"E").await?;

// Switch back to 9600 baud after exiting programming mode
tracing::debug!("Switching back to 9600 baud");
port.set_baud_rate(9600)?;
tokio::time::sleep(Duration::from_millis(100)).await;
```

This ensures the PC and radio are always synchronized on baud rate.

## Testing
✅ Code compiles successfully: `cargo build --release`
✅ All 80 tests pass: `cargo test --lib`
✅ Code formatted: `cargo fmt`

## Next Steps
User should test the upload functionality by:
1. Downloading memories from radio
2. Modifying one or more memories
3. Uploading back to radio
4. Verifying radio does NOT reset
5. Verifying all configuration settings are preserved
6. Verifying memory changes were applied correctly

## Technical Notes
- The memory map is approximately 500KB and contains:
  - 1200 memory channels (40 bytes each)
  - Memory flags (4 bytes each)
  - Memory names (16 bytes each)
  - Radio configuration settings
  - Band settings
  - Scan settings
  - Other radio-specific data

- This fix ensures only the memory channels are modified while preserving all other data
- The download step adds ~30 seconds to the upload process but is essential for safety
