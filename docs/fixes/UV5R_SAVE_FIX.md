# UV-5R Save Functionality Fix

## Problem Identified

After successfully downloading memories from a UV-5R radio, attempting to save the .img file failed with the error:

```
Failed to update memory #1: Radio error: Invalid duplex: split
```

The log showed `chirp_rs::drivers::thd75 ... encode_memory #1`, indicating the save function was hardcoded to use the TH-D75 driver regardless of which radio was actually downloaded.

## Root Cause

The `save_file()` function in `src/gui/qt_gui.rs` was hardcoded to always create a `THD75Radio` instance (line 1709), similar to the download issue we fixed earlier. Additionally:

1. The `AppState` struct didn't track which radio vendor/model the data came from
2. The `DownloadState::Complete` variant didn't preserve vendor/model information
3. The metadata created for saved files was always "Kenwood"/"TH-D75"

## Solution

### 1. Extended AppState to Track Radio Type (qt_gui.rs)

Added two new fields to the `AppState` struct:
```rust
struct AppState {
    // ... existing fields
    radio_vendor: Option<String>,
    radio_model: Option<String>,
}
```

Updated all 5 AppState initialization locations to include these fields with `None` values.

### 2. Modified DownloadState to Preserve Vendor/Model (qt_gui.rs)

Changed the `Complete` variant to include vendor and model strings:
```rust
enum DownloadState {
    Idle,
    InProgress(DownloadProgress),
    Complete(Result<(Vec<Memory>, crate::memmap::MemoryMap, String, String), String>),
    // Complete now contains: (memories, mmap, vendor, model) on success
}
```

### 3. Updated start_download_async to Store Vendor/Model (qt_gui.rs)

Modified the download completion to include vendor/model in the result:
```rust
// Clone vendor/model for later use
let vendor_clone = vendor_str.clone();
let model_clone = model_str.clone();

// ... download code ...

// Store result with vendor/model info
let mut state = DOWNLOAD_STATE.lock().unwrap();
*state = DownloadState::Complete(
    result.map(|(memories, mmap)| (memories, mmap, vendor_clone, model_clone))
);
```

### 4. Fixed get_download_result to Use Correct Driver (qt_gui.rs)

Updated `get_download_result()` to:
- Extract vendor/model from the Complete result
- Use the correct driver to get bank names (UV-5R doesn't support banks)
- Populate AppState with vendor/model

```rust
match result {
    DownloadState::Complete(Ok((memories, mmap, vendor, model))) => {
        // Get bank names using correct driver
        let bank_names = match (vendor.to_lowercase().as_str(), model.as_str()) {
            ("baofeng", "UV-5R") => vec![],  // No banks
            ("kenwood", "TH-D75") | ("kenwood", "TH-D74") => {
                // Use TH-D75 driver to get bank names
            }
            _ => vec![],
        };

        // ... populate AppState with vendor/model ...
        *data = Some(AppState {
            // ... all fields ...
            radio_vendor: Some(vendor),
            radio_model: Some(model),
        });
    }
}
```

### 5. Fixed save_file to Dynamically Select Driver (qt_gui.rs)

Completely rewrote the `save_file()` function to:
- Check AppState for vendor/model (defaults to TH-D75 for backwards compatibility)
- Match on vendor/model to instantiate the correct driver
- Update memories using the correct driver
- Create metadata with the actual vendor/model

```rust
// Determine vendor and model
let (vendor, model) = match (&state.radio_vendor, &state.radio_model) {
    (Some(v), Some(m)) => (v.clone(), m.clone()),
    _ => ("Kenwood".to_string(), "TH-D75".to_string()),  // Default for old files
};

// Create correct driver
let mmap = match (vendor.to_lowercase().as_str(), model.as_str()) {
    ("baofeng", "UV-5R") => {
        use crate::drivers::uv5r::UV5RRadio;
        let mut radio = UV5RRadio::new();
        // ... process and update memories ...
        radio.mmap.unwrap()
    }
    ("kenwood", "TH-D75") | ("kenwood", "TH-D74") | _ => {
        use crate::drivers::thd75::THD75Radio;
        let mut radio = THD75Radio::new();
        // ... process and update memories ...
        radio.mmap.clone().unwrap()
    }
};

// Create metadata with actual vendor/model
let metadata = Metadata::new(&vendor, &model);
```

## Files Modified

1. `src/gui/qt_gui.rs` - All changes:
   - `AppState` struct: Added `radio_vendor` and `radio_model` fields
   - Updated all 5 AppState initializations to include new fields
   - `DownloadState` enum: Extended `Complete` variant to include vendor/model
   - `start_download_async()`: Store vendor/model with download result
   - `get_download_result()`: Extract vendor/model, use correct driver for banks, populate AppState
   - `download_from_radio()` (blocking version): Same fixes as async version
   - `save_file()`: Dynamically select driver based on vendor/model
   - Removed unused imports: `save_img`, `Radio` (cleanup)

## Benefits

1. **Correct Driver Selection**: Save function now uses UV5RRadio for UV-5R data, THD75Radio for Kenwood data
2. **Proper Metadata**: Saved .img files have correct vendor/model metadata
3. **Bank Name Handling**: UV-5R correctly shows no banks (empty array) instead of attempting to read TH-D75 banks
4. **Backwards Compatibility**: Defaults to TH-D75 for old files without vendor/model info
5. **Extensibility**: Easy to add new radio types - just add another match arm in both functions

## Testing

- All 21 UV-5R driver unit tests pass
- Code builds successfully with no errors
- No clippy warnings related to changes

## Expected Behavior After Fix

When you download from a UV-5R radio and save the .img file:

1. Download completes successfully and stores "Baofeng"/"UV-5R" in AppState
2. GUI shows memories with correct data (no bank columns for UV-5R)
3. Save operation uses UV5RRadio to encode memories
4. .img file is created with correct Baofeng/UV-5R metadata
5. File can be re-loaded and uploaded back to the radio

## Next Steps

1. Test saving a UV-5R .img file after download
2. Verify the saved file can be loaded with `parse_memory` tool
3. Verify the saved file can be uploaded back to the radio
4. Check that GUI displays UV-5R memory fields correctly (no nonsensical values)
