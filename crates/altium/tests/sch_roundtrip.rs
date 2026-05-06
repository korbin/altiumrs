//! End-to-end round-trip tests for `.SchLib` and `.SchDoc`.

use std::fs;
use std::path::{Path, PathBuf};

use altium::sch;

fn testdata_dir() -> Option<PathBuf> {
    [PathBuf::from("../../testdata"), PathBuf::from("testdata")]
        .into_iter()
        .find(|p| p.exists())
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

fn component_counts(c: &sch::Component) -> [usize; 19] {
    [
        c.pins.len(),
        c.lines.len(),
        c.rectangles.len(),
        c.rounded_rectangles.len(),
        c.polygons.len(),
        c.polylines.len(),
        c.ellipses.len(),
        c.elliptical_arcs.len(),
        c.pies.len(),
        c.arcs.len(),
        c.beziers.len(),
        c.labels.len(),
        c.net_labels.len(),
        c.junctions.len(),
        c.parameters.len(),
        c.text_frames.len(),
        c.images.len(),
        c.symbols.len(),
        c.implementations.len(),
    ]
}

#[test]
fn read_every_schlib_in_testdata() {
    let Some(dir) = testdata_dir() else {
        return;
    };
    let files = collect_with_extension(&dir, "schlib");
    if files.is_empty() {
        return;
    }
    for path in files {
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let _library = sch::Library::from_bytes(bytes)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    }
}

#[test]
fn read_every_schdoc_in_testdata() {
    let Some(dir) = testdata_dir() else {
        return;
    };
    let files = collect_with_extension(&dir, "schdoc");
    if files.is_empty() {
        return;
    }
    for path in files {
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let _document = sch::Document::from_bytes(bytes)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    }
}

#[test]
fn schlib_zero_raw_residue() {
    // Lock in 100% read fidelity for SchLib: every record in every file should
    // dispatch into a typed primitive vector — never to `raw_records`.
    let Some(dir) = testdata_dir() else {
        return;
    };
    let files = collect_with_extension(&dir, "schlib");
    if files.is_empty() {
        return;
    }
    for path in files {
        let bytes = fs::read(&path).unwrap();
        let library = sch::Library::from_bytes(bytes).unwrap();
        for c in &library.components {
            assert_eq!(
                c.raw_records.len(),
                0,
                "untyped record(s) in {} component {:?}: {:?}",
                path.display(),
                c.name,
                c.raw_records.iter().map(|r| r.flag).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn schdoc_zero_raw_residue() {
    // Lock in 100% read fidelity for SchDoc: sheet header (RECORD=31), file
    // header (no RECORD key), template (RECORD=39), and sheet annotations
    // (RECORD=32/33) are typed; every other record goes to a typed primitive
    // vector. `raw_records` should always be empty after this work.
    let Some(dir) = testdata_dir() else {
        return;
    };
    let files = collect_with_extension(&dir, "schdoc");
    if files.is_empty() {
        return;
    }
    for path in files {
        let bytes = fs::read(&path).unwrap();
        let doc = sch::Document::from_bytes(bytes).unwrap();
        assert_eq!(
            doc.raw_records.len(),
            0,
            "untyped {} record(s) in {}",
            doc.raw_records.len(),
            path.display(),
        );
    }
}

#[test]
fn schlib_typed_primitives_populated() {
    // For at least one library in testdata, expect *some* component to have
    // typed pins / shapes — if zero across the whole library, our reader
    // regressed to raw_records-only.
    let Some(dir) = testdata_dir() else {
        return;
    };
    let files = collect_with_extension(&dir, "schlib");
    let mut total_pins = 0usize;
    let mut total_shapes = 0usize;
    let mut total_components = 0usize;
    for path in &files {
        let bytes = fs::read(path).unwrap();
        let library = sch::Library::from_bytes(bytes).unwrap();
        for c in &library.components {
            total_components += 1;
            total_pins += c.pins.len();
            total_shapes += c.lines.len()
                + c.rectangles.len()
                + c.polygons.len()
                + c.polylines.len()
                + c.ellipses.len()
                + c.arcs.len()
                + c.beziers.len()
                + c.rounded_rectangles.len()
                + c.elliptical_arcs.len()
                + c.pies.len();
        }
    }
    assert!(total_components > 0, "no components parsed");
    assert!(
        total_pins > 0,
        "expected at least one typed Pin across testdata"
    );
    assert!(
        total_shapes > 0,
        "expected at least one typed graphic primitive across testdata"
    );
}

#[test]
fn schlib_round_trips_through_writer() {
    let Some(dir) = testdata_dir() else {
        return;
    };
    let files = collect_with_extension(&dir, "schlib");
    if files.is_empty() {
        return;
    }
    for path in files {
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let original = sch::Library::from_bytes(bytes)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let written = original
            .to_bytes()
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        let reparsed = sch::Library::from_bytes(written)
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
                a.description,
                b.description,
                "description drift in {}",
                path.display()
            );
            assert_eq!(
                component_counts(a),
                component_counts(b),
                "primitive counts drift in {}",
                path.display()
            );
        }
    }
}

#[test]
fn schdoc_round_trips_through_writer() {
    let Some(dir) = testdata_dir() else {
        return;
    };
    let files = collect_with_extension(&dir, "schdoc");
    if files.is_empty() {
        return;
    }
    for path in files {
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let original = sch::Document::from_bytes(bytes)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let written = original
            .to_bytes()
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        let reparsed = sch::Document::from_bytes(written)
            .unwrap_or_else(|e| panic!("re-parse {}: {e}", path.display()));

        assert_eq!(
            original.components.len(),
            reparsed.components.len(),
            "components drift in {}",
            path.display()
        );
        assert_eq!(
            original.wires.len(),
            reparsed.wires.len(),
            "wires drift in {}",
            path.display()
        );
        assert_eq!(
            original.net_labels.len(),
            reparsed.net_labels.len(),
            "net_labels drift in {}",
            path.display()
        );
        assert_eq!(
            original.junctions.len(),
            reparsed.junctions.len(),
            "junctions drift in {}",
            path.display()
        );
        assert_eq!(
            original.power_objects.len(),
            reparsed.power_objects.len(),
            "power_objects drift in {}",
            path.display()
        );
        assert_eq!(
            original.parameters.len(),
            reparsed.parameters.len(),
            "parameters drift in {}",
            path.display()
        );
    }
}
