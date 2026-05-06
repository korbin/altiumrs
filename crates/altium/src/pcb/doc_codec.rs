//! Parameter-block codec for PcbDoc-only storages (`Components6`, `Polygons6`,
//! `Rules6`, `Classes6`, `DifferentialPairs6`, `Rooms6`, `EmbeddedBoards6`,
//! `Nets6`).
//!
//! Coord values come in two forms: `"<n>mil"` with mil suffix, or a raw
//! integer (internal raw units). [`parse_coord_loose`] accepts either.

#![allow(clippy::field_reassign_with_default)]

use std::collections::BTreeMap;

use super::component::Component;
use super::embedded::EmbeddedBoard;
use super::polygon::{Polygon, PolygonVertex};
use super::primitives::Net;
use super::rule::{DifferentialPair, ObjectClass, Room, Rule};
use crate::coord::{Coord, CoordPoint};
use crate::parameter::ParameterMap;

/// Best-effort coord parser: accepts `"123mil"`, `"4.5mm"`, or a raw integer.
fn parse_coord_loose(s: &str) -> Option<Coord> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(c) = trimmed.parse::<Coord>() {
        return Some(c);
    }
    if let Ok(raw) = trimmed.parse::<i32>() {
        return Some(Coord::from_raw(raw));
    }
    None
}

fn get_coord_loose(params: &ParameterMap, key: &str) -> Option<Coord> {
    params.get(key).and_then(parse_coord_loose)
}

fn get_bool_explicit(params: &ParameterMap, key: &str) -> Option<bool> {
    params.get(key).map(|v| v.eq_ignore_ascii_case("TRUE"))
}

fn parse_layer(s: &str) -> i32 {
    if let Ok(n) = s.trim().parse::<i32>() {
        return n;
    }
    super::binary::layer_name_to_byte(s) as i32
}

fn format_mil_coord(c: Coord) -> String {
    format!("{:.4}mil", c.to_mils())
}

/// Inverse of [`super::binary::layer_name_to_byte`] for serialising
/// `EmbeddedBoard` records (which store the layer name, not byte).
fn layer_byte_to_name(layer: i32) -> String {
    match layer {
        1 => "TOP".into(),
        32 => "BOTTOM".into(),
        33 => "TOPOVERLAY".into(),
        34 => "BOTTOMOVERLAY".into(),
        35 => "TOPPASTE".into(),
        36 => "BOTTOMPASTE".into(),
        37 => "TOPSOLDER".into(),
        38 => "BOTTOMSOLDER".into(),
        55 => "DRILLGUIDE".into(),
        56 => "KEEPOUT".into(),
        73 => "DRILLDRAWING".into(),
        74 => "MULTILAYER".into(),
        n if (2..=31).contains(&n) => format!("MIDLAYER{}", n - 1),
        n if (39..=54).contains(&n) => format!("INTERNALPLANE{}", n - 38),
        n if (57..=72).contains(&n) => format!("MECHANICAL{}", n - 56),
        _ => layer.to_string(),
    }
}

// Net

pub fn net_from_params(params: &ParameterMap) -> Net {
    Net {
        name: params.get("NAME").unwrap_or_default().to_string(),
    }
}

pub fn net_to_params(net: &Net, params: &mut ParameterMap) {
    params.insert("NAME", net.name.clone());
}

// Component (parameter-form, used in PcbDoc Components6)

pub fn component_from_params(params: &ParameterMap) -> Component {
    let mut consumed: Vec<&str> = Vec::new();
    let mut c = Component::default();

    if let Some(v) = params.get("PATTERN") {
        c.name = v.to_string();
        c.pattern = Some(v.to_string());
        consumed.push("PATTERN");
    }
    if let Some(v) = params.get("DESCRIPTION") {
        c.description = Some(v.to_string());
        consumed.push("DESCRIPTION");
    }
    if let Some(v) = get_coord_loose(params, "HEIGHT") {
        c.height = v;
        consumed.push("HEIGHT");
    }
    if let Some(v) = params.get("COMMENT") {
        c.comment = Some(v.to_string());
        consumed.push("COMMENT");
    }
    if let Some(v) = get_coord_loose(params, "X") {
        c.x = v;
        consumed.push("X");
    }
    if let Some(v) = get_coord_loose(params, "Y") {
        c.y = v;
        consumed.push("Y");
    }
    if let Some(v) = params.get_f64("ROTATION") {
        c.rotation = v;
        consumed.push("ROTATION");
    }
    if let Some(v) = params.get("LAYER") {
        c.layer = parse_layer(v);
        consumed.push("LAYER");
    }

    if let Some(v) = get_bool_explicit(params, "COMMENTON") {
        c.comment_on = v;
        consumed.push("COMMENTON");
    }
    if let Some(v) = params.get_i32("COMMENTAUTOPOSITION") {
        c.comment_auto_position = v;
        consumed.push("COMMENTAUTOPOSITION");
    }
    if let Some(v) = get_bool_explicit(params, "NAMEON") {
        c.name_on = v;
        consumed.push("NAMEON");
    }
    if let Some(v) = params.get_i32("NAMEAUTOPOSITION") {
        c.name_auto_position = v;
        consumed.push("NAMEAUTOPOSITION");
    }
    if let Some(v) = get_bool_explicit(params, "LOCKSTRINGS") {
        c.lock_strings = v;
        consumed.push("LOCKSTRINGS");
    }

    if let Some(v) = params.get_i32("COMPONENTKIND") {
        c.component_kind = v;
        consumed.push("COMPONENTKIND");
    }
    if let Some(v) = get_bool_explicit(params, "ENABLED") {
        c.enabled = v;
        consumed.push("ENABLED");
    }
    if let Some(v) = get_bool_explicit(params, "FLIPPEDONLAYER") {
        c.flipped_on_layer = v;
        consumed.push("FLIPPEDONLAYER");
    }
    if let Some(v) = params.get_i32("GROUPNUM") {
        c.group_num = v;
        consumed.push("GROUPNUM");
    }
    if let Some(v) = get_bool_explicit(params, "ISBGA") {
        c.is_bga = v;
        consumed.push("ISBGA");
    }
    if let Some(v) = params.get_i32("CHANNELOFFSET") {
        c.channel_offset = v;
        consumed.push("CHANNELOFFSET");
    }

    for (key, target) in [
        ("FOOTPRINTDESCRIPTION", &mut c.footprint_description),
        ("SOURCEDESIGNATOR", &mut c.source_designator),
        ("SOURCELIBREFRENCE", &mut c.source_lib_reference),
        ("SOURCECOMPONENTLIBRARY", &mut c.source_component_library),
        ("SOURCEDESCRIPTION", &mut c.source_description),
        ("SOURCEFOOTPRINTLIBRARY", &mut c.source_footprint_library),
        ("SOURCEUNIQUEID", &mut c.source_unique_id),
        ("SOURCEHIERARCHICALPATH", &mut c.source_hierarchical_path),
        ("SOURCECOMPDESIGNITEMID", &mut c.source_comp_design_item_id),
        ("ITEMGUID", &mut c.item_guid),
        ("REVISIONGUID", &mut c.item_revision_guid),
        ("VAULTGUID", &mut c.vault_guid),
        ("UNIQUEID", &mut c.unique_id),
        ("MODELHASH", &mut c.model_hash),
        ("PACKAGESPECIFICHASH", &mut c.package_specific_hash),
        ("DEFAULTPCB3DMODEL", &mut c.default_pcb_3d_model),
    ] {
        if let Some(v) = params.get(key) {
            *target = Some(v.to_string());
            consumed.push(key);
        }
    }

    c.additional_parameters = collect_additional(params, &consumed);
    c
}

pub fn component_to_params(component: &Component, params: &mut ParameterMap) {
    for (k, v) in &component.additional_parameters {
        params.insert(k, v.clone());
    }
    params.insert("PATTERN", component.name.clone());
    if let Some(d) = &component.description {
        params.insert("DESCRIPTION", d.clone());
    }
    if component.height.to_raw() != 0 {
        params.insert("HEIGHT", component.height.to_raw().to_string());
    }
    if let Some(c) = &component.comment {
        params.insert("COMMENT", c.clone());
    }
    if component.x.to_raw() != 0 {
        params.insert("X", component.x.to_raw().to_string());
    }
    if component.y.to_raw() != 0 {
        params.insert("Y", component.y.to_raw().to_string());
    }
    if component.rotation != 0.0 {
        params.insert("ROTATION", component.rotation.to_string());
    }
    if component.layer != 0 {
        params.insert("LAYER", component.layer.to_string());
    }
    if component.comment_on {
        params.insert("COMMENTON", "TRUE");
    }
    if component.comment_auto_position != 0 {
        params.insert(
            "COMMENTAUTOPOSITION",
            component.comment_auto_position.to_string(),
        );
    }
    if component.name_on {
        params.insert("NAMEON", "TRUE");
    }
    if component.name_auto_position != 0 {
        params.insert("NAMEAUTOPOSITION", component.name_auto_position.to_string());
    }
    if component.lock_strings {
        params.insert("LOCKSTRINGS", "TRUE");
    }
    if component.component_kind != 0 {
        params.insert("COMPONENTKIND", component.component_kind.to_string());
    }
    if !component.enabled {
        params.insert("ENABLED", "FALSE");
    }
    if component.flipped_on_layer {
        params.insert("FLIPPEDONLAYER", "TRUE");
    }
    if component.group_num != 0 {
        params.insert("GROUPNUM", component.group_num.to_string());
    }
    if component.is_bga {
        params.insert("ISBGA", "TRUE");
    }
    for (key, value) in [
        ("SOURCEDESIGNATOR", component.source_designator.as_deref()),
        (
            "SOURCELIBREFRENCE",
            component.source_lib_reference.as_deref(),
        ),
        (
            "SOURCECOMPONENTLIBRARY",
            component.source_component_library.as_deref(),
        ),
        ("SOURCEDESCRIPTION", component.source_description.as_deref()),
        (
            "SOURCEFOOTPRINTLIBRARY",
            component.source_footprint_library.as_deref(),
        ),
        ("SOURCEUNIQUEID", component.source_unique_id.as_deref()),
        (
            "SOURCEHIERARCHICALPATH",
            component.source_hierarchical_path.as_deref(),
        ),
        (
            "SOURCECOMPDESIGNITEMID",
            component.source_comp_design_item_id.as_deref(),
        ),
        ("ITEMGUID", component.item_guid.as_deref()),
        ("REVISIONGUID", component.item_revision_guid.as_deref()),
        ("VAULTGUID", component.vault_guid.as_deref()),
        ("UNIQUEID", component.unique_id.as_deref()),
        ("MODELHASH", component.model_hash.as_deref()),
        (
            "PACKAGESPECIFICHASH",
            component.package_specific_hash.as_deref(),
        ),
        (
            "DEFAULTPCB3DMODEL",
            component.default_pcb_3d_model.as_deref(),
        ),
    ] {
        if let Some(v) = value {
            if !v.is_empty() {
                params.insert(key, v.to_string());
            }
        }
    }
}

// Polygon

pub fn polygon_from_params(params: &ParameterMap) -> Polygon {
    let mut consumed = Vec::<&str>::new();
    let mut p = Polygon::default();

    if let Some(v) = params.get("LAYER") {
        p.layer = parse_layer(v);
        consumed.push("LAYER");
    }
    if let Some(v) = params.get("NET") {
        p.net = Some(v.to_string());
        consumed.push("NET");
    }
    if let Some(v) = params.get("NAME") {
        p.name = Some(v.to_string());
        consumed.push("NAME");
    }
    if let Some(v) = params.get("UNIQUEID") {
        p.unique_id = Some(v.to_string());
        consumed.push("UNIQUEID");
    }
    if let Some(v) = params.get("POLYGONTYPE") {
        // Older files store this as a string ("Polygon", "PolygonCutout", "BoardOutline").
        p.polygon_type = match v.trim() {
            "" => 0,
            "Polygon" => 0,
            "PolygonCutout" | "Cutout" => 1,
            "BoardOutline" => 2,
            other => other.parse::<i32>().unwrap_or(0),
        };
        consumed.push("POLYGONTYPE");
    }

    if let Some(v) = params.get_i32("HATCHSTYLE") {
        p.poly_hatch_style = v;
    } else if let Some(v) = params.get_i32("POLYHATCHSTYLE") {
        p.poly_hatch_style = v;
        p.poly_hatch_uses_legacy_key = true;
    }
    consumed.extend(["HATCHSTYLE", "POLYHATCHSTYLE"]);

    if let Some(v) = params.get_i32("POURMODE") {
        p.pour_over = v;
    } else if let Some(v) = params.get_i32("POUROVER") {
        p.pour_over = v;
        p.pour_over_uses_legacy_key = true;
    }
    consumed.extend(["POURMODE", "POUROVER"]);

    if let Some(v) = get_bool_explicit(params, "REMOVEISLANDSBYAREA") {
        p.remove_islands_by_area = v;
    }
    if let Some(v) = params.get_i32("ISLANDAREATHRESHOLD") {
        p.island_area_threshold = v;
    }
    if let Some(v) = get_bool_explicit(params, "REMOVEDEAD") {
        p.remove_dead = v;
    }
    if let Some(v) = get_bool_explicit(params, "REMOVENECKS") {
        p.remove_narrow_necks = v;
    } else if let Some(v) = get_bool_explicit(params, "REMOVENARROWNECKS") {
        p.remove_narrow_necks = v;
        p.remove_necks_uses_legacy_key = true;
    }
    if let Some(v) = get_bool_explicit(params, "USEOCTAGONS") {
        p.use_octagons = v;
    }
    if let Some(v) = get_bool_explicit(params, "AVOIDOBST") {
        p.avoid_obstacles = v;
    } else if let Some(v) = get_bool_explicit(params, "AVOIDOBSTICLES") {
        p.avoid_obstacles = v;
        p.avoid_obstacles_uses_legacy_key = true;
    }
    consumed.extend([
        "REMOVEISLANDSBYAREA",
        "ISLANDAREATHRESHOLD",
        "REMOVEDEAD",
        "REMOVENECKS",
        "REMOVENARROWNECKS",
        "USEOCTAGONS",
        "AVOIDOBST",
        "AVOIDOBSTICLES",
    ]);

    for (key, target) in [
        ("GRIDSIZE", &mut p.grid),
        ("TRACKWIDTH", &mut p.track_size),
        ("MINPRIMLENGTH", &mut p.min_track),
        ("NECKWIDTH", &mut p.neck_width_threshold),
        ("ARCAPPROXIMATION", &mut p.arc_approximation),
        ("BORDERWIDTH", &mut p.border_width),
        ("SOLDERMASKEXPANSION", &mut p.solder_mask_expansion),
        ("PASTEMASKEXPANSION", &mut p.paste_mask_expansion),
        ("RELIEFAIRGAP", &mut p.relief_air_gap),
        ("RELIEFCONDUCTORWIDTH", &mut p.relief_conductor_width),
        ("POWERPLANECLEARANCE", &mut p.power_plane_clearance),
        (
            "POWERPLANERELIEFEXPANSION",
            &mut p.power_plane_relief_expansion,
        ),
    ] {
        if let Some(v) = get_coord_loose(params, key) {
            *target = v;
        }
        consumed.push(key);
    }

    if let Some(v) = params.get_i32("POURORDER") {
        p.pour_index = v;
    }
    if let Some(v) = params.get_i32("RELIEFENTRIES") {
        p.relief_entries = v;
    }
    if let Some(v) = params.get_i32("POWERPLANECONNECTSTYLE") {
        p.power_plane_connect_style = v;
    }
    if let Some(v) = params.get_i64("REPOURAREA") {
        p.area_size = v;
    }
    consumed.extend([
        "POURORDER",
        "RELIEFENTRIES",
        "POWERPLANECONNECTSTYLE",
        "REPOURAREA",
        "FLAGS",
    ]);

    let lock =
        get_bool_explicit(params, "PRIMITIVELOCK").or_else(|| get_bool_explicit(params, "LOCKED"));
    if let Some(v) = lock {
        p.primitive_lock = v;
    }
    if let Some(v) = get_bool_explicit(params, "SHELVED") {
        p.is_hidden = v;
    }
    if let Some(v) = get_bool_explicit(params, "POUROVERSAMENETPOLYGONS") {
        p.pour_over_same_net_polygons = v;
    }
    if let Some(v) = get_bool_explicit(params, "ENABLED") {
        p.enabled = v;
    }
    if let Some(v) = get_bool_explicit(params, "KEEPOUT") {
        p.is_keepout = v;
    }
    if let Some(v) = get_bool_explicit(params, "POLYGONOUTLINE") {
        p.polygon_outline = v;
    }
    if let Some(v) = get_bool_explicit(params, "POURED") {
        p.poured = v;
    }
    if let Some(v) = get_bool_explicit(params, "AUTOGENERATENAME") {
        p.auto_generate_name = v;
    }
    if let Some(v) = get_bool_explicit(params, "CLIPACUTECORNERS") {
        p.clip_acute_corners = v;
    }
    if let Some(v) = get_bool_explicit(params, "DRAWDEADCOPPER") {
        p.draw_dead_copper = v;
    }
    if let Some(v) = get_bool_explicit(params, "DRAWREMOVEDISLANDS") {
        p.draw_removed_islands = v;
    }
    if let Some(v) = get_bool_explicit(params, "DRAWREMOVEDNECKS") {
        p.draw_removed_necks = v;
    }
    if let Some(v) = get_bool_explicit(params, "EXPANDOUTLINE") {
        p.expand_outline = v;
    }
    if let Some(v) = get_bool_explicit(params, "IGNOREVIOLATIONS") {
        p.ignore_violations = v;
    }
    if let Some(v) = get_bool_explicit(params, "MITRECORNERS") {
        p.mitre_corners = v;
    }
    if let Some(v) = get_bool_explicit(params, "OBEYPOLYGONCUTOUT") {
        p.obey_polygon_cutout = v;
    }
    if let Some(v) = get_bool_explicit(params, "OPTIMALVOIDROTATION") {
        p.optimal_void_rotation = v;
    }
    if let Some(v) = get_bool_explicit(params, "ALLOWGLOBALEDIT") {
        p.allow_global_edit = v;
    }
    if let Some(v) = get_bool_explicit(params, "MOVEABLE") {
        p.moveable = v;
    }
    if let Some(v) = get_bool_explicit(params, "ARCPOURMODE") {
        p.arc_pour_mode = v;
    }
    consumed.extend([
        "PRIMITIVELOCK",
        "LOCKED",
        "SHELVED",
        "POUROVERSAMENETPOLYGONS",
        "ENABLED",
        "KEEPOUT",
        "POLYGONOUTLINE",
        "POURED",
        "AUTOGENERATENAME",
        "CLIPACUTECORNERS",
        "DRAWDEADCOPPER",
        "DRAWREMOVEDISLANDS",
        "DRAWREMOVEDNECKS",
        "EXPANDOUTLINE",
        "IGNOREVIOLATIONS",
        "MITRECORNERS",
        "OBEYPOLYGONCUTOUT",
        "OPTIMALVOIDROTATION",
        "ALLOWGLOBALEDIT",
        "MOVEABLE",
        "ARCPOURMODE",
    ]);

    // Vertex parsing — Altium uses two distinct formats depending on file age:
    //   New: POINTCOUNT + SA<i>.X / SA<i>.Y          (linear only, integer raw)
    //   Old: NV (or no count) + VX<i> / VY<i> / KIND<i> / CX<i> / CY<i> /
    //                            SA<i> / EA<i> / R<i> (arc-aware, "mil" coords)
    // We auto-detect format. Vertex count comes from POINTCOUNT, NV, or by
    // scanning until VX<i>/SA<i>.X stops being present.
    let new_form = params.contains_key("SA0.X") || params.contains_key("POINTCOUNT");
    let old_form = !new_form && (params.contains_key("VX0") || params.contains_key("KIND0"));

    let count = if new_form {
        params.get_i32("POINTCOUNT").unwrap_or_else(|| {
            // Scan SA<i>.X presence.
            let mut c = 0;
            while params.contains_key(&format!("SA{c}.X")) {
                c += 1;
            }
            c
        })
    } else if old_form {
        params.get_i32("NV").unwrap_or_else(|| {
            let mut c = 0;
            while params.contains_key(&format!("VX{c}")) || params.contains_key(&format!("KIND{c}"))
            {
                c += 1;
            }
            c
        })
    } else {
        params
            .get_i32("NV")
            .or(params.get_i32("POINTCOUNT"))
            .unwrap_or(0)
    };

    if count > 0 {
        p.point_count = count;
        if new_form {
            for i in 0..count {
                let kx = format!("SA{i}.X");
                let ky = format!("SA{i}.Y");
                let x = params.get_i32(&kx).unwrap_or(0);
                let y = params.get_i32(&ky).unwrap_or(0);
                p.vertices.push(PolygonVertex::linear(CoordPoint::new(
                    Coord::from_raw(x),
                    Coord::from_raw(y),
                )));
            }
        } else if old_form {
            for i in 0..count {
                let kind = params.get_i32(&format!("KIND{i}")).unwrap_or(0);
                let vx = params
                    .get(&format!("VX{i}"))
                    .and_then(parse_coord_loose)
                    .unwrap_or(Coord::ZERO);
                let vy = params
                    .get(&format!("VY{i}"))
                    .and_then(parse_coord_loose)
                    .unwrap_or(Coord::ZERO);
                let cx = params
                    .get(&format!("CX{i}"))
                    .and_then(parse_coord_loose)
                    .unwrap_or(Coord::ZERO);
                let cy = params
                    .get(&format!("CY{i}"))
                    .and_then(parse_coord_loose)
                    .unwrap_or(Coord::ZERO);
                let sa = params.get_f64(&format!("SA{i}")).unwrap_or(0.0);
                let ea = params.get_f64(&format!("EA{i}")).unwrap_or(0.0);
                let r = params
                    .get(&format!("R{i}"))
                    .and_then(parse_coord_loose)
                    .unwrap_or(Coord::ZERO);
                p.vertices.push(PolygonVertex {
                    point: CoordPoint::new(vx, vy),
                    kind,
                    arc_center: CoordPoint::new(cx, cy),
                    start_angle: sa,
                    end_angle: ea,
                    radius: r,
                });
            }
        }
    }
    consumed.extend(["POINTCOUNT", "NV"]);
    // Mark per-vertex keys as consumed regardless of which format we read.
    let consumed_vertex_keys: Vec<String> = (0..count)
        .flat_map(|i| {
            [
                format!("SA{i}.X"),
                format!("SA{i}.Y"),
                format!("VX{i}"),
                format!("VY{i}"),
                format!("KIND{i}"),
                format!("CX{i}"),
                format!("CY{i}"),
                format!("SA{i}"),
                format!("EA{i}"),
                format!("R{i}"),
            ]
        })
        .collect();
    let consumed_vertex_refs: Vec<&str> = consumed_vertex_keys.iter().map(|s| s.as_str()).collect();

    let mut all_consumed: Vec<&str> = consumed.clone();
    all_consumed.extend(consumed_vertex_refs);
    p.additional_parameters = collect_additional_optional(params, &all_consumed);
    p
}

pub fn polygon_to_params(p: &Polygon, params: &mut ParameterMap) {
    if let Some(extra) = &p.additional_parameters {
        for (k, v) in extra {
            params.insert(k, v.clone());
        }
    }
    params.insert("LAYER", p.layer.to_string());
    params.insert("NET", p.net.clone().unwrap_or_default());
    params.insert("POLYGONTYPE", p.polygon_type.to_string());
    if let Some(name) = &p.name {
        if !name.is_empty() {
            params.insert("NAME", name.clone());
        }
    }
    if let Some(uid) = &p.unique_id {
        if !uid.is_empty() {
            params.insert("UNIQUEID", uid.clone());
        }
    }
    params.insert(
        if p.poly_hatch_uses_legacy_key {
            "POLYHATCHSTYLE"
        } else {
            "HATCHSTYLE"
        },
        p.poly_hatch_style.to_string(),
    );
    params.insert(
        if p.pour_over_uses_legacy_key {
            "POUROVER"
        } else {
            "POURMODE"
        },
        p.pour_over.to_string(),
    );
    params.insert(
        "REMOVEISLANDSBYAREA",
        if p.remove_islands_by_area {
            "TRUE"
        } else {
            "FALSE"
        },
    );
    params.insert("ISLANDAREATHRESHOLD", p.island_area_threshold.to_string());
    params.insert("REMOVEDEAD", if p.remove_dead { "TRUE" } else { "FALSE" });
    params.insert(
        if p.remove_necks_uses_legacy_key {
            "REMOVENARROWNECKS"
        } else {
            "REMOVENECKS"
        },
        if p.remove_narrow_necks {
            "TRUE"
        } else {
            "FALSE"
        },
    );
    params.insert("USEOCTAGONS", if p.use_octagons { "TRUE" } else { "FALSE" });
    params.insert(
        if p.avoid_obstacles_uses_legacy_key {
            "AVOIDOBSTICLES"
        } else {
            "AVOIDOBST"
        },
        if p.avoid_obstacles { "TRUE" } else { "FALSE" },
    );

    for (key, val) in [
        ("GRIDSIZE", p.grid),
        ("TRACKWIDTH", p.track_size),
        ("MINPRIMLENGTH", p.min_track),
        ("NECKWIDTH", p.neck_width_threshold),
        ("ARCAPPROXIMATION", p.arc_approximation),
        ("BORDERWIDTH", p.border_width),
        ("SOLDERMASKEXPANSION", p.solder_mask_expansion),
        ("PASTEMASKEXPANSION", p.paste_mask_expansion),
        ("RELIEFAIRGAP", p.relief_air_gap),
        ("RELIEFCONDUCTORWIDTH", p.relief_conductor_width),
        ("POWERPLANECLEARANCE", p.power_plane_clearance),
        ("POWERPLANERELIEFEXPANSION", p.power_plane_relief_expansion),
    ] {
        if val.to_raw() != 0 {
            params.insert(key, val.to_raw().to_string());
        }
    }

    if p.pour_index != 0 {
        params.insert("POURORDER", p.pour_index.to_string());
    }
    if p.relief_entries != 0 {
        params.insert("RELIEFENTRIES", p.relief_entries.to_string());
    }
    if p.power_plane_connect_style != 0 {
        params.insert(
            "POWERPLANECONNECTSTYLE",
            p.power_plane_connect_style.to_string(),
        );
    }
    if p.area_size != 0 {
        params.insert("REPOURAREA", p.area_size.to_string());
    }

    if p.primitive_lock {
        params.insert("PRIMITIVELOCK", "TRUE");
    }
    if p.is_hidden {
        params.insert("SHELVED", "TRUE");
    }
    if p.pour_over_same_net_polygons {
        params.insert("POUROVERSAMENETPOLYGONS", "TRUE");
    }
    if !p.enabled {
        params.insert("ENABLED", "FALSE");
    }
    if p.is_keepout {
        params.insert("KEEPOUT", "TRUE");
    }
    if p.polygon_outline {
        params.insert("POLYGONOUTLINE", "TRUE");
    }
    if p.poured {
        params.insert("POURED", "TRUE");
    }
    if p.auto_generate_name {
        params.insert("AUTOGENERATENAME", "TRUE");
    }
    if p.clip_acute_corners {
        params.insert("CLIPACUTECORNERS", "TRUE");
    }
    if p.draw_dead_copper {
        params.insert("DRAWDEADCOPPER", "TRUE");
    }
    if p.draw_removed_islands {
        params.insert("DRAWREMOVEDISLANDS", "TRUE");
    }
    if p.draw_removed_necks {
        params.insert("DRAWREMOVEDNECKS", "TRUE");
    }
    if p.expand_outline {
        params.insert("EXPANDOUTLINE", "TRUE");
    }
    if p.ignore_violations {
        params.insert("IGNOREVIOLATIONS", "TRUE");
    }
    if p.mitre_corners {
        params.insert("MITRECORNERS", "TRUE");
    }
    if p.obey_polygon_cutout {
        params.insert("OBEYPOLYGONCUTOUT", "TRUE");
    }
    if p.optimal_void_rotation {
        params.insert("OPTIMALVOIDROTATION", "TRUE");
    }
    if p.allow_global_edit {
        params.insert("ALLOWGLOBALEDIT", "TRUE");
    }
    if p.moveable {
        params.insert("MOVEABLE", "TRUE");
    }
    if p.arc_pour_mode {
        params.insert("ARCPOURMODE", "TRUE");
    }

    params.insert("POINTCOUNT", p.vertices.len().to_string());
    for (i, v) in p.vertices.iter().enumerate() {
        params.insert(format!("SA{i}.X").as_str(), v.point.x.to_raw().to_string());
        params.insert(format!("SA{i}.Y").as_str(), v.point.y.to_raw().to_string());
    }
}

// Rule

pub fn rule_from_params(params: &ParameterMap) -> Rule {
    let mut r = Rule::default();
    let mut typed = BTreeMap::new();
    for (n, v, _) in params.iter() {
        typed.insert(n.to_string(), v.to_string());
    }
    if let Some(v) = params.get("NAME") {
        r.name = v.to_string();
    }
    if let Some(v) = params.get("RULEKIND") {
        r.rule_kind = v.to_string();
    }
    if let Some(v) = params.get("COMMENT") {
        r.comment = v.to_string();
    }
    if let Some(v) = params.get("UNIQUEID") {
        r.unique_id = v.to_string();
    }
    if let Some(v) = get_bool_explicit(params, "ENABLED") {
        r.enabled = v;
    }
    if let Some(v) = params.get_i32("PRIORITY") {
        r.priority = v;
    }
    if let Some(v) = params.get("SCOPE1EXPRESSION") {
        r.scope1_expression = v.to_string();
    }
    if let Some(v) = params.get("SCOPE2EXPRESSION") {
        r.scope2_expression = v.to_string();
    }
    r.parameters = typed;
    r
}

pub fn rule_to_params(r: &Rule, params: &mut ParameterMap) {
    for (k, v) in &r.parameters {
        params.insert(k, v.clone());
    }
    // Typed fields override.
    if !r.name.is_empty() {
        params.insert("NAME", r.name.clone());
    }
    if !r.rule_kind.is_empty() {
        params.insert("RULEKIND", r.rule_kind.clone());
    }
    if !r.comment.is_empty() {
        params.insert("COMMENT", r.comment.clone());
    }
    if !r.unique_id.is_empty() {
        params.insert("UNIQUEID", r.unique_id.clone());
    }
    params.insert("ENABLED", if r.enabled { "TRUE" } else { "FALSE" });
    if r.priority != 0 {
        params.insert("PRIORITY", r.priority.to_string());
    }
    if !r.scope1_expression.is_empty() {
        params.insert("SCOPE1EXPRESSION", r.scope1_expression.clone());
    }
    if !r.scope2_expression.is_empty() {
        params.insert("SCOPE2EXPRESSION", r.scope2_expression.clone());
    }
}

// ObjectClass

pub fn object_class_from_params(params: &ParameterMap) -> ObjectClass {
    let mut o = ObjectClass::default();
    let mut typed = BTreeMap::new();
    for (n, v, _) in params.iter() {
        typed.insert(n.to_string(), v.to_string());
    }
    if let Some(v) = params.get("NAME") {
        o.name = v.to_string();
    }
    if let Some(v) = params.get("SUPERCLASS") {
        o.super_class = v.to_string();
    }
    if let Some(v) = params.get("SUBCLASS") {
        o.sub_class = v.to_string();
    }
    if let Some(v) = params.get("UNIQUEID") {
        o.unique_id = v.to_string();
    }
    if let Some(v) = params.get("KIND") {
        o.kind = v.to_string();
    }
    if let Some(v) = get_bool_explicit(params, "ENABLED") {
        o.enabled = v;
    }
    let mut i = 0;
    loop {
        let key = format!("MEMBER{i}");
        match params.get(&key) {
            Some(v) => {
                o.members.push(v.to_string());
                i += 1;
            }
            None => break,
        }
    }
    o.parameters = typed;
    o
}

pub fn object_class_to_params(o: &ObjectClass, params: &mut ParameterMap) {
    for (k, v) in &o.parameters {
        params.insert(k, v.clone());
    }
    if !o.name.is_empty() {
        params.insert("NAME", o.name.clone());
    }
    if !o.super_class.is_empty() {
        params.insert("SUPERCLASS", o.super_class.clone());
    }
    if !o.sub_class.is_empty() {
        params.insert("SUBCLASS", o.sub_class.clone());
    }
    if !o.unique_id.is_empty() {
        params.insert("UNIQUEID", o.unique_id.clone());
    }
    if !o.kind.is_empty() {
        params.insert("KIND", o.kind.clone());
    }
    params.insert("ENABLED", if o.enabled { "TRUE" } else { "FALSE" });
    for (i, m) in o.members.iter().enumerate() {
        params.insert(format!("MEMBER{i}").as_str(), m.clone());
    }
}

// DifferentialPair

pub fn differential_pair_from_params(params: &ParameterMap) -> DifferentialPair {
    let mut d = DifferentialPair::default();
    let mut typed = BTreeMap::new();
    for (n, v, _) in params.iter() {
        typed.insert(n.to_string(), v.to_string());
    }
    if let Some(v) = params.get("NAME") {
        d.name = v.to_string();
    }
    if let Some(v) = params.get("POSITIVENETNAME") {
        d.positive_net_name = v.to_string();
    }
    if let Some(v) = params.get("NEGATIVENETNAME") {
        d.negative_net_name = v.to_string();
    }
    if let Some(v) = params.get("UNIQUEID") {
        d.unique_id = v.to_string();
    }
    if let Some(v) = get_bool_explicit(params, "ENABLED") {
        d.enabled = v;
    }
    d.parameters = typed;
    d
}

pub fn differential_pair_to_params(d: &DifferentialPair, params: &mut ParameterMap) {
    for (k, v) in &d.parameters {
        params.insert(k, v.clone());
    }
    if !d.name.is_empty() {
        params.insert("NAME", d.name.clone());
    }
    if !d.positive_net_name.is_empty() {
        params.insert("POSITIVENETNAME", d.positive_net_name.clone());
    }
    if !d.negative_net_name.is_empty() {
        params.insert("NEGATIVENETNAME", d.negative_net_name.clone());
    }
    if !d.unique_id.is_empty() {
        params.insert("UNIQUEID", d.unique_id.clone());
    }
    params.insert("ENABLED", if d.enabled { "TRUE" } else { "FALSE" });
}

// Room

pub fn room_from_params(params: &ParameterMap) -> Room {
    let mut r = Room::default();
    let mut typed = BTreeMap::new();
    for (n, v, _) in params.iter() {
        typed.insert(n.to_string(), v.to_string());
    }
    if let Some(v) = params.get("NAME") {
        r.name = v.to_string();
    }
    if let Some(v) = params.get("UNIQUEID") {
        r.unique_id = v.to_string();
    }
    r.parameters = typed;
    r
}

pub fn room_to_params(r: &Room, params: &mut ParameterMap) {
    for (k, v) in &r.parameters {
        params.insert(k, v.clone());
    }
    if !r.name.is_empty() {
        params.insert("NAME", r.name.clone());
    }
    if !r.unique_id.is_empty() {
        params.insert("UNIQUEID", r.unique_id.clone());
    }
}

// EmbeddedBoard

pub fn embedded_board_from_params(params: &ParameterMap) -> EmbeddedBoard {
    let mut b = EmbeddedBoard::default();
    if let Some(v) = params.get("DOCUMENTPATH") {
        b.document_path = Some(v.to_string());
    }
    if let Some(v) = params.get("VIEWPORTTITLE") {
        b.viewport_title = Some(v.to_string());
    }
    if let Some(v) = params.get("FONTNAME") {
        b.title_font_name = Some(v.to_string());
    }
    if let Some(v) = params.get("LAYER") {
        b.layer = parse_layer(v);
    }
    if let Some(v) = params.get_f64("ROTATION") {
        b.rotation = v;
    }
    if let Some(v) = params.get_f64("VIEWPORTSCALE") {
        b.scale = v;
    }
    if let Some(v) = get_bool_explicit(params, "MIRROR") {
        b.mirror_flag = v;
    }
    if let Some(v) = get_bool_explicit(params, "KEEPOUT") {
        b.is_keepout = v;
    }
    if let Some(v) = get_bool_explicit(params, "POLYGONOUTLINE") {
        b.polygon_outline = v;
    }
    if let Some(v) = get_bool_explicit(params, "USERROUTED") {
        b.user_routed = v;
    }
    if let Some(v) = get_bool_explicit(params, "ISVIEWPORT") {
        b.is_viewport = v;
    }
    if let Some(v) = get_bool_explicit(params, "VIEWPORTVISIBLE") {
        b.viewport_visible = v;
    }
    if let Some(v) = params.get_i32("ORIGINMODE") {
        b.origin_mode = v;
    }
    if let Some(v) = params.get_i32("COLCOUNT") {
        b.col_count = v;
    }
    if let Some(v) = params.get_i32("ROWCOUNT") {
        b.row_count = v;
    }
    if let Some(v) = params.get_i32("UNIONINDEX") {
        b.union_index = v;
    }
    if let Some(v) = params.get_i32("FONTSIZE") {
        b.title_font_size = v;
    }
    if let Some(v) = params.get_i32("FONTCOLOR") {
        b.title_font_color = v;
    }
    if let Some(v) = get_coord_loose(params, "X1") {
        b.x1_location = v;
    }
    if let Some(v) = get_coord_loose(params, "Y1") {
        b.y1_location = v;
    }
    if let Some(v) = get_coord_loose(params, "X2") {
        b.x2_location = v;
    }
    if let Some(v) = get_coord_loose(params, "Y2") {
        b.y2_location = v;
    }
    if let Some(v) = get_coord_loose(params, "COLSPACING") {
        b.col_spacing = v;
    }
    if let Some(v) = get_coord_loose(params, "ROWSPACING") {
        b.row_spacing = v;
    }
    b
}

pub fn embedded_board_to_params(b: &EmbeddedBoard, params: &mut ParameterMap) {
    params.insert("SELECTION", "FALSE");
    params.insert("LOCKED", "FALSE");
    params.insert(
        "POLYGONOUTLINE",
        if b.polygon_outline { "TRUE" } else { "FALSE" },
    );
    params.insert("USERROUTED", if b.user_routed { "TRUE" } else { "FALSE" });
    params.insert("KEEPOUT", if b.is_keepout { "TRUE" } else { "FALSE" });
    params.insert("MIRROR", if b.mirror_flag { "TRUE" } else { "FALSE" });
    params.insert("LAYER", layer_byte_to_name(b.layer));
    params.insert("UNIONINDEX", b.union_index.to_string());
    if b.origin_mode != 0 {
        params.insert("ORIGINMODE", b.origin_mode.to_string());
    }
    params.insert("COLCOUNT", b.col_count.to_string());
    params.insert("ROWCOUNT", b.row_count.to_string());
    params.insert("X1", format_mil_coord(b.x1_location));
    params.insert("Y1", format_mil_coord(b.y1_location));
    params.insert("X2", format_mil_coord(b.x2_location));
    params.insert("Y2", format_mil_coord(b.y2_location));
    params.insert("X", format_mil_coord(b.x1_location));
    params.insert("Y", format_mil_coord(b.y1_location));
    params.insert("COLSPACING", format_mil_coord(b.col_spacing));
    params.insert("ROWSPACING", format_mil_coord(b.row_spacing));
    params.insert("ROTATION", format!(" {:E}", b.rotation));
    params.insert("ISVIEWPORT", if b.is_viewport { "TRUE" } else { "FALSE" });
    params.insert(
        "VIEWPORTVISIBLE",
        if b.viewport_visible { "TRUE" } else { "FALSE" },
    );
    if let Some(t) = &b.viewport_title {
        if !t.is_empty() {
            params.insert("VIEWPORTTITLE", t.clone());
        }
    }
    if b.scale != 0.0 {
        params.insert("VIEWPORTSCALE", format!("{:.3}", b.scale));
    }
    if let Some(n) = &b.title_font_name {
        if !n.is_empty() {
            params.insert("FONTNAME", n.clone());
        }
    }
    if b.title_font_size != 0 {
        params.insert("FONTSIZE", b.title_font_size.to_string());
    }
    if b.title_font_color != 0 {
        params.insert("FONTCOLOR", b.title_font_color.to_string());
    }
    if let Some(d) = &b.document_path {
        if !d.is_empty() {
            params.insert("DOCUMENTPATH", d.clone());
        }
    }
}

// Helpers

fn collect_additional(params: &ParameterMap, consumed: &[&str]) -> BTreeMap<String, String> {
    let mut consumed_upper: Vec<String> = consumed.iter().map(|s| s.to_ascii_uppercase()).collect();
    consumed_upper.sort();
    let mut out = BTreeMap::new();
    for (n, v, _) in params.iter() {
        if consumed_upper
            .binary_search(&n.to_ascii_uppercase())
            .is_err()
        {
            out.insert(n.to_string(), v.to_string());
        }
    }
    out
}

fn collect_additional_optional(
    params: &ParameterMap,
    consumed: &[&str],
) -> Option<BTreeMap<String, String>> {
    let mut consumed_upper: Vec<String> = consumed.iter().map(|s| s.to_ascii_uppercase()).collect();
    consumed_upper.sort();
    let mut additional = BTreeMap::new();
    for (n, v, _) in params.iter() {
        if n.starts_with("SA") && (n.ends_with(".X") || n.ends_with(".Y")) {
            continue;
        }
        if consumed_upper
            .binary_search(&n.to_ascii_uppercase())
            .is_err()
        {
            additional.insert(n.to_string(), v.to_string());
        }
    }
    if additional.is_empty() {
        None
    } else {
        Some(additional)
    }
}
