//! Binary helpers shared by the schematic reader and writer.
//!
//! Schematic files mix two coordinate quantisations:
//!
//! - **DXP units** (1 DXP = 10 mil = 100 000 raw) — primary integer in
//!   `LOCATION.X`, `RADIUS`, etc.
//! - **Sub-DXP fractional remainder** (1 frac = 1 raw unit) — companion
//!   `LOCATION.X_FRAC` field. Binary pin records omit the FRAC and stash it
//!   in the auxiliary `PinFrac` stream.

use std::io::{Cursor, Read};

use crate::binary::{BinaryReader, BinaryWriter};
use crate::coord::Coord;
use crate::error::Result;
use crate::parameter::ParameterMap;

/// 1 DXP = 10 mils = 100 000 raw coordinate units.
pub const RAW_PER_DXP: i32 = 100_000;

/// Compose a `Coord` from the DXP integer and sub-DXP fractional remainder.
pub fn coord_from_dxp_frac(dxp: i32, frac: i32) -> Coord {
    Coord::from_raw(dxp.wrapping_mul(RAW_PER_DXP).wrapping_add(frac))
}

/// Decompose a `Coord` into `(dxp, frac)`.
pub fn coord_to_dxp_frac(coord: Coord) -> (i32, i32) {
    let raw = coord.to_raw();
    let dxp = raw / RAW_PER_DXP;
    let frac = raw % RAW_PER_DXP;
    (dxp, frac)
}

/// Map a `LineWidth` index (0=Small, 1=Medium, 2=Large) to a real Coord.
pub fn line_width_from_index(index: i32) -> Coord {
    match index {
        1 => Coord::from_mils(2.0),
        2 => Coord::from_mils(4.0),
        _ => Coord::from_mils(1.0),
    }
}

/// Reverse: pick the closest index for a given line width.
pub fn line_width_to_index(width: Coord) -> i32 {
    let mils = width.to_mils();
    if mils >= 3.0 {
        2
    } else if mils >= 1.5 {
        1
    } else {
        0
    }
}

/// Record-type tags used in the schematic `Data` stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum SchRecordType {
    Component = 1,
    Pin = 2,
    Symbol = 3,
    Label = 4,
    Bezier = 5,
    Polyline = 6,
    Polygon = 7,
    Ellipse = 8,
    Pie = 9,
    RoundedRectangle = 10,
    EllipticalArc = 11,
    Arc = 12,
    Line = 13,
    Rectangle = 14,
    SheetSymbol = 15,
    SheetEntry = 16,
    PowerObject = 17,
    Port = 18,
    NoErc = 22,
    NetLabel = 25,
    Bus = 26,
    Wire = 27,
    TextFrame = 28,
    Junction = 29,
    Image = 30,
    Designator = 34,
    BusEntry = 37,
    Parameter = 41,
    ParameterSet = 43,
    ImplementationList = 44,
    Implementation = 45,
    MapDefinerList = 46,
    MapDefiner = 47,
    ImplementationParameters = 48,
    HarnessConnector = 215,
    HarnessEntry = 216,
    HarnessType = 217,
    SignalHarness = 218,
    Blanket = 225,
}

impl SchRecordType {
    pub fn from_i32(value: i32) -> Option<Self> {
        Some(match value {
            1 => Self::Component,
            2 => Self::Pin,
            3 => Self::Symbol,
            4 => Self::Label,
            5 => Self::Bezier,
            6 => Self::Polyline,
            7 => Self::Polygon,
            8 => Self::Ellipse,
            9 => Self::Pie,
            10 => Self::RoundedRectangle,
            11 => Self::EllipticalArc,
            12 => Self::Arc,
            13 => Self::Line,
            14 => Self::Rectangle,
            15 => Self::SheetSymbol,
            16 => Self::SheetEntry,
            17 => Self::PowerObject,
            18 => Self::Port,
            22 => Self::NoErc,
            25 => Self::NetLabel,
            26 => Self::Bus,
            27 => Self::Wire,
            28 => Self::TextFrame,
            29 => Self::Junction,
            30 => Self::Image,
            34 => Self::Designator,
            37 => Self::BusEntry,
            41 => Self::Parameter,
            43 => Self::ParameterSet,
            44 => Self::ImplementationList,
            45 => Self::Implementation,
            46 => Self::MapDefinerList,
            47 => Self::MapDefiner,
            48 => Self::ImplementationParameters,
            215 => Self::HarnessConnector,
            216 => Self::HarnessEntry,
            217 => Self::HarnessType,
            218 => Self::SignalHarness,
            225 => Self::Blanket,
            _ => return None,
        })
    }
}

/// Decode a binary pin record (`flag = 0x01`) into a parameter map shaped to
/// match the parameter-form record (so `from_params` works uniformly).
/// Mirror of [`decode_binary_pin`] for the writer. Produces the raw 37+
/// byte body that real Altium SchLibs use for pin records (flag = 0x01,
/// not the ASCII RECORD=2 form). Altium's pin renderer reads font/color
/// customisations out of the sibling `PinTextData` stream indexed by pin
/// position; ASCII pin records' `DESIGNATOR.FONTMODE`/`CUSTOMFONTID`
/// fields are NOT honoured for canvas rendering, so to make `--font`
/// actually take effect we have to emit the binary form.
pub fn encode_binary_pin(params: &crate::parameter::ParameterMap) -> Vec<u8> {
    use crate::encoding;
    let mut out = Vec::with_capacity(64);
    let g = |k: &str| params.get(k).unwrap_or("");
    let gi32 = |k: &str| g(k).parse::<i32>().unwrap_or(0);
    out.extend_from_slice(&2i32.to_le_bytes()); // record_type
    out.push(0); // unknown1
    out.extend_from_slice(&(gi32("OWNERPARTID") as i16).to_le_bytes());
    out.push(gi32("OWNERPARTDISPLAYMODE") as u8);
    out.push(gi32("SYMBOL_INNEREDGE") as u8);
    out.push(gi32("SYMBOL_OUTEREDGE") as u8);
    out.push(gi32("SYMBOL_INSIDE") as u8);
    out.push(gi32("SYMBOL_OUTSIDE") as u8);
    let desc = encoding::encode(g("DESCRIPTION"));
    out.push(desc.len() as u8);
    out.extend_from_slice(&desc);
    out.push(gi32("FORMALTYPE") as u8);
    out.push(gi32("ELECTRICAL") as u8);
    out.push(gi32("PINCONGLOMERATE") as u8);
    out.extend_from_slice(&(gi32("PINLENGTH") as i16).to_le_bytes());
    out.extend_from_slice(&(gi32("LOCATION.X") as i16).to_le_bytes());
    out.extend_from_slice(&(gi32("LOCATION.Y") as i16).to_le_bytes());
    out.extend_from_slice(&gi32("COLOR").to_le_bytes());
    for key in [
        "NAME",
        "DESIGNATOR",
        "SWAPIDGROUP",
        "PARTANDSEQUENCE",
        "DEFAULTVALUE",
    ] {
        let bytes = encoding::encode(g(key));
        out.push(bytes.len() as u8);
        out.extend_from_slice(&bytes);
    }
    out
}

pub fn decode_binary_pin(body: &[u8]) -> Result<Option<ParameterMap>> {
    let mut br = BinaryReader::new(Cursor::new(body))?;
    let record_type = br.read_i32()?;
    if record_type != 2 {
        return Ok(None);
    }
    let _unknown1 = br.read_u8()?;
    let owner_part_id = br.read_i16()? as i32;
    let owner_part_display_mode = br.read_u8()? as i32;
    let symbol_inner_edge = br.read_u8()? as i32;
    let symbol_outer_edge = br.read_u8()? as i32;
    let symbol_inside = br.read_u8()? as i32;
    let symbol_outside = br.read_u8()? as i32;
    let description = br.read_pascal_string()?;
    let formal_type = br.read_u8()? as i32;
    let electrical = br.read_u8()? as i32;
    let pin_conglomerate = br.read_u8()? as i32;
    let pin_length = br.read_i16()? as i32;
    let location_x = br.read_i16()? as i32;
    let location_y = br.read_i16()? as i32;
    let color = br.read_i32()?;
    let name = br.read_pascal_string()?;
    let designator = br.read_pascal_string()?;
    let swap_id_group = br.read_pascal_string()?;
    let _part_and_sequence = br.read_pascal_string()?;
    let default_value = br.read_pascal_string()?;

    let mut params = ParameterMap::new();
    params.insert("RECORD", "2");
    params.insert("OWNERPARTID", owner_part_id.to_string());
    params.insert("OWNERPARTDISPLAYMODE", owner_part_display_mode.to_string());
    params.insert("SYMBOL_INNEREDGE", symbol_inner_edge.to_string());
    params.insert("SYMBOL_OUTEREDGE", symbol_outer_edge.to_string());
    params.insert("SYMBOL_INSIDE", symbol_inside.to_string());
    params.insert("SYMBOL_OUTSIDE", symbol_outside.to_string());
    params.insert("DESCRIPTION", description);
    params.insert("FORMALTYPE", formal_type.to_string());
    params.insert("ELECTRICAL", electrical.to_string());
    params.insert("PINCONGLOMERATE", pin_conglomerate.to_string());
    params.insert("PINLENGTH", pin_length.to_string());
    params.insert("LOCATION.X", location_x.to_string());
    params.insert("LOCATION.Y", location_y.to_string());
    params.insert("COLOR", color.to_string());
    params.insert("NAME", name);
    params.insert("DESIGNATOR", designator);
    params.insert("SWAPIDGROUP", swap_id_group);
    params.insert("DEFAULTVALUE", default_value);
    Ok(Some(params))
}

/// Position / font customisation of one pin text (designator or name) as
/// stored in a component's `PinTextData` stream. Each stream entry is the
/// designator block followed by the name block; every block is a flag byte
/// followed by the groups the flags enable:
///
/// ```text
/// flags: u8
/// [flags & 0x01] margin: i32        custom position, raw internal units
/// [flags & 0x02]                    rotation anchor (no payload)
/// [flags & 0x04]                    rotation relative (no payload)
/// [flags & 0x10] font_id: u8, 0u8, color: i32   custom font (+ colour, BGR)
/// ```
///
/// Verified against every entry in the libraries on hand (36k pins); the
/// two payload-less bits are mapped to the `CUSTOMPOSITION.ROTATIONANCHOR`
/// / `ROTATIONRELATIVE` pin properties in that order, which is the one
/// assignment not confirmed by a sample.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PinTextCustomisation {
    pub custom_position: bool,
    /// Raw internal units (1/10000 mil), like `Coord::to_raw`.
    pub margin: i32,
    pub rotation_anchor: bool,
    pub rotation_relative: bool,
    pub custom_font: bool,
    /// Index into the FileHeader font table.
    pub font_id: u8,
    /// BGR colour, meaningful only with `custom_font`.
    pub color: i32,
}

const PIN_TEXT_FLAG_POSITION: u8 = 0x01;
const PIN_TEXT_FLAG_ROTATION_ANCHOR: u8 = 0x02;
const PIN_TEXT_FLAG_ROTATION_RELATIVE: u8 = 0x04;
const PIN_TEXT_FLAG_FONT: u8 = 0x10;
const PIN_TEXT_KNOWN_FLAGS: u8 = PIN_TEXT_FLAG_POSITION
    | PIN_TEXT_FLAG_ROTATION_ANCHOR
    | PIN_TEXT_FLAG_ROTATION_RELATIVE
    | PIN_TEXT_FLAG_FONT;

impl PinTextCustomisation {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Decode one customisation block starting at `*pos`, advancing `pos` past
/// it. Returns `None` (leaving `pos` untouched) on truncation or unknown
/// flag bits.
pub fn decode_pin_text_customisation(bytes: &[u8], pos: &mut usize) -> Option<PinTextCustomisation> {
    let mut p = *pos;
    let flags = *bytes.get(p)?;
    p += 1;
    if flags & !PIN_TEXT_KNOWN_FLAGS != 0 {
        return None;
    }
    let mut c = PinTextCustomisation {
        custom_position: flags & PIN_TEXT_FLAG_POSITION != 0,
        rotation_anchor: flags & PIN_TEXT_FLAG_ROTATION_ANCHOR != 0,
        rotation_relative: flags & PIN_TEXT_FLAG_ROTATION_RELATIVE != 0,
        custom_font: flags & PIN_TEXT_FLAG_FONT != 0,
        ..Default::default()
    };
    if c.custom_position {
        c.margin = i32::from_le_bytes(bytes.get(p..p + 4)?.try_into().ok()?);
        p += 4;
    }
    if c.custom_font {
        c.font_id = *bytes.get(p)?;
        c.color = i32::from_le_bytes(bytes.get(p + 2..p + 6)?.try_into().ok()?);
        p += 6;
    }
    *pos = p;
    Some(c)
}

/// Inverse of [`decode_pin_text_customisation`].
pub fn encode_pin_text_customisation(c: &PinTextCustomisation, out: &mut Vec<u8>) {
    let mut flags = 0u8;
    if c.custom_position {
        flags |= PIN_TEXT_FLAG_POSITION;
    }
    if c.rotation_anchor {
        flags |= PIN_TEXT_FLAG_ROTATION_ANCHOR;
    }
    if c.rotation_relative {
        flags |= PIN_TEXT_FLAG_ROTATION_RELATIVE;
    }
    if c.custom_font {
        flags |= PIN_TEXT_FLAG_FONT;
    }
    out.push(flags);
    if c.custom_position {
        out.extend_from_slice(&c.margin.to_le_bytes());
    }
    if c.custom_font {
        out.push(c.font_id);
        out.push(0);
        out.extend_from_slice(&c.color.to_le_bytes());
    }
}

/// Decompress a zlib-wrapped buffer into a `Vec<u8>`.
pub fn zlib_decompress(input: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = flate2::read::ZlibDecoder::new(input);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

/// Compress to zlib using the default level (level 6) — matches what real
/// Altium emits (`78 9c` zlib magic) for `PinTextData` / `PinFrac` /
/// `Storage` per-entry blocks. `Compression::best` produces `78 da` which
/// is byte-different even though both decompress identically.
pub fn zlib_compress(input: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(input)?;
    Ok(encoder.finish()?)
}

/// Read the "compressed storage" block layout used by `PinFrac`,
/// `PinSymbolLineWidth`, and the schematic `Storage` stream:
///
/// ```text
/// header parameter block
/// while has_more:
///   block:
///     0xD0 tag
///     pascal string (entry name / index)
///     i32 compressed_size
///     compressed_size bytes (zlib body)
/// ```
///
/// Yields each `(name, decompressed)` pair to the visitor.
pub fn read_compressed_storage<F>(data: &[u8], mut visit: F) -> Result<()>
where
    F: FnMut(&str, &[u8]) -> Result<()>,
{
    if data.is_empty() {
        return Ok(());
    }
    let mut br = BinaryReader::new(Cursor::new(data))?;
    // Skip the header parameter block.
    br.skip_block()?;
    while br.has_more()? {
        let (_flags, size) = br.read_block_header()?;
        if size == 0 {
            break;
        }
        let block_start = br.position()?;
        let tag = br.read_u8()?;
        if tag != 0xD0 {
            // Bail; surface incomplete state to the caller.
            let consumed = br.position()? - block_start;
            if u64::from(size) > consumed {
                br.skip(u64::from(size) - consumed)?;
            }
            continue;
        }
        let name = br.read_pascal_string()?;
        let compressed_size = br.read_i32()?;
        if compressed_size <= 0 || compressed_size as u64 > u64::from(size) {
            let consumed = br.position()? - block_start;
            if u64::from(size) > consumed {
                br.skip(u64::from(size) - consumed)?;
            }
            continue;
        }
        let mut compressed = vec![0u8; compressed_size as usize];
        br.read_exact(&mut compressed)?;
        // Skip corrupted entries rather than aborting the whole stream.
        if let Ok(decoded) = zlib_decompress(&compressed) {
            visit(&name, &decoded)?;
        }
        let consumed = br.position()? - block_start;
        if u64::from(size) > consumed {
            br.skip(u64::from(size) - consumed)?;
        }
    }
    Ok(())
}

/// Inverse of [`read_compressed_storage`]: emit a header parameter block
/// followed by `entries.len()` compressed bodies tagged 0xD0.
pub fn write_compressed_storage<W>(
    bw: &mut BinaryWriter<W>,
    header: &ParameterMap,
    entries: &[(String, Vec<u8>)],
) -> Result<()>
where
    W: std::io::Write + std::io::Seek,
{
    use crate::encoding;
    bw.write_block(|w| {
        let mut s = String::new();
        crate::parameter::write_block_into(&mut s, header, '|');
        w.write_bytes(&encoding::encode(&s))?;
        w.write_u8(0)?;
        Ok(())
    })?;
    for (name, raw) in entries {
        let compressed = zlib_compress(raw)?;
        // Each per-entry block carries flag 0x01 in the high byte of its
        // i32 length prefix — Altium uses that bit to distinguish compressed
        // entry blocks from the un-flagged header block. Without it the
        // pin renderer skips the stream and falls back to default fonts.
        bw.write_block_with_flags(0x01, |w| {
            w.write_u8(0xD0)?;
            w.write_pascal_string(name)?;
            w.write_i32(compressed.len() as i32)?;
            w.write_bytes(&compressed)?;
            Ok(())
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxp_round_trip() {
        let coord = Coord::from_raw(123_456_789);
        let (dxp, frac) = coord_to_dxp_frac(coord);
        assert_eq!(dxp, 1234);
        assert_eq!(frac, 56_789);
        assert_eq!(coord_from_dxp_frac(dxp, frac), coord);
    }

    #[test]
    fn dxp_negative() {
        let coord = Coord::from_raw(-50);
        let (dxp, frac) = coord_to_dxp_frac(coord);
        assert_eq!(dxp, 0);
        assert_eq!(frac, -50);
        assert_eq!(coord_from_dxp_frac(dxp, frac), coord);
    }

    #[test]
    fn line_width_indices() {
        assert_eq!(line_width_from_index(0), Coord::from_mils(1.0));
        assert_eq!(line_width_from_index(1), Coord::from_mils(2.0));
        assert_eq!(line_width_from_index(2), Coord::from_mils(4.0));
        assert_eq!(line_width_to_index(Coord::from_mils(1.0)), 0);
        assert_eq!(line_width_to_index(Coord::from_mils(2.0)), 1);
        assert_eq!(line_width_to_index(Coord::from_mils(4.0)), 2);
    }

    #[test]
    fn record_type_round_trip() {
        for v in [1, 2, 3, 12, 22, 25, 41, 44, 47, 225] {
            assert_eq!(SchRecordType::from_i32(v).unwrap() as i32, v);
        }
        assert_eq!(SchRecordType::from_i32(99), None);
    }
}
