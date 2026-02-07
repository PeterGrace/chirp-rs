# Radio Dump Tool

A utility to download and save raw memory data from your radio for debugging and analysis.

## Build

```bash
cargo build --bin radio-dump --release
```

## Usage

```bash
./target/release/radio-dump <port> <vendor> <model>
```

### Example

```bash
./target/release/radio-dump /dev/ttyACM0 Kenwood TH-D75
```

## Output Files

The tool creates two files in the current directory:

- **radio_dump.bin** - Raw binary data (500+ KB)
- **radio_dump.hex** - Human-readable hex dump with ASCII

## Analysis Commands

### View hex dump:
```bash
hexdump -C radio_dump.bin | less
```

### View the formatted hex file:
```bash
cat radio_dump.hex | less
```

### Search for a specific frequency:
For example, to find 441.950 MHz (0x1A588E30):
```bash
# In little-endian: 30 8E 58 1A
grep "30 8E 58 1A" radio_dump.hex
```

### Search for text (like memory names):
```bash
grep -i "W3EOC" radio_dump.hex
```

### Find the offset of data:
```bash
# Look at the left column (Offset) in radio_dump.hex
# Or use hexdump to show offsets:
hexdump -C radio_dump.bin | grep "30 8E 58 1A"
```

## Debugging Memory Layout

Once you have the dump files:

1. **Find the frequency** - Search for the hex bytes of the frequency you're looking for
2. **Note the offset** - Look at the left column to see where it's located in memory
3. **Compare with expected** - Compare to what our formula calculates (see debug logs)
4. **Identify pattern** - Look for padding, gaps, or different structures

## Creating Tests

You can use the dumped data to create Rust unit tests:

```rust
#[test]
fn test_memory_40_offset() {
    // Memory 40 "W3EOC" is 441.950 MHz
    // Found at offset 0x???? in dump
    // Our formula calculates: 0x????
    // ...
}
```

## Tips

- **Enable debug logging**: Set `RUST_LOG=debug` to see detailed offset calculations
- **Multiple dumps**: Compare dumps from Python CHIRP vs our tool to verify
- **Hex editor**: Use tools like `hexedit`, `ghex`, or `xxd` for interactive exploration
