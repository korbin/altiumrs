//! Compound-file smoke tests against real Altium libraries.

use std::path::Path;

use altium::compound::CompoundFile;

const SAMPLE_PCBLIB: &str = "../../testdata/PCB - LEADLESS - QFN - PSEMI QFN-12 3x3x0.5.PcbLib";
const SAMPLE_SCHLIB: &str = "../../testdata/SCH - RF - MIXER - AD ADL5801.SchLib";

fn read_file(path: &str) -> Option<Vec<u8>> {
    let p = Path::new(path);
    if !p.exists() {
        return None;
    }
    Some(std::fs::read(p).expect("read sample"))
}

#[test]
fn opens_pcblib_sample() {
    let Some(bytes) = read_file(SAMPLE_PCBLIB) else {
        eprintln!("skipping: {SAMPLE_PCBLIB} not present");
        return;
    };
    let cf = CompoundFile::open(bytes).expect("open pcblib");

    assert!(cf.is_stream("FileHeader"), "FileHeader stream missing");
    assert!(cf.is_storage("Library"), "Library storage missing");

    let library_children: Vec<_> = cf
        .list_children("Library")
        .expect("list /Library")
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(
        library_children.iter().any(|n| n == "Header"),
        "Library/Header stream missing; saw {library_children:?}"
    );
    assert!(
        library_children.iter().any(|n| n == "Data"),
        "Library/Data stream missing; saw {library_children:?}"
    );
}

#[test]
fn opens_schlib_sample() {
    let Some(bytes) = read_file(SAMPLE_SCHLIB) else {
        eprintln!("skipping: {SAMPLE_SCHLIB} not present");
        return;
    };
    let cf = CompoundFile::open(bytes).expect("open schlib");

    assert!(cf.is_stream("FileHeader"), "FileHeader stream missing");
    let root_children: Vec<_> = cf.list_children("/").expect("root listing");
    assert!(!root_children.is_empty());
}
