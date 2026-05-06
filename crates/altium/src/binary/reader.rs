use std::io::{self, Read, Seek, SeekFrom};

use byteorder::{LE, ReadBytesExt};

use super::{BLOCK_FLAGS_SHIFT, BLOCK_SIZE_MASK};
use crate::encoding;
use crate::error::{Error, Result};

/// Little-endian reader for Altium binary streams.
///
/// Expects the underlying source to implement `Read + Seek` so that
/// [`BinaryReader::has_more`] and the various `skip_*` helpers can behave like
/// their C# counterparts (no extra buffering, no peek-ahead allocations).
pub struct BinaryReader<R> {
    inner: R,
    length: u64,
}

impl<R: Read + Seek> BinaryReader<R> {
    /// Wrap a reader. Records the stream length up-front so `has_more` and
    /// related queries are cheap.
    pub fn new(mut inner: R) -> io::Result<Self> {
        let length = inner.seek(SeekFrom::End(0))?;
        inner.seek(SeekFrom::Start(0))?;
        Ok(Self { inner, length })
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    pub fn position(&mut self) -> io::Result<u64> {
        self.inner.stream_position()
    }

    pub fn length(&self) -> u64 {
        self.length
    }

    pub fn has_more(&mut self) -> io::Result<bool> {
        Ok(self.position()? < self.length)
    }

    pub fn seek_to(&mut self, pos: u64) -> io::Result<()> {
        self.inner.seek(SeekFrom::Start(pos)).map(|_| ())
    }

    pub fn skip(&mut self, n: u64) -> io::Result<()> {
        if n == 0 {
            return Ok(());
        }
        let pos = self.position()?;
        self.inner.seek(SeekFrom::Start(pos + n)).map(|_| ())
    }

    /// Read exactly the requested bytes.
    pub fn read_exact(&mut self, dst: &mut [u8]) -> io::Result<()> {
        self.inner.read_exact(dst)
    }

    pub fn read_u8(&mut self) -> io::Result<u8> {
        self.inner.read_u8()
    }

    pub fn read_i16(&mut self) -> io::Result<i16> {
        self.inner.read_i16::<LE>()
    }

    pub fn read_u16(&mut self) -> io::Result<u16> {
        self.inner.read_u16::<LE>()
    }

    pub fn read_i32(&mut self) -> io::Result<i32> {
        self.inner.read_i32::<LE>()
    }

    pub fn read_u32(&mut self) -> io::Result<u32> {
        self.inner.read_u32::<LE>()
    }

    pub fn read_i64(&mut self) -> io::Result<i64> {
        self.inner.read_i64::<LE>()
    }

    pub fn read_f32(&mut self) -> io::Result<f32> {
        self.inner.read_f32::<LE>()
    }

    pub fn read_f64(&mut self) -> io::Result<f64> {
        self.inner.read_f64::<LE>()
    }

    /// Read a one-byte-length-prefixed Windows-1252 string.
    pub fn read_pascal_string(&mut self) -> Result<String> {
        let len = self.read_u8()? as usize;
        if len == 0 {
            return Ok(String::new());
        }
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf)?;
        Ok(encoding::decode(&buf))
    }

    /// Read the size-prefix header and return `(flags, sanitised_size)`.
    pub fn read_block_header(&mut self) -> io::Result<(u8, u32)> {
        let raw = self.read_u32()?;
        let flags = (raw >> BLOCK_FLAGS_SHIFT) as u8;
        let size = raw & BLOCK_SIZE_MASK;
        Ok((flags, size))
    }

    /// Read a size-prefixed block of bytes (flags discarded).
    pub fn read_block(&mut self) -> Result<Vec<u8>> {
        let (_flags, size) = self.read_block_header()?;
        if size == 0 {
            return Ok(Vec::new());
        }
        let mut buf = vec![0u8; size as usize];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Read a size-prefixed block, returning both the flag byte and the bytes.
    pub fn read_block_with_flags(&mut self) -> Result<(u8, Vec<u8>)> {
        let (flags, size) = self.read_block_header()?;
        if size == 0 {
            return Ok((flags, Vec::new()));
        }
        let mut buf = vec![0u8; size as usize];
        self.read_exact(&mut buf)?;
        Ok((flags, buf))
    }

    /// Skip a size-prefixed block, returning its size.
    pub fn skip_block(&mut self) -> io::Result<u32> {
        let (_flags, size) = self.read_block_header()?;
        if size > 0 {
            self.skip(u64::from(size))?;
        }
        Ok(size)
    }

    /// Read a `[i32 size][u8 len][len bytes][padding]` block.
    pub fn read_pascal_string_block(&mut self) -> Result<String> {
        let (_flags, size) = self.read_block_header()?;
        if size == 0 {
            return Ok(String::new());
        }
        let len = self.read_u8()? as u32;
        let mut consumed = 1u32;
        let s = if len == 0 {
            String::new()
        } else {
            let mut buf = vec![0u8; len as usize];
            self.read_exact(&mut buf)?;
            consumed += len;
            encoding::decode(&buf)
        };
        if consumed < size {
            self.skip(u64::from(size - consumed))?;
        }
        Ok(s)
    }

    /// Read a `[i32 size][N bytes — null terminated string + padding]` block.
    /// Used for parameter records and component-body C-strings.
    pub fn read_c_string_block(&mut self) -> Result<String> {
        let bytes = self.read_block()?;
        Ok(decode_c_string(&bytes))
    }

    /// Read a fixed 64-byte UTF-16LE buffer with optional NUL termination.
    pub fn read_font_name(&mut self) -> Result<String> {
        let mut buf = [0u8; 64];
        self.read_exact(&mut buf)?;
        decode_font_name(&buf)
    }
}

/// Find the NUL terminator (if any) and decode the prefix as Windows-1252.
fn decode_c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    encoding::decode(&bytes[..end])
}

fn decode_font_name(buf: &[u8; 64]) -> Result<String> {
    let mut units = [0u16; 32];
    for (i, chunk) in buf.chunks_exact(2).enumerate() {
        units[i] = u16::from_le_bytes([chunk[0], chunk[1]]);
    }
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16(&units[..end]).map_err(|e| Error::Encoding(e.to_string()))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn read_primitives_little_endian() {
        let bytes = [
            0xAA, // u8
            0x01, 0x00, // u16 = 1
            0xFF, 0xFF, // u16 = 65535 / i16 = -1
            0x78, 0x56, 0x34, 0x12, // i32 = 0x12345678
        ];
        let mut r = BinaryReader::new(Cursor::new(&bytes[..])).unwrap();
        assert_eq!(r.read_u8().unwrap(), 0xAA);
        assert_eq!(r.read_u16().unwrap(), 1);
        assert_eq!(r.read_i16().unwrap(), -1);
        assert_eq!(r.read_i32().unwrap(), 0x1234_5678);
        assert!(!r.has_more().unwrap());
    }

    #[test]
    fn read_pascal_string_basic() {
        let bytes = [0x05, b'h', b'e', b'l', b'l', b'o'];
        let mut r = BinaryReader::new(Cursor::new(&bytes[..])).unwrap();
        assert_eq!(r.read_pascal_string().unwrap(), "hello");
    }

    #[test]
    fn read_pascal_string_empty() {
        let bytes = [0x00];
        let mut r = BinaryReader::new(Cursor::new(&bytes[..])).unwrap();
        assert_eq!(r.read_pascal_string().unwrap(), "");
    }

    #[test]
    fn read_block_with_flags() {
        // size = 4 bytes, flags = 0x42
        let mut bytes = vec![];
        bytes.extend_from_slice(&u32::to_le_bytes((0x42 << 24) | 4));
        bytes.extend_from_slice(b"abcd");
        let mut r = BinaryReader::new(Cursor::new(bytes)).unwrap();
        let (flags, body) = r.read_block_with_flags().unwrap();
        assert_eq!(flags, 0x42);
        assert_eq!(&body, b"abcd");
    }

    #[test]
    fn read_pascal_string_block_pads() {
        // size = 8: 1 (len) + 5 (chars) + 2 (padding)
        let mut bytes = vec![];
        bytes.extend_from_slice(&u32::to_le_bytes(8));
        bytes.push(5);
        bytes.extend_from_slice(b"hello");
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(b"NEXT");
        let mut r = BinaryReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(r.read_pascal_string_block().unwrap(), "hello");
        // Position now at the start of "NEXT" (i.e. 4 + 8 = 12).
        let pos = r.position().unwrap();
        assert_eq!(pos, 12);
    }

    #[test]
    fn read_c_string_block_strips_padding() {
        // size = 8: "PARAM" + null + 2 padding bytes.
        let mut bytes = vec![];
        bytes.extend_from_slice(&u32::to_le_bytes(8));
        bytes.extend_from_slice(b"PARAM\0\0\0");
        let mut r = BinaryReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(r.read_c_string_block().unwrap(), "PARAM");
    }

    #[test]
    fn read_font_name_strips_null() {
        let mut buf = [0u8; 64];
        for (i, c) in "Arial".encode_utf16().enumerate() {
            buf[i * 2] = (c & 0xFF) as u8;
            buf[i * 2 + 1] = (c >> 8) as u8;
        }
        let mut r = BinaryReader::new(Cursor::new(buf)).unwrap();
        assert_eq!(r.read_font_name().unwrap(), "Arial");
    }

    #[test]
    fn skip_block_advances_past_payload() {
        let mut bytes = vec![];
        bytes.extend_from_slice(&u32::to_le_bytes(6));
        bytes.extend_from_slice(b"ABCDEF");
        bytes.extend_from_slice(b"after");
        let mut r = BinaryReader::new(Cursor::new(bytes)).unwrap();
        let size = r.skip_block().unwrap();
        assert_eq!(size, 6);
        let mut tail = String::new();
        r.inner.read_to_string(&mut tail).unwrap();
        assert_eq!(tail, "after");
    }

    #[test]
    fn has_more_tracks_position() {
        let bytes = [1u8, 2, 3];
        let mut r = BinaryReader::new(Cursor::new(&bytes[..])).unwrap();
        assert!(r.has_more().unwrap());
        r.read_u8().unwrap();
        r.read_u8().unwrap();
        r.read_u8().unwrap();
        assert!(!r.has_more().unwrap());
    }
}
