//! Structural-summary snapshots. Review with `cargo insta review`.

use std::fs;
use std::path::{Path, PathBuf};

use altium::{pcb, sch};
use serde_json::{Value, json};

fn testdata_dir() -> Option<PathBuf> {
    [PathBuf::from("../../testdata"), PathBuf::from("testdata")]
        .into_iter()
        .find(|p| p.exists())
}

fn locate_first(dir: &Path, ext_lower: &str) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let p = e.path();
        let ext = p.extension()?.to_string_lossy().to_lowercase();
        (ext == ext_lower).then_some(p)
    })
}

fn pcb_library_summary(library: &pcb::Library) -> Value {
    json!({
        "unique_id": library.unique_id,
        "component_count": library.components.len(),
        "model_count": library.models.len(),
        "section_key_count": library.section_keys.len(),
        "components": library
            .components
            .iter()
            .map(|c| json!({
                "name": c.name,
                "description": c.description,
                "pads": c.pads.len(),
                "tracks": c.tracks.len(),
                "vias": c.vias.len(),
                "arcs": c.arcs.len(),
                "texts": c.texts.len(),
                "fills": c.fills.len(),
                "regions": c.regions.len(),
                "bodies": c.component_bodies.len(),
            }))
            .collect::<Vec<_>>(),
    })
}

fn pcb_document_summary(document: &pcb::Document) -> Value {
    json!({
        "components": document.components.len(),
        "pads": document.pads.len(),
        "tracks": document.tracks.len(),
        "vias": document.vias.len(),
        "arcs": document.arcs.len(),
        "texts": document.texts.len(),
        "fills": document.fills.len(),
        "regions": document.regions.len(),
        "bodies": document.component_bodies.len(),
        "polygons": document.polygons.len(),
        "rules": document.rules.len(),
        "classes": document.classes.len(),
        "differential_pairs": document.differential_pairs.len(),
        "rooms": document.rooms.len(),
        "embedded_boards": document.embedded_boards.len(),
        "nets": document.nets.len(),
        "pads_assigned_to_components": document
            .components
            .iter()
            .map(|c| c.pads.len())
            .sum::<usize>(),
        "has_board_params": document.board_parameters.is_some(),
    })
}

fn sch_library_summary(library: &sch::Library) -> Value {
    json!({
        "components": library
            .components
            .iter()
            .map(|c| json!({
                "name": c.name,
                "description": c.description,
                "designator_prefix": c.designator_prefix,
                "comment": c.comment,
                "pins": c.pins.len(),
                "lines": c.lines.len(),
                "rectangles": c.rectangles.len(),
                "polygons": c.polygons.len(),
                "polylines": c.polylines.len(),
                "arcs": c.arcs.len(),
                "labels": c.labels.len(),
                "parameters": c.parameters.len(),
                "implementations": c.implementations.len(),
                "raw_records": c.raw_records.len(),
            }))
            .collect::<Vec<_>>(),
        "section_keys": library.section_keys.len(),
        "embedded_images": library.embedded_images.len(),
    })
}

fn sch_document_summary(document: &sch::Document) -> Value {
    json!({
        "components": document.components.len(),
        "wires": document.wires.len(),
        "junctions": document.junctions.len(),
        "net_labels": document.net_labels.len(),
        "power_objects": document.power_objects.len(),
        "ports": document.ports.len(),
        "no_ercs": document.no_ercs.len(),
        "buses": document.buses.len(),
        "bus_entries": document.bus_entries.len(),
        "parameters": document.parameters.len(),
        "parameter_sets": document.parameter_sets.len(),
        "blankets": document.blankets.len(),
        "sheet_symbols": document.sheet_symbols.len(),
        "raw_records": document.raw_records.len(),
        "has_header_params": document.header_parameters.is_some(),
        "additional_streams": document.additional_streams.len(),
    })
}

#[test]
fn snapshot_pcblib() {
    let Some(dir) = testdata_dir() else {
        return;
    };
    let Some(path) = locate_first(&dir, "pcblib") else {
        return;
    };
    let bytes = fs::read(&path).unwrap();
    let library = pcb::Library::from_bytes(bytes).unwrap();
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("snapshots");
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        insta::assert_json_snapshot!("pcblib", pcb_library_summary(&library));
    });
}

#[test]
fn snapshot_pcbdoc() {
    let Some(dir) = testdata_dir() else {
        return;
    };
    let Some(path) = locate_first(&dir, "pcbdoc") else {
        return;
    };
    let bytes = fs::read(&path).unwrap();
    let document = pcb::Document::from_bytes(bytes).unwrap();
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("snapshots");
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        insta::assert_json_snapshot!("pcbdoc", pcb_document_summary(&document));
    });
}

#[test]
fn snapshot_schlib() {
    let Some(dir) = testdata_dir() else {
        return;
    };
    let Some(path) = locate_first(&dir, "schlib") else {
        return;
    };
    let bytes = fs::read(&path).unwrap();
    let library = sch::Library::from_bytes(bytes).unwrap();
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("snapshots");
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        insta::assert_json_snapshot!("schlib", sch_library_summary(&library));
    });
}

#[test]
fn snapshot_schdoc() {
    let Some(dir) = testdata_dir() else {
        return;
    };
    let Some(path) = locate_first(&dir, "schdoc") else {
        return;
    };
    let bytes = fs::read(&path).unwrap();
    let document = sch::Document::from_bytes(bytes).unwrap();
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("snapshots");
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        insta::assert_json_snapshot!("schdoc", sch_document_summary(&document));
    });
}
