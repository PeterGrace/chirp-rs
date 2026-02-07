// Parser combinators using nom for complex binary structures
// Reference: chirp/bitwise.py

use super::bcd::{bcd_to_int_be, bcd_to_int_le};
use nom::{
    bytes::complete::take,
    error::{Error, ErrorKind},
    IResult,
};

/// Parse a BCD-encoded value (big-endian) of specified byte length
pub fn parse_bcd_be(num_bytes: usize) -> impl Fn(&[u8]) -> IResult<&[u8], u64> {
    move |input: &[u8]| {
        let (input, bytes) = take(num_bytes)(input)?;
        let value = bcd_to_int_be(bytes)
            .map_err(|_| nom::Err::Error(Error::new(input, ErrorKind::Verify)))?;
        Ok((input, value))
    }
}

/// Parse a BCD-encoded value (little-endian) of specified byte length
pub fn parse_bcd_le(num_bytes: usize) -> impl Fn(&[u8]) -> IResult<&[u8], u64> {
    move |input: &[u8]| {
        let (input, bytes) = take(num_bytes)(input)?;
        let value = bcd_to_int_le(bytes)
            .map_err(|_| nom::Err::Error(Error::new(input, ErrorKind::Verify)))?;
        Ok((input, value))
    }
}

/// Parse a BCD value with automatic endianness
pub fn parse_bcd(num_bytes: usize, little_endian: bool) -> impl Fn(&[u8]) -> IResult<&[u8], u64> {
    move |input: &[u8]| {
        if little_endian {
            parse_bcd_le(num_bytes)(input)
        } else {
            parse_bcd_be(num_bytes)(input)
        }
    }
}

/// Parse a null-terminated character array
pub fn parse_cstring(max_len: usize) -> impl Fn(&[u8]) -> IResult<&[u8], String> {
    move |input: &[u8]| {
        let (input, bytes) = take(max_len)(input)?;

        // Find null terminator
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());

        // Convert to string, replacing invalid UTF-8
        let s = String::from_utf8_lossy(&bytes[..end]).to_string();

        Ok((input, s))
    }
}

/// Parse a fixed-length character array (not null-terminated)
pub fn parse_char_array(len: usize) -> impl Fn(&[u8]) -> IResult<&[u8], String> {
    move |input: &[u8]| {
        let (input, bytes) = take(len)(input)?;
        let s = String::from_utf8_lossy(bytes).to_string();
        Ok((input, s))
    }
}

/// Parse a u16 big-endian
pub fn parse_u16_be(input: &[u8]) -> IResult<&[u8], u16> {
    let (input, bytes) = take(2usize)(input)?;
    Ok((input, u16::from_be_bytes([bytes[0], bytes[1]])))
}

/// Parse a u16 little-endian
pub fn parse_u16_le(input: &[u8]) -> IResult<&[u8], u16> {
    let (input, bytes) = take(2usize)(input)?;
    Ok((input, u16::from_le_bytes([bytes[0], bytes[1]])))
}

/// Parse a u24 big-endian
pub fn parse_u24_be(input: &[u8]) -> IResult<&[u8], u32> {
    let (input, bytes) = take(3usize)(input)?;
    Ok((input, u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]])))
}

/// Parse a u24 little-endian
pub fn parse_u24_le(input: &[u8]) -> IResult<&[u8], u32> {
    let (input, bytes) = take(3usize)(input)?;
    Ok((input, u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])))
}

/// Parse a u32 big-endian
pub fn parse_u32_be(input: &[u8]) -> IResult<&[u8], u32> {
    let (input, bytes) = take(4usize)(input)?;
    Ok((
        input,
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    ))
}

/// Parse a u32 little-endian
pub fn parse_u32_le(input: &[u8]) -> IResult<&[u8], u32> {
    let (input, bytes) = take(4usize)(input)?;
    Ok((
        input,
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    ))
}

/// Parse an array of elements using a parser
pub fn parse_array<'a, O, F>(
    count_val: usize,
    mut parser: F,
) -> impl FnMut(&'a [u8]) -> IResult<&'a [u8], Vec<O>>
where
    F: FnMut(&'a [u8]) -> IResult<&'a [u8], O>,
{
    move |mut input: &'a [u8]| {
        let mut results = Vec::with_capacity(count_val);
        for _ in 0..count_val {
            let (remaining, value) = parser(input)?;
            results.push(value);
            input = remaining;
        }
        Ok((input, results))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bcd() {
        let data = [0x12, 0x34, 0x56];
        let (_, value) = parse_bcd_be(3)(&data).unwrap();
        assert_eq!(value, 123456);

        let data_le = [0x56, 0x34, 0x12];
        let (_, value) = parse_bcd_le(3)(&data_le).unwrap();
        assert_eq!(value, 123456);
    }

    #[test]
    fn test_parse_cstring() {
        let data = b"Hello\0World";
        let (_, s) = parse_cstring(11)(data).unwrap();
        assert_eq!(s, "Hello");

        let data2 = b"NoNull";
        let (_, s) = parse_cstring(6)(data2).unwrap();
        assert_eq!(s, "NoNull");
    }

    #[test]
    fn test_parse_char_array() {
        let data = b"ABCDEF";
        let (_, s) = parse_char_array(6)(data).unwrap();
        assert_eq!(s, "ABCDEF");
    }

    #[test]
    fn test_parse_integers() {
        let data = [0x12, 0x34];
        let (_, value) = parse_u16_be(&data).unwrap();
        assert_eq!(value, 0x1234);

        let (_, value) = parse_u16_le(&data).unwrap();
        assert_eq!(value, 0x3412);

        let data = [0x12, 0x34, 0x56];
        let (_, value) = parse_u24_be(&data).unwrap();
        assert_eq!(value, 0x123456);

        let data = [0x12, 0x34, 0x56, 0x78];
        let (_, value) = parse_u32_be(&data).unwrap();
        assert_eq!(value, 0x12345678);
    }

    #[test]
    fn test_parse_array() {
        let data = [0x12, 0x34, 0x56, 0x78];
        let mut parser = parse_array(2, parse_u16_be);
        let (_, values) = parser(&data).unwrap();
        assert_eq!(values, vec![0x1234, 0x5678]);
    }
}
