# UV-5R Complete Bug Fixes Summary

This document summarizes all the bugs discovered and fixed in the UV-5R driver during testing with actual hardware.

## Overview

Through systematic testing with real UV-5R hardware, we discovered and fixed four critical encoding bugs:

1. **Byte 12 bit field positions** (isuhf and scode)
2. **Power level encoding** (lowpower field values)
3. **TSQL tone encoding** (using rtone instead of ctone)
4. **Dynamic power field UI** (conditional visibility)

All fixes have been verified against CHIRP Python source code and documented.

---

## Bug #1: Byte 12 Encoding (CRITICAL)

### Symptoms
- **Radio display**: Both "+" and "-" indicators lit simultaneously
- **Offset display**: 0.00 instead of expected value (e.g., 600 kHz)
- **Impact**: All memory uploads affected - duplex/offset completely broken

### Root Cause

**Incorrect bit field positions in byte 12**

According to CHIRP source (`/home/pgrace/repos/chirp/chirp/drivers/uv5r.py` lines 38-40):
```c
u8 unused1:3,    // bits 0-2: unused (3 bits)
   isuhf:1,      // bit 3: band indicator (1=UHF, 0=VHF)
   scode:4;      // bits 4-7: PTT-ID code (4 bits)
```

Correct bit layout:
```
Bit:    7   6   5   4   3   2   1   0
Field: [scode....] [i] [unused......]
```

### Our Incorrect Code

**Decoding** (line ~110):
```rust
let isuhf = (data[12] & 0x01) != 0;      // Reading bit 0 ❌
let scode = (data[12] >> 1) & 0x0F;      // Reading bits 1-4 ❌
```

**Encoding** (line ~153):
```rust
bytes[12] = (if self.isuhf { 0x01 } else { 0x00 })  // Writing to bit 0 ❌
          | ((self.scode & 0x0F) << 1);              // Writing to bits 1-4 ❌
```

### Corrected Code

**Decoding** (line ~110):
```rust
let isuhf = (data[12] & 0x08) != 0;      // Reading bit 3 ✓
let scode = (data[12] >> 4) & 0x0F;      // Reading bits 4-7 ✓
```

**Encoding** (line ~153):
```rust
bytes[12] = (if self.isuhf { 0x08 } else { 0x00 })  // Writing to bit 3 ✓
          | ((self.scode & 0x0F) << 4);              // Writing to bits 4-7 ✓
```

### Why This Caused the Problem

When we incorrectly wrote to bits 0-4 instead of bits 3-7:
1. **We overwrote bits 0-2** (which should be unused)
2. **isuhf flag was in wrong position** (bit 0 instead of bit 3)
3. **scode was shifted wrong** (bits 1-4 instead of bits 4-7)

This corruption of byte 12 caused the radio's display logic to malfunction:
- Radio became confused about the band (VHF vs UHF)
- Unknown bits being set in unused area triggered undefined behavior
- Display showing both "+" and "-" indicates radio was in invalid state

### Files Modified
- `src/drivers/uv5r.rs`: Lines ~110, ~153

### Related Documentation
- See `UV5R_BYTE12_BUG_FIX.md` for detailed analysis

---

## Bug #2: Power Level Encoding

### Symptoms
- **User action**: Set power to "Low" in GUI
- **Radio displayed**: "MID" (middle power level)
- **Expected**: "LOW"
- **Strange behavior**: Radio UI only shows "HIGH" and "LOW" options, no "MID"!

### Root Cause

**Incorrect lowpower field values**

We were using:
- `lowpower = 0` for High ✓
- `lowpower = 2` for Low ❌

Correct encoding (from CHIRP source):

**Standard UV-5R** (2 power levels):
```python
UV5R_POWER_LEVELS = [
    PowerLevel("High", watts=4.00),  # lowpower=0
    PowerLevel("Low",  watts=1.00)   # lowpower=1
]
```

**Tri-Power UV-5R variants** (3 power levels):
```python
UV5R_POWER_LEVELS3 = [
    PowerLevel("High", watts=8.00),  # lowpower=0
    PowerLevel("Med",  watts=4.00),  # lowpower=1
    PowerLevel("Low",  watts=1.00)   # lowpower=2
]
```

The `lowpower` field is a **2-bit value** (bits 0-1 of byte 14):
- `0` = High power
- `1` = Low power (standard) OR Med power (tri-power variants)
- `2` = Low power (tri-power variants only)
- `3` = Invalid/unused

### Why It Showed "MID"

When we set `lowpower = 2` for "Low":
1. **Standard UV-5R**: Only understands values 0 (High) and 1 (Low)
2. **Radio received**: lowpower=2
3. **Radio interpreted**: Third power level (which is "Mid" on tri-power radios)
4. **Radio displayed**: "MID" (even though it shouldn't exist!)

The radio has **hidden support for 3 power levels** in the memory structure, but the standard UV-5R only has 2 physical power levels.

### The Fix

**Encoding** (lines ~621-633):
```rust
let lowpower = if let Some(ref power) = mem.power {
    if power.watts() < 2.5 {
        1 // Low (1W) ✓ - CHANGED from 2
    } else {
        0 // High (4W)
    }
} else {
    0 // Default to high
};
```

**Decoding** (lines ~455-465):
```rust
let power_index = match raw.lowpower {
    0 => 0, // High
    1 => 1, // Low
    2 => 1, // Mid (treat as Low on standard UV-5R)
    _ => 0, // Invalid, default to High
};
```

**Note**: We treat `lowpower=2` (Mid) as "Low" when decoding, in case the radio has a memory programmed by a tri-power variant or other software.

### Files Modified
- `src/drivers/uv5r.rs`: Lines ~77, ~455-465, ~621-633

### Related Documentation
- See `UV5R_POWER_ENCODING_FIX.md` for detailed analysis

---

## Bug #3: TSQL Tone Encoding

### Symptoms
- **Binary comparison**: TX frequency showed as simplex instead of +600kHz offset
- **Tone values**: before.bin had rxtone=0, after.bin had rxtone=91.5Hz

### Root Cause

**TSQL mode was using mem.rtone instead of mem.ctone**

In TSQL (Tone Squelch) mode, both TX and RX should use the **same** tone. This is stored in the `ctone` field, not `rtone`.

### Our Incorrect Code

```rust
"TSQL" => {
    txtone = tone_to_u16(mem.rtone);  // ❌ Using rtone
    rxtone = tone_to_u16(mem.rtone);  // ❌ Using rtone
}
```

### Correct Code (from CHIRP)

CHIRP source (`/home/pgrace/repos/chirp/chirp/drivers/uv5r.py` lines 1144-1151):
```python
elif mem.tmode == "TSQL":
    _mem.txtone = int(mem.ctone * 10)  # Uses ctone
    _mem.rxtone = int(mem.ctone * 10)  # Uses ctone
```

### The Fix

```rust
"TSQL" => {
    // Note: For TSQL, both use ctone (not rtone)
    txtone = tone_to_u16(mem.ctone);  // ✓
    rxtone = tone_to_u16(mem.ctone);  // ✓
}
```

### Important Note

During testing, the user noted: **"The baofeng written memory may have inadvertently turned on TSQL mode."**

This suggests that the official Baofeng app (which created after.bin) may have changed the tone mode during the write operation. The TSQL encoding fix is correct according to CHIRP specification, but the test scenario might not have been intended to use TSQL mode.

### Files Modified
- `src/drivers/uv5r.rs`: `encode_tone_mode()` function

---

## Bug #4: Dynamic Power Field UI

### Symptoms
- **Initial issue**: Power field wasn't showing in edit dialog for UV-5R
- **Secondary issue**: When changing power from "High" to "Low", value disappeared from table

### Root Cause #1: Missing Feature Flag

UV-5R's `get_features()` had `valid_power_levels` populated but was missing `has_variable_power: true`.

**Fix**:
```rust
fn get_features(&self) -> RadioFeatures {
    RadioFeatures {
        has_variable_power: true,  // ✓ ADDED
        valid_power_levels: POWER_LEVELS
            .iter()
            .map(|(label, watts)| PowerLevel::from_watts(*label, *watts))
            .collect(),
        // ... other fields
    }
}
```

### Root Cause #2: PowerLevel Parsing

The `update_memory()` function was using `PowerLevel::parse()` to parse power strings, but `parse()` only handles numeric formats like "5W", not labels like "High" or "Low".

**Fix**: Changed to look up PowerLevel by label in radio's valid_power_levels:
```rust
let power_level = if !power_str.is_empty() {
    let features = match (vendor, model) {
        ("Baofeng", "UV-5R") => {
            use crate::drivers::uv5r::UV5RRadio;
            UV5RRadio::new().get_features()
        }
        // ... other radios
    };
    features.valid_power_levels.iter()
        .find(|p| p.label() == power_str)
        .cloned()
} else {
    None
};
```

### Files Modified
- `src/drivers/uv5r.rs`: `get_features()` function
- `src/gui/qt_gui.rs`: `update_memory()` function

### Related Documentation
- See `DYNAMIC_POWER_FIELD.md` for detailed implementation

---

## Testing Status

### ✅ Completed Tests
- [x] Build successful with all fixes
- [x] Byte 12 encoding verified against CHIRP source
- [x] Power level encoding verified against CHIRP source
- [x] TSQL tone encoding verified against CHIRP source
- [x] Dynamic UI field logic tested

### 🔄 Pending Hardware Tests
- [ ] Upload memory with duplex "+" and 600kHz offset
- [ ] Verify radio displays "+" (not both "+/-")
- [ ] Verify radio displays correct offset (600kHz, not 0.00)
- [ ] Upload memory with "Low" power setting
- [ ] Verify radio displays "LOW" (not "MID")
- [ ] Test various tone modes (Tone, TSQL, DTCS)

---

## Files Modified Summary

1. **src/drivers/uv5r.rs**:
   - Line ~110: Fixed byte 12 decoding (isuhf from bit 3, scode from bits 4-7)
   - Line ~153: Fixed byte 12 encoding (isuhf to bit 3, scode to bits 4-7)
   - Lines ~455-465: Fixed power decoding (handle all 3 lowpower values)
   - Lines ~621-633: Fixed power encoding (use lowpower=1 for Low)
   - `encode_tone_mode()`: Fixed TSQL to use ctone instead of rtone
   - `get_features()`: Added has_variable_power flag

2. **src/gui/qt_gui.rs**:
   - `update_memory()`: Changed power parsing to label lookup
   - `get_radio_features()`: Added FFI function for dynamic UI
   - `showEditDialog()`: Added conditional power field visibility
   - `load_file()`: Set radio_vendor and radio_model

---

## References

- **CHIRP Python source**: `/home/pgrace/repos/chirp/chirp/drivers/uv5r.py`
  - Lines 38-40: Memory structure definition (byte 12 layout)
  - Lines 318-319: Power level lists (UV5R_POWER_LEVELS)
  - Lines 764-769: PowerLevel definitions
  - Lines 1013-1022: Decode logic
  - Lines 1144-1151: Encode logic (TSQL uses ctone)

---

## Key Takeaways

1. **Always verify bit field positions**: Don't assume documentation is correct - check reference implementations
2. **Test with actual hardware**: File parsing alone won't catch encoding bugs
3. **Check for variant differences**: UV-5R has many variants with different capabilities
4. **TSQL mode uses ctone**: Both TX and RX use the same tone from ctone field
5. **Radio has hidden features**: Standard UV-5R can display "MID" even though it's not physically supported

---

## Future Enhancements

To support tri-power UV-5R variants (UV-5R+, BF-F8HP):
1. Add variant detection in `UV5RRadio::new()` or during handshake
2. Add "Med" (4W) to `POWER_LEVELS` constant for tri-power radios
3. Update `get_features()` to return 3 power levels for tri-power variants
4. Adjust encoding/decoding to use 0/1/2 mapping for tri-power
5. Update GUI to show "High"/"Med"/"Low" dropdown for tri-power radios

For now, we correctly support the standard 2-level UV-5R, which is the most common variant.
