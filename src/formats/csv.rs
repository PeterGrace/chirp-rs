//! CSV file format handler for import/export

use crate::core::Memory;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CsvError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("CSV parse error: {0}")]
    Parse(String),

    #[error("Invalid CSV format: {0}")]
    InvalidFormat(String),
}

pub type Result<T> = std::result::Result<T, CsvError>;

/// Export memories to CSV file
pub fn export_csv(filename: impl AsRef<Path>, memories: &[Memory]) -> Result<()> {
    let mut file = File::create(filename)?;

    // Write header
    let header = Memory::CSV_HEADER.join(",");
    writeln!(file, "{}", header)?;

    // Write each non-empty memory as a CSV row
    for mem in memories {
        // Skip empty memories (check both empty flag and frequency)
        if mem.empty || mem.freq == 0 {
            continue;
        }
        let row = mem.to_csv();
        writeln!(file, "{}", row.join(","))?;
    }

    Ok(())
}

/// Import memories from CSV file
pub fn import_csv(filename: impl AsRef<Path>) -> Result<Vec<Memory>> {
    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let mut memories = Vec::new();
    let mut lines = reader.lines();

    // Read header line
    let header_line = lines
        .next()
        .ok_or_else(|| CsvError::InvalidFormat("Empty CSV file".to_string()))??;

    let headers: Vec<String> = header_line
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    // Build column index map (column name -> position)
    let mut column_map = std::collections::HashMap::new();
    for (idx, header) in headers.iter().enumerate() {
        column_map.insert(header.clone(), idx);
    }

    // Read data lines
    for (line_num, line_result) in lines.enumerate() {
        let line = line_result?;
        if line.trim().is_empty() {
            continue; // Skip empty lines
        }

        match parse_csv_line_flexible(&line, &column_map, line_num + 2) {
            Ok(mem) => memories.push(mem),
            Err(e) => {
                tracing::warn!("Skipping line {}: {}", line_num + 2, e);
                // Continue importing other memories instead of failing completely
            }
        }
    }

    Ok(memories)
}

/// Parse a single CSV line into a Memory struct (flexible column mapping)
fn parse_csv_line_flexible(
    line: &str,
    column_map: &std::collections::HashMap<String, usize>,
    line_num: usize,
) -> Result<Memory> {
    let fields: Vec<&str> = line.split(',').map(|s| s.trim()).collect();

    // Helper to get field by column name
    let get_field = |name: &str| -> Option<&str> {
        column_map
            .get(name)
            .and_then(|&idx| fields.get(idx))
            .copied()
    };

    // Location is required
    let number: u32 = get_field("Location")
        .ok_or_else(|| CsvError::Parse(format!("Line {}: Missing Location column", line_num)))?
        .parse()
        .map_err(|_| CsvError::Parse(format!("Line {}: Invalid location number", line_num)))?;

    let mut mem = Memory::new(number);

    // Optional fields - use defaults if not present
    if let Some(name) = get_field("Name") {
        mem.name = name.to_string();
    }

    if let Some(freq) = get_field("Frequency") {
        mem.freq = parse_frequency(freq)?;
    }

    if let Some(duplex) = get_field("Duplex") {
        mem.duplex = duplex.to_string();
    }

    if let Some(offset) = get_field("Offset") {
        mem.offset = parse_frequency(offset)?;
    }

    if let Some(tmode) = get_field("Tone") {
        mem.tmode = tmode.to_string();
    }

    if let Some(rtone) = get_field("rToneFreq") {
        mem.rtone = rtone.parse().unwrap_or(88.5);
    }

    if let Some(ctone) = get_field("cToneFreq") {
        mem.ctone = ctone.parse().unwrap_or(88.5);
    }

    if let Some(dtcs) = get_field("DtcsCode") {
        mem.dtcs = dtcs.parse().unwrap_or(23);
    }

    if let Some(pol) = get_field("DtcsPolarity") {
        mem.dtcs_polarity = pol.to_string();
    }

    if let Some(rx_dtcs) = get_field("RxDtcsCode") {
        mem.rx_dtcs = rx_dtcs.parse().unwrap_or(23);
    }

    if let Some(cross) = get_field("CrossMode") {
        mem.cross_mode = cross.to_string();
    }

    if let Some(mode) = get_field("Mode") {
        mem.mode = mode.to_string();
    }

    if let Some(tstep) = get_field("TStep") {
        mem.tuning_step = tstep.parse().unwrap_or(5.0);
    }

    if let Some(skip) = get_field("Skip") {
        mem.skip = skip.to_string();
    }

    if let Some(power) = get_field("Power") {
        if !power.is_empty() {
            use crate::core::PowerLevel;
            if let Ok(p) = PowerLevel::parse(power) {
                mem.power = Some(p);
            }
        }
    }

    if let Some(comment) = get_field("Comment") {
        mem.comment = comment.to_string();
    }

    // D-STAR fields
    if let Some(urcall) = get_field("URCALL") {
        mem.dv_urcall = urcall.to_string();
    }

    if let Some(rpt1) = get_field("RPT1CALL") {
        mem.dv_rpt1call = rpt1.to_string();
    }

    if let Some(rpt2) = get_field("RPT2CALL") {
        mem.dv_rpt2call = rpt2.to_string();
    }

    if let Some(dvcode) = get_field("DVCODE") {
        mem.dv_code = dvcode.parse().unwrap_or(0);
    }

    // Bank
    if let Some(bank) = get_field("Bank") {
        mem.bank = bank.parse().unwrap_or(0);
    }

    // Band (for multi-band radios like IC-9700)
    if let Some(band_str) = get_field("Band") {
        if !band_str.is_empty() {
            mem.band = band_str.parse().ok();
        }
    }

    // Mark as non-empty if it has a valid frequency
    mem.empty = mem.freq == 0;

    Ok(mem)
}

/// Parse frequency from MHz string to Hz
fn parse_frequency(freq_str: &str) -> Result<u64> {
    if freq_str.is_empty() || freq_str == "0" {
        return Ok(0);
    }

    let freq_mhz: f64 = freq_str
        .parse()
        .map_err(|_| CsvError::Parse(format!("Invalid frequency: {}", freq_str)))?;

    Ok((freq_mhz * 1_000_000.0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_export_import_roundtrip() -> Result<()> {
        // Create test memories
        let mut mem1 = Memory::new(1);
        mem1.name = "Test 1".to_string();
        mem1.freq = 146_520_000;
        mem1.mode = "FM".to_string();
        mem1.bank = 0;

        let mut mem2 = Memory::new(2);
        mem2.name = "Test 2".to_string();
        mem2.freq = 147_330_000;
        mem2.duplex = "+".to_string();
        mem2.offset = 600_000;
        mem2.tmode = "Tone".to_string();
        mem2.rtone = 100.0;
        mem2.bank = 1;

        let memories = vec![mem1, mem2];

        // Export to CSV (uses official CHIRP format - 21 columns, no Bank)
        let temp_file = NamedTempFile::new().unwrap();
        export_csv(temp_file.path(), &memories)?;

        // Import back
        let imported = import_csv(temp_file.path())?;

        // Verify - note that Bank is NOT preserved since it's not in official CHIRP CSV
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].name, "Test 1");
        assert_eq!(imported[0].freq, 146_520_000);
        assert_eq!(imported[0].bank, 0); // Bank preserved (was 0)
        assert_eq!(imported[1].name, "Test 2");
        assert_eq!(imported[1].freq, 147_330_000);
        assert_eq!(imported[1].duplex, "+");
        assert_eq!(imported[1].bank, 0); // Bank NOT preserved (defaults to 0)

        Ok(())
    }

    #[test]
    fn test_parse_frequency() -> Result<()> {
        assert_eq!(parse_frequency("146.520")?, 146_520_000);
        assert_eq!(parse_frequency("441.950")?, 441_950_000);
        assert_eq!(parse_frequency("0")?, 0);
        assert_eq!(parse_frequency("")?, 0);
        Ok(())
    }

    #[test]
    fn test_import_partial_csv() -> Result<()> {
        // Test CSV with only a subset of columns
        let csv_content = "Location,Name,Frequency,Mode\n1,Test,146.520,FM\n2,Test2,147.330,NFM\n";

        let temp_file = NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), csv_content).unwrap();

        let imported = import_csv(temp_file.path())?;

        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].name, "Test");
        assert_eq!(imported[0].freq, 146_520_000);
        assert_eq!(imported[0].mode, "FM");
        assert_eq!(imported[0].duplex, ""); // Should be default
        assert_eq!(imported[0].bank, 0); // Should be default

        assert_eq!(imported[1].name, "Test2");
        assert_eq!(imported[1].mode, "NFM");

        Ok(())
    }

    #[test]
    fn test_import_different_column_order() -> Result<()> {
        // Test CSV with columns in different order
        let csv_content = "Frequency,Location,Mode,Name\n146.520,5,FM,Reversed\n";

        let temp_file = NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), csv_content).unwrap();

        let imported = import_csv(temp_file.path())?;

        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].number, 5);
        assert_eq!(imported[0].name, "Reversed");
        assert_eq!(imported[0].freq, 146_520_000);
        assert_eq!(imported[0].mode, "FM");

        Ok(())
    }
}
