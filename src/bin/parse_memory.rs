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
use chirp_rs::drivers::uv5r::UV5RRadio;
use chirp_rs::drivers::{CloneModeRadio, Radio};
use chirp_rs::formats::img::load_img;
use chirp_rs::memmap::MemoryMap;
use std::env;

/// Command line arguments
struct Args {
    file: String,
    filter: Option<String>,
    show_raw: bool,
    radio_type: Option<String>,
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

    // Determine which driver to use based on --radio arg, metadata, or default
    let (vendor, model) = if let Some(ref radio) = args.radio_type {
        // Use explicit radio type from command line
        match radio.to_lowercase().as_str() {
            "uv5r" | "uv-5r" => ("Baofeng".to_string(), "UV-5R".to_string()),
            "thd75" | "th-d75" => ("Kenwood".to_string(), "TH-D75".to_string()),
            "thd74" | "th-d74" => ("Kenwood".to_string(), "TH-D74".to_string()),
            _ => {
                eprintln!("Unknown radio type: {}", radio);
                eprintln!("Supported types: uv5r, thd75, thd74");
                std::process::exit(1);
            }
        }
    } else if !metadata.vendor.is_empty() {
        // Use metadata from .img file
        (metadata.vendor.clone(), metadata.model.clone())
    } else {
        // Auto-detect based on file size or default to TH-D75
        if mmap.len() <= 0x2000 {
            // Small file, likely UV-5R (6152 bytes = 0x1808)
            println!("Note: Auto-detected UV-5R based on file size. Use --radio to override.");
            ("Baofeng".to_string(), "UV-5R".to_string())
        } else {
            // Large file, likely TH-D75
            ("Kenwood".to_string(), "TH-D75".to_string())
        }
    };

    // Create appropriate radio driver and get memories
    let (memories, has_banks) = match (vendor.to_lowercase().as_str(), model.as_str()) {
        ("baofeng", "UV-5R") => {
            let mut radio = UV5RRadio::new();
            radio.process_mmap(&mmap)?;
            let mems = get_memories_filtered(&mut radio, &args)?;
            (mems, false) // UV-5R has no banks
        }
        ("kenwood", "TH-D75") | ("kenwood", "TH-D74") | _ => {
            let mut radio = THD75Radio::new();
            radio.process_mmap(&mmap)?;
            let mems = get_memories_filtered(&mut radio, &args)?;
            (mems, true) // TH-D75 has banks
        }
    };

    // Display bank names for radios that have them
    if !metadata.vendor.is_empty() && has_banks {
        print_bank_names(&mmap);
    }

    // Display memories
    match args.filter.as_deref() {
        None => {
            // Show all non-empty memories
            println!("=== All Non-Empty Memories ===\n");
            let non_empty: Vec<_> = memories.iter().filter(|m| !m.empty).collect();
            println!("Found {} non-empty memories\n", non_empty.len());

            for mem in non_empty {
                print_memory(mem, Some(&mmap), args.show_raw, &vendor, &model);
            }
        }
        Some(_) => {
            // Single memory or range
            for mem in &memories {
                if mem.empty {
                    println!("Memory #{}: <empty>\n", mem.number);
                } else {
                    print_memory(mem, Some(&mmap), args.show_raw, &vendor, &model);
                }
            }
        }
    }

    Ok(())
}

/// Get memories based on filter (reusable for any driver)
fn get_memories_filtered(
    radio: &mut dyn Radio,
    args: &Args,
) -> anyhow::Result<Vec<chirp_rs::core::Memory>> {
    match args.filter.as_deref() {
        None => {
            // Get all memories
            Ok(radio.get_memories()?)
        }
        Some(range) if range.contains('-') => {
            // Range like "32-50"
            let parts: Vec<&str> = range.split('-').collect();
            let start: u32 = parts[0].parse()?;
            let end: u32 = parts[1].parse()?;

            let mut memories = Vec::new();
            for num in start..=end {
                // Create an empty memory placeholder if memory doesn't exist
                match radio.get_memory(num)? {
                    Some(mem) => memories.push(mem),
                    None => {
                        let mut empty_mem = chirp_rs::core::Memory::new(num);
                        empty_mem.empty = true;
                        memories.push(empty_mem);
                    }
                }
            }
            Ok(memories)
        }
        Some(num_str) => {
            // Single memory number
            let num: u32 = num_str.parse()?;
            match radio.get_memory(num)? {
                Some(mem) => Ok(vec![mem]),
                None => {
                    let mut empty_mem = chirp_rs::core::Memory::new(num);
                    empty_mem.empty = true;
                    Ok(vec![empty_mem])
                }
            }
        }
    }
}

/// Parse command line arguments
fn parse_args() -> anyhow::Result<Args> {
    let args: Vec<String> = env::args().collect();
    let mut show_raw = false;
    let mut radio_type = None;
    let mut positional = vec![];
    let mut i = 1;

    // Parse flags and options
    while i < args.len() {
        match args[i].as_str() {
            "--raw" => {
                show_raw = true;
                i += 1;
            }
            "--radio" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --radio requires a value");
                    print_usage(&args[0]);
                    std::process::exit(1);
                }
                radio_type = Some(args[i + 1].clone());
                i += 2;
            }
            "--help" | "-h" => {
                print_usage(&args[0]);
                std::process::exit(0);
            }
            arg if arg.starts_with('-') => {
                eprintln!("Unknown flag: {}", arg);
                print_usage(&args[0]);
                std::process::exit(1);
            }
            _ => {
                positional.push(args[i].clone());
                i += 1;
            }
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
        radio_type,
    })
}

/// Print usage information
fn print_usage(program: &str) {
    eprintln!("Usage: {} [OPTIONS] <file> [memory_number|range]", program);
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --raw               Show raw memory/bank data (debug mode)");
    eprintln!("  --radio <type>      Force radio type (uv5r, thd75) for raw files");
    eprintln!("  -h, --help          Show this help message");
    eprintln!();
    eprintln!("Examples:");
    eprintln!(
        "  {} radio.img                    # Show all non-empty memories",
        program
    );
    eprintln!(
        "  {} radio.bin --radio uv5r       # Parse raw UV-5R dump",
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
fn print_memory(
    mem: &chirp_rs::core::Memory,
    mmap: Option<&MemoryMap>,
    show_raw: bool,
    vendor: &str,
    model: &str,
) {
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

    // Show bank info with name if available (only for radios with banks)
    let has_banks = matches!(
        (vendor.to_lowercase().as_str(), model),
        ("kenwood", "TH-D75") | ("kenwood", "TH-D74")
    );

    if has_banks {
        if let Some(map) = mmap {
            if let Some(bank_name) = get_bank_name(map, mem.bank as usize) {
                println!("  Bank:         {} (\"{}\")", mem.bank, bank_name);
            } else {
                println!("  Bank:         {}", mem.bank);
            }
        } else {
            println!("  Bank:         {}", mem.bank);
        }
    }

    println!();

    // Show raw data if requested
    if show_raw {
        if let Some(map) = mmap {
            // Only show bank data for radios with banks
            if has_banks {
                print_raw_bank_data(map, mem.number);
            }
            print_raw_memory_data(map, mem.number, vendor, model);
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
fn print_raw_memory_data(mmap: &MemoryMap, number: u32, vendor: &str, model: &str) {
    println!("  Raw Memory Data:");

    // Calculate offset and size based on radio type
    let (offset, mem_size) = match (vendor.to_lowercase().as_str(), model) {
        ("baofeng", "UV-5R") => {
            // UV-5R: 16 bytes per memory, sequential at MEMORY_BASE (0x0008)
            let offset = 0x0008 + (number as usize * 16);
            (offset, 16)
        }
        _ => {
            // TH-D75: 40 bytes per memory, groups of 6 with padding
            let group = (number / 6) as usize;
            let index = (number % 6) as usize;
            let offset = 0x4000 + (group * (6 * 40 + 16)) + (index * 40);
            (offset, 40)
        }
    };

    println!("  Memory offset: 0x{:04X}", offset);

    // Read memory bytes
    if let Ok(bytes) = mmap.get(offset, Some(mem_size)) {
        print!("  First {} bytes:   ", mem_size);
        for (i, byte) in bytes.iter().enumerate() {
            print!("{:02X} ", byte);
            if i == 7 || i == 15 || i == 23 || i == 31 {
                print!(" ");
            }
        }
        println!();

        // Show as ASCII
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

        // Decode key fields (BCD frequencies)
        if bytes.len() >= 8 {
            let freq_bcd = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            let tx_bcd = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

            // For UV-5R, frequencies are BCD encoded (multiply by 10)
            // For TH-D75, frequencies are already in Hz
            let is_uv5r = vendor.to_lowercase() == "baofeng" && model == "UV-5R";

            if freq_bcd != 0 && freq_bcd != 0xFFFFFFFF {
                if is_uv5r {
                    // Try to decode as BCD
                    use chirp_rs::bitwise::bcd_to_int;
                    match bcd_to_int(&freq_bcd.to_le_bytes(), true) {
                        Ok(value) => {
                            let freq = value * 10;
                            println!(
                                "  RX freq (BCD):    {:08X} = {} Hz ({:.6} MHz)",
                                freq_bcd,
                                freq,
                                freq as f64 / 1_000_000.0
                            );
                        }
                        Err(e) => {
                            println!("  RX freq (BCD):    {:08X} (invalid BCD: {})", freq_bcd, e);
                        }
                    }
                } else {
                    println!(
                        "  RX freq:          {} Hz ({:.6} MHz)",
                        freq_bcd,
                        freq_bcd as f64 / 1_000_000.0
                    );
                }
            }

            if tx_bcd != 0 && tx_bcd != 0xFFFFFFFF {
                if is_uv5r {
                    use chirp_rs::bitwise::bcd_to_int;
                    match bcd_to_int(&tx_bcd.to_le_bytes(), true) {
                        Ok(value) => {
                            let freq = value * 10;
                            println!(
                                "  TX freq (BCD):    {:08X} = {} Hz ({:.6} MHz)",
                                tx_bcd,
                                freq,
                                freq as f64 / 1_000_000.0
                            );
                        }
                        Err(e) => {
                            println!("  TX freq (BCD):    {:08X} (invalid BCD: {})", tx_bcd, e);
                        }
                    }
                } else {
                    println!(
                        "  TX freq:          {} Hz ({:.6} MHz)",
                        tx_bcd,
                        tx_bcd as f64 / 1_000_000.0
                    );
                }
            }
        }

        // Show name location for UV-5R
        if vendor.to_lowercase() == "baofeng" && model == "UV-5R" {
            let name_offset = 0x1008 + (number as usize * 16);
            if let Ok(name_bytes) = mmap.get(name_offset, Some(7)) {
                let name = String::from_utf8_lossy(name_bytes)
                    .replace('\u{ffff}', " ")
                    .trim_end()
                    .to_string();
                println!("\n  Name offset:      0x{:04X}", name_offset);
                println!("  Name bytes:       {:02X?}", name_bytes);
                println!("  Name string:      \"{}\"", name);
            }
        }

        println!();
    }
}
