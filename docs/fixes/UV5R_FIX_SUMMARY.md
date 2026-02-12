# UV-5R Driver Fix Summary

## Problem Identified

The UV-5R driver was implemented correctly, but the GUI's radio operations module (`src/gui/radio_ops.rs`) was hardcoded to always instantiate a TH-D75 driver for all clone mode radios, regardless of which radio was selected.

## Root Cause

In `src/gui/radio_ops.rs`, both `download_clone_mode()` and `upload_clone_mode()` functions had this problematic code:

```rust
// This was wrong - always created TH-D75!
let mut driver = THD75Radio::new();
```

The log showed the system correctly identified "Baofeng UV-5R" but then called TH-D75 protocol methods (`detect_baud`, `get_id`), which don't exist in the UV-5R protocol.

## Changes Made

### 1. Fixed Driver Instantiation (radio_ops.rs)

**`download_clone_mode()` function:**
- Added import for `UV5RRadio`
- Changed function to match on vendor/model and instantiate the correct driver:
  - `("baofeng", "UV-5R")` → creates `UV5RRadio`
  - `("kenwood", "TH-D75"|"TH-D74")` → creates `THD75Radio`
- Each driver now properly calls its own `sync_in()` and `get_memories()` methods

**`upload_clone_mode()` function:**
- Added import for `UV5RRadio`
- Changed function to match on vendor/model and instantiate the correct driver
- Each driver branch handles its own memory limits (UV-5R: 128 channels, TH-D75: 1200 channels)
- Properly clones progress callback to avoid borrow checker issues

### 2. Added Baofeng Serial Port Configuration (radio_ops.rs)

Added proper DTR/RTS settings for Baofeng radios in both download and upload paths:

```rust
else if vendor.to_lowercase() == "baofeng" {
    tracing::debug!("Setting DTR=false, RTS=false for Baofeng radio");
    port.set_dtr(false).map_err(|e| format!("Failed to set DTR: {}", e))?;
    port.set_rts(false).map_err(|e| format!("Failed to set RTS: {}", e))?;
    tracing::debug!("Baofeng: DTR/RTS configured");
}
```

Baofeng clone cables typically don't use DTR/RTS signaling, so both are set to false.

### 3. Made UV5RRadio.mmap Public (uv5r.rs)

Changed:
```rust
pub struct UV5RRadio {
    pub mmap: Option<MemoryMap>,  // Made public
    vendor: String,
    model: String,
}
```

This allows the upload function to access the modified memory map after updating memories, consistent with how TH-D75 works.

## Testing the Fix

### Prerequisites
1. Baofeng UV-5R radio
2. Programming cable (K-plug or compatible USB cable)
3. Radio must be powered on
4. No need to put radio in special mode - the driver handles handshake

### Expected Behavior

When you initiate a download from the GUI, you should now see in the logs:

```
Creating UV-5R driver instance
Trying magic sequence: [50 BB FF 20 12 07 25]
Received ACK after magic
Received ident: [...]
Handshake successful with 291 magic
Downloading from radio
```

The radio should respond to the handshake and begin transferring data.

### Troubleshooting

If the download still times out:

1. **Check Cable Connection**
   - Make sure the K-plug is fully inserted
   - Try unplugging and re-plugging the USB cable

2. **Verify Serial Port**
   - Confirm `/dev/ttyUSB0` is the correct port
   - Check permissions: `ls -l /dev/ttyUSB0`
   - Add user to dialout group if needed: `sudo usermod -a -G dialout $USER`

3. **Try Different Magic Sequence**
   - The driver tries two magic sequences: UV5R_MODEL_291 and UV5R_MODEL_ORIG
   - If both fail, your radio variant might need a different sequence
   - Check which sequence is logged in the attempt

4. **Cable Type**
   - Some cheap cables have issues with timing
   - Official or FTDI-based cables work more reliably
   - Check if cable works with CHIRP Python version

5. **Radio Variants**
   - UV-5R has many variants (UV-5R+, BF-F8HP, UV-82, etc.)
   - The current implementation supports base UV-5R protocol
   - Some variants may need protocol adjustments

## Protocol Details

The UV-5R clone mode protocol:

1. **Handshake:**
   - Send magic bytes (7 bytes) with 10ms delay between each
   - Wait for ACK (0x06)
   - Send 0x02
   - Read ident (8-12 bytes ending with 0xDD)
   - Send ACK
   - Wait for final ACK

2. **Download:**
   - Send "S" + addr (u16 BE) + size (u8)
   - Receive "X" + addr (u16 BE) + size (u8) + data
   - Send ACK
   - Repeat for all blocks (64 bytes per block, 0x0000-0x1800)

3. **Upload:**
   - Send "X" + addr (u16 BE) + size (u8) + data
   - Receive ACK
   - Repeat for all blocks (16 bytes per block)
   - Skip ranges: 0x0CF8-0x0D08, 0x0DF8-0x0E08

## Files Modified

1. `src/gui/radio_ops.rs` - Fixed driver instantiation and added Baofeng serial config
2. `src/drivers/uv5r.rs` - Made mmap field public

## Verification

All UV-5R driver tests pass:
```
test result: ok. 21 passed; 0 failed
```

Project builds successfully in release mode.

## Next Steps

1. Test with actual UV-5R hardware
2. If successful, document any cable-specific quirks
3. If handshake fails, capture log and check magic sequence compatibility
4. Consider adding support for UV-5R variants (UV-5R+, BF-F8HP, UV-82, etc.)
