//! Structural checks for a `.PcbLib`: the redundant tables Altium keeps next
//! to each footprint (record counts, per-primitive GUIDs, wide strings, the
//! component table of contents, embedded model checksums) must agree with the
//! records themselves, or Altium shows stale or missing data.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use crate::compound::CompoundFile;
use crate::error::Result;

use super::records::{kind_name, split_footprint_records, RawRecord};

/// One finding of [`check_pcblib`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Footprint the finding belongs to, `None` for library-level ones.
    pub footprint: Option<String>,
    pub message: String,
}

fn u32_at(b: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?))
}

fn c_string_block(b: &[u8]) -> Option<&[u8]> {
    let len = u32_at(b, 0)? as usize;
    let s = b.get(4..4 + len)?;
    Some(s.strip_suffix(&[0]).unwrap_or(s))
}

/// Altium's checksum of an embedded STEP payload: the byte-position-weighted
/// sum (weight 1 for the first byte) modulo 2^32, stored as a signed integer.
pub fn step_checksum(bytes: &[u8]) -> i32 {
    let mut sum: u32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let w = if i == 0 { 1 } else { i as u32 };
        sum = sum.wrapping_add((b as u32).wrapping_mul(w));
    }
    sum as i32
}

/// Expected main-block length of a record type in Altium's current layout.
fn expected_len(kind: u8) -> Option<usize> {
    match kind {
        1 => Some(60),
        2 => Some(202),
        4 => Some(49),
        5 => Some(252),
        6 => Some(50),
        _ => None,
    }
}

fn guid_type_code(kind: u8) -> Option<u32> {
    match kind {
        1 | 2 | 4 | 5 => Some(u32::from(kind)),
        11 => Some(89),
        12 => Some(90),
        _ => None,
    }
}

fn param_value<'a>(params: &'a [u8], key: &str) -> Option<&'a [u8]> {
    let needle = format!("{key}=");
    let mut p = 0;
    while let Some(i) = params[p..]
        .windows(needle.len())
        .position(|w| w == needle.as_bytes())
    {
        let at = p + i;
        if at == 0 || params[at - 1] == b'|' {
            let v = &params[at + needle.len()..];
            let end = v.iter().position(|&c| c == b'|').unwrap_or(v.len());
            return Some(&v[..end]);
        }
        p = at + 1;
    }
    None
}

/// Run every check on an opened `.PcbLib`.
pub fn check_pcblib(cf: &mut CompoundFile) -> Result<Vec<Problem>> {
    let mut problems = Vec::new();
    let mut lib_problem = |m: String| Problem {
        footprint: None,
        message: m,
    };

    // Footprint names in library order and their storage keys.
    let lib_data = cf.try_read_stream("Library/Data")?.unwrap_or_default();
    let mut names = Vec::new();
    if let Some(len) = u32_at(&lib_data, 0) {
        let mut p = 4 + len as usize;
        if let Some(count) = u32_at(&lib_data, p) {
            p += 4;
            for _ in 0..count {
                let Some(bl) = u32_at(&lib_data, p) else { break };
                let block = &lib_data[(p + 4).min(lib_data.len())..(p + 4 + bl as usize).min(lib_data.len())];
                if let Some((&n, rest)) = block.split_first() {
                    names.push(crate::encoding::decode(&rest[..(n as usize).min(rest.len())]));
                }
                p += 4 + bl as usize;
            }
        }
    } else {
        problems.push(lib_problem("Library/Data is missing or empty".into()));
    }
    let mut section_keys: BTreeMap<String, String> = BTreeMap::new();
    if let Some(sk) = cf.try_read_stream("SectionKeys")? {
        let mut p = 4;
        let count = u32_at(&sk, 0).unwrap_or(0);
        let read_block = |p: &mut usize| -> Option<String> {
            let bl = u32_at(&sk, *p)? as usize;
            let block = sk.get(*p + 4..*p + 4 + bl)?;
            *p += 4 + bl;
            let (&n, rest) = block.split_first()?;
            Some(crate::encoding::decode(&rest[..(n as usize).min(rest.len())]))
        };
        for _ in 0..count {
            let (Some(name), Some(key)) = (read_block(&mut p), read_block(&mut p)) else { break };
            section_keys.insert(name, key);
        }
    }
    let storages: BTreeSet<String> = cf
        .list_children("/")?
        .into_iter()
        .filter(|e| e.is_storage && e.name != "Library" && e.name != "FileVersionInfo")
        .map(|e| e.name)
        .collect();
    let mut expected_storages = BTreeSet::new();
    for name in &names {
        let key = section_keys.get(name).cloned().unwrap_or_else(|| name.clone());
        if !storages.contains(&key) {
            problems.push(lib_problem(format!("footprint {name} is listed in Library/Data but has no storage {key}")));
        }
        expected_storages.insert(key);
    }
    for s in storages.difference(&expected_storages) {
        problems.push(lib_problem(format!("storage {s} is not listed in Library/Data")));
    }

    // Per footprint.
    let mut toc_expected = Vec::new();
    let mut body_model_refs: Vec<(String, String)> = Vec::new();
    for name in &names {
        let key = section_keys.get(name).cloned().unwrap_or_else(|| name.clone());
        let mut problem = |m: String| Problem {
            footprint: Some(name.clone()),
            message: m,
        };
        let Some(data) = cf.try_read_stream(format!("{key}/Data"))? else {
            problems.push(problem("no Data stream".into()));
            continue;
        };
        let (pattern, records) = match split_footprint_records(&data) {
            Ok(v) => v,
            Err(e) => {
                problems.push(problem(format!("Data stream does not parse: {e}")));
                continue;
            }
        };
        if &pattern != name {
            problems.push(problem(format!("Data stream names the pattern {pattern:?}")));
        }
        if let Some(h) = cf.try_read_stream(format!("{key}/Header"))? {
            let n = u32_at(&h, 0).unwrap_or(0) as usize;
            if n != records.len() {
                problems.push(problem(format!("Header says {n} primitives, Data has {}", records.len())));
            }
        }
        let params = cf.try_read_stream(format!("{key}/Parameters"))?.unwrap_or_default();
        let params = c_string_block(&params).unwrap_or(&[]).to_vec();
        match param_value(&params, "PATTERN") {
            Some(p) if crate::encoding::decode(p) == *name => {}
            other => problems.push(problem(format!("Parameters PATTERN is {:?}", other.map(crate::encoding::decode)))),
        }
        toc_expected.push(name.clone());

        for (i, r) in records.iter().enumerate() {
            if let Some(exp) = expected_len(r.kind) {
                let got = r.main_block().len();
                if got != exp {
                    problems.push(problem(format!(
                        "record {i} ({}) has a {got}-byte body; Altium writes {exp} (legacy or truncated layout)",
                        kind_name(r.kind)
                    )));
                }
            }
            if r.kind == 12 {
                let body = r.main_block();
                if let Some(id) = param_value(body, "MODELID") {
                    let embed = param_value(body, "MODEL.EMBED").map(|v| v == b"TRUE").unwrap_or(false);
                    if embed {
                        body_model_refs.push((name.clone(), crate::encoding::decode(id)));
                    }
                }
            }
        }

        // Per-primitive GUID table: row 0 is the component, then one row per
        // record with its type code and 0-based index.
        if let Some(g) = cf.try_read_stream(format!("{key}/PrimitiveGuids/Data"))? {
            if !g.is_empty() {
                let rows = g.len() / 24;
                if g.len() % 24 != 0 || rows != records.len() + 1 {
                    problems.push(problem(format!(
                        "PrimitiveGuids has {rows} rows for {} records (expected {})",
                        records.len(),
                        records.len() + 1
                    )));
                } else {
                    for (i, r) in records.iter().enumerate() {
                        let row = &g[(i + 1) * 24..(i + 2) * 24];
                        let code = u32_at(row, 0).unwrap_or(0);
                        let idx = u32_at(row, 4).unwrap_or(0) as usize;
                        if idx != i {
                            problems.push(problem(format!("PrimitiveGuids row {} points at record {idx}, expected {i}", i + 1)));
                            break;
                        }
                        if let Some(exp) = guid_type_code(r.kind) {
                            if code != exp {
                                problems.push(problem(format!(
                                    "PrimitiveGuids row {} has type code {code} for a {} (expected {exp})",
                                    i + 1,
                                    kind_name(r.kind)
                                )));
                                break;
                            }
                        }
                    }
                }
                if let Some(h) = cf.try_read_stream(format!("{key}/PrimitiveGuids/Header"))? {
                    let n = u32_at(&h, 0).unwrap_or(0) as usize;
                    if n != rows {
                        problems.push(problem(format!("PrimitiveGuids/Header says {n} rows, Data has {rows}")));
                    }
                }
            }
        }

        // Wide strings: ENCODEDTEXT{i} per text, and each text points at its own entry.
        let texts: Vec<&RawRecord> = records.iter().filter(|r| r.kind == 5).collect();
        if !texts.is_empty() {
            let ws = cf.try_read_stream(format!("{key}/WideStrings"))?.unwrap_or_default();
            let ws = c_string_block(&ws).unwrap_or(&[]).to_vec();
            for (i, t) in texts.iter().enumerate() {
                let body = t.main_block();
                let idx = body.get(115..119).map(|b| i32::from_le_bytes(b.try_into().unwrap())).unwrap_or(-1);
                if idx != i as i32 {
                    problems.push(problem(format!("text {i} ({:?}) has wide-string index {idx}", t.text().unwrap_or_default())));
                }
                let expected: String = t
                    .text()
                    .unwrap_or_default()
                    .chars()
                    .map(|c| (c as u32).to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                match param_value(&ws, &format!("ENCODEDTEXT{i}")) {
                    Some(v) if v == expected.as_bytes() => {}
                    Some(v) => problems.push(problem(format!(
                        "WideStrings ENCODEDTEXT{i} is {:?}, text says {:?}",
                        crate::encoding::decode(v),
                        t.text().unwrap_or_default()
                    ))),
                    None => problems.push(problem(format!("WideStrings has no ENCODEDTEXT{i}"))),
                }
            }
        }
    }

    // Component table of contents.
    if let Some(toc) = cf.try_read_stream("Library/ComponentParamsTOC/Data")? {
        let body = c_string_block(&toc).unwrap_or(&[]);
        let listed: Vec<String> = body
            .split(|&c| c == b'\n')
            .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
            .filter(|l| !l.is_empty())
            .filter_map(|l| param_value(l, "Name").map(crate::encoding::decode))
            .collect();
        if listed != toc_expected {
            let extra: Vec<_> = listed.iter().filter(|n| !toc_expected.contains(n)).cloned().collect();
            let missing: Vec<_> = toc_expected.iter().filter(|n| !listed.contains(n)).cloned().collect();
            problems.push(lib_problem(format!(
                "ComponentParamsTOC lists {} footprints for {} in the library (extra: {:?}, missing: {:?})",
                listed.len(),
                toc_expected.len(),
                extra,
                missing
            )));
        }
    }

    // Embedded models: record count, payload streams and checksums.
    let mut model_ids = BTreeSet::new();
    if let Some(md) = cf.try_read_stream("Library/Models/Data")? {
        let mut p = 0;
        let mut i = 0;
        while let Some(len) = u32_at(&md, p) {
            let rec = md.get(p + 4..p + 4 + len as usize).unwrap_or(&[]);
            let rec = rec.strip_suffix(&[0]).unwrap_or(rec);
            if let Some(id) = param_value(rec, "ID") {
                model_ids.insert(crate::encoding::decode(id));
            }
            let embedded = param_value(rec, "EMBED").map(|v| v == b"TRUE").unwrap_or(false);
            match cf.try_read_stream(format!("Library/Models/{i}"))? {
                None => problems.push(lib_problem(format!("model {i} has no payload stream"))),
                Some(payload) if embedded => {
                    let mut out = Vec::new();
                    match flate2::read::ZlibDecoder::new(&payload[..]).read_to_end(&mut out) {
                        Ok(_) => {
                            let expected = param_value(rec, "CHECKSUM")
                                .and_then(|v| crate::encoding::decode(v).trim().parse::<i64>().ok())
                                .map(|v| v as i32);
                            let got = step_checksum(&out);
                            if expected != Some(got) {
                                problems.push(lib_problem(format!(
                                    "model {i} ({}) checksum is {:?}, payload gives {got}",
                                    param_value(rec, "NAME").map(crate::encoding::decode).unwrap_or_default(),
                                    expected
                                )));
                            }
                        }
                        Err(e) => problems.push(lib_problem(format!("model {i} payload does not inflate: {e}"))),
                    }
                }
                Some(_) => {}
            }
            p += 4 + len as usize;
            i += 1;
        }
        if let Some(h) = cf.try_read_stream("Library/Models/Header")? {
            let n = u32_at(&h, 0).unwrap_or(0) as usize;
            if n != i {
                problems.push(lib_problem(format!("Library/Models/Header says {n} models, Data has {i}")));
            }
        }
    }
    for (fp, id) in body_model_refs {
        if !model_ids.contains(&id) {
            problems.push(Problem {
                footprint: Some(fp),
                message: format!("body refers to embedded model {id} which is not in Library/Models"),
            });
        }
    }
    Ok(problems)
}
