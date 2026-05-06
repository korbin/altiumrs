//! Embedded board placeholder primitive (a sub-board referenced from the
//! parent design).

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::coord::Coord;

/// A reference to another PCB document embedded into this one.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EmbeddedBoard {
    pub document_path: Option<String>,
    pub layer: i32,
    pub rotation: f64,
    pub mirror_flag: bool,
    pub origin_mode: i32,
    pub scale: f64,
    pub col_count: i32,
    pub col_spacing: Coord,
    pub row_count: i32,
    pub row_spacing: Coord,
    pub unique_id: Option<String>,
    pub enabled: bool,
    pub is_keepout: bool,
    pub is_electrical_prim: bool,
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
    pub is_viewport: bool,
    pub viewport_title: Option<String>,
    pub viewport_visible: bool,
    pub title_font_color: i32,
    pub title_font_name: Option<String>,
    pub title_font_size: i32,
    pub title_object: i32,
    pub transmit_board_shape: bool,
    pub transmit_dimensions: bool,
    pub transmit_drill_table: bool,
    pub transmit_layers_enabled_top: bool,
    pub transmit_layer_stack_table: bool,
    pub transmit_parameters_count: i32,
    pub allow_global_edit: bool,
    pub moveable: bool,
    pub paste_mask_expansion: Coord,
    pub is_hidden: bool,
    pub x1_location: Coord,
    pub y1_location: Coord,
    pub x2_location: Coord,
    pub y2_location: Coord,
}
