//! Document-stream record layout: text pins with inline customisation,
//! owner indices that survive re-ordering, and verbatim Storage passthrough.

#![allow(clippy::field_reassign_with_default)]

use std::collections::BTreeMap;

use altium::compound::CompoundFile;
use altium::coord::{Coord, CoordPoint};
use altium::enums::PinOrientation;
use altium::sch;
use altium::sch::implementation::{Implementation, MapDefiner};
use altium::sch::primitives::{Parameter, Pin, Rectangle};

/// `(flags, fields)` per record; fields of text records keyed by upper-case
/// name, binary records reported with an empty map.
fn records(schdoc: &[u8]) -> Vec<(u8, BTreeMap<String, String>)> {
    let mut cf = CompoundFile::open(schdoc.to_vec()).expect("compound file");
    let data = cf.read_stream("FileHeader").expect("FileHeader stream");
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 4 <= data.len() {
        let header = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        let len = (header & 0x00ff_ffff) as usize;
        let flags = (header >> 24) as u8;
        let body = &data[i + 4..i + 4 + len];
        let mut fields = BTreeMap::new();
        if flags == 0 {
            let text = String::from_utf8_lossy(body);
            for part in text.trim_end_matches('\0').split('|') {
                if let Some((k, v)) = part.split_once('=') {
                    fields.insert(k.to_ascii_uppercase(), v.to_string());
                }
            }
        }
        out.push((flags, fields));
        i += 4 + len;
    }
    out
}

fn header() -> BTreeMap<String, String> {
    let mut h = BTreeMap::new();
    h.insert("HEADER".to_string(), "Protel for Windows - Schematic Capture Binary File Version 5.0".to_string());
    h
}

fn pin(name: &str, designator: &str, y_mils: f64) -> Pin {
    let mut p = Pin::default();
    p.name = Some(name.into());
    p.designator = Some(designator.into());
    p.location = CoordPoint::new(Coord::from_mils(0.0), Coord::from_mils(y_mils));
    p.length = Coord::from_mils(300.0);
    p.orientation = PinOrientation::Left;
    p.show_name = true;
    p.show_designator = true;
    p.common.owner_part_id = 1;
    p
}

fn component(lib_ref: &str, pins: Vec<Pin>) -> sch::Component {
    let mut c = sch::Component::new(lib_ref);
    c.lib_reference = Some(lib_ref.into());
    c.pins = pins;
    c.parameters.push(Parameter {
        name: "Designator".into(),
        value: format!("{lib_ref}_DES"),
        ..Default::default()
    });
    c
}

#[test]
fn document_pins_are_text_records_with_inline_customisation() {
    let mut p = pin("NAME_A", "1", 100.0);
    p.name_font_mode = 1;
    p.name_custom_font_id = 2;
    p.name_custom_color = 255;
    p.name_position_mode = 1;
    p.name_custom_position_margin = 5;
    p.name_custom_position_rotation_relative = true;
    p.designator_font_mode = 1;
    p.designator_custom_font_id = 3;
    p.designator_custom_color = 16_711_680;
    p.pin_propagation_delay = 0.0;
    p.swap_id_part = Some("0¦&¦1".into());

    let mut doc = sch::Document::default();
    doc.header_parameters = Some(header());
    doc.components.push(component("PART", vec![p]));
    let bytes = doc.to_bytes().expect("write");

    let recs = records(&bytes);
    assert!(recs.iter().all(|(flags, _)| *flags == 0), "no binary records in a document");
    let pin_rec = recs
        .iter()
        .find(|(_, f)| f.get("RECORD").map(String::as_str) == Some("2"))
        .map(|(_, f)| f)
        .expect("a text pin record");
    // position 0x01 | rotation-relative 0x04 | font 0x10
    assert_eq!(pin_rec.get("PINNAME_POSITIONCONGLOMERATE").map(String::as_str), Some("21"));
    assert_eq!(pin_rec.get("NAME_CUSTOMFONTID").map(String::as_str), Some("2"));
    assert_eq!(pin_rec.get("NAME_CUSTOMCOLOR").map(String::as_str), Some("255"));
    assert_eq!(pin_rec.get("NAME_CUSTOMPOSITION_MARGIN").map(String::as_str), Some("5"));
    assert_eq!(pin_rec.get("PINDESIGNATOR_POSITIONCONGLOMERATE").map(String::as_str), Some("16"));
    assert_eq!(pin_rec.get("DESIGNATOR_CUSTOMFONTID").map(String::as_str), Some("3"));
    assert_eq!(pin_rec.get("DESIGNATOR_CUSTOMCOLOR").map(String::as_str), Some("16711680"));
    assert_eq!(pin_rec.get("PINPROPAGATIONDELAY").map(String::as_str), Some("0.000000E+000"));
    assert_eq!(pin_rec.get("SWAPIDPART").map(String::as_str), Some("0¦&¦1"));
    assert!(!pin_rec.keys().any(|k| k.contains('.') && k.contains("FONT")), "no dotted legacy keys");

    let back = sch::Document::from_bytes(bytes).expect("read");
    let q = &back.components[0].pins[0];
    assert_eq!(q.name_font_mode, 1);
    assert_eq!(q.name_custom_font_id, 2);
    assert_eq!(q.name_custom_color, 255);
    assert_eq!(q.name_position_mode, 1);
    assert_eq!(q.name_custom_position_margin, 5);
    assert!(q.name_custom_position_rotation_relative);
    assert_eq!(q.name_custom_position_rotation_anchor, 0);
    assert_eq!(q.designator_font_mode, 1);
    assert_eq!(q.designator_custom_font_id, 3);
    assert_eq!(q.designator_custom_color, 16_711_680);
    assert_eq!(q.swap_id_part.as_deref(), Some("0¦&¦1"));
}

#[test]
fn document_children_name_their_owner_by_absolute_record_index() {
    // Two components, each with a pin, a parameter and one implementation
    // with a map definer, behind a sheet record so indices are not trivial.
    let mut doc = sch::Document::default();
    doc.header_parameters = Some(header());
    let mut sheet = BTreeMap::new();
    sheet.insert("SHEETSTYLE".to_string(), "15".to_string());
    doc.sheet_settings = Some(sheet);
    // A free rectangle at document level must not acquire an owner.
    let mut free = Rectangle::default();
    free.corner1 = CoordPoint::new(Coord::from_mils(0.0), Coord::from_mils(0.0));
    free.corner2 = CoordPoint::new(Coord::from_mils(10.0), Coord::from_mils(10.0));
    doc.rectangles.push(free);
    for lib_ref in ["FIRST", "SECOND"] {
        let mut c = component(lib_ref, vec![pin(&format!("{lib_ref}_PIN"), "1", 0.0)]);
        let mut body = Rectangle::default();
        body.corner1 = CoordPoint::new(Coord::from_mils(0.0), Coord::from_mils(0.0));
        body.corner2 = CoordPoint::new(Coord::from_mils(100.0), Coord::from_mils(50.0));
        body.common.owner_part_id = 1;
        c.rectangles.push(body);
        let mut imp = Implementation::default();
        imp.model_name = Some(format!("{lib_ref}_MODEL"));
        imp.model_type = Some("PCBLIB".into());
        imp.map_definers.push(MapDefiner::default());
        c.implementations.push(imp);
        doc.components.push(c);
    }
    let bytes = doc.to_bytes().expect("write");
    let recs = records(&bytes);

    // Records after the header record are numbered from 0.
    let by_index = |i: usize| &recs[i + 1].1;
    let owner_of = |f: &BTreeMap<String, String>| -> usize {
        f.get("OWNERINDEX").and_then(|v| v.parse().ok()).expect("OWNERINDEX present")
    };
    let mut checked = 0;
    for (_, f) in recs.iter().skip(1) {
        match f.get("RECORD").map(String::as_str) {
            Some("14") => {
                // Body rectangles name their component; the free one has no owner.
                match f.get("OWNERINDEX") {
                    Some(_) => {
                        assert_eq!(by_index(owner_of(f)).get("RECORD").map(String::as_str), Some("1"));
                        checked += 1;
                    }
                    None => assert_eq!(f.get("CORNER.X").map(String::as_str), Some("1")),
                }
            }
            Some("2") | Some("34") | Some("41") | Some("44") => {
                let owner = by_index(owner_of(f));
                assert_eq!(owner.get("RECORD").map(String::as_str), Some("1"));
                // The pin/parameter/model belongs to the component it names.
                let lib = owner.get("LIBREFERENCE").cloned().unwrap_or_default();
                let mine = match f.get("RECORD").map(String::as_str) {
                    Some("2") => f.get("NAME").cloned(),
                    _ => f.get("TEXT").cloned(),
                }
                .unwrap_or_else(|| lib.clone());
                if f.get("RECORD").map(String::as_str) != Some("44") {
                    assert!(mine.starts_with(&lib), "{mine} owned by {lib}");
                }
                checked += 1;
            }
            Some("45") => {
                assert_eq!(by_index(owner_of(f)).get("RECORD").map(String::as_str), Some("44"));
                let comp = by_index(owner_of(by_index(owner_of(f))));
                let model = f.get("MODELNAME").cloned().unwrap_or_default();
                assert!(model.starts_with(comp.get("LIBREFERENCE").unwrap()), "{model}");
                checked += 1;
            }
            Some("46") | Some("48") => {
                assert_eq!(by_index(owner_of(f)).get("RECORD").map(String::as_str), Some("45"));
                checked += 1;
            }
            Some("47") => {
                assert_eq!(by_index(owner_of(f)).get("RECORD").map(String::as_str), Some("46"));
                checked += 1;
            }
            _ => {}
        }
    }
    assert!(checked >= 16, "checked {checked} owned records");

    // Reading the stream back files the bodies under their components again.
    let back = sch::Document::from_bytes(bytes).expect("read");
    assert_eq!(back.rectangles.len(), 1, "one free rectangle");
    assert!(back.components.iter().all(|c| c.rectangles.len() == 1 && c.pins.len() == 1));
}

#[test]
fn document_storage_stream_round_trips_verbatim() {
    let mut doc = sch::Document::default();
    doc.header_parameters = Some(header());
    let storage = b"\x15\x00\x00\x00|HEADER=Icon storage\x00".to_vec();
    doc.additional_streams.insert("Storage".into(), storage.clone());
    let bytes = doc.to_bytes().expect("write");
    let back = sch::Document::from_bytes(bytes).expect("read");
    assert_eq!(back.additional_streams.get("Storage"), Some(&storage));
}

#[test]
fn document_keeps_keys_the_model_does_not_know() {
    // A sheet symbol with an entry carrying keys the typed model has no
    // field for; both must come back after read -> write -> read.
    use altium::sch::component::RawRecord;
    let rec = |s: &str| RawRecord { flag: 0, bytes: format!("{s}\0").into_bytes() };
    let mut doc = sch::Document::default();
    doc.header_parameters = Some(header());
    doc.raw_records.push(rec("|RECORD=15|OWNERPARTID=-1|LOCATION.X=900|LOCATION.Y=1320|XSIZE=160|YSIZE=150|COLOR=128|AREACOLOR=8454016|ISSOLID=T|UNIQUEID=SYMBOLAA|SYMBOLTYPE=Normal"));
    doc.raw_records.push(rec("|RECORD=16|OWNERINDEX=0|OWNERPARTID=-1|DISTANCEFROMTOP=2|COLOR=128|AREACOLOR=8454143|TEXTCOLOR=128|TEXTFONTID=2|TEXTSTYLE=Full|NAME=RX_L_P|UNIQUEID=ENTRYAAA|IOTYPE=1|ARROWKIND=Block & Triangle"));
    doc.raw_records.push(rec("|RECORD=22|OWNERPARTID=-1|LOCATION.X=100|LOCATION.Y=100|COLOR=255|ISACTIVE=T|SYMBOL=Thick Cross|UNIQUEID=NOERCAAA"));
    let first = doc.to_bytes().expect("write raw");
    let typed = sch::Document::from_bytes(first).expect("read typed");
    assert_eq!(typed.sheet_symbols.len(), 1);
    assert_eq!(typed.no_ercs[0].symbol.as_deref(), Some("Thick Cross"));
    assert_eq!(typed.sheet_symbols[0].entries.len(), 1);
    let second = typed.to_bytes().expect("write typed");

    let recs = records(&second);
    let sym = recs.iter().find(|(_, f)| f.get("RECORD").map(String::as_str) == Some("15")).map(|(_, f)| f).expect("symbol");
    assert_eq!(sym.get("SYMBOLTYPE").map(String::as_str), Some("Normal"));
    let (entry_pos, entry) = recs
        .iter()
        .enumerate()
        .find(|(_, (_, f))| f.get("RECORD").map(String::as_str) == Some("16"))
        .map(|(i, (_, f))| (i, f))
        .expect("entry");
    assert_eq!(entry.get("TEXTFONTID").map(String::as_str), Some("2"));
    assert_eq!(entry.get("TEXTSTYLE").map(String::as_str), Some("Full"));
    assert_eq!(entry.get("ARROWKIND").map(String::as_str), Some("Block & Triangle"));
    // and it still names the symbol as its owner (records numbered after the header)
    let owner: usize = entry.get("OWNERINDEX").unwrap().parse().unwrap();
    assert_eq!(recs[owner + 1].1.get("RECORD").map(String::as_str), Some("15"));
    assert!(entry_pos > owner + 1, "entry follows its owner");
    assert!(!recs.iter().any(|(_, f)| f.keys().any(|k| k.starts_with("__"))), "no reserved keys on disk");
    let noerc = recs.iter().find(|(_, f)| f.get("RECORD").map(String::as_str) == Some("22")).map(|(_, f)| f).expect("no-erc");
    assert_eq!(noerc.get("SYMBOL").map(String::as_str), Some("Thick Cross"));
}
