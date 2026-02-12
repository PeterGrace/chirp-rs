# UV-5R Power Level Encoding Bug Fix

## Problem Discovered

When setting a memory's power to "Low" and uploading to the radio:
- **Radio displayed**: "MID" (Middle power level)
- **Expected**: "LOW"
- **Radio UI limitation**: Only shows "HIGH" and "LOW" options, no "MID"!

This revealed an incorrect power level encoding in our UV-5R driver.

## Root Cause

### Our Incorrect Encoding
```rust
// WRONG: We were using 0=High, 2=Low
let lowpower = if power.watts() < 2.5 {
    2 // Low  ← WRONG!
} else {
    0 // High
};
```

### Correct Encoding (from CHIRP source)
According to `/home/pgrace/repos/chirp/chirp/drivers/uv5r.py`:

**Standard UV-5R (2 power levels):**
```python
UV5R_POWER_LEVELS = [
    chirp_common.PowerLevel("High", watts=4.00),  # lowpower=0
    chirp_common.PowerLevel("Low",  watts=1.00)   # lowpower=1
]
```

**Tri-Power UV-5R variants (3 power levels):**
```python
UV5R_POWER_LEVELS3 = [
    chirp_common.PowerLevel("High", watts=8.00),  # lowpower=0
    chirp_common.PowerLevel("Med",  watts=4.00),  # lowpower=1
    chirp_common.PowerLevel("Low",  watts=1.00)   # lowpower=2
]
```

The `lowpower` field is a **2-bit value** (bits 0-1 of byte 14):
- `0` = High power
- `1` = Low power (standard UV-5R) OR Med power (tri-power variants)
- `2` = Low power (tri-power variants only)
- `3` = Invalid/unused

## Why Our Bug Caused "MID" to Display

When we set `lowpower = 2` for "Low":
1. **Standard UV-5R**: Only understands lowpower values 0 (High) and 1 (Low)
2. **Radio received**: lowpower=2
3. **Radio interpreted**: Third power level (which would be "Mid" on tri-power radios)
4. **Radio displayed**: "MID" (even though it shouldn't exist!)
5. **Radio UI**: Only shows High/Low because it's not a tri-power variant

The radio has **hidden support for 3 power levels** in the memory structure, but the standard UV-5R:
- Only has 2 physical power levels (High=4W, Low=1W)
- Only shows 2 options in the UI (High, Low)
- But will display "MID" if the memory contains lowpower=1 or 2 (depending on variant)

## The Fix

### Encoding (Memory → Radio)
```rust
let lowpower = if let Some(ref power) = mem.power {
    // Match power level (High=0, Low=1)
    if power.watts() < 2.5 {
        1 // Low (1W)  ← CORRECT!
    } else {
        0 // High (4W)
    }
} else {
    0 // Default to high
};
```

### Decoding (Radio → Memory)
```rust
// Power level (lowpower: 0=High, 1=Low, 2=Mid on tri-power variants)
let power_index = match raw.lowpower {
    0 => 0, // High
    1 => 1, // Low
    2 => 1, // Mid (treat as Low on standard UV-5R)
    _ => 0, // Invalid, default to High
};
```

**Note**: We treat `lowpower=2` (Mid) as "Low" when decoding, in case the radio has a memory programmed by a tri-power variant or other software.

## Tri-Power Variants

Some UV-5R variants support 3 power levels:
- **UV-5R+**: May support High/Mid/Low (8W/4W/1W)
- **UV-5RA**: May support High/Mid/Low
- **BF-F8HP**: High power variant (8W/4W/1W)

CHIRP uses a `_tri_power` flag to distinguish these:
```python
if self._tri_power:
    levels = UV5R_POWER_LEVELS3  # High, Med, Low
else:
    levels = UV5R_POWER_LEVELS   # High, Low
```

Our driver currently only supports the standard 2-level encoding. To support tri-power variants, we would need to:
1. Detect the radio variant
2. Add "Med" as a power option
3. Use appropriate encoding (0/1/2 instead of 0/1)

## Testing

### Before Fix:
```
User sets: "Low" (1W)
We encoded: lowpower=2
Radio showed: "MID"  ← WRONG!
```

### After Fix:
```
User sets: "Low" (1W)
We encode: lowpower=1
Radio shows: "LOW"  ← CORRECT!
```

## Files Modified

**src/drivers/uv5r.rs**:
1. **Line 77**: Updated comment on RawMemory struct
2. **Lines 455-465**: Fixed decode_memory() to use lowpower values 0/1 correctly
3. **Lines 621-633**: Fixed encode_memory() to set lowpower=1 (not 2) for "Low"

## References

- CHIRP Python source: `/home/pgrace/repos/chirp/chirp/drivers/uv5r.py`
  - Lines 318-319: Power level lists
  - Lines 764-769: PowerLevel definitions
  - Lines 1013-1022: Decode logic
  - Lines 1144-1151: Encode logic

## Future Enhancements

To support tri-power UV-5R variants:
1. Add variant detection in `UV5RRadio::new()` or during handshake
2. Add "Med" (4W) to `POWER_LEVELS` constant for tri-power radios
3. Update `get_features()` to return 3 power levels for tri-power variants
4. Adjust encoding/decoding to use 0/1/2 mapping for tri-power
5. Update GUI to show "High"/"Med"/"Low" dropdown for tri-power radios

For now, we correctly support the standard 2-level UV-5R, which is the most common variant.
