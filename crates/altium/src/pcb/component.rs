//! PCB footprint (a named collection of pads, tracks, arcs, …).

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::primitives::{Arc, ComponentBody, Fill, Pad, Region, Text, Track, Via};
use crate::coord::{Coord, CoordPoint, CoordRect};

/// A PCB component / footprint.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Component {
    pub name: String,
    pub description: Option<String>,
    pub height: Coord,

    pub comment: Option<String>,
    pub comment_on: bool,
    pub comment_auto_position: i32,
    pub component_kind: i32,
    pub enabled: bool,
    pub enable_pin_swapping: bool,
    pub enable_part_swapping: bool,
    pub flipped_on_layer: bool,
    pub footprint_description: Option<String>,
    pub footprint_configurable_parameters_encoded: Option<String>,
    pub footprint_configurator_name: Option<String>,
    pub fpga_display_mode: i32,
    pub group_num: i32,
    pub is_bga: bool,
    pub is_keepout: bool,
    pub is_electrical_prim: bool,
    pub is_pre_route: bool,
    pub is_tenting: bool,
    pub is_tenting_top: bool,
    pub is_tenting_bottom: bool,
    pub is_testpoint_top: bool,
    pub is_testpoint_bottom: bool,
    pub is_assy_testpoint_top: bool,
    pub is_assy_testpoint_bottom: bool,
    pub item_guid: Option<String>,
    pub item_revision_guid: Option<String>,
    pub jumpers_visible: bool,
    pub layer: i32,
    pub layer_used_top: bool,
    pub lock_strings: bool,
    pub model_hash: Option<String>,
    pub name_auto_position: i32,
    pub name_on: bool,
    pub package_specific_hash: Option<String>,
    pub pattern: Option<String>,
    pub polygon_outline: bool,
    pub power_plane_clearance: Coord,
    pub power_plane_connect_style: i32,
    pub power_plane_relief_expansion: Coord,
    pub primitive_lock: bool,
    pub relief_air_gap: Coord,
    pub relief_conductor_width: Coord,
    pub relief_entries: i32,
    pub rotation: f64,
    pub solder_mask_expansion: Coord,
    pub source_comp_design_item_id: Option<String>,
    pub source_component_library: Option<String>,
    pub source_description: Option<String>,
    pub source_designator: Option<String>,
    pub source_footprint_library: Option<String>,
    pub source_hierarchical_path: Option<String>,
    pub source_lib_reference: Option<String>,
    pub source_unique_id: Option<String>,
    pub tear_drop: bool,
    pub union_index: i32,
    pub unique_id: Option<String>,
    pub user_routed: bool,
    pub vault_guid: Option<String>,
    pub x: Coord,
    pub y: Coord,
    pub default_pcb_3d_model: Option<String>,
    pub axis_count: i32,
    pub channel_offset: i32,
    pub allow_global_edit: bool,
    pub moveable: bool,
    pub paste_mask_expansion: Coord,

    /// OLE streams from this component's storage that we don't model as
    /// typed fields (e.g. `PrimitiveGuids`, `UniqueIDPrimitiveInformation`).
    pub additional_streams: BTreeMap<String, Vec<u8>>,
    /// Parameters from the component's `Parameters` stream that aren't typed.
    pub additional_parameters: BTreeMap<String, String>,

    pub pads: Vec<Pad>,
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
    pub arcs: Vec<Arc>,
    pub texts: Vec<Text>,
    pub fills: Vec<Fill>,
    pub regions: Vec<Region>,
    pub component_bodies: Vec<ComponentBody>,
}

impl Component {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            enabled: true,
            jumpers_visible: true,
            // Standard component kind (Altium's `kind` enum: 0=Standard,
            // 1=Mechanical, 2=Graphical, …). `1` matches what the editor emits
            // for new footprints.
            component_kind: 1,
            // Top layer — placement layer for new footprints.
            layer: 1,
            ..Self::default()
        }
    }

    pub fn bounds(&self) -> CoordRect {
        let mut acc = CoordRect::EMPTY;
        for p in &self.pads {
            acc = acc.union(p.bounds());
        }
        for t in &self.tracks {
            acc = acc.union(t.bounds());
        }
        for v in &self.vias {
            acc = acc.union(v.bounds());
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
        for b in &self.component_bodies {
            acc = acc.union(b.bounds());
        }
        acc
    }

    pub fn origin(&self) -> CoordPoint {
        CoordPoint::new(self.x, self.y)
    }
}
