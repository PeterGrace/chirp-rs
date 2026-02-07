//! Radio memory dump utility
//! Downloads raw memory data from a radio and saves it to files for analysis

use chirp_rs::drivers::{get_driver, CloneModeRadio, RadioResult};
use chirp_rs::serial::SerialPort;
use std::env;
use std::fs::File;
use std::io::Write;
use tracing_subscriber::{fmt::format::FmtSpan, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    let format_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_span_events(FmtSpan::NONE);

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(format_layer)
        .init();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: {} <port> <vendor> <model>", args[0]);
        eprintln!("Example: {} /dev/ttyACM0 Kenwood TH-D75", args[0]);
        eprintln!("\nThis will download raw memory and save to:");
        eprintln!("  - radio_dump.bin (raw binary)");
        eprintln!("  - radio_dump.hex (hex dump)");
        std::process::exit(1);
    }

    let port_name = &args[1];
    let vendor = &args[2];
    let model = &args[3];

    tracing::info!("Radio Memory Dump Utility");
    tracing::info!("Port: {}", port_name);
    tracing::info!("Radio: {} {}", vendor, model);

    // Initialize drivers
    chirp_rs::drivers::init_drivers();

    // Get driver
    let driver_info = get_driver(vendor, model)
        .ok_or_else(|| anyhow::anyhow!("Driver not found for {} {}", vendor, model))?;

    if !driver_info.is_clone_mode {
        anyhow::bail!("Only clone mode radios are supported by this tool");
    }

    tracing::info!("Found driver: {} {}", driver_info.vendor, driver_info.model);

    // Open serial port
    tracing::info!("Opening serial port...");
    let serial_config = chirp_rs::serial::SerialConfig {
        baud_rate: 9600,
        data_bits: serialport::DataBits::Eight,
        stop_bits: serialport::StopBits::One,
        parity: serialport::Parity::None,
        flow_control: serialport::FlowControl::Hardware,
        timeout: std::time::Duration::from_secs(10),
    };

    let mut port = SerialPort::open(port_name, serial_config)?;

    // Set DTR/RTS for Kenwood radios
    if vendor == "Kenwood" {
        port.set_dtr(true)?;
        port.set_rts(false)?;
        tracing::info!("Set DTR=true, RTS=false for Kenwood radio");
    }

    port.clear_all()?;

    // Download memory
    tracing::info!("Downloading memory from radio...");
    tracing::info!("This may take several minutes. Please wait...");

    let raw_data = download_raw_memory(&mut port, vendor, model).await?;

    tracing::info!("Downloaded {} bytes", raw_data.len());

    // Save binary file
    let bin_path = "radio_dump.bin";
    let mut bin_file = File::create(bin_path)?;
    bin_file.write_all(&raw_data)?;
    tracing::info!("Saved raw binary to: {}", bin_path);

    // Save hex dump
    let hex_path = "radio_dump.hex";
    let mut hex_file = File::create(hex_path)?;
    write_hex_dump(&mut hex_file, &raw_data)?;
    tracing::info!("Saved hex dump to: {}", hex_path);

    // Print summary
    println!("\n=== Download Complete ===");
    println!("Raw binary: {} ({} bytes)", bin_path, raw_data.len());
    println!("Hex dump:   {}", hex_path);
    println!("\nYou can now:");
    println!("  - View hex: hexdump -C {}", bin_path);
    println!("  - View text: cat {}", hex_path);
    println!("  - Search: grep '30 8E 58 1A' {}", hex_path);

    Ok(())
}

async fn download_raw_memory(
    port: &mut SerialPort,
    vendor: &str,
    model: &str,
) -> RadioResult<Vec<u8>> {
    use chirp_rs::drivers::thd75::THD75Radio;

    // Only TH-D75 is supported for now
    if vendor != "Kenwood" || model != "TH-D75" {
        return Err(chirp_rs::drivers::RadioError::Radio(format!(
            "Unsupported radio: {} {}",
            vendor, model
        )));
    }

    // Create driver instance
    let mut radio = THD75Radio::new();

    // Progress callback
    let progress = |current: usize, total: usize, message: &str| {
        if current % 100 == 0 || current == total {
            tracing::info!("[{}/{}] {}", current, total, message);
        }
    };

    // Download memory
    let mmap = radio.sync_in(port, Some(Box::new(progress))).await?;

    // Extract raw data
    let memsize = radio.get_memsize();
    let raw_data = match mmap.get(0, Some(memsize)) {
        Ok(data) => data.to_vec(),
        Err(e) => return Err(chirp_rs::drivers::RadioError::Radio(e.to_string())),
    };

    Ok(raw_data)
}

fn write_hex_dump(file: &mut File, data: &[u8]) -> std::io::Result<()> {
    writeln!(
        file,
        "Offset(h) 00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F  ASCII"
    )?;
    writeln!(
        file,
        "========================================================================="
    )?;

    for (offset, chunk) in data.chunks(16).enumerate() {
        // Offset
        write!(file, "{:08X}  ", offset * 16)?;

        // Hex bytes
        for (i, byte) in chunk.iter().enumerate() {
            write!(file, "{:02X} ", byte)?;
            if i == 7 {
                write!(file, " ")?;
            }
        }

        // Padding for incomplete lines
        if chunk.len() < 16 {
            for i in chunk.len()..16 {
                write!(file, "   ")?;
                if i == 7 {
                    write!(file, " ")?;
                }
            }
        }

        // ASCII representation
        write!(file, " ")?;
        for byte in chunk {
            let c = if byte.is_ascii_graphic() || *byte == b' ' {
                *byte as char
            } else {
                '.'
            };
            write!(file, "{}", c)?;
        }

        writeln!(file)?;
    }

    Ok(())
}
