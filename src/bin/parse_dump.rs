//! Parse memory dump utility
//! Parses a binary memory dump file and displays decoded memories

use chirp_rs::drivers::thd75::THD75Radio;
use chirp_rs::drivers::{CloneModeRadio, Radio};
use chirp_rs::memmap::MemoryMap;
use std::env;
use std::fs;

fn main() -> anyhow::Result<()> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <dump_file.bin> [memory_number]", args[0]);
        eprintln!("\nExamples:");
        eprintln!(
            "  {} radio_dump.bin           # Show all non-empty memories",
            args[0]
        );
        eprintln!(
            "  {} radio_dump.bin 40        # Show only memory #40",
            args[0]
        );
        eprintln!(
            "  {} radio_dump.bin 32-50     # Show memories 32-50",
            args[0]
        );
        std::process::exit(1);
    }

    let dump_file = &args[1];
    let filter = args.get(2).map(|s| s.as_str());

    // Read dump file
    println!("Reading dump file: {}", dump_file);
    let data = fs::read(dump_file)?;
    println!("Loaded {} bytes\n", data.len());

    // Create memory map
    let mmap = MemoryMap::new(data);

    // Create radio driver
    let mut radio = THD75Radio::new();

    // Load memory map into radio
    radio.process_mmap(&mmap)?;

    // Parse based on filter
    match filter {
        None => {
            // Show all non-empty memories
            println!("=== All Non-Empty Memories ===\n");
            let memories = radio.get_memories()?;
            println!("Found {} non-empty memories\n", memories.len());

            for mem in &memories {
                print_memory(mem);
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
                    Some(mem) => print_memory(&mem),
                    None => println!("Memory #{}: <empty>\n", num),
                }
            }
        }
        Some(num_str) => {
            // Single memory number
            let num: u32 = num_str.parse()?;
            println!("=== Memory #{} ===\n", num);

            match radio.get_memory(num)? {
                Some(mem) => {
                    print_memory(&mem);
                    print_raw_data(&radio, &mmap, num)?;
                }
                None => println!("Memory #{}: <empty>", num),
            }
        }
    }

    Ok(())
}

fn print_memory(mem: &chirp_rs::core::Memory) {
    println!("Memory #{}: \"{}\"", mem.number, mem.name);
    println!(
        "  Frequency:    {} Hz ({:.4} MHz)",
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
    println!();
}

fn print_raw_data(radio: &THD75Radio, mmap: &MemoryMap, number: u32) -> anyhow::Result<()> {
    // Calculate offset
    let group = (number / 3) as usize;
    let index = (number % 3) as usize;
    let offset = 0x4000 + (group * (3 * 80 + 16)) + (index * 80);

    println!("  Raw Data:");
    println!("  Calculated offset: 0x{:04X} (groups of 3)", offset);

    // Try reading raw bytes
    if let Ok(bytes) = mmap.get(offset, Some(40)) {
        print!("  First 40 bytes:   ");
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

        // Decode frequency
        if bytes.len() >= 4 {
            let freq = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if freq != 0 && freq != 0xFFFFFFFF {
                println!(
                    "  Decoded freq:     {} Hz ({:.4} MHz)",
                    freq,
                    freq as f64 / 1_000_000.0
                );
            }
        }
    }

    // Also try groups of 6 formula
    let group_6 = (number / 6) as usize;
    let index_6 = (number % 6) as usize;
    let offset_6 = 0x4000 + (group_6 * (6 * 80 + 16)) + (index_6 * 80);

    if offset_6 != offset {
        println!("  Alternate offset: 0x{:04X} (groups of 6)", offset_6);
        if let Ok(bytes) = mmap.get(offset_6, Some(16)) {
            print!("  First 16 bytes:   ");
            for byte in bytes {
                print!("{:02X} ", byte);
            }
            println!();
        }
    }

    println!();
    Ok(())
}
