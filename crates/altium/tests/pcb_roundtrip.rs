//! End-to-end round-trip tests for `.PcbLib` and `.PcbDoc`.

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
    for path in files {
        let bytes = fs::read(&path).unwrap();
        let doc = pcb::Document::from_bytes(bytes).unwrap();
        total_components += doc.components.len();
        total_polygons += doc.polygons.len();
        total_rules += doc.rules.len();
        total_classes += doc.classes.len();
        total_nets += doc.nets.len();
        total_assigned_pads += doc.components.iter().map(|c| c.pads.len()).sum::<usize>();
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
}
