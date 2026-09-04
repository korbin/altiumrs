//! Schematic primitives.

#![allow(clippy::too_many_arguments)]

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::coord::{Coord, CoordPoint, CoordRect};
use crate::enums::{PinElectricalType, PinOrientation, PowerPortStyle, TextJustification};

/// Common ownership and visibility fields present on (almost) every schematic
/// primitive record.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PrimitiveCommon {
    pub owner_index: i32,
    pub is_not_accessible: bool,
    pub index_in_sheet: i32,
    pub owner_part_id: i32,
    pub owner_part_display_mode: i32,
    pub graphically_locked: bool,
    pub disabled: bool,
    pub dimmed: bool,
    pub unique_id: Option<String>,
}

macro_rules! sch_struct {
    // Auto-derive Default — the standard form.
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $($field_vis:vis $field:ident: $ty:ty),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq)]
        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
        $vis struct $name {
            $($field_vis $field: $ty,)*
            pub common: PrimitiveCommon,
        }
    };

    // Custom-default form. The `default { ... }` clause overrides specific
    // field values; everything not listed falls back to `Default::default()`
    // for that field's type.
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $($field_vis:vis $field:ident: $ty:ty),* $(,)?
        }
        default {
            $($df:ident: $dv:expr),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
        $vis struct $name {
            $($field_vis $field: $ty,)*
            pub common: PrimitiveCommon,
        }

        impl Default for $name {
            fn default() -> Self {
                // Build with each field's type-default, then override the
                // entries listed in `default { … }`. Using assignment instead
                // of struct-update syntax avoids Rust's "field specified more
                // than once" error when an override targets a field that is
                // also in the per-field default list.
                #[allow(clippy::field_reassign_with_default)]
                let mut __value = Self {
                    $($field: <$ty as Default>::default(),)*
                    common: PrimitiveCommon::default(),
                };
                $(__value.$df = $dv;)*
                __value
            }
        }
    };
}

sch_struct! {
    /// A schematic pin (component port).
    pub struct Pin {
        pub name: Option<String>,
        pub designator: Option<String>,
        pub location: CoordPoint,
        pub length: Coord,
        pub electrical_type: PinElectricalType,
        pub orientation: PinOrientation,
        pub show_name: bool,
        pub show_designator: bool,
        pub description: Option<String>,
        pub formal_type: i32,
        pub symbol_inner_edge: i32,
        pub symbol_outer_edge: i32,
        pub symbol_inside: i32,
        pub symbol_outside: i32,
        pub symbol_line_width: i32,
        pub swap_id_part: Option<String>,
        pub pin_propagation_delay: i32,
        pub designator_custom_font_id: i32,
        pub name_custom_font_id: i32,
        pub width: i32,
        pub color: i32,
        pub area_color: i32,
        pub default_value: Option<String>,
        pub is_hidden: bool,
        pub designator_custom_color: i32,
        pub designator_custom_position_margin: i32,
        pub designator_custom_position_rotation_anchor: i32,
        pub designator_custom_position_rotation_relative: bool,
        pub designator_font_mode: i32,
        pub designator_position_mode: i32,
        pub name_custom_color: i32,
        pub name_custom_position_margin: i32,
        pub name_custom_position_rotation_anchor: i32,
        pub name_custom_position_rotation_relative: bool,
        pub name_font_mode: i32,
        pub name_position_mode: i32,
        pub swap_id_pair: Option<String>,
        pub swap_id_part_pin: Option<String>,
        pub swap_id_pin: Option<String>,
        pub hidden_net_name: Option<String>,
        pub pin_package_length: Coord,
    }
}

impl Pin {
    pub fn bounds(&self) -> CoordRect {
        let end = match self.orientation {
            PinOrientation::Right => {
                CoordPoint::new(self.location.x + self.length, self.location.y)
            }
            PinOrientation::Left => CoordPoint::new(self.location.x - self.length, self.location.y),
            PinOrientation::Up => CoordPoint::new(self.location.x, self.location.y + self.length),
            PinOrientation::Down => CoordPoint::new(self.location.x, self.location.y - self.length),
        };
        CoordRect::from_corners(self.location, end)
    }
}

sch_struct! {
    /// Straight line segment.
    pub struct Line {
        pub start: CoordPoint,
        pub end: CoordPoint,
        pub width: Coord,
        pub color: i32,
        pub line_style: i32,
        pub area_color: i32,
    }
}

sch_struct! {
    /// Filled / outlined rectangle.
    pub struct Rectangle {
        pub corner1: CoordPoint,
        pub corner2: CoordPoint,
        pub line_width: Coord,
        pub line_style: i32,
        pub is_filled: bool,
        pub is_transparent: bool,
        pub color: i32,
        pub fill_color: i32,
    }
    default {
        // Body-shape defaults: dark border + light yellow fill, matching the
        // values Altium emits for fresh symbol bodies. `line_width` of 1 mil
        // corresponds to the editor's "Smallest" line-width setting.
        color: 128,
        fill_color: 11_599_871,
        line_width: Coord::from_mils(1.0),
    }
}

sch_struct! {
    /// Rectangle with rounded corners.
    pub struct RoundedRectangle {
        pub corner1: CoordPoint,
        pub corner2: CoordPoint,
        pub corner_radius_x: Coord,
        pub corner_radius_y: Coord,
        pub line_width: i32,
        pub line_style: i32,
        pub color: i32,
        pub fill_color: i32,
        pub is_filled: bool,
        pub is_transparent: bool,
    }
    default {
        color: 128,
        fill_color: 11_599_871,
    }
}

sch_struct! {
    /// Closed polygon.
    pub struct Polygon {
        pub vertices: Vec<CoordPoint>,
        pub line_width: i32,
        pub color: i32,
        pub fill_color: i32,
        pub is_filled: bool,
        pub is_transparent: bool,
    }
    default {
        color: 128,
        fill_color: 11_599_871,
    }
}

sch_struct! {
    /// Open polyline. Optional start/end shapes (arrows, circles, …).
    ///
    /// `line_style` is stored as a raw `i32` for the same reason as
    /// [`Wire::line_style`] — see that field's docs.
    pub struct Polyline {
        pub vertices: Vec<CoordPoint>,
        pub line_width: i32,
        pub color: i32,
        pub line_style: i32,
        pub start_line_shape: i32,
        pub end_line_shape: i32,
        pub line_shape_size: i32,
        pub area_color: i32,
        pub is_transparent: bool,
        pub is_solid: bool,
    }
}

sch_struct! {
    /// Ellipse / circle.
    pub struct Ellipse {
        pub center: CoordPoint,
        pub radius_x: Coord,
        pub radius_y: Coord,
        pub line_width: i32,
        pub color: i32,
        pub fill_color: i32,
        pub is_filled: bool,
        pub is_transparent: bool,
    }
    default {
        color: 128,
        fill_color: 11_599_871,
    }
}

sch_struct! {
    /// Filled arc segment.
    pub struct Pie {
        pub center: CoordPoint,
        pub radius: Coord,
        pub start_angle: f64,
        pub end_angle: f64,
        pub line_width: i32,
        pub color: i32,
        pub fill_color: i32,
        pub is_filled: bool,
        pub is_transparent: bool,
    }
}

sch_struct! {
    /// Cubic Bezier curve (control points in groups of four).
    pub struct Bezier {
        pub control_points: Vec<CoordPoint>,
        pub line_width: i32,
        pub color: i32,
        pub area_color: i32,
    }
}

sch_struct! {
    /// Circular arc.
    pub struct Arc {
        pub center: CoordPoint,
        pub radius: Coord,
        pub start_angle: f64,
        pub end_angle: f64,
        pub line_width: i32,
        pub color: i32,
        pub area_color: i32,
    }
}

sch_struct! {
    /// Elliptical arc with independent X/Y radii.
    pub struct EllipticalArc {
        pub center: CoordPoint,
        pub primary_radius: Coord,
        pub secondary_radius: Coord,
        pub start_angle: f64,
        pub end_angle: f64,
        pub line_width: Coord,
        pub color: i32,
        pub area_color: i32,
    }
}

sch_struct! {
    /// Wire segment (electrical net path).
    ///
    /// `line_style` is stored as a raw `i32` (rather than [`SchLineStyle`]) to
    /// preserve unknown values that real Altium files occasionally carry in
    /// this slot — converting through the typed enum would silently collapse
    /// any unrecognised int back to `Solid` on round-trip.
    pub struct Wire {
        pub vertices: Vec<CoordPoint>,
        pub line_width: i32,
        pub color: i32,
        pub line_style: i32,
        pub area_color: i32,
        pub is_solid: bool,
        pub is_transparent: bool,
        pub auto_wire: bool,
        pub underline_color: i32,
    }
}

sch_struct! {
    /// Bus segment (multi-signal path).
    pub struct Bus {
        pub vertices: Vec<CoordPoint>,
        pub line_width: i32,
        pub line_style: i32,
        pub color: i32,
        pub area_color: i32,
    }
}

sch_struct! {
    /// Two-segment bus entry (wire ↔ bus connector).
    pub struct BusEntry {
        pub location: CoordPoint,
        pub corner: CoordPoint,
        pub line_width: i32,
        pub color: i32,
    }
}

sch_struct! {
    /// Wire junction (connection dot).
    pub struct Junction {
        pub location: CoordPoint,
        pub size: Coord,
        pub color: i32,
        pub locked: bool,
    }
}

sch_struct! {
    /// Free-standing text label.
    pub struct Label {
        pub text: String,
        pub location: CoordPoint,
        pub font_id: i32,
        pub justification: TextJustification,
        pub rotation: f64,
        pub color: i32,
        pub is_mirrored: bool,
        pub is_hidden: bool,
        pub area_color: i32,
    }
}

sch_struct! {
    /// Net label (names a wire / bus).
    pub struct NetLabel {
        pub location: CoordPoint,
        pub text: String,
        pub orientation: i32,
        pub justification: TextJustification,
        pub font_id: i32,
        pub color: i32,
        pub is_mirrored: bool,
        pub area_color: i32,
    }
}

sch_struct! {
    /// Component / sheet parameter.
    pub struct Parameter {
        pub name: String,
        pub value: String,
        pub location: CoordPoint,
        pub orientation: i32,
        pub justification: TextJustification,
        pub font_id: i32,
        pub color: i32,
        pub is_visible: bool,
        pub hide_name: bool,
        pub param_type: i32,
        pub show_name: bool,
        pub is_mirrored: bool,
        pub is_read_only: bool,
        pub description: Option<String>,
        pub area_color: i32,
        pub auto_position: i32,
        pub is_configurable: bool,
        pub is_rule: bool,
        pub is_system_parameter: bool,
        pub text_horz_anchor: i32,
        pub text_vert_anchor: i32,
        pub text_is_utf8: bool,
        pub allow_database_synchronize: bool,
        pub allow_library_synchronize: bool,
        pub name_is_read_only: bool,
        pub physical_designator: Option<String>,
        pub value_is_read_only: bool,
        pub variant_option: Option<String>,
    }
}

sch_struct! {
    /// Parameter set (collection of directive parameters).
    pub struct ParameterSet {
        pub parameters: Vec<Parameter>,
        pub location: CoordPoint,
        pub orientation: i32,
        pub style: i32,
        pub color: i32,
        pub area_color: i32,
        pub name: Option<String>,
        pub show_hidden_fields: bool,
        pub border_width: i32,
        pub is_solid: bool,
    }
}

sch_struct! {
    /// Multi-line text frame.
    pub struct TextFrame {
        pub corner1: CoordPoint,
        pub corner2: CoordPoint,
        pub text: String,
        pub orientation: i32,
        pub alignment: TextJustification,
        pub font_id: i32,
        pub text_color: i32,
        pub border_color: i32,
        pub fill_color: i32,
        pub show_border: bool,
        pub is_filled: bool,
        pub is_transparent: bool,
        pub word_wrap: bool,
        pub clip_to_rect: bool,
        pub line_width: i32,
        pub line_style: i32,
        pub text_margin: i32,
    }
}

sch_struct! {
    /// Embedded raster image.
    pub struct Image {
        pub corner1: CoordPoint,
        pub corner2: CoordPoint,
        pub keep_aspect: bool,
        pub embed_image: bool,
        pub filename: Option<String>,
        pub image_data: Option<Vec<u8>>,
        pub border_color: i32,
        pub show_border: bool,
        pub line_width: i32,
        pub area_color: i32,
        pub is_solid: bool,
        pub line_style: i32,
        pub is_transparent: bool,
    }
}

sch_struct! {
    /// Reference to a built-in symbol (GND, VCC, etc.).
    pub struct Symbol {
        pub location: CoordPoint,
        pub symbol_type: i32,
        pub is_mirrored: bool,
        pub orientation: i32,
        pub line_width: i32,
        pub scale_factor: i32,
        pub color: i32,
    }
}

sch_struct! {
    /// Power port (VCC/GND/...).
    pub struct PowerObject {
        pub location: CoordPoint,
        pub text: String,
        pub style: PowerPortStyle,
        pub rotation: f64,
        pub show_net_name: bool,
        pub is_cross_sheet_connector: bool,
        pub color: i32,
        pub font_id: i32,
        pub area_color: i32,
        pub is_custom_style: bool,
        pub is_mirrored: bool,
        pub justification: i32,
    }
}

sch_struct! {
    /// "No ERC" suppression marker.
    pub struct NoErc {
        pub location: CoordPoint,
        pub orientation: i32,
        pub color: i32,
        pub is_active: bool,
        pub symbol: i32,
        pub area_color: i32,
        pub suppress_all: bool,
        pub error_kind_set_to_suppress: Option<String>,
    }
}

sch_struct! {
    /// Sheet-level connection point.
    pub struct Port {
        pub location: CoordPoint,
        pub name: String,
        pub io_type: i32,
        pub style: i32,
        pub alignment: i32,
        pub width: Coord,
        pub height: Coord,
        pub border_width: i32,
        pub auto_size: bool,
        pub connected_end: i32,
        pub cross_reference: Option<String>,
        pub show_net_name: bool,
        pub harness_type: Option<String>,
        pub harness_color: i32,
        pub is_custom_style: bool,
        pub font_id: i32,
        pub color: i32,
        pub area_color: i32,
        pub text_color: i32,
    }
}

sch_struct! {
    /// Sheet symbol (reference to a sub-sheet in hierarchical designs).
    pub struct SheetSymbol {
        pub entries: Vec<SheetEntry>,
        pub location: CoordPoint,
        pub x_size: Coord,
        pub y_size: Coord,
        pub is_mirrored: bool,
        pub file_name: Option<String>,
        pub sheet_name: Option<String>,
        pub line_width: i32,
        pub color: i32,
        pub area_color: i32,
        pub is_solid: bool,
        pub show_hidden_fields: bool,
        pub symbol_type: i32,
        pub design_item_id: Option<String>,
        pub item_guid: Option<String>,
        pub lib_identifier_kind: i32,
        pub library_identifier: Option<String>,
        pub revision_guid: Option<String>,
        pub source_library_name: Option<String>,
        pub vault_guid: Option<String>,
    }
}

sch_struct! {
    /// Connection point on a [`SheetSymbol`].
    pub struct SheetEntry {
        pub side: i32,
        pub distance_from_top: Coord,
        pub name: String,
        pub io_type: i32,
        pub style: i32,
        pub arrow_kind: i32,
        pub harness_type: Option<String>,
        pub harness_color: i32,
        pub font_id: i32,
        pub color: i32,
        pub area_color: i32,
        pub text_color: i32,
        pub text_style: i32,
    }
}

sch_struct! {
    /// "Blanket": a region that applies a directive to all enclosed nets.
    pub struct Blanket {
        pub vertices: Vec<CoordPoint>,
        pub parameters: Vec<Parameter>,
        pub is_collapsed: bool,
        pub line_width: i32,
        pub line_style: i32,
        pub is_solid: bool,
        pub is_transparent: bool,
        pub color: i32,
        pub area_color: i32,
    }
}

sch_struct! {
    /// Harness connector (record 215): fans a signal harness out into its
    /// member nets. `location` is the top-left corner; entries hang off the
    /// left/right edge `distance_from_top` below it, and the bundle attaches
    /// at `primary_connection_position` down the `side` edge.
    pub struct HarnessConnector {
        pub entries: Vec<HarnessEntry>,
        pub harness_type: Option<HarnessType>,
        pub location: CoordPoint,
        pub x_size: Coord,
        pub y_size: Coord,
        pub primary_connection_position: Coord,
        // 0 = left, 1 = right, 2 = top, 3 = bottom.
        pub side: i32,
        pub line_width: i32,
        pub color: i32,
        pub area_color: i32,
    }
}

sch_struct! {
    /// One named signal on a [`HarnessConnector`] (record 216).
    pub struct HarnessEntry {
        // 0 = left, 1 = right.
        pub side: i32,
        // Distance below the connector's top edge (already scaled from
        // Altium's 100-mil slot count to a coordinate).
        pub distance_from_top: Coord,
        pub name: String,
        pub color: i32,
        pub area_color: i32,
        pub text_color: i32,
        pub text_font_id: i32,
        pub text_style: Option<String>,
        pub owner_index_additional_list: bool,
    }
}

sch_struct! {
    /// Harness-type text on a [`HarnessConnector`] (record 217).
    pub struct HarnessType {
        pub location: CoordPoint,
        pub text: String,
        pub color: i32,
        pub font_id: i32,
        pub is_hidden: bool,
        pub not_auto_position: bool,
        pub owner_index_additional_list: bool,
    }
}

sch_struct! {
    /// Signal harness polyline (record 218): carries a bundle of nets
    /// between ports, sheet entries and harness connectors.
    pub struct SignalHarness {
        pub vertices: Vec<CoordPoint>,
        pub color: i32,
        pub line_width: i32,
    }
}
