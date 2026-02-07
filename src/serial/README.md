# Serial Communication Module

This module provides async serial communication for radio I/O with built-in support for block-based protocols, progress reporting, and testing.

## Features

- **Async I/O**: Built on tokio for non-blocking serial communication
- **Block Protocols**: Helpers for radios that transfer memory in fixed-size blocks
- **Progress Callbacks**: Real-time progress reporting for GUI integration
- **Timeout Handling**: Configurable timeouts with automatic retry logic
- **Mock Serial Port**: Full testing support without hardware
- **DTR/RTS Control**: Hardware flow control for radios that need it

## Quick Start

### Opening a Port

```rust
use chirp_rs::serial::{SerialPort, SerialConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Simple configuration
    let config = SerialConfig::new(9600);
    let mut port = SerialPort::open("/dev/ttyUSB0", config)?;

    // Or with custom settings
    let config = SerialConfig::new(19200)
        .with_timeout(Duration::from_secs(5))
        .with_hardware_flow();

    let mut port = SerialPort::open("COM3", config)?;

    Ok(())
}
```

### Basic Read/Write

```rust
// Write command
port.write_all(b"READ_MEMORY").await?;

// Read response
let mut buffer = [0u8; 64];
port.read_exact(&mut buffer).await?;

// Read with partial reads allowed
let n = port.read(&mut buffer).await?;
println!("Read {} bytes", n);

// Flush output
port.flush().await?;
```

### List Available Ports

```rust
use chirp_rs::serial::list_ports;

let ports = list_ports()?;
for port in ports {
    println!("Available port: {}", port);
}
```

## Block-Based Protocol

Many radios transfer memory in fixed-size blocks. The `BlockProtocol` helper makes this easy:

### CloneModeRadio Download

```rust
use chirp_rs::serial::{BlockProtocol, SerialPort, SerialConfig};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = SerialPort::open("/dev/ttyUSB0", SerialConfig::new(9600))?;

    // Radio has 16384 bytes of memory in 64-byte blocks
    let protocol = BlockProtocol::new(64, 16384);

    // Define how to request each block
    let request_block = |block_num: usize| {
        // Example: Send block number as command
        vec![0x52, (block_num >> 8) as u8, (block_num & 0xFF) as u8]
    };

    // Download with progress callback
    let progress = Arc::new(|bytes, total, msg: &str| {
        println!("{} - {}/{} bytes", msg, bytes, total);
    });

    let memory_data = protocol.download(
        &mut port,
        request_block,
        Some(progress)
    ).await?;

    println!("Downloaded {} bytes", memory_data.len());
    Ok(())
}
```

### Simple Sequential Download

For radios that stream all data after a single command:

```rust
let protocol = BlockProtocol::new(64, 16384);

// Send initialization command
let init_cmd = b"DOWNLOAD_START";

// Progress callback
let progress = Arc::new(|bytes, total, msg: &str| {
    let percent = (bytes as f32 / total as f32) * 100.0;
    println!("{:.1}% - {}", percent, msg);
});

// Download all data
let memory_data = protocol.download_simple(
    &mut port,
    init_cmd,
    Some(progress)
).await?;
```

### Upload to Radio

```rust
let memory_data = vec![0u8; 16384]; // Your memory image

let protocol = BlockProtocol::new(64, 16384);

// Define how to format each block for upload
let send_block = |block_num: usize, data: &[u8]| {
    let mut packet = vec![0x57]; // Write command
    packet.push((block_num >> 8) as u8);
    packet.push((block_num & 0xFF) as u8);
    packet.extend_from_slice(data);
    packet
};

// Upload with progress
let progress = Arc::new(|bytes, total, msg: &str| {
    println!("{}", msg);
});

protocol.upload(
    &mut port,
    &memory_data,
    send_block,
    Some(progress)
).await?;
```

## Protocol Helpers

### Read Until Delimiter

```rust
use chirp_rs::serial::protocol::read_until;

// Read until newline
let response = read_until(&mut port, b'\n', 256).await?;
println!("Response: {}", String::from_utf8_lossy(&response));
```

### Expect Specific Response

```rust
use chirp_rs::serial::protocol::expect_response;

// Send command
port.write_all(b"IDENTIFY").await?;

// Expect specific ACK
expect_response(&mut port, b"OK\r\n").await?;
```

## Hardware Control

```rust
// Set DTR (Data Terminal Ready) - some radios use this to enter programming mode
port.set_dtr(true)?;

// Set RTS (Request To Send)
port.set_rts(true)?;

// Clear buffers
port.clear_all()?;
port.clear_input()?;
port.clear_output()?;

// Check bytes available
let available = port.bytes_to_read()?;
println!("{} bytes waiting", available);
```

## Mock Serial Port for Testing

The mock serial port lets you test radio drivers without hardware:

```rust
use chirp_rs::serial::mock::MockSerialPort;

#[tokio::test]
async fn test_radio_download() {
    let mut mock_port = MockSerialPort::new();

    // Simulate radio response
    let memory_data = vec![0xAA; 256];
    mock_port.push_read_data(&memory_data);

    // Test your download code
    let mut buffer = vec![0u8; 256];
    mock_port.read_exact(&mut buffer).await.unwrap();

    assert_eq!(buffer, memory_data);

    // Verify correct commands were sent
    assert!(mock_port.was_written(b"DOWNLOAD"));
}
```

### Mock with Delays

Simulate realistic timing:

```rust
let mut mock_port = MockSerialPort::new()
    .with_delay(10); // 10ms delay per operation

mock_port.push_read_data(b"SLOW_RESPONSE");
```

### Mock CloneModeRadio

```rust
use chirp_rs::serial::mock::mock_clone_mode_radio;

// Simulate a radio with 16K memory in 64-byte blocks
let memory = vec![0x42; 16384];
let mut mock_radio = mock_clone_mode_radio(memory.clone(), 64);

// Your download code will work with the mock
let protocol = BlockProtocol::new(64, 16384);
let downloaded = protocol.download_simple(
    &mut mock_radio,
    b"START",
    None
).await.unwrap();

assert_eq!(downloaded, memory);
```

## Real-World Examples

### Kenwood TH-D75 (CloneModeRadio)

```rust
async fn download_thd75(port_name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let config = SerialConfig::new(9600)
        .with_timeout(Duration::from_secs(10));

    let mut port = SerialPort::open(port_name, config)?;

    // TH-D75 uses 57344 bytes in 32-byte blocks
    let protocol = BlockProtocol::new(32, 57344);

    // Send download command
    port.write_all(&[0x52, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]).await?;

    // Wait for ACK
    let mut ack = [0u8; 1];
    port.read_exact(&mut ack).await?;
    if ack[0] != 0x06 {
        return Err("Radio did not ACK".into());
    }

    // Download all blocks
    let progress = Arc::new(|bytes, total, msg: &str| {
        eprintln!("{}", msg);
    });

    let data = protocol.download_simple(
        &mut port,
        &[],  // No additional init needed
        Some(progress)
    ).await?;

    Ok(data)
}
```

### ICOM IC-9700 (CI-V Protocol)

```rust
// IC-9700 uses command-based CI-V protocol, not bulk clone
async fn read_ic9700_memory(
    port: &mut SerialPort,
    channel: u16
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // CI-V command format: FE FE <radio> <controller> <cmd> <data> FD
    let mut cmd = vec![
        0xFE, 0xFE,           // Preamble
        0xA2,                 // IC-9700 address
        0xE0,                 // Controller address
        0x1A, 0x00,           // Read memory command
        (channel & 0xFF) as u8,
        (channel >> 8) as u8,
        0xFD                  // End marker
    ];

    port.write_all(&cmd).await?;

    // Read response (FE FE E0 A2 1A 00 ... FD)
    let response = read_until(port, 0xFD, 256).await?;

    // Parse memory data from response
    Ok(response[8..response.len()-1].to_vec())
}
```

## Error Handling

```rust
use chirp_rs::serial::SerialError;

match port.read_exact(&mut buffer).await {
    Ok(()) => println!("Read successful"),
    Err(SerialError::Timeout(duration)) => {
        eprintln!("Timeout after {:?}", duration);
    }
    Err(SerialError::Port(msg)) => {
        eprintln!("Port error: {}", msg);
    }
    Err(SerialError::Io(e)) => {
        eprintln!("IO error: {}", e);
    }
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
}
```

## Integration with GUI

For iced or other GUI frameworks, use progress callbacks:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// Shared progress state
let progress_bytes = Arc::new(AtomicUsize::new(0));
let progress_clone = progress_bytes.clone();

// Create callback that updates GUI
let callback = Arc::new(move |bytes, _total, _msg: &str| {
    progress_clone.store(bytes, Ordering::Relaxed);
});

// Start download in background task
tokio::spawn(async move {
    let result = protocol.download(
        &mut port,
        request_block,
        Some(callback)
    ).await;

    // Send result to GUI via channel
    tx.send(result).unwrap();
});

// In GUI update loop, read progress
let bytes_downloaded = progress_bytes.load(Ordering::Relaxed);
update_progress_bar(bytes_downloaded);
```

## Best Practices

1. **Always use timeouts**: Radios can hang or disconnect
2. **Clear buffers before operations**: `port.clear_all()?`
3. **Handle partial reads**: Use `read_exact()` for fixed-size data
4. **Test with mock ports**: Write tests before touching hardware
5. **Report progress**: Long operations should update users
6. **Set DTR/RTS appropriately**: Some radios require specific control signals
7. **Retry on transient errors**: Network-like behavior for robustness

## Testing

```bash
# Run all serial tests
cargo test serial

# Run only mock tests
cargo test serial::mock

# Run with output
cargo test serial -- --nocapture
```

## Platform Support

- **Linux**: `/dev/ttyUSB0`, `/dev/ttyACM0`
- **macOS**: `/dev/tty.usbserial-*`
- **Windows**: `COM1`, `COM3`, etc.

Use `list_ports()` to auto-detect available ports.

## See Also

- [drivers module](../drivers/README.md) - Radio driver implementations
- [memmap module](../memmap/README.md) - Memory storage
- [tokio docs](https://docs.rs/tokio/) - Async runtime
- [serialport docs](https://docs.rs/serialport/) - Underlying serial library
