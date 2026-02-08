# Change Log

## 2026-02-07 - Bank/Group Support (commit a07b667)

### Added
- **Bank/Group field** to Memory struct
  - Supports organizing memories into logical banks (0-9)
  - Bank assignments preserved in encode/decode cycle
  - Added to CSV export
- **GUI Bank column** in Qt memory table
  - Display bank assignment for each memory
  - 50-pixel column width
- **Tests** for bank preservation
  - `test_bank_assignments()` - Verify bank structure from radio data
  - Updated `test_encode_memories_full()` - Verify bank preservation in roundtrip

### Implementation
- `src/core/memory.rs`: Added `bank: u8` field
- `src/drivers/thd75.rs`: Read/write bank from MemoryFlags.group
- `src/gui/qt_gui.rs`: Added Bank column to table (column 13)

### Verified Bank Structure
- Bank 0: Memories 0-100
- Bank 1: Memories 101-199
- Bank 2: Memories 200-202

## 2026-02-07 - Iced GUI Removal (commit f3f6b5d)

### Removed
- `src/gui/app.rs` (~689 lines) - Iced MVU application
- `src/gui/messages.rs` (~53 lines) - Iced message types
- `src/gui/dialogs.rs` (~136 lines) - Iced dialog widgets
- **Total: ~878 lines of unused code removed**

### Kept
- `src/gui/qt_gui.rs` - Qt GUI (main UI)
- `src/gui/radio_ops.rs` - Radio operations (used by Qt GUI)

### Rationale
- Project standardized on Qt GUI exclusively
- No iced dependencies in Cargo.toml
- Qt provides better native integration and table widgets

## 2026-02-07 - Save to .img Functionality (commit b2191ef)

### Added
- **Full encode/save implementation** for TH-D75 memories
- **5 encoding helper functions** with proper error handling:
  - `find_tone_index()` - Convert tone frequency to index
  - `find_dtcs_index()` - Convert DTCS code to index
  - `find_tuning_step_index()` - Convert step to index
  - `find_mode_index()` - Convert mode string to index
  - `parse_duplex()` - Convert duplex string to enum
- **Core encoding functions**:
  - `encode_memory()` - Convert Memory → RawMemory + MemoryFlags
  - `encode_memories()` - Convert Vec<Memory> → MemoryMap
  - `set_memory()` - Update individual memory in MemoryMap
- **GUI save handlers**:
  - Updated `save_file()` FFI function in qt_gui.rs
  - Added `get_current_filepath()` helper
  - Updated Save and Save As menu actions
  - Success/error dialogs with user feedback

### Tests Added
1. `test_encode_decode_roundtrip` - Single memory roundtrip
2. `test_encode_memories_full` - Full 1200-memory roundtrip
3. `test_encode_dv_memory` - D-STAR memory encoding
4. `test_helper_functions` - Unit tests for lookup functions

### Key Features
- Perfect roundtrip compatibility (decode → encode → decode)
- Handles all memory types: FM, DV/D-STAR, AM, SSB
- Proper D-STAR call sign encoding
- Descriptive error messages
- All 11 TH-D75 tests passing

### User Workflow
1. Load `.img` file or download from radio
2. Edit memories in GUI
3. Click File → Save or Save As
4. GUI encodes to MemoryMap and saves
5. File can be reloaded - all changes persist

## 2026-02-07 - D-STAR Field Support

### Added
- D-STAR call sign fields to Memory struct:
  - `dv_urcall` - URCALL (destination callsign)
  - `dv_rpt1call` - RPT1CALL (repeater 1)
  - `dv_rpt2call` - RPT2CALL (repeater 2)
  - `dv_code` - Digital code
- GUI columns for URCALL, RPT1, RPT2
- Conditional display (only show for DV memories)
- CSV export includes D-STAR fields

### Implementation
- Parse from bytes 15-39 of raw memory
- Only populate for DV mode memories
- Empty strings for non-DV memories

### Discovery
- CHIRP CSV export bug: Shows D-STAR fields as empty even when populated
- Our implementation correctly parses from raw data

## 2026-02-07 - TH-D75 Memory Layout Discovery

### Critical Fix #1: Memory Size
**Before:** 80-byte memories (from old documentation)
**After:** 40-byte memories (actual hardware)

**Formula:**
- Structure: Groups of 6 memories + 16-byte padding
- Offset = `0x4000 + (group * (6*40 + 16)) + (index * 40)`
- Where: `group = mem_num / 6`, `index = mem_num % 6`

**Memory Structure (40 bytes):**
```
Bytes 0-3:   Frequency (u32 LE)
Bytes 4-7:   Offset (u32 LE)
Byte 8:      Tuning step
Byte 9:      Mode + narrow flags
Byte 10:     Tone modes + duplex
Byte 11:     TX tone index
Byte 12:     RX tone index
Byte 13:     DTCS code
Byte 14:     Digital squelch
Bytes 15-22: D-STAR URCALL
Bytes 23-30: D-STAR RPT1CALL
Bytes 31-38: D-STAR RPT2CALL
Byte 39:     D-STAR code
```

### Critical Fix #2: Duplex Bit Position
**Before:** Bits 6-7 of byte 10 (from documentation)
**After:** Bits 0-1 of byte 10 (actual hardware)

**Byte 10 Layout:**
- Bits 0-1: duplex (00=simplex, 01='+', 10='-')
- Bit 2: dtcs_mode
- Bit 3: cross_mode
- Bit 5: split
- Bits 6-7: tone_mode/ctcss_mode flags

**Impact:**
- All 1200 memories now decode correctly
- Fixed memories 32-50 (previously corrupt)
- Fixed memories 0-31 (incorrect offsets before)

### How Discovered
1. Initial: Used 80-byte memories, groups of 3
2. Observation: Memories 0-31 worked, 32-50 corrupt (0xFFFFFFFF)
3. Comparison: Checked offsets vs official CHIRP .img
4. Analysis: Systematic search found 40-byte spacing
5. Verification: Groups of 6 + 16-byte padding matches data

## Earlier Development

### GUI Framework: Qt (C++ bindings)
**Decision:** Use Qt via qmetaobject-rs and cpp! macro
**Rationale:**
- Native look and feel on all platforms
- Excellent table widget (QTableWidget)
- Mature ecosystem
- Better file dialogs and system integration
- Rust-Qt bindings via qmetaobject crate

### Architecture
**Core modules:**
- `core/` - Memory structures, constants
- `drivers/` - Radio-specific implementations
- `formats/` - File format handlers (.img, .csv)
- `serial/` - Serial port communication
- `memmap/` - Memory map abstraction
- `bitwise/` - Binary parsing utilities
- `gui/` - Qt-based GUI

### TH-D75 Driver Features
- 1200 memory channels
- USB serial communication (clone mode)
- Automatic baud rate detection (9600 → 57600)
- Progress tracking for downloads/uploads
- Error recovery and retries
- Full D-STAR support
- CTCSS/DCS tone support
- Bank/Group organization

### File Formats
- **IMG Format**: JSON metadata + base64-encoded binary
  - Compatible with original CHIRP
  - Includes radio model, vendor info
  - Binary memory map preserved exactly
- **CSV Format**: Tabular export
  - All memory fields included
  - D-STAR fields
  - Bank assignments
  - Compatible with spreadsheets

### Testing Strategy
- Unit tests for all encoding/decoding
- Real radio data in `test_data/`
- Roundtrip verification
- 76 tests, all passing
- Test coverage for edge cases

## Contributing

See [README.md](README.md) for build instructions and [CLAUDE.md](CLAUDE.md) for coding standards.
