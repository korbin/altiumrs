//! Low-level binary read / write helpers for Altium streams. Little-endian.

mod reader;
mod writer;

pub use reader::BinaryReader;
pub use writer::BinaryWriter;

/// Mask that extracts the data size from a block header (low 24 bits).
pub(crate) const BLOCK_SIZE_MASK: u32 = 0x00FF_FFFF;

/// Number of bits the flag byte occupies in a block header.
pub(crate) const BLOCK_FLAGS_SHIFT: u32 = 24;
