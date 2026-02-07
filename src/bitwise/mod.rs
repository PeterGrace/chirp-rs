// Binary parsing framework for radio memory structures
// Rust alternative to Python's bitwise DSL

pub mod bcd;
pub mod elements;
pub mod parser;
pub mod types;

pub use bcd::{bcd_to_int, int_to_bcd, BcdArray};
pub use elements::{
    read_u16_be, read_u16_le, read_u24_be, read_u24_le, read_u32_be, read_u32_le, write_u16_be,
    write_u16_le, write_u24_be, write_u24_le, write_u32_be, write_u32_le,
};
pub use parser::{parse_bcd, parse_char_array};
pub use types::Endianness;
