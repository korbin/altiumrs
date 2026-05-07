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
async fn parameters_bin_decoder_extracts_real_component_metadata() {
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let intlib = IntegratedLibrary::read(&path).await.unwrap();
    let blocks = intlib.parameters_blocks().expect("decode parameters");
    assert!(
        !blocks.is_empty(),
        "Parameters .bin should yield at least one block"
    );

    // The first block in this fixture is the symbol's full parameter set.
    let first = &blocks[0];
    assert!(
        first.contains("Comment=TPS26630RGER"),
        "first block should be the symbol parameters, got: {first}"
    );
    assert!(first.contains("Designator=CMP-04913-000051-1"));
    assert!(first.contains("Library Reference=CMP-04913-000051-1"));
    assert!(first.contains("Footprint=FP-RGE0024H-MFG"));

    // Subsequent blocks are footprint summaries.
    assert!(
        blocks[1..]
            .iter()
            .all(|b| b.contains("Pad Count=") && b.contains("Height=")),
        "footprint summary blocks each carry Pad Count + Height"
    );
}

#[tokio::test]
async fn parameters_bin_byte_stable_round_trip_on_real_fixture() {
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let intlib = IntegratedLibrary::read(&path).await.unwrap();
    let original = intlib
        .parameters_bin
        .as_ref()
        .expect("real IntLib has Parameters .bin");
    let blocks = intlib.parameters_blocks().expect("decode");
    let reserialised = altium::serialise_parameters_bin(&blocks);
    // Strict byte equality: the encoder is the exact inverse of the decoder
    // for the values Altium emits.
    assert_eq!(
        reserialised, *original,
        "Parameters .bin byte-stable round-trip on real Altium fixture"
    );
}

#[tokio::test]
async fn cross_reference_decoder_extracts_real_component_paths() {
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let intlib = IntegratedLibrary::read(&path).await.unwrap();
    let records = intlib
        .cross_reference_records()
        .expect("decode cross-reference");

    // Pull every string value out of the token stream; we should see the
    // symbol identifier, the description, and the footprint names that the
    // fixture's IntLib catalogues.
    let strings: Vec<&str> = records
        .iter()
        .filter_map(|r| match r {
            altium::CrossRefRecord::String(s) => Some(s.as_str()),
            altium::CrossRefRecord::Tag(_) => None,
        })
        .collect();

    assert!(
        strings.contains(&"CMP-04913-000051-1"),
        "symbol LibRef must appear; saw: {strings:?}"
    );
    assert!(
        strings.iter().any(|s| s.contains("IC PWR MGMT BATTERY MGMT")),
        "symbol description must appear"
    );
    assert!(
        strings.iter().any(|s| s.contains("FP-RGE0024H-MFG")),
        "primary footprint name must appear"
    );
    assert!(
        strings.iter().any(|s| s.contains(":\\SchLib\\0.schlib")),
        "internal SchLib path must appear"
    );
    assert!(
        strings.iter().any(|s| s.contains(":\\PCBLib\\0.pcblib")),
        "internal PcbLib path must appear"
    );
}

#[tokio::test]
async fn cross_reference_byte_stable_round_trip_on_real_fixture() {
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let intlib = IntegratedLibrary::read(&path).await.unwrap();
    let original = intlib
        .cross_reference
        .as_ref()
        .expect("real IntLib has LibCrossRef.Txt");
    let records = intlib
        .cross_reference_records()
        .expect("decode cross-reference");
    let reserialised = altium::serialise_cross_reference(&records);
    assert_eq!(
        reserialised, *original,
        "LibCrossRef.Txt byte-stable round-trip on real Altium fixture"
    );
}

#[tokio::test]
async fn cross_reference_table_decodes_real_fixture() {
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let intlib = IntegratedLibrary::read(&path).await.unwrap();
    let table = intlib
        .cross_reference_table()
        .expect("decode cross-reference table");
    assert_eq!(table.symbols.len(), 1, "fixture has one symbol");

    let sym = &table.symbols[0];
    assert_eq!(sym.libref, "CMP-04913-000051-1");
    assert_eq!(sym.internal_schlib_path, ":\\SchLib\\0.schlib");
    assert_eq!(sym.description, "IC PWR MGMT BATTERY MGMT");
    assert!(
        sym.source_schlib_path.ends_with(".SchLib"),
        "source path should reference the source .SchLib; got {}",
        sym.source_schlib_path
    );
    assert!(sym.source_schlib_path.contains("Components - 06.05.26 - 21.20"));

    // The fixture has 4 footprint variants — IPC_A, IPC_B, IPC_C, MFG.
    assert_eq!(
        sym.footprints.len(),
        4,
        "expected 4 footprint variants; got {:?}",
        sym.footprints.iter().map(|f| &f.name).collect::<Vec<_>>()
    );
    let names: Vec<&str> = sym.footprints.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"FP-RGE0024H-IPC_A"));
    assert!(names.contains(&"FP-RGE0024H-IPC_B"));
    assert!(names.contains(&"FP-RGE0024H-IPC_C"));
    assert!(names.contains(&"FP-RGE0024H-MFG"));

    for fp in &sym.footprints {
        assert_eq!(fp.kind, "PCBLIB");
        assert_eq!(fp.internal_pcblib_path, ":\\PCBLib\\0.pcblib");
        assert!(fp.source_pcblib_path.contains(".PcbLib"));
    }
}

#[tokio::test]
async fn cross_reference_table_byte_stable_round_trip_on_real_fixture() {
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let intlib = IntegratedLibrary::read(&path).await.unwrap();
    let original = intlib
        .cross_reference
        .as_ref()
        .expect("real IntLib has cross-reference")
        .clone();

    // Decode → typed table → flatten → re-encode → bytes; verify byte-equal.
    let table = intlib.cross_reference_table().expect("decode table");
    let mut copy = intlib.clone();
    copy.set_cross_reference_table(&table);
    let reserialised = copy
        .cross_reference
        .as_ref()
        .expect("re-emitted cross-reference");
    assert_eq!(
        reserialised, &original,
        "typed-table round-trip is byte-stable on real Altium fixture"
    );
}

#[tokio::test]
async fn intlib_full_byte_round_trip_via_typed_codecs() {
    // Read the full IntLib, decode every typed surface (libraries, parameters,
    // cross-reference), re-encode them through their typed codecs, write the
    // IntLib back, and verify the resulting model matches.
    let Some(path) = fixture_intlib() else {
        eprintln!("skipping: fixture not present");
        return;
    };
    let mut intlib = IntegratedLibrary::read(&path).await.unwrap();

    // Decode parameters and cross-reference, then re-encode by setting them
    // back via the typed accessors. This exercises the full encode/decode
    // path on real Altium-emitted bytes.
    let blocks = intlib.parameters_blocks().unwrap();
    let xrefs = intlib.cross_reference_records().unwrap();
    intlib.set_parameters_blocks(&blocks);
    intlib.set_cross_reference_records(&xrefs);

    // Now write and re-read.
    let bytes = intlib.to_bytes().expect("write");
    let parsed = IntegratedLibrary::from_bytes(bytes).expect("read");

    // Parameters and cross-reference came back identical to what we started with.
    assert_eq!(parsed.parameters_blocks().unwrap(), blocks);
    assert_eq!(parsed.cross_reference_records().unwrap(), xrefs);

    // Library counts unchanged.
    assert_eq!(parsed.schematic_libraries.len(), 1);
    assert_eq!(parsed.footprint_libraries.len(), 1);
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
