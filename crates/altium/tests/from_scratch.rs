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
    doc.nets.push(pcb::Net { name: "VCC".into(), ..Default::default() });
    doc.nets.push(pcb::Net { name: "GND".into(), ..Default::default() });

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
    assert_eq!(
        params.get("HEADER").map(|s| s.as_str()),
        Some("PCB 6.0 Binary Library File")
    );
    assert_eq!(
        params.get("KIND").map(|s| s.as_str()),
        Some("Protel_Advanced_PCB_Library")
    );
    assert!(params.contains_key("V9_MASTERSTACK_STYLE"));
    assert_eq!(
        params.get("V9_MASTERSTACK_NAME").map(|s| s.as_str()),
        Some("Master layer stack")
    );
    assert_eq!(
        params.get("V9_STACK_LAYER3_NAME").map(|s| s.as_str()),
        Some("Top Layer"),
        "the canonical 2-layer stack puts Top Layer at index 3"
    );
    assert_eq!(
        params.get("V9_STACK_LAYER5_NAME").map(|s| s.as_str()),
        Some("Bottom Layer")
    );
    assert!(
        params.contains_key("V9_CACHE_LAYER0_NAME"),
        "cache layers should be present"
    );
}

#[test]
fn pcblib_user_supplied_params_keep_their_values() {
    use std::collections::BTreeMap;
    // If the caller has populated library_parameters, their entries take
    // precedence over the defaults; only missing keys get filled in.
    let mut lib = pcb::Library::default();
    let mut user = BTreeMap::new();
    user.insert(
        "V9_MASTERSTACK_NAME".to_string(),
        "Custom Stack".to_string(),
    );
    user.insert(
        "HEADER".to_string(),
        "PCB 6.0 Binary Library File".to_string(),
    );
    lib.library_parameters = Some(user);
    lib.components.push(pcb::Component::new("X"));

    let parsed = pcb::Library::from_bytes(lib.to_bytes().unwrap()).unwrap();
    let params = parsed.library_parameters.as_ref().unwrap();
    assert_eq!(
        params.get("V9_MASTERSTACK_NAME").map(|s| s.as_str()),
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
