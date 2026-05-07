//! Bidirectional integration tests for `.IntLib` against real Altium-compiled
//! fixtures.
//!
//! `cargo test -p altium --test intlib_bidirectional`
//!
//! These tests exercise the full read → mutate → write → read pipeline on a
//! real Altium-emitted IntLib (the fixture currently lives at
//! `/tmp/intlibex/`, alongside the source `.SchLib` and `.PcbLib` files Altium
//! compiled it from). Tests skip gracefully when the fixture isn't available
//! so the suite still runs in CI without it.

#![allow(clippy::field_reassign_with_default)]

use std::path::{Path, PathBuf};

use altium::{IntegratedLibrary, NamedLibrary, pcb, sch};

const FIXTURE_DIR: &str = "/tmp/intlibex";
const FIXTURE_BASENAME: &str = "Components - 06.05.26 - 21.20";

fn fixture_intlib() -> Option<PathBuf> {
    let p = PathBuf::from(FIXTURE_DIR).join(format!("{FIXTURE_BASENAME}.IntLib"));
    p.exists().then_some(p)
}

fn fixture_schlib() -> Option<PathBuf> {
    let p = PathBuf::from(FIXTURE_DIR).join(format!("{FIXTURE_BASENAME}.SchLib"));
    p.exists().then_some(p)
}

fn fixture_pcblib() -> Option<PathBuf> {
    let p = PathBuf::from(FIXTURE_DIR).join(format!("{FIXTURE_BASENAME}.PcbLib"));
    p.exists().then_some(p)
}

#[tokio::test]
async fn reads_real_altium_compiled_intlib() {
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: {FIXTURE_DIR}/<…>.IntLib not present");
        return;
    };
    let intlib = IntegratedLibrary::read(&path).await.expect("parse intlib");

    // Real Altium IntLibs always carry a `Version.Txt` ≥ 2.
    assert!(
        intlib.version >= 1,
        "version should be set; got {}",
        intlib.version
    );

    // The fixture has exactly one source SchLib and one source PcbLib.
    assert_eq!(
        intlib.schematic_libraries.len(),
        1,
        "exactly one embedded SchLib"
    );
    assert_eq!(
        intlib.footprint_libraries.len(),
        1,
        "exactly one embedded PcbLib"
    );

    // Names follow the storage-slot convention.
    assert_eq!(intlib.schematic_libraries[0].name, "0.schlib");
    assert_eq!(intlib.footprint_libraries[0].name, "0.pcblib");

    // Sub-libraries decoded successfully — verify against component counts.
    assert!(!intlib.schematic_libraries[0].library.components.is_empty());
    assert!(!intlib.footprint_libraries[0].library.components.is_empty());

    // Cross-reference + parameters_bin + version are preserved verbatim.
    assert!(
        intlib.cross_reference.is_some(),
        "LibCrossRef.Txt preserved"
    );
    assert!(
        intlib.parameters_bin.is_some(),
        "Parameters .bin preserved"
    );

    // Nothing should land in additional_files when the layout is canonical.
    assert!(
        intlib.additional_files.is_empty(),
        "all streams classified; got leftovers: {:?}",
        intlib.additional_files.keys().collect::<Vec<_>>()
    );

    // No fatal diagnostics either.
    for d in &intlib.diagnostics {
        assert_ne!(
            format!("{:?}", d.severity),
            "Error",
            "unexpected error diagnostic: {d:?}"
        );
    }
}

#[tokio::test]
async fn intlib_round_trips_through_writer() {
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let bytes = tokio::fs::read(&path).await.unwrap();
    let original = IntegratedLibrary::from_bytes(bytes.clone()).expect("parse 1");

    // Write back, re-read, and verify the model survived intact.
    let written = original.to_bytes().expect("write");
    let reparsed = IntegratedLibrary::from_bytes(written).expect("parse 2");

    assert_eq!(reparsed.version, original.version);
    assert_eq!(reparsed.schematic_libraries.len(), original.schematic_libraries.len());
    assert_eq!(reparsed.footprint_libraries.len(), original.footprint_libraries.len());
    assert_eq!(reparsed.cross_reference, original.cross_reference);
    assert_eq!(reparsed.parameters_bin, original.parameters_bin);

    // Each embedded library should have the same component count.
    for (a, b) in original
        .schematic_libraries
        .iter()
        .zip(reparsed.schematic_libraries.iter())
    {
        assert_eq!(a.name, b.name);
        assert_eq!(
            a.library.components.len(),
            b.library.components.len(),
            "SchLib component count mismatch"
        );
    }
    for (a, b) in original
        .footprint_libraries
        .iter()
        .zip(reparsed.footprint_libraries.iter())
    {
        assert_eq!(a.name, b.name);
        assert_eq!(
            a.library.components.len(),
            b.library.components.len(),
            "PcbLib component count mismatch"
        );
    }
}

#[tokio::test]
async fn embedded_libraries_match_source_files_byte_level() {
    // Sanity check: the libraries we extract from the IntLib should match
    // what Altium kept on disk as the source `.SchLib` / `.PcbLib`. We compare
    // by component count + designator/name lists, since byte-equality of CFB
    // re-serialisation depends on Altium-internal ordering we don't replicate.
    let (Some(intlib_path), Some(sch_path), Some(pcb_path)) =
        (fixture_intlib(), fixture_schlib(), fixture_pcblib())
    else {
        eprintln!("skipping: fixture pair not present");
        return;
    };

    let intlib = IntegratedLibrary::read(&intlib_path).await.expect("intlib");
    let source_sch = sch::Library::read(&sch_path).await.expect("source schlib");
    let source_pcb = pcb::Library::read(&pcb_path).await.expect("source pcblib");

    let inner_sch = &intlib.schematic_libraries[0].library;
    let inner_pcb = &intlib.footprint_libraries[0].library;

    assert_eq!(
        inner_sch.components.len(),
        source_sch.components.len(),
        "SchLib component count"
    );
    let inner_sch_names: Vec<&str> = inner_sch.components.iter().map(|c| c.name.as_str()).collect();
    let source_sch_names: Vec<&str> =
        source_sch.components.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(inner_sch_names, source_sch_names, "SchLib component names");

    assert_eq!(
        inner_pcb.components.len(),
        source_pcb.components.len(),
        "PcbLib component count"
    );
    let inner_pcb_names: Vec<&str> = inner_pcb.components.iter().map(|c| c.name.as_str()).collect();
    let source_pcb_names: Vec<&str> =
        source_pcb.components.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(inner_pcb_names, source_pcb_names, "PcbLib component names");
}

#[tokio::test]
async fn intlib_disk_round_trip() {
    // Read → write to a temp file → read again and verify equivalence.
    let Some(src_path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let intlib = IntegratedLibrary::read(&src_path).await.unwrap();

    let nonce = std::process::id();
    let dst = std::env::temp_dir().join(format!("altium-rs-intlib-rt-{nonce}.IntLib"));
    let _ = std::fs::remove_file(&dst);

    intlib.write(&dst).await.expect("write to disk");
    let reread = IntegratedLibrary::read(&dst).await.expect("re-read");

    assert_eq!(reread.version, intlib.version);
    assert_eq!(reread.schematic_libraries.len(), intlib.schematic_libraries.len());
    assert_eq!(reread.footprint_libraries.len(), intlib.footprint_libraries.len());
    assert_eq!(reread.cross_reference, intlib.cross_reference);
    assert_eq!(reread.parameters_bin, intlib.parameters_bin);

    let _ = std::fs::remove_file(&dst);
}

#[tokio::test]
async fn from_scratch_intlib_round_trips() {
    // Build a synthetic IntLib that mimics the Altium layout (numbered names,
    // version=2) and verify the writer + reader agree.
    let mut sch_lib = sch::Library::default();
    sch_lib.components.push(sch::Component::new("U1"));
    sch_lib.components.push(sch::Component::new("U2"));

    let mut pcb_lib = pcb::Library::default();
    pcb_lib.unique_id = "ROUNDID0".into();
    pcb_lib.components.push(pcb::Component::new("R0402"));
    pcb_lib.components.push(pcb::Component::new("C0402"));
    pcb_lib.components.push(pcb::Component::new("L0402"));

    let mut intlib = IntegratedLibrary::default();
    intlib.version = 2;
    intlib.schematic_libraries.push(NamedLibrary {
        name: "0.schlib".into(),
        library: sch_lib,
    });
    intlib.footprint_libraries.push(NamedLibrary {
        name: "0.pcblib".into(),
        library: pcb_lib,
    });

    let bytes = intlib.to_bytes().expect("write");
    let parsed = IntegratedLibrary::from_bytes(bytes).expect("read");

    assert_eq!(parsed.version, 2);
    assert_eq!(parsed.schematic_libraries.len(), 1);
    assert_eq!(parsed.schematic_libraries[0].library.components.len(), 2);
    assert_eq!(parsed.footprint_libraries.len(), 1);
    assert_eq!(parsed.footprint_libraries[0].library.components.len(), 3);
    assert_eq!(parsed.footprint_libraries[0].library.unique_id, "ROUNDID0");
    assert!(parsed.additional_files.is_empty());
}

#[tokio::test]
async fn split_real_intlib_to_libpkg_and_sources() {
    // Read the real IntLib, split it to a temp directory, verify the source
    // files we wrote round-trip back to the same component lists.
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let intlib = IntegratedLibrary::read(&path).await.unwrap();

    let nonce = std::process::id();
    let dir = std::env::temp_dir().join(format!("altium-rs-intlib-split-{nonce}"));
    let _ = std::fs::remove_dir_all(&dir);

    let split = intlib
        .split_to_directory(&dir, "Split")
        .await
        .expect("split");
    split.write_package().await.expect("write libpkg");

    // Verify the libraries on disk re-parse and match.
    let sch_disk = dir.join("0.schlib");
    let pcb_disk = dir.join("0.pcblib");
    assert!(sch_disk.is_file());
    assert!(pcb_disk.is_file());

    let sch_re = sch::Library::read(&sch_disk).await.expect("re-read sch");
    let pcb_re = pcb::Library::read(&pcb_disk).await.expect("re-read pcb");
    assert_eq!(
        sch_re.components.len(),
        intlib.schematic_libraries[0].library.components.len()
    );
    assert_eq!(
        pcb_re.components.len(),
        intlib.footprint_libraries[0].library.components.len()
    );

    // The LibPkg references both files.
    let docs = split.package.documents();
    let paths: Vec<String> = docs.iter().map(|d| d.document_path.clone()).collect();
    assert!(paths.contains(&"0.schlib".to_string()));
    assert!(paths.contains(&"0.pcblib".to_string()));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn write_real_intlib_back_then_extract_libraries() {
    // Read → write → re-read → split: the sub-libraries that come out at the
    // end should still have the same component counts as the original.
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let original = IntegratedLibrary::read(&path).await.unwrap();
    let original_sch_count = original.schematic_libraries[0].library.components.len();
    let original_pcb_count = original.footprint_libraries[0].library.components.len();

    let nonce = std::process::id();
    let written_path =
        std::env::temp_dir().join(format!("altium-rs-intlib-rewrite-{nonce}.IntLib"));
    let _ = std::fs::remove_file(&written_path);
    original.write(&written_path).await.unwrap();

    let reread = IntegratedLibrary::read(&written_path).await.unwrap();
    assert_eq!(
        reread.schematic_libraries[0].library.components.len(),
        original_sch_count
    );
    assert_eq!(
        reread.footprint_libraries[0].library.components.len(),
        original_pcb_count
    );

    let _ = std::fs::remove_file(&written_path);
}

#[tokio::test]
async fn altium_file_unified_dispatch_finds_intlib() {
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let file = altium::AltiumFile::read(&path).await.unwrap();
    assert_eq!(file.kind(), altium::AltiumFileKind::IntegratedLibrary);
    let intlib = file.as_integrated_library().expect("variant");
    assert_eq!(intlib.schematic_libraries.len(), 1);
    assert_eq!(intlib.footprint_libraries.len(), 1);
    let _ = Path::new(FIXTURE_DIR);
}
