# UV-5R Byte 12 Encoding Bug Fix

## Problem Discovered

When uploading memories with offset/duplex to the UV-5R:
- **Radio displayed**: Both "+" and "-" indicators lit at the same time
- **Offset showed**: 0.00 instead of expected value (e.g., 600 kHz)

## Root Cause

**Incorrect bit field encoding in Byte 12**

According to CHIRP's memory structure (`/home/pgrace/repos/chirp/chirp/drivers/uv5r.py` line 38-40):

```c
u8 unused1:3,    // bits 0-2: unused (3 bits)
   isuhf:1,      // bit 3: band indicator (1=UHF, 0=VHF)
   scode:4;      // bits 4-7: PTT-ID code (4 bits)
```

**Correct bit layout**:
```
Bit:    7   6   5   4   3   2   1   0
Field: [scode....] [i] [unused...]
```

### Our Incorrect Code

**Decoding (WRONG)**:
```rust
// Byte 12: isuhf (bit 0), scode (bits 1-4)  ← WRONG positions!
let isuhf = (data[12] & 0x01) != 0;      // Reading bit 0
let scode = (data[12] >> 1) & 0x0F;      // Reading bits 1-4
```

**Encoding (WRONG)**:
```rust
// Byte 12: isuhf (bit 0), scode (bits 1-4)  ← WRONG positions!
bytes[12] = (if self.isuhf { 0x01 } else { 0x00 })  // Writing to bit 0
          | ((self.scode & 0x0F) << 1);              // Writing to bits 1-4
```

### Corrected Code

**Decoding (CORRECT)**:
```rust
// Byte 12: unused1 (bits 0-2), isuhf (bit 3), scode (bits 4-7)
let isuhf = (data[12] & 0x08) != 0;      // Reading bit 3
let scode = (data[12] >> 4) & 0x0F;      // Reading bits 4-7
```

**Encoding (CORRECT)**:
```rust
// Byte 12: unused1 (bits 0-2), isuhf (bit 3), scode (bits 4-7)
bytes[12] = (if self.isuhf { 0x08 } else { 0x00 })  // Writing to bit 3
          | ((self.scode & 0x0F) << 4);              // Writing to bits 4-7
```

## Why This Caused Duplex/Offset Issues

When we incorrectly wrote to bits 0-4 instead of bits 3-7:
1. **We were overwriting bits 0-2** (which should be unused)
2. **isuhf flag was in the wrong position** (bit 0 instead of bit 3)
3. **scode was shifted wrong** (bits 1-4 instead of bits 4-7)

This corruption of byte 12 likely caused the radio's display logic to malfunction:
- Radio might have been confused about the band (VHF vs UHF)
- Unknown bits being set in the unused area might trigger unexpected display behavior
- The display showing both "+" and "-" suggests the radio was in an undefined state

## Impact

This bug affected:
1. **All memory uploads** - Every memory written had byte 12 corrupted
2. **Band detection** - isuhf flag in wrong position could confuse VHF/UHF logic
3. **PTT-ID** - scode in wrong position could cause PTT-ID issues
4. **Duplex display** - Corruption caused bizarre display behavior (both +/- shown)

## The Fix

Fixed both encoding and decoding of byte 12 to match CHIRP's bit field layout:

**src/drivers/uv5r.rs**:
1. **Line ~110**: Fixed decoding - read isuhf from bit 3, scode from bits 4-7
2. **Line ~153**: Fixed encoding - write isuhf to bit 3, scode to bits 4-7

## Testing

### Before Fix:
```
Memory with +600kHz offset uploaded
Radio showed: Both + and - lit, offset = 0.00
Byte 12 value: Incorrect bit positions
```

### After Fix:
```
Memory with +600kHz offset uploaded
Radio shows: Only + lit, offset = 600kHz
Byte 12 value: Correct bit positions (isuhf=bit3, scode=bits4-7)
```

## How This Was Discovered

1. User uploaded a memory with duplex="+" and offset=600kHz
2. Radio displayed both "+" and "-" indicators simultaneously
3. Radio showed offset as 0.00 instead of 600kHz
4. Investigation revealed byte 12 encoding mismatch with CHIRP
5. Verified against CHIRP Python source code structure definition

## References

- **CHIRP source**: `/home/pgrace/repos/chirp/chirp/drivers/uv5r.py`
  - Lines 38-40: Memory structure definition
  - Shows correct bit field layout for byte 12

## Additional Notes

This is a critical bug that affected ALL memory writes. The fact that:
- Downloads worked (we could read memories correctly from radio)
- But uploads caused display corruption

...suggests that the radio was tolerant of reading our incorrectly-encoded data, but when we wrote it back, the incorrect bit positions caused the radio's display and logic to malfunction.

This highlights the importance of:
1. Carefully matching bit field layouts to reference implementations
2. Testing both read AND write operations
3. Verifying behavior on actual hardware, not just parsing files
