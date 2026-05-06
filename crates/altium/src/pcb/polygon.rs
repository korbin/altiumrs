//! Copper polygon pours.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::coord::{Coord, CoordPoint};

/// One vertex of a [`Polygon`] outline. Linear vertices use [`PolygonVertex::linear`];
/// arc vertices carry the arc geometry inline.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PolygonVertex {
    pub point: CoordPoint,
    /// 0 = linear, 1 = arc.
    pub kind: i32,
    pub arc_center: CoordPoint,
    pub start_angle: f64,
    pub end_angle: f64,
    pub radius: Coord,
}

impl PolygonVertex {
    pub fn linear(point: CoordPoint) -> Self {
        Self {
            point,
            kind: 0,
            ..Self::default()
        }
    }
}

/// A copper-polygon pour with hatch settings, neck handling, and the full
/// vertex list.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Polygon {
    pub vertices: Vec<PolygonVertex>,
    pub origin: CoordPoint,
    pub layer: i32,
    pub name: Option<String>,
    pub net: Option<String>,
    pub unique_id: Option<String>,

    pub enabled: bool,
    pub is_keepout: bool,
    pub is_electrical_prim: bool,
    pub is_free_primitive: bool,
    pub is_pre_route: bool,
    pub tear_drop: bool,
    pub polygon_outline: bool,
    pub user_routed: bool,
    pub union_index: i32,

    pub is_tenting: bool,
    pub is_tenting_top: bool,
    pub is_tenting_bottom: bool,
    pub is_testpoint_top: bool,
    pub is_testpoint_bottom: bool,
    pub is_assy_testpoint_top: bool,
    pub is_assy_testpoint_bottom: bool,

    pub power_plane_clearance: Coord,
    pub power_plane_connect_style: i32,
    pub power_plane_relief_expansion: Coord,
    pub relief_air_gap: Coord,
    pub relief_conductor_width: Coord,
    pub relief_entries: i32,
    pub solder_mask_expansion: Coord,
    pub primitive_lock: bool,

    pub polygon_type: i32,
    pub poly_hatch_style: i32,
    pub pour_over: i32,
    pub border_width: Coord,
    pub track_size: Coord,
    pub grid: Coord,
    pub min_track: Coord,

    pub avoid_obstacles: bool,
    /// Set when the source file used the legacy `AVOIDOBSTICLES` (sic) spelling
    /// instead of `AVOIDOBST`. The writer mirrors the source spelling so the
    /// document round-trips byte-for-byte.
    pub avoid_obstacles_uses_legacy_key: bool,
    /// Set when the source file used the legacy `POUROVER` spelling instead of
    /// `POURMODE`.
    pub pour_over_uses_legacy_key: bool,
    /// Set when the source file used the legacy `POLYHATCHSTYLE` spelling
    /// instead of `HATCHSTYLE`.
    pub poly_hatch_uses_legacy_key: bool,
    /// Set when the source file used the legacy `REMOVENARROWNECKS` spelling
    /// instead of `REMOVENECKS`.
    pub remove_necks_uses_legacy_key: bool,
    pub arc_pour_mode: bool,
    pub auto_generate_name: bool,
    pub clip_acute_corners: bool,
    pub draw_dead_copper: bool,
    pub draw_removed_islands: bool,
    pub draw_removed_necks: bool,
    pub expand_outline: bool,
    pub ignore_violations: bool,
    pub island_area_threshold: i32,
    pub mitre_corners: bool,
    pub neck_width_threshold: Coord,
    pub obey_polygon_cutout: bool,
    pub optimal_void_rotation: bool,
    pub point_count: i32,
    pub remove_dead: bool,
    pub remove_islands_by_area: bool,
    pub remove_narrow_necks: bool,
    pub use_octagons: bool,

    pub allow_global_edit: bool,
    pub moveable: bool,
    pub paste_mask_expansion: Coord,
    pub is_hidden: bool,
    pub poured: bool,
    pub pour_index: i32,
    pub area_size: i64,
    pub arc_approximation: Coord,
    pub pour_over_same_net_polygons: bool,

    /// Catch-all for parameters we don't model as named fields.
    pub additional_parameters: Option<BTreeMap<String, String>>,
}
