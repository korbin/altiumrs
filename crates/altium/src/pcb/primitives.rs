//! Geometric primitives that live on a PCB: pads, tracks, vias, arcs, text,
//! fills, regions, and 3D component bodies.

/// Flag bits Altium sets on every primitive it creates (bit 3); see `binary::FLAG_ALTIUM_NATIVE`.
fn default_flags_extra() -> u16 {
    super::binary::FLAG_ALTIUM_NATIVE
}

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::coord::{Coord, CoordPoint, CoordRect};
use crate::enums::{PadHoleType, PadShape, PcbStrokeFont, PcbTextKind, TextJustification};

/// A copper trace segment.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Track {
    pub start: CoordPoint,
    pub end: CoordPoint,
    pub width: Coord,
    pub layer: i32,
    pub net: Option<String>,
    pub net_index: u16,
    /// Index into the document's `components` list, `-1` for free primitives.
    pub component_index: i32,
    pub unique_id: Option<String>,
    pub user_routed: bool,
    pub union_index: i32,

    pub enabled: bool,
    pub is_locked: bool,
    pub is_keepout: bool,
    /// Unmodelled flag bits from the record header, preserved verbatim.
    #[cfg_attr(feature = "serde", serde(default = "default_flags_extra"))]
    pub flags_extra: u16,
    /// Outline segment of a polygon pour (flag bit 1).
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_polygon_outline: bool,
    pub polygon_outline: bool,

    pub is_free_primitive: bool,
    pub is_electrical_prim: bool,
    pub is_pre_route: bool,
    pub tear_drop: bool,

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
    pub paste_mask_expansion: Coord,
    pub is_hidden: bool,
    pub allow_global_edit: bool,
    pub moveable: bool,

    /// Original on-disk record body, preserved so unmodeled bytes (union
    /// membership, teardrop flags, format tails) survive write-back. The
    /// writer patches the typed fields above into this buffer when present.
    #[cfg_attr(
        feature = "serde",
        serde(
            default,
            with = "crate::serde_bytes::b64_opt",
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub raw_record: Option<Vec<u8>>,
}

impl Default for Track {
    fn default() -> Self {
        // `enabled: true` mirrors what Altium emits — copper primitives without
        // it are read by Altium but rendered invisible.
        Self {
            start: CoordPoint::default(),
            end: CoordPoint::default(),
            width: Coord::ZERO,
            layer: 0,
            net: None,
            net_index: 0,
            component_index: -1,
            unique_id: None,
            user_routed: false,
            union_index: 0,
            enabled: true,
            is_locked: false,
            is_keepout: false,
            flags_extra: default_flags_extra(),
            is_polygon_outline: false,
            polygon_outline: false,
            is_free_primitive: false,
            is_electrical_prim: false,
            is_pre_route: false,
            tear_drop: false,
            is_tenting: false,
            is_tenting_top: false,
            is_tenting_bottom: false,
            is_testpoint_top: false,
            is_testpoint_bottom: false,
            is_assy_testpoint_top: false,
            is_assy_testpoint_bottom: false,
            power_plane_clearance: Coord::ZERO,
            power_plane_connect_style: 0,
            power_plane_relief_expansion: Coord::ZERO,
            relief_air_gap: Coord::ZERO,
            relief_conductor_width: Coord::ZERO,
            relief_entries: 0,
            solder_mask_expansion: Coord::ZERO,
            paste_mask_expansion: Coord::ZERO,
            is_hidden: false,
            allow_global_edit: false,
            moveable: false,
            raw_record: None,
        }
    }
}

impl Track {
    pub fn bounds(&self) -> CoordRect {
        let half = self.width / 2;
        CoordRect::from_xyxy(
            self.start.x.min(self.end.x) - half,
            self.start.y.min(self.end.y) - half,
            self.start.x.max(self.end.x) + half,
            self.start.y.max(self.end.y) + half,
        )
    }
}

/// An arc primitive (full circles use `start_angle = 0`, `end_angle = 360`).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Arc {
    pub center: CoordPoint,
    pub radius: Coord,
    pub start_angle: f64,
    pub end_angle: f64,
    pub width: Coord,
    pub layer: i32,
    pub net: Option<String>,
    pub net_index: u16,
    /// Index into the document's `components` list, `-1` for free primitives.
    pub component_index: i32,
    pub unique_id: Option<String>,
    pub user_routed: bool,
    pub union_index: i32,

    pub enabled: bool,
    pub is_locked: bool,
    pub is_keepout: bool,
    /// Unmodelled flag bits from the record header, preserved verbatim.
    #[cfg_attr(feature = "serde", serde(default = "default_flags_extra"))]
    pub flags_extra: u16,
    /// Outline segment of a polygon pour (flag bit 1).
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_polygon_outline: bool,
    pub polygon_outline: bool,

    pub is_free_primitive: bool,
    pub is_electrical_prim: bool,
    pub is_pre_route: bool,
    pub tear_drop: bool,

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
    pub paste_mask_expansion: Coord,
    pub is_hidden: bool,
    pub allow_global_edit: bool,
    pub moveable: bool,

    /// Original on-disk record body, preserved so unmodeled bytes (union
    /// membership, teardrop flags, format tails) survive write-back. The
    /// writer patches the typed fields above into this buffer when present.
    #[cfg_attr(
        feature = "serde",
        serde(
            default,
            with = "crate::serde_bytes::b64_opt",
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub raw_record: Option<Vec<u8>>,
}

impl Default for Arc {
    fn default() -> Self {
        Self {
            center: CoordPoint::default(),
            radius: Coord::ZERO,
            start_angle: 0.0,
            end_angle: 0.0,
            width: Coord::ZERO,
            layer: 0,
            net: None,
            net_index: 0,
            component_index: -1,
            unique_id: None,
            user_routed: false,
            union_index: 0,
            enabled: true,
            is_locked: false,
            is_keepout: false,
            flags_extra: default_flags_extra(),
            is_polygon_outline: false,
            polygon_outline: false,
            is_free_primitive: false,
            is_electrical_prim: false,
            is_pre_route: false,
            tear_drop: false,
            is_tenting: false,
            is_tenting_top: false,
            is_tenting_bottom: false,
            is_testpoint_top: false,
            is_testpoint_bottom: false,
            is_assy_testpoint_top: false,
            is_assy_testpoint_bottom: false,
            power_plane_clearance: Coord::ZERO,
            power_plane_connect_style: 0,
            power_plane_relief_expansion: Coord::ZERO,
            relief_air_gap: Coord::ZERO,
            relief_conductor_width: Coord::ZERO,
            relief_entries: 0,
            solder_mask_expansion: Coord::ZERO,
            paste_mask_expansion: Coord::ZERO,
            is_hidden: false,
            allow_global_edit: false,
            moveable: false,
            raw_record: None,
        }
    }
}

impl Arc {
    pub fn bounds(&self) -> CoordRect {
        let extent = self.radius + self.width / 2;
        CoordRect::from_center(self.center, extent * 2, extent * 2)
    }

    pub fn sweep_angle(&self) -> f64 {
        self.end_angle - self.start_angle
    }
}

/// A copper pad. Holds the per-layer size/shape arrays needed for round-tripping
/// the on-disk 596-byte size/shape block.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Pad {
    pub designator: Option<String>,
    pub name: Option<String>,
    pub net: Option<String>,
    pub net_index: u16,
    pub component_index: i32,
    pub unique_id: Option<String>,

    pub location: CoordPoint,
    pub layer: i32,
    pub rotation: f64,

    /// Top-layer pad size.
    pub size_top: CoordPoint,
    /// Mid-layer pad size (used when `mode != 0`).
    pub size_middle: CoordPoint,
    /// Bottom-layer pad size.
    pub size_bottom: CoordPoint,

    pub shape_top: PadShape,
    pub shape_middle: PadShape,
    pub shape_bottom: PadShape,

    pub hole_size: Coord,
    pub hole_type: PadHoleType,
    pub hole_width: Coord,
    pub hole_rotation: f64,
    pub hole_slot_length: i32,
    pub drill_type: i32,
    pub is_plated: bool,
    pub is_hole_size_valid: bool,
    pub hole_positive_tolerance: Coord,
    pub hole_negative_tolerance: Coord,

    /// Stack mode (0=Simple, 1=Top-Mid-Bottom, 2=Full Stack).
    pub mode: i32,
    pub corner_radius_percentage: i32,

    pub power_plane_connect_style: i32,
    pub power_plane_clearance: Coord,
    pub power_plane_relief_expansion: Coord,
    pub relief_air_gap: Coord,
    pub relief_conductor_width: Coord,
    pub relief_entries: i32,

    pub paste_mask_expansion: Coord,
    pub solder_mask_expansion: Coord,
    pub solder_mask_expansion_from_hole_edge: bool,
    pub solder_mask_expansion_from_hole_edge_with_rule: bool,

    pub is_tenting_top: bool,
    pub is_tenting_bottom: bool,
    pub is_tenting: bool,
    pub is_top_paste_enabled: bool,
    pub is_bottom_paste_enabled: bool,

    pub swap_id_pad: Option<String>,
    pub swap_id_part: Option<String>,
    pub swapped_pad_name: Option<String>,
    pub jumper_id: i32,
    pub owner_part_id: i32,
    pub daisy_chain_style: i32,

    pub enabled: bool,
    pub is_locked: bool,
    pub is_keepout: bool,
    /// Unmodelled flag bits from the record header, preserved verbatim.
    #[cfg_attr(feature = "serde", serde(default = "default_flags_extra"))]
    pub flags_extra: u16,
    /// Fabrication test point, top side (flag bit 7).
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_testpoint_fab_top: bool,
    /// Fabrication test point, bottom side (flag bit 8).
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_testpoint_fab_bottom: bool,
    pub is_hidden: bool,
    pub is_test_point_top: bool,
    pub is_test_point_bottom: bool,
    pub is_assy_testpoint_top: bool,
    pub is_assy_testpoint_bottom: bool,
    pub user_routed: bool,
    pub union_index: i32,
    pub is_free_primitive: bool,
    pub is_electrical_prim: bool,
    pub is_pre_route: bool,
    pub tear_drop: bool,
    pub polygon_outline: bool,
    pub is_surface_mount: bool,
    pub is_pad_stack: bool,
    pub is_virtual_pin: bool,
    pub is_counter_hole: bool,
    pub allow_global_edit: bool,
    pub moveable: bool,

    pub has_corner_radius_chamfer: bool,
    pub has_custom_chamfered_rectangle: bool,
    pub has_custom_donut: bool,
    pub has_custom_mask_donut_shapes: bool,
    pub has_custom_mask_shapes: bool,
    pub has_custom_rounded_rectangle: bool,
    pub has_custom_shapes: bool,
    pub has_rounded_rectangular_shapes: bool,
    pub multi_layer_high_bits: i32,

    pub pad_has_offset_on_any: bool,
    pub x_pad_offset_all: Coord,
    pub y_pad_offset_all: Coord,
    pub pin_package_length: Coord,
    pub max_c_signal_layers: Coord,
    pub max_x_signal_layers: Coord,
    pub max_y_signal_layers: Coord,

    pub top_x_size: Coord,
    pub top_y_size: Coord,
    pub mid_x_size: Coord,
    pub mid_y_size: Coord,
    pub bot_x_size: Coord,
    pub bot_y_size: Coord,
    pub top_shape: i32,
    pub mid_shape: i32,
    pub bot_shape: i32,

    /// Per-layer X sizes for 29 internal copper layers (raw values).
    pub layer_x_sizes: [i32; 29],
    /// Per-layer Y sizes for 29 internal copper layers (raw values).
    pub layer_y_sizes: [i32; 29],
    /// Per-layer shape values (29 internal layers).
    pub internal_layer_shapes: [u8; 29],

    /// Per-layer X offsets from hole centre (32 layers, raw).
    pub offset_x_from_hole_center: [i32; 32],
    /// Per-layer Y offsets from hole centre (32 layers, raw).
    pub offset_y_from_hole_center: [i32; 32],

    /// Whether per-layer rounded-rectangle overrides are active.
    pub has_rounded_rect_byte: u8,
    /// Per-layer shape overrides (32 layers).
    pub per_layer_shapes: [u8; 32],
    /// Per-layer corner-radius percentages (32 layers).
    pub per_layer_corner_radii: [u8; 32],
    /// Whether the extended size/shape block was present in the original file.
    pub has_size_shape_block: bool,

    /// Verbatim bytes of the reserved block that appears between the
    /// designator and the net-string blocks. Real Altium emits a single zero
    /// byte; capture-and-replay preserves any payload Altium has put there in
    /// a future format revision so we don't silently strip it.
    #[cfg_attr(feature = "serde", serde(default, with = "crate::serde_bytes::b64"))]
    pub reserved_block_after_designator: Vec<u8>,
    /// Verbatim bytes of the reserved block that follows the net string.
    #[cfg_attr(feature = "serde", serde(default, with = "crate::serde_bytes::b64"))]
    pub reserved_block_after_net_string: Vec<u8>,
    /// The pad's net-string block, typically `"|&|0"` in real Altium files.
    pub net_string_block: String,

    /// Original on-disk record body, preserved so unmodeled bytes (union
    /// membership, teardrop flags, format tails) survive write-back. The
    /// writer patches the typed fields above into this buffer when present.
    #[cfg_attr(
        feature = "serde",
        serde(
            default,
            with = "crate::serde_bytes::b64_opt",
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub raw_record: Option<Vec<u8>>,
    /// Original size/shape block bytes (see `raw_record`).
    #[cfg_attr(
        feature = "serde",
        serde(
            default,
            with = "crate::serde_bytes::b64_opt",
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub raw_size_shape: Option<Vec<u8>>,
}

impl Default for Pad {
    fn default() -> Self {
        Self {
            designator: None,
            name: None,
            net: None,
            net_index: 0,
            component_index: -1,
            unique_id: None,
            location: CoordPoint::default(),
            layer: 0,
            rotation: 0.0,
            size_top: CoordPoint::default(),
            size_middle: CoordPoint::default(),
            size_bottom: CoordPoint::default(),
            shape_top: PadShape::Round,
            shape_middle: PadShape::Round,
            shape_bottom: PadShape::Round,
            hole_size: Coord::ZERO,
            hole_type: PadHoleType::Round,
            hole_width: Coord::ZERO,
            hole_rotation: 0.0,
            hole_slot_length: 0,
            drill_type: 0,
            is_plated: true,
            is_hole_size_valid: false,
            hole_positive_tolerance: Coord::ZERO,
            hole_negative_tolerance: Coord::ZERO,
            mode: 0,
            corner_radius_percentage: 50,
            power_plane_connect_style: 0,
            power_plane_clearance: Coord::ZERO,
            power_plane_relief_expansion: Coord::ZERO,
            relief_air_gap: Coord::ZERO,
            relief_conductor_width: Coord::ZERO,
            // Altium's standard 4-spoke thermal relief.
            relief_entries: 4,
            paste_mask_expansion: Coord::ZERO,
            solder_mask_expansion: Coord::ZERO,
            solder_mask_expansion_from_hole_edge: false,
            solder_mask_expansion_from_hole_edge_with_rule: false,
            is_tenting_top: false,
            is_tenting_bottom: false,
            is_tenting: false,
            is_top_paste_enabled: false,
            is_bottom_paste_enabled: false,
            swap_id_pad: None,
            swap_id_part: None,
            swapped_pad_name: None,
            jumper_id: 0,
            owner_part_id: 0,
            daisy_chain_style: 0,
            enabled: true,
            is_locked: false,
            is_keepout: false,
            flags_extra: default_flags_extra(),
            is_testpoint_fab_top: false,
            is_testpoint_fab_bottom: false,
            is_hidden: false,
            is_test_point_top: false,
            is_test_point_bottom: false,
            is_assy_testpoint_top: false,
            is_assy_testpoint_bottom: false,
            user_routed: false,
            union_index: 0,
            is_free_primitive: false,
            is_electrical_prim: false,
            is_pre_route: false,
            tear_drop: false,
            polygon_outline: false,
            is_surface_mount: false,
            is_pad_stack: false,
            is_virtual_pin: false,
            is_counter_hole: false,
            allow_global_edit: false,
            moveable: false,
            has_corner_radius_chamfer: false,
            has_custom_chamfered_rectangle: false,
            has_custom_donut: false,
            has_custom_mask_donut_shapes: false,
            has_custom_mask_shapes: false,
            has_custom_rounded_rectangle: false,
            has_custom_shapes: false,
            has_rounded_rectangular_shapes: false,
            multi_layer_high_bits: 0,
            pad_has_offset_on_any: false,
            x_pad_offset_all: Coord::ZERO,
            y_pad_offset_all: Coord::ZERO,
            pin_package_length: Coord::ZERO,
            max_c_signal_layers: Coord::ZERO,
            max_x_signal_layers: Coord::ZERO,
            max_y_signal_layers: Coord::ZERO,
            top_x_size: Coord::ZERO,
            top_y_size: Coord::ZERO,
            mid_x_size: Coord::ZERO,
            mid_y_size: Coord::ZERO,
            bot_x_size: Coord::ZERO,
            bot_y_size: Coord::ZERO,
            top_shape: 0,
            mid_shape: 0,
            bot_shape: 0,
            layer_x_sizes: [0; 29],
            layer_y_sizes: [0; 29],
            internal_layer_shapes: [0; 29],
            offset_x_from_hole_center: [0; 32],
            offset_y_from_hole_center: [0; 32],
            has_rounded_rect_byte: 0,
            per_layer_shapes: [0; 32],
            per_layer_corner_radii: [0; 32],
            has_size_shape_block: false,
            // Match what real Altium emits for fresh pads.
            reserved_block_after_designator: vec![0u8],
            reserved_block_after_net_string: vec![0u8],
            net_string_block: "|&|0".to_string(),
            raw_record: None,
            raw_size_shape: None,
        }
    }
}

impl Pad {
    pub fn bounds(&self) -> CoordRect {
        CoordRect::from_center(self.location, self.size_top.x, self.size_top.y)
    }
}

/// A plated through-hole connection between layers.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Via {
    pub location: CoordPoint,
    pub diameter: Coord,
    pub hole_size: Coord,
    pub start_layer: i32,
    pub end_layer: i32,
    pub layer: i32,
    /// Index into the document's `Nets6` table; `0` means "no net".
    pub net_index: u16,
    /// Index into the document's `Components6` table; `-1` for free vias.
    pub component_index: i32,
    pub net: Option<String>,
    pub unique_id: Option<String>,

    pub is_tented: bool,
    pub is_test_point: bool,
    pub solder_mask_expansion: Coord,
    pub solder_mask_expansion_manual: bool,
    pub paste_mask_expansion: Coord,
    pub is_hidden: bool,
    pub allow_global_edit: bool,
    pub moveable: bool,
    pub is_tenting_top: bool,
    pub is_tenting_bottom: bool,

    pub enabled: bool,
    pub is_locked: bool,
    pub is_keepout: bool,
    /// Unmodelled flag bits from the record header, preserved verbatim.
    #[cfg_attr(feature = "serde", serde(default = "default_flags_extra"))]
    pub flags_extra: u16,
    /// Fabrication test point, top side (flag bit 7).
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_testpoint_fab_top: bool,
    /// Fabrication test point, bottom side (flag bit 8).
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_testpoint_fab_bottom: bool,
    pub mode: i32,
    pub height: Coord,
    pub is_plated: bool,

    pub thermal_relief_air_gap: Coord,
    pub thermal_relief_conductors: i32,
    pub thermal_relief_conductors_width: Coord,

    pub power_plane_clearance: Coord,
    pub power_plane_relief_expansion: Coord,
    pub power_plane_connect_style: i32,

    pub drill_layer_pair_type: i32,
    pub hole_positive_tolerance: Coord,
    pub hole_negative_tolerance: Coord,

    pub is_free_primitive: bool,
    pub is_pre_route: bool,
    pub tear_drop: bool,
    pub is_testpoint_top: bool,
    pub is_testpoint_bottom: bool,
    pub is_assy_testpoint_top: bool,
    pub is_assy_testpoint_bottom: bool,
    pub is_electrical_prim: bool,
    pub is_backdrill: bool,
    pub is_counter_hole: bool,
    pub user_routed: bool,
    pub union_index: i32,
    pub polygon_outline: bool,
    pub solder_mask_expansion_from_hole_edge: bool,

    /// Per-layer diameter values (32 entries).
    pub diameters: [Coord; 32],

    /// 8-byte reserved field that real Altium emits as `[0,0,0,1,1,1,1,0]`.
    /// Captured on read so non-canonical files round-trip byte-for-byte.
    pub reserved_block_8: [u8; 8],
    /// 1-byte reserved field after the solder-mask-manual flag (default `1`).
    pub reserved_byte_after_mask_flag: u8,
    /// `i16` reserved field after the per-layer diameters (default `15`).
    pub trailing_reserved_i16: i16,
    /// `i32` reserved field at the end of the via record (default `259`).
    pub trailing_reserved_i32: i32,

    /// Original on-disk record body, preserved so unmodeled bytes (union
    /// membership, teardrop flags, format tails) survive write-back. The
    /// writer patches the typed fields above into this buffer when present.
    #[cfg_attr(
        feature = "serde",
        serde(
            default,
            with = "crate::serde_bytes::b64_opt",
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub raw_record: Option<Vec<u8>>,
}

impl Default for Via {
    fn default() -> Self {
        Self {
            location: CoordPoint::default(),
            diameter: Coord::ZERO,
            hole_size: Coord::ZERO,
            start_layer: 1,
            end_layer: 32,
            layer: 74,
            net_index: 0,
            component_index: -1,
            net: None,
            unique_id: None,
            is_tented: false,
            is_test_point: false,
            solder_mask_expansion: Coord::ZERO,
            solder_mask_expansion_manual: false,
            paste_mask_expansion: Coord::ZERO,
            is_hidden: false,
            allow_global_edit: false,
            moveable: false,
            is_tenting_top: false,
            is_tenting_bottom: false,
            enabled: true,
            is_locked: false,
            is_keepout: false,
            flags_extra: default_flags_extra(),
            is_testpoint_fab_top: false,
            is_testpoint_fab_bottom: false,
            mode: 0,
            height: Coord::ZERO,
            is_plated: true,
            thermal_relief_air_gap: Coord::ZERO,
            thermal_relief_conductors: 0,
            thermal_relief_conductors_width: Coord::ZERO,
            power_plane_clearance: Coord::ZERO,
            power_plane_relief_expansion: Coord::ZERO,
            power_plane_connect_style: 0,
            drill_layer_pair_type: 0,
            hole_positive_tolerance: Coord::ZERO,
            hole_negative_tolerance: Coord::ZERO,
            is_free_primitive: false,
            is_pre_route: false,
            tear_drop: false,
            is_testpoint_top: false,
            is_testpoint_bottom: false,
            is_assy_testpoint_top: false,
            is_assy_testpoint_bottom: false,
            is_electrical_prim: false,
            is_backdrill: false,
            is_counter_hole: false,
            user_routed: false,
            union_index: 0,
            polygon_outline: false,
            solder_mask_expansion_from_hole_edge: false,
            diameters: [Coord::ZERO; 32],
            reserved_block_8: [0, 0, 0, 1, 1, 1, 1, 0],
            reserved_byte_after_mask_flag: 1,
            trailing_reserved_i16: 15,
            trailing_reserved_i32: 259,
            raw_record: None,
        }
    }
}

impl Via {
    pub fn bounds(&self) -> CoordRect {
        CoordRect::from_center(self.location, self.diameter, self.diameter)
    }
}

/// A solid filled rectangle (axis-aligned but rotatable).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Fill {
    pub corner1: CoordPoint,
    pub corner2: CoordPoint,
    pub layer: i32,
    pub rotation: f64,
    pub net: Option<String>,
    pub net_index: u16,
    /// Index into the document's `components` list, `-1` for free fills.
    pub component_index: i32,
    pub unique_id: Option<String>,
    pub is_locked: bool,
    pub user_routed: bool,
    pub union_index: i32,
    pub enabled: bool,
    pub is_keepout: bool,
    /// Unmodelled flag bits from the record header, preserved verbatim.
    #[cfg_attr(feature = "serde", serde(default = "default_flags_extra"))]
    pub flags_extra: u16,
    pub is_free_primitive: bool,
    pub is_electrical_prim: bool,
    pub is_pre_route: bool,
    pub tear_drop: bool,
    pub polygon_outline: bool,
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
    pub paste_mask_expansion: Coord,
    pub is_hidden: bool,
    pub allow_global_edit: bool,
    pub moveable: bool,
    pub is_redundant: bool,

    /// Original on-disk record body, preserved so unmodeled bytes (union
    /// membership, teardrop flags, format tails) survive write-back. The
    /// writer patches the typed fields above into this buffer when present.
    #[cfg_attr(
        feature = "serde",
        serde(
            default,
            with = "crate::serde_bytes::b64_opt",
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub raw_record: Option<Vec<u8>>,
}

impl Default for Fill {
    fn default() -> Self {
        Self {
            corner1: CoordPoint::default(),
            corner2: CoordPoint::default(),
            layer: 0,
            rotation: 0.0,
            net: None,
            net_index: 0,
            component_index: -1,
            unique_id: None,
            is_locked: false,
            user_routed: false,
            union_index: 0,
            enabled: true,
            is_keepout: false,
            flags_extra: default_flags_extra(),
            is_free_primitive: false,
            is_electrical_prim: false,
            is_pre_route: false,
            tear_drop: false,
            polygon_outline: false,
            is_tenting: false,
            is_tenting_top: false,
            is_tenting_bottom: false,
            is_testpoint_top: false,
            is_testpoint_bottom: false,
            is_assy_testpoint_top: false,
            is_assy_testpoint_bottom: false,
            power_plane_clearance: Coord::ZERO,
            power_plane_connect_style: 0,
            power_plane_relief_expansion: Coord::ZERO,
            relief_air_gap: Coord::ZERO,
            relief_conductor_width: Coord::ZERO,
            relief_entries: 0,
            solder_mask_expansion: Coord::ZERO,
            paste_mask_expansion: Coord::ZERO,
            is_hidden: false,
            allow_global_edit: false,
            moveable: false,
            is_redundant: false,
            raw_record: None,
        }
    }
}

impl Fill {
    pub fn bounds(&self) -> CoordRect {
        CoordRect::from_corners(self.corner1, self.corner2)
    }

    pub fn width(&self) -> Coord {
        (self.corner2.x - self.corner1.x).abs()
    }

    pub fn height(&self) -> Coord {
        (self.corner2.y - self.corner1.y).abs()
    }
}

/// A polygon outline in copper or as a keepout.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Region {
    pub outline: Vec<CoordPoint>,
    /// Hole outlines (cutouts) following the main outline on disk.
    #[cfg_attr(feature = "serde", serde(default))]
    pub holes: Vec<Vec<CoordPoint>>,
    pub layer: i32,
    pub kind: i32,
    /// Index into the document's `Nets6` table; `0` means "no net".
    pub net_index: u16,
    /// Index into the document's `Components6` table; `-1` for free regions.
    pub component_index: i32,
    pub net: Option<String>,
    pub name: Option<String>,
    pub unique_id: Option<String>,
    pub is_locked: bool,
    pub enabled: bool,
    pub is_keepout: bool,
    /// Unmodelled flag bits from the record header, preserved verbatim.
    #[cfg_attr(feature = "serde", serde(default = "default_flags_extra"))]
    pub flags_extra: u16,
    /// The region is a teardrop (flag bit 4).
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_teardrop: bool,
    pub cavity_height: Coord,
    pub user_routed: bool,
    pub union_index: i32,
    /// `-1` for standalone regions; `>=0` is the sub-polygon ordinal
    /// inside a parent polygon-pour decomposition.
    pub sub_poly_index: i32,
    /// `TRUE` when the region is the geometry of a custom-shape pad;
    /// distinguishes shape-based regions from polygon-pour fragments
    /// and auto-generated teardrop / track-widening regions.
    pub is_shape_based: bool,
    /// Arc-approximation resolution carried in the Region6 record
    /// (typically 0.5 mil). Kept so writer output round-trips.
    pub arc_resolution: f64,
    pub is_free_primitive: bool,
    pub is_electrical_prim: bool,
    pub is_pre_route: bool,
    pub tear_drop: bool,
    pub polygon_outline: bool,
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
    pub paste_mask_expansion: Coord,
    pub is_hidden: bool,
    pub allow_global_edit: bool,
    pub moveable: bool,
    pub is_simple_region: bool,
    pub virtual_cutout: bool,
    pub hole_count: i32,
    pub total_vertex_count: i32,
    pub area: i64,
    pub arc_approximation: Coord,
    /// Parameters in the C-string block we don't model as fields.
    pub additional_parameters: Option<BTreeMap<String, String>>,

    /// Original on-disk record body, preserved so unmodeled bytes (union
    /// membership, teardrop flags, format tails) survive write-back. The
    /// writer patches the typed fields above into this buffer when present.
    #[cfg_attr(
        feature = "serde",
        serde(
            default,
            with = "crate::serde_bytes::b64_opt",
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub raw_record: Option<Vec<u8>>,
}

impl Default for Region {
    fn default() -> Self {
        Self {
            outline: Vec::new(),
            holes: Vec::new(),
            layer: 0,
            kind: 0,
            net_index: 0,
            component_index: -1,
            net: None,
            name: None,
            unique_id: None,
            is_locked: false,
            enabled: true,
            is_keepout: false,
            flags_extra: default_flags_extra(),
            is_teardrop: false,
            cavity_height: Coord::ZERO,
            user_routed: false,
            union_index: 0,
            sub_poly_index: -1,
            is_shape_based: false,
            arc_resolution: 0.5,
            is_free_primitive: false,
            is_electrical_prim: false,
            is_pre_route: false,
            tear_drop: false,
            polygon_outline: false,
            is_tenting: false,
            is_tenting_top: false,
            is_tenting_bottom: false,
            is_testpoint_top: false,
            is_testpoint_bottom: false,
            is_assy_testpoint_top: false,
            is_assy_testpoint_bottom: false,
            power_plane_clearance: Coord::ZERO,
            power_plane_connect_style: 0,
            power_plane_relief_expansion: Coord::ZERO,
            relief_air_gap: Coord::ZERO,
            relief_conductor_width: Coord::ZERO,
            relief_entries: 0,
            solder_mask_expansion: Coord::ZERO,
            paste_mask_expansion: Coord::ZERO,
            is_hidden: false,
            allow_global_edit: false,
            moveable: false,
            is_simple_region: false,
            virtual_cutout: false,
            hole_count: 0,
            total_vertex_count: 0,
            area: 0,
            arc_approximation: Coord::ZERO,
            additional_parameters: None,
            raw_record: None,
        }
    }
}

impl Region {
    pub fn bounds(&self) -> CoordRect {
        if self.outline.is_empty() {
            return CoordRect::EMPTY;
        }
        let mut acc = CoordRect::EMPTY;
        for pt in &self.outline {
            acc = acc.union_point(*pt);
        }
        acc
    }
}

/// PCB silkscreen / copper text. Stores TrueType + barcode auxiliary fields
/// alongside the stroke-font basics.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Text {
    pub text: String,
    pub location: CoordPoint,
    pub height: Coord,
    pub stroke_width: Coord,
    pub rotation: f64,
    pub layer: i32,
    /// Index into the document's `Nets6` table; `0` means "no net".
    pub net_index: u16,
    /// Index into the document's `Components6` table; `-1` for free text.
    pub component_index: i32,
    pub is_mirrored: bool,
    pub justification: TextJustification,

    pub is_truetype: bool,
    pub font_name: Option<String>,
    pub text_kind: PcbTextKind,
    pub stroke_font: PcbStrokeFont,
    pub font_bold: bool,
    pub font_italic: bool,

    pub is_inverted: bool,
    pub use_inverted_rectangle: bool,
    pub inverted_border: Coord,
    pub inverted_rect_width: Coord,
    pub inverted_rect_height: Coord,
    pub inverted_rect_justification: TextJustification,
    pub inverted_rect_text_offset: Coord,

    pub barcode_lr_margin: Coord,
    pub barcode_tb_margin: Coord,
    pub bar_code_kind: i32,
    pub bar_code_bit_pattern: Option<String>,
    pub bar_code_full_height: Coord,
    pub bar_code_full_width: Coord,
    pub bar_code_min_width: Coord,
    pub bar_code_show_text: bool,
    pub bar_code_inverted: bool,
    pub bar_code_render_mode: i32,
    pub bar_code_font_name: Option<String>,
    pub bar_code_x_margin: Coord,
    pub bar_code_y_margin: Coord,

    pub font_id: i32,
    pub use_tt_fonts: bool,
    pub multi_line: bool,
    pub word_wrap: bool,
    pub mirror_flag: bool,

    pub is_locked: bool,
    pub is_keepout: bool,
    /// Unmodelled flag bits from the record header, preserved verbatim.
    #[cfg_attr(feature = "serde", serde(default = "default_flags_extra"))]
    pub flags_extra: u16,
    /// V7 layer id written in the record tail when it cannot be derived from
    /// `layer` (mechanical layers 17..32, whose layer byte Altium clamps to
    /// Mechanical 16). Zero means "derive from `layer`".
    #[cfg_attr(feature = "serde", serde(default))]
    pub layer_v7: u32,
    pub size: Coord,
    pub width: Coord,
    pub multiline_text_height: Coord,
    pub multiline_text_width: Coord,
    pub multiline_text_resize_enabled: bool,
    pub multiline_text_auto_position: i32,
    pub ttf_text_height: Coord,
    pub ttf_text_width: Coord,
    pub ttf_inverted_text_justify: i32,
    pub ttf_offset_from_inverted_rect: Coord,

    pub snap_point_x: Coord,
    pub snap_point_y: Coord,
    pub x1_location: Coord,
    pub y1_location: Coord,
    pub x2_location: Coord,
    pub y2_location: Coord,

    pub unique_id: Option<String>,
    pub enabled: bool,
    pub is_hidden: bool,
    pub underlying_string: Option<String>,
    pub converted_string: Option<String>,

    pub user_routed: bool,
    pub union_index: i32,
    pub is_free_primitive: bool,
    pub is_electrical_prim: bool,
    pub is_pre_route: bool,
    pub tear_drop: bool,
    pub polygon_outline: bool,
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
    pub paste_mask_expansion: Coord,
    pub allow_global_edit: bool,
    pub moveable: bool,
    pub is_redundant: bool,

    pub bold: bool,
    pub italic: bool,
    pub mirrored: bool,
    pub is_comment: bool,
    pub is_designator: bool,
    /// "Frame" option (text record offset 230).
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_frame: bool,
    /// "Offset border" option (text record offset 231).
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_offset_border: bool,
    /// Whether Altium honours [`Self::justification`] (record offset 240).
    /// Altium stores `location` as the bottom-left of the text's bounding
    /// box regardless; the justification only governs how the text
    /// re-anchors when its string changes (e.g. `.Designator` resolving).
    #[cfg_attr(feature = "serde", serde(default))]
    pub justification_valid: bool,
    pub inverted: bool,
    pub inverted_tt_text_border: Coord,
    pub inv_rect_height: Coord,
    pub inv_rect_width: Coord,

    pub advance_snapping: bool,
    pub border_space_type: i32,
    pub can_edit_multiline_rect_size: bool,
    pub char_set: i32,
    pub disable_special_string_conversion: bool,

    /// Index into the document's wide-string table (-1 if ASCII only).
    pub wide_string_index: i32,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            text: String::new(),
            location: CoordPoint::default(),
            height: Coord::ZERO,
            stroke_width: Coord::ZERO,
            rotation: 0.0,
            layer: 1,
            net_index: 0,
            component_index: -1,
            is_mirrored: false,
            justification: TextJustification::BottomLeft,
            is_truetype: false,
            font_name: None,
            text_kind: PcbTextKind::Stroke,
            stroke_font: PcbStrokeFont::Default,
            font_bold: false,
            font_italic: false,
            is_inverted: false,
            use_inverted_rectangle: false,
            inverted_border: Coord::ZERO,
            inverted_rect_width: Coord::ZERO,
            inverted_rect_height: Coord::ZERO,
            inverted_rect_justification: TextJustification::BottomLeft,
            inverted_rect_text_offset: Coord::ZERO,
            barcode_lr_margin: Coord::ZERO,
            barcode_tb_margin: Coord::ZERO,
            bar_code_kind: 0,
            bar_code_bit_pattern: None,
            // Barcode block defaults mirror the bytes Altium writes for
            // ordinary (non-barcode) text records.
            bar_code_full_height: Coord::from_raw(2_100_000),
            bar_code_full_width: Coord::from_raw(10_500_000),
            bar_code_min_width: Coord::ZERO,
            bar_code_show_text: true,
            bar_code_inverted: true,
            bar_code_render_mode: 1,
            bar_code_font_name: Some("Arial".into()),
            bar_code_x_margin: Coord::from_raw(200_000),
            bar_code_y_margin: Coord::from_raw(200_000),
            font_id: 0,
            use_tt_fonts: false,
            multi_line: false,
            word_wrap: false,
            mirror_flag: false,
            is_locked: false,
            is_keepout: false,
            flags_extra: default_flags_extra(),
            layer_v7: 0,
            size: Coord::ZERO,
            width: Coord::ZERO,
            multiline_text_height: Coord::ZERO,
            multiline_text_width: Coord::ZERO,
            multiline_text_resize_enabled: false,
            multiline_text_auto_position: 0,
            ttf_text_height: Coord::ZERO,
            ttf_text_width: Coord::ZERO,
            ttf_inverted_text_justify: 0,
            ttf_offset_from_inverted_rect: Coord::ZERO,
            snap_point_x: Coord::ZERO,
            snap_point_y: Coord::ZERO,
            x1_location: Coord::ZERO,
            y1_location: Coord::ZERO,
            x2_location: Coord::ZERO,
            y2_location: Coord::ZERO,
            unique_id: None,
            enabled: true,
            is_hidden: false,
            underlying_string: None,
            converted_string: None,
            user_routed: false,
            union_index: 0,
            is_free_primitive: false,
            is_electrical_prim: false,
            is_pre_route: false,
            tear_drop: false,
            polygon_outline: false,
            is_tenting: false,
            is_tenting_top: false,
            is_tenting_bottom: false,
            is_testpoint_top: false,
            is_testpoint_bottom: false,
            is_assy_testpoint_top: false,
            is_assy_testpoint_bottom: false,
            power_plane_clearance: Coord::ZERO,
            power_plane_connect_style: 0,
            power_plane_relief_expansion: Coord::ZERO,
            relief_air_gap: Coord::ZERO,
            relief_conductor_width: Coord::ZERO,
            relief_entries: 0,
            solder_mask_expansion: Coord::ZERO,
            paste_mask_expansion: Coord::ZERO,
            allow_global_edit: false,
            moveable: false,
            is_redundant: false,
            bold: false,
            italic: false,
            mirrored: false,
            is_comment: false,
            is_designator: false,
            is_frame: false,
            is_offset_border: false,
            justification_valid: false,
            inverted: false,
            inverted_tt_text_border: Coord::ZERO,
            inv_rect_height: Coord::ZERO,
            inv_rect_width: Coord::ZERO,
            advance_snapping: false,
            border_space_type: 0,
            can_edit_multiline_rect_size: false,
            char_set: 0,
            disable_special_string_conversion: false,
            wide_string_index: -1,
        }
    }
}

impl Text {
    pub fn bounds(&self) -> CoordRect {
        // Approximate width based on text length and height.
        let approx_width = Coord::from_raw(
            (self.height.to_raw() as f64 * self.text.chars().count() as f64 * 0.6) as i32,
        );
        CoordRect::from_xyxy(
            self.location.x,
            self.location.y,
            self.location.x + approx_width,
            self.location.y + self.height,
        )
    }
}

/// 3D component body — outline + reference to a STEP model in the library.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ComponentBody {
    pub outline: Vec<CoordPoint>,
    pub layer_name: String,
    pub layer: i32,
    /// Index into the document's `Nets6` table; `0` means "no net".
    pub net_index: u16,
    /// Index into the document's `Components6` table; `-1` for free bodies.
    pub component_index: i32,
    pub name: Option<String>,
    pub kind: i32,
    pub is_shape_based: bool,

    pub standoff_height: Coord,
    pub overall_height: Coord,
    pub body_color_3d: i32,
    pub body_opacity_3d: f64,

    pub model_id: Option<String>,
    pub model_embed: bool,
    pub model_type: i32,
    pub model_2d_location: CoordPoint,
    pub model_2d_rotation: f64,
    pub model_3d_rot_x: f64,
    pub model_3d_rot_y: f64,
    pub model_3d_rot_z: f64,
    pub model_3d_dz: Coord,
    pub model_name: Option<String>,
    pub model_checksum: i32,
    pub model_source: Option<String>,
    #[cfg_attr(
        feature = "serde",
        serde(default, with = "crate::serde_bytes::b64_opt")
    )]
    pub step_model_data: Option<Vec<u8>>,

    pub enabled: bool,
    pub is_keepout: bool,
    /// Unmodelled flag bits from the record header, preserved verbatim.
    #[cfg_attr(feature = "serde", serde(default = "default_flags_extra"))]
    pub flags_extra: u16,
    pub cavity_height: Coord,
    pub unique_id: Option<String>,
    pub user_routed: bool,
    pub union_index: i32,

    pub is_free_primitive: bool,
    pub is_electrical_prim: bool,
    pub is_pre_route: bool,
    pub tear_drop: bool,
    pub polygon_outline: bool,
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
    pub paste_mask_expansion: Coord,
    pub is_simple_region: bool,
    pub is_hidden: bool,
    pub is_locked: bool,
    pub allow_global_edit: bool,
    pub moveable: bool,
    pub override_color: bool,
    pub model_has_changed: bool,
    pub body_projection: i32,
    pub texture: Option<String>,
    pub texture_center: i32,
    pub texture_rotation: i32,
    pub texture_size: Coord,
    pub hole_count: i32,
    pub axis_count: i32,
    pub area: i64,
    pub sub_poly_index: i32,
    pub arc_resolution: f64,
    pub identifier: Option<String>,
    pub additional_parameters: Option<BTreeMap<String, String>>,
}

impl Default for ComponentBody {
    fn default() -> Self {
        Self {
            outline: Vec::new(),
            layer_name: "MECHANICAL1".to_string(),
            layer: 57,
            net_index: 0,
            component_index: -1,
            name: None,
            kind: 0,
            is_shape_based: false,
            standoff_height: Coord::ZERO,
            overall_height: Coord::ZERO,
            body_color_3d: 0xE0_E0E0,
            body_opacity_3d: 1.0,
            model_id: None,
            model_embed: true,
            model_type: 1,
            model_2d_location: CoordPoint::default(),
            model_2d_rotation: 0.0,
            model_3d_rot_x: 0.0,
            model_3d_rot_y: 0.0,
            model_3d_rot_z: 0.0,
            model_3d_dz: Coord::ZERO,
            model_name: None,
            model_checksum: 0,
            // "Undefined" is the placeholder Altium emits when no STEP model is
            // attached; leaving this as None makes the editor treat the body as
            // shape-only and skip 3D rendering.
            model_source: Some("Undefined".into()),
            step_model_data: None,
            enabled: true,
            is_keepout: false,
            flags_extra: default_flags_extra(),
            cavity_height: Coord::ZERO,
            unique_id: None,
            user_routed: false,
            union_index: 0,
            is_free_primitive: false,
            is_electrical_prim: false,
            is_pre_route: false,
            tear_drop: false,
            polygon_outline: false,
            is_tenting: false,
            is_tenting_top: false,
            is_tenting_bottom: false,
            is_testpoint_top: false,
            is_testpoint_bottom: false,
            is_assy_testpoint_top: false,
            is_assy_testpoint_bottom: false,
            power_plane_clearance: Coord::ZERO,
            power_plane_connect_style: 0,
            power_plane_relief_expansion: Coord::ZERO,
            relief_air_gap: Coord::ZERO,
            relief_conductor_width: Coord::ZERO,
            relief_entries: 0,
            solder_mask_expansion: Coord::ZERO,
            paste_mask_expansion: Coord::ZERO,
            is_simple_region: false,
            is_hidden: false,
            is_locked: false,
            allow_global_edit: false,
            moveable: false,
            override_color: false,
            model_has_changed: false,
            body_projection: 0,
            texture: None,
            texture_center: 0,
            texture_rotation: 0,
            texture_size: Coord::ZERO,
            hole_count: 0,
            axis_count: 0,
            area: 0,
            sub_poly_index: 0,
            // Real Altium files always have arc_resolution=1.0; leaving 0.0
            // makes curved bodies tessellate to a single line segment.
            arc_resolution: 1.0,
            identifier: None,
            additional_parameters: None,
        }
    }
}

impl ComponentBody {
    pub fn bounds(&self) -> CoordRect {
        if self.outline.is_empty() {
            return CoordRect::EMPTY;
        }
        let mut acc = CoordRect::EMPTY;
        for pt in &self.outline {
            acc = acc.union_point(*pt);
        }
        acc
    }
}

/// A named electrical net.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Net {
    pub name: String,
    /// Raw `Nets6` record parameters (color, visibility, per-layer routing
    /// widths, …), preserved verbatim for write-back.
    #[cfg_attr(feature = "serde", serde(default))]
    pub parameters: Vec<(String, String)>,
}
