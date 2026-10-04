//! Copper polygon pours.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::coord::{Coord, CoordPoint};

/// How a vertex runs to the next: a straight line or an arc (the stored
/// `KIND` integer: 0, 1). A stored value outside the two is kept as
/// [`VertexKind::Unknown`] so it writes back unchanged. Serialized as the
/// stored integer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(from = "i32", into = "i32"))]
pub enum VertexKind {
    /// A straight run (`KIND` 0).
    #[default]
    Line,
    /// An arc about the vertex's centre (`KIND` 1).
    Arc,
    /// Another stored value, kept verbatim.
    Unknown(i32),
}

impl VertexKind {
    /// The kind a stored `KIND` integer names.
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => Self::Line,
            1 => Self::Arc,
            other => Self::Unknown(other),
        }
    }

    /// The integer this kind is stored as.
    pub fn to_raw(self) -> i32 {
        match self {
            Self::Line => 0,
            Self::Arc => 1,
            Self::Unknown(value) => value,
        }
    }
}

impl From<i32> for VertexKind {
    fn from(value: i32) -> Self {
        Self::from_raw(value)
    }
}

impl From<VertexKind> for i32 {
    fn from(kind: VertexKind) -> i32 {
        kind.to_raw()
    }
}

/// One vertex of a [`Polygon`] outline. Linear vertices use [`PolygonVertex::linear`];
/// arc vertices carry the arc geometry inline.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PolygonVertex {
    pub point: CoordPoint,
    /// How the vertex runs to the next.
    pub kind: VertexKind,
    pub arc_center: CoordPoint,
    pub start_angle: f64,
    pub end_angle: f64,
    pub radius: Coord,
}

impl PolygonVertex {
    pub fn linear(point: CoordPoint) -> Self {
        Self {
            point,
            kind: VertexKind::Line,
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
    /// Set when the source file stored vertices in the legacy arc-aware
    /// `KIND<i>`/`VX<i>`/… form rather than `POINTCOUNT` + `SA<i>.X/Y`.
    /// The writer mirrors the source form.
    #[cfg_attr(feature = "serde", serde(default))]
    pub vertices_use_legacy_form: bool,
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
