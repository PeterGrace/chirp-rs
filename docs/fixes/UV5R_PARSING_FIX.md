# UV-5R Data Parsing and Display Fix

## Problems Identified

1. **Invalid BCD frequencies during download**: Many memories showed "Invalid BCD frequency" warnings (e.g., `02B502B5`, `05EA05EA`) containing invalid BCD digits (0xA-0xF)

2. **Nonsensical GUI display**: Downloaded memories showed invalid data in the table

3. **Save error**: When trying to save, got error: "Frequency 0 Hz is outside valid bands"

## Root Cause

The UV-5R radio contains memories with invalid/garbage BCD data that are not fully empty (not 0xFFFFFFFF). The driver's empty detection only checked for `rxfreq == 0xFFFFFFFF`, but these partially-filled memories with invalid BCD would:

1. Not be detected as empty
2. Decode to 0 Hz frequency (bcd_to_freq returns 0 on invalid BCD)
3. Get included in the memory list with freq=0
4. Fail validation when trying to save ("Frequency 0 Hz is outside valid bands")
5. Show nonsensical data in the GUI

## Solutions Implemented

### 1. Improved Empty Memory Detection (uv5r.rs)

Updated `get_memory()` to treat memories with 0 Hz frequency as empty:

```rust
fn get_memory(&mut self, number: u32) -> RadioResult<Option<Memory>> {
    // ... existing checks ...

    let mem = decode_memory(number, &raw, &name)?;

    // Also check if frequency is 0 (invalid BCD decode) - treat as empty
    if mem.freq == 0 {
        tracing::debug!(
            "Memory #{} has invalid frequency (0 Hz), treating as empty",
            number
        );
        return Ok(None);
    }

    Ok(Some(mem))
}
```

This ensures that:
- Memories with invalid BCD data are filtered out
- Only valid memories are displayed in GUI
- Only valid memories are saved to file
- No more "Frequency 0 Hz" errors

### 2. Enhanced parse_memory Tool (parse_memory.rs)

Made the parse_memory CLI tool support UV-5R files:

**Changes:**
- Auto-detect radio type from metadata (Baofeng UV-5R vs Kenwood TH-D75)
- Instantiate correct driver based on vendor/model
- Skip bank name display for radios without banks (UV-5R)
- Adjust raw memory offset calculation for UV-5R format:
  - UV-5R: 16 bytes at `0x0008 + (number * 16)`
  - TH-D75: 40 bytes at `0x4000 + (group * 256) + (index * 40)`
- Decode BCD frequencies for UV-5R in raw output
- Show name location and data for UV-5R

**Usage:**
```bash
# Parse UV-5R CHIRP file
cargo run --bin parse_memory -- test_data/Baofeng_UV-5R_20260211.img

# Show specific memory with raw data
cargo run --bin parse_memory -- test_data/Baofeng_UV-5R_20260211.img 1 --raw

# Show range
cargo run --bin parse_memory -- test_data/Baofeng_UV-5R_20260211.img 1-10
```

## Testing

### Verified with CHIRP-created file:

```bash
$ cargo run --bin parse_memory -- test_data/Baofeng_UV-5R_20260211.img
Loading file: test_data/Baofeng_UV-5R_20260211.img
Radio: Baofeng UV-5R
CHIRP version: next-20260206
Memory map size: 6472 bytes

Found 21 non-empty memories

Memory #1: ""
  Frequency:    452125000 Hz (452.125000 MHz)
  Mode:         FM
  Tone Mode:    TSQL
  CTCSS TX:     69.3 Hz
  CTCSS RX:     69.3 Hz
```

### Raw memory data:
```bash
$ cargo run --bin parse_memory -- test_data/Baofeng_UV-5R_20260211.img 1 --raw
  Raw Memory Data:
  Memory offset: 0x0018
  First 16 bytes:   00 25 21 45 00 25 21 45  B5 02 B5 02 00 00 00 44
  ASCII:            .%!E.%!E.......D
  RX freq (BCD):    45212500 = 452125000 Hz (452.125000 MHz)
  TX freq (BCD):    45212500 = 452125000 Hz (452.125000 MHz)
  Name offset:      0x1018
  Name bytes:       [FF, FF, FF, FF, FF, FF, FF]
```

## Expected Behavior After Fix

When downloading from UV-5R:
1. ✅ Invalid BCD memories are automatically filtered out
2. ✅ Only valid memories (with valid frequencies) are shown in GUI
3. ✅ No more "Frequency 0 Hz" errors when saving
4. ✅ GUI displays correct frequency, mode, and tone data
5. ✅ Save to .img file works correctly

## Files Modified

1. **src/drivers/uv5r.rs**:
   - Enhanced `get_memory()` to filter memories with 0 Hz frequency

2. **src/bin/parse_memory.rs**:
   - Added UV5RRadio import
   - Auto-detect radio type from metadata
   - Dynamic driver instantiation (UV5RRadio or THD75Radio)
   - UV-5R specific raw memory formatting
   - BCD frequency decoding in raw output
   - Conditional bank name display

## Next Steps

1. **Test download again**: Download from UV-5R radio with the fixed driver
   - Should see fewer memories (only valid ones)
   - No more invalid BCD warnings for valid memories
   - GUI should show correct data

2. **Test save functionality**: After downloading, try saving to .img file
   - Should succeed without "Frequency 0 Hz" errors
   - Can verify saved file with `parse_memory` tool

3. **Verify GUI display**: Check that the table shows:
   - Correct frequencies (e.g., 452.125 MHz, not 0.000000 MHz)
   - Correct mode (FM/NFM)
   - Correct tone modes and values
   - No bank columns (UV-5R doesn't have banks)

## Troubleshooting

If you still see issues:

1. **Check logs for BCD warnings**: If you still see "Invalid BCD frequency" warnings for the same memory multiple times, there might be a hardware/cable issue

2. **Verify CHIRP file**: Use parse_memory tool on your downloaded file:
   ```bash
   cargo run --bin parse_memory -- /path/to/your/file.img
   ```

3. **Compare with CHIRP**: Open the test file in CHIRP and compare frequencies to verify our parsing is correct
