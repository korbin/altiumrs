//! Top-level PCB document (`.PcbDoc`).

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::component::Component;
use super::embedded::EmbeddedBoard;
use super::layer::LayerStack;
use super::polygon::Polygon;
use super::primitives::{Arc, ComponentBody, Fill, Net, Pad, Region, Text, Track, Via};
use super::rule::{DifferentialPair, ObjectClass, Room, Rule};
use crate::coord::CoordRect;
use crate::diagnostic::Diagnostic;

/// A PCB document.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Document {
    /// Warnings collected during parsing.
    pub diagnostics: Vec<Diagnostic>,

    pub components: Vec<Component>,
    pub pads: Vec<Pad>,
    pub vias: Vec<Via>,
    pub tracks: Vec<Track>,
    pub arcs: Vec<Arc>,
    pub texts: Vec<Text>,
    pub fills: Vec<Fill>,
    pub regions: Vec<Region>,
    pub component_bodies: Vec<ComponentBody>,
    pub polygons: Vec<Polygon>,
    pub nets: Vec<Net>,
    pub embedded_boards: Vec<EmbeddedBoard>,
    pub rules: Vec<Rule>,
    pub classes: Vec<ObjectClass>,
    pub differential_pairs: Vec<DifferentialPair>,
    pub rooms: Vec<Room>,

    /// Raw `Board6` parameters preserved verbatim. `None` means no `Board6`
    /// storage was present (which is allowed for trivial documents).
    pub board_parameters: Option<BTreeMap<String, String>>,

    /// Additional storages/streams we don't model. Keys use
    /// `"StorageName/StreamName"` (or just `"StreamName"` for root streams).
    pub additional_streams: BTreeMap<String, Vec<u8>>,
}

impl Document {
    /// Layer stack derived from `board_parameters`. Returns `None` if absent.
    pub fn layer_stack(&self) -> Option<LayerStack> {
        let params = self.board_parameters.as_ref()?;
        LayerStack::from_board_parameters(params)
    }

    /// Bounding box of every primitive directly referenced by the document.
    pub fn bounds(&self) -> CoordRect {
        let mut acc = CoordRect::EMPTY;
        for p in &self.pads {
            acc = acc.union(p.bounds());
        }
        for v in &self.vias {
            acc = acc.union(v.bounds());
        }
        for t in &self.tracks {
            acc = acc.union(t.bounds());
        }
        for a in &self.arcs {
            acc = acc.union(a.bounds());
        }
        for x in &self.texts {
            acc = acc.union(x.bounds());
        }
        for f in &self.fills {
            acc = acc.union(f.bounds());
        }
        for r in &self.regions {
            acc = acc.union(r.bounds());
        }
        acc
    }
}
