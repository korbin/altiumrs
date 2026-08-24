//! JSON serialization roundtrip: `serde_json` value → back must reproduce
//! the in-memory document exactly (including raw byte blobs).

#![cfg(feature = "serde")]

use std::fs;
use std::path::{Path, PathBuf};

use altium::{pcb, sch};

fn testdata_dir() -> Option<PathBuf> {
    [PathBuf::from("../../testdata"), PathBuf::from("testdata")]
        .into_iter()
        .find(|p| p.exists())
}

fn files_with_ext(dir: &Path, ext_lower: &str) -> Vec<PathBuf> {
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

fn assert_json_roundtrip<T>(value: &T, label: &str)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(value).unwrap_or_else(|e| panic!("serialize {label}: {e}"));
    let back: T =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("deserialize {label}: {e}"));
    assert!(value == &back, "JSON roundtrip drift in {label}");
}

#[test]
fn pcbdoc_json_round_trips() {
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    for path in files_with_ext(&dir, "pcbdoc") {
        let doc = pcb::Document::from_bytes(fs::read(&path).unwrap()).unwrap();
        assert_json_roundtrip(&doc, &path.display().to_string());
    }
}

#[test]
fn pcblib_json_round_trips() {
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    for path in files_with_ext(&dir, "pcblib") {
        let lib = pcb::Library::from_bytes(fs::read(&path).unwrap()).unwrap();
        assert_json_roundtrip(&lib, &path.display().to_string());
    }
}

#[test]
fn schdoc_json_round_trips() {
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    for path in files_with_ext(&dir, "schdoc") {
        let doc = sch::Document::from_bytes(fs::read(&path).unwrap()).unwrap();
        assert_json_roundtrip(&doc, &path.display().to_string());
    }
}

#[test]
fn schlib_json_round_trips() {
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    for path in files_with_ext(&dir, "schlib") {
        let lib = sch::Library::from_bytes(fs::read(&path).unwrap()).unwrap();
        assert_json_roundtrip(&lib, &path.display().to_string());
    }
}

#[test]
fn bomdoc_json_round_trips() {
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    for path in files_with_ext(&dir, "bomdoc") {
        let doc = altium::BomDocument::from_bytes(fs::read(&path).unwrap()).unwrap();
        assert_json_roundtrip(&doc, &path.display().to_string());
    }
}

#[test]
fn intlib_json_round_trips() {
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    for path in files_with_ext(&dir, "intlib") {
        let lib = altium::IntegratedLibrary::from_bytes(fs::read(&path).unwrap()).unwrap();
        assert_json_roundtrip(&lib, &path.display().to_string());
    }
}
