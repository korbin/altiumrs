//! The two per-footprint identity tables Altium keeps next to the records.
//!
//! `PrimitiveGuids/Data` holds 24-byte rows: a u32 type code (the record
//! type byte for arcs, pads, vias, tracks, texts and fills, 89 for regions,
//! 90 for bodies, 85 for the footprint itself in row 0), a u32 0-based
//! record index and a 16-byte GUID. `UniqueIDPrimitiveInformation/Data`
//! holds one `|PRIMITIVEINDEX=i|PRIMITIVEOBJECTID=Pad|UNIQUEID=XXXXXXXX`
//! entry per pad, by record index. Both refer to records by position, so
//! they are read into the primitives here and regenerated on every write;
//! carrying them verbatim breaks Altium ("Catastrophic Failure" on load) as
//! soon as a record is inserted or removed before a pad.

use std::hash::{BuildHasher, RandomState};
use std::sync::atomic::{AtomicU64, Ordering};

use super::component::Component;
use super::primitives::ExtendedEntry;

/// Side tables keyed by record index, with an inline `u32` count.
const INLINE_COUNT_TABLES: [&str; 3] = ["CornerRadiusChamfer", "CustomShapes", "SharedUnion"];
/// Side table stored as a storage with `Header` (count) and `Data`.
const HEADER_TABLE: &str = "ExtendedPrimitiveInformation";

/// Type code used in `PrimitiveGuids` rows for a record type byte.
pub fn guid_type_code(kind: u8) -> u32 {
    match kind {
        11 => 89,
        12 => 90,
        k => u32::from(k),
    }
}

/// Type code of the footprint row (row 0).
pub const COMPONENT_GUID_CODE: u32 = 85;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn random_u64() -> u64 {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    RandomState::new().hash_one((n, t))
}

/// A fresh `{8-4-4-4-12}` GUID (random, version 4 layout).
pub fn new_guid() -> String {
    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&random_u64().to_le_bytes());
    b[8..].copy_from_slice(&random_u64().to_le_bytes());
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format_guid(&b)
}

/// A fresh eight-letter unique id of the kind Altium gives pads.
pub fn new_unique_id() -> String {
    let mut r = random_u64();
    (0..8)
        .map(|_| {
            let c = (b'A' + (r % 26) as u8) as char;
            r /= 26;
            c
        })
        .collect()
}

/// Format the 16 bytes of a GUID row (mixed-endian, as stored) as
/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
pub fn format_guid(b: &[u8; 16]) -> String {
    format!(
        "{{{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        b[3], b[2], b[1], b[0], b[5], b[4], b[7], b[6], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

/// Parse a `{8-4-4-4-12}` string back into the stored byte order.
pub fn parse_guid(s: &str) -> Option<[u8; 16]> {
    let hex: Vec<u8> = s
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .bytes()
        .filter(|b| *b != b'-')
        .collect();
    if hex.len() != 32 {
        return None;
    }
    let mut v = [0u8; 16];
    for i in 0..16 {
        v[i] = u8::from_str_radix(std::str::from_utf8(&hex[2 * i..2 * i + 2]).ok()?, 16).ok()?;
    }
    Some([
        v[3], v[2], v[1], v[0], v[5], v[4], v[7], v[6], v[8], v[9], v[10], v[11], v[12], v[13], v[14], v[15],
    ])
}

fn entry_index(text: &str) -> Option<usize> {
    text.split('|')
        .find_map(|kv| kv.strip_prefix("PRIMITIVEINDEX="))
        .and_then(|v| v.trim().parse().ok())
}

fn with_index(text: &str, i: usize) -> String {
    text.split('|')
        .map(|kv| {
            if kv.starts_with("PRIMITIVEINDEX=") {
                format!("PRIMITIVEINDEX={i}")
            } else {
                kv.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// Parse `[u32 len][text\0]...` entries, optionally preceded by a `u32` count.
fn parse_entries(data: &[u8], inline_count: bool) -> Vec<String> {
    let mut p = if inline_count { 4 } else { 0 };
    let mut out = Vec::new();
    while p + 4 <= data.len() {
        let len = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
        let entry = &data[(p + 4).min(data.len())..(p + 4 + len).min(data.len())];
        p += 4 + len;
        out.push(crate::encoding::decode(entry.strip_suffix(&[0]).unwrap_or(entry)));
    }
    out
}

fn extended_slot<'a>(component: &'a mut Component, kind: u8, k: usize) -> Option<&'a mut Vec<ExtendedEntry>> {
    match kind {
        1 => component.arcs.get_mut(k).map(|p| &mut p.extended),
        2 => component.pads.get_mut(k).map(|p| &mut p.extended),
        3 => component.vias.get_mut(k).map(|p| &mut p.extended),
        4 => component.tracks.get_mut(k).map(|p| &mut p.extended),
        5 => component.texts.get_mut(k).map(|p| &mut p.extended),
        6 => component.fills.get_mut(k).map(|p| &mut p.extended),
        11 => component.regions.get_mut(k).map(|p| &mut p.extended),
        12 => component.component_bodies.get_mut(k).map(|p| &mut p.extended),
        _ => None,
    }
}

fn extended_of(component: &Component, kind: u8, k: usize) -> &[ExtendedEntry] {
    match kind {
        1 => component.arcs.get(k).map(|p| p.extended.as_slice()),
        2 => component.pads.get(k).map(|p| p.extended.as_slice()),
        3 => component.vias.get(k).map(|p| p.extended.as_slice()),
        4 => component.tracks.get(k).map(|p| p.extended.as_slice()),
        5 => component.texts.get(k).map(|p| p.extended.as_slice()),
        6 => component.fills.get(k).map(|p| p.extended.as_slice()),
        11 => component.regions.get(k).map(|p| p.extended.as_slice()),
        12 => component.component_bodies.get(k).map(|p| p.extended.as_slice()),
        _ => None,
    }
    .unwrap_or(&[])
}

/// (kind, list index) of every record, in record order.
pub(crate) fn record_slots(order: &[u8]) -> Vec<(u8, usize)> {
    let mut counters = [0usize; 13];
    order
        .iter()
        .map(|&kind| {
            let k = counters[kind as usize];
            counters[kind as usize] += 1;
            (kind, k)
        })
        .collect()
}

/// Split the tables out of a freshly read footprint's `additional_streams`
/// into the typed `guid` / `unique_id` fields. `order` is the record order of
/// the `Data` stream. Returns how many entries could not be applied.
pub(crate) fn absorb_tables(component: &mut Component, order: &[u8]) -> usize {
    let mut skipped = 0;
    let guids = component.additional_streams.remove("PrimitiveGuids/Data");
    component.additional_streams.remove("PrimitiveGuids/Header");
    if let Some(g) = guids {
        let rows: Vec<&[u8]> = g.chunks_exact(24).collect();
        if let Some(row0) = rows.first() {
            if u32::from_le_bytes(row0[0..4].try_into().unwrap()) == COMPONENT_GUID_CODE {
                component.guid = Some(format_guid(row0[8..24].try_into().unwrap()));
            }
        }
        let mut counters = [0usize; 13];
        for (i, &kind) in order.iter().enumerate() {
            let guid = rows
                .get(i + 1)
                .filter(|r| u32::from_le_bytes(r[4..8].try_into().unwrap()) as usize == i)
                .map(|r| format_guid(r[8..24].try_into().unwrap()));
            let k = counters[kind as usize];
            counters[kind as usize] += 1;
            let slot = match kind {
                1 => component.arcs.get_mut(k).map(|p| &mut p.guid),
                2 => component.pads.get_mut(k).map(|p| &mut p.guid),
                3 => component.vias.get_mut(k).map(|p| &mut p.guid),
                4 => component.tracks.get_mut(k).map(|p| &mut p.guid),
                5 => component.texts.get_mut(k).map(|p| &mut p.guid),
                6 => component.fills.get_mut(k).map(|p| &mut p.guid),
                11 => component.regions.get_mut(k).map(|p| &mut p.guid),
                12 => component.component_bodies.get_mut(k).map(|p| &mut p.guid),
                _ => None,
            };
            match (slot, guid) {
                (Some(slot), Some(guid)) => *slot = Some(guid),
                (_, None) if rows.len() > 1 => skipped += 1,
                _ => {}
            }
        }
    }
    let ids = component.additional_streams.remove("UniqueIDPrimitiveInformation/Data");
    component.additional_streams.remove("UniqueIDPrimitiveInformation/Header");
    if let Some(u) = ids {
        // record index -> pad list index
        let mut pad_at = vec![None; order.len()];
        let mut k = 0;
        for (i, &kind) in order.iter().enumerate() {
            if kind == 2 {
                pad_at[i] = Some(k);
                k += 1;
            }
        }
        let mut p = 0;
        while p + 4 <= u.len() {
            let len = u32::from_le_bytes(u[p..p + 4].try_into().unwrap()) as usize;
            let entry = &u[(p + 4).min(u.len())..(p + 4 + len).min(u.len())];
            p += 4 + len;
            let text = crate::encoding::decode(entry.strip_suffix(&[0]).unwrap_or(entry));
            let mut index = None;
            let mut id = None;
            let mut is_pad = false;
            for kv in text.split('|') {
                if let Some(v) = kv.strip_prefix("PRIMITIVEINDEX=") {
                    index = v.trim().parse::<usize>().ok();
                } else if let Some(v) = kv.strip_prefix("PRIMITIVEOBJECTID=") {
                    is_pad = v == "Pad";
                } else if let Some(v) = kv.strip_prefix("UNIQUEID=") {
                    id = Some(v.to_string());
                }
            }
            match (index.and_then(|i| pad_at.get(i).copied().flatten()), id, is_pad) {
                (Some(k), Some(id), true) => {
                    if let Some(pad) = component.pads.get_mut(k) {
                        pad.unique_id = Some(id);
                    }
                }
                _ => skipped += 1,
            }
        }
    }
    // Side tables keyed by record index.
    let slots = record_slots(order);
    let mut tables: Vec<(String, Vec<String>)> = Vec::new();
    for name in INLINE_COUNT_TABLES {
        if let Some(data) = component.additional_streams.get(name) {
            let entries = parse_entries(data, true);
            if !entries.is_empty() {
                component.additional_streams.remove(name);
                tables.push((name.to_string(), entries));
            }
        }
    }
    if let Some(data) = component.additional_streams.get(&format!("{HEADER_TABLE}/Data")) {
        let entries = parse_entries(data, false);
        if !entries.is_empty() {
            component.additional_streams.remove(&format!("{HEADER_TABLE}/Data"));
            component.additional_streams.remove(&format!("{HEADER_TABLE}/Header"));
            tables.push((HEADER_TABLE.to_string(), entries));
        }
    }
    for (stream, entries) in tables {
        for (seq, text) in entries.into_iter().enumerate() {
            match entry_index(&text).and_then(|i| slots.get(i).copied()) {
                Some((kind, k)) => {
                    if let Some(slot) = extended_slot(component, kind, k) {
                        slot.push(ExtendedEntry {
                            stream: stream.clone(),
                            text,
                            seq: seq as u32,
                        });
                        continue;
                    }
                    skipped += 1;
                }
                None => skipped += 1,
            }
        }
    }
    // Regions that belong to a pad name it by record ordinal (1-based).
    let pad_at: Vec<Option<usize>> = slots
        .iter()
        .map(|&(kind, k)| if kind == 2 { Some(k) } else { None })
        .collect();
    for region in &mut component.regions {
        let Some(extra) = region.additional_parameters.as_mut() else { continue };
        let Some(key) = extra.keys().find(|k| k.eq_ignore_ascii_case("PADINDEX")).cloned() else { continue };
        let value = extra.get(&key).and_then(|v| v.trim().parse::<usize>().ok());
        match value.and_then(|n| n.checked_sub(1)).and_then(|i| pad_at.get(i).copied().flatten()) {
            Some(k) => {
                region.pad_ref = Some(k);
                extra.remove(&key);
            }
            None => skipped += 1,
        }
    }
    skipped
}

/// The regenerated side tables for a record order: stream path relative to
/// the footprint storage, and its bytes.
pub(crate) fn build_side_tables(component: &Component, order: &[u8]) -> Vec<(String, Vec<u8>)> {
    let slots = record_slots(order);
    let mut grouped: std::collections::BTreeMap<String, Vec<(u32, usize, String)>> = std::collections::BTreeMap::new();
    for (i, &(kind, k)) in slots.iter().enumerate() {
        for e in extended_of(component, kind, k) {
            grouped.entry(e.stream.clone()).or_default().push((e.seq, i, with_index(&e.text, i)));
        }
    }
    let mut out = Vec::new();
    for (stream, mut entries) in grouped {
        entries.sort_by_key(|(seq, i, _)| (*seq, *i));
        let entries: Vec<String> = entries.into_iter().map(|(_, _, t)| t).collect();
        let mut data = Vec::new();
        let inline = stream != HEADER_TABLE;
        if inline {
            data.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        }
        for e in &entries {
            let mut bytes = crate::encoding::encode(e);
            bytes.push(0);
            data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&bytes);
        }
        if inline {
            out.push((stream, data));
        } else {
            out.push((format!("{stream}/Header"), (entries.len() as u32).to_le_bytes().to_vec()));
            out.push((format!("{stream}/Data"), data));
        }
    }
    out
}

/// Record ordinal (1-based) of every pad, by pad list index.
pub(crate) fn pad_ordinals(order: &[u8]) -> Vec<usize> {
    record_slots(order)
        .iter()
        .enumerate()
        .filter(|(_, (kind, _))| *kind == 2)
        .map(|(i, _)| i + 1)
        .collect()
}

/// Build both tables for the record order the writer is about to emit.
/// Returns `(PrimitiveGuids data, UniqueIDPrimitiveInformation data, entry count)`.
pub(crate) fn build_tables(component: &Component, order: &[u8]) -> (Vec<u8>, Vec<u8>, u32) {
    let mut guids = Vec::with_capacity((order.len() + 1) * 24);
    let comp_guid = component.guid.as_deref().and_then(parse_guid).unwrap_or_else(|| parse_guid(&new_guid()).unwrap());
    guids.extend_from_slice(&COMPONENT_GUID_CODE.to_le_bytes());
    guids.extend_from_slice(&0u32.to_le_bytes());
    guids.extend_from_slice(&comp_guid);
    let mut ids = Vec::new();
    let mut count = 0u32;
    let mut counters = [0usize; 13];
    for (i, &kind) in order.iter().enumerate() {
        let k = counters[kind as usize];
        counters[kind as usize] += 1;
        let (guid, unique_id) = match kind {
            1 => (component.arcs.get(k).and_then(|p| p.guid.clone()), None),
            2 => component.pads.get(k).map(|p| (p.guid.clone(), p.unique_id.clone())).unwrap_or((None, None)),
            3 => (component.vias.get(k).and_then(|p| p.guid.clone()), None),
            4 => (component.tracks.get(k).and_then(|p| p.guid.clone()), None),
            5 => (component.texts.get(k).and_then(|p| p.guid.clone()), None),
            6 => (component.fills.get(k).and_then(|p| p.guid.clone()), None),
            11 => (component.regions.get(k).and_then(|p| p.guid.clone()), None),
            12 => (component.component_bodies.get(k).and_then(|p| p.guid.clone()), None),
            _ => (None, None),
        };
        let bytes = guid.as_deref().and_then(parse_guid).unwrap_or_else(|| parse_guid(&new_guid()).unwrap());
        guids.extend_from_slice(&guid_type_code(kind).to_le_bytes());
        guids.extend_from_slice(&(i as u32).to_le_bytes());
        guids.extend_from_slice(&bytes);
        if kind == 2 {
            let id = unique_id.unwrap_or_else(new_unique_id);
            let entry = format!("|PRIMITIVEINDEX={i}|PRIMITIVEOBJECTID=Pad|UNIQUEID={id}");
            let mut bytes = crate::encoding::encode(&entry);
            bytes.push(0);
            ids.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            ids.extend_from_slice(&bytes);
            count += 1;
        }
    }
    (guids, ids, count)
}
