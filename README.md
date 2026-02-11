# CHIRP-RS

A Rust port of [CHIRP](https://chirp.danplanet.com/), a tool for programming amateur radio equipment.

## Features

### Supported Radios
- **Kenwood TH-D75** - Full support with D-STAR capabilities
  - Read/write radio memory via USB
  - Load/save `.img` files
  - Import/export CSV files
  - D-STAR call sign support (URCALL, RPT1CALL, RPT2CALL)
  - Bank/Group organization

### Capabilities
- **Memory Management**
  - 1200 memory channels for TH-D75
  - Full memory editing (frequency, offset, duplex, mode, tones, etc.)
  - CTCSS/DCS tone support
  - Bank/Group organization
  - Skip/lockout flags

- **D-STAR Support**
  - URCALL (destination callsign)
  - RPT1CALL (repeater 1)
  - RPT2CALL (repeater 2)
  - Digital code support

- **File Formats**
  - `.img` - CHIRP image format (read/write)
  - `.csv` - CSV export format (write)

- **Radio Communication**
  - USB serial communication via clone mode
  - Automatic baud rate detection
  - Progress tracking for downloads/uploads
  - Error recovery and retries

## Building

### Prerequisites
- Rust 1.70+ (install from [rustup.rs](https://rustup.rs/))
- Qt 5.x (for GUI)
- C++ compiler (for Qt bindings)

### Build Commands

```bash
# Build library
cargo build --release

# Build GUI application
cargo build --release --bin chirp-rs --features gui

# Run tests
cargo test

# Build CLI tools
cargo build --release --bin radio-dump
cargo build --release --bin parse-dump
```

## Usage

### GUI Application

```bash
# Run the Qt GUI
cargo run --features gui

# Or run the compiled binary
./target/release/chirp-rs
```

**Features:**
- File → Open: Load `.img` files
- File → Save/Save As: Save memory changes to `.img` files
- Radio → Download from Radio: Read memories from connected radio
- Radio → Upload to Radio: Write memories to connected radio
- Edit memories by double-clicking rows

### CLI Tools

#### Radio Dump Tool
Download memories from a radio to a binary dump file:

```bash
cargo run --bin radio-dump -- <port> <vendor> <model> <output_file>

# Example:
cargo run --bin radio-dump -- /dev/ttyUSB0 Kenwood TH-D75 radio_dump.bin
```

See `RADIO_DUMP_TOOL.md` for detailed documentation.

#### Parse Memory Tool
Parse and display memories from CHIRP `.img` files or raw binary dumps:

```bash
cargo run --bin parse-memory -- [OPTIONS] <file> [memory_number|range]

# Examples:
cargo run --bin parse-memory -- radio.img                  # Show all non-empty
cargo run --bin parse-memory -- radio.d75 40               # Show memory #40
cargo run --bin parse-memory -- radio.img 32-50            # Show range
cargo run --bin parse-memory -- --raw radio.img 40         # Show with raw data
```

This tool automatically detects whether the file is a CHIRP `.img` file (with metadata)
or a raw binary dump, and displays all available information including:
- Metadata (vendor, model, CHIRP version) when available
- Bank names (for .img files)
- D-STAR fields (URCALL, RPT1/2) for DV mode
- Comprehensive tone information (CTCSS, DTCS)
- Raw memory/bank data (with --raw flag)

## Architecture

### Module Structure
```
src/
├── core/           # Core memory structures and types
├── drivers/        # Radio-specific drivers
│   ├── thd75.rs   # Kenwood TH-D75/D74 driver
│   └── traits.rs  # Radio trait definitions
├── formats/        # File format handlers (.img, .csv)
├── serial/         # Serial port communication
├── memmap/         # Memory map abstraction
├── bitwise/        # Binary data parsing utilities
└── gui/            # Qt-based GUI
    ├── qt_gui.rs  # Main Qt application
    └── radio_ops.rs # Async radio operations
```

### Key Components

#### Memory Structure
The `Memory` struct represents a single radio memory channel with fields for:
- Basic info: number, name, frequency, offset
- Operating mode: FM, NFM, AM, DV (D-STAR), etc.
- Tone settings: CTCSS, DCS, cross mode
- D-STAR fields: URCALL, RPT1CALL, RPT2CALL
- Bank assignment: Organize memories into groups

#### Radio Drivers
Implement the `Radio` and `CloneModeRadio` traits:
- `get_memory()` - Read a single memory
- `set_memory()` - Write a single memory
- `sync_in()` - Download all memories from radio
- `sync_out()` - Upload all memories to radio

#### File Formats
- **IMG Format**: JSON metadata + base64-encoded binary memory map
- **CSV Format**: Tabular export of all memory fields

## Development

### Code Standards
See [CLAUDE.md](CLAUDE.md) for detailed coding guidelines. Key points:
- Follow Rust API Guidelines
- Use `thiserror` for error types
- Write comprehensive tests
- Document all public APIs
- Run `cargo fmt` and `cargo clippy` before committing

### Testing
```bash
# Run all tests
cargo test

# Run TH-D75 specific tests
cargo test thd75

# Run with test output
cargo test -- --nocapture
```

### Adding New Radio Support

1. Create a new driver in `src/drivers/your_radio.rs`
2. Implement `Radio` and `CloneModeRadio` traits
3. Register the driver in `src/drivers/mod.rs`
4. Add memory layout constants and structures
5. Implement `decode_memory()` and `encode_memory()` functions
6. Write comprehensive tests with real radio data

## Documentation

- [PROGRESS.md](PROGRESS.md) - Development progress and milestones
- [CHANGES.md](CHANGES.md) - Changelog
- [GUI_FRAMEWORK.md](GUI_FRAMEWORK.md) - GUI architecture details
- [docs/THD75_PROTOCOL.md](docs/THD75_PROTOCOL.md) - TH-D75 protocol documentation
- [RADIO_DUMP_TOOL.md](RADIO_DUMP_TOOL.md) - Radio dump tool usage

## License

See original CHIRP project for licensing information.

## Contributing

This is a learning project and port of the original CHIRP. Contributions welcome!

1. Ensure all tests pass: `cargo test`
2. Format code: `cargo fmt`
3. Check with clippy: `cargo clippy -- -D warnings`
4. Write tests for new features
5. Update documentation

## Acknowledgments

Based on the original [CHIRP](https://chirp.danplanet.com/) project by Dan Smith and contributors.
