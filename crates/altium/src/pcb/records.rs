//! Raw record access for footprint `Data` streams.
//!
//! A footprint's `Data` stream is a name block followed by records of the
//! form `[type u8][u32 len][block]`; pads carry six length-prefixed blocks
//! and texts two (the 252-byte body and the string). This module slices the
//! stream into records without interpreting them, so tools can diff or lint
//! libraries record by record.

use crate::error::{Error, Result};

/// One raw record: its type byte and every byte belonging to it (type byte,
/// length prefixes and blocks included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRecord {
    pub kind: u8,
    pub bytes: Vec<u8>,
}

impl RawRecord {
    /// The main block of the record: the fifth block of a pad, the 252-byte
    /// body of a text, the single block of everything else.
    pub fn main_block(&self) -> &[u8] {
        let b = &self.bytes;
        if self.kind == 2 {
            let mut p = 1;
            for i in 0..5 {
                let len = u32_at(b, p).unwrap_or(0) as usize;
                if i == 4 {
                    return &b[(p + 4).min(b.len())..(p + 4 + len).min(b.len())];
                }
                p += 4 + len;
            }
            return &[];
        }
        let len = u32_at(b, 1).unwrap_or(0) as usize;
        &b[5.min(b.len())..(5 + len).min(b.len())]
    }

    /// Layer byte of the record (first byte of the main block).
    pub fn layer(&self) -> Option<u8> {
        self.main_block().first().copied()
    }

    /// The string of a text record.
    pub fn text(&self) -> Option<String> {
        if self.kind != 5 {
            return None;
        }
        let len = u32_at(&self.bytes, 1)? as usize;
        let s = &self.bytes[5 + len..];
        let n = *s.get(4)? as usize;
        Some(crate::encoding::decode(s.get(5..5 + n)?))
    }
}

fn u32_at(b: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?))
}

/// Split a footprint `Data` stream into its pattern name and raw records.
pub fn split_footprint_records(data: &[u8]) -> Result<(String, Vec<RawRecord>)> {
    let corrupt = |m: &str| Error::corrupt_in(m, "Data");
    let name_len = u32_at(data, 0).ok_or_else(|| corrupt("missing name block"))? as usize;
    let name_block = data
        .get(4..4 + name_len)
        .ok_or_else(|| corrupt("truncated name block"))?;
    let name = match name_block.split_first() {
        Some((&n, rest)) => crate::encoding::decode(&rest[..(n as usize).min(rest.len())]),
        None => String::new(),
    };
    let mut p = 4 + name_len;
    let mut out = Vec::new();
    while p + 5 <= data.len() {
        let kind = data[p];
        let start = p;
        p += 1;
        let blocks = match kind {
            2 => 6,
            5 => 2,
            _ => 1,
        };
        for _ in 0..blocks {
            let len = u32_at(data, p).ok_or_else(|| corrupt("truncated block length"))? as usize;
            p += 4 + len;
            if p > data.len() {
                return Err(corrupt("block runs past the end of the stream"));
            }
        }
        out.push(RawRecord {
            kind,
            bytes: data[start..p].to_vec(),
        });
    }
    Ok((name, out))
}

/// Human name of a record type byte.
pub fn kind_name(kind: u8) -> &'static str {
    match kind {
        1 => "arc",
        2 => "pad",
        3 => "via",
        4 => "track",
        5 => "text",
        6 => "fill",
        11 => "region",
        12 => "body",
        _ => "unknown",
    }
}
