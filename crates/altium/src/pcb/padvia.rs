//! The board's Pad/Via Library (Altium 22 and later): pad and via templates,
//! which primitives are linked to which template, and the via structure
//! (IPC-4761 type) a via template states.
//!
//! Three streams carry it:
//!
//! - `PadViaLibrary/Data`: the board's own library (`PADVIALIBRARY.*`, one
//!   `[i32 length][parameters]` record), then its templates, each
//!   `[u8 kind][i32 length][parameters]` (`TEMPLATE.*`, a via template's `TEMPLATE.VIA.*`, and its
//!   structure: `STRUCTURETYPE`, `FEATURESCOUNT`, `TYPE{i}`, `SIDE{i}`,
//!   `MATERIAL{i}`);
//! - `PadViaLibraryLinks/Data`: one `[i32 length][parameters]` record a
//!   linked primitive
//!   (`PRIMITIVEINDEX`, `PRIMITIVEOBJECTID`, `TEMPLATELINK.LIBRARYID`,
//!   `TEMPLATELINK.TEMPLATEID`);
//! - `PadViaLibraryCache/Data`: the libraries the board's primitives name
//!   whose templates it caches (a library listed there with no templates is
//!   one the file names but does not hold).
//!
//! A via also names its template in its own `Vias6` record
//! ([`super::Via::template_id`]), which is how a via whose template the file
//! does not hold is still seen as linked, to a template left unresolved.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::model3d::parse_models_data;
use crate::coord::Coord;

/// A via's structure: its IPC-4761 type (how the hole is protected: tented,
/// covered, plugged, filled, capped), as a via template's `STRUCTURETYPE`
/// states it.
///
/// The ordinal is the position in IPC-4761's list with "none" first: 0 none,
/// 1 I-a, 2 I-b, 3 II-a, 4 II-b, 5 III-a, 6 III-b, 7 IV-a, 8 IV-b, 9 V,
/// 10 VI-a, 11 VI-b, 12 VII. This reading is inferred from that order (a
/// template named `POFV`, plated over filled via, states 12); a value outside
/// it is [`ViaStructure::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ViaStructure {
    /// No protection: an open via.
    None,
    /// Type I-a: tented on one side.
    TentedOneSide,
    /// Type I-b: tented on both sides.
    TentedBothSides,
    /// Type II-a: tented and covered on one side.
    TentedCoveredOneSide,
    /// Type II-b: tented and covered on both sides.
    TentedCoveredBothSides,
    /// Type III-a: plugged on one side.
    PluggedOneSide,
    /// Type III-b: plugged on both sides.
    PluggedBothSides,
    /// Type IV-a: plugged and covered on one side.
    PluggedCoveredOneSide,
    /// Type IV-b: plugged and covered on both sides.
    PluggedCoveredBothSides,
    /// Type V: filled.
    Filled,
    /// Type VI-a: filled and covered on one side.
    FilledCoveredOneSide,
    /// Type VI-b: filled and covered on both sides.
    FilledCoveredBothSides,
    /// Type VII: filled and capped (plated over).
    FilledCapped,
    /// A `STRUCTURETYPE` outside the list, kept verbatim.
    Unknown(i32),
}

impl ViaStructure {
    /// The structure a `STRUCTURETYPE` value names.
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::TentedOneSide,
            2 => Self::TentedBothSides,
            3 => Self::TentedCoveredOneSide,
            4 => Self::TentedCoveredBothSides,
            5 => Self::PluggedOneSide,
            6 => Self::PluggedBothSides,
            7 => Self::PluggedCoveredOneSide,
            8 => Self::PluggedCoveredBothSides,
            9 => Self::Filled,
            10 => Self::FilledCoveredOneSide,
            11 => Self::FilledCoveredBothSides,
            12 => Self::FilledCapped,
            other => Self::Unknown(other),
        }
    }
}

/// Its IPC-4761 name (`VII`, `I-a`), or `none` or `unknown (<n>)`, for
/// logs; the API is the enum.
impl std::fmt::Display for ViaStructure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::None => "none",
            Self::TentedOneSide => "I-a",
            Self::TentedBothSides => "I-b",
            Self::TentedCoveredOneSide => "II-a",
            Self::TentedCoveredBothSides => "II-b",
            Self::PluggedOneSide => "III-a",
            Self::PluggedBothSides => "III-b",
            Self::PluggedCoveredOneSide => "IV-a",
            Self::PluggedCoveredBothSides => "IV-b",
            Self::Filled => "V",
            Self::FilledCoveredOneSide => "VI-a",
            Self::FilledCoveredBothSides => "VI-b",
            Self::FilledCapped => "VII",
            Self::Unknown(value) => return write!(f, "unknown ({value})"),
        };
        f.write_str(name)
    }
}

/// One feature of a via structure: its kind and side codes and material, as
/// the template writes them (`TYPE{i}`, `SIDE{i}`, `MATERIAL{i}`). The kind
/// and side are the file's codes; their meaning is not established (open).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ViaFeature {
    pub kind: i32,
    pub side: i32,
    pub material: String,
}

/// The via part of a template (`TEMPLATE.VIA.*`) and its structure.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ViaTemplate {
    /// The hole size (`TEMPLATE.VIA.HOLESIZE`).
    pub hole_size: Option<Coord>,
    /// The first stack entry's diameter (`TEMPLATE.VIA.STACKDATA0.DIAMETER`).
    pub diameter: Option<Coord>,
    /// Tented on top (`TEMPLATE.VIA.ISTENTING_TOP`).
    pub tenting_top: Option<bool>,
    /// Tented on the bottom (`TEMPLATE.VIA.ISTENTING_BOTTOM`).
    pub tenting_bottom: Option<bool>,
    /// The structure (`STRUCTURETYPE`), where the template states one.
    pub structure: Option<ViaStructure>,
    /// Its features (`FEATURESCOUNT` of them).
    pub features: Vec<ViaFeature>,
}

/// One pad or via template of a library.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PadViaTemplate {
    /// `TEMPLATE.TEMPLATEID`.
    pub id: String,
    /// `TEMPLATE.LIBRARYID`.
    pub library_id: String,
    /// `TEMPLATE.TEMPLATENAME`.
    pub name: String,
    /// `TEMPLATE.TEMPLATEDESCRIPTION`.
    pub description: String,
    /// The via part, for a via template.
    pub via: Option<ViaTemplate>,
    /// The record's kind byte as the library stream writes it: the file's
    /// code; its meaning is not established (open).
    pub record_kind: u8,
    /// Every parameter as written, keys upper-cased.
    pub parameters: BTreeMap<String, String>,
}

/// One primitive linked to a template (`PadViaLibraryLinks`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PadViaLink {
    /// The primitive's index among the document's pads or vias.
    pub primitive_index: usize,
    /// `Via` or `Pad`.
    pub object: String,
    pub library_id: String,
    pub template_id: String,
}

/// A library the board names (`PADVIALIBRARY.*`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PadViaLibraryInfo {
    pub id: String,
    pub name: String,
}

/// The board's Pad/Via Library: its own library, its templates, its links
/// and the libraries it caches.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PadViaLibraries {
    /// The board's own library (`PadViaLibrary/Data`'s header).
    pub library: Option<PadViaLibraryInfo>,
    /// Its templates.
    pub templates: Vec<PadViaTemplate>,
    /// The primitives linked to templates.
    pub links: Vec<PadViaLink>,
    /// The libraries listed in the cache (`PadViaLibraryCache/Data`).
    pub cached: Vec<PadViaLibraryInfo>,
}

impl PadViaLibraries {
    /// The template of an id (case-insensitive), where the file holds it.
    pub fn template(&self, id: &str) -> Option<&PadViaTemplate> {
        self.templates
            .iter()
            .find(|t| t.id.eq_ignore_ascii_case(id))
    }
}

/// How a via's template resolves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViaTemplateLink<'a> {
    /// The via names no template.
    None,
    /// The via's template, held in the file.
    Resolved(&'a PadViaTemplate),
    /// The via names a template the file does not hold: its id and its
    /// library's.
    Unresolved {
        template_id: &'a str,
        library_id: &'a str,
    },
}

/// Parse the three streams from a document's unmodelled streams.
pub(crate) fn read(streams: &BTreeMap<String, Vec<u8>>) -> PadViaLibraries {
    let records = |name: &str| {
        streams
            .get(name)
            .map(|d| parse_models_data(d))
            .unwrap_or_default()
    };
    let info = |r: &BTreeMap<String, String>| {
        r.get("PADVIALIBRARY.LIBRARYID")
            .map(|id| PadViaLibraryInfo {
                id: id.clone(),
                name: r
                    .get("PADVIALIBRARY.LIBRARYNAME")
                    .cloned()
                    .unwrap_or_default(),
            })
    };
    let mut out = PadViaLibraries::default();
    let library = streams
        .get("PadViaLibrary/Data")
        .map(|d| kind_records(d))
        .unwrap_or_default();
    for (kind, record) in library {
        if let Some(library) = info(&record) {
            out.library.get_or_insert(library);
            continue;
        }
        let Some(id) = record.get("TEMPLATE.TEMPLATEID").cloned() else {
            continue;
        };
        let text = |key: &str| record.get(key).cloned().unwrap_or_default();
        let coord = |key: &str| record.get(key).and_then(|v| Coord::parse_altium(v).ok());
        let flag = |key: &str| record.get(key).map(|v| v.eq_ignore_ascii_case("TRUE"));
        let int = |key: &str| record.get(key).and_then(|v| v.trim().parse::<i32>().ok());
        let is_via = record.keys().any(|k| k.starts_with("TEMPLATE.VIA."));
        let via = is_via.then(|| ViaTemplate {
            hole_size: coord("TEMPLATE.VIA.HOLESIZE"),
            diameter: coord("TEMPLATE.VIA.STACKDATA0.DIAMETER"),
            tenting_top: flag("TEMPLATE.VIA.ISTENTING_TOP"),
            tenting_bottom: flag("TEMPLATE.VIA.ISTENTING_BOTTOM"),
            structure: int("STRUCTURETYPE").map(ViaStructure::from_raw),
            features: (0..int("FEATURESCOUNT").unwrap_or(0))
                .map(|i| ViaFeature {
                    kind: int(&format!("TYPE{i}")).unwrap_or(-1),
                    side: int(&format!("SIDE{i}")).unwrap_or(-1),
                    material: text(&format!("MATERIAL{i}")),
                })
                .collect(),
        });
        out.templates.push(PadViaTemplate {
            id,
            library_id: text("TEMPLATE.LIBRARYID"),
            name: text("TEMPLATE.TEMPLATENAME"),
            description: text("TEMPLATE.TEMPLATEDESCRIPTION"),
            via,
            record_kind: kind,
            parameters: record.clone(),
        });
    }
    for record in records("PadViaLibraryLinks/Data") {
        let (Some(index), Some(template_id)) = (
            record
                .get("PRIMITIVEINDEX")
                .and_then(|v| v.trim().parse::<usize>().ok()),
            record.get("TEMPLATELINK.TEMPLATEID"),
        ) else {
            continue;
        };
        out.links.push(PadViaLink {
            primitive_index: index,
            object: record.get("PRIMITIVEOBJECTID").cloned().unwrap_or_default(),
            library_id: record
                .get("TEMPLATELINK.LIBRARYID")
                .cloned()
                .unwrap_or_default(),
            template_id: template_id.clone(),
        });
    }
    out.cached = records("PadViaLibraryCache/Data")
        .iter()
        .filter_map(info)
        .collect();
    out
}

/// The records of a library stream: the first `[i32 length][parameters]`,
/// then `[u8 kind][i32 length][parameters]` to the end (the header's kind is
/// 0). A record that runs past the stream ends the read.
fn kind_records(data: &[u8]) -> Vec<(u8, BTreeMap<String, String>)> {
    let params = |bytes: &[u8]| {
        let stripped: Vec<u8> = bytes.iter().copied().filter(|&b| b != 0).collect();
        let text = crate::encoding::decode(&stripped);
        text.split('|')
            .filter_map(|part| part.split_once('='))
            .map(|(k, v)| (k.to_uppercase(), v.to_owned()))
            .collect::<BTreeMap<_, _>>()
    };
    let length = |at: usize| {
        data.get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .map(i32::from_le_bytes)
            .and_then(|n| usize::try_from(n).ok())
    };
    let mut out = Vec::new();
    let Some(first) = length(0) else {
        return out;
    };
    let Some(bytes) = data.get(4..4 + first) else {
        return out;
    };
    out.push((0, params(bytes)));
    let mut at = 4 + first;
    while at < data.len() {
        let kind = data[at];
        let Some(n) = length(at + 1) else {
            break;
        };
        let Some(bytes) = data.get(at + 5..at + 5 + n) else {
            break;
        };
        out.push((kind, params(bytes)));
        at += 5 + n;
    }
    out
}

/// A GUID as Altium writes it in text (`{69CF59AA-F061-47B3-...}`) from its
/// 16 bytes as stored in a binary record (Windows layout: the first three
/// fields little-endian); `None` for the all-zero GUID.
pub(crate) fn guid_text(bytes: &[u8]) -> Option<String> {
    let b: [u8; 16] = bytes.get(..16)?.try_into().ok()?;
    if b.iter().all(|&x| x == 0) {
        return None;
    }
    Some(format!(
        "{{{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        b[3],
        b[2],
        b[1],
        b[0],
        b[5],
        b[4],
        b[7],
        b[6],
        b[8],
        b[9],
        b[10],
        b[11],
        b[12],
        b[13],
        b[14],
        b[15]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(records: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for r in records {
            let bytes = r.as_bytes();
            out.extend_from_slice(&(bytes.len() as i32).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        out
    }

    #[test]
    fn guid_bytes_read_in_windows_layout() {
        let bytes = [
            0xaa, 0x59, 0xcf, 0x69, 0x61, 0xf0, 0xb3, 0x47, 0x9e, 0xd8, 0x8b, 0x02, 0x53, 0xf9,
            0xee, 0xcb,
        ];
        assert_eq!(
            guid_text(&bytes).unwrap(),
            "{69CF59AA-F061-47B3-9ED8-8B0253F9EECB}"
        );
        assert_eq!(guid_text(&[0; 16]), None);
    }

    #[test]
    fn a_via_template_and_its_links_are_read() {
        let mut streams = BTreeMap::new();
        let mut library =
            stream(&["|PADVIALIBRARY.LIBRARYID={L}|PADVIALIBRARY.LIBRARYNAME=<Local>"]);
        let template = "|TEMPLATE.LIBRARYID={L}|TEMPLATE.TEMPLATEID={T}|TEMPLATE.TEMPLATENAME=v40h30pofv|TEMPLATE.VIA.HOLESIZE=11.811mil|TEMPLATE.VIA.ISTENTING_TOP=TRUE|TEMPLATE.VIA.STACKDATA0.DIAMETER=15.748mil|STRUCTURETYPE=12|FEATURESCOUNT=2|TYPE0=0|SIDE0=2|MATERIAL0=|TYPE1=4|SIDE1=2|MATERIAL1=";
        library.push(3);
        library.extend_from_slice(&(template.len() as i32).to_le_bytes());
        library.extend_from_slice(template.as_bytes());
        streams.insert("PadViaLibrary/Data".to_string(), library);
        streams.insert(
            "PadViaLibraryLinks/Data".to_string(),
            stream(&["|PRIMITIVEINDEX=3|PRIMITIVEOBJECTID=Via|TEMPLATELINK.LIBRARYID={L}|TEMPLATELINK.TEMPLATEID={T}"]),
        );
        streams.insert(
            "PadViaLibraryCache/Data".to_string(),
            stream(&["|PADVIALIBRARY.LIBRARYID={C}|PADVIALIBRARY.LIBRARYNAME=<Local>"]),
        );
        let libraries = read(&streams);
        assert_eq!(libraries.library.as_ref().unwrap().id, "{L}");
        let template = libraries.template("{t}").unwrap();
        let via = template.via.as_ref().unwrap();
        assert_eq!(via.structure, Some(ViaStructure::FilledCapped));
        assert_eq!(via.structure.unwrap().to_string(), "VII");
        assert_eq!(via.hole_size, Some(Coord::from_mils(11.811)));
        assert_eq!(via.features.len(), 2);
        assert_eq!(via.features[1].kind, 4);
        assert_eq!(template.record_kind, 3);
        assert_eq!(libraries.links[0].primitive_index, 3);
        assert_eq!(libraries.cached[0].id, "{C}");
        assert_eq!(ViaStructure::from_raw(13), ViaStructure::Unknown(13));
    }
}
