//! Adversarial-input harness: malformed bytes must produce `Err`, never panic.

use std::fs;
use std::path::PathBuf;

use altium::{AltiumFileKind, pcb, sch};

fn testdata_dir() -> Option<PathBuf> {
    let candidates = [PathBuf::from("../../testdata"), PathBuf::from("testdata")];
    candidates.into_iter().find(|p| p.exists())
}

/// Try every reader against `bytes` and assert no panic. We don't care about
/// the result — both `Ok` and `Err` are valid responses to garbage.
fn try_all_readers(label: &str, bytes: Vec<u8>) {
    let _ = pcb::Library::from_bytes(bytes.clone());
    let _ = pcb::Document::from_bytes(bytes.clone());
    let _ = sch::Library::from_bytes(bytes.clone());
    let _ = sch::Document::from_bytes(bytes.clone());

    // Also exercise the kind-dispatching surface for completeness.
    for kind in [
        AltiumFileKind::PcbLibrary,
        AltiumFileKind::SchLibrary,
        AltiumFileKind::PcbDocument,
        AltiumFileKind::SchDocument,
    ] {
        let _ = altium::AltiumFile::from_bytes(kind, bytes.clone());
    }

    eprintln!("{label}: ok ({} bytes)", bytes.len());
}

#[test]
fn empty_buffer_does_not_panic() {
    try_all_readers("empty", Vec::new());
}

#[test]
fn single_byte_does_not_panic() {
    try_all_readers("single-byte", vec![0u8]);
}

#[test]
fn all_zero_buffer_does_not_panic() {
    try_all_readers("zeros-1k", vec![0u8; 1024]);
}

#[test]
fn all_ff_buffer_does_not_panic() {
    try_all_readers("ones-1k", vec![0xFFu8; 1024]);
}

#[test]
fn random_pseudo_buffer_does_not_panic() {
    // Deterministic xorshift — no PRNG dep needed; just produces a varied byte
    // stream that wouldn't accidentally form a valid CFB header.
    let mut state = 0x12345678u32;
    let mut bytes = vec![0u8; 4096];
    for b in &mut bytes {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *b = state as u8;
    }
    try_all_readers("xorshift-4k", bytes);
}

#[test]
fn cfb_magic_with_garbage_payload_does_not_panic() {
    // CFB magic bytes followed by 4 KiB of zeros — looks like the start of a
    // compound file but the directory/sector tables are all-zero garbage.
    let mut bytes = vec![
        0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, // magic
    ];
    bytes.resize(4 * 1024, 0);
    try_all_readers("cfb-magic-only", bytes);
}

#[test]
fn truncated_real_files_do_not_panic() {
    // Pull every file in testdata, lop off bytes from the end at 4 different
    // truncation points, and feed each truncation to every reader.
    let Some(dir) = testdata_dir() else {
        eprintln!("skipping: no testdata directory");
        return;
    };
    let read = match fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        if bytes.len() < 32 {
            continue;
        }
        for &fraction in &[0.05f64, 0.50, 0.95, 1.0] {
            let cut = ((bytes.len() as f64) * fraction) as usize;
            let mut truncated = bytes[..cut].to_vec();
            // Bonus: flip a byte in the middle to simulate corruption.
            if truncated.len() > 100 {
                let mid = truncated.len() / 2;
                truncated[mid] ^= 0xFF;
            }
            try_all_readers(
                &format!("trunc-{:.0}%-{}", fraction * 100.0, path.display()),
                truncated,
            );
        }
    }
}

#[test]
fn oversized_pseudo_block_headers_do_not_panic() {
    // Construct payloads that *look* like they declare huge block sizes.
    // The block-size field is masked to 24 bits at parse time, so the worst
    // case is a 16 MB allocation request — we still want it to surface as Err
    // on truncation rather than OOM the test.
    for declared_size in [0x00_FF_FF_FFu32, 0x00_FF_FF_00, 0x00_01_00_00] {
        let mut bytes = vec![
            0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, // CFB magic prefix
        ];
        // Pad to a typical CFB header size, then jam the size field somewhere
        // a parser might read it.
        bytes.resize(512, 0);
        bytes.extend_from_slice(&declared_size.to_le_bytes());
        // Tail with a few bytes — far less than the declared size.
        bytes.extend_from_slice(&[0xCC; 16]);
        try_all_readers(&format!("size-claim-{declared_size:08x}"), bytes);
    }
}

#[test]
fn coord_parser_does_not_panic_on_garbage() {
    // The Coord parser sits on the user-input boundary — exercise the
    // common-but-bad inputs. None should panic.
    use std::str::FromStr;

    use altium::Coord;

    let inputs = [
        "",
        " ",
        "   inch",
        "mil",
        "mm",
        "0xFF",
        "1e500",
        "-1e-500",
        "NaN",
        "inf",
        "1mil1mil",
        "999999999999999999999999",
        "💀",
        "1.0.0mil",
    ];
    for s in inputs {
        let _ = Coord::parse_altium(s);
        let _ = Coord::from_str(s);
    }
}

#[test]
fn parameter_map_does_not_panic_on_pathological_input() {
    use altium::parameter::{ParameterMap, Parameters};

    let inputs = [
        "",
        "|",
        "||",
        "|||||||",
        "|=",
        "|=value",
        "|key=",
        "|FOO",
        "|%UTF8%=value",
        "|%UTF8%KEY=",
        "|key=val|key=val|key=val", // duplicate keys
        "|\0=binary\0in\0value",
        // A single trailing entry — exercises the no-separator-after-last path.
        "|FOO=",
    ];
    for s in inputs {
        let _: Vec<_> = Parameters::new(s).collect();
        let _ = ParameterMap::parse(s);
    }

    // 1 MiB of `|=` separators — should iterate to completion and return
    // entries with empty names without panicking or hanging.
    let huge: String = "|=".repeat(1024 * 512);
    let count = Parameters::new(&huge).count();
    assert_eq!(count, 1024 * 512);
}
