# UV-5R Critical Upload Bug Fix

## Problem

After editing a memory and uploading to the radio, the radio became corrupted with invalid data showing frequencies like 39.3 MHz instead of valid UHF frequencies.

## Root Cause

**File offset to radio address mismatch in sync_out (upload) function:**

### File Format
Our .img files have:
- **0x0000-0x0007**: 8-byte header (ident bytes)
- **0x0008-0x1808**: Radio memory data (memory #0 starts at file offset 0x0008)

### Radio Addresses
The radio expects:
- **0x0000-0x000F**: Memory #0
- **0x0010-0x001F**: Memory #1
- etc.

### The Bug
```rust
// BUGGY CODE (line 977-982):
for addr in (start..end).step_by(WRITE_BLOCK_SIZE) {
    let data = mmap.get(addr, Some(size))?;      // Read from FILE offset 'addr'
    self.write_block(port, addr as u16, data).await?;  // Write to RADIO address 'addr'
    //                      ^^^^^^^^^^ BUG: addr includes the 8-byte header offset!
}
```

When uploading:
- Loop starts with `addr = 0x0008` (after the file header)
- Reads from **file offset 0x0008** (memory #0 data in file) ✓
- Writes to **radio address 0x0008** (middle of memory #0!) ✗

This caused an **8-byte shift**, corrupting all radio memory:
- Memory #0 got overwritten with tone bytes from memory #1
- All subsequent memories shifted by 8 bytes
- Radio became unusable

## The Fix

```rust
// FIXED CODE:
for file_offset in (start..end).step_by(WRITE_BLOCK_SIZE) {
    let data = mmap.get(file_offset, Some(size))?;  // Read from file

    // Convert file offset to radio address
    // File has 8-byte header at 0x0000-0x0007, radio expects data at 0x0000
    let radio_addr = (file_offset - 8) as u16;  // <-- KEY FIX!

    self.write_block(port, radio_addr, data).await?;  // Write to correct address
}
```

Now the mapping is correct:
- File offset 0x0008 → Radio address 0x0000 (memory #0) ✓
- File offset 0x0018 → Radio address 0x0010 (memory #1) ✓
- etc.

## Evidence from Corrupted Radio

Downloaded data from corrupted radio shows:
```
Memory #0 at offset 0x0008:
  Raw bytes: 93 03 93 03 00 00 00 44
  Frequency: 39.303930 MHz (INVALID!)
  TX offset: 440.000000 MHz (NONSENSICAL!)
```

The bytes `93 03 93 03` are actually **CTCSS tone values** from memory #1's tone field, proving the 8-byte shift occurred.

## Recovery Steps

**IMPORTANT: The radio is now corrupted and must be restored from a good backup!**

1. **Use the known-good CHIRP file to restore the radio:**
   ```bash
   # This file has valid data
   test_data/Baofeng_UV-5R_20260211.img
   ```

2. **After code is fixed, restore the radio:**
   - Rebuild the app: `cargo build --release`
   - Open the app
   - Load `test_data/Baofeng_UV-5R_20260211.img`
   - Upload to radio (with fixed code)
   - Verify by downloading again

3. **Test the fix:**
   - Download from radio
   - Verify memories parse correctly
   - Edit a memory
   - Upload to radio
   - Download again and verify the edit persisted correctly

## Files Modified

**src/drivers/uv5r.rs** - sync_out() function (lines 975-990):
- Renamed `addr` to `file_offset` for clarity
- Added `radio_addr = file_offset - 8` conversion
- Changed `write_block()` to use `radio_addr` instead of `addr`

## Testing Checklist

After fix:
- [ ] Build succeeds: `cargo build`
- [ ] Load good backup file in app
- [ ] Upload to radio with fixed code
- [ ] Download from radio
- [ ] Parse downloaded file - verify frequencies are correct (452-462 MHz range)
- [ ] Edit a memory in GUI
- [ ] Upload to radio
- [ ] Download from radio again
- [ ] Verify the edit persisted correctly
- [ ] Compare downloaded file with original - should match

## Why This Bug Wasn't Caught Earlier

1. **Download worked correctly** - The download (sync_in) code was correct all along
2. **Save to file worked** - Saving downloaded data to .img files worked
3. **Upload was untested** - We hadn't tested the upload (sync_out) path until now
4. **No validation** - The code didn't validate the radio address mapping

This is why comprehensive testing of **both directions** (download AND upload) is critical for radio drivers.

## Prevention

Added explicit comments in the code to document the file-offset-to-radio-address mapping. Future drivers should:
1. Clearly document file format vs radio address space
2. Test both download AND upload paths
3. Verify round-trip: download → edit → upload → download → compare
