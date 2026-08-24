//! End-to-end round-trip tests for `.PcbLib` and `.PcbDoc`.

#![allow(clippy::field_reassign_with_default)]

use std::fs;
use std::path::{Path, PathBuf};

use altium::pcb;

fn testdata_dir() -> Option<PathBuf> {
    let candidates = [PathBuf::from("../../testdata"), PathBuf::from("testdata")];
    candidates.into_iter().find(|p| p.exists())
}

fn collect_with_extension(dir: &Path, ext_lower: &str) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    read.flatten()
        .filter_map(|e| {
            let p = e.path();
            let ext = p.extension()?.to_string_lossy().to_lowercase();
            (ext == ext_lower).then_some(p)
        })
        .collect()
}

#[test]
fn read_every_pcblib_in_testdata() {
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    let files = collect_with_extension(&dir, "pcblib");
    assert!(!files.is_empty(), "no .PcbLib files in {dir:?}");
    for path in files {
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let _library = pcb::Library::from_bytes(bytes)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    }
}

#[test]
fn read_every_pcbdoc_in_testdata() {
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    let files = collect_with_extension(&dir, "pcbdoc");
    if files.is_empty() {
        eprintln!("skipping: no .PcbDoc files in {dir:?}");
        return;
    }
    for path in files {
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let _document = pcb::Document::from_bytes(bytes)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    }
}

#[test]
fn pcblib_round_trips_through_writer() {
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    let files = collect_with_extension(&dir, "pcblib");
    assert!(!files.is_empty(), "no .PcbLib files in {dir:?}");
    for path in files {
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let original = pcb::Library::from_bytes(bytes)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let written = original
            .to_bytes()
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        let reparsed = pcb::Library::from_bytes(written)
            .unwrap_or_else(|e| panic!("re-parse {}: {e}", path.display()));

        assert_eq!(
            original.components.len(),
            reparsed.components.len(),
            "component count drift in {}",
            path.display()
        );
        for (a, b) in original.components.iter().zip(reparsed.components.iter()) {
            assert_eq!(a.name, b.name, "name drift in {}", path.display());
            assert_eq!(
                a.pads.len(),
                b.pads.len(),
                "pads drift in {}",
                path.display()
            );
            assert_eq!(
                a.tracks.len(),
                b.tracks.len(),
                "tracks drift in {}",
                path.display()
            );
            assert_eq!(
                a.arcs.len(),
                b.arcs.len(),
                "arcs drift in {}",
                path.display()
            );
            assert_eq!(
                a.texts.len(),
                b.texts.len(),
                "texts drift in {}",
                path.display()
            );
            assert_eq!(
                a.fills.len(),
                b.fills.len(),
                "fills drift in {}",
                path.display()
            );
            assert_eq!(
                a.regions.len(),
                b.regions.len(),
                "regions drift in {}",
                path.display()
            );
            assert_eq!(
                a.component_bodies.len(),
                b.component_bodies.len(),
                "bodies drift in {}",
                path.display()
            );
        }
    }
}

#[test]
fn pcbdoc_round_trips_through_writer() {
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    let files = collect_with_extension(&dir, "pcbdoc");
    if files.is_empty() {
        eprintln!("skipping: no .PcbDoc files in {dir:?}");
        return;
    }
    for path in files {
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let original = pcb::Document::from_bytes(bytes)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let written = original
            .to_bytes()
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        let reparsed = pcb::Document::from_bytes(written)
            .unwrap_or_else(|e| panic!("re-parse {}: {e}", path.display()));

        assert_eq!(
            original.arcs.len(),
            reparsed.arcs.len(),
            "arcs drift in {}",
            path.display()
        );
        assert_eq!(
            original.pads.len(),
            reparsed.pads.len(),
            "pads drift in {}",
            path.display()
        );
        assert_eq!(
            original.vias.len(),
            reparsed.vias.len(),
            "vias drift in {}",
            path.display()
        );
        assert_eq!(
            original.tracks.len(),
            reparsed.tracks.len(),
            "tracks drift in {}",
            path.display()
        );
        assert_eq!(
            original.texts.len(),
            reparsed.texts.len(),
            "texts drift in {}",
            path.display()
        );
        assert_eq!(
            original.fills.len(),
            reparsed.fills.len(),
            "fills drift in {}",
            path.display()
        );
        assert_eq!(
            original.regions.len(),
            reparsed.regions.len(),
            "regions drift in {}",
            path.display()
        );
        assert_eq!(
            original.component_bodies.len(),
            reparsed.component_bodies.len(),
            "bodies drift in {}",
            path.display()
        );
        assert_eq!(
            original.components.len(),
            reparsed.components.len(),
            "components drift in {}",
            path.display()
        );
        assert_eq!(
            original.polygons.len(),
            reparsed.polygons.len(),
            "polygons drift in {}",
            path.display()
        );
        assert_eq!(
            original.rules.len(),
            reparsed.rules.len(),
            "rules drift in {}",
            path.display()
        );
        assert_eq!(
            original.classes.len(),
            reparsed.classes.len(),
            "classes drift in {}",
            path.display()
        );
        assert_eq!(
            original.differential_pairs.len(),
            reparsed.differential_pairs.len(),
            "differential_pairs drift in {}",
            path.display()
        );
        assert_eq!(
            original.rooms.len(),
            reparsed.rooms.len(),
            "rooms drift in {}",
            path.display()
        );
        assert_eq!(
            original.embedded_boards.len(),
            reparsed.embedded_boards.len(),
            "embedded_boards drift in {}",
            path.display()
        );
        assert_eq!(
            original.nets.len(),
            reparsed.nets.len(),
            "nets drift in {}",
            path.display()
        );

        // net_index / component_index on Via / Text / Region / ComponentBody
        // come from the binary common-prefix bytes (`0xFFFF` sentinel for
        // "absent"). They were silently zero'd by an older writer; assert the
        // current writer round-trips them.
        for (a, b) in original.vias.iter().zip(reparsed.vias.iter()) {
            assert_eq!(
                (a.net_index, a.component_index),
                (b.net_index, b.component_index),
                "via net/component drift in {}",
                path.display()
            );
        }
        for (a, b) in original.texts.iter().zip(reparsed.texts.iter()) {
            assert_eq!(
                (a.net_index, a.component_index),
                (b.net_index, b.component_index),
                "text net/component drift in {}",
                path.display()
            );
        }
        for (a, b) in original.regions.iter().zip(reparsed.regions.iter()) {
            assert_eq!(
                (a.net_index, a.component_index),
                (b.net_index, b.component_index),
                "region net/component drift in {}",
                path.display()
            );
        }
        for (a, b) in original
            .component_bodies
            .iter()
            .zip(reparsed.component_bodies.iter())
        {
            assert_eq!(
                (a.net_index, a.component_index),
                (b.net_index, b.component_index),
                "body net/component drift in {}",
                path.display()
            );
        }
    }
}

#[test]
fn pcbdoc_typed_storages_populate() {
    // Sanity: at least one PcbDoc in testdata should populate Components6,
    // Polygons6, Rules6, Classes6, Nets6.
    let Some(dir) = testdata_dir() else {
        return;
    };
    let files = collect_with_extension(&dir, "pcbdoc");
    if files.is_empty() {
        return;
    }
    let mut total_components = 0;
    let mut total_polygons = 0;
    let mut total_rules = 0;
    let mut total_classes = 0;
    let mut total_nets = 0;
    let mut total_assigned_pads = 0;
    let mut total_regions = 0;
    let mut regions_with_subpoly_set = 0;
    let mut regions_with_shape_based = 0;
    let mut regions_with_arc_resolution = 0;
    let mut total_pads = 0;
    let mut smt_pads = 0;
    let mut pads_with_top_paste = 0;
    let mut pads_with_bottom_paste = 0;
    let mut pads_with_nonzero_corner_radius = 0;
    let mut pads_with_custom_shapes = 0;
    let mut pads_with_per_layer_rect = 0;
    for path in files {
        let bytes = fs::read(&path).unwrap();
        let doc = pcb::Document::from_bytes(bytes).unwrap();
        total_components += doc.components.len();
        total_polygons += doc.polygons.len();
        total_rules += doc.rules.len();
        total_classes += doc.classes.len();
        total_nets += doc.nets.len();
        total_assigned_pads += doc.components.iter().map(|c| c.pads.len()).sum::<usize>();
        for r in doc.regions.iter().chain(doc.components.iter().flat_map(|c| c.regions.iter())) {
            total_regions += 1;
            // SUBPOLYINDEX = -1 is the standalone-region default; any other
            // value means we actually parsed it off disk.
            if r.sub_poly_index != -1 {
                regions_with_subpoly_set += 1;
            }
            if r.is_shape_based {
                regions_with_shape_based += 1;
            }
            if r.arc_resolution != 0.5 && r.arc_resolution != 0.0 {
                regions_with_arc_resolution += 1;
            }
        }
        for p in doc.pads.iter().chain(doc.components.iter().flat_map(|c| c.pads.iter())) {
            total_pads += 1;
            if p.is_surface_mount {
                smt_pads += 1;
            }
            if p.is_top_paste_enabled {
                pads_with_top_paste += 1;
            }
            if p.is_bottom_paste_enabled {
                pads_with_bottom_paste += 1;
            }
            if p.corner_radius_percentage != 0 && p.corner_radius_percentage != 50 {
                pads_with_nonzero_corner_radius += 1;
            }
            if p.has_custom_shapes {
                pads_with_custom_shapes += 1;
            }
            if p.has_rounded_rectangular_shapes {
                pads_with_per_layer_rect += 1;
            }
        }
    }
    assert!(total_components > 0, "no Components6 records parsed");
    assert!(total_polygons > 0, "no Polygons6 records parsed");
    assert!(total_rules > 0, "no Rules6 records parsed");
    assert!(total_classes > 0, "no Classes6 records parsed");
    assert!(total_nets > 0, "no Nets6 records parsed");
    assert!(
        total_assigned_pads > 0,
        "no pads were assigned to components by component_index"
    );
    // Stat-only — useful as a regression signal but not a hard
    // requirement (testdata mix changes over time).
    eprintln!(
        "regions: {} total, {} with subpoly!=-1, {} shape-based, {} non-default arc_resolution",
        total_regions,
        regions_with_subpoly_set,
        regions_with_shape_based,
        regions_with_arc_resolution,
    );
    eprintln!(
        "pads: {} total, {} SMT, {} top-paste, {} bottom-paste, \
         {} non-default corner radius, {} custom shape, {} per-layer rectangle",
        total_pads,
        smt_pads,
        pads_with_top_paste,
        pads_with_bottom_paste,
        pads_with_nonzero_corner_radius,
        pads_with_custom_shapes,
        pads_with_per_layer_rect,
    );
    // Hard floor: testdata contains SMT pads and pads on the top and
    // bottom signal layers (or through-hole pads that span both). If
    // any of these counters go to zero, the reader regressed on flag
    // derivation.
    assert!(smt_pads > 0, "no SMT pads detected — flag derivation regression");
    assert!(
        pads_with_top_paste > 0,
        "no top-paste pads detected — flag derivation regression"
    );
    assert!(
        pads_with_bottom_paste > 0,
        "no bottom-paste pads detected — flag derivation regression"
    );
}

// ─── Embedded sub-board dereferencing ──────────────────────────────────────

#[tokio::test]
async fn embedded_board_resolves_against_testdata_pair() {
    // `Power Adapter Panel.PcbDoc` references `USB Power Adapter.PcbDoc` as a
    // sibling via DOCUMENTPATH. Resolving the embedded board against the
    // testdata directory should produce a fully-parsed sub-document.
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    let parent = dir.join("Power Adapter Panel.PcbDoc");
    if !parent.exists() {
        eprintln!("skipping: {parent:?} not present");
        return;
    }

    let doc = pcb::Document::read(&parent).await.expect("read parent");
    assert_eq!(
        doc.embedded_boards.len(),
        1,
        "fixture should carry exactly one embedded board"
    );
    assert_eq!(
        doc.embedded_boards[0].document_path.as_deref(),
        Some("USB Power Adapter.PcbDoc")
    );

    // resolved_path joins relative to the parent directory.
    let resolved = doc.embedded_boards[0]
        .resolved_path(&dir)
        .expect("path resolves");
    assert_eq!(resolved.file_name().unwrap(), "USB Power Adapter.PcbDoc");

    // Single-board resolver returns the sub-document.
    let sub = doc.embedded_boards[0]
        .resolve_at(&dir)
        .await
        .expect("sub-board parses");
    let pad_count = sub.pads.len();
    let component_count = sub.components.len();
    assert!(
        pad_count + component_count + sub.tracks.len() > 0,
        "sub-board parsed empty"
    );

    // Document-level batch resolver returns one entry parallel to embedded_boards.
    let resolved_all = doc.resolve_embedded_boards_at(&dir).await;
    assert_eq!(resolved_all.len(), 1);
    let sub_again = resolved_all
        .into_iter()
        .next()
        .unwrap()
        .expect("batch resolution succeeds");
    assert_eq!(sub_again.pads.len(), pad_count);
    assert_eq!(sub_again.components.len(), component_count);
}

#[tokio::test]
async fn embedded_board_custom_loader_can_intercept() {
    use altium::pcb::{BoardLoader, FileBoardLoader};

    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    let parent = dir.join("Power Adapter Panel.PcbDoc");
    if !parent.exists() {
        eprintln!("skipping: {parent:?} not present");
        return;
    }
    let doc = pcb::Document::read(&parent).await.expect("read parent");

    // A counting loader that delegates to FileBoardLoader. Demonstrates the
    // trait wiring works for caching / observability use cases.
    struct Counting<'a> {
        inner: &'a FileBoardLoader,
        count: std::cell::Cell<usize>,
    }
    impl BoardLoader for Counting<'_> {
        fn load(&self, document_path: &str) -> altium::Result<Option<Vec<u8>>> {
            self.count.set(self.count.get() + 1);
            self.inner.load(document_path)
        }
    }

    let inner = FileBoardLoader::new(&dir);
    let counting = Counting {
        inner: &inner,
        count: std::cell::Cell::new(0),
    };
    let results = doc.resolve_embedded_boards_with(&counting);
    assert_eq!(results.len(), 1);
    assert!(results.into_iter().next().unwrap().is_ok());
    assert_eq!(counting.count.get(), 1, "loader was consulted exactly once");
}

// ─── Embedded sub-board flattening ────────────────────────────────────────

#[tokio::test]
async fn flatten_embedded_boards_inlines_subdoc_primitives() {
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    let parent_path = dir.join("Power Adapter Panel.PcbDoc");
    let sibling = dir.join("USB Power Adapter.PcbDoc");
    if !parent_path.exists() || !sibling.exists() {
        eprintln!("skipping: testdata pair not present");
        return;
    }

    // Read both docs separately so we have ground-truth counts.
    let parent = pcb::Document::read(&parent_path).await.expect("parent");
    let sub = pcb::Document::read(&sibling).await.expect("sibling");
    let flat = parent.flatten_embedded_boards_at(&dir).await;

    // After flattening, no embedded references should remain.
    assert!(
        flat.embedded_boards.is_empty(),
        "all embedded references should resolve in the testdata pair"
    );

    // Power Adapter Panel.PcbDoc has a 3×4 array of USB Power Adapter.PcbDoc.
    let board = &parent.embedded_boards[0];
    let instances =
        (board.col_count.max(1) as usize) * (board.row_count.max(1) as usize);
    assert_eq!(instances, 12, "fixture is 3×4");

    // Each sub-doc primitive should appear `instances` times in the flat
    // result, on top of the parent's own primitives.
    assert_eq!(
        flat.tracks.len(),
        parent.tracks.len() + sub.tracks.len() * instances,
        "track count = parent + instances × sub.tracks"
    );
    assert_eq!(
        flat.pads.len(),
        parent.pads.len() + sub.pads.len() * instances,
        "pad count = parent + instances × sub.pads"
    );
    assert_eq!(
        flat.components.len(),
        parent.components.len() + sub.components.len() * instances,
        "component count parallels"
    );

    // Diagnostics should be empty when everything resolved.
    assert!(
        flat.diagnostics.is_empty(),
        "diagnostics should be empty on full resolution; got {:?}",
        flat.diagnostics
    );

    // Bounds expand to include the array spread (sanity: flat bounds ⊇ parent
    // bounds).
    let parent_bb = parent.bounds();
    let flat_bb = flat.bounds();
    if !parent_bb.is_empty() {
        assert!(
            flat_bb.min.x <= parent_bb.min.x && flat_bb.min.y <= parent_bb.min.y,
            "flat bounds {flat_bb:?} should contain parent bounds {parent_bb:?}"
        );
    }
}

#[tokio::test]
async fn flatten_array_replicates_subdoc_pads_at_correct_offsets() {
    use altium::pcb::EmbeddedBoard;
    use altium::{Coord, CoordPoint};

    // Build a synthetic sub-doc with a single pad at the origin, then a
    // parent with a 2×3 array of it, and verify the flattened pads sit at
    // the expected world offsets.
    let mut sub = pcb::Document::default();
    let mut pad = pcb::Pad::default();
    pad.location = CoordPoint::new(Coord::ZERO, Coord::ZERO);
    pad.size_top = CoordPoint::new(Coord::from_mils(40.0), Coord::from_mils(40.0));
    pad.size_middle = pad.size_top;
    pad.size_bottom = pad.size_top;
    pad.layer = 1;
    sub.pads.push(pad);
    let sub_bytes = sub.to_bytes().expect("write sub");

    // In-memory loader for the synthetic sub-doc.
    struct Mem(Vec<u8>);
    impl pcb::BoardLoader for Mem {
        fn load(&self, _: &str) -> altium::Result<Option<Vec<u8>>> {
            Ok(Some(self.0.clone()))
        }
    }
    let loader = Mem(sub_bytes);

    let mut parent = pcb::Document::default();
    let mut board = EmbeddedBoard::default();
    board.document_path = Some("Sub.PcbDoc".into());
    board.layer = 1;
    // Placement translation: where the sub-doc's board origin (0,0 here)
    // lands. X1..Y2 is only the cached bounding box and must not shift
    // the placement.
    board.x_location = Coord::from_mils(100.0);
    board.y_location = Coord::from_mils(50.0);
    board.x1_location = Coord::from_mils(80.0);
    board.y1_location = Coord::from_mils(30.0);
    board.x2_location = Coord::from_mils(120.0);
    board.y2_location = Coord::from_mils(70.0);
    board.col_count = 2;
    board.row_count = 3;
    board.col_spacing = Coord::from_mils(200.0);
    board.row_spacing = Coord::from_mils(150.0);
    parent.embedded_boards.push(board);

    let flat = parent.flatten_embedded_boards_with(&loader);
    assert_eq!(flat.pads.len(), 6, "2×3 array of one-pad sub-doc");

    // Each pad should be at (X + col*col_spacing, Y + row*row_spacing).
    let mut got: Vec<(i32, i32)> = flat
        .pads
        .iter()
        .map(|p| (p.location.x.to_raw(), p.location.y.to_raw()))
        .collect();
    got.sort();
    let mut expected: Vec<(i32, i32)> = Vec::new();
    for col in 0..2 {
        for row in 0..3 {
            expected.push((
                Coord::from_mils(100.0 + 200.0 * col as f64).to_raw(),
                Coord::from_mils(50.0 + 150.0 * row as f64).to_raw(),
            ));
        }
    }
    expected.sort();
    assert_eq!(got, expected);
}

#[tokio::test]
async fn flatten_subtracts_child_origin_and_remaps_indices() {
    use altium::pcb::EmbeddedBoard;
    use altium::{Coord, CoordPoint};

    // Sub-doc: board origin at (100, 200) mil, one component whose pad is
    // linked to it (component_index 0) and netted to "GND" (index 2 of
    // [VCC, GND], 1-based).
    let mut sub = pcb::Document::default();
    sub.board_parameters = Some(vec![
        ("ORIGINX".to_string(), "100mil".to_string()),
        ("ORIGINY".to_string(), "200mil".to_string()),
    ]);
    sub.nets.push(pcb::Net { name: "VCC".into(), ..Default::default() });
    sub.nets.push(pcb::Net { name: "GND".into(), ..Default::default() });
    let mut sub_comp = pcb::Component::new("SUB-COMP");
    sub_comp.x = Coord::from_mils(150.0);
    sub_comp.y = Coord::from_mils(260.0);
    sub.components.push(sub_comp);
    let mut pad = pcb::Pad::default();
    pad.location = CoordPoint::new(Coord::from_mils(150.0), Coord::from_mils(260.0));
    pad.size_top = CoordPoint::new(Coord::from_mils(40.0), Coord::from_mils(40.0));
    pad.size_middle = pad.size_top;
    pad.size_bottom = pad.size_top;
    pad.layer = 1;
    pad.component_index = 0;
    // The writer resolves net linkage from the name; the reader hands back
    // the numeric index (2 = 1-based position of "GND" in [VCC, GND]).
    pad.net = Some("GND".into());
    pad.net_index = 2;
    sub.pads.push(pad);
    let sub_bytes = sub.to_bytes().expect("write sub");

    struct Mem(Vec<u8>);
    impl pcb::BoardLoader for Mem {
        fn load(&self, _: &str) -> altium::Result<Option<Vec<u8>>> {
            Ok(Some(self.0.clone()))
        }
    }
    let loader = Mem(sub_bytes);

    // Parent: its own component + pad on "GND", and the sub-board placed
    // so its board origin lands at (1000, 2000) mil.
    let mut parent = pcb::Document::default();
    parent.nets.push(pcb::Net { name: "GND".into(), ..Default::default() });
    parent.components.push(pcb::Component::new("PARENT-COMP"));
    let mut ppad = pcb::Pad::default();
    ppad.size_top = CoordPoint::new(Coord::from_mils(40.0), Coord::from_mils(40.0));
    ppad.layer = 1;
    ppad.component_index = 0;
    ppad.net = Some("GND".into());
    ppad.net_index = 1;
    parent.pads.push(ppad);
    let mut board = EmbeddedBoard::default();
    board.document_path = Some("Sub.PcbDoc".into());
    board.layer = 1;
    board.x_location = Coord::from_mils(1000.0);
    board.y_location = Coord::from_mils(2000.0);
    parent.embedded_boards.push(board);

    let flat = parent.flatten_embedded_boards_with(&loader);
    assert_eq!(flat.components.len(), 2);
    assert_eq!(flat.pads.len(), 2);

    // Sub pad: (150,260) − child origin (100,200) + placement (1000,2000).
    let sub_pad = &flat.pads[1];
    assert_eq!(
        sub_pad.location,
        CoordPoint::new(Coord::from_mils(1050.0), Coord::from_mils(2060.0)),
        "child board origin must be subtracted before the placement offset"
    );
    // Its owner is the *second* merged component, not the parent's.
    assert_eq!(sub_pad.component_index, 1);
    // Nets merged by name: [GND, VCC]; the sub's GND (was 2) is now 1.
    assert_eq!(sub_pad.net_index, 1);
    assert_eq!(flat.nets.len(), 2);
    // Parent's pad is untouched.
    assert_eq!(flat.pads[0].component_index, 0);
    assert_eq!(flat.pads[0].net_index, 1);
    // The sub component's anchor moved with the same transform.
    assert_eq!(flat.components[1].x, Coord::from_mils(1050.0));
    assert_eq!(flat.components[1].y, Coord::from_mils(2060.0));
    // Its owned pad clone stayed in step with the document-level pad.
    assert_eq!(flat.components[1].pads.len(), 1);
    assert_eq!(flat.components[1].pads[0].location, sub_pad.location);
    assert_eq!(flat.components[1].pads[0].component_index, 1);
}

#[tokio::test]
async fn flatten_unresolved_board_is_preserved_with_diagnostic() {
    use altium::pcb::EmbeddedBoard;
    use altium::{Coord, CoordPoint};

    // Loader that knows about no boards.
    struct Empty;
    impl pcb::BoardLoader for Empty {
        fn load(&self, _: &str) -> altium::Result<Option<Vec<u8>>> {
            Ok(None)
        }
    }

    let mut parent = pcb::Document::default();
    let mut board = EmbeddedBoard::default();
    board.document_path = Some("Missing.PcbDoc".into());
    board.layer = 1;
    board.x1_location = Coord::from_mils(0.0);
    board.y1_location = Coord::from_mils(0.0);
    board.x2_location = Coord::from_mils(100.0);
    board.y2_location = Coord::from_mils(100.0);
    board.col_count = 1;
    board.row_count = 1;
    parent.embedded_boards.push(board);

    let flat = parent.flatten_embedded_boards_with(&Empty);

    // Reference is preserved (so a renderer can still placeholder it) and a
    // diagnostic explains the failure.
    assert_eq!(flat.embedded_boards.len(), 1);
    assert_eq!(
        flat.embedded_boards[0].document_path.as_deref(),
        Some("Missing.PcbDoc")
    );
    assert!(
        !flat.diagnostics.is_empty(),
        "missing sub-board should emit a diagnostic"
    );
    let msg = flat.diagnostics[0].message.clone();
    assert!(
        msg.contains("Missing.PcbDoc"),
        "diagnostic should name the missing path; got: {msg}"
    );
    let _ = CoordPoint::default(); // suppress unused-import warning if test path skips
}

#[tokio::test]
async fn flatten_self_referencing_board_caps_recursion() {
    use altium::pcb::EmbeddedBoard;
    use altium::{Coord, CoordPoint};

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

    struct Mem(Vec<u8>);
    impl pcb::BoardLoader for Mem {
        fn load(&self, _: &str) -> altium::Result<Option<Vec<u8>>> {
            Ok(Some(self.0.clone()))
        }
    }
    let loader = Mem(sub_bytes);

    let mut parent = pcb::Document::default();
    let mut top = EmbeddedBoard::default();
    top.document_path = Some("Self.PcbDoc".into());
    top.layer = 1;
    top.x1_location = Coord::from_mils(0.0);
    top.y1_location = Coord::from_mils(0.0);
    top.x2_location = Coord::from_mils(100.0);
    top.y2_location = Coord::from_mils(100.0);
    top.col_count = 1;
    top.row_count = 1;
    parent.embedded_boards.push(top);

    // Should terminate; the deepest sub-board reference is preserved at the
    // ceiling and surfaces a "max recursion depth" diagnostic.
    let flat = parent.flatten_embedded_boards_with(&loader);
    assert_eq!(
        flat.embedded_boards.len(),
        1,
        "exactly one preserved reference at the depth cap"
    );
    let msg = flat.diagnostics.last().unwrap().message.clone();
    assert!(
        msg.contains("max recursion depth") || msg.contains("recursion"),
        "expected depth-limit diagnostic; got: {msg}"
    );
    let _ = CoordPoint::default();
}

#[tokio::test]
async fn embedded_board_resolution_fails_cleanly_when_sibling_missing() {
    // Resolving against an empty directory should produce a structured error
    // rather than panic.
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    let parent = dir.join("Power Adapter Panel.PcbDoc");
    if !parent.exists() {
        eprintln!("skipping: {parent:?} not present");
        return;
    }
    let doc = pcb::Document::read(&parent).await.expect("read parent");

    let empty = std::env::temp_dir().join(format!(
        "altium-rs-empty-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&empty).unwrap();
    let results = doc.resolve_embedded_boards_at(&empty).await;
    assert_eq!(results.len(), 1);
    let err = results.into_iter().next().unwrap().unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("USB Power Adapter") || msg.contains("not found"),
        "expected a 'not found' error mentioning the missing sibling, got: {msg}"
    );
    let _ = std::fs::remove_dir(&empty);
}

#[test]
fn custom_shape_pads_pair_against_real_files() {
    // At least one .PcbDoc in the testdata corpus carries shape-based
    // regions paired with placeholder pads (these are custom-shape EPs
    // — e.g. the thermal pad on a QFN). The pairing API has to return
    // at least one pair, and every pair has to be self-consistent:
    // same layer, same component_index, pad location inside region bbox.
    let Some(dir) = testdata_dir() else {
        return;
    };
    let files = collect_with_extension(&dir, "pcbdoc");
    if files.is_empty() {
        return;
    }
    let mut total_pairs = 0;
    for path in files {
        let bytes = fs::read(&path).unwrap();
        let doc = pcb::Document::from_bytes(bytes).unwrap();
        let pairs = doc.custom_shape_pads();
        for csp in &pairs {
            assert_eq!(
                csp.pad.layer, csp.region.layer,
                "pad/region layer mismatch in {}",
                path.display()
            );
            assert_eq!(
                csp.pad.component_index, csp.region.component_index,
                "pad/region component mismatch in {}",
                path.display()
            );
            assert!(
                csp.region.bounds().contains(csp.pad.location),
                "pad centroid outside region bbox in {}",
                path.display()
            );
            assert!(
                csp.region.is_shape_based,
                "non-shape-based region returned in {}",
                path.display()
            );
        }
        total_pairs += pairs.len();
    }
    eprintln!("custom-shape pad/region pairs across testdata: {total_pairs}");
    assert!(
        total_pairs > 0,
        "no custom-shape pad/region pairs found across testdata — \
         either the matcher regressed or the testdata changed"
    );
}
