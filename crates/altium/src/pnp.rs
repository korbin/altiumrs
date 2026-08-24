//! Pick-and-place (centroid) export from a PCB document.

use crate::coord::{Coord, CoordPoint};
use crate::pcb::Document;

/// One placement row.
#[derive(Debug, Clone, PartialEq)]
pub struct PnpEntry {
    pub designator: String,
    /// Comment/value; falls back to the source library reference when the
    /// component carries no comment.
    pub comment: String,
    pub footprint: String,
    /// Component reference point, relative to the board origin unless the
    /// caller asked for absolute workspace coordinates.
    pub location: CoordPoint,
    pub rotation: f64,
    pub bottom: bool,
    pub description: String,
}

/// Output dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PnpFormat {
    /// Altium-style: `Designator,Comment,Layer,Footprint,Center-X,Center-Y,Rotation,Description`.
    Altium,
    /// JLCPCB CPL: `Designator,Val,Package,Mid X,Mid Y,Rotation,Layer`.
    Jlc,
    /// KiCad `.pos` CSV: `Ref,Val,Package,PosX,PosY,Rot,Side`.
    Kicad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PnpUnits {
    Mm,
    Mil,
}

/// Altium `COMPONENTKIND` values (the API's `TComponentKind` ordinals).
const KIND_STANDARD: i32 = 0;
const KIND_STANDARD_NO_BOM: i32 = 5;

/// Extract placement entries, sorted by designator (numeric-aware).
///
/// Coordinates are relative to the board origin (matching what Altium's own
/// pick-and-place export shows) unless `absolute` is set.
///
/// Mirrors Altium's built-in export: only Standard components are listed;
/// Standard (No BOM) components join them when `include_no_bom` is set.
/// Mechanical, graphical, net-tie, and jumper components never appear.
pub fn pnp_entries(doc: &Document, absolute: bool, include_no_bom: bool) -> Vec<PnpEntry> {
    let origin = if absolute {
        CoordPoint::default()
    } else {
        doc.board_origin()
    };
    let mut entries: Vec<PnpEntry> = doc
        .components
        .iter()
        .filter(|c| {
            c.component_kind == KIND_STANDARD
                || (include_no_bom && c.component_kind == KIND_STANDARD_NO_BOM)
        })
        .map(|c| PnpEntry {
            designator: c.source_designator.clone().unwrap_or_default(),
            comment: c
                .comment
                .clone()
                .filter(|s| !s.is_empty())
                .or_else(|| c.source_lib_reference.clone())
                .unwrap_or_default(),
            footprint: c
                .pattern
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| c.name.clone()),
            location: CoordPoint::new(c.x - origin.x, c.y - origin.y),
            rotation: c.rotation,
            bottom: c.layer == 32,
            description: c.source_description.clone().unwrap_or_default(),
        })
        .collect();
    entries.sort_by(|a, b| designator_key(&a.designator).cmp(&designator_key(&b.designator)));
    entries
}

/// Sort key splitting a trailing number off the prefix, so `R2` < `R16`.
fn designator_key(s: &str) -> (String, u64) {
    let digits = s.chars().rev().take_while(|c| c.is_ascii_digit()).count();
    let (prefix, num) = s.split_at(s.len() - digits);
    (prefix.to_uppercase(), num.parse().unwrap_or(0))
}

fn fmt_coord(c: Coord, units: PnpUnits) -> String {
    match units {
        PnpUnits::Mm => format!("{:.4}", c.to_mm()),
        PnpUnits::Mil => format!("{:.2}", c.to_mils()),
    }
}

fn fmt_rotation(r: f64) -> String {
    if r.fract() == 0.0 {
        format!("{r:.0}")
    } else {
        format!("{r:.2}")
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Render entries as CSV in the requested dialect.
pub fn format_pnp_csv(entries: &[PnpEntry], format: PnpFormat, units: PnpUnits) -> String {
    let unit_name = match units {
        PnpUnits::Mm => "mm",
        PnpUnits::Mil => "mil",
    };
    let mut out = String::new();
    match format {
        PnpFormat::Altium => {
            out.push_str(&format!(
                "Designator,Comment,Layer,Footprint,Center-X({unit_name}),Center-Y({unit_name}),Rotation,Description\n"
            ));
            for e in entries {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{},{}\n",
                    csv_field(&e.designator),
                    csv_field(&e.comment),
                    if e.bottom { "BottomLayer" } else { "TopLayer" },
                    csv_field(&e.footprint),
                    fmt_coord(e.location.x, units),
                    fmt_coord(e.location.y, units),
                    fmt_rotation(e.rotation),
                    csv_field(&e.description),
                ));
            }
        }
        PnpFormat::Jlc => {
            out.push_str("Designator,Val,Package,Mid X,Mid Y,Rotation,Layer\n");
            for e in entries {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{}\n",
                    csv_field(&e.designator),
                    csv_field(&e.comment),
                    csv_field(&e.footprint),
                    fmt_coord(e.location.x, units),
                    fmt_coord(e.location.y, units),
                    fmt_rotation(e.rotation),
                    if e.bottom { "Bottom" } else { "Top" },
                ));
            }
        }
        PnpFormat::Kicad => {
            out.push_str("Ref,Val,Package,PosX,PosY,Rot,Side\n");
            for e in entries {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{}\n",
                    csv_field(&e.designator),
                    csv_field(&e.comment),
                    csv_field(&e.footprint),
                    fmt_coord(e.location.x, units),
                    fmt_coord(e.location.y, units),
                    fmt_rotation(e.rotation),
                    if e.bottom { "bottom" } else { "top" },
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcb::Component;

    fn doc_with(designators: &[(&str, f64, f64)]) -> Document {
        let mut doc = Document::default();
        doc.board_parameters = Some(vec![
            ("ORIGINX".into(), "100mil".into()),
            ("ORIGINY".into(), "200mil".into()),
        ]);
        for (d, x, y) in designators {
            let mut c = Component::new("R0402");
            c.component_kind = 0;
            c.source_designator = Some(d.to_string());
            c.comment = Some("10K".into());
            c.x = Coord::from_mils(*x);
            c.y = Coord::from_mils(*y);
            c.rotation = 90.0;
            doc.components.push(c);
        }
        doc
    }

    #[test]
    fn origin_relative_and_sorted() {
        let doc = doc_with(&[("R16", 300.0, 400.0), ("R2", 100.0, 200.0)]);
        let entries = pnp_entries(&doc, false, false);
        // Numeric-aware sort: R2 before R16.
        assert_eq!(entries[0].designator, "R2");
        assert_eq!(entries[1].designator, "R16");
        // Board origin (100, 200) subtracted.
        assert_eq!(entries[0].location, CoordPoint::new(Coord::ZERO, Coord::ZERO));
        assert_eq!(
            entries[1].location,
            CoordPoint::new(Coord::from_mils(200.0), Coord::from_mils(200.0))
        );
        // Absolute keeps workspace coordinates.
        let abs = pnp_entries(&doc, true, false);
        assert_eq!(abs[0].location, CoordPoint::new(Coord::from_mils(100.0), Coord::from_mils(200.0)));
    }

    #[test]
    fn kind_filtering_matches_altium() {
        let mut doc = doc_with(&[("R1", 100.0, 200.0)]);
        for (d, kind) in [("TP1", 5), ("MH1", 1), ("G1", 2), ("NT1", 4), ("J1", 6)] {
            let mut c = Component::new("X");
            c.component_kind = kind;
            c.source_designator = Some(d.into());
            doc.components.push(c);
        }
        // Default: Standard only.
        let names: Vec<_> = pnp_entries(&doc, false, false)
            .into_iter()
            .map(|e| e.designator)
            .collect();
        assert_eq!(names, ["R1"]);
        // With the flag: Standard + Standard (No BOM); never the others.
        let names: Vec<_> = pnp_entries(&doc, false, true)
            .into_iter()
            .map(|e| e.designator)
            .collect();
        assert_eq!(names, ["R1", "TP1"]);
    }

    #[test]
    fn csv_dialects() {
        let doc = doc_with(&[("R1", 100.0, 200.0)]);
        let entries = pnp_entries(&doc, false, false);
        let altium = format_pnp_csv(&entries, PnpFormat::Altium, PnpUnits::Mm);
        assert!(altium.starts_with("Designator,Comment,Layer,Footprint,Center-X(mm)"));
        assert!(altium.contains("R1,10K,TopLayer,R0402,0.0000,0.0000,90,"));
        let jlc = format_pnp_csv(&entries, PnpFormat::Jlc, PnpUnits::Mm);
        assert!(jlc.contains("R1,10K,R0402,0.0000,0.0000,90,Top"));
        let kicad = format_pnp_csv(&entries, PnpFormat::Kicad, PnpUnits::Mil);
        assert!(kicad.contains("R1,10K,R0402,0.00,0.00,90,top"));
    }

    #[test]
    fn quotes_fields_with_commas() {
        assert_eq!(csv_field("10K, 1%"), "\"10K, 1%\"");
        assert_eq!(csv_field("plain"), "plain");
    }
}
