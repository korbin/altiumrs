//! Schematic document (`.SchDoc`).

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::component::Component;
use super::primitives::{
    Arc, Bezier, Blanket, Bus, BusEntry, Ellipse, EllipticalArc, Image, Junction, Label, Line,
    NetLabel, NoErc, Parameter, ParameterSet, Pie, Polygon, Polyline, Port, PowerObject, Rectangle,
    RoundedRectangle, SheetEntry, SheetSymbol, Symbol, TextFrame, Wire,
};
use crate::coord::CoordRect;
use crate::diagnostic::Diagnostic;

/// A schematic document.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Document {
    pub diagnostics: Vec<Diagnostic>,

    pub components: Vec<Component>,
    pub wires: Vec<Wire>,
    pub net_labels: Vec<NetLabel>,
    pub junctions: Vec<Junction>,
    pub power_objects: Vec<PowerObject>,
    pub labels: Vec<Label>,
    pub parameters: Vec<Parameter>,
    pub lines: Vec<Line>,
    pub rectangles: Vec<Rectangle>,
    pub rounded_rectangles: Vec<RoundedRectangle>,
    pub polygons: Vec<Polygon>,
    pub polylines: Vec<Polyline>,
    pub arcs: Vec<Arc>,
    pub beziers: Vec<Bezier>,
    pub ellipses: Vec<Ellipse>,
    pub elliptical_arcs: Vec<EllipticalArc>,
    pub pies: Vec<Pie>,
    pub text_frames: Vec<TextFrame>,
    pub images: Vec<Image>,
    pub symbols: Vec<Symbol>,
    pub no_ercs: Vec<NoErc>,
    pub bus_entries: Vec<BusEntry>,
    pub buses: Vec<Bus>,
    pub ports: Vec<Port>,
    pub sheet_symbols: Vec<SheetSymbol>,
    pub sheet_entries: Vec<SheetEntry>,
    pub blankets: Vec<Blanket>,
    pub parameter_sets: Vec<ParameterSet>,

    /// Header parameters (page size, fonts, grid, etc.).
    pub header_parameters: Option<BTreeMap<String, String>>,
    /// Sheet settings record (RECORD=31) preserved verbatim.
    pub sheet_settings: Option<BTreeMap<String, String>>,
    /// Template-file reference (RECORD=39): pre-component sheet-level record.
    pub template_record: Option<BTreeMap<String, String>>,
    /// Sheet-level annotations (RECORD=32, e.g. SheetName labels) keyed by
    /// `OWNERINDEX` to a [`super::primitives::SheetSymbol`].
    pub sheet_name_annotations: Vec<BTreeMap<String, String>>,
    /// Sheet-level annotations (RECORD=33, e.g. SheetFileName labels).
    pub sheet_filename_annotations: Vec<BTreeMap<String, String>>,
    /// Raw records that don't map to a known type. Held in the order the
    /// reader encountered them so writes can replay them in position.
    pub raw_records: Vec<super::component::RawRecord>,
    /// Storages/streams we don't model.
    pub additional_streams: BTreeMap<String, Vec<u8>>,
    /// Records we couldn't classify; kept so writes stay byte-for-byte close.
    pub opaque_records: Vec<BTreeMap<String, String>>,
}

impl Document {
    pub fn bounds(&self) -> CoordRect {
        let mut acc = CoordRect::EMPTY;
        for comp in &self.components {
            acc = acc.union(comp.bounds());
        }
        for r in &self.rectangles {
            acc = acc.union(CoordRect::from_corners(r.corner1, r.corner2));
        }
        for r in &self.rounded_rectangles {
            acc = acc.union(CoordRect::from_corners(r.corner1, r.corner2));
        }
        for line in &self.lines {
            acc = acc.union(CoordRect::from_corners(line.start, line.end));
        }
        for w in &self.wires {
            for v in &w.vertices {
                acc = acc.union_point(*v);
            }
        }
        for j in &self.junctions {
            acc = acc.union_point(j.location);
        }
        acc
    }
}
