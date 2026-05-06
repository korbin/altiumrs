//! Schematic record ↔ typed primitive codec.

#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::single_match)]

use super::binary::{coord_from_dxp_frac, coord_to_dxp_frac, line_width_from_index};
use super::component::Component;
use super::implementation::{Implementation, MapDefiner};
use super::primitives::{
    Arc, Bezier, Blanket, Bus, BusEntry, Ellipse, EllipticalArc, Image, Junction, Label, Line,
    NetLabel, NoErc, Parameter, ParameterSet, Pie, Pin, Polygon, Polyline, Port, PowerObject,
    PrimitiveCommon, Rectangle, RoundedRectangle, SheetEntry, SheetSymbol, Symbol, TextFrame, Wire,
};
use crate::coord::{Coord, CoordPoint};
use crate::dto::sch as dto;
use crate::enums::{PinElectricalType, PinOrientation, PowerPortStyle, TextJustification};
use crate::parameter::{ParameterMap, format_bool_long};

// Component (record 1)

pub fn component_apply_record(component: &mut Component, params: &ParameterMap) {
    let d = dto::ComponentDto::from_params(params);
    component.location = CoordPoint::new(
        coord_from_dxp_frac(d.location_x, d.location_x_frac),
        coord_from_dxp_frac(d.location_y, d.location_y_frac),
    );
    component.lib_reference = d.lib_reference.clone();
    component.description = d.component_description.clone();
    // PARTCOUNT in binary includes part 0 (common); user-facing count is N-1.
    component.part_count = (d.part_count - 1).max(0);
    component.display_mode_count = d.display_mode_count;
    component.display_mode = d.display_mode;
    component.orientation = d.orientation;
    component.current_part_id = d.current_part_id;
    component.show_hidden_pins = d.show_hidden_pins;
    component.library_path = d.library_path.clone();
    component.source_library_name = d.source_library_name.clone();
    component.sheet_part_file_name = d.sheet_part_file_name.clone();
    component.target_file_name = d.target_file_name.clone();
    component.area_color = d.area_color;
    component.color = d.color;
    component.override_colors = d.override_colors;
    component.designator_prefix = d.designator_prefix.clone();
    component.designator_locked = d.designator_locked;
    component.part_id_locked = d.part_id_locked;
    component.design_item_id = d.design_item_id.clone();
    component.component_kind = d.component_kind;
    component.alias_list = d.alias_list.clone();
    component.all_pin_count = d.all_pin_count;
    component.symbol_reference = if d.symbol_reference.is_empty() {
        None
    } else {
        Some(d.symbol_reference.clone())
    };
    component.database_library_name = d.database_library_name.clone();
    component.database_table_name = d.database_table_name.clone();
    component.library_identifier = d.library_identifier.clone();
    component.vault_guid = d.vault_guid.clone();
    component.item_guid = d.item_guid.clone();
    component.revision_guid = d.revision_guid.clone();
    component.pins_moveable = d.pins_moveable;
    component.pin_color = d.pin_color;
    component.show_hidden_fields = d.show_hidden_fields;
    component.not_used_b_table_name = d.not_used_b_table_name.clone();
    component.configuration_parameters = d.configuration_parameters.clone();
    component.configurator_name = d.configurator_name.clone();
    component.display_field_names = d.display_field_names;
    component.is_mirrored = d.is_mirrored;
    component.is_unmanaged = d.is_unmanaged;
    component.is_user_configurable = d.is_user_configurable;
    component.lib_identifier_kind = d.lib_identifier_kind;
    component.revision_details = d.revision_details.clone();
    component.revision_hrid = d.revision_hrid.clone();
    component.revision_state = d.revision_state.clone();
    component.revision_status = d.revision_status.clone();
    component.symbol_item_guid = d.symbol_item_guid.clone();
    component.symbol_items_guid = d.symbol_items_guid.clone();
    component.symbol_revision_guid = d.symbol_revision_guid.clone();
    component.symbol_vault_guid = d.symbol_vault_guid.clone();
    component.use_db_table_name = d.use_db_table_name;
    component.use_library_name = d.use_library_name;
    component.variant_option = d.variant_option;
    component.vault_hrid = d.vault_hrid.clone();
    component.generic_component_template_guid = d.generic_component_template_guid.clone();
    component.graphically_locked = d.graphically_locked;
    component.disabled = d.disabled;
    component.dimmed = d.dimmed;
    component.unique_id = d.unique_id.clone();
    component.owner_part_id = d.owner_part_id;
    component.owner_part_display_mode = d.owner_part_display_mode;
    if component.name.is_empty() {
        component.name = d
            .lib_reference
            .clone()
            .or_else(|| d.design_item_id.clone())
            .unwrap_or_default();
    }
}

pub fn component_to_record_params(component: &Component, params: &mut ParameterMap) {
    let mut d = dto::ComponentDto::default();
    let (lx, lxf) = coord_to_dxp_frac(component.location.x);
    let (ly, lyf) = coord_to_dxp_frac(component.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    d.lib_reference = component.lib_reference.clone();
    d.component_description = component.description.clone();
    // Inverse of "user-facing PARTCOUNT - 1": always emit at least 1.
    d.part_count = component.part_count.max(0) + 1;
    d.display_mode_count = component.display_mode_count;
    d.display_mode = component.display_mode;
    d.orientation = component.orientation;
    d.current_part_id = component.current_part_id;
    d.show_hidden_pins = component.show_hidden_pins;
    d.library_path = component.library_path.clone();
    d.source_library_name = component.source_library_name.clone();
    d.sheet_part_file_name = component.sheet_part_file_name.clone();
    d.target_file_name = component.target_file_name.clone();
    d.area_color = component.area_color;
    d.color = component.color;
    d.override_colors = component.override_colors;
    d.designator_prefix = component.designator_prefix.clone();
    d.designator_locked = component.designator_locked;
    d.part_id_locked = component.part_id_locked;
    d.design_item_id = component.design_item_id.clone();
    d.component_kind = component.component_kind;
    d.alias_list = component.alias_list.clone();
    d.all_pin_count = component.all_pin_count;
    d.symbol_reference = component.symbol_reference.clone().unwrap_or_default();
    d.database_library_name = component.database_library_name.clone();
    d.database_table_name = component.database_table_name.clone();
    d.library_identifier = component.library_identifier.clone();
    d.vault_guid = component.vault_guid.clone();
    d.item_guid = component.item_guid.clone();
    d.revision_guid = component.revision_guid.clone();
    d.pins_moveable = component.pins_moveable;
    d.pin_color = component.pin_color;
    d.show_hidden_fields = component.show_hidden_fields;
    d.not_used_b_table_name = component.not_used_b_table_name.clone();
    d.configuration_parameters = component.configuration_parameters.clone();
    d.configurator_name = component.configurator_name.clone();
    d.display_field_names = component.display_field_names;
    d.is_mirrored = component.is_mirrored;
    d.is_unmanaged = component.is_unmanaged;
    d.is_user_configurable = component.is_user_configurable;
    d.lib_identifier_kind = component.lib_identifier_kind;
    d.revision_details = component.revision_details.clone();
    d.revision_hrid = component.revision_hrid.clone();
    d.revision_state = component.revision_state.clone();
    d.revision_status = component.revision_status.clone();
    d.symbol_item_guid = component.symbol_item_guid.clone();
    d.symbol_items_guid = component.symbol_items_guid.clone();
    d.symbol_revision_guid = component.symbol_revision_guid.clone();
    d.symbol_vault_guid = component.symbol_vault_guid.clone();
    d.use_db_table_name = component.use_db_table_name;
    d.use_library_name = component.use_library_name;
    d.variant_option = component.variant_option;
    d.vault_hrid = component.vault_hrid.clone();
    d.generic_component_template_guid = component.generic_component_template_guid.clone();
    d.graphically_locked = component.graphically_locked;
    d.disabled = component.disabled;
    d.dimmed = component.dimmed;
    d.unique_id = component.unique_id.clone();
    d.owner_part_id = component.owner_part_id;
    d.owner_part_display_mode = component.owner_part_display_mode;
    d.to_params(params);
    // Special: SYMBOLREFERENCE is required even when empty; some Altium files
    // expect the key to exist. Force it.
    if !params.contains_key("SYMBOLREFERENCE") {
        params.insert("SYMBOLREFERENCE", "");
    }
}

// Helpers for vertex-bearing records

fn read_vertices(params: &ParameterMap, declared_count: i32, scan_extra: i32) -> Vec<CoordPoint> {
    let mut out = Vec::new();
    let upper = declared_count.max(scan_extra);
    for i in 1..=upper {
        let xk = format!("X{i}");
        let yk = format!("Y{i}");
        let xkf = format!("X{i}_FRAC");
        let ykf = format!("Y{i}_FRAC");
        let has_x = params.contains_key(&xk);
        let has_y = params.contains_key(&yk);
        if !has_x && !has_y && i > declared_count {
            break;
        }
        let xi = params.get_i32(&xk).unwrap_or(0);
        let yi = params.get_i32(&yk).unwrap_or(0);
        let xf = params.get_i32(&xkf).unwrap_or(0);
        let yf = params.get_i32(&ykf).unwrap_or(0);
        out.push(CoordPoint::new(
            coord_from_dxp_frac(xi, xf),
            coord_from_dxp_frac(yi, yf),
        ));
    }
    out
}

fn write_vertices(params: &mut ParameterMap, vertices: &[CoordPoint]) {
    if vertices.is_empty() {
        return;
    }
    params.insert("LOCATIONCOUNT", vertices.len().to_string());
    for (i, v) in vertices.iter().enumerate() {
        let idx = i + 1;
        let (xi, xf) = coord_to_dxp_frac(v.x);
        let (yi, yf) = coord_to_dxp_frac(v.y);
        // Always emit X{n}/Y{n} even when zero — real Altium files write the
        // coordinate explicitly, and round-tripping needs to preserve that.
        // FRAC subkeys keep the omit-on-zero convention since most whole-mil
        // values legitimately have zero fractional part.
        params.insert(format!("X{idx}").as_str(), xi.to_string());
        if xf != 0 {
            params.insert(format!("X{idx}_FRAC").as_str(), xf.to_string());
        }
        params.insert(format!("Y{idx}").as_str(), yi.to_string());
        if yf != 0 {
            params.insert(format!("Y{idx}_FRAC").as_str(), yf.to_string());
        }
    }
}

fn dxp_pair_to_params(
    params: &mut ParameterMap,
    name_x: &str,
    name_x_frac: &str,
    name_y: &str,
    name_y_frac: &str,
    point: CoordPoint,
) {
    let (xi, xf) = coord_to_dxp_frac(point.x);
    let (yi, yf) = coord_to_dxp_frac(point.y);
    if xi != 0 {
        params.insert(name_x, xi.to_string());
    }
    if xf != 0 {
        params.insert(name_x_frac, xf.to_string());
    }
    if yi != 0 {
        params.insert(name_y, yi.to_string());
    }
    if yf != 0 {
        params.insert(name_y_frac, yf.to_string());
    }
}

// Pin

const PIN_CONG_ORIENTATION: i32 = 0x03;
const PIN_CONG_SHOW_NAME: i32 = 0x08;
const PIN_CONG_SHOW_DESIGNATOR: i32 = 0x10;

pub fn pin_from_params(params: &ParameterMap) -> Pin {
    let d = dto::PinDto::from_params(params);
    let cong = d.pin_conglomerate;
    let orientation = match cong & PIN_CONG_ORIENTATION {
        0 => PinOrientation::Right,
        1 => PinOrientation::Up,
        2 => PinOrientation::Left,
        3 => PinOrientation::Down,
        _ => PinOrientation::Right,
    };
    let electrical =
        PinElectricalType::try_from(d.electrical).unwrap_or(PinElectricalType::Passive);

    Pin {
        name: d.name.clone(),
        designator: d.designator.clone(),
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        length: coord_from_dxp_frac(d.pin_length, d.pin_length_frac),
        electrical_type: electrical,
        orientation,
        show_name: (cong & PIN_CONG_SHOW_NAME) != 0,
        show_designator: (cong & PIN_CONG_SHOW_DESIGNATOR) != 0,
        description: d.description.clone(),
        formal_type: d.formal_type,
        symbol_inner_edge: d.symbol_inner_edge,
        symbol_outer_edge: d.symbol_outer_edge,
        symbol_inside: d.symbol_inside,
        symbol_outside: d.symbol_outside,
        symbol_line_width: d.symbol_line_width,
        swap_id_part: if d.swap_id_part != 0 {
            Some(d.swap_id_part.to_string())
        } else {
            None
        },
        pin_propagation_delay: d.pin_propagation_delay as i32,
        designator_custom_font_id: d.designator_custom_font_id,
        name_custom_font_id: d.name_custom_font_id,
        width: d.width,
        color: d.color,
        area_color: d.area_color,
        default_value: d.default_value.clone(),
        is_hidden: d.is_hidden,
        designator_custom_color: d.designator_custom_color,
        designator_custom_position_margin: d.designator_custom_position_margin,
        designator_custom_position_rotation_anchor: d.designator_custom_position_rotation_anchor,
        designator_custom_position_rotation_relative: d
            .designator_custom_position_rotation_relative,
        designator_font_mode: d.designator_font_mode,
        designator_position_mode: d.designator_position_mode,
        name_custom_color: d.name_custom_color,
        name_custom_position_margin: d.name_custom_position_margin,
        name_custom_position_rotation_anchor: d.name_custom_position_rotation_anchor,
        name_custom_position_rotation_relative: d.name_custom_position_rotation_relative,
        name_font_mode: d.name_font_mode,
        name_position_mode: d.name_position_mode,
        swap_id_pair: d.swap_id_pair.clone(),
        swap_id_part_pin: d.swap_id_part_pin.clone(),
        swap_id_pin: d.swap_id_pin.clone(),
        hidden_net_name: d.hidden_net_name.clone(),
        pin_package_length: coord_from_dxp_frac(d.pin_package_length, 0),
        common: d.extract_common(),
    }
}

pub fn pin_to_params(pin: &Pin, params: &mut ParameterMap) {
    let mut d = dto::PinDto::default();
    let (lx, lxf) = coord_to_dxp_frac(pin.location.x);
    let (ly, lyf) = coord_to_dxp_frac(pin.location.y);
    let (pl, plf) = coord_to_dxp_frac(pin.length);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    d.pin_length = pl;
    d.pin_length_frac = plf;
    d.color = pin.color;
    d.name = pin.name.clone();
    d.designator = pin.designator.clone();
    d.description = pin.description.clone();
    d.electrical = pin.electrical_type as i32;
    let mut cong = pin.orientation as i32 & PIN_CONG_ORIENTATION;
    if pin.show_name {
        cong |= PIN_CONG_SHOW_NAME;
    }
    if pin.show_designator {
        cong |= PIN_CONG_SHOW_DESIGNATOR;
    }
    // Bit 5 (0x20) is always set by real Altium when emitting pins
    cong |= 0x20;
    d.pin_conglomerate = cong;
    d.formal_type = pin.formal_type;
    d.symbol_inner_edge = pin.symbol_inner_edge;
    d.symbol_outer_edge = pin.symbol_outer_edge;
    d.symbol_inside = pin.symbol_inside;
    d.symbol_outside = pin.symbol_outside;
    d.symbol_line_width = pin.symbol_line_width;
    d.swap_id_part = pin
        .swap_id_part
        .as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    d.pin_propagation_delay = pin.pin_propagation_delay as f64;
    d.designator_custom_font_id = pin.designator_custom_font_id;
    d.name_custom_font_id = pin.name_custom_font_id;
    d.width = pin.width;
    d.area_color = pin.area_color;
    d.default_value = pin.default_value.clone();
    d.is_hidden = pin.is_hidden;
    d.designator_custom_color = pin.designator_custom_color;
    d.designator_custom_position_margin = pin.designator_custom_position_margin;
    d.designator_custom_position_rotation_anchor = pin.designator_custom_position_rotation_anchor;
    d.designator_custom_position_rotation_relative =
        pin.designator_custom_position_rotation_relative;
    d.designator_font_mode = pin.designator_font_mode;
    d.designator_position_mode = pin.designator_position_mode;
    d.name_custom_color = pin.name_custom_color;
    d.name_custom_position_margin = pin.name_custom_position_margin;
    d.name_custom_position_rotation_anchor = pin.name_custom_position_rotation_anchor;
    d.name_custom_position_rotation_relative = pin.name_custom_position_rotation_relative;
    d.name_font_mode = pin.name_font_mode;
    d.name_position_mode = pin.name_position_mode;
    d.swap_id_pair = pin.swap_id_pair.clone();
    d.swap_id_part_pin = pin.swap_id_part_pin.clone();
    d.swap_id_pin = pin.swap_id_pin.clone();
    d.hidden_net_name = pin.hidden_net_name.clone();
    let (pkg, _) = coord_to_dxp_frac(pin.pin_package_length);
    d.pin_package_length = pkg;
    d.apply_common_from(&pin.common);
    d.to_params(params);
}

// Line

pub fn line_from_params(params: &ParameterMap) -> Line {
    let d = dto::LineDto::from_params(params);
    Line {
        start: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        end: CoordPoint::new(
            coord_from_dxp_frac(d.corner_x, d.corner_x_frac),
            coord_from_dxp_frac(d.corner_y, d.corner_y_frac),
        ),
        width: line_width_from_index(d.line_width),
        color: d.color,
        line_style: d.line_style,
        area_color: d.area_color,
        common: d.extract_common(),
    }
}

pub fn line_to_params(line: &Line, params: &mut ParameterMap) {
    let mut d = dto::LineDto::default();
    dxp_pair_to_params(
        params,
        "LOCATION.X",
        "LOCATION.X_FRAC",
        "LOCATION.Y",
        "LOCATION.Y_FRAC",
        line.start,
    );
    let (cx, cxf) = coord_to_dxp_frac(line.end.x);
    let (cy, cyf) = coord_to_dxp_frac(line.end.y);
    d.corner_x = cx;
    d.corner_x_frac = cxf;
    d.corner_y = cy;
    d.corner_y_frac = cyf;
    d.line_width = super::binary::line_width_to_index(line.width);
    d.color = line.color;
    d.line_style = line.line_style;
    d.area_color = line.area_color;
    d.apply_common_from(&line.common);
    d.to_params(params);
    // dxp_pair_to_params already wrote the start point but the DTO's to_params
    // only wrote non-zero fields; ensure the dxp pair sticks if values are 0.
    let _ = format_bool_long; // silences unused-import in modules with no bool
}

// Rectangle

pub fn rectangle_from_params(params: &ParameterMap) -> Rectangle {
    let d = dto::RectangleDto::from_params(params);
    Rectangle {
        corner1: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        corner2: CoordPoint::new(
            coord_from_dxp_frac(d.corner_x, d.corner_x_frac),
            coord_from_dxp_frac(d.corner_y, d.corner_y_frac),
        ),
        line_width: line_width_from_index(d.line_width),
        line_style: d.line_style,
        is_filled: d.is_solid,
        is_transparent: d.transparent,
        color: d.color,
        fill_color: d.area_color,
        common: d.extract_common(),
    }
}

pub fn rectangle_to_params(r: &Rectangle, params: &mut ParameterMap) {
    let mut d = dto::RectangleDto::default();
    let (lx, lxf) = coord_to_dxp_frac(r.corner1.x);
    let (ly, lyf) = coord_to_dxp_frac(r.corner1.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    let (cx, cxf) = coord_to_dxp_frac(r.corner2.x);
    let (cy, cyf) = coord_to_dxp_frac(r.corner2.y);
    d.corner_x = cx;
    d.corner_x_frac = cxf;
    d.corner_y = cy;
    d.corner_y_frac = cyf;
    d.line_width = super::binary::line_width_to_index(r.line_width);
    d.line_style = r.line_style;
    d.is_solid = r.is_filled;
    d.transparent = r.is_transparent;
    d.color = r.color;
    d.area_color = r.fill_color;
    d.apply_common_from(&r.common);
    d.to_params(params);
}

// Label

pub fn label_from_params(params: &ParameterMap) -> Label {
    let d = dto::LabelDto::from_params(params);
    Label {
        text: d.text.clone().unwrap_or_default(),
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        font_id: d.font_id,
        justification: TextJustification::try_from(d.justification)
            .unwrap_or(TextJustification::BottomLeft),
        rotation: d.orientation as f64 * 90.0,
        color: d.color,
        is_mirrored: d.is_mirrored,
        is_hidden: d.is_hidden,
        area_color: d.area_color,
        common: d.extract_common(),
    }
}

pub fn label_to_params(l: &Label, params: &mut ParameterMap) {
    let mut d = dto::LabelDto::default();
    let (lx, lxf) = coord_to_dxp_frac(l.location.x);
    let (ly, lyf) = coord_to_dxp_frac(l.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    d.text = if l.text.is_empty() {
        None
    } else {
        Some(l.text.clone())
    };
    d.font_id = l.font_id;
    d.justification = l.justification as i32;
    d.orientation = (l.rotation / 90.0).round() as i32;
    d.color = l.color;
    d.is_mirrored = l.is_mirrored;
    d.is_hidden = l.is_hidden;
    d.area_color = l.area_color;
    d.apply_common_from(&l.common);
    d.to_params(params);
}

// Wire

pub fn wire_from_params(params: &ParameterMap) -> Wire {
    let d = dto::WireDto::from_params(params);
    let vertices = read_vertices(params, d.location_count, 10);
    Wire {
        vertices,
        line_width: d.line_width,
        color: d.color,
        // Preserve the raw int — unknown values (anything outside SchLineStyle's
        // 0..=3) would otherwise collapse to Solid on round-trip.
        line_style: d.line_style,
        area_color: d.area_color,
        is_solid: d.is_solid,
        is_transparent: d.transparent,
        auto_wire: d.auto_wire,
        underline_color: d.underline_color,
        common: d.extract_common(),
    }
}

pub fn wire_to_params(w: &Wire, params: &mut ParameterMap) {
    let mut d = dto::WireDto::default();
    d.line_width = w.line_width;
    d.color = w.color;
    d.line_style = w.line_style;
    d.location_count = w.vertices.len() as i32;
    d.area_color = w.area_color;
    d.is_solid = w.is_solid;
    d.transparent = w.is_transparent;
    d.auto_wire = w.auto_wire;
    d.underline_color = w.underline_color;
    d.apply_common_from(&w.common);
    d.to_params(params);
    write_vertices(params, &w.vertices);
}

// Polyline

pub fn polyline_from_params(params: &ParameterMap) -> Polyline {
    let d = dto::PolylineDto::from_params(params);
    let vertices = read_vertices(params, d.location_count, 50);
    Polyline {
        vertices,
        line_width: d.line_width,
        color: d.color,
        // Raw int (see `Wire::line_style`).
        line_style: d.line_style,
        start_line_shape: d.start_line_shape,
        end_line_shape: d.end_line_shape,
        line_shape_size: d.line_shape_size,
        area_color: d.area_color,
        is_transparent: d.transparent,
        is_solid: d.is_solid,
        common: d.extract_common(),
    }
}

pub fn polyline_to_params(p: &Polyline, params: &mut ParameterMap) {
    let mut d = dto::PolylineDto::default();
    d.line_width = p.line_width;
    d.color = p.color;
    d.line_style = p.line_style;
    d.location_count = p.vertices.len() as i32;
    d.area_color = p.area_color;
    d.transparent = p.is_transparent;
    d.is_solid = p.is_solid;
    d.start_line_shape = p.start_line_shape;
    d.end_line_shape = p.end_line_shape;
    d.line_shape_size = p.line_shape_size;
    d.apply_common_from(&p.common);
    d.to_params(params);
    write_vertices(params, &p.vertices);
}

// Polygon

pub fn polygon_from_params(params: &ParameterMap) -> Polygon {
    let d = dto::PolygonDto::from_params(params);
    let vertices = read_vertices(params, d.location_count, 50);
    Polygon {
        vertices,
        line_width: d.line_width,
        color: d.color,
        fill_color: d.area_color,
        is_filled: d.is_solid,
        is_transparent: d.transparent,
        common: d.extract_common(),
    }
}

pub fn polygon_to_params(p: &Polygon, params: &mut ParameterMap) {
    let mut d = dto::PolygonDto::default();
    d.line_width = p.line_width;
    d.color = p.color;
    d.area_color = p.fill_color;
    d.is_solid = p.is_filled;
    d.transparent = p.is_transparent;
    d.location_count = p.vertices.len() as i32;
    d.apply_common_from(&p.common);
    d.to_params(params);
    write_vertices(params, &p.vertices);
}

// Bezier

pub fn bezier_from_params(params: &ParameterMap) -> Bezier {
    let d = dto::BezierDto::from_params(params);
    let points = read_vertices(params, d.location_count, 50);
    Bezier {
        control_points: points,
        line_width: d.line_width,
        color: d.color,
        area_color: d.area_color,
        common: d.extract_common(),
    }
}

pub fn bezier_to_params(b: &Bezier, params: &mut ParameterMap) {
    let mut d = dto::BezierDto::default();
    d.line_width = b.line_width;
    d.color = b.color;
    d.area_color = b.area_color;
    d.location_count = b.control_points.len() as i32;
    d.apply_common_from(&b.common);
    d.to_params(params);
    write_vertices(params, &b.control_points);
}

// Arc / Pie / Ellipse / EllipticalArc / RoundedRectangle

pub fn arc_from_params(params: &ParameterMap) -> Arc {
    let d = dto::ArcDto::from_params(params);
    Arc {
        center: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        radius: coord_from_dxp_frac(d.radius, d.radius_frac),
        start_angle: d.start_angle,
        end_angle: d.end_angle,
        line_width: d.line_width,
        color: d.color,
        area_color: d.area_color,
        common: d.extract_common(),
    }
}

pub fn arc_to_params(a: &Arc, params: &mut ParameterMap) {
    let mut d = dto::ArcDto::default();
    let (lx, lxf) = coord_to_dxp_frac(a.center.x);
    let (ly, lyf) = coord_to_dxp_frac(a.center.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    let (r, rf) = coord_to_dxp_frac(a.radius);
    d.radius = r;
    d.radius_frac = rf;
    d.start_angle = a.start_angle;
    d.end_angle = a.end_angle;
    d.line_width = a.line_width;
    d.color = a.color;
    d.area_color = a.area_color;
    d.apply_common_from(&a.common);
    d.to_params(params);
}

pub fn pie_from_params(params: &ParameterMap) -> Pie {
    let d = dto::PieDto::from_params(params);
    Pie {
        center: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        radius: coord_from_dxp_frac(d.radius, d.radius_frac),
        start_angle: d.start_angle,
        end_angle: d.end_angle,
        line_width: d.line_width,
        color: d.color,
        fill_color: d.area_color,
        is_filled: d.is_solid,
        is_transparent: d.transparent,
        common: d.extract_common(),
    }
}

pub fn pie_to_params(p: &Pie, params: &mut ParameterMap) {
    let mut d = dto::PieDto::default();
    let (lx, lxf) = coord_to_dxp_frac(p.center.x);
    let (ly, lyf) = coord_to_dxp_frac(p.center.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    let (r, rf) = coord_to_dxp_frac(p.radius);
    d.radius = r;
    d.radius_frac = rf;
    d.start_angle = p.start_angle;
    d.end_angle = p.end_angle;
    d.line_width = p.line_width;
    d.color = p.color;
    d.area_color = p.fill_color;
    d.is_solid = p.is_filled;
    d.transparent = p.is_transparent;
    d.apply_common_from(&p.common);
    d.to_params(params);
}

pub fn ellipse_from_params(params: &ParameterMap) -> Ellipse {
    let d = dto::EllipseDto::from_params(params);
    let primary = coord_from_dxp_frac(d.radius, d.radius_frac);
    let secondary = if d.secondary_radius != 0 {
        coord_from_dxp_frac(d.secondary_radius, d.secondary_radius_frac)
    } else {
        primary
    };
    Ellipse {
        center: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        radius_x: primary,
        radius_y: secondary,
        line_width: d.line_width,
        color: d.color,
        fill_color: d.area_color,
        is_filled: d.is_solid,
        is_transparent: d.transparent,
        common: d.extract_common(),
    }
}

pub fn ellipse_to_params(e: &Ellipse, params: &mut ParameterMap) {
    let mut d = dto::EllipseDto::default();
    let (lx, lxf) = coord_to_dxp_frac(e.center.x);
    let (ly, lyf) = coord_to_dxp_frac(e.center.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    let (r, rf) = coord_to_dxp_frac(e.radius_x);
    d.radius = r;
    d.radius_frac = rf;
    if e.radius_y != e.radius_x {
        let (sr, srf) = coord_to_dxp_frac(e.radius_y);
        d.secondary_radius = sr;
        d.secondary_radius_frac = srf;
    }
    d.line_width = e.line_width;
    d.color = e.color;
    d.area_color = e.fill_color;
    d.is_solid = e.is_filled;
    d.transparent = e.is_transparent;
    d.apply_common_from(&e.common);
    d.to_params(params);
}

pub fn elliptical_arc_from_params(params: &ParameterMap) -> EllipticalArc {
    let d = dto::EllipticalArcDto::from_params(params);
    EllipticalArc {
        center: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        primary_radius: coord_from_dxp_frac(d.radius, d.radius_frac),
        secondary_radius: coord_from_dxp_frac(d.secondary_radius, d.secondary_radius_frac),
        start_angle: d.start_angle,
        end_angle: d.end_angle,
        line_width: coord_from_dxp_frac(d.line_width, d.line_width_frac),
        color: d.color,
        area_color: d.area_color,
        common: d.extract_common(),
    }
}

pub fn elliptical_arc_to_params(e: &EllipticalArc, params: &mut ParameterMap) {
    let mut d = dto::EllipticalArcDto::default();
    let (lx, lxf) = coord_to_dxp_frac(e.center.x);
    let (ly, lyf) = coord_to_dxp_frac(e.center.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    let (r, rf) = coord_to_dxp_frac(e.primary_radius);
    d.radius = r;
    d.radius_frac = rf;
    let (sr, srf) = coord_to_dxp_frac(e.secondary_radius);
    d.secondary_radius = sr;
    d.secondary_radius_frac = srf;
    d.start_angle = e.start_angle;
    d.end_angle = e.end_angle;
    let (lw, lwf) = coord_to_dxp_frac(e.line_width);
    d.line_width = lw;
    d.line_width_frac = lwf;
    d.color = e.color;
    d.area_color = e.area_color;
    d.apply_common_from(&e.common);
    d.to_params(params);
}

pub fn rounded_rectangle_from_params(params: &ParameterMap) -> RoundedRectangle {
    let d = dto::RoundedRectangleDto::from_params(params);
    RoundedRectangle {
        corner1: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        corner2: CoordPoint::new(
            coord_from_dxp_frac(d.corner_x, d.corner_x_frac),
            coord_from_dxp_frac(d.corner_y, d.corner_y_frac),
        ),
        corner_radius_x: coord_from_dxp_frac(d.corner_x_radius, 0),
        corner_radius_y: coord_from_dxp_frac(d.corner_y_radius, 0),
        line_width: d.line_width,
        line_style: d.line_style,
        color: d.color,
        fill_color: d.area_color,
        is_filled: d.is_solid,
        is_transparent: d.transparent,
        common: d.extract_common(),
    }
}

pub fn rounded_rectangle_to_params(r: &RoundedRectangle, params: &mut ParameterMap) {
    let mut d = dto::RoundedRectangleDto::default();
    let (lx, lxf) = coord_to_dxp_frac(r.corner1.x);
    let (ly, lyf) = coord_to_dxp_frac(r.corner1.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    let (cx, cxf) = coord_to_dxp_frac(r.corner2.x);
    let (cy, cyf) = coord_to_dxp_frac(r.corner2.y);
    d.corner_x = cx;
    d.corner_x_frac = cxf;
    d.corner_y = cy;
    d.corner_y_frac = cyf;
    let (rx, _) = coord_to_dxp_frac(r.corner_radius_x);
    d.corner_x_radius = rx;
    let (ry, _) = coord_to_dxp_frac(r.corner_radius_y);
    d.corner_y_radius = ry;
    d.line_width = r.line_width;
    d.line_style = r.line_style;
    d.color = r.color;
    d.area_color = r.fill_color;
    d.is_solid = r.is_filled;
    d.transparent = r.is_transparent;
    d.apply_common_from(&r.common);
    d.to_params(params);
}

// NetLabel / Junction / Parameter / TextFrame / Image / Symbol

pub fn net_label_from_params(params: &ParameterMap) -> NetLabel {
    let d = dto::NetLabelDto::from_params(params);
    NetLabel {
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        text: d.text.clone().unwrap_or_default(),
        orientation: d.orientation,
        justification: TextJustification::try_from(d.justification)
            .unwrap_or(TextJustification::BottomLeft),
        font_id: d.font_id,
        color: d.color,
        is_mirrored: d.is_mirrored,
        area_color: d.area_color,
        common: d.extract_common(),
    }
}

pub fn net_label_to_params(n: &NetLabel, params: &mut ParameterMap) {
    let mut d = dto::NetLabelDto::default();
    let (lx, lxf) = coord_to_dxp_frac(n.location.x);
    let (ly, lyf) = coord_to_dxp_frac(n.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    d.text = if n.text.is_empty() {
        None
    } else {
        Some(n.text.clone())
    };
    d.orientation = n.orientation;
    d.justification = n.justification as i32;
    d.font_id = n.font_id;
    d.color = n.color;
    d.is_mirrored = n.is_mirrored;
    d.area_color = n.area_color;
    d.apply_common_from(&n.common);
    d.to_params(params);
}

pub fn junction_from_params(params: &ParameterMap) -> Junction {
    let d = dto::JunctionDto::from_params(params);
    Junction {
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        size: Coord::from_raw(d.size),
        color: d.color,
        locked: d.locked,
        common: d.extract_common(),
    }
}

pub fn junction_to_params(j: &Junction, params: &mut ParameterMap) {
    let mut d = dto::JunctionDto::default();
    let (lx, lxf) = coord_to_dxp_frac(j.location.x);
    let (ly, lyf) = coord_to_dxp_frac(j.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    d.size = j.size.to_raw();
    d.color = j.color;
    d.locked = j.locked;
    d.apply_common_from(&j.common);
    d.to_params(params);
}

pub fn parameter_from_params(params: &ParameterMap) -> Parameter {
    let d = dto::ParameterDto::from_params(params);
    Parameter {
        name: d.name.clone().unwrap_or_default(),
        value: d.text.clone().unwrap_or_default(),
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        orientation: d.orientation,
        justification: TextJustification::try_from(d.justification)
            .unwrap_or(TextJustification::BottomLeft),
        font_id: d.font_id,
        color: d.color,
        is_visible: !d.is_hidden,
        hide_name: d.hide_name,
        param_type: d.param_type,
        show_name: d.show_name,
        is_mirrored: d.is_mirrored,
        is_read_only: d.read_only_state != 0,
        description: d.description.clone(),
        area_color: d.area_color,
        auto_position: d.auto_position,
        is_configurable: d.is_configurable,
        is_rule: d.is_rule,
        is_system_parameter: d.is_system_parameter,
        text_horz_anchor: d.text_horz_anchor,
        text_vert_anchor: d.text_vert_anchor,
        text_is_utf8: params.is_utf8("TEXT"),
        allow_database_synchronize: d.allow_database_synchronize,
        allow_library_synchronize: d.allow_library_synchronize,
        name_is_read_only: d.name_is_read_only,
        physical_designator: d.physical_designator.clone(),
        value_is_read_only: d.value_is_read_only,
        variant_option: d.variant_option.clone(),
        common: d.extract_common(),
    }
}

pub fn parameter_to_params(p: &Parameter, params: &mut ParameterMap) {
    let mut d = dto::ParameterDto::default();
    let (lx, lxf) = coord_to_dxp_frac(p.location.x);
    let (ly, lyf) = coord_to_dxp_frac(p.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    d.color = p.color;
    d.name = if p.name.is_empty() {
        None
    } else {
        Some(p.name.clone())
    };
    d.text = if p.value.is_empty() {
        None
    } else {
        Some(p.value.clone())
    };
    d.description = p.description.clone();
    d.param_type = p.param_type;
    d.font_id = p.font_id;
    d.orientation = p.orientation;
    d.justification = p.justification as i32;
    d.show_name = p.show_name;
    d.is_mirrored = p.is_mirrored;
    d.is_hidden = !p.is_visible;
    d.read_only_state = if p.is_read_only { 1 } else { 0 };
    d.area_color = p.area_color;
    d.auto_position = p.auto_position;
    d.is_configurable = p.is_configurable;
    d.is_rule = p.is_rule;
    d.is_system_parameter = p.is_system_parameter;
    d.text_horz_anchor = p.text_horz_anchor;
    d.text_vert_anchor = p.text_vert_anchor;
    d.hide_name = p.hide_name;
    d.allow_database_synchronize = p.allow_database_synchronize;
    d.allow_library_synchronize = p.allow_library_synchronize;
    d.name_is_read_only = p.name_is_read_only;
    d.physical_designator = p.physical_designator.clone();
    d.value_is_read_only = p.value_is_read_only;
    d.variant_option = p.variant_option.clone();
    d.apply_common_from(&p.common);
    d.to_params(params);
    if p.text_is_utf8 && !p.value.is_empty() {
        // Re-flag the TEXT entry as UTF-8.
        params.insert_utf8("TEXT", p.value.clone());
    }
}

pub fn text_frame_from_params(params: &ParameterMap) -> TextFrame {
    let d = dto::TextFrameDto::from_params(params);
    TextFrame {
        corner1: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        corner2: CoordPoint::new(
            coord_from_dxp_frac(d.corner_x, d.corner_x_frac),
            coord_from_dxp_frac(d.corner_y, d.corner_y_frac),
        ),
        text: d.text.clone().unwrap_or_default(),
        orientation: d.orientation,
        alignment: TextJustification::try_from(d.alignment)
            .unwrap_or(TextJustification::BottomLeft),
        font_id: d.font_id,
        text_color: d.text_color,
        border_color: d.color,
        fill_color: d.area_color,
        show_border: d.show_border,
        is_filled: d.is_solid,
        is_transparent: d.transparent,
        word_wrap: d.word_wrap,
        clip_to_rect: d.clip_to_rect,
        line_width: d.line_width,
        line_style: d.line_style,
        text_margin: d.text_margin,
        common: d.extract_common(),
    }
}

pub fn text_frame_to_params(t: &TextFrame, params: &mut ParameterMap) {
    let mut d = dto::TextFrameDto::default();
    let (lx, lxf) = coord_to_dxp_frac(t.corner1.x);
    let (ly, lyf) = coord_to_dxp_frac(t.corner1.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    let (cx, cxf) = coord_to_dxp_frac(t.corner2.x);
    let (cy, cyf) = coord_to_dxp_frac(t.corner2.y);
    d.corner_x = cx;
    d.corner_x_frac = cxf;
    d.corner_y = cy;
    d.corner_y_frac = cyf;
    d.text = if t.text.is_empty() {
        None
    } else {
        Some(t.text.clone())
    };
    d.orientation = t.orientation;
    d.alignment = t.alignment as i32;
    d.font_id = t.font_id;
    d.text_color = t.text_color;
    d.color = t.border_color;
    d.area_color = t.fill_color;
    d.show_border = t.show_border;
    d.is_solid = t.is_filled;
    d.transparent = t.is_transparent;
    d.word_wrap = t.word_wrap;
    d.clip_to_rect = t.clip_to_rect;
    d.line_width = t.line_width;
    d.line_style = t.line_style;
    d.text_margin = t.text_margin;
    d.apply_common_from(&t.common);
    d.to_params(params);
}

pub fn image_from_params(params: &ParameterMap) -> Image {
    let d = dto::ImageDto::from_params(params);
    Image {
        corner1: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        corner2: CoordPoint::new(
            coord_from_dxp_frac(d.corner_x, d.corner_x_frac),
            coord_from_dxp_frac(d.corner_y, d.corner_y_frac),
        ),
        keep_aspect: d.keep_aspect,
        embed_image: d.embed_image,
        filename: d.file_name.clone(),
        image_data: None,
        border_color: d.color,
        show_border: d.show_border,
        line_width: d.line_width,
        area_color: d.area_color,
        is_solid: d.is_solid,
        line_style: d.line_style,
        is_transparent: d.transparent,
        common: d.extract_common(),
    }
}

pub fn image_to_params(i: &Image, params: &mut ParameterMap) {
    let mut d = dto::ImageDto::default();
    let (lx, lxf) = coord_to_dxp_frac(i.corner1.x);
    let (ly, lyf) = coord_to_dxp_frac(i.corner1.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    let (cx, cxf) = coord_to_dxp_frac(i.corner2.x);
    let (cy, cyf) = coord_to_dxp_frac(i.corner2.y);
    d.corner_x = cx;
    d.corner_x_frac = cxf;
    d.corner_y = cy;
    d.corner_y_frac = cyf;
    d.color = i.border_color;
    d.line_width = i.line_width;
    d.keep_aspect = i.keep_aspect;
    d.embed_image = i.embed_image;
    d.file_name = i.filename.clone();
    d.area_color = i.area_color;
    d.is_solid = i.is_solid;
    d.line_style = i.line_style;
    d.transparent = i.is_transparent;
    d.show_border = i.show_border;
    d.apply_common_from(&i.common);
    d.to_params(params);
}

pub fn symbol_from_params(params: &ParameterMap) -> Symbol {
    let d = dto::SymbolDto::from_params(params);
    let scale = if d.scale_factor != 0 {
        d.scale_factor
    } else {
        1
    };
    Symbol {
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        symbol_type: d.symbol,
        is_mirrored: d.is_mirrored,
        orientation: d.orientation,
        line_width: d.line_width,
        scale_factor: scale,
        color: d.color,
        common: d.extract_common(),
    }
}

pub fn symbol_to_params(s: &Symbol, params: &mut ParameterMap) {
    let mut d = dto::SymbolDto::default();
    let (lx, lxf) = coord_to_dxp_frac(s.location.x);
    let (ly, lyf) = coord_to_dxp_frac(s.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    d.color = s.color;
    d.symbol = s.symbol_type;
    d.is_mirrored = s.is_mirrored;
    d.orientation = s.orientation;
    d.line_width = s.line_width;
    d.scale_factor = s.scale_factor;
    d.apply_common_from(&s.common);
    d.to_params(params);
}

// PowerObject / NoErc / BusEntry / Bus / Port / SheetSymbol / SheetEntry

pub fn power_object_from_params(params: &ParameterMap) -> PowerObject {
    let d = dto::PowerObjectDto::from_params(params);
    PowerObject {
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        text: d.text.clone().unwrap_or_default(),
        style: PowerPortStyle::try_from(d.style).unwrap_or(PowerPortStyle::Circle),
        rotation: d.orientation as f64 * 90.0,
        show_net_name: d.show_net_name,
        is_cross_sheet_connector: d.is_cross_sheet_connector,
        color: d.color,
        font_id: d.font_id,
        area_color: d.area_color,
        is_custom_style: d.is_custom_style,
        is_mirrored: d.is_mirrored,
        justification: d.justification,
        common: d.extract_common(),
    }
}

pub fn power_object_to_params(p: &PowerObject, params: &mut ParameterMap) {
    let mut d = dto::PowerObjectDto::default();
    let (lx, lxf) = coord_to_dxp_frac(p.location.x);
    let (ly, lyf) = coord_to_dxp_frac(p.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    d.color = p.color;
    d.text = if p.text.is_empty() {
        None
    } else {
        Some(p.text.clone())
    };
    d.style = p.style as i32;
    d.orientation = (p.rotation / 90.0).round() as i32;
    d.show_net_name = p.show_net_name;
    d.is_cross_sheet_connector = p.is_cross_sheet_connector;
    d.font_id = p.font_id;
    d.area_color = p.area_color;
    d.is_custom_style = p.is_custom_style;
    d.is_mirrored = p.is_mirrored;
    d.justification = p.justification;
    d.apply_common_from(&p.common);
    d.to_params(params);
}

pub fn no_erc_from_params(params: &ParameterMap) -> NoErc {
    let d = dto::NoErcDto::from_params(params);
    NoErc {
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        orientation: d.orientation,
        color: d.color,
        is_active: d.is_active,
        symbol: d.symbol,
        area_color: d.area_color,
        suppress_all: d.suppress_all,
        error_kind_set_to_suppress: d.error_kind_set_to_suppress.clone(),
        common: d.extract_common(),
    }
}

pub fn no_erc_to_params(n: &NoErc, params: &mut ParameterMap) {
    let mut d = dto::NoErcDto::default();
    let (lx, lxf) = coord_to_dxp_frac(n.location.x);
    let (ly, lyf) = coord_to_dxp_frac(n.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    d.orientation = n.orientation;
    d.color = n.color;
    d.is_active = n.is_active;
    d.symbol = n.symbol;
    d.area_color = n.area_color;
    d.suppress_all = n.suppress_all;
    d.error_kind_set_to_suppress = n.error_kind_set_to_suppress.clone();
    d.apply_common_from(&n.common);
    d.to_params(params);
}

pub fn bus_entry_from_params(params: &ParameterMap) -> BusEntry {
    let d = dto::BusEntryDto::from_params(params);
    BusEntry {
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        corner: CoordPoint::new(
            coord_from_dxp_frac(d.corner_x, d.corner_x_frac),
            coord_from_dxp_frac(d.corner_y, d.corner_y_frac),
        ),
        line_width: d.line_width,
        color: d.color,
        common: d.extract_common(),
    }
}

pub fn bus_entry_to_params(b: &BusEntry, params: &mut ParameterMap) {
    let mut d = dto::BusEntryDto::default();
    let (lx, lxf) = coord_to_dxp_frac(b.location.x);
    let (ly, lyf) = coord_to_dxp_frac(b.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    let (cx, cxf) = coord_to_dxp_frac(b.corner.x);
    let (cy, cyf) = coord_to_dxp_frac(b.corner.y);
    d.corner_x = cx;
    d.corner_x_frac = cxf;
    d.corner_y = cy;
    d.corner_y_frac = cyf;
    d.line_width = b.line_width;
    d.color = b.color;
    d.apply_common_from(&b.common);
    d.to_params(params);
}

pub fn bus_from_params(params: &ParameterMap) -> Bus {
    let d = dto::BusDto::from_params(params);
    let vertices = read_vertices(params, d.location_count, 10);
    Bus {
        vertices,
        line_width: d.line_width,
        line_style: d.line_style,
        color: d.color,
        area_color: d.area_color,
        common: d.extract_common(),
    }
}

pub fn bus_to_params(b: &Bus, params: &mut ParameterMap) {
    let mut d = dto::BusDto::default();
    d.line_width = b.line_width;
    d.line_style = b.line_style;
    d.color = b.color;
    d.area_color = b.area_color;
    d.location_count = b.vertices.len() as i32;
    d.apply_common_from(&b.common);
    d.to_params(params);
    write_vertices(params, &b.vertices);
}

pub fn port_from_params(params: &ParameterMap) -> Port {
    let d = dto::PortDto::from_params(params);
    Port {
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        name: d.name.clone().unwrap_or_default(),
        io_type: d.io_type,
        style: d.style,
        alignment: d.alignment,
        width: coord_from_dxp_frac(d.width, 0),
        height: coord_from_dxp_frac(d.height, 0),
        border_width: d.border_width,
        auto_size: d.auto_size,
        connected_end: d.connected_end,
        cross_reference: d.cross_reference.clone(),
        show_net_name: d.show_net_name,
        harness_type: d.harness_type.clone(),
        harness_color: d.harness_color,
        is_custom_style: d.is_custom_style,
        font_id: d.font_id,
        color: d.color,
        area_color: d.area_color,
        text_color: d.text_color,
        common: d.extract_common(),
    }
}

pub fn port_to_params(p: &Port, params: &mut ParameterMap) {
    let mut d = dto::PortDto::default();
    let (lx, lxf) = coord_to_dxp_frac(p.location.x);
    let (ly, lyf) = coord_to_dxp_frac(p.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    d.name = if p.name.is_empty() {
        None
    } else {
        Some(p.name.clone())
    };
    d.io_type = p.io_type;
    d.style = p.style;
    d.alignment = p.alignment;
    let (w, _) = coord_to_dxp_frac(p.width);
    d.width = w;
    let (h, _) = coord_to_dxp_frac(p.height);
    d.height = h;
    d.border_width = p.border_width;
    d.auto_size = p.auto_size;
    d.connected_end = p.connected_end;
    d.cross_reference = p.cross_reference.clone();
    d.show_net_name = p.show_net_name;
    d.harness_type = p.harness_type.clone();
    d.harness_color = p.harness_color;
    d.is_custom_style = p.is_custom_style;
    d.font_id = p.font_id;
    d.color = p.color;
    d.area_color = p.area_color;
    d.text_color = p.text_color;
    d.apply_common_from(&p.common);
    d.to_params(params);
}

pub fn sheet_symbol_from_params(params: &ParameterMap) -> SheetSymbol {
    let d = dto::SheetSymbolDto::from_params(params);
    SheetSymbol {
        entries: Vec::new(),
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        x_size: coord_from_dxp_frac(d.x_size, 0),
        y_size: coord_from_dxp_frac(d.y_size, 0),
        is_mirrored: d.is_mirrored,
        file_name: d.file_name.clone(),
        sheet_name: d.sheet_name.clone(),
        line_width: d.line_width,
        color: d.color,
        area_color: d.area_color,
        is_solid: d.is_solid,
        show_hidden_fields: d.show_hidden_fields,
        symbol_type: d.symbol_type,
        design_item_id: d.design_item_id.clone(),
        item_guid: d.item_guid.clone(),
        lib_identifier_kind: d.lib_identifier_kind,
        library_identifier: d.library_identifier.clone(),
        revision_guid: d.revision_guid.clone(),
        source_library_name: d.source_library_name.clone(),
        vault_guid: d.vault_guid.clone(),
        common: d.extract_common(),
    }
}

pub fn sheet_symbol_to_params(s: &SheetSymbol, params: &mut ParameterMap) {
    let mut d = dto::SheetSymbolDto::default();
    let (lx, lxf) = coord_to_dxp_frac(s.location.x);
    let (ly, lyf) = coord_to_dxp_frac(s.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    let (xs, _) = coord_to_dxp_frac(s.x_size);
    d.x_size = xs;
    let (ys, _) = coord_to_dxp_frac(s.y_size);
    d.y_size = ys;
    d.is_mirrored = s.is_mirrored;
    d.file_name = s.file_name.clone();
    d.sheet_name = s.sheet_name.clone();
    d.line_width = s.line_width;
    d.color = s.color;
    d.area_color = s.area_color;
    d.is_solid = s.is_solid;
    d.show_hidden_fields = s.show_hidden_fields;
    d.symbol_type = s.symbol_type;
    d.design_item_id = s.design_item_id.clone();
    d.item_guid = s.item_guid.clone();
    d.lib_identifier_kind = s.lib_identifier_kind;
    d.library_identifier = s.library_identifier.clone();
    d.revision_guid = s.revision_guid.clone();
    d.source_library_name = s.source_library_name.clone();
    d.vault_guid = s.vault_guid.clone();
    d.apply_common_from(&s.common);
    d.to_params(params);
}

pub fn sheet_entry_from_params(params: &ParameterMap) -> SheetEntry {
    let d = dto::SheetEntryDto::from_params(params);
    SheetEntry {
        side: d.side,
        distance_from_top: coord_from_dxp_frac(d.distance_from_top, 0),
        name: d.name.clone().unwrap_or_default(),
        io_type: d.io_type,
        style: d.style,
        arrow_kind: d.arrow_kind,
        harness_type: d.harness_type.clone(),
        harness_color: d.harness_color,
        font_id: d.font_id,
        color: d.color,
        area_color: d.area_color,
        text_color: d.text_color,
        text_style: d.text_style,
        common: d.extract_common(),
    }
}

pub fn sheet_entry_to_params(e: &SheetEntry, params: &mut ParameterMap) {
    let mut d = dto::SheetEntryDto::default();
    d.side = e.side;
    let (dft, _) = coord_to_dxp_frac(e.distance_from_top);
    d.distance_from_top = dft;
    d.name = if e.name.is_empty() {
        None
    } else {
        Some(e.name.clone())
    };
    d.io_type = e.io_type;
    d.style = e.style;
    d.arrow_kind = e.arrow_kind;
    d.harness_type = e.harness_type.clone();
    d.harness_color = e.harness_color;
    d.font_id = e.font_id;
    d.color = e.color;
    d.area_color = e.area_color;
    d.text_color = e.text_color;
    d.text_style = e.text_style;
    d.apply_common_from(&e.common);
    d.to_params(params);
}

// Blanket / ParameterSet

pub fn blanket_from_params(params: &ParameterMap) -> Blanket {
    let d = dto::BlanketDto::from_params(params);
    let vertices = read_vertices(params, d.location_count, 50);
    Blanket {
        vertices,
        parameters: Vec::new(),
        is_collapsed: d.is_collapsed,
        line_width: d.line_width,
        line_style: d.line_style,
        is_solid: d.is_solid,
        is_transparent: d.transparent,
        color: d.color,
        area_color: d.area_color,
        common: d.extract_common(),
    }
}

pub fn blanket_to_params(b: &Blanket, params: &mut ParameterMap) {
    let mut d = dto::BlanketDto::default();
    d.color = b.color;
    d.area_color = b.area_color;
    d.line_width = b.line_width;
    d.line_style = b.line_style;
    d.is_solid = b.is_solid;
    d.transparent = b.is_transparent;
    d.is_collapsed = b.is_collapsed;
    d.location_count = b.vertices.len() as i32;
    d.apply_common_from(&b.common);
    d.to_params(params);
    write_vertices(params, &b.vertices);
}

pub fn parameter_set_from_params(params: &ParameterMap) -> ParameterSet {
    let d = dto::ParameterSetDto::from_params(params);
    ParameterSet {
        parameters: Vec::new(),
        location: CoordPoint::new(
            coord_from_dxp_frac(d.location_x, d.location_x_frac),
            coord_from_dxp_frac(d.location_y, d.location_y_frac),
        ),
        orientation: d.orientation,
        style: d.style,
        color: d.color,
        area_color: d.area_color,
        name: d.name.clone(),
        show_hidden_fields: d.show_hidden_fields,
        border_width: d.border_width,
        is_solid: d.is_solid,
        common: d.extract_common(),
    }
}

pub fn parameter_set_to_params(p: &ParameterSet, params: &mut ParameterMap) {
    let mut d = dto::ParameterSetDto::default();
    let (lx, lxf) = coord_to_dxp_frac(p.location.x);
    let (ly, lyf) = coord_to_dxp_frac(p.location.y);
    d.location_x = lx;
    d.location_x_frac = lxf;
    d.location_y = ly;
    d.location_y_frac = lyf;
    d.orientation = p.orientation;
    d.style = p.style;
    d.color = p.color;
    d.area_color = p.area_color;
    d.name = p.name.clone();
    d.show_hidden_fields = p.show_hidden_fields;
    d.border_width = p.border_width;
    d.is_solid = p.is_solid;
    d.apply_common_from(&p.common);
    d.to_params(params);
}

// Implementation hierarchy (records 44-48)

pub fn implementation_from_params(params: &ParameterMap) -> Implementation {
    Implementation {
        description: params.get("DESCRIPTION").map(str::to_owned),
        model_name: params.get("MODELNAME").map(str::to_owned),
        model_type: params.get("MODELTYPE").map(str::to_owned),
        is_current: params.get_bool("ISCURRENT"),
        data_file_kinds: collect_indexed(params, "MODELDATAFILEKIND", 1),
        data_file_entities: collect_indexed(params, "MODELDATAFILEENTITY", 1),
        map_definers: Vec::new(),
        common: common_record_from_params(params),
    }
}

pub fn implementation_to_params(impl_: &Implementation, params: &mut ParameterMap) {
    params.insert("RECORD", "45");
    if let Some(d) = &impl_.description {
        params.insert("DESCRIPTION", d.clone());
    }
    if let Some(n) = &impl_.model_name {
        params.insert("MODELNAME", n.clone());
    }
    if let Some(t) = &impl_.model_type {
        params.insert("MODELTYPE", t.clone());
    }
    if impl_.is_current {
        params.insert("ISCURRENT", "T");
    }
    let max = impl_
        .data_file_kinds
        .len()
        .max(impl_.data_file_entities.len());
    if max > 0 {
        params.insert("DATAFILECOUNT", max.to_string());
        for (i, kind) in impl_.data_file_kinds.iter().enumerate() {
            params.insert(format!("MODELDATAFILEKIND{}", i + 1).as_str(), kind.clone());
        }
        for (i, ent) in impl_.data_file_entities.iter().enumerate() {
            params.insert(
                format!("MODELDATAFILEENTITY{}", i + 1).as_str(),
                ent.clone(),
            );
        }
    }
    write_common_to_params(params, &impl_.common);
}

pub fn map_definer_from_params(params: &ParameterMap) -> MapDefiner {
    MapDefiner {
        designator_interface: params.get("DESINTF").map(str::to_owned),
        designator_implementations: collect_indexed(params, "DESIMP", 0),
        is_trivial: params.get_bool("ISTRIVIAL"),
        common: common_record_from_params(params),
    }
}

pub fn map_definer_to_params(m: &MapDefiner, params: &mut ParameterMap) {
    params.insert("RECORD", "47");
    if let Some(d) = &m.designator_interface {
        params.insert("DESINTF", d.clone());
    }
    if !m.designator_implementations.is_empty() {
        params.insert(
            "DESIMPCOUNT",
            m.designator_implementations.len().to_string(),
        );
        for (i, imp) in m.designator_implementations.iter().enumerate() {
            params.insert(format!("DESIMP{i}").as_str(), imp.clone());
        }
    }
    if m.is_trivial {
        params.insert("ISTRIVIAL", "T");
    }
    write_common_to_params(params, &m.common);
}

fn collect_indexed(params: &ParameterMap, prefix: &str, start: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = start;
    loop {
        let key = format!("{prefix}{i}");
        match params.get(&key) {
            Some(v) => {
                out.push(v.to_string());
                i += 1;
            }
            None => break,
        }
    }
    out
}

fn common_record_from_params(params: &ParameterMap) -> PrimitiveCommon {
    PrimitiveCommon {
        owner_index: params.get_i32("OWNERINDEX").unwrap_or(0),
        is_not_accessible: params.get_bool("ISNOTACCESIBLE"),
        index_in_sheet: params.get_i32("INDEXINSHEET").unwrap_or(0),
        owner_part_id: params.get_i32("OWNERPARTID").unwrap_or(0),
        owner_part_display_mode: params.get_i32("OWNERPARTDISPLAYMODE").unwrap_or(0),
        graphically_locked: params.get_bool("GRAPHICALLYLOCKED"),
        disabled: params.get_bool("DISABLED"),
        dimmed: params.get_bool("DIMMED"),
        unique_id: params.get("UNIQUEID").map(str::to_owned),
    }
}

fn write_common_to_params(params: &mut ParameterMap, c: &PrimitiveCommon) {
    if c.owner_index != 0 {
        params.insert("OWNERINDEX", c.owner_index.to_string());
    }
    if c.is_not_accessible {
        params.insert("ISNOTACCESIBLE", "T");
    }
    if c.index_in_sheet != 0 {
        params.insert("INDEXINSHEET", c.index_in_sheet.to_string());
    }
    if c.owner_part_id != 0 {
        params.insert("OWNERPARTID", c.owner_part_id.to_string());
    }
    if c.owner_part_display_mode != 0 {
        params.insert(
            "OWNERPARTDISPLAYMODE",
            c.owner_part_display_mode.to_string(),
        );
    }
    if c.graphically_locked {
        params.insert("GRAPHICALLYLOCKED", "T");
    }
    if c.disabled {
        params.insert("DISABLED", "T");
    }
    if c.dimmed {
        params.insert("DIMMED", "T");
    }
    if let Some(uid) = &c.unique_id {
        params.insert("UNIQUEID", uid.clone());
    }
}
