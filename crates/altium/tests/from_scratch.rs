//! From-scratch creation round-trip tests.

#![allow(clippy::field_reassign_with_default)]

use altium::coord::{Coord, CoordPoint};
use altium::enums::PadShape;
use altium::pcb;

#[test]
fn pcblib_from_scratch_with_one_pad() {
    let mut pad = pcb::Pad::default();
    pad.designator = Some("1".into());
    pad.location = CoordPoint::new(Coord::from_mils(0.0), Coord::from_mils(0.0));
    pad.size_top = CoordPoint::new(Coord::from_mils(60.0), Coord::from_mils(60.0));
    pad.size_middle = pad.size_top;
    pad.size_bottom = pad.size_top;
    pad.shape_top = PadShape::Round;
    pad.shape_middle = PadShape::Round;
    pad.shape_bottom = PadShape::Round;
    pad.hole_size = Coord::from_mils(30.0);
    pad.is_plated = true;
    pad.layer = 1;

    let mut component = pcb::Component::new("R0402");
    component.description = Some("Test resistor 0402".into());
    component.height = Coord::from_mm(0.4);
    component.pads.push(pad);

    let mut library = pcb::Library::default();
    library.unique_id = "AAAAAAAA".into();
    library.components.push(component);

    let bytes = library.to_bytes().expect("write");
    let parsed = pcb::Library::from_bytes(bytes).expect("read");

    assert_eq!(parsed.components.len(), 1);
    let comp = &parsed.components[0];
    assert_eq!(comp.name, "R0402");
    assert_eq!(comp.description.as_deref(), Some("Test resistor 0402"));
    assert_eq!(comp.pads.len(), 1);
    assert_eq!(comp.pads[0].designator.as_deref(), Some("1"));
    assert_eq!(comp.pads[0].size_top.x, Coord::from_mils(60.0));
    assert_eq!(comp.pads[0].hole_size, Coord::from_mils(30.0));
    assert_eq!(comp.pads[0].shape_top, PadShape::Round);
    assert!(comp.pads[0].is_plated);
}

#[test]
fn pcblib_from_scratch_with_track_arc_text() {
    let mut track = pcb::Track::default();
    track.start = CoordPoint::new(Coord::from_mils(-50.0), Coord::ZERO);
    track.end = CoordPoint::new(Coord::from_mils(50.0), Coord::ZERO);
    track.width = Coord::from_mils(10.0);
    track.layer = 33; // top overlay

    let mut arc = pcb::Arc::default();
    arc.center = CoordPoint::new(Coord::ZERO, Coord::ZERO);
    arc.radius = Coord::from_mils(20.0);
    arc.start_angle = 0.0;
    arc.end_angle = 360.0;
    arc.width = Coord::from_mils(5.0);
    arc.layer = 33;

    let mut text = pcb::Text::default();
    text.text = "ABC".into();
    text.location = CoordPoint::new(Coord::from_mils(-30.0), Coord::from_mils(30.0));
    text.height = Coord::from_mils(40.0);
    text.stroke_width = Coord::from_mils(8.0);
    text.layer = 33;

    let mut component = pcb::Component::new("CIRCLE");
    component.tracks.push(track);
    component.arcs.push(arc);
    component.texts.push(text);

    let mut library = pcb::Library::default();
    library.unique_id = "BBBBBBBB".into();
    library.components.push(component);

    let parsed = pcb::Library::from_bytes(library.to_bytes().unwrap()).unwrap();
    let comp = &parsed.components[0];
    assert_eq!(comp.tracks.len(), 1);
    assert_eq!(comp.arcs.len(), 1);
    assert_eq!(comp.texts.len(), 1);
    assert_eq!(comp.tracks[0].width, Coord::from_mils(10.0));
    assert_eq!(comp.arcs[0].radius, Coord::from_mils(20.0));
    assert_eq!(comp.arcs[0].sweep_angle(), 360.0);
    assert_eq!(comp.texts[0].text, "ABC");
}

#[test]
fn empty_pcblib_round_trips() {
    let library = pcb::Library::default();
    let bytes = library.to_bytes().expect("write empty");
    let parsed = pcb::Library::from_bytes(bytes).expect("read empty");
    assert!(parsed.components.is_empty());
}

#[test]
fn empty_pcbdoc_round_trips() {
    let document = pcb::Document::default();
    let bytes = document.to_bytes().expect("write empty");
    let parsed = pcb::Document::from_bytes(bytes).expect("read empty");
    assert!(parsed.pads.is_empty());
    assert!(parsed.tracks.is_empty());
}

#[test]
fn pcbdoc_from_scratch_with_via_and_track() {
    let mut via = pcb::Via::default();
    via.location = CoordPoint::new(Coord::from_mils(100.0), Coord::from_mils(100.0));
    via.diameter = Coord::from_mils(28.0);
    via.hole_size = Coord::from_mils(14.0);
    via.start_layer = 1;
    via.end_layer = 32;
    via.layer = 74;
    via.is_plated = true;

    let mut track = pcb::Track::default();
    track.start = CoordPoint::new(Coord::ZERO, Coord::ZERO);
    track.end = CoordPoint::new(Coord::from_mils(200.0), Coord::ZERO);
    track.width = Coord::from_mils(8.0);
    track.layer = 1;

    let mut document = pcb::Document::default();
    document.vias.push(via);
    document.tracks.push(track);

    let bytes = document.to_bytes().expect("write doc");
    let parsed = pcb::Document::from_bytes(bytes).expect("read doc");
    assert_eq!(parsed.vias.len(), 1);
    assert_eq!(parsed.tracks.len(), 1);
    assert_eq!(parsed.vias[0].diameter, Coord::from_mils(28.0));
    assert_eq!(parsed.tracks[0].end.x, Coord::from_mils(200.0));
}

#[test]
fn pcblib_preserves_unique_id() {
    let mut library = pcb::Library::default();
    library.unique_id = "ABCDEF12".into();
    library.components.push(pcb::Component::new("X"));
    let parsed = pcb::Library::from_bytes(library.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.unique_id, "ABCDEF12");
}

// PcbDoc from-scratch covering the typed storages added in 2026-05

#[test]
fn pcbdoc_from_scratch_with_nets_and_components() {
    let mut doc = pcb::Document::default();
    doc.nets.push(pcb::Net {
        name: "VCC".into(),
        ..Default::default()
    });
    doc.nets.push(pcb::Net {
        name: "GND".into(),
        ..Default::default()
    });

    let mut comp = pcb::Component::new("R1");
    comp.x = Coord::from_mils(100.0);
    comp.y = Coord::from_mils(200.0);
    comp.height = Coord::from_mm(0.4);
    comp.description = Some("0402 resistor".into());
    comp.pattern = Some("R0402".into());
    comp.layer = 1;
    doc.components.push(comp);

    let mut pad = pcb::Pad::default();
    pad.designator = Some("1".into());
    pad.location = CoordPoint::new(Coord::from_mils(90.0), Coord::from_mils(200.0));
    pad.size_top = CoordPoint::new(Coord::from_mils(40.0), Coord::from_mils(40.0));
    pad.size_middle = pad.size_top;
    pad.size_bottom = pad.size_top;
    pad.shape_top = PadShape::Rectangular;
    pad.shape_middle = PadShape::Rectangular;
    pad.shape_bottom = PadShape::Rectangular;
    pad.layer = 1;
    pad.component_index = 0; // links to the first component
    pad.net = Some("VCC".into());
    doc.pads.push(pad);

    let bytes = doc.to_bytes().expect("write");
    let parsed = pcb::Document::from_bytes(bytes).expect("read");

    assert_eq!(parsed.nets.len(), 2);
    assert_eq!(parsed.nets[0].name, "VCC");
    assert_eq!(parsed.components.len(), 1);
    assert_eq!(parsed.components[0].name, "R1");
    assert_eq!(
        parsed.components[0].pads.len(),
        1,
        "pad linked to component"
    );
    assert_eq!(parsed.pads.len(), 1);
    assert_eq!(parsed.pads[0].net.as_deref(), Some("VCC"));
}

#[test]
fn pcbdoc_from_scratch_with_polygon() {
    let mut doc = pcb::Document::default();
    let mut poly = pcb::Polygon::default();
    poly.layer = 1;
    poly.name = Some("GND_POUR".into());
    poly.net = Some("GND".into());
    poly.poly_hatch_style = 1;
    poly.point_count = 4;
    poly.vertices
        .push(pcb::PolygonVertex::linear(CoordPoint::new(
            Coord::from_mils(0.0),
            Coord::from_mils(0.0),
        )));
    poly.vertices
        .push(pcb::PolygonVertex::linear(CoordPoint::new(
            Coord::from_mils(1000.0),
            Coord::from_mils(0.0),
        )));
    poly.vertices
        .push(pcb::PolygonVertex::linear(CoordPoint::new(
            Coord::from_mils(1000.0),
            Coord::from_mils(500.0),
        )));
    poly.vertices
        .push(pcb::PolygonVertex::linear(CoordPoint::new(
            Coord::from_mils(0.0),
            Coord::from_mils(500.0),
        )));
    doc.polygons.push(poly);

    let parsed = pcb::Document::from_bytes(doc.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.polygons.len(), 1);
    assert_eq!(parsed.polygons[0].name.as_deref(), Some("GND_POUR"));
    assert_eq!(parsed.polygons[0].vertices.len(), 4);
}

#[test]
#[ignore = "Rules6/Rooms6 writers not implemented yet (write_doc_rooms is dead code)"]
fn pcbdoc_from_scratch_with_rules_classes_diff_pairs_rooms() {
    use altium::pcb::rule::{DifferentialPair, ObjectClass, Room, Rule};
    let mut doc = pcb::Document::default();
    doc.rules.push(Rule {
        name: "Clearance_All".into(),
        rule_kind: "Clearance".into(),
        enabled: true,
        priority: 1,
        comment: "default".into(),
        unique_id: "RUID0001".into(),
        scope1_expression: "All".into(),
        scope2_expression: "All".into(),
        parameters: Default::default(),
        rule_type_code: 0,
    });
    let mut class = ObjectClass::default();
    class.name = "Power".into();
    class.kind = "NetClass".into();
    class.enabled = true;
    class.members.push("VCC".into());
    class.members.push("GND".into());
    doc.classes.push(class);
    doc.differential_pairs.push(DifferentialPair {
        name: "USB_DATA".into(),
        positive_net_name: "USB_DP".into(),
        negative_net_name: "USB_DM".into(),
        unique_id: "DPUID01".into(),
        enabled: true,
        parameters: Default::default(),
    });
    doc.rooms.push(Room {
        name: "MCU_Room".into(),
        unique_id: "ROOM01".into(),
        parameters: Default::default(),
    });

    let parsed = pcb::Document::from_bytes(doc.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.rules.len(), 1);
    assert_eq!(parsed.rules[0].name, "Clearance_All");
    assert_eq!(parsed.rules[0].rule_kind, "Clearance");
    assert_eq!(parsed.classes.len(), 1);
    assert_eq!(parsed.classes[0].members, vec!["VCC", "GND"]);
    assert_eq!(parsed.differential_pairs.len(), 1);
    assert_eq!(parsed.differential_pairs[0].positive_net_name, "USB_DP");
    assert_eq!(parsed.rooms.len(), 1);
    assert_eq!(parsed.rooms[0].name, "MCU_Room");
}

// SchLib from-scratch with typed primitives

#[test]
fn schlib_from_scratch_with_pin_and_rectangle() {
    use altium::enums::{PinElectricalType, PinOrientation};
    use altium::sch::primitives::{Pin, Rectangle};

    let mut comp = altium::sch::Component::new("U1");
    comp.description = Some("Op-Amp".into());
    comp.designator_prefix = Some("U".into());
    comp.lib_reference = Some("U1".into());
    comp.symbol_reference = Some("U1".into());

    let mut body = Rectangle::default();
    body.corner1 = CoordPoint::new(Coord::from_mils(-50.0), Coord::from_mils(-30.0));
    body.corner2 = CoordPoint::new(Coord::from_mils(50.0), Coord::from_mils(30.0));
    body.color = 0;
    body.fill_color = 0xFFFFE0;
    body.is_filled = true;
    body.line_width = Coord::from_mils(1.0);
    comp.rectangles.push(body);

    let mut pin = Pin::default();
    pin.name = Some("VCC".into());
    pin.designator = Some("1".into());
    pin.location = CoordPoint::new(Coord::from_mils(-50.0), Coord::ZERO);
    pin.length = Coord::from_mils(20.0);
    pin.orientation = PinOrientation::Right;
    pin.electrical_type = PinElectricalType::Power;
    pin.show_name = true;
    pin.show_designator = true;
    comp.pins.push(pin);

    let mut lib = altium::sch::Library::default();
    lib.components.push(comp);

    let bytes = lib.to_bytes().expect("write schlib");
    let parsed = altium::sch::Library::from_bytes(bytes).expect("read schlib");

    assert_eq!(parsed.components.len(), 1);
    let c = &parsed.components[0];
    assert_eq!(c.name, "U1");
    assert_eq!(c.pins.len(), 1);
    assert_eq!(c.pins[0].name.as_deref(), Some("VCC"));
    assert_eq!(c.pins[0].designator.as_deref(), Some("1"));
    assert_eq!(c.rectangles.len(), 1);
}

// Default-emission tests
//
// These verify the writer fills in load-bearing parameters real Altium files
// always carry — without them the editor can't render the file properly.

#[test]
fn schlib_default_file_header_carries_sheet_style() {
    // From-scratch SchLib should round-trip with sheet-style params populated.
    let mut lib = altium::sch::Library::default();
    lib.components.push(altium::sch::Component::new("U1"));
    let parsed = altium::sch::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();

    let header = &parsed.file_header_parameters;
    assert_eq!(
        header.get("AreaColor").map(|s| s.as_str()),
        Some("16317695"),
        "AreaColor must be present so the editor doesn't draw a black sheet"
    );
    assert_eq!(header.get("FontIdCount").map(|s| s.as_str()), Some("1"));
    assert_eq!(
        header.get("FontName1").map(|s| s.as_str()),
        Some("Times New Roman")
    );
    assert_eq!(header.get("LibRef0").map(|s| s.as_str()), Some("U1"));
    // Single-part component → PartCount0 stored as part_count + 1.
    assert_eq!(header.get("PartCount0").map(|s| s.as_str()), Some("2"));
    assert_eq!(header.get("CompCount").map(|s| s.as_str()), Some("1"));
}

#[test]
fn schlib_default_storage_stub_is_emitted() {
    // Without any embedded images, the writer should still emit a minimal
    // `Storage` stream containing `|HEADER=Icon storage|`. Real Altium
    // libraries always carry this stream; without it the editor renders the
    // sheet background black.
    let mut lib = altium::sch::Library::default();
    lib.components.push(altium::sch::Component::new("U1"));
    let bytes = lib.to_bytes().unwrap();

    let mut cf = altium::compound::CompoundFile::open(bytes).unwrap();
    let raw = cf
        .read_stream("Storage")
        .expect("Storage stream must exist even without embedded images");
    let body = String::from_utf8_lossy(&raw);
    assert!(
        body.contains("HEADER=Icon storage"),
        "Storage stub body must reference Icon storage; got: {body:?}"
    );
}

#[test]
fn pcb_via_preserves_reserved_magic_constants() {
    // Via has four reserved-byte regions the format hardcodes; preserving
    // them verbatim lets non-canonical files round-trip exactly.
    let mut via = pcb::Via::default();
    via.location = CoordPoint::new(Coord::ZERO, Coord::ZERO);
    via.diameter = Coord::from_mils(20.0);
    via.hole_size = Coord::from_mils(10.0);
    // Non-canonical reserved values.
    via.reserved_block_8 = [9, 8, 7, 6, 5, 4, 3, 2];
    via.reserved_byte_after_mask_flag = 7;
    via.trailing_reserved_i16 = 42;
    via.trailing_reserved_i32 = 9001;

    let mut comp = pcb::Component::new("X");
    comp.vias.push(via);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);

    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    let v = &parsed.components[0].vias[0];
    assert_eq!(v.reserved_block_8, [9, 8, 7, 6, 5, 4, 3, 2]);
    assert_eq!(v.reserved_byte_after_mask_flag, 7);
    assert_eq!(v.trailing_reserved_i16, 42);
    assert_eq!(v.trailing_reserved_i32, 9001);
}

#[test]
fn pcb_pad_preserves_reserved_blocks_and_net_string() {
    // Pad has two reserved blocks (between designator and net string, and
    // after the net string). Older writer always emitted `[0]`; the current
    // writer round-trips the original bytes for any non-canonical content.
    let mut pad = pcb::Pad::default();
    pad.designator = Some("1".into());
    pad.location = CoordPoint::new(Coord::ZERO, Coord::ZERO);
    pad.size_top = CoordPoint::new(Coord::from_mils(60.0), Coord::from_mils(60.0));
    pad.size_middle = pad.size_top;
    pad.size_bottom = pad.size_top;
    pad.layer = 1;
    // Non-canonical reserved bytes and net-string block.
    pad.reserved_block_after_designator = vec![0xAB, 0xCD, 0xEF];
    pad.reserved_block_after_net_string = vec![0x01, 0x02];
    pad.net_string_block = "|&|7".into();

    let mut comp = pcb::Component::new("X");
    comp.pads.push(pad);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);

    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    let p = &parsed.components[0].pads[0];
    assert_eq!(p.reserved_block_after_designator, vec![0xAB, 0xCD, 0xEF]);
    assert_eq!(p.reserved_block_after_net_string, vec![0x01, 0x02]);
    assert_eq!(p.net_string_block, "|&|7");
}

#[test]
fn pcbdoc_polygon_legacy_keys_round_trip_through_writer() {
    // Polygons in older PcbDoc files use legacy spellings: POUROVER, AVOIDOBSTICLES,
    // POLYHATCHSTYLE, REMOVENARROWNECKS. The writer used to overwrite them
    // with the canonical spellings; now it should preserve whatever the source
    // file used.
    use altium::parameter::ParameterMap;
    use altium::pcb::doc_codec::{polygon_from_params, polygon_to_params};

    // Build a parameter set using the four legacy spellings.
    let mut input = ParameterMap::new();
    input.insert("POUROVER", "1");
    input.insert("AVOIDOBSTICLES", "TRUE");
    input.insert("POLYHATCHSTYLE", "2");
    input.insert("REMOVENARROWNECKS", "TRUE");

    let p = polygon_from_params(&input);
    assert!(p.avoid_obstacles_uses_legacy_key);
    assert!(p.pour_over_uses_legacy_key);
    assert!(p.poly_hatch_uses_legacy_key);
    assert!(p.remove_necks_uses_legacy_key);

    // Round-trip: write, then verify the legacy spellings come back out.
    let mut out = ParameterMap::new();
    polygon_to_params(&p, &mut out);
    assert!(out.contains_key("POUROVER"));
    assert!(!out.contains_key("POURMODE"));
    assert!(out.contains_key("AVOIDOBSTICLES"));
    assert!(!out.contains_key("AVOIDOBST"));
    assert!(out.contains_key("POLYHATCHSTYLE"));
    assert!(!out.contains_key("HATCHSTYLE"));
    assert!(out.contains_key("REMOVENARROWNECKS"));
    assert!(!out.contains_key("REMOVENECKS"));

    // And the canonical-key variant emits canonical keys.
    let mut canonical = ParameterMap::new();
    canonical.insert("POURMODE", "1");
    canonical.insert("AVOIDOBST", "TRUE");
    let p2 = polygon_from_params(&canonical);
    assert!(!p2.avoid_obstacles_uses_legacy_key);
    assert!(!p2.pour_over_uses_legacy_key);
    let mut out2 = ParameterMap::new();
    polygon_to_params(&p2, &mut out2);
    assert!(out2.contains_key("POURMODE"));
    assert!(!out2.contains_key("POUROVER"));
    assert!(out2.contains_key("AVOIDOBST"));
    assert!(!out2.contains_key("AVOIDOBSTICLES"));
}

#[test]
fn schlib_wire_polyline_line_style_preserves_unknown_values() {
    // Wire/Polyline `line_style` is a raw int — values outside the
    // `SchLineStyle` 0..=3 range must round-trip rather than collapse to 0.
    use altium::sch::primitives::{Polyline, Wire};

    let mut wire = Wire::default();
    wire.line_style = 7; // unknown style int
    let mut poly = Polyline::default();
    poly.line_style = 9;

    let mut comp = altium::sch::Component::new("U1");
    comp.wires.push(wire);
    comp.polylines.push(poly);
    let mut lib = altium::sch::Library::default();
    lib.components.push(comp);

    let parsed = altium::sch::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    let c = &parsed.components[0];
    assert_eq!(
        c.wires[0].line_style, 7,
        "unknown wire line_style must round-trip"
    );
    assert_eq!(
        c.polylines[0].line_style, 9,
        "unknown polyline line_style must round-trip"
    );
}

#[test]
fn schlib_part_count_round_trips_for_multi_part_components() {
    // Regression test: PARTCOUNT is stored as user_count + 1 in Altium files,
    // so a 2-part component is `PARTCOUNT=3`. Verify both the per-component
    // DTO and the FileHeader manifest stay in sync with the user-facing count
    // through a write-then-read cycle.
    for n in [1, 2, 5] {
        let mut comp = altium::sch::Component::new("U");
        comp.part_count = n;
        let mut lib = altium::sch::Library::default();
        lib.components.push(comp);
        let parsed = altium::sch::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
        assert_eq!(
            parsed.components[0].part_count, n,
            "user-facing part_count must round-trip"
        );
        assert_eq!(
            parsed
                .file_header_parameters
                .get("PartCount0")
                .map(String::as_str),
            Some((n + 1).to_string().as_str()),
            "FileHeader manifest stores user_count + 1"
        );
    }
}

#[test]
fn schlib_component_default_carries_altium_placeholders() {
    let comp = altium::sch::Component::new("X");
    assert_eq!(
        comp.area_color, 11_599_871,
        "fresh symbol body should be light yellow"
    );
    assert_eq!(comp.color, 128, "default border colour is dark grey");
    assert_eq!(comp.library_path.as_deref(), Some("*"));
    assert_eq!(comp.source_library_name.as_deref(), Some("*"));
    assert_eq!(comp.sheet_part_file_name.as_deref(), Some("*"));
    assert_eq!(comp.target_file_name.as_deref(), Some("*"));
    assert!(comp.part_id_locked);
    assert_eq!(comp.part_count, 1);
    assert_eq!(comp.current_part_id, 1);
    assert_eq!(comp.display_mode_count, 1);
    assert_eq!(comp.owner_part_id, -1);
}

#[test]
fn sch_body_shape_defaults_match_altium_palette() {
    use altium::sch::primitives::{Ellipse, Polygon, Rectangle, RoundedRectangle};

    let r = Rectangle::default();
    assert_eq!(r.color, 128);
    assert_eq!(r.fill_color, 11_599_871);
    assert_eq!(r.line_width, Coord::from_mils(1.0));

    let rr = RoundedRectangle::default();
    assert_eq!(rr.color, 128);
    assert_eq!(rr.fill_color, 11_599_871);

    let p = Polygon::default();
    assert_eq!(p.color, 128);
    assert_eq!(p.fill_color, 11_599_871);

    let e = Ellipse::default();
    assert_eq!(e.color, 128);
    assert_eq!(e.fill_color, 11_599_871);
}

#[test]
fn pcblib_default_data_carries_v9_master_stack() {
    // Empty PcbLib should carry a full V9 stack-up after round-trip — Altium
    // refuses to open libraries that lack one.
    let mut lib = pcb::Library::default();
    lib.unique_id = "DEADBEEF".into();
    lib.components.push(pcb::Component::new("X"));
    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();

    let params = parsed
        .library_parameters
        .as_ref()
        .expect("Library/Data parameters present");
    assert_eq!(params.get("HEADER"), Some("PCB 6.0 Binary Library File"));
    assert_eq!(params.get("KIND"), Some("Protel_Advanced_PCB_Library"));
    assert!(params.contains_key("V9_MASTERSTACK_STYLE"));
    assert_eq!(
        params.get("V9_MASTERSTACK_NAME"),
        Some("Master layer stack")
    );
    assert_eq!(
        params.get("V9_STACK_LAYER3_NAME"),
        Some("Top Layer"),
        "the canonical 2-layer stack puts Top Layer at index 3"
    );
    assert_eq!(params.get("V9_STACK_LAYER5_NAME"), Some("Bottom Layer"));
    assert!(
        params.contains_key("V9_CACHE_LAYER0_NAME"),
        "cache layers should be present"
    );
}

#[test]
fn pcblib_user_supplied_params_keep_their_values() {
    use altium::parameter::ParameterMap;
    // If the caller has populated library_parameters, their entries take
    // precedence over the defaults; only missing keys get filled in.
    let mut lib = pcb::Library::default();
    let mut user = ParameterMap::new();
    user.insert("V9_MASTERSTACK_NAME", "Custom Stack");
    user.insert("HEADER", "PCB 6.0 Binary Library File");
    lib.library_parameters = Some(user);
    lib.components.push(pcb::Component::new("X"));

    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    let params = parsed.library_parameters.as_ref().unwrap();
    assert_eq!(
        params.get("V9_MASTERSTACK_NAME"),
        Some("Custom Stack"),
        "user-supplied value preserved"
    );
    assert!(
        params.contains_key("V9_STACK_LAYER0_NAME"),
        "missing defaults still populated"
    );
}

#[test]
fn pcb_track_arc_fill_region_default_to_enabled() {
    // Real Altium-emitted primitives always have `enabled: true`; without it
    // the editor reads them but renders nothing.
    use altium::pcb::{Arc, Fill, Region, Track};

    assert!(Track::default().enabled);
    assert!(Arc::default().enabled);
    assert!(Fill::default().enabled);
    assert!(Region::default().enabled);
}

#[test]
fn pcb_pad_default_has_four_thermal_relief_spokes() {
    let pad = pcb::Pad::default();
    assert_eq!(pad.relief_entries, 4);
}

#[test]
fn pcb_component_body_defaults_match_altium() {
    let body = pcb::ComponentBody::default();
    assert_eq!(body.model_source.as_deref(), Some("Undefined"));
    assert_eq!(body.arc_resolution, 1.0);
}

#[test]
fn pcb_component_new_defaults_to_top_layer_standard_kind() {
    let c = pcb::Component::new("R0402");
    assert_eq!(c.layer, 1, "new components default to Top layer");
    assert_eq!(c.component_kind, 1, "Standard component kind");
    assert!(c.enabled);
    assert!(c.jumpers_visible);
}

#[test]
fn pcblib_section_keys_round_trip_with_long_names() {
    // Footprint names longer than the 31-char OLE storage limit go through the
    // `SectionKeys` stream, where each entry is a pair of size-prefixed
    // Pascal-string blocks (LibRef, then storage key). The reader used to
    // read the LibRef as a bare Pascal string, which desynced on any library
    // with more than a couple of long names (e.g. real Altium libraries).
    let names = [
        "CONN-SMD_10P-P0.40_DF40C-10DP-0.4V-51",
        "CONN-SMD_20P-P0.40_DF40C-20DP-0.4V-51",
        "CONN-SMD_30P-P0.50_X0502FVS-30D1S-LPSN",
    ];
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    for (i, name) in names.iter().enumerate() {
        let mut comp = pcb::Component::new(*name);
        comp.pads.push(
            pcb::PadBuilder::new()
                .at(Coord::ZERO, Coord::ZERO)
                .size(Coord::from_mils(20.0), Coord::from_mils(20.0))
                .shape(PadShape::Rectangular)
                .designator((i + 1).to_string())
                .build(),
        );
        lib.components.push(comp);
    }

    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).expect("read back");
    let parsed_names: Vec<&str> = parsed.components.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(parsed_names, names);
    assert_eq!(parsed.section_keys.len(), names.len());
    for (name, key) in &parsed.section_keys {
        assert!(names.contains(&name.as_str()), "unexpected LibRef {name:?}");
        assert_eq!(
            key.len(),
            31,
            "storage key {key:?} should be truncated to 31 chars"
        );
    }
    assert!(parsed.components.iter().all(|c| c.pads.len() == 1));
}

#[test]
fn pcblib_text_writes_full_record_with_justification() {
    use altium::enums::{PcbTextKind, TextJustification};

    let mut text = pcb::Text::default();
    text.text = ".Designator".into();
    text.layer = 63; // Mechanical 7
    text.height = Coord::from_mm(1.0);
    text.stroke_width = Coord::from_mm(0.1);
    text.text_kind = PcbTextKind::TrueType;
    text.is_truetype = true;
    text.font_name = Some("Source Code Pro".into());
    text.justification = TextJustification::MiddleCenter;
    text.justification_valid = true;
    text.is_frame = true;

    let mut comp = pcb::Component::new("T");
    comp.texts.push(text);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let bytes = lib.to_bytes().unwrap();

    // Data = [pattern block][0x05][u32 252][record][string block]: the record
    // must be Altium's full 252-byte layout, with the autoposition byte
    // (5 = centre/centre) at 132, the V7 layer id at 226 and the
    // justification-valid flag at 240.
    let mut cf = altium::compound::CompoundFile::open(bytes.clone()).unwrap();
    let data = cf.read_stream("T/Data").unwrap();
    let pattern_len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let p = 4 + pattern_len;
    assert_eq!(data[p], 5, "text record id");
    let len = u32::from_le_bytes(data[p + 1..p + 5].try_into().unwrap()) & 0x00FF_FFFF;
    assert_eq!(len, 252, "text record length");
    let rec = &data[p + 5..p + 5 + 252];
    assert_eq!(rec[132], 5);
    assert_eq!(rec[160], 1, "authoritative text kind = TrueType");
    assert_eq!(
        u32::from_le_bytes(rec[226..230].try_into().unwrap()),
        0x0102_0007
    );
    assert_eq!(rec[230], 1, "frame flag");
    assert_eq!(rec[240], 1, "justification valid flag");

    let parsed = pcb::Library::from_bytes(bytes).unwrap();
    let t = &parsed.components[0].texts[0];
    assert_eq!(t.justification, TextJustification::MiddleCenter);
    assert!(t.justification_valid);
    assert!(t.is_frame);
    assert!(t.is_truetype);
    assert_eq!(t.font_name.as_deref(), Some("Source Code Pro"));
    assert_eq!(t.bar_code_font_name.as_deref(), Some("Arial"));
}

#[test]
fn pcblib_height_is_written_as_mil_string() {
    let mut comp = pcb::Component::new("H");
    comp.height = Coord::from_mm(4.05);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let bytes = lib.to_bytes().unwrap();

    let mut cf = altium::compound::CompoundFile::open(bytes.clone()).unwrap();
    let params = String::from_utf8_lossy(&cf.read_stream("H/Parameters").unwrap()).into_owned();
    assert!(params.contains("|HEIGHT=159.4488mil"), "{params}");

    let parsed = pcb::Library::from_bytes(bytes).unwrap();
    let delta = (parsed.components[0].height - Coord::from_mm(4.05)).abs();
    assert!(delta.to_raw() <= 1, "height round-trip drift {delta:?}");
}

#[test]
fn schlib_pin_text_data_round_trips_fonts_colours_and_positions() {
    use altium::sch::primitives::Pin;

    let mut a = Pin::default();
    a.name = Some("A".into());
    a.designator = Some("1".into());
    a.name_font_mode = 1;
    a.name_custom_font_id = 3;
    a.name_custom_color = 0x00FF_00FF;
    a.designator_font_mode = 1;
    a.designator_custom_font_id = 2;
    a.designator_position_mode = 1;
    a.designator_custom_position_margin = 500_000;
    a.designator_custom_position_rotation_relative = true;
    let mut b = Pin::default();
    b.name = Some("B".into());
    b.designator = Some("2".into());

    let mut comp = altium::sch::Component::new("U1");
    comp.pins.push(a);
    comp.pins.push(b);
    let mut lib = altium::sch::Library::default();
    lib.components.push(comp);

    let parsed = altium::sch::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    let c = &parsed.components[0];
    assert!(
        !c.additional_streams.contains_key("PinTextData"),
        "PinTextData is decoded into the pins, not carried as a raw stream"
    );
    let a = &c.pins[0];
    assert_eq!(a.name_font_mode, 1);
    assert_eq!(a.name_custom_font_id, 3);
    assert_eq!(a.name_custom_color, 0x00FF_00FF);
    assert_eq!(a.designator_font_mode, 1);
    assert_eq!(a.designator_custom_font_id, 2);
    assert_eq!(a.designator_position_mode, 1);
    assert_eq!(a.designator_custom_position_margin, 500_000);
    assert!(a.designator_custom_position_rotation_relative);
    let b = &c.pins[1];
    assert_eq!(b.name_font_mode, 0);
    assert_eq!(b.designator_font_mode, 0);
    assert_eq!(b.name_position_mode, 0);
}

#[test]
fn sch_pin_text_data_codec_matches_altium_layout() {
    use altium::sch::binary::{decode_pin_text_customisation, encode_pin_text_customisation};

    // Entry captured from an Altium-written library: designator and name
    // both use custom position (50 mil / 80 mil margins) and font slot 3.
    let hex = "1120a107000300000000001100350c00030000000000";
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let mut pos = 0;
    let d = decode_pin_text_customisation(&bytes, &mut pos).unwrap();
    let n = decode_pin_text_customisation(&bytes, &mut pos).unwrap();
    assert_eq!(pos, bytes.len(), "both blocks consume the whole entry");
    assert!(d.custom_position && d.custom_font);
    assert_eq!(d.margin, 500_000);
    assert_eq!(d.font_id, 3);
    assert_eq!(d.color, 0);
    assert_eq!(n.margin, 800_000);
    assert_eq!(n.font_id, 3);

    let mut out = Vec::new();
    encode_pin_text_customisation(&d, &mut out);
    encode_pin_text_customisation(&n, &mut out);
    assert_eq!(out, bytes, "encoder is the exact inverse");

    // A default (no customisation) name block is a single zero byte.
    let mut pos = 0;
    let bytes = [0x00u8, 0x10, 0x01, 0, 0, 0, 0, 0];
    let d = decode_pin_text_customisation(&bytes, &mut pos).unwrap();
    let n = decode_pin_text_customisation(&bytes, &mut pos).unwrap();
    assert!(d.is_default());
    assert!(n.custom_font && n.font_id == 1 && pos == bytes.len());
}

#[test]
fn schlib_non_ascii_text_round_trips_through_utf8_twins() {
    use altium::sch::primitives::Parameter;

    let mut comp = altium::sch::Component::new("R1");
    comp.description = Some("Resistor 100Ω ±1 %".into());
    let mut value = Parameter::default();
    value.name = "Value".into();
    value.value = "100Ω".into();
    comp.parameters.push(value);
    let mut temp = Parameter::default();
    temp.name = "Operating Temperature".into();
    temp.value = "-55 °C to +125 °C".into();
    comp.parameters.push(temp);
    let mut lib = altium::sch::Library::default();
    lib.components.push(comp);

    let bytes = lib.to_bytes().unwrap();
    // Altium's layout: UTF-8 copy, two empty entries, then the plain twin.
    let hay = bytes.as_slice();
    let needle = "%UTF8%TEXT=100Ω|||TEXT=100Ω".as_bytes();
    assert!(
        hay.windows(needle.len()).any(|w| w == needle),
        "parameter value must be written as a %UTF8% copy plus plain twin"
    );
    let needle = "%UTF8%TEXT=-55 °C to +125 °C|||TEXT=-55 °C".as_bytes();
    assert!(
        hay.windows(needle.len()).any(|w| w == needle),
        "Win-1252-representable text is promoted to %UTF8% as well"
    );

    let parsed = altium::sch::Library::from_bytes(bytes).unwrap();
    let c = &parsed.components[0];
    assert_eq!(c.description.as_deref(), Some("Resistor 100Ω ±1 %"));
    assert_eq!(c.parameters[0].value, "100Ω");
    assert_eq!(c.parameters[1].value, "-55 °C to +125 °C");
}

fn schlib_bytes_contain(bytes: &[u8], needle: &str) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[test]
fn schlib_ellipse_always_writes_secondary_radius() {
    use altium::sch::primitives::Ellipse;
    let mut e = Ellipse::default();
    e.radius_x = altium::Coord::from_mils(10.0);
    e.radius_y = altium::Coord::from_mils(10.0);
    let mut comp = altium::sch::Component::new("L1");
    comp.ellipses.push(e);
    let mut lib = altium::sch::Library::default();
    lib.components.push(comp);
    let bytes = lib.to_bytes().unwrap();
    assert!(
        schlib_bytes_contain(&bytes, "|SECONDARYRADIUS=1"),
        "a circle must still carry SecondaryRadius, Altium reads a missing one as 0"
    );
    let parsed = altium::sch::Library::from_bytes(bytes).unwrap();
    assert_eq!(
        parsed.components[0].ellipses[0].radius_y,
        altium::Coord::from_mils(10.0)
    );
}

#[test]
fn schlib_polyline_over_50_vertices_uses_extra_locations() {
    use altium::sch::primitives::Polyline;
    let mut p = Polyline::default();
    for i in 0..54 {
        p.vertices.push(altium::CoordPoint::new(
            altium::Coord::from_mils(i as f64 * 10.0),
            altium::Coord::from_mils(5.0),
        ));
    }
    let mut comp = altium::sch::Component::new("U1");
    comp.polylines.push(p);
    let mut lib = altium::sch::Library::default();
    lib.components.push(comp);
    let bytes = lib.to_bytes().unwrap();
    assert!(schlib_bytes_contain(&bytes, "|LOCATIONCOUNT=50|"));
    assert!(schlib_bytes_contain(&bytes, "|EXTRALOCATIONCOUNT=4"));
    assert!(schlib_bytes_contain(&bytes, "|EX51="));
    assert!(!schlib_bytes_contain(&bytes, "|X51="));
    let parsed = altium::sch::Library::from_bytes(bytes).unwrap();
    let v = &parsed.components[0].polylines[0].vertices;
    assert_eq!(v.len(), 54);
    assert_eq!(v[53].x, altium::Coord::from_mils(530.0));
}

#[test]
fn schlib_implementation_data_files_and_map_containers_round_trip() {
    use altium::sch::implementation::{Implementation, MapDefiner};
    let mut imp = Implementation::default();
    imp.model_name = Some("RDF0022A".into());
    imp.model_type = Some("PCBLIB".into());
    imp.is_current = true;
    imp.data_file_kinds = vec!["PCBLib".into()];
    imp.data_file_entities = vec!["RDF0022A".into()];
    imp.integrated_model = true;
    let mut plain = imp.clone();
    plain.model_name = Some("OTHER".into());
    let mut map = MapDefiner::default();
    map.designator_interface = Some("10".into());
    map.designator_implementations = vec!["10".into(), "12".into()];
    imp.map_definers.push(map);
    let mut comp = altium::sch::Component::new("U1");
    comp.implementations.push(imp);
    comp.implementations.push(plain);
    let mut lib = altium::sch::Library::default();
    lib.components.push(comp);
    let bytes = lib.to_bytes().unwrap();
    // Altium's 0-based data-file keys and its always-present containers.
    assert!(schlib_bytes_contain(&bytes, "|DATAFILECOUNT=1|"));
    assert!(schlib_bytes_contain(
        &bytes,
        "|MODELDATAFILEENTITY0=RDF0022A|"
    ));
    assert!(schlib_bytes_contain(&bytes, "|MODELDATAFILEKIND0=PCBLib|"));
    assert!(schlib_bytes_contain(&bytes, "|INTEGRATEDMODEL=T"));
    assert!(!schlib_bytes_contain(&bytes, "MODELDATAFILEKIND1="));
    let count = |needle: &str| {
        bytes
            .windows(needle.len())
            .filter(|w| *w == needle.as_bytes())
            .count()
    };
    assert_eq!(count("|RECORD=45|"), 2);
    // single-key container records end at the NUL, so match without a trailing pipe
    assert_eq!(
        count("|RECORD=46"),
        2,
        "MapDefinerList container is written for every implementation"
    );
    assert_eq!(count("|RECORD=47|"), 1);
    assert_eq!(count("|RECORD=48"), 2);
    // Altium links the hierarchy by record index: 46/48 point at their 45, 47 at its 46
    let record_has = |head: &str, key: &str| {
        bytes
            .windows(head.len())
            .enumerate()
            .filter(|(_, w)| *w == head.as_bytes())
            .all(|(i, _)| {
                let end = bytes[i..]
                    .iter()
                    .position(|&b| b == 0)
                    .map_or(bytes.len(), |e| i + e);
                schlib_bytes_contain(&bytes[i..end], key)
            })
    };
    assert!(record_has("|RECORD=45|", "|OWNERINDEX="));
    assert!(record_has("|RECORD=46", "|OWNERINDEX="));
    assert!(record_has("|RECORD=47|", "|OWNERINDEX="));
    assert!(record_has("|RECORD=48", "|OWNERINDEX="));
    let parsed = altium::sch::Library::from_bytes(bytes).unwrap();
    let imps = &parsed.components[0].implementations;
    assert_eq!(imps.len(), 2);
    assert_eq!(imps[0].data_file_entities, vec!["RDF0022A".to_string()]);
    assert_eq!(imps[0].data_file_kinds, vec!["PCBLib".to_string()]);
    assert!(imps[0].integrated_model);
    assert_eq!(imps[0].map_definers.len(), 1);
    assert_eq!(
        imps[0].map_definers[0].designator_implementations,
        vec!["10".to_string(), "12".to_string()]
    );
    assert!(imps[1].map_definers.is_empty());
}

#[test]
fn schlib_implementation_owner_indexes_count_from_the_component_record() {
    use altium::sch::implementation::{Implementation, MapDefiner};
    use altium::sch::primitives::{Parameter, Pin};
    // Data stream layout: 0 component, 1-2 pins, 3-4 parameters, 5 = 44,
    // 6 = 45, 7 = 46, 8 = 47, 9 = 48.
    let mut comp = altium::sch::Component::new("U1");
    for d in ["1", "2"] {
        let mut p = Pin::default();
        p.designator = Some(d.into());
        p.name = Some(d.into());
        comp.pins.push(p);
    }
    for n in ["Designator", "Comment"] {
        let mut p = Parameter::default();
        p.name = n.into();
        p.value = "x".into();
        comp.parameters.push(p);
    }
    let mut imp = Implementation::default();
    imp.model_name = Some("FP".into());
    imp.model_type = Some("PCBLIB".into());
    let mut map = MapDefiner::default();
    map.designator_interface = Some("1".into());
    map.designator_implementations = vec!["1".into()];
    imp.map_definers.push(map);
    comp.implementations.push(imp);
    let mut lib = altium::sch::Library::default();
    lib.components.push(comp);
    let bytes = lib.to_bytes().unwrap();
    let has = |s: &str| bytes.windows(s.len()).any(|w| w == s.as_bytes());
    assert!(
        has("|OWNERINDEX=5"),
        "the 45 must point at the 44 at index 5"
    );
    assert!(has("|RECORD=46|OWNERINDEX=6"));
    assert!(has("|RECORD=48|OWNERINDEX=6"));
    assert!(
        has("|OWNERINDEX=7"),
        "the 47 must point at the 46 at index 7"
    );
    assert!(!has("|OWNERINDEX=4"));
}

fn pcb_bytes_contain(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle)
}

/// One stream of a written compound file (searching the raw file would miss
/// strings that straddle a 64-byte mini-sector boundary).
fn pcb_stream(file: &[u8], path: &str) -> Vec<u8> {
    let mut cf = altium::compound::CompoundFile::open(file.to_vec()).unwrap();
    cf.read_stream(path).unwrap()
}

#[test]
fn pcblib_unmodelled_flag_bits_round_trip() {
    let mut text = pcb::Text::default();
    text.text = ".Designator".into();
    text.layer = 63;
    text.flags_extra = 0x08; // Altium sets bit 3 on every text/region/body it writes
    let mut body = altium::pcb::primitives::ComponentBody::default();
    body.layer_name = "MECHANICAL2".into();
    body.flags_extra = 0x08;
    let mut comp = pcb::Component::new("FLAGS");
    comp.texts.push(text);
    comp.component_bodies.push(body);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.components[0].texts[0].flags_extra, 0x08);
    assert_eq!(parsed.components[0].component_bodies[0].flags_extra, 0x08);
    assert!(!parsed.components[0].texts[0].is_locked);
}

#[test]
fn pcblib_body_parameters_use_altiums_form() {
    let mut body = altium::pcb::primitives::ComponentBody::default();
    body.layer_name = "MECHANICAL2".into();
    body.overall_height = Coord::from_mils(56.2992);
    body.model_type = 1;
    body.model_embed = true;
    body.model_name = Some("X.step".into());
    let mut comp = pcb::Component::new("BODY");
    comp.component_bodies.push(body);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let file = lib.to_bytes().unwrap();
    let bytes = pcb_stream(&file, "BODY/Data");
    // no leading separator, NAME kept as a single space, zero coords as "0mil"
    assert!(pcb_bytes_contain(
        &bytes,
        b"V7_LAYER=MECHANICAL2|NAME= |KIND=0|"
    ));
    assert!(!pcb_bytes_contain(&bytes, b"|V7_LAYER=MECHANICAL2"));
    assert!(pcb_bytes_contain(
        &bytes,
        b"|CAVITYHEIGHT=0mil|STANDOFFHEIGHT=0mil|OVERALLHEIGHT=56.2992mil|"
    ));
    assert!(pcb_bytes_contain(&bytes, b"|IDENTIFIER=|TEXTURE=|MODELID="));
    assert!(pcb_bytes_contain(
        &bytes,
        b"|MODEL.EMBED=TRUE|MODEL.NAME=X.step|MODEL.2D.X=0mil|"
    ));
    let parsed = pcb::Library::from_bytes(file).unwrap();
    assert_eq!(
        parsed.components[0].component_bodies[0].overall_height,
        Coord::from_mils(56.2992)
    );
}

#[test]
fn pcblib_primitive_order_is_preserved() {
    let mut region = altium::pcb::primitives::Region::default();
    region.layer = 33;
    for (x, y) in [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)] {
        region.outline.push(altium::CoordPoint::new(
            Coord::from_mils(x),
            Coord::from_mils(y),
        ));
    }
    let mut body = altium::pcb::primitives::ComponentBody::default();
    body.layer_name = "MECHANICAL2".into();
    let mut comp = pcb::Component::new("ORDER");
    comp.regions.push(region.clone());
    comp.component_bodies.push(body.clone());
    comp.primitive_order = vec![12, 11]; // body first, as many Altium libraries store it
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.components[0].primitive_order, vec![12, 11]);

    // Without an order the writer groups by kind.
    let mut comp = pcb::Component::new("GROUPED");
    comp.regions.push(region);
    comp.component_bodies.push(body);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.components[0].primitive_order, vec![11, 12]);
}

#[test]
fn pcblib_step_payload_round_trips_non_utf8_bytes() {
    // GBK bytes that are not valid UTF-8 (0xC0 never starts a UTF-8 sequence)
    let raw: Vec<u8> =
        b"ISO-10303-21;\r\n#1 = PRODUCT ( '\xcd\xb9\xcc\xa8-\xc0\xad\xc9\xec' ) ;\r\n".to_vec();
    let mut model = altium::pcb::model3d::Model3d::default();
    model.id = "{11111111-2222-3333-4444-555555555555}".into();
    model.name = "gbk.step".into();
    model.is_embedded = true;
    model.model_source = "Undefined".into();
    model.step_data = raw.iter().map(|&b| b as char).collect();
    model.step_data_is_latin1 = true;
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.models.push(model);
    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    let m = &parsed.models[0];
    assert!(m.step_data_is_latin1, "non-UTF-8 payload must be flagged");
    let back: Vec<u8> = m.step_data.chars().map(|c| c as u8).collect();
    assert_eq!(back, raw, "payload bytes must survive unchanged");

    // Plain ASCII/UTF-8 payloads stay ordinary strings.
    let mut model = altium::pcb::model3d::Model3d::default();
    model.id = "{11111111-2222-3333-4444-555555555556}".into();
    model.name = "ascii.step".into();
    model.is_embedded = true;
    model.step_data = "ISO-10303-21;\r\n".into();
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.models.push(model);
    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    assert!(!parsed.models[0].step_data_is_latin1);
    assert_eq!(parsed.models[0].step_data, "ISO-10303-21;\r\n");
}

#[test]
fn pcblib_footprint_parameters_put_typed_keys_first() {
    let mut comp = pcb::Component::new("PARAMS");
    comp.additional_parameters
        .insert("AREA".into(), "123.000000".into());
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let bytes = lib.to_bytes().unwrap();
    let pos = |needle: &[u8]| bytes.windows(needle.len()).position(|w| w == needle);
    let p = pos(b"|PATTERN=PARAMS|HEIGHT=0mil|").expect("typed keys with Altium's mil form");
    let a = pos(b"|AREA=123.000000").expect("extra parameter");
    assert!(p < a, "PATTERN/HEIGHT must precede the extra parameters");
}

#[test]
fn pcblib_body_checksum_and_model_source_use_altiums_form() {
    let mut body = altium::pcb::primitives::ComponentBody::default();
    body.layer_name = "MECHANICAL1".into();
    body.model_checksum = -595_556_585; // the signed form of 3699410711
    body.model_source = None; // older bodies have no MODEL.MODELSOURCE at all
    let mut extra = std::collections::BTreeMap::new();
    extra.insert("BODYOVERRIDECOLOR".to_string(), "TRUE".to_string());
    extra.insert("MODEL.SNAPCOUNT".to_string(), "1".to_string());
    extra.insert("MODEL.S0X".to_string(), "1mil".to_string());
    extra.insert("MODEL.S0Y".to_string(), "2mil".to_string());
    extra.insert("MODEL.S0Z".to_string(), "3mil".to_string());
    body.additional_parameters = Some(extra);
    let mut comp = pcb::Component::new("BODY2");
    comp.component_bodies.push(body);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let file = lib.to_bytes().unwrap();
    let data = pcb_stream(&file, "BODY2/Data");
    assert!(pcb_bytes_contain(&data, b"|MODEL.CHECKSUM=3699410711|"));
    assert!(!pcb_bytes_contain(&data, b"MODEL.MODELSOURCE"));
    // ARCRESOLUTION is written twice, and the extra keys sit in Altium's slots
    assert!(pcb_bytes_contain(
        &data,
        b"|UNIONINDEX=0|ARCRESOLUTION=1mil|ISSHAPEBASED="
    ));
    assert!(pcb_bytes_contain(
        &data,
        b"|BODYPROJECTION=0|ARCRESOLUTION=1mil|BODYCOLOR3D="
    ));
    assert!(pcb_bytes_contain(
        &data,
        b"|BODYOPACITY3D=1.000|BODYOVERRIDECOLOR=TRUE|IDENTIFIER="
    ));
    assert!(pcb_bytes_contain(
        &data,
        b"|MODEL.3D.DZ=0mil|MODEL.SNAPCOUNT=1|MODEL.S0X=1mil|MODEL.S0Y=2mil|MODEL.S0Z=3mil|MODEL.MODELTYPE="
    ));
    let parsed = pcb::Library::from_bytes(file).unwrap();
    let b = &parsed.components[0].component_bodies[0];
    assert_eq!(b.model_checksum, -595_556_585);
    assert_eq!(b.model_source, None);
}

#[test]
fn pcblib_footprint_parameters_follow_altiums_order() {
    let mut comp = pcb::Component::new("FID");
    comp.description = Some(String::new());
    comp.item_guid = Some(String::new());
    comp.item_revision_guid = Some(String::new());
    comp.additional_parameters
        .insert("AREA".into(), "1395004296608.000000".into());
    comp.additional_parameters
        .insert("COMPONENTKIND".into(), "5".into());
    comp.additional_parameters
        .insert("GRIDSNGUIDE".into(), "GU0_TYPE<EQ>Line".into());
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let data = pcb_stream(&lib.to_bytes().unwrap(), "FID/Parameters");
    assert!(pcb_bytes_contain(
        &data,
        b"|PATTERN=FID|HEIGHT=0mil|DESCRIPTION=|GRIDSNGUIDE=GU0_TYPE<EQ>Line|ITEMGUID=|REVISIONGUID=|COMPONENTKIND=5|AREA=1395004296608.000000"
    ));
}

#[test]
fn pcblib_library_parameters_keep_order_markers_and_raw_values() {
    use altium::parameter::ParameterMap;
    let mut user = ParameterMap::new();
    user.insert("FILENAME", "X.PcbLib");
    user.insert("KIND", "Protel_Advanced_PCB_Library");
    user.insert("LAYER5DIELMATERIAL", "FR-4\r");
    user.insert("RECORD", "Board");
    user.insert("LAYER6NAME", "Mid-Layer 5");
    // Altium restarts the Board record every few layers; the marker repeats.
    let text = "|FILENAME=X.PcbLib|KIND=Protel_Advanced_PCB_Library|LAYER5DIELMATERIAL=FR-4\r|RECORD=Board|LAYER6NAME=Mid-Layer 5|RECORD=Board|LAYER11NAME=Mid-Layer 10";
    let user = ParameterMap::parse(text);
    assert_eq!(user.len(), 7, "every RECORD marker is kept");
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.library_parameters = Some(user);
    let file = lib.to_bytes().unwrap();
    let data = pcb_stream(&file, "Library/Data");
    assert!(pcb_bytes_contain(&data, text.as_bytes()));
    assert!(!pcb_bytes_contain(&data, b"|HEADER="));
    assert!(!pcb_bytes_contain(&data, b"|WEIGHT="));
    let parsed = pcb::Library::from_bytes(file).unwrap();
    let names: Vec<&str> = parsed
        .library_parameters
        .as_ref()
        .unwrap()
        .iter()
        .map(|(n, _, _)| n)
        .take(7)
        .collect();
    assert_eq!(
        names,
        [
            "FILENAME",
            "KIND",
            "LAYER5DIELMATERIAL",
            "RECORD",
            "LAYER6NAME",
            "RECORD",
            "LAYER11NAME"
        ]
    );
    assert_eq!(
        parsed
            .library_parameters
            .as_ref()
            .unwrap()
            .get("LAYER5DIELMATERIAL"),
        Some("FR-4\r")
    );
}

#[test]
fn pcblib_text_keeps_extended_mechanical_layer_id() {
    let mut t1 = pcb::Text::default();
    t1.text = ".Designator".into();
    t1.layer = 72; // Altium clamps Mechanical 29 to the Mechanical 16 byte
    t1.layer_v7 = 0x0102_001D;
    let mut t2 = pcb::Text::default();
    t2.text = ".Comment".into();
    t2.layer = 63;
    let mut comp = pcb::Component::new("TXT");
    comp.texts.push(t1);
    comp.texts.push(t2);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.components[0].texts[0].layer_v7, 0x0102_001D);
    assert_eq!(parsed.components[0].texts[1].layer_v7, 0);
    assert_eq!(parsed.components[0].texts[1].layer, 63);
}

#[test]
fn pcblib_typed_flag_bits_round_trip() {
    use altium::enums::PcbStrokeFont;
    use altium::pcb::{Arc, Region, Track};
    let mut pad = pcb::Pad::default();
    pad.designator = Some("1".into());
    pad.is_testpoint_fab_top = true;
    pad.is_testpoint_fab_bottom = true;
    let mut track = Track::default();
    track.is_polygon_outline = true;
    let mut arc = Arc::default();
    arc.radius = Coord::from_mils(10.0);
    arc.is_polygon_outline = true;
    let mut region = Region::default();
    region.outline = vec![
        CoordPoint::new(Coord::from_mils(0.0), Coord::from_mils(0.0)),
        CoordPoint::new(Coord::from_mils(10.0), Coord::from_mils(0.0)),
        CoordPoint::new(Coord::from_mils(10.0), Coord::from_mils(10.0)),
    ];
    region.is_teardrop = true;
    let mut text = pcb::Text::default();
    text.text = "x".into();
    text.stroke_font = PcbStrokeFont::SansSerif;
    let mut comp = pcb::Component::new("FLG");
    comp.pads.push(pad);
    comp.tracks.push(track);
    comp.arcs.push(arc);
    comp.regions.push(region);
    comp.texts.push(text);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    let c = &parsed.components[0];
    assert!(c.pads[0].is_testpoint_fab_top && c.pads[0].is_testpoint_fab_bottom);
    assert!(c.tracks[0].is_polygon_outline);
    assert!(c.arcs[0].is_polygon_outline);
    assert!(c.regions[0].is_teardrop);
    // new primitives carry the bit Altium sets on everything it creates
    assert_eq!(c.pads[0].flags_extra, 0x08);
    assert_eq!(c.tracks[0].flags_extra, 0x08);
    assert_eq!(c.texts[0].stroke_font, PcbStrokeFont::SansSerif);
    // Altium's stroke-font table is 1-based
    assert_eq!(i32::from(PcbStrokeFont::Default), 1);
    assert_eq!(i32::from(PcbStrokeFont::SansSerif), 2);
    assert_eq!(i32::from(PcbStrokeFont::Serif), 3);
}

#[test]
fn pcblib_section_keys_keep_file_order() {
    let long_a = "HEADER-TH-FEMALE-2x8P-P2.54MM-H-ODD-EVEN";
    let long_b = "HEADER-TH-FEMALE-2x8P-P2.54MM-H-SHORT-ODD-EVEN";
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(pcb::Component::new(long_a));
    lib.components.push(pcb::Component::new(long_b));
    lib.section_key_order = vec![long_b.to_string(), long_a.to_string()];
    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.section_key_order, [long_b, long_a]);
    assert_eq!(parsed.components.len(), 2);
}

#[test]
fn pcblib_unnamed_model_record_uses_altiums_form() {
    let mut model = altium::pcb::model3d::Model3d::default();
    model.id = "{A2ADB3E4-F0D2-4BEA-8095-3D851A0AD734}".into();
    model.name = String::new();
    model.is_embedded = true;
    model.model_source = "Undefined".into();
    model.checksum = -1_743_349_208;
    model.step_data = "ISO-10303-21;".into();
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.models.push(model);
    let data = pcb_stream(&lib.to_bytes().unwrap(), "Library/Models/Data");
    assert!(pcb_bytes_contain(
        &data,
        b"|NAME=|EMBED=TRUE|MODELSOURCE=Undefined|ID={A2ADB3E4-F0D2-4BEA-8095-3D851A0AD734}|ROTX=0.000|ROTY=0.000|ROTZ=0.000|DZ=0|CHECKSUM=-1743349208\0"
    ));
}

#[test]
fn pcblib_component_params_toc_matches_altium() {
    let mut a = pcb::Component::new("BZM2");
    a.height = Coord::from_mils(31.4961);
    let mut pad = pcb::Pad::default();
    pad.designator = Some("1".into());
    a.pads.push(pad);
    let mut b = pcb::Component::new("C0201");
    b.description = Some("cap".into());
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(a);
    lib.components.push(b);
    let file = lib.to_bytes().unwrap();
    let text = b"Name=BZM2|Pad Count=1|Height=31.4961|Description=\r\nName=C0201|Pad Count=0|Height=0|Description=cap\r\n\0";
    let mut expected = ((text.len()) as u32).to_le_bytes().to_vec();
    expected.extend_from_slice(text);
    assert_eq!(
        pcb_stream(&file, "Library/ComponentParamsTOC/Data"),
        expected
    );
    assert_eq!(
        pcb_stream(&file, "Library/ComponentParamsTOC/Header"),
        1u32.to_le_bytes()
    );
    // the derived table is not carried as an opaque stream
    let parsed = pcb::Library::from_bytes(file).unwrap();
    assert!(
        !parsed
            .additional_library_streams
            .keys()
            .any(|k| k.starts_with("ComponentParamsTOC"))
    );
}

#[test]
fn pcblib_track_arc_fill_records_use_altiums_full_layout() {
    use altium::pcb::{Arc, Fill, Track};
    let mut track = Track::default();
    track.layer = 64; // Mechanical 8
    track.start = CoordPoint::new(Coord::from_mils(-10.0), Coord::from_mils(0.0));
    track.end = CoordPoint::new(Coord::from_mils(10.0), Coord::from_mils(0.0));
    let mut arc = Arc::default();
    arc.layer = 33;
    arc.radius = Coord::from_mils(10.0);
    let mut fill = Fill::default();
    fill.layer = 63;
    fill.corner2 = CoordPoint::new(Coord::from_mils(10.0), Coord::from_mils(10.0));
    let mut comp = pcb::Component::new("LAYOUT");
    comp.tracks.push(track);
    comp.arcs.push(arc);
    comp.fills.push(fill);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let data = pcb_stream(&lib.to_bytes().unwrap(), "LAYOUT/Data");
    // walk the records: [u32 name block][type u8][u32 len][block]...
    let mut p = 4 + u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let mut seen = std::collections::BTreeMap::new();
    while p + 5 <= data.len() {
        let t = data[p];
        let len = u32::from_le_bytes(data[p + 1..p + 5].try_into().unwrap()) as usize;
        let block = &data[p + 5..p + 5 + len];
        seen.insert(t, block.to_vec());
        p += 5 + len;
    }
    let track = &seen[&4];
    assert_eq!(track.len(), 49);
    assert_eq!(&track[33..49], &[0, 0, 0, 0, 0, 0, 0, 0, 0x08, 0x00, 0x02, 0x01, 0, 0, 0, 0]);
    let arc = &seen[&1];
    assert_eq!(arc.len(), 60);
    assert_eq!(&arc[45..60], &[0, 0, 0, 0, 0, 0, 0, 0x06, 0x00, 0x03, 0x01, 0, 0, 0, 0]);
    let fill = &seen[&6];
    assert_eq!(fill.len(), 50);
    assert_eq!(&fill[37..50], &[0, 0, 0, 0, 0, 0x07, 0x00, 0x02, 0x01, 0, 0, 0, 0]);
}

#[test]
fn pcblib_records_split_and_lint_cleanly() {
    use altium::pcb::lint::{check_pcblib, step_checksum};
    use altium::pcb::records::split_footprint_records;
    use altium::pcb::{Arc, Track};
    let mut comp = pcb::Component::new("LINT");
    let mut pad = pcb::Pad::default();
    pad.designator = Some("1".into());
    comp.pads.push(pad);
    let mut track = Track::default();
    track.layer = 33;
    track.end = CoordPoint::new(Coord::from_mils(10.0), Coord::from_mils(0.0));
    comp.tracks.push(track);
    let mut arc = Arc::default();
    arc.radius = Coord::from_mils(5.0);
    comp.arcs.push(arc);
    let mut text = pcb::Text::default();
    text.text = ".Designator".into();
    text.layer = 63;
    comp.texts.push(text);
    let mut lib = pcb::Library::default();
    lib.unique_id = "AAAAAAAA".into();
    lib.components.push(comp);
    let file = lib.to_bytes().unwrap();
    let data = pcb_stream(&file, "LINT/Data");
    let (name, records) = split_footprint_records(&data).unwrap();
    assert_eq!(name, "LINT");
    let kinds: Vec<u8> = records.iter().map(|r| r.kind).collect();
    assert_eq!(kinds.iter().copied().collect::<std::collections::BTreeSet<u8>>(), [1u8, 2, 4, 5].into_iter().collect());
    let text = records.iter().find(|r| r.kind == 5).unwrap();
    assert_eq!(text.text().as_deref(), Some(".Designator"));
    assert_eq!(text.layer(), Some(63));
    assert_eq!(text.main_block().len(), 252);
    let pad = records.iter().find(|r| r.kind == 2).unwrap();
    assert_eq!(pad.main_block().len(), 202);
    let mut cf = altium::compound::CompoundFile::open(file).unwrap();
    let problems = check_pcblib(&mut cf).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    // checksum: weight 1 for the first byte, then the byte index
    assert_eq!(step_checksum(b"AB"), 65 + 66);
    assert_eq!(step_checksum(b"ABC"), 65 + 66 + 2 * 67);
}
