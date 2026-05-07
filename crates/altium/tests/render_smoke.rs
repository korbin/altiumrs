//! Smoke tests for the `render` feature.
//!
//! `cargo test -p altium --features render --test render_smoke`

#![cfg(feature = "render")]
#![allow(clippy::field_reassign_with_default)]

use altium::coord::{Coord, CoordPoint};
use altium::enums::{PadShape, PinElectricalType, PinOrientation, PowerPortStyle};
use altium::render::{RenderOptions, overline};
use altium::{pcb, sch};

fn build_synth_footprint() -> pcb::Component {
    let mut pad1 = pcb::Pad::default();
    pad1.designator = Some("1".into());
    pad1.location = CoordPoint::new(Coord::from_mils(-50.0), Coord::ZERO);
    pad1.size_top = CoordPoint::new(Coord::from_mils(40.0), Coord::from_mils(60.0));
    pad1.size_middle = pad1.size_top;
    pad1.size_bottom = pad1.size_top;
    pad1.shape_top = PadShape::Rectangular;
    pad1.shape_middle = PadShape::Rectangular;
    pad1.shape_bottom = PadShape::Rectangular;
    pad1.layer = 1;

    let mut pad2 = pcb::Pad::default();
    pad2.designator = Some("2".into());
    pad2.location = CoordPoint::new(Coord::from_mils(50.0), Coord::ZERO);
    pad2.size_top = pad1.size_top;
    pad2.size_middle = pad1.size_top;
    pad2.size_bottom = pad1.size_top;
    pad2.shape_top = PadShape::Round;
    pad2.shape_middle = PadShape::Round;
    pad2.shape_bottom = PadShape::Round;
    pad2.layer = 1;

    let mut track = pcb::Track::default();
    track.start = CoordPoint::new(Coord::from_mils(-30.0), Coord::ZERO);
    track.end = CoordPoint::new(Coord::from_mils(30.0), Coord::ZERO);
    track.width = Coord::from_mils(8.0);
    track.layer = 1;

    let mut component = pcb::Component::new("R0402");
    component.pads.push(pad1);
    component.pads.push(pad2);
    component.tracks.push(track);
    component
}

#[test]
fn render_pcb_to_svg() {
    let component = build_synth_footprint();
    let svg = component.render_svg(RenderOptions::default());
    assert!(svg.starts_with("<svg "));
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("<rect"), "expected pad rects, got: {svg}");
    assert!(svg.contains("<line"), "expected track line, got: {svg}");
    // Round pad should render as ellipse, rect pad as rect.
    assert!(svg.contains("<ellipse"), "expected round pad ellipse");
}

#[test]
fn render_pcb_to_png() {
    let component = build_synth_footprint();
    let png = component.render_png(RenderOptions::default()).expect("png");
    assert_eq!(&png[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
}

#[test]
fn render_empty_component_does_not_panic() {
    let comp = pcb::Component::new("EMPTY");
    let svg = comp.render_svg(RenderOptions::default());
    assert!(svg.contains("<svg"));
    let png = comp.render_png(RenderOptions::default()).expect("png");
    assert!(png.len() > 8);
}

#[test]
fn pad_shape_variants_render() {
    use sch::primitives::Junction;
    let _ = Junction::default(); // sanity: imports compile
    for shape in [
        PadShape::Round,
        PadShape::Rectangular,
        PadShape::Octagonal,
        PadShape::RoundedRectangle,
    ] {
        let mut pad = pcb::Pad::default();
        pad.location = CoordPoint::new(Coord::ZERO, Coord::ZERO);
        pad.size_top = CoordPoint::new(Coord::from_mils(40.0), Coord::from_mils(60.0));
        pad.size_middle = pad.size_top;
        pad.size_bottom = pad.size_top;
        pad.shape_top = shape;
        pad.shape_middle = shape;
        pad.shape_bottom = shape;
        pad.corner_radius_percentage = 25;
        pad.layer = 1;
        pad.designator = Some("X".into());
        let mut comp = pcb::Component::new(format!("PAD-{shape:?}"));
        comp.pads.push(pad);
        let svg = comp.render_svg(RenderOptions::default());
        match shape {
            PadShape::Round => assert!(svg.contains("<ellipse"), "round → ellipse"),
            PadShape::Rectangular => assert!(svg.contains("<rect"), "rect → rect"),
            PadShape::Octagonal => assert!(svg.contains("<polygon"), "oct → polygon"),
            PadShape::RoundedRectangle => assert!(svg.contains("rx="), "rounded → rect with rx"),
        }
    }
}

fn build_synth_sch() -> sch::Component {
    use sch::primitives::{Pin, Rectangle};
    let mut comp = sch::Component::new("U1");
    let mut body = Rectangle::default();
    body.corner1 = CoordPoint::new(Coord::from_mils(-40.0), Coord::from_mils(-30.0));
    body.corner2 = CoordPoint::new(Coord::from_mils(40.0), Coord::from_mils(30.0));
    body.color = 0x000000;
    body.fill_color = 0xFFFFE0;
    body.is_filled = true;
    comp.rectangles.push(body);

    for (i, (orient, off)) in [
        (PinOrientation::Right, 20.0),
        (PinOrientation::Left, -20.0),
        (PinOrientation::Up, 0.0),
        (PinOrientation::Down, 0.0),
    ]
    .iter()
    .enumerate()
    {
        let mut pin = Pin::default();
        pin.designator = Some((i + 1).to_string());
        pin.name = Some(format!("P{}", i + 1));
        pin.location = match orient {
            PinOrientation::Right => {
                CoordPoint::new(Coord::from_mils(-40.0), Coord::from_mils(*off))
            }
            PinOrientation::Left => CoordPoint::new(Coord::from_mils(40.0), Coord::from_mils(*off)),
            PinOrientation::Up => CoordPoint::new(Coord::from_mils(*off), Coord::from_mils(-30.0)),
            PinOrientation::Down => CoordPoint::new(Coord::from_mils(*off), Coord::from_mils(30.0)),
        };
        pin.length = Coord::from_mils(20.0);
        pin.orientation = *orient;
        pin.electrical_type = if i == 0 {
            PinElectricalType::Input
        } else if i == 1 {
            PinElectricalType::Output
        } else {
            PinElectricalType::Passive
        };
        pin.show_name = true;
        pin.show_designator = true;
        comp.pins.push(pin);
    }
    comp
}

#[test]
fn render_sch_to_svg_with_pin_glyphs() {
    let comp = build_synth_sch();
    let svg = comp.render_svg(RenderOptions::default());
    assert!(svg.contains("<svg "));
    // Body rectangle.
    assert!(svg.contains("<rect"));
    // Pin lines (4 pins → at least 4 lines).
    let line_count = svg.matches("<line ").count();
    assert!(line_count >= 4, "expected ≥ 4 pin lines, got {line_count}");
    // Pin name / designator text.
    assert!(svg.contains("<text"));
    // Input/Output electrical-symbol triangles: pin 0 is Input, pin 1 Output.
    let polygon_count = svg.matches("<polygon ").count();
    assert!(polygon_count >= 2, "expected pin glyph triangles");
}

#[test]
fn render_sch_to_png_works() {
    let comp = build_synth_sch();
    let png = comp.render_png(RenderOptions::default()).expect("png");
    assert_eq!(&png[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
}

#[test]
fn power_port_styles_render() {
    use sch::primitives::PowerObject;
    let mut comp = sch::Component::new("PWR");
    for (i, style) in [
        PowerPortStyle::Circle,
        PowerPortStyle::Arrow,
        PowerPortStyle::Bar,
        PowerPortStyle::Wave,
        PowerPortStyle::PowerGround,
        PowerPortStyle::SignalGround,
        PowerPortStyle::Earth,
        PowerPortStyle::GostArrow,
        PowerPortStyle::GostPowerGround,
        PowerPortStyle::GostEarth,
        PowerPortStyle::GostBar,
    ]
    .iter()
    .enumerate()
    {
        let mut po = PowerObject::default();
        po.location = CoordPoint::new(Coord::from_mils(i as f64 * 60.0), Coord::ZERO);
        po.text = format!("V{i}");
        po.show_net_name = true;
        po.style = *style;
        comp.power_objects.push(po);
    }
    let svg = comp.render_svg(RenderOptions::default());
    assert!(svg.contains("<svg "));
    // Each power glyph is composed of lines / circles. Count drawing ops > 11.
    let total_ops = svg.matches("<line").count()
        + svg.matches("<circle").count()
        + svg.matches("<ellipse").count();
    assert!(total_ops >= 11, "got {total_ops} ops");
}

#[test]
fn overline_parser_segments_correctly() {
    let segs = overline::parse("RESET\\");
    assert_eq!(segs.len(), 2);
    assert!(!segs[0].overline);
    assert!(segs[1].overline);
    assert_eq!(segs[1].text, "T");

    let segs = overline::parse("R\\E\\S\\E\\T\\");
    // All overlined → single combined segment.
    assert_eq!(segs.len(), 1);
    assert!(segs[0].overline);
    assert_eq!(segs[0].text, "RESET");

    let plain = overline::parse("plain");
    assert_eq!(plain.len(), 1);
    assert!(!plain[0].overline);
}

#[test]
fn pcb_render_uses_layer_priority() {
    // Build a component where the *vector-of-tracks* order DIFFERS from the
    // expected on-screen draw order. We push tracks on the Top layer (priority 6)
    // BEFORE tracks on the Bottom layer (priority 0). The renderer must sort
    // by priority and emit the bottom-layer track first in the SVG output.
    let mut top = pcb::Track::default();
    top.start = CoordPoint::new(Coord::from_mils(-50.0), Coord::ZERO);
    top.end = CoordPoint::new(Coord::from_mils(50.0), Coord::ZERO);
    top.width = Coord::from_mils(8.0);
    top.layer = 1; // Top — priority 6

    let mut bottom = pcb::Track::default();
    bottom.start = CoordPoint::new(Coord::from_mils(-50.0), Coord::from_mils(10.0));
    bottom.end = CoordPoint::new(Coord::from_mils(50.0), Coord::from_mils(10.0));
    bottom.width = Coord::from_mils(8.0);
    bottom.layer = 32; // Bottom — priority 0

    let mut overlay = pcb::Track::default();
    overlay.start = CoordPoint::new(Coord::from_mils(-50.0), Coord::from_mils(-10.0));
    overlay.end = CoordPoint::new(Coord::from_mils(50.0), Coord::from_mils(-10.0));
    overlay.width = Coord::from_mils(8.0);
    overlay.layer = 33; // Top Overlay — priority 2

    // Push in the WRONG order (top, then overlay, then bottom) — sorter must reorder.
    let mut comp = pcb::Component::new("LAYERS");
    comp.tracks.push(top);
    comp.tracks.push(overlay);
    comp.tracks.push(bottom);

    let svg = comp.render_svg(RenderOptions::default());
    // Each track is one <line> with a stroke="#XXXXXX" attribute; locate by colour.
    let pos_bottom = svg.find("stroke=\"#0000ff\"").expect("bottom layer line");
    let pos_overlay = svg.find("stroke=\"#ffff00\"").expect("top overlay line");
    let pos_top = svg.find("stroke=\"#ff0000\"").expect("top layer line");
    // Sorted draw order: bottom (0) → overlay (2) → top (6).
    assert!(
        pos_bottom < pos_overlay,
        "bottom must draw before overlay (got bottom={pos_bottom}, overlay={pos_overlay})",
    );
    assert!(
        pos_overlay < pos_top,
        "overlay must draw before top (got overlay={pos_overlay}, top={pos_top})",
    );
}

#[test]
fn pcb_render_pads_after_tracks_same_layer() {
    // Within the same priority, type tiebreaker should put pads after tracks
    // so pad bodies are visible at trace junctions.
    let mut track = pcb::Track::default();
    track.start = CoordPoint::new(Coord::ZERO, Coord::ZERO);
    track.end = CoordPoint::new(Coord::from_mils(100.0), Coord::ZERO);
    track.width = Coord::from_mils(10.0);
    track.layer = 1;

    let mut pad = pcb::Pad::default();
    pad.designator = Some("1".into());
    pad.location = CoordPoint::new(Coord::from_mils(50.0), Coord::ZERO);
    pad.size_top = CoordPoint::new(Coord::from_mils(40.0), Coord::from_mils(40.0));
    pad.size_middle = pad.size_top;
    pad.size_bottom = pad.size_top;
    pad.shape_top = PadShape::Rectangular;
    pad.shape_middle = PadShape::Rectangular;
    pad.shape_bottom = PadShape::Rectangular;
    pad.layer = 1;

    // Push pad first, track second — sort must still emit track-then-pad.
    let mut comp = pcb::Component::new("PAD-AFTER-TRACK");
    comp.pads.push(pad);
    comp.tracks.push(track);

    let svg = comp.render_svg(RenderOptions::default());
    // The track is a <line>; the pad body is a <rect>. Track must precede pad.
    let pos_track = svg.find("<line ").expect("track line");
    let pos_pad = svg.find("<rect ").expect("pad rect");
    // The first <rect> may be the SVG background; skip it if so.
    let pos_pad = if let Some(stripped) = svg.get(pos_pad..) {
        if stripped.contains("fill=\"#ffffff\"") {
            // Background — find next <rect>.
            let next = svg[pos_pad + 1..].find("<rect ").expect("pad rect");
            pos_pad + 1 + next
        } else {
            pos_pad
        }
    } else {
        pos_pad
    };
    assert!(
        pos_track < pos_pad,
        "track ({pos_track}) must precede pad ({pos_pad}) on same layer",
    );
}

#[test]
fn layer_color_known_layers() {
    use altium::render::layer_colors::color_for;
    // Top is red, bottom is blue.
    let top = color_for(1);
    let bottom = color_for(32);
    assert_eq!(top.r, 0xFF);
    assert_eq!(bottom.b, 0xFF);
    // Top overlay yellow.
    let overlay = color_for(33);
    assert_eq!(overlay.r, 0xFF);
    assert_eq!(overlay.g, 0xFF);
    assert_eq!(overlay.b, 0x00);
}

// ─── Embedded sub-board recursive rendering ───────────────────────────────

use altium::pcb::{BoardLoader, EmbeddedBoard, FileBoardLoader};
use altium::Result as AltiumResult;

/// In-memory loader keyed by `document_path`. Lets render tests exercise the
/// recursive path without touching the filesystem.
struct InMemoryLoader {
    map: std::collections::HashMap<String, Vec<u8>>,
    calls: std::cell::Cell<usize>,
}

impl InMemoryLoader {
    fn new() -> Self {
        Self {
            map: std::collections::HashMap::new(),
            calls: std::cell::Cell::new(0),
        }
    }
    fn insert(&mut self, path: &str, bytes: Vec<u8>) {
        self.map.insert(path.to_string(), bytes);
    }
}

impl BoardLoader for InMemoryLoader {
    fn load(&self, document_path: &str) -> AltiumResult<Option<Vec<u8>>> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.map.get(document_path).cloned())
    }
}

fn build_marker_subdoc() -> pcb::Document {
    // A sub-document with a single very-distinctive arc — easy to count in
    // the resulting SVG.
    let mut sub = pcb::Document::default();
    let mut arc = pcb::Arc::default();
    arc.center = CoordPoint::new(Coord::ZERO, Coord::ZERO);
    arc.radius = Coord::from_mils(20.0);
    arc.start_angle = 0.0;
    arc.end_angle = 360.0;
    arc.width = Coord::from_mils(5.0);
    arc.layer = 33; // top overlay → yellow
    arc.enabled = true;
    sub.arcs.push(arc);
    sub
}

#[test]
fn embedded_board_renders_placeholder_without_loader() {
    let mut parent = pcb::Document::default();
    let mut board = EmbeddedBoard::default();
    board.document_path = Some("Missing.PcbDoc".into());
    board.layer = 1;
    board.x1_location = Coord::from_mils(0.0);
    board.y1_location = Coord::from_mils(0.0);
    board.x2_location = Coord::from_mils(200.0);
    board.y2_location = Coord::from_mils(150.0);
    board.col_count = 1;
    board.row_count = 1;
    parent.embedded_boards.push(board);

    let svg = parent.render_svg(RenderOptions::default());
    // The placeholder draws a labelled rectangle. Both should appear.
    assert!(
        svg.contains("Missing.PcbDoc"),
        "placeholder label must include the unresolved DOCUMENTPATH; got:\n{svg}"
    );
    assert!(svg.contains("<rect"), "placeholder draws a rectangle");
}

#[test]
fn embedded_board_renders_subdoc_through_loader() {
    let sub = build_marker_subdoc();
    let sub_bytes = sub.to_bytes().expect("write sub");

    let mut loader = InMemoryLoader::new();
    loader.insert("Sub.PcbDoc", sub_bytes);

    let mut parent = pcb::Document::default();
    let mut board = EmbeddedBoard::default();
    board.document_path = Some("Sub.PcbDoc".into());
    board.layer = 1;
    board.x1_location = Coord::from_mils(0.0);
    board.y1_location = Coord::from_mils(0.0);
    board.x2_location = Coord::from_mils(100.0);
    board.y2_location = Coord::from_mils(100.0);
    board.col_count = 1;
    board.row_count = 1;
    parent.embedded_boards.push(board);

    let opts = RenderOptions {
        width: 600,
        height: 600,
        ..RenderOptions::default()
    };
    let svg = parent.render_svg_with_loader(opts, &loader);

    // The sub-document had a circle (Arc with full sweep). The renderer emits
    // <circle .../> for a full-sweep arc. So we should see one circle in the
    // output, and no placeholder label.
    assert!(
        svg.contains("<circle"),
        "sub-document arc should render as a circle; got:\n{svg}"
    );
    assert!(
        !svg.contains("Sub.PcbDoc"),
        "no placeholder label when sub-doc resolves; got:\n{svg}"
    );
    assert!(loader.calls.get() >= 1, "loader was consulted");
}

#[test]
fn embedded_board_array_replicates_subdoc() {
    // 2×3 array of a single-arc sub-document. We expect 6 circle elements in
    // the SVG (one per array instance).
    let sub = build_marker_subdoc();
    let sub_bytes = sub.to_bytes().expect("write sub");

    let mut loader = InMemoryLoader::new();
    loader.insert("Sub.PcbDoc", sub_bytes);

    let mut parent = pcb::Document::default();
    let mut board = EmbeddedBoard::default();
    board.document_path = Some("Sub.PcbDoc".into());
    board.layer = 1;
    board.x1_location = Coord::from_mils(0.0);
    board.y1_location = Coord::from_mils(0.0);
    board.x2_location = Coord::from_mils(50.0);
    board.y2_location = Coord::from_mils(50.0);
    board.col_count = 2;
    board.row_count = 3;
    board.col_spacing = Coord::from_mils(80.0);
    board.row_spacing = Coord::from_mils(80.0);
    parent.embedded_boards.push(board);

    let opts = RenderOptions {
        width: 800,
        height: 800,
        ..RenderOptions::default()
    };
    let svg = parent.render_svg_with_loader(opts, &loader);

    let circle_count = svg.matches("<circle").count();
    assert_eq!(
        circle_count, 6,
        "expected 6 sub-doc copies for 2×3 array, got {circle_count}"
    );
}

#[test]
fn embedded_board_falls_back_when_loader_returns_none() {
    let loader = InMemoryLoader::new(); // empty — loads return None

    let mut parent = pcb::Document::default();
    let mut board = EmbeddedBoard::default();
    board.document_path = Some("Ghost.PcbDoc".into());
    board.layer = 1;
    board.x1_location = Coord::from_mils(0.0);
    board.y1_location = Coord::from_mils(0.0);
    board.x2_location = Coord::from_mils(100.0);
    board.y2_location = Coord::from_mils(100.0);
    board.col_count = 1;
    board.row_count = 1;
    parent.embedded_boards.push(board);

    let svg = parent.render_svg_with_loader(RenderOptions::default(), &loader);

    // Failure is rendered as a placeholder with the path label rather than
    // silently dropping the reference.
    assert!(svg.contains("Ghost.PcbDoc"), "missing-board label visible");
}

#[test]
fn embedded_board_recursion_depth_caps_out() {
    // A self-referencing sub-document. The recursion guard should stop after
    // MAX_EMBEDDED_DEPTH levels; the test just checks rendering terminates and
    // produces some output.
    let mut sub = pcb::Document::default();
    let mut self_ref = EmbeddedBoard::default();
    self_ref.document_path = Some("Self.PcbDoc".into());
    self_ref.layer = 1;
    self_ref.x1_location = Coord::from_mils(0.0);
    self_ref.y1_location = Coord::from_mils(0.0);
    self_ref.x2_location = Coord::from_mils(50.0);
    self_ref.y2_location = Coord::from_mils(50.0);
    self_ref.col_count = 1;
    self_ref.row_count = 1;
    sub.embedded_boards.push(self_ref);
    let sub_bytes = sub.to_bytes().expect("write sub");

    let mut loader = InMemoryLoader::new();
    loader.insert("Self.PcbDoc", sub_bytes);

    let mut parent = pcb::Document::default();
    let mut top = EmbeddedBoard::default();
    top.document_path = Some("Self.PcbDoc".into());
    top.layer = 1;
    top.x1_location = Coord::from_mils(0.0);
    top.y1_location = Coord::from_mils(0.0);
    top.x2_location = Coord::from_mils(200.0);
    top.y2_location = Coord::from_mils(200.0);
    top.col_count = 1;
    top.row_count = 1;
    parent.embedded_boards.push(top);

    // No panic, no infinite loop, output non-empty.
    let svg = parent.render_svg_with_loader(RenderOptions::default(), &loader);
    assert!(!svg.is_empty());
}

#[test]
fn embedded_board_resolves_file_loader_against_testdata_pair() {
    let testdata = std::path::PathBuf::from("../../testdata");
    if !testdata.exists() {
        eprintln!("skipping: testdata not present");
        return;
    }
    let parent_path = testdata.join("Power Adapter Panel.PcbDoc");
    let sibling = testdata.join("USB Power Adapter.PcbDoc");
    if !parent_path.exists() || !sibling.exists() {
        eprintln!("skipping: testdata pair not present");
        return;
    }

    let parent_bytes = std::fs::read(&parent_path).unwrap();
    let parent = pcb::Document::from_bytes(parent_bytes).expect("parse parent");
    let loader = FileBoardLoader::new(testdata);

    let svg = parent.render_svg_with_loader(RenderOptions::default(), &loader);
    assert!(svg.starts_with("<?xml") || svg.starts_with("<svg"));
    // 12 instances (3×4 array) × at least one drawable primitive per instance.
    // A SVG with no embedded sub-render would have far fewer paths than one
    // with the sub-board replicated 12×; sanity-check on element count.
    let primitive_count = svg.matches("<rect").count()
        + svg.matches("<line").count()
        + svg.matches("<path").count()
        + svg.matches("<circle").count()
        + svg.matches("<polyline").count()
        + svg.matches("<polygon").count();
    assert!(
        primitive_count > 50,
        "expected many primitives from 3×4 sub-board replication, got {primitive_count}"
    );
}
