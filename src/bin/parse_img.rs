//! Parse .img file utility
//! Loads a CHIRP .img file and displays decoded memories with bank information

use chirp_rs::drivers::thd75::THD75Radio;
use chirp_rs::drivers::{CloneModeRadio, Radio};
use chirp_rs::formats::img::load_img;
use std::env;

fn main() -> anyhow::Result<()> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file.img> [memory_number]", args[0]);
        eprintln!("\nExamples:");
        eprintln!(
            "  {} radio.img                # Show all non-empty memories",
            args[0]
        );
        eprintln!(
            "  {} radio.img 40             # Show only memory #40",
            args[0]
        );
        eprintln!(
            "  {} radio.img 32-50          # Show memories 32-50",
            args[0]
        );
        std::process::exit(1);
    }

    let img_file = &args[1];
    let filter = args.get(2).map(|s| s.as_str());

    // Load .img file
    println!("Loading .img file: {}", img_file);
    let (mmap, metadata) = load_img(img_file)?;
    println!("Radio: {} {}", metadata.vendor, metadata.model);
    println!("CHIRP version: {}", metadata.chirp_version);
    println!("Memory map size: {} bytes\n", mmap.len());

    // Create radio driver
    let mut radio = THD75Radio::new();

    // Load memory map into radio
    radio.process_mmap(&mmap)?;

    // Read and display bank names
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

    // Parse based on filter
    match filter {
        None => {
            // Show all non-empty memories
            println!("=== All Non-Empty Memories ===\n");
            let memories = radio.get_memories()?;

            // Filter out empty memories for display
            let non_empty: Vec<_> = memories.iter().filter(|m| !m.empty).collect();
            println!("Found {} non-empty memories\n", non_empty.len());

            for mem in non_empty {
                print_memory_with_mmap(mem, Some(&mmap));
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
                    Some(mem) => print_memory_with_mmap(&mem, Some(&mmap)),
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
                    print_memory_with_mmap(&mem, Some(&mmap));
                    print_raw_bank_data(&radio, &mmap, num)?;
                }
                None => println!("Memory #{}: <empty>", num),
            }
        }
    }

    Ok(())
}

fn print_memory(mem: &chirp_rs::core::Memory) {
    print_memory_with_mmap(mem, None);
}

fn print_memory_with_bank_name(mem: &chirp_rs::core::Memory, bank_name: &str) {
    println!("Memory #{}: \"{}\"", mem.number, mem.name);
    println!(
        "  Frequency:    {} Hz ({:.4} MHz)",
        mem.freq,
        mem.freq as f64 / 1_000_000.0
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
    println!(
        "  Offset:       {} Hz ({:.2} MHz)",
        mem.offset,
        mem.offset as f64 / 1_000_000.0
    );

    // Show tone information
    if !mem.tmode.is_empty() {
        println!("  Tone Mode:    {}", mem.tmode);
        if mem.tmode == "Tone" || mem.tmode == "TSQL" {
            println!("  CTCSS TX:     {} Hz", mem.rtone);
        }
        if mem.tmode == "TSQL" {
            println!("  CTCSS RX:     {} Hz", mem.ctone);
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
    println!("  Bank:         {} (\"{}\")", mem.bank, bank_name);
    println!();
}

fn print_memory_with_mmap(
    mem: &chirp_rs::core::Memory,
    mmap: Option<&chirp_rs::memmap::MemoryMap>,
) {
    // Get bank name if mmap is provided
    let bank_name = if let Some(map) = mmap {
        get_bank_name(map, mem.bank as usize)
    } else {
        format!("Bank {}", mem.bank)
    };
    print_memory_with_bank_name(mem, &bank_name);
}

fn get_bank_name(mmap: &chirp_rs::memmap::MemoryMap, bank_index: usize) -> String {
    const GROUP_NAME_OFFSET: usize = 1152;
    const NAME_SECTION_START: usize = 0x10000;

    let name_offset = NAME_SECTION_START + ((GROUP_NAME_OFFSET + bank_index) * 16);
    if let Ok(name_bytes) = mmap.get(name_offset, Some(16)) {
        let name = String::from_utf8_lossy(name_bytes)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        if name.is_empty() {
            format!("Bank {}", bank_index)
        } else {
            name
        }
    } else {
        format!("Bank {}", bank_index)
    }
}

fn print_raw_bank_data(
    _radio: &THD75Radio,
    mmap: &chirp_rs::memmap::MemoryMap,
    number: u32,
) -> anyhow::Result<()> {
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
    Ok(())
}
