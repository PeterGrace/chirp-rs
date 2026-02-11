//! Unified memory parsing utility
//! Parses CHIRP .img files or raw binary dumps and displays decoded memories
//!
//! This tool auto-detects the file format and displays all available information:
//! - Metadata (vendor, model, CHIRP version) for .img files
//! - Bank names (for .img files)
//! - D-STAR fields (URCALL, RPT1/2) for DV mode
//! - Comprehensive tone information (CTCSS, DTCS)
//! - Raw memory/bank data (with --raw flag)

use chirp_rs::drivers::thd75::THD75Radio;
use chirp_rs::drivers::{CloneModeRadio, Radio};
use chirp_rs::formats::img::load_img;
use chirp_rs::memmap::MemoryMap;
use std::env;

/// Command line arguments
struct Args {
    file: String,
    filter: Option<String>,
    show_raw: bool,
}

fn main() -> anyhow::Result<()> {
    let args = parse_args()?;

    // Load file (handles both .img and raw dumps)
    println!("Loading file: {}", args.file);
    let (mmap, metadata) = load_img(&args.file)?;

    // Display metadata if available (indicates .img format)
    if !metadata.vendor.is_empty() {
        println!("Radio: {} {}", metadata.vendor, metadata.model);
        println!("CHIRP version: {}", metadata.chirp_version);
    } else {
        println!("Raw memory dump (no metadata)");
    }
    println!("Memory map size: {} bytes\n", mmap.len());

    // Create radio driver
    let mut radio = THD75Radio::new();
    radio.process_mmap(&mmap)?;

    // Display bank names for .img files
    if !metadata.vendor.is_empty() {
        print_bank_names(&mmap);
    }

    // Parse and display memories based on filter
    match args.filter.as_deref() {
        None => {
            // Show all non-empty memories
            println!("=== All Non-Empty Memories ===\n");
            let memories = radio.get_memories()?;

            // Filter out empty memories for display
            let non_empty: Vec<_> = memories.iter().filter(|m| !m.empty).collect();
            println!("Found {} non-empty memories\n", non_empty.len());

            for mem in non_empty {
                print_memory(mem, Some(&mmap), args.show_raw);
            }
        }
        Some(range) if range.contains('-') => {
            // Range like "32-50"
            let parts: Vec<&str> = range.split('-').collect();
            let start: u32 = parts[0].parse()?;
            let end: u32 = parts[1].parse()?;

            println!("=== Memories {} to {} ===\n", start, end);
            for num in start..=end {
                match radio.get_memory(num)? {
                    Some(mem) => print_memory(&mem, Some(&mmap), args.show_raw),
                    None => println!("Memory #{}: <empty>\n", num),
                }
            }
        }
        Some(num_str) => {
            // Single memory number
            let num: u32 = num_str.parse()?;
            println!("=== Memory #{} ===\n", num);

            match radio.get_memory(num)? {
                Some(mem) => print_memory(&mem, Some(&mmap), args.show_raw),
                None => println!("Memory #{}: <empty>", num),
            }
        }
    }

    Ok(())
}

/// Parse command line arguments
fn parse_args() -> anyhow::Result<Args> {
    let args: Vec<String> = env::args().collect();
    let mut show_raw = false;
    let mut positional = vec![];

    // Separate flags from positional arguments
    for arg in &args[1..] {
        if arg == "--raw" {
            show_raw = true;
        } else if arg == "--help" || arg == "-h" {
            print_usage(&args[0]);
            std::process::exit(0);
        } else if !arg.starts_with('-') {
            positional.push(arg.clone());
        } else {
            eprintln!("Unknown flag: {}", arg);
            print_usage(&args[0]);
            std::process::exit(1);
        }
    }

    if positional.is_empty() {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    Ok(Args {
        file: positional[0].clone(),
        filter: positional.get(1).cloned(),
        show_raw,
    })
}

/// Print usage information
fn print_usage(program: &str) {
    eprintln!("Usage: {} [OPTIONS] <file> [memory_number|range]", program);
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --raw          Show raw memory/bank data (debug mode)");
    eprintln!("  -h, --help     Show this help message");
    eprintln!();
    eprintln!("Examples:");
    eprintln!(
        "  {} radio.img                    # Show all non-empty memories",
        program
    );
    eprintln!(
        "  {} radio.d75                    # Works with raw dumps too",
        program
    );
    eprintln!(
        "  {} radio.img 40                 # Show memory #40",
        program
    );
    eprintln!(
        "  {} radio.img 32-50              # Show range 32-50",
        program
    );
    eprintln!(
        "  {} --raw radio.img 40           # Show memory #40 with raw data",
        program
    );
}

/// Print all bank names from memory map
fn print_bank_names(mmap: &MemoryMap) {
    println!("=== Bank Names ===");
    const GROUP_NAME_OFFSET: usize = 1152;
    const NAME_SECTION_START: usize = 0x10000;

    for i in 0..30 {
        let name_offset = NAME_SECTION_START + ((GROUP_NAME_OFFSET + i) * 16);
        if let Ok(name_bytes) = mmap.get(name_offset, Some(16)) {
            let name = String::from_utf8_lossy(name_bytes)
                .trim_end_matches('\0')
                .trim()
                .to_string();
            if name.is_empty() {
                println!("Bank {}: (empty)", i);
            } else {
                println!("Bank {}: \"{}\"", i, name);
            }
        }
    }
    println!();
}

/// Get bank name from memory map
fn get_bank_name(mmap: &MemoryMap, bank_index: usize) -> Option<String> {
    const GROUP_NAME_OFFSET: usize = 1152;
    const NAME_SECTION_START: usize = 0x10000;

    let name_offset = NAME_SECTION_START + ((GROUP_NAME_OFFSET + bank_index) * 16);
    if let Ok(name_bytes) = mmap.get(name_offset, Some(16)) {
        let name = String::from_utf8_lossy(name_bytes)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

/// Print memory information with optional bank name and raw data
fn print_memory(mem: &chirp_rs::core::Memory, mmap: Option<&MemoryMap>, show_raw: bool) {
    println!("Memory #{}: \"{}\"", mem.number, mem.name);
    println!(
        "  Frequency:    {} Hz ({:.6} MHz)",
        mem.freq,
        mem.freq as f64 / 1_000_000.0
    );
    println!(
        "  Offset:       {} Hz ({:.2} MHz)",
        mem.offset,
        mem.offset as f64 / 1_000_000.0
    );
    println!("  Mode:         {}", mem.mode);
    println!(
        "  Duplex:       {}",
        if mem.duplex.is_empty() {
            "none"
        } else {
            &mem.duplex
        }
    );

    // Show D-STAR fields for DV mode
    if mem.mode == "DV" {
        if !mem.dv_urcall.is_empty() {
            println!("  URCALL:       {}", mem.dv_urcall);
        }
        if !mem.dv_rpt1call.is_empty() {
            println!("  RPT1CALL:     {}", mem.dv_rpt1call);
        }
        if !mem.dv_rpt2call.is_empty() {
            println!("  RPT2CALL:     {}", mem.dv_rpt2call);
        }
        if mem.dv_code != 0 {
            println!("  DV Code:      {}", mem.dv_code);
        }
    } else {
        // Show tone information for non-DV modes
        println!(
            "  Tone Mode:    {}",
            if mem.tmode.is_empty() {
                "none"
            } else {
                &mem.tmode
            }
        );

        match mem.tmode.as_str() {
            "Tone" => {
                // Encode only - just show TX tone
                println!("  CTCSS TX:     {} Hz", mem.rtone);
            }
            "TSQL" => {
                // Tone squelch - show both TX and RX
                println!("  CTCSS TX:     {} Hz", mem.rtone);
                println!("  CTCSS RX:     {} Hz", mem.ctone);
            }
            "DTCS" => {
                // Digital codes
                println!("  DTCS:         {}", mem.dtcs);
            }
            _ => {
                // No tone mode or other modes
            }
        }
    }

    println!(
        "  Skip:         {}",
        if mem.skip.is_empty() {
            "none"
        } else {
            &mem.skip
        }
    );
    println!("  Tuning Step:  {} kHz", mem.tuning_step);

    // Show bank info with name if available
    if let Some(map) = mmap {
        if let Some(bank_name) = get_bank_name(map, mem.bank as usize) {
            println!("  Bank:         {} (\"{}\")", mem.bank, bank_name);
        } else {
            println!("  Bank:         {}", mem.bank);
        }
    } else {
        println!("  Bank:         {}", mem.bank);
    }

    println!();

    // Show raw data if requested
    if show_raw {
        if let Some(map) = mmap {
            print_raw_bank_data(map, mem.number);
            print_raw_memory_data(map, mem.number);
        }
    }
}

/// Print raw bank/flags data
fn print_raw_bank_data(mmap: &MemoryMap, number: u32) {
    // Bank/flags are at 0x2000 + (channel * 4)
    let flags_offset = 0x2000 + (number as usize * 4);

    println!("  Raw Bank/Flags Data:");
    println!("  Flags offset: 0x{:04X}", flags_offset);

    if let Ok(bytes) = mmap.get(flags_offset, Some(4)) {
        print!("  4 bytes:      ");
        for byte in bytes {
            print!("{:02X} ", byte);
        }
        println!();

        println!(
            "  Byte 0 (used):    0x{:02X} (0x00=used, 0xFF=empty)",
            bytes[0]
        );
        println!(
            "  Byte 1 (lockout): 0x{:02X} (bit 7 = lockout flag)",
            bytes[1]
        );
        println!("  Byte 2 (group):   {} (bank number)", bytes[2]);
        println!("  Byte 3:           0x{:02X}", bytes[3]);
    }

    println!();
}

/// Print raw memory data
fn print_raw_memory_data(mmap: &MemoryMap, number: u32) {
    // Calculate offset using correct formula: groups of 6 memories with 40-byte size
    let group = (number / 6) as usize;
    let index = (number % 6) as usize;
    let offset = 0x4000 + (group * (6 * 40 + 16)) + (index * 40);

    println!("  Raw Memory Data:");
    println!("  Memory offset: 0x{:04X}", offset);

    // Read first 40 bytes (full memory structure)
    if let Ok(bytes) = mmap.get(offset, Some(40)) {
        print!("  First 40 bytes:   ");
        for (i, byte) in bytes.iter().enumerate() {
            print!("{:02X} ", byte);
            if i == 7 || i == 15 || i == 23 || i == 31 {
                print!(" ");
            }
        }
        println!();

        // Show as ASCII for name fields
        print!("  ASCII:            ");
        for byte in bytes {
            let c = if byte.is_ascii_graphic() || *byte == b' ' {
                *byte as char
            } else {
                '.'
            };
            print!("{}", c);
        }
        println!("\n");

        // Decode key fields
        if bytes.len() >= 8 {
            let freq = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            let offset_val = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

            if freq != 0 && freq != 0xFFFFFFFF {
                println!(
                    "  Decoded freq:     {} Hz ({:.6} MHz)",
                    freq,
                    freq as f64 / 1_000_000.0
                );
            }
            if offset_val != 0 && offset_val != 0xFFFFFFFF {
                println!(
                    "  Decoded offset:   {} Hz ({:.2} MHz)",
                    offset_val,
                    offset_val as f64 / 1_000_000.0
                );
            }
        }

        println!();
    }
}
