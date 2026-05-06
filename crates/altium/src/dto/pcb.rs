//! PCB record DTOs.

use altium_derive::AltiumRecord;

use crate::Coord;

/// Arc record (PCB).
#[derive(Debug, Clone, Default, PartialEq, AltiumRecord)]
#[altium(record = "Arc")]
pub struct ArcDto {
    #[altium(name = "X")]
    pub center_x: i32,
    #[altium(name = "Y")]
    pub center_y: i32,
    #[altium(name = "RADIUS")]
    pub radius: i32,
    #[altium(name = "STARTANGLE")]
    pub start_angle: f64,
    #[altium(name = "ENDANGLE")]
    pub end_angle: f64,
    #[altium(name = "WIDTH")]
    pub width: i32,
    #[altium(name = "LAYER")]
    pub layer: i32,
    #[altium(name = "NET")]
    pub net: Option<String>,
    #[altium(name = "COMPONENT")]
    pub component_index: i32,
    #[altium(name = "FLAGS")]
    pub flags: i32,
    #[altium(name = "KEEPOUT")]
    pub is_keepout: bool,
    #[altium(name = "UNIQUEID")]
    pub unique_id: Option<String>,
    #[altium(name = "USERROUTED")]
    pub user_routed: bool,
    #[altium(name = "POLYGONINDEX")]
    pub polygon_index: i32,
    #[altium(name = "SUBPOLYINDEX")]
    pub sub_polygon_index: i32,
}

/// Footprint / component header record (PCB).
#[derive(Debug, Clone, Default, PartialEq, AltiumRecord)]
#[altium(record = "Component")]
pub struct ComponentDto {
    #[altium(name = "PATTERN")]
    pub pattern: Option<String>,
    #[altium(name = "DESCRIPTION")]
    pub description: Option<String>,
    #[altium(name = "HEIGHT")]
    pub height: i32,
    #[altium(name = "ITEMGUID")]
    pub item_guid: Option<String>,
    #[altium(name = "REVISIONGUID")]
    pub revision_guid: Option<String>,
    #[altium(name = "SOURCECOMPONENTLIBRARY")]
    pub source_component_library: Option<String>,
    #[altium(name = "SOURCELIBRARYREFERENCE")]
    pub source_library_reference: Option<String>,
    #[altium(name = "X")]
    pub location_x: i32,
    #[altium(name = "Y")]
    pub location_y: i32,
    #[altium(name = "ROTATION")]
    pub rotation: f64,
    #[altium(name = "LAYER")]
    pub layer: i32,
    #[altium(name = "DESIGNATOR")]
    pub designator: Option<String>,
    #[altium(name = "COMMENT")]
    pub comment: Option<String>,
    #[altium(name = "LOCKED")]
    pub is_locked: bool,
    #[altium(name = "MIRRORED")]
    pub is_mirrored: bool,
    #[altium(name = "UNIQUEID")]
    pub unique_id: Option<String>,
}

/// Solid fill record (PCB).
#[derive(Debug, Clone, Default, PartialEq, AltiumRecord)]
#[altium(record = "Fill")]
pub struct FillDto {
    #[altium(name = "X1")]
    pub corner1_x: i32,
    #[altium(name = "Y1")]
    pub corner1_y: i32,
    #[altium(name = "X2")]
    pub corner2_x: i32,
    #[altium(name = "Y2")]
    pub corner2_y: i32,
    #[altium(name = "ROTATION")]
    pub rotation: f64,
    #[altium(name = "LAYER")]
    pub layer: i32,
    #[altium(name = "NET")]
    pub net: Option<String>,
    #[altium(name = "COMPONENT")]
    pub component_index: i32,
    #[altium(name = "FLAGS")]
    pub flags: i32,
    #[altium(name = "KEEPOUT")]
    pub is_keepout: bool,
    #[altium(name = "UNIQUEID")]
    pub unique_id: Option<String>,
    #[altium(name = "USERROUTED")]
    pub user_routed: bool,
    #[altium(name = "POLYGONINDEX")]
    pub polygon_index: i32,
}

/// Pad record (PCB): flat fields. Per-layer arrays live on the model type.
#[derive(Debug, Clone, Default, PartialEq, AltiumRecord)]
#[altium(record = "Pad")]
pub struct PadDto {
    #[altium(name = "NAME")]
    pub designator: Option<String>,
    #[altium(name = "X")]
    pub location_x: i32,
    #[altium(name = "Y")]
    pub location_y: i32,
    #[altium(name = "XSIZE")]
    pub size_top_x: i32,
    #[altium(name = "YSIZE")]
    pub size_top_y: i32,
    #[altium(name = "XMIDSIZE")]
    pub size_middle_x: i32,
    #[altium(name = "YMIDSIZE")]
    pub size_middle_y: i32,
    #[altium(name = "XBOTSIZE")]
    pub size_bottom_x: i32,
    #[altium(name = "YBOTSIZE")]
    pub size_bottom_y: i32,
    #[altium(name = "HOLESIZE")]
    pub hole_size: i32,
    #[altium(name = "SHAPE")]
    pub shape_top: i32,
    #[altium(name = "MIDSHAPE")]
    pub shape_middle: i32,
    #[altium(name = "BOTSHAPE")]
    pub shape_bottom: i32,
    #[altium(name = "HOLESHAPE")]
    pub hole_shape: i32,
    #[altium(name = "LAYER")]
    pub layer: i32,
    #[altium(name = "ROTATION")]
    pub rotation: f64,
    #[altium(name = "HOLEROTATION")]
    pub hole_rotation: f64,
    #[altium(name = "PLATED")]
    pub is_plated: bool,
    #[altium(name = "NET")]
    pub net: Option<String>,
    #[altium(name = "JUMPERID")]
    pub jumper_id: i32,
    #[altium(name = "XOFFSET")]
    pub offset_from_hole_center_x: i32,
    #[altium(name = "YOFFSET")]
    pub offset_from_hole_center_y: i32,
    #[altium(name = "HOLESLOTLENGTH")]
    pub hole_slot_length: i32,
    #[altium(name = "CORNERRADIUS")]
    pub corner_radius_top: i32,
    #[altium(name = "MIDCORNERRADIUS")]
    pub corner_radius_middle: i32,
    #[altium(name = "BOTCORNERRADIUS")]
    pub corner_radius_bottom: i32,
    #[altium(name = "STACKMODE")]
    pub stack_mode: i32,
    #[altium(name = "PASTEMASKEXPANSIONMODE")]
    pub paste_mask_expansion_mode: i32,
    #[altium(name = "PASTEMASKEXPANSION")]
    pub paste_mask_expansion: i32,
    #[altium(name = "SOLDERMASKEXPANSIONMODE")]
    pub solder_mask_expansion_mode: i32,
    #[altium(name = "SOLDERMASKEXPANSION")]
    pub solder_mask_expansion: i32,
    #[altium(name = "FLAGS")]
    pub flags: i32,
    #[altium(name = "UNIQUEID")]
    pub unique_id: Option<String>,
    #[altium(name = "USERROUTED")]
    pub user_routed: bool,
}

/// Polygon record (PCB).
#[derive(Debug, Clone, Default, PartialEq, AltiumRecord)]
#[altium(record = "Polygon")]
pub struct PolygonDto {
    #[altium(name = "NAME")]
    pub name: Option<String>,
    #[altium(name = "LAYER")]
    pub layer: i32,
    #[altium(name = "NET")]
    pub net: Option<String>,
    #[altium(name = "POURORDER")]
    pub pour_order: i32,
    #[altium(name = "GRIDSIZE")]
    pub grid_size: i32,
    #[altium(name = "TRACKWIDTH")]
    pub track_width: i32,
    #[altium(name = "MINPRIMLENGTH")]
    pub min_primitive_length: i32,
    #[altium(name = "USEOCTAGONS")]
    pub use_octagons: bool,
    #[altium(name = "HATCHSTYLE")]
    pub hatch_style: i32,
    #[altium(name = "POURMODE")]
    pub pour_mode: i32,
    #[altium(name = "REMOVEDEAD")]
    pub remove_dead_copper: bool,
    #[altium(name = "REMOVENECKS")]
    pub remove_narrow_necks: bool,
    #[altium(name = "NECKWIDTH")]
    pub neck_width: i32,
    #[altium(name = "ARCAPPROXIMATION")]
    pub arc_approximation: i32,
    #[altium(name = "POUROVERSAMENETPOLYGONS")]
    pub pour_over_same_net_polygons: bool,
    #[altium(name = "NV")]
    pub vertex_count: i32,
    #[altium(name = "FLAGS")]
    pub flags: i32,
    #[altium(name = "UNIQUEID")]
    pub unique_id: Option<String>,
    #[altium(name = "LOCKED")]
    pub is_locked: bool,
    #[altium(name = "SHELVED")]
    pub is_shelved: bool,
    #[altium(name = "AVOIDOBST")]
    pub avoid_obstacles: bool,
    #[altium(name = "POLYGONTYPE")]
    pub polygon_type: i32,
}

/// Polygon vertex record.
#[derive(Debug, Clone, Default, PartialEq, AltiumRecord)]
#[altium(record = "PolygonVertex")]
pub struct PolygonVertexDto {
    #[altium(name = "VX")]
    pub x: i32,
    #[altium(name = "VY")]
    pub y: i32,
    #[altium(name = "KIND")]
    pub kind: i32,
    #[altium(name = "CX")]
    pub arc_center_x: i32,
    #[altium(name = "CY")]
    pub arc_center_y: i32,
    #[altium(name = "SA")]
    pub start_angle: f64,
    #[altium(name = "EA")]
    pub end_angle: f64,
    #[altium(name = "R")]
    pub radius: i32,
}

/// Text record (PCB).
#[derive(Debug, Clone, Default, PartialEq, AltiumRecord)]
#[altium(record = "Text")]
pub struct TextDto {
    #[altium(name = "X")]
    pub location_x: i32,
    #[altium(name = "Y")]
    pub location_y: i32,
    #[altium(name = "TEXT")]
    pub text: Option<String>,
    #[altium(name = "HEIGHT")]
    pub height: i32,
    #[altium(name = "WIDTH")]
    pub width: i32,
    #[altium(name = "ROTATION")]
    pub rotation: f64,
    #[altium(name = "LAYER")]
    pub layer: i32,
    #[altium(name = "MIRRORED")]
    pub is_mirrored: bool,
    #[altium(name = "TEXTKIND")]
    pub text_kind: i32,
    #[altium(name = "STROKEFONT")]
    pub stroke_font: i32,
    #[altium(name = "STROKEWIDTH")]
    pub stroke_width: i32,
    #[altium(name = "FONTNAME")]
    pub font_name: Option<String>,
    #[altium(name = "BOLD")]
    pub is_bold: bool,
    #[altium(name = "ITALIC")]
    pub is_italic: bool,
    #[altium(name = "INVERTED")]
    pub is_inverted: bool,
    #[altium(name = "INVERTEDBORDER")]
    pub inverted_border: i32,
    #[altium(name = "INVERTEDRECT")]
    pub inverted_rect: bool,
    #[altium(name = "INVERTEDRECTWIDTH")]
    pub inverted_rect_width: i32,
    #[altium(name = "INVERTEDRECTHEIGHT")]
    pub inverted_rect_height: i32,
    #[altium(name = "INVERTEDRECTJUSTIFICATION")]
    pub inverted_rect_justification: i32,
    #[altium(name = "INVERTEDRECTTEXTOFFSET")]
    pub inverted_rect_text_offset: i32,
    #[altium(name = "BARCODELRMARGIN")]
    pub barcode_lr_margin: i32,
    #[altium(name = "BARCODETBMARGIN")]
    pub barcode_tb_margin: i32,
    #[altium(name = "COMPONENT")]
    pub component_index: i32,
    #[altium(name = "FLAGS")]
    pub flags: i32,
    #[altium(name = "UNIQUEID")]
    pub unique_id: Option<String>,
    #[altium(name = "HIDDEN")]
    pub is_hidden: bool,
    #[altium(name = "DESIGNATORTYPE")]
    pub designator_type: i32,
}

/// Track / trace record.
#[derive(Debug, Clone, Default, PartialEq, AltiumRecord)]
#[altium(record = "Track")]
pub struct TrackDto {
    #[altium(name = "X1")]
    pub start_x: i32,
    #[altium(name = "Y1")]
    pub start_y: i32,
    #[altium(name = "X2")]
    pub end_x: i32,
    #[altium(name = "Y2")]
    pub end_y: i32,
    #[altium(name = "WIDTH")]
    pub width: i32,
    #[altium(name = "LAYER")]
    pub layer: i32,
    #[altium(name = "NET")]
    pub net: Option<String>,
    #[altium(name = "COMPONENT")]
    pub component_index: i32,
    #[altium(name = "FLAGS")]
    pub flags: i32,
    #[altium(name = "KEEPOUT")]
    pub is_keepout: bool,
    #[altium(name = "UNIQUEID")]
    pub unique_id: Option<String>,
    #[altium(name = "USERROUTED")]
    pub user_routed: bool,
    #[altium(name = "POLYGONINDEX")]
    pub polygon_index: i32,
    #[altium(name = "SUBPOLYINDEX")]
    pub sub_polygon_index: i32,
}

/// Via record.
#[derive(Debug, Clone, Default, PartialEq, AltiumRecord)]
#[altium(record = "Via")]
pub struct ViaDto {
    #[altium(name = "X")]
    pub location_x: i32,
    #[altium(name = "Y")]
    pub location_y: i32,
    #[altium(name = "HOLESIZE")]
    pub hole_size: i32,
    #[altium(name = "SIZE")]
    pub diameter: i32,
    #[altium(name = "DIAMETER_TOPLAYER")]
    pub diameter_top: i32,
    #[altium(name = "DIAMETER_MIDLAYER")]
    pub diameter_middle: i32,
    #[altium(name = "DIAMETER_BOTTOMLAYER")]
    pub diameter_bottom: i32,
    #[altium(name = "STARTLAYER")]
    pub start_layer: i32,
    #[altium(name = "ENDLAYER")]
    pub end_layer: i32,
    #[altium(name = "LAYER")]
    pub layer: i32,
    #[altium(name = "NET")]
    pub net: Option<String>,
    #[altium(name = "THERMALRELIEFAIRGAPWIDTH")]
    pub thermal_relief_air_gap_width: i32,
    #[altium(name = "THERMALRELIEFCONDUCTORS")]
    pub thermal_relief_conductors: i32,
    #[altium(name = "THERMALRELIEFCONDUCTORSWIDTH")]
    pub thermal_relief_conductors_width: i32,
    #[altium(name = "SOLDERMASKEXPANSIONMODE")]
    pub solder_mask_expansion_mode: i32,
    #[altium(name = "SOLDERMASKEXPANSION")]
    pub solder_mask_expansion: i32,
    #[altium(name = "DIAMETERSTACK")]
    pub diameter_stack_mode: i32,
    #[altium(name = "FLAGS")]
    pub flags: i32,
    #[altium(name = "UNIQUEID")]
    pub unique_id: Option<String>,
    #[altium(name = "USERROUTED")]
    pub user_routed: bool,
    #[altium(name = "FREEVIA")]
    pub is_free_via: bool,
}

// Coord-typed re-export so callers don't have to import it from the crate
// root when working with these DTOs.
#[allow(dead_code)]
type _ImportCoord = Coord;
