use std::io::{self, Seek, SeekFrom, Write};

use byteorder::{LE, WriteBytesExt};

use super::{BLOCK_FLAGS_SHIFT, BLOCK_SIZE_MASK};
use crate::encoding;
use crate::error::Result;

/// Little-endian writer for Altium binary streams.
///
/// Requires `Write + Seek` because size-prefixed blocks are emitted with a
/// placeholder header that gets back-patched once the body has been written.
pub struct BinaryWriter<W> {
    inner: W,
}

impl<W: Write + Seek> BinaryWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    pub fn position(&mut self) -> io::Result<u64> {
        self.inner.stream_position()
    }

    pub fn write_u8(&mut self, value: u8) -> io::Result<()> {
        self.inner.write_u8(value)
    }

    pub fn write_i16(&mut self, value: i16) -> io::Result<()> {
        self.inner.write_i16::<LE>(value)
    }

    pub fn write_u16(&mut self, value: u16) -> io::Result<()> {
        self.inner.write_u16::<LE>(value)
    }

    pub fn write_i32(&mut self, value: i32) -> io::Result<()> {
        self.inner.write_i32::<LE>(value)
    }

    pub fn write_u32(&mut self, value: u32) -> io::Result<()> {
        self.inner.write_u32::<LE>(value)
    }

    pub fn write_i64(&mut self, value: i64) -> io::Result<()> {
        self.inner.write_i64::<LE>(value)
    }

    pub fn write_f32(&mut self, value: f32) -> io::Result<()> {
        self.inner.write_f32::<LE>(value)
    }

    pub fn write_f64(&mut self, value: f64) -> io::Result<()> {
        self.inner.write_f64::<LE>(value)
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.inner.write_all(bytes)
    }

    pub fn write_fill(&mut self, byte: u8, count: usize) -> io::Result<()> {
        let chunk = vec![byte; count];
        self.inner.write_all(&chunk)
    }

    /// Write a one-byte-length-prefixed Windows-1252 string. Truncates
    /// silently at 255 bytes — strings that long don't appear in valid Altium
    /// records, but we don't want to panic if they do.
    pub fn write_pascal_string(&mut self, s: &str) -> io::Result<()> {
        let bytes = encoding::encode(s);
        let len = bytes.len().min(u8::MAX as usize) as u8;
        self.inner.write_u8(len)?;
        self.inner.write_all(&bytes[..len as usize])?;
        Ok(())
    }

    /// Write a NUL-terminated Windows-1252 string.
    pub fn write_c_string(&mut self, s: &str) -> io::Result<()> {
        let bytes = encoding::encode(s);
        self.inner.write_all(&bytes)?;
        self.inner.write_u8(0)?;
        Ok(())
    }

    /// Write a fixed 64-byte UTF-16LE buffer (NUL-terminated and zero-padded).
    pub fn write_font_name(&mut self, name: &str) -> io::Result<()> {
        let mut buf = [0u8; 64];
        let mut idx = 0usize;
        for unit in name.encode_utf16() {
            // Reserve 2 bytes for the NUL terminator at positions 62..64.
            if idx + 2 > 62 {
                break;
            }
            buf[idx] = (unit & 0xFF) as u8;
            buf[idx + 1] = (unit >> 8) as u8;
            idx += 2;
        }
        self.inner.write_all(&buf)
    }

    /// Write a size-prefixed block; the closure produces the body, and the
    /// header is back-patched with the byte count and (optional) flag byte.
    pub fn write_block_with_flags<F>(&mut self, flags: u8, body: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        let header_pos = self.position()?;
        self.write_u32(0)?; // placeholder
        body(self)?;
        let end_pos = self.position()?;
        let size = u32::try_from(end_pos - header_pos - 4)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "block too large"))?;
        if size & BLOCK_SIZE_MASK != size {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "block size > 24 bits").into());
        }
        let header = (u32::from(flags) << BLOCK_FLAGS_SHIFT) | size;
        self.inner.seek(SeekFrom::Start(header_pos))?;
        self.write_u32(header)?;
        self.inner.seek(SeekFrom::Start(end_pos))?;
        Ok(())
    }

    /// Equivalent to `write_block_with_flags(0, body)`.
    pub fn write_block<F>(&mut self, body: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        self.write_block_with_flags(0, body)
    }

    /// Write a size-prefixed block whose body is a Pascal-style string.
    pub fn write_pascal_string_block(&mut self, s: &str) -> Result<()> {
        self.write_block(|w| {
            w.write_pascal_string(s)?;
            Ok(())
        })
    }

    /// Write a size-prefixed block whose body is a NUL-terminated string.
    pub fn write_c_string_block(&mut self, s: &str) -> Result<()> {
        self.write_block(|w| {
            w.write_c_string(s)?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::binary::BinaryReader;

    fn round_trip(write: impl FnOnce(&mut BinaryWriter<Cursor<Vec<u8>>>) -> Result<()>) -> Vec<u8> {
        let mut w = BinaryWriter::new(Cursor::new(Vec::new()));
        write(&mut w).unwrap();
        w.into_inner().into_inner()
    }

    #[test]
    fn primitive_round_trip() {
        let bytes = round_trip(|w| {
            w.write_u8(0xAA)?;
            w.write_i16(-1)?;
            w.write_u32(0x1234_5678)?;
            w.write_f64(2.5)?;
            Ok(())
        });
        let mut r = BinaryReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(r.read_u8().unwrap(), 0xAA);
        assert_eq!(r.read_i16().unwrap(), -1);
        assert_eq!(r.read_u32().unwrap(), 0x1234_5678);
        assert_eq!(r.read_f64().unwrap(), 2.5);
    }

    #[test]
    fn pascal_string_round_trip() {
        let bytes = round_trip(|w| {
            w.write_pascal_string("hello")?;
            Ok(())
        });
        let mut r = BinaryReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(r.read_pascal_string().unwrap(), "hello");
    }

    #[test]
    fn block_back_patches_size_and_flags() {
        let bytes = round_trip(|w| {
            w.write_block_with_flags(0x42, |w| {
                w.write_bytes(b"abcdef")?;
                Ok(())
            })?;
            Ok(())
        });
        assert_eq!(bytes.len(), 4 + 6);
        let header = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(header >> 24, 0x42);
        assert_eq!(header & 0x00FF_FFFF, 6);
        assert_eq!(&bytes[4..], b"abcdef");
    }

    #[test]
    fn pascal_string_block_round_trips() {
        let bytes = round_trip(|w| {
            w.write_pascal_string_block("KEY=value")?;
            Ok(())
        });
        let mut r = BinaryReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(r.read_pascal_string_block().unwrap(), "KEY=value");
    }

    #[test]
    fn c_string_block_round_trips() {
        let bytes = round_trip(|w| {
            w.write_c_string_block("|FOO=bar|BAZ=qux")?;
            Ok(())
        });
        let mut r = BinaryReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(r.read_c_string_block().unwrap(), "|FOO=bar|BAZ=qux");
    }

    #[test]
    fn font_name_round_trips() {
        let bytes = round_trip(|w| {
            w.write_font_name("Arial")?;
            Ok(())
        });
        assert_eq!(bytes.len(), 64);
        let mut r = BinaryReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(r.read_font_name().unwrap(), "Arial");
    }

    #[test]
    fn write_fill() {
        let bytes = round_trip(|w| {
            w.write_fill(0xFF, 4)?;
            Ok(())
        });
        assert_eq!(bytes, vec![0xFF; 4]);
    }
}
