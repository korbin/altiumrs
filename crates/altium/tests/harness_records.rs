//! Harness connector / entry / type / signal-harness records (215-218)
//! survive a write + read round trip and keep their geometry.

use altium::coord::{Coord, CoordPoint};
use altium::sch;

fn dxp(v: i32) -> Coord {
    Coord::from_raw(v * sch::binary::RAW_PER_DXP)
}

#[test]
fn harness_records_round_trip() {
    let mut doc = sch::Document::default();
    let mut hc = sch::HarnessConnector::default();
    hc.location = CoordPoint::new(dxp(450), dxp(650));
    hc.x_size = dxp(40);
    hc.y_size = dxp(40);
    hc.primary_connection_position = dxp(20);
    hc.side = 1;
    hc.line_width = 1;
    hc.color = 13213327;
    for (name, slot) in [("D_N", 1), ("D_P", 2)] {
        let mut e = sch::HarnessEntry::default();
        e.name = name.into();
        e.side = 1;
        e.distance_from_top = dxp(10 * slot);
        e.text_style = Some("Full".into());
        e.owner_index_additional_list = true;
        hc.entries.push(e);
    }
    let mut ht = sch::HarnessType::default();
    ht.text = "LVDS".into();
    ht.location = CoordPoint::new(dxp(460), dxp(650));
    ht.is_hidden = true;
    hc.harness_type = Some(ht);
    doc.harness_connectors.push(hc);
    let mut sh = sch::SignalHarness::default();
    sh.vertices = vec![
        CoordPoint::new(dxp(10), dxp(20)),
        CoordPoint::new(dxp(30), dxp(20)),
        CoordPoint::new(dxp(30), dxp(60)),
    ];
    sh.color = 7;
    doc.signal_harnesses.push(sh);

    let bytes = doc.to_bytes().expect("write");
    {
        let mut cf = altium::compound::CompoundFile::open(bytes.clone()).expect("ole");
        let add = cf.read_stream("Additional").expect("Additional stream");
        let text = String::from_utf8_lossy(&add).to_string();
        assert!(text.contains("|Weight=5"), "1 connector + 2 entries + 1 type + 1 harness: {text}");
        assert!(text.contains("|RECORD=215|"));
        let fh = cf.read_stream("FileHeader").expect("FileHeader");
        assert!(!String::from_utf8_lossy(&fh).contains("RECORD=215"), "harness records belong in Additional");
    }
    let parsed = sch::Document::from_bytes(bytes).expect("read");
    assert!(parsed.raw_records.is_empty(), "harness records must be typed");
    assert_eq!(parsed.additional_header_parameters.as_ref().and_then(|h| h.get("Weight").cloned()).as_deref(), Some("5"));
    assert_eq!(parsed.harness_connectors.len(), 1);
    let p = &parsed.harness_connectors[0];
    let o = &doc.harness_connectors[0];
    assert_eq!(p.location, o.location);
    assert_eq!(p.x_size, o.x_size);
    assert_eq!(p.y_size, o.y_size);
    assert_eq!(p.primary_connection_position, o.primary_connection_position);
    assert_eq!(p.side, o.side);
    assert_eq!(p.entries.len(), 2);
    for (pe, oe) in p.entries.iter().zip(&o.entries) {
        assert_eq!(pe.name, oe.name);
        assert_eq!(pe.side, oe.side);
        assert_eq!(pe.distance_from_top, oe.distance_from_top);
        assert_eq!(pe.text_style, oe.text_style);
    }
    let pt = p.harness_type.as_ref().expect("type");
    assert_eq!(pt.text, "LVDS");
    assert_eq!(pt.location, o.harness_type.as_ref().unwrap().location);
    assert!(pt.is_hidden);
    assert_eq!(parsed.signal_harnesses.len(), 1);
    assert_eq!(parsed.signal_harnesses[0].vertices, doc.signal_harnesses[0].vertices);
    assert_eq!(parsed.signal_harnesses[0].color, 7);
}

#[test]
fn entry_distance_is_scaled_from_grid_slots() {
    // A raw sheet entry with DISTANCEFROMTOP=9 sits nine 100-mil slots
    // (90 DXP) below the symbol's top edge.
    let mut doc = sch::Document::default();
    let mut sym = sch::SheetSymbol::default();
    sym.location = CoordPoint::new(dxp(940), dxp(520));
    sym.x_size = dxp(200);
    sym.y_size = dxp(340);
    let mut e = sch::SheetEntry::default();
    e.name = "GEM3_RGMII".into();
    e.distance_from_top = dxp(90);
    sym.entries.push(e);
    doc.sheet_symbols.push(sym);
    let bytes = doc.to_bytes().expect("write");
    let text = String::from_utf8_lossy(&bytes).to_string();
    assert!(
        text.contains("DISTANCEFROMTOP=9|") || text.contains("DISTANCEFROMTOP=9\0"),
        "expected slot count 9 in the written record"
    );
    let parsed = sch::Document::from_bytes(bytes).expect("read");
    assert_eq!(parsed.sheet_symbols[0].entries[0].distance_from_top, dxp(90));
}
