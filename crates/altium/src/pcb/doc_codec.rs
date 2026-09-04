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
pub(crate) fn parse_coord_loose(s: &str) -> Option<Coord> {
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

pub(crate) fn format_mil_coord(c: Coord) -> String {
    // Altium writes "0mil", "0.5mil", "56.2992mil": up to four decimals with
    // trailing zeros (and a bare point) trimmed.
    let s = format!("{:.4}", c.to_mils());
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{}mil", if s.is_empty() || s == "-" { "0" } else { s })
}

/// Inverse of [`super::binary::layer_name_to_byte`] for param records that
/// store layer names — the short dialect Altium emits in these records
/// ("MID2", "PLANE1"), not the long stream names.
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
        n if (2..=31).contains(&n) => format!("MID{}", n - 1),
        n if (39..=54).contains(&n) => format!("PLANE{}", n - 38),
        n if (57..=72).contains(&n) => format!("MECHANICAL{}", n - 56),
        _ => layer.to_string(),
    }
}

// Net

pub fn net_from_params(params: &ParameterMap) -> Net {
    let mut parameters = Vec::new();
    for (n, v, _) in params.iter() {
        parameters.push((n.to_string(), v.to_string()));
    }
    Net {
        name: params.get("NAME").unwrap_or_default().to_string(),
        parameters,
    }
}

pub fn net_to_params(net: &Net, params: &mut ParameterMap) {
    for (k, v) in &net.parameters {
        params.insert(k, v.clone());
    }
    params.insert("NAME", net.name.clone());
}

// Component (parameter-form, used in PcbDoc Components6)

pub fn component_from_params(params: &ParameterMap) -> Component {
    let mut consumed: Vec<&str> = Vec::new();
    let mut c = Component::default();
    // Altium omits ENABLED; absence means enabled.
    c.enabled = true;

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
    }
    if let Some(v) = get_bool_explicit(params, "NAMEON") {
        c.name_on = v;
        consumed.push("NAMEON");
    }
    if let Some(v) = params.get_i32("NAMEAUTOPOSITION") {
        c.name_auto_position = v;
    }
    if let Some(v) = get_bool_explicit(params, "LOCKSTRINGS") {
        c.lock_strings = v;
        consumed.push("LOCKSTRINGS");
    }

    // Component type. Modern files store it in COMPONENTKINDVERSION2
    // (TComponentKind ordinals; 5 = Standard (No BOM)) while keeping a
    // legacy COMPONENTKIND alongside — the V2 key is authoritative. Both
    // pass through `additional_parameters` verbatim.
    if let Some(v) = params.get_i32("COMPONENTKIND") {
        c.component_kind = v;
    }
    if let Some(v) = params.get_i32("COMPONENTKINDVERSION2") {
        c.component_kind = v;
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
    }
    if let Some(v) = get_bool_explicit(params, "ISBGA") {
        c.is_bga = v;
        consumed.push("ISBGA");
    }
    if let Some(v) = params.get_i32("CHANNELOFFSET") {
        c.channel_offset = v;
    }

    // Same for FOOTPRINTDESCRIPTION: read into the typed field, carried by
    // passthrough.
    if let Some(v) = params.get("FOOTPRINTDESCRIPTION") {
        c.footprint_description = Some(v.to_string());
    }

    for (key, target) in [
        ("SOURCEDESIGNATOR", &mut c.source_designator),
        ("SOURCELIBREFERENCE", &mut c.source_lib_reference),
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
    // Coords as mil strings; always-present toggles written even when
    // FALSE (a dropped NAMEON=FALSE flips designator labels visible).
    if component.height.to_raw() != 0 {
        params.insert("HEIGHT", format_mil_coord(component.height));
    }
    if let Some(c) = &component.comment {
        params.insert("COMMENT", c.clone());
    }
    params.insert("X", format_mil_coord(component.x));
    params.insert("Y", format_mil_coord(component.y));
    params.insert("ROTATION", component.rotation.to_string());
    params.insert("LAYER", layer_byte_to_name(component.layer));
    params.insert(
        "COMMENTON",
        if component.comment_on {
            "TRUE"
        } else {
            "FALSE"
        },
    );
    params.insert("NAMEON", if component.name_on { "TRUE" } else { "FALSE" });
    // Autopositions, GROUPNUM, and CHANNELOFFSET travel via passthrough;
    // only non-default typed values are (re)written.
    if component.comment_auto_position != 0 {
        params.insert(
            "COMMENTAUTOPOSITION",
            component.comment_auto_position.to_string(),
        );
    }
    if component.name_auto_position != 0 {
        params.insert("NAMEAUTOPOSITION", component.name_auto_position.to_string());
    }
    if component.group_num != 0 {
        params.insert("GROUPNUM", component.group_num.to_string());
    }
    if component.channel_offset != 0 {
        params.insert("CHANNELOFFSET", component.channel_offset.to_string());
    }
    if component.lock_strings {
        params.insert("LOCKSTRINGS", "TRUE");
    }
    // File-loaded components carry their kind keys via passthrough; only
    // from-scratch ones need them derived (legacy key knows 0..2 only).
    if component.component_kind != 0
        && !params.contains_key("COMPONENTKIND")
        && !params.contains_key("COMPONENTKINDVERSION2")
    {
        let legacy = match component.component_kind {
            1 | 2 => component.component_kind,
            _ => 0,
        };
        params.insert("COMPONENTKIND", legacy.to_string());
        params.insert(
            "COMPONENTKINDVERSION2",
            component.component_kind.to_string(),
        );
    }
    if !component.enabled {
        params.insert("ENABLED", "FALSE");
    }
    if component.flipped_on_layer {
        params.insert("FLIPPEDONLAYER", "TRUE");
    }
    if component.is_bga {
        params.insert("ISBGA", "TRUE");
    }
    for (key, value) in [
        ("SOURCEDESIGNATOR", component.source_designator.as_deref()),
        (
            "SOURCELIBREFERENCE",
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
    // Only writer-derived keys (layer, net, name, id, vertices) are
    // consumed; everything else is read into typed fields but also passes
    // through `additional_parameters` verbatim.
    let mut consumed = Vec::<&str>::new();
    let mut p = Polygon::default();
    // Altium omits ENABLED; absence means enabled.
    p.enabled = true;

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
        // Stored as a string in real files.
        p.polygon_type = match v.trim() {
            "" => 0,
            "Polygon" => 0,
            "PolygonCutout" | "Cutout" => 1,
            "BoardOutline" => 2,
            "Split Plane" => 3,
            other => other.parse::<i32>().unwrap_or(0),
        };
    }

    // HATCHSTYLE is a string in real files ("Solid", "45Degree", …); the
    // numeric forms come from older third-party writers.
    if let Some(v) = params.get("HATCHSTYLE") {
        p.poly_hatch_style = match v.trim() {
            "None" => 0,
            "Solid" => 1,
            "45Degree" => 2,
            "90Degree" => 3,
            "Horizontal" => 4,
            "Vertical" => 5,
            other => other.parse::<i32>().unwrap_or(0),
        };
    } else if let Some(v) = params.get_i32("POLYHATCHSTYLE") {
        p.poly_hatch_style = v;
        p.poly_hatch_uses_legacy_key = true;
    }

    // POURMODE is numeric; the legacy POUROVER key is a bool.
    if let Some(v) = params.get_i32("POURMODE") {
        p.pour_over = v;
    } else if let Some(v) = params.get_i32("POUROVER") {
        p.pour_over = v;
        p.pour_over_uses_legacy_key = true;
    } else if let Some(v) = get_bool_explicit(params, "POUROVER") {
        p.pour_over = i32::from(v);
        p.pour_over_uses_legacy_key = true;
    }

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

    // LOCKED and PRIMITIVELOCK are distinct flags in real records — only
    // PRIMITIVELOCK maps to `primitive_lock`; LOCKED passes through.
    if let Some(v) = get_bool_explicit(params, "PRIMITIVELOCK") {
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

    // Vertex parsing — Altium uses two distinct formats depending on file age:
    //   New: POINTCOUNT + SA<i>.X / SA<i>.Y          (linear only, integer raw)
    //   Old: NV (or no count) + VX<i> / VY<i> / KIND<i> / CX<i> / CY<i> /
    //                            SA<i> / EA<i> / R<i> (arc-aware, "mil" coords)
    // We auto-detect format. Vertex count comes from POINTCOUNT, NV, or by
    // scanning until VX<i>/SA<i>.X stops being present.
    let new_form = params.contains_key("SA0.X") || params.contains_key("POINTCOUNT");
    let old_form = !new_form && (params.contains_key("VX0") || params.contains_key("KIND0"));
    p.vertices_use_legacy_form = old_form;

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
    // File-loaded polygons carry their settings in `additional_parameters`
    // (see `polygon_from_params`); only from-scratch polygons need them
    // derived from typed fields.
    let from_scratch = p.additional_parameters.is_none();
    if let Some(extra) = &p.additional_parameters {
        for (k, v) in extra {
            params.insert(k, v.clone());
        }
    }
    params.insert("LAYER", layer_byte_to_name(p.layer));
    params.insert("NET", p.net.clone().unwrap_or_default());
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

    if from_scratch {
        params.insert(
            "POLYGONTYPE",
            match p.polygon_type {
                1 => "Cutout",
                2 => "BoardOutline",
                3 => "Split Plane",
                _ => "Polygon",
            },
        );
        params.insert(
            "HATCHSTYLE",
            match p.poly_hatch_style {
                0 => "None",
                2 => "45Degree",
                3 => "90Degree",
                4 => "Horizontal",
                5 => "Vertical",
                _ => "Solid",
            },
        );
        params.insert("POUROVER", if p.pour_over != 0 { "TRUE" } else { "FALSE" });
        params.insert(
            "REMOVEISLANDSBYAREA",
            if p.remove_islands_by_area {
                "TRUE"
            } else {
                "FALSE"
            },
        );
        params.insert("REMOVEDEAD", if p.remove_dead { "TRUE" } else { "FALSE" });
        params.insert(
            "REMOVENECKS",
            if p.remove_narrow_necks {
                "TRUE"
            } else {
                "FALSE"
            },
        );
        params.insert("USEOCTAGONS", if p.use_octagons { "TRUE" } else { "FALSE" });
        params.insert(
            "AVOIDOBST",
            if p.avoid_obstacles { "TRUE" } else { "FALSE" },
        );
        params.insert("LOCKED", "FALSE");
        params.insert("KEEPOUT", if p.is_keepout { "TRUE" } else { "FALSE" });
        params.insert(
            "POLYGONOUTLINE",
            if p.polygon_outline { "TRUE" } else { "FALSE" },
        );
        params.insert(
            "PRIMITIVELOCK",
            if p.primitive_lock { "TRUE" } else { "FALSE" },
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
                params.insert(key, format_mil_coord(val));
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
        if p.is_hidden {
            params.insert("SHELVED", "TRUE");
        }
        if !p.enabled {
            params.insert("ENABLED", "FALSE");
        }
    }

    // Vertices go back out in the form they came in: the legacy arc-aware
    // KIND/VX/VY/CX/CY/SA/EA/R keys, or the newer POINTCOUNT + SA<i>.X/Y.
    if p.vertices_use_legacy_form {
        for (i, v) in p.vertices.iter().enumerate() {
            params.insert(format!("KIND{i}").as_str(), v.kind.to_string());
            params.insert(format!("VX{i}").as_str(), format_mil_coord(v.point.x));
            params.insert(format!("VY{i}").as_str(), format_mil_coord(v.point.y));
            params.insert(format!("CX{i}").as_str(), format_mil_coord(v.arc_center.x));
            params.insert(format!("CY{i}").as_str(), format_mil_coord(v.arc_center.y));
            params.insert(format!("SA{i}").as_str(), v.start_angle.to_string());
            params.insert(format!("EA{i}").as_str(), v.end_angle.to_string());
            params.insert(format!("R{i}").as_str(), format_mil_coord(v.radius));
        }
    } else {
        params.insert("POINTCOUNT", p.vertices.len().to_string());
        for (i, v) in p.vertices.iter().enumerate() {
            params.insert(format!("SA{i}.X").as_str(), v.point.x.to_raw().to_string());
            params.insert(format!("SA{i}.Y").as_str(), v.point.y.to_raw().to_string());
        }
    }
}

// Rule

pub fn rule_from_params(params: &ParameterMap) -> Rule {
    let mut r = Rule::default();
    let mut typed: Vec<(String, String)> = Vec::new();
    for (n, v, _) in params.iter() {
        typed.push((n.to_string(), v.to_string()));
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
    // Typed fields only shadow the original value when the key was
    // present in the source record — different rule kinds carry
    // different key sets and we don't want to inject keys (e.g. ENABLED)
    // a kind doesn't normally emit.
    let had = |k: &str| {
        r.parameters
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(k))
    };
    for (k, v) in &r.parameters {
        params.insert(k, v.clone());
    }
    if !r.name.is_empty() && had("NAME") {
        params.insert("NAME", r.name.clone());
    }
    if !r.rule_kind.is_empty() && had("RULEKIND") {
        params.insert("RULEKIND", r.rule_kind.clone());
    }
    if !r.comment.is_empty() && had("COMMENT") {
        params.insert("COMMENT", r.comment.clone());
    }
    if !r.unique_id.is_empty() && had("UNIQUEID") {
        params.insert("UNIQUEID", r.unique_id.clone());
    }
    if had("ENABLED") {
        params.insert("ENABLED", if r.enabled { "TRUE" } else { "FALSE" });
    }
    if r.priority != 0 && had("PRIORITY") {
        params.insert("PRIORITY", r.priority.to_string());
    }
    if !r.scope1_expression.is_empty() && had("SCOPE1EXPRESSION") {
        params.insert("SCOPE1EXPRESSION", r.scope1_expression.clone());
    }
    if !r.scope2_expression.is_empty() && had("SCOPE2EXPRESSION") {
        params.insert("SCOPE2EXPRESSION", r.scope2_expression.clone());
    }
}

// ObjectClass

pub fn object_class_from_params(params: &ParameterMap) -> ObjectClass {
    let mut o = ObjectClass::default();
    let mut typed: Vec<(String, String)> = Vec::new();
    for (n, v, _) in params.iter() {
        typed.push((n.to_string(), v.to_string()));
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
    // Same as rule_to_params: typed fields only shadow when the key was
    // present in the source record.
    let had = |k: &str| {
        o.parameters
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(k))
    };
    for (k, v) in &o.parameters {
        params.insert(k, v.clone());
    }
    if !o.name.is_empty() && had("NAME") {
        params.insert("NAME", o.name.clone());
    }
    if !o.super_class.is_empty() && had("SUPERCLASS") {
        params.insert("SUPERCLASS", o.super_class.clone());
    }
    if !o.sub_class.is_empty() && had("SUBCLASS") {
        params.insert("SUBCLASS", o.sub_class.clone());
    }
    if !o.unique_id.is_empty() && had("UNIQUEID") {
        params.insert("UNIQUEID", o.unique_id.clone());
    }
    if !o.kind.is_empty() && had("KIND") {
        params.insert("KIND", o.kind.clone());
    }
    if had("ENABLED") {
        params.insert("ENABLED", if o.enabled { "TRUE" } else { "FALSE" });
    }
    for (i, m) in o.members.iter().enumerate() {
        let key = format!("MEMBER{i}");
        if had(&key) || !o.parameters.is_empty() {
            params.insert(key.as_str(), m.clone());
        }
    }
}

// DifferentialPair

pub fn differential_pair_from_params(params: &ParameterMap) -> DifferentialPair {
    let mut d = DifferentialPair::default();
    let mut typed: Vec<(String, String)> = Vec::new();
    for (n, v, _) in params.iter() {
        typed.push((n.to_string(), v.to_string()));
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
    // Altium omits ENABLED; absence means enabled.
    d.enabled = get_bool_explicit(params, "ENABLED").unwrap_or(true);
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
    // Only an explicit FALSE is written; Altium doesn't emit ENABLED.
    if !d.enabled {
        params.insert("ENABLED", "FALSE");
    }
}

// Room

pub fn room_from_params(params: &ParameterMap) -> Room {
    let mut r = Room::default();
    let mut typed: Vec<(String, String)> = Vec::new();
    for (n, v, _) in params.iter() {
        typed.push((n.to_string(), v.to_string()));
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
    if let Some(v) = get_coord_loose(params, "X") {
        b.x_location = v;
    }
    if let Some(v) = get_coord_loose(params, "Y") {
        b.y_location = v;
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
    // Unmodeled keys (VIEWPORTX1..Y2, VISIBLELAYERS, …) pass through.
    b.additional_parameters = collect_additional(
        params,
        &[
            "DOCUMENTPATH",
            "VIEWPORTTITLE",
            "FONTNAME",
            "LAYER",
            "ROTATION",
            "VIEWPORTSCALE",
            "MIRROR",
            "KEEPOUT",
            "POLYGONOUTLINE",
            "USERROUTED",
            "ISVIEWPORT",
            "VIEWPORTVISIBLE",
            "ORIGINMODE",
            "COLCOUNT",
            "ROWCOUNT",
            "UNIONINDEX",
            "FONTSIZE",
            "FONTCOLOR",
            "X",
            "Y",
            "X1",
            "Y1",
            "X2",
            "Y2",
            "COLSPACING",
            "ROWSPACING",
        ],
    );
    b
}

pub fn embedded_board_to_params(b: &EmbeddedBoard, params: &mut ParameterMap) {
    // Defaults first, then raw passthrough, then typed fields — later
    // inserts win.
    params.insert("SELECTION", "FALSE");
    params.insert("LOCKED", "FALSE");
    for (k, v) in &b.additional_parameters {
        params.insert(k, v.clone());
    }
    params.insert(
        "POLYGONOUTLINE",
        if b.polygon_outline { "TRUE" } else { "FALSE" },
    );
    params.insert("USERROUTED", if b.user_routed { "TRUE" } else { "FALSE" });
    params.insert("KEEPOUT", if b.is_keepout { "TRUE" } else { "FALSE" });
    params.insert("MIRROR", if b.mirror_flag { "TRUE" } else { "FALSE" });
    params.insert("LAYER", layer_byte_to_name(b.layer));
    params.insert("UNIONINDEX", b.union_index.to_string());
    params.insert("ORIGINMODE", b.origin_mode.to_string());
    params.insert("COLCOUNT", b.col_count.to_string());
    params.insert("ROWCOUNT", b.row_count.to_string());
    params.insert("X1", format_mil_coord(b.x1_location));
    params.insert("Y1", format_mil_coord(b.y1_location));
    params.insert("X2", format_mil_coord(b.x2_location));
    params.insert("Y2", format_mil_coord(b.y2_location));
    params.insert("X", format_mil_coord(b.x_location));
    params.insert("Y", format_mil_coord(b.y_location));
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
