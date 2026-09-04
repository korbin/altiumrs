//! Binary helpers shared by the PCB reader and writer.

use std::io::{Read, Seek, Write};

use crate::binary::{BinaryReader, BinaryWriter};
use crate::coord::{Coord, CoordPoint};
use crate::error::Result;

/// Bit 2: Unlocked (inverted — clear means locked).
pub(crate) const FLAG_UNLOCKED: u16 = 0x04;
/// Bit 5: Tenting top.
pub(crate) const FLAG_TENTING_TOP: u16 = 0x20;
/// Bit 6: Tenting bottom.
pub(crate) const FLAG_TENTING_BOTTOM: u16 = 0x40;
/// Bit 9: Keepout.
pub(crate) const FLAG_KEEPOUT: u16 = 0x200;

/// Flags decoded from the per-primitive bitfield.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PrimitiveFlags {
    pub is_locked: bool,
    pub is_tenting_top: bool,
    pub is_tenting_bottom: bool,
    pub is_keepout: bool,
    pub is_polygon_outline: bool,
    pub is_teardrop: bool,
    pub is_testpoint_fab_top: bool,
    pub is_testpoint_fab_bottom: bool,
    /// Flag bits not modelled as booleans (bit 3, `FLAG_ALTIUM_NATIVE`, and
    /// anything unknown); carried through verbatim so a rewrite does not
    /// change them.
    pub extra: u16,
}

/// Bit 1: the track/arc is the outline of a polygon pour.
pub const FLAG_POLYGON_OUTLINE: u16 = 0x02;
/// Bit 3: set by Altium on every primitive it creates itself. Third-party
/// writers (EasyEDA/LCSC exports, for instance) leave it clear and Altium keeps
/// whatever it finds, so it is preserved verbatim through `extra` and set by
/// default on primitives built from scratch. Its meaning is not documented.
pub const FLAG_ALTIUM_NATIVE: u16 = 0x08;
/// Bit 4: the region is a teardrop.
pub const FLAG_TEARDROP: u16 = 0x10;
/// Bit 7: fabrication test point on the top side (pads and vias).
pub const FLAG_TESTPOINT_FAB_TOP: u16 = 0x80;
/// Bit 8: fabrication test point on the bottom side (pads and vias).
pub const FLAG_TESTPOINT_FAB_BOTTOM: u16 = 0x100;

const KNOWN_FLAGS: u16 = FLAG_UNLOCKED
    | FLAG_POLYGON_OUTLINE
    | FLAG_TEARDROP
    | FLAG_TENTING_TOP
    | FLAG_TENTING_BOTTOM
    | FLAG_TESTPOINT_FAB_TOP
    | FLAG_TESTPOINT_FAB_BOTTOM
    | FLAG_KEEPOUT;

impl PrimitiveFlags {
    pub fn decode(bits: u16) -> Self {
        Self {
            is_locked: (bits & FLAG_UNLOCKED) == 0,
            is_tenting_top: (bits & FLAG_TENTING_TOP) != 0,
            is_tenting_bottom: (bits & FLAG_TENTING_BOTTOM) != 0,
            is_keepout: (bits & FLAG_KEEPOUT) != 0,
            is_polygon_outline: (bits & FLAG_POLYGON_OUTLINE) != 0,
            is_teardrop: (bits & FLAG_TEARDROP) != 0,
            is_testpoint_fab_top: (bits & FLAG_TESTPOINT_FAB_TOP) != 0,
            is_testpoint_fab_bottom: (bits & FLAG_TESTPOINT_FAB_BOTTOM) != 0,
            extra: bits & !KNOWN_FLAGS,
        }
    }

    pub fn encode(self) -> u16 {
        let mut bits = 0u16;
        if !self.is_locked {
            bits |= FLAG_UNLOCKED;
        }
        if self.is_tenting_top {
            bits |= FLAG_TENTING_TOP;
        }
        if self.is_tenting_bottom {
            bits |= FLAG_TENTING_BOTTOM;
        }
        if self.is_keepout {
            bits |= FLAG_KEEPOUT;
        }
        if self.is_polygon_outline {
            bits |= FLAG_POLYGON_OUTLINE;
        }
        if self.is_teardrop {
            bits |= FLAG_TEARDROP;
        }
        if self.is_testpoint_fab_top {
            bits |= FLAG_TESTPOINT_FAB_TOP;
        }
        if self.is_testpoint_fab_bottom {
            bits |= FLAG_TESTPOINT_FAB_BOTTOM;
        }
        bits | (self.extra & !KNOWN_FLAGS)
    }
}

/// Decoded common-prefix fields shared by every PCB primitive.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CommonPrefix {
    pub layer: u8,
    pub flags: u16,
    /// Net index into the `Nets6` table, using the in-memory 1-based
    /// convention (`0` for "no net"). On disk the index is 0-based with
    /// `0xFFFF` as the sentinel; the codec translates at the boundary.
    pub net_index: i32,
    /// Component index into the `Components6` table (`-1` for free primitives).
    pub component_index: i32,
}

/// Common primitive prefix (`layer:u8 + flags:u16 + 10 reserved bytes`).
///
/// The 10 reserved bytes carry `[u16 netIndex][u16 ?][u16 componentIndex][u32 ?]`.
/// Both indices use `0xFFFF` as the sentinel for "absent".
pub(crate) fn read_common_prefix<R: Read + Seek>(
    reader: &mut BinaryReader<R>,
) -> Result<CommonPrefix> {
    let layer = reader.read_u8()?;
    let flags = reader.read_u16()?;
    let net_raw = reader.read_u16()?;
    reader.skip(2)?; // reserved
    let component_raw = reader.read_u16()?;
    reader.skip(4)?; // remaining reserved
    let net_index = if net_raw == 0xFFFF {
        0
    } else {
        i32::from(net_raw) + 1
    };
    let component_index = if component_raw == 0xFFFF {
        -1
    } else {
        component_raw as i32
    };
    Ok(CommonPrefix {
        layer,
        flags,
        net_index,
        component_index,
    })
}

/// Mirror of [`read_common_prefix`] for the writer side. The reserved bytes
/// are 0xFF rather than zero — Altium fills them that way and we reproduce it.
pub(crate) fn write_common_prefix<W: Write + Seek>(
    writer: &mut BinaryWriter<W>,
    layer: u8,
    flags: u16,
) -> Result<()> {
    write_common_prefix_full(writer, layer, flags, -1, -1)
}

/// Full common-prefix writer with explicit `net_index` and `component_index`.
/// Pass `-1` for either to emit the `0xFFFF` "absent" sentinel.
pub(crate) fn write_common_prefix_full<W: Write + Seek>(
    writer: &mut BinaryWriter<W>,
    layer: u8,
    flags: u16,
    net_index: i32,
    component_index: i32,
) -> Result<()> {
    writer.write_u8(layer)?;
    writer.write_u16(flags)?;
    // Inverse of the read-side translation: in-memory 1-based (0 = none)
    // to on-disk 0-based (0xFFFF = none).
    let net_word: u16 = if (1..=0xFFFF).contains(&net_index) {
        (net_index - 1) as u16
    } else {
        0xFFFF
    };
    writer.write_u16(net_word)?;
    writer.write_u16(0xFFFF)?; // reserved
    let comp_word: u16 = if (0..=0xFFFE).contains(&component_index) {
        component_index as u16
    } else {
        0xFFFF
    };
    writer.write_u16(comp_word)?;
    writer.write_u32(0xFFFF_FFFF)?; // reserved
    Ok(())
}

/// In-place patches for raw record buffers (see `raw_record` on the
/// primitive structs): the writer patches typed fields into the original
/// bytes so unmodeled content survives verbatim.
pub(crate) fn patch_u8(buf: &mut [u8], off: usize, v: u8) {
    if off < buf.len() {
        buf[off] = v;
    }
}

pub(crate) fn patch_u16(buf: &mut [u8], off: usize, v: u16) {
    if off + 2 <= buf.len() {
        buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }
}

pub(crate) fn patch_i32(buf: &mut [u8], off: usize, v: i32) {
    if off + 4 <= buf.len() {
        buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }
}

pub(crate) fn patch_f64(buf: &mut [u8], off: usize, v: f64) {
    if off + 8 <= buf.len() {
        buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
    }
}

pub(crate) fn patch_point(buf: &mut [u8], off: usize, p: CoordPoint) {
    patch_i32(buf, off, p.x.to_raw());
    patch_i32(buf, off + 4, p.y.to_raw());
}

/// Patch layer/flags/net/component into a raw common prefix. Only the four
/// modeled flag bits are replaced; all other flag bits keep their original
/// values (they carry unmodeled state such as teardrop markers).
pub(crate) fn patch_common_prefix(
    buf: &mut [u8],
    layer: u8,
    flags: PrimitiveFlags,
    net_index: i32,
    component_index: i32,
) {
    if buf.len() < 13 {
        return;
    }
    const MODELED: u16 = FLAG_UNLOCKED | FLAG_TENTING_TOP | FLAG_TENTING_BOTTOM | FLAG_KEEPOUT;
    buf[0] = layer;
    let orig = u16::from_le_bytes([buf[1], buf[2]]);
    patch_u16(buf, 1, (orig & !MODELED) | flags.encode());
    let net_word: u16 = if (1..=0xFFFF).contains(&net_index) {
        (net_index - 1) as u16
    } else {
        0xFFFF
    };
    patch_u16(buf, 3, net_word);
    let comp_word: u16 = if (0..=0xFFFE).contains(&component_index) {
        component_index as u16
    } else {
        0xFFFF
    };
    patch_u16(buf, 7, comp_word);
}

pub(crate) fn read_coord_point<R: Read + Seek>(reader: &mut BinaryReader<R>) -> Result<CoordPoint> {
    let x = reader.read_i32()?;
    let y = reader.read_i32()?;
    Ok(CoordPoint::new(Coord::from_raw(x), Coord::from_raw(y)))
}

pub(crate) fn write_coord_point<W: Write + Seek>(
    writer: &mut BinaryWriter<W>,
    point: CoordPoint,
) -> Result<()> {
    writer.write_i32(point.x.to_raw())?;
    writer.write_i32(point.y.to_raw())?;
    Ok(())
}

/// Map a `V7_LAYER` parameter string (e.g., `"MECHANICAL1"`) to a layer byte.
///
/// Returns `0` for an unrecognised name; that matches the C# heuristic.
pub fn layer_name_to_byte(layer_name: &str) -> u8 {
    if layer_name.is_empty() {
        return 0;
    }
    let normalised: String = layer_name
        .chars()
        .filter(|&c| c != ' ' && c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();

    if let Some(rest) = normalised.strip_prefix("MECHANICAL") {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=16).contains(&n) {
                return 56 + n as u8;
            }
        }
    }
    if let Some(rest) = normalised.strip_prefix("MIDLAYER") {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=30).contains(&n) {
                return 1 + n as u8;
            }
        }
    } else if let Some(rest) = normalised.strip_prefix("MID") {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=30).contains(&n) {
                return 1 + n as u8;
            }
        }
    }
    if let Some(rest) = normalised.strip_prefix("INTERNALPLANE") {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=16).contains(&n) {
                return 38 + n as u8;
            }
        }
    } else if let Some(rest) = normalised.strip_prefix("PLANE") {
        // Short dialect used in param records (e.g. Polygons6 `LAYER=PLANE1`).
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=16).contains(&n) {
                return 38 + n as u8;
            }
        }
    }

    match normalised.as_str() {
        "TOPLAYER" | "TOP" => 1,
        "BOTTOMLAYER" | "BOTTOM" => 32,
        "TOPOVERLAY" => 33,
        "BOTTOMOVERLAY" => 34,
        "TOPPASTE" => 35,
        "BOTTOMPASTE" => 36,
        "TOPSOLDER" => 37,
        "BOTTOMSOLDER" => 38,
        "DRILLGUIDE" => 55,
        "KEEPOUTLAYER" | "KEEPOUT" => 56,
        "DRILLDRAWING" => 73,
        "MULTILAYER" => 74,
        _ => 0,
    }
}

/// Reverse of [`layer_name_to_byte`]: produce the canonical `V7_LAYER` name
/// for a given layer byte. Used when writing primitives that embed the layer
/// name inside their ASCII parameter block (e.g. Region, ComponentBody).
pub fn layer_byte_to_name(byte: u8) -> String {
    match byte {
        1 => "TOPLAYER".to_string(),
        2..=31 => format!("MID{}", byte - 1),
        32 => "BOTTOMLAYER".to_string(),
        33 => "TOPOVERLAY".to_string(),
        34 => "BOTTOMOVERLAY".to_string(),
        35 => "TOPPASTE".to_string(),
        36 => "BOTTOMPASTE".to_string(),
        37 => "TOPSOLDER".to_string(),
        38 => "BOTTOMSOLDER".to_string(),
        39..=54 => format!("INTERNALPLANE{}", byte - 38),
        55 => "DRILLGUIDE".to_string(),
        56 => "KEEPOUTLAYER".to_string(),
        57..=72 => format!("MECHANICAL{}", byte - 56),
        73 => "DRILLDRAWING".to_string(),
        74 => "MULTILAYER".to_string(),
        _ => "MECHANICAL1".to_string(),
    }
}

/// Object IDs for the binary `Data` stream of a PCB component.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectId {
    Arc = 1,
    Pad = 2,
    Via = 3,
    Track = 4,
    Text = 5,
    Fill = 6,
    Region = 11,
    ComponentBody = 12,
}

impl ObjectId {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::Arc),
            2 => Some(Self::Pad),
            3 => Some(Self::Via),
            4 => Some(Self::Track),
            5 => Some(Self::Text),
            6 => Some(Self::Fill),
            11 => Some(Self::Region),
            12 => Some(Self::ComponentBody),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_round_trip() {
        for &(locked, t_top, t_bot, keepout) in &[
            (false, false, false, false),
            (true, true, false, false),
            (false, false, true, true),
            (true, true, true, true),
        ] {
            let f = PrimitiveFlags {
                is_locked: locked,
                is_tenting_top: t_top,
                is_tenting_bottom: t_bot,
                is_keepout: keepout,
                ..PrimitiveFlags::default()
            };
            let bits = f.encode();
            let decoded = PrimitiveFlags::decode(bits);
            assert_eq!(decoded.is_locked, locked);
            assert_eq!(decoded.is_tenting_top, t_top);
            assert_eq!(decoded.is_tenting_bottom, t_bot);
            assert_eq!(decoded.is_keepout, keepout);
        }
    }

    #[test]
    fn unlocked_clear_means_locked() {
        // bits = 0 → all flags absent → locked=true (bit clear), the rest false
        let f = PrimitiveFlags::decode(0);
        assert!(f.is_locked);
        assert!(!f.is_tenting_top);
        assert!(!f.is_tenting_bottom);
        assert!(!f.is_keepout);
    }

    #[test]
    fn layer_names_map_correctly() {
        assert_eq!(layer_name_to_byte("MECHANICAL1"), 57);
        assert_eq!(layer_name_to_byte("Mechanical 16"), 72);
        assert_eq!(layer_name_to_byte("TOP"), 1);
        assert_eq!(layer_name_to_byte("BOTTOMLAYER"), 32);
        assert_eq!(layer_name_to_byte("MIDLAYER1"), 2);
        assert_eq!(layer_name_to_byte("Mid30"), 31);
        assert_eq!(layer_name_to_byte("INTERNALPLANE3"), 41);
        assert_eq!(layer_name_to_byte("MULTILAYER"), 74);
        assert_eq!(layer_name_to_byte(""), 0);
        assert_eq!(layer_name_to_byte("UNKNOWN"), 0);
    }
}
