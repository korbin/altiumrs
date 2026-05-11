//! Byte-perfect round-trip test for `.BomDoc` fixtures.

use std::fs;
use std::path::PathBuf;

use altium::bom::BomDocument;

fn candidate_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("../../testdata"),
        PathBuf::from("testdata"),
        PathBuf::from("../.."),
        PathBuf::from("."),
    ]
}

fn collect_bomdocs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in candidate_dirs() {
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let Some(ext) = path.extension() else {
                continue;
            };
            if ext.to_string_lossy().eq_ignore_ascii_case("bomdoc") {
                out.push(path);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn round_trip_every_bomdoc() {
    let files = collect_bomdocs();
    if files.is_empty() {
        eprintln!("skipping: no .BomDoc fixtures found");
        return;
    }
    for path in files {
        let original =
            fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let doc = BomDocument::from_bytes(original.clone())
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let re = doc
            .to_bytes()
            .unwrap_or_else(|e| panic!("emit {}: {e}", path.display()));
        assert_eq!(
            original,
            re,
            "byte-for-byte round-trip failed for {}",
            path.display()
        );
        assert!(
            doc.header().is_some(),
            "{} parsed without a BOM header record",
            path.display()
        );
    }
}

#[test]
fn header_and_items_visible() {
    let files = collect_bomdocs();
    let Some(path) = files.first() else {
        eprintln!("skipping: no .BomDoc fixtures found");
        return;
    };
    let bytes = fs::read(path).unwrap();
    let doc = BomDocument::from_bytes(bytes).unwrap();
    assert!(doc.header().is_some());
    assert!(!doc.records.is_empty());
    // The fixture is a project LiveBOM and must surface at least one
    // CatalogItem.
    assert!(doc.item_count() > 0);
    for item in doc.items() {
        let _ = item.design_item_id();
        let _ = item.description();
        let _ = item.component_parameters();
    }
}
