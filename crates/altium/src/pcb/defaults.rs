//! Default parameter sets emitted when authoring a PcbLib from scratch.
//!
//! Altium Designer refuses to open a `.PcbLib` without a `V9_MASTERSTACK_*` /
//! `V9_STACK_LAYER*` block. The values below describe a generic 2-layer stack-up;
//! callers who want a different stackup can populate `library_parameters`
//! themselves and that gets written verbatim.

use std::collections::BTreeMap;

use crate::parameter::ParameterMap;

/// Append a 2-layer master stack to `params`, mirroring what the Altium editor
/// writes for a fresh blank PcbLib.
pub(crate) fn populate_default_pcblib_parameters(
    params: &mut ParameterMap,
    component_count: usize,
) {
    params.insert("HEADER", "PCB 6.0 Binary Library File");
    params.insert("KIND", "Protel_Advanced_PCB_Library");
    params.insert("VERSION", "3.00");
    let (date, time) = current_date_time();
    params.insert("DATE", date);
    params.insert("TIME", time);
    params.insert("WEIGHT", component_count.to_string());

    // Master stack metadata.
    params.insert("V9_MASTERSTACK_STYLE", "0");
    params.insert(
        "V9_MASTERSTACK_ID",
        "{00000000-0000-0000-0000-000000000001}",
    );
    params.insert("V9_MASTERSTACK_NAME", "Master layer stack");
    params.insert("V9_MASTERSTACK_SHOWTOPDIELECTRIC", "FALSE");
    params.insert("V9_MASTERSTACK_SHOWBOTTOMDIELECTRIC", "FALSE");
    params.insert("V9_MASTERSTACK_ISFLEX", "FALSE");

    // Standard 2-layer stack: TopPaste / TopOverlay / TopSolder / TopLayer /
    // Dielectric1 / BottomLayer / BottomSolder / BottomOverlay / BottomPaste.
    // LAYERIDs are the canonical Altium layer-ID enum values.
    type LayerSpec = (&'static str, u32, &'static [(&'static str, &'static str)]);
    let stack: &[LayerSpec] = &[
        ("Top Paste", 16_973_832, &[("USEDBYPRIMS", "FALSE")]),
        ("Top Overlay", 16_973_830, &[("USEDBYPRIMS", "TRUE")]),
        (
            "Top Solder",
            16_973_834,
            &[
                ("USEDBYPRIMS", "FALSE"),
                ("DIELTYPE", "3"),
                ("DIELCONST", "3.500"),
                ("DIELHEIGHT", "0.4mil"),
                ("DIELMATERIAL", "Solder Resist"),
                ("COVERLAY_EXPANSION", "0mil"),
            ],
        ),
        (
            "Top Layer",
            16_777_217,
            &[
                ("USEDBYPRIMS", "TRUE"),
                ("COPTHICK", "1.4mil"),
                ("COMPONENTPLACEMENT", "1"),
            ],
        ),
        (
            "Dielectric 1",
            17_039_361,
            &[
                ("USEDBYPRIMS", "FALSE"),
                ("DIELTYPE", "0"),
                ("DIELCONST", "4.800"),
                ("DIELHEIGHT", "12.6mil"),
                ("DIELMATERIAL", "FR-4"),
            ],
        ),
        (
            "Bottom Layer",
            16_842_751,
            &[
                ("USEDBYPRIMS", "TRUE"),
                ("COPTHICK", "1.4mil"),
                ("COMPONENTPLACEMENT", "2"),
            ],
        ),
        (
            "Bottom Solder",
            16_973_835,
            &[
                ("USEDBYPRIMS", "FALSE"),
                ("DIELTYPE", "3"),
                ("DIELCONST", "3.500"),
                ("DIELHEIGHT", "0.4mil"),
                ("DIELMATERIAL", "Solder Resist"),
                ("COVERLAY_EXPANSION", "0mil"),
            ],
        ),
        ("Bottom Overlay", 16_973_831, &[("USEDBYPRIMS", "FALSE")]),
        ("Bottom Paste", 16_973_833, &[("USEDBYPRIMS", "FALSE")]),
    ];

    for (i, (name, layer_id, extras)) in stack.iter().enumerate() {
        let prefix = format!("V9_STACK_LAYER{i}");
        params.insert(
            format!("{prefix}_ID").as_str(),
            stable_layer_guid(b"stack", i),
        );
        params.insert(format!("{prefix}_NAME").as_str(), (*name).to_string());
        params.insert(format!("{prefix}_LAYERID").as_str(), layer_id.to_string());
        for (k, v) in *extras {
            params.insert(format!("{prefix}_{k}").as_str(), (*v).to_string());
        }
    }

    // Cache layers — utility layers Altium expects to find. Without the cache
    // block the editor still opens the file but loses some layer toggles.
    let cache: &[(&str, u32)] = &[
        ("Multi-Layer", 16_973_839),
        ("Connections", 16_973_840),
        ("Background", 16_973_841),
        ("DRC Error Markers", 16_973_842),
        ("DRC Detail Markers", 16_973_850),
        ("Selections", 16_973_843),
        ("Visible Grid 1", 16_973_844),
        ("Visible Grid 2", 16_973_845),
        ("Pad Holes", 16_973_846),
        ("Via Holes", 16_973_847),
        ("Top Pad Master", 16_973_848),
        ("Bottom Pad Master", 16_973_849),
    ];
    for (i, (name, layer_id)) in cache.iter().enumerate() {
        let prefix = format!("V9_CACHE_LAYER{i}");
        params.insert(
            format!("{prefix}_ID").as_str(),
            stable_layer_guid(b"cache", i),
        );
        params.insert(format!("{prefix}_NAME").as_str(), (*name).to_string());
        params.insert(format!("{prefix}_LAYERID").as_str(), layer_id.to_string());
        params.insert(format!("{prefix}_USEDBYPRIMS").as_str(), "FALSE");
    }
}

/// Merge the defaults into the user-provided map, only filling in keys that are
/// missing. Used by the writer so a partially-populated `library_parameters`
/// still gets the load-bearing V9 stack entries.
pub(crate) fn ensure_pcblib_parameters_complete(
    user: &BTreeMap<String, String>,
    component_count: usize,
    out: &mut ParameterMap,
) {
    for (k, v) in user {
        out.insert(k, v.clone());
    }
    let mut defaults = ParameterMap::new();
    populate_default_pcblib_parameters(&mut defaults, component_count);
    for (name, value, _is_utf8) in defaults.iter() {
        if !out.contains_key(name) {
            out.insert(name, value.to_string());
        }
    }
}

/// Default Board6 scalar keys for a freshly-authored PcbDoc.
pub(crate) fn populate_default_pcbdoc_parameters(params: &mut ParameterMap) {
    let (date, time) = current_date_time_us();

    // Per-board header object that precedes the global settings.
    params.insert("SELECTION", "FALSE");
    params.insert("LAYER", "UNKNOWN");
    params.insert("LOCKED", "FALSE");
    params.insert("POLYGONOUTLINE", "FALSE");
    params.insert("USERROUTED", "TRUE");
    params.insert("KEEPOUT", "FALSE");
    params.insert("UNIONINDEX", "0");

    // Format-marker keys: KIND/VERSION are what makes the file read as a
    // modern PcbDoc rather than legacy Protel.
    params.insert("KIND", "Protel_Advanced_PCB");
    params.insert("VERSION", "5.01");
    params.insert("DATE", date);
    params.insert("TIME", time);

    params.insert("ORIGINX", "0mil");
    params.insert("ORIGINY", "0mil");
    params.insert("BIGVISIBLEGRIDSIZE", "0.000");
    params.insert("VISIBLEGRIDSIZE", "0.000");
    params.insert("ELECTRICALGRIDRANGE", "3.937mil");
    params.insert("ELECTRICALGRIDENABLED", "TRUE");
    params.insert("SNAPGRIDSIZE", "39370.078740");
    params.insert("SNAPGRIDSIZEX", "39370.078740");
    params.insert("SNAPGRIDSIZEY", "39370.078740");
    params.insert("TRACKGRIDSIZE", "200000.000000");
    params.insert("VIAGRIDSIZE", "200000.000000");
    params.insert("COMPONENTGRIDSIZE", "39370.078740");
    params.insert("COMPONENTGRIDSIZEX", "39370.078740");
    params.insert("COMPONENTGRIDSIZEY", "39370.078740");
    params.insert("DOTGRID", "TRUE");
    params.insert("DISPLAYUNIT", "0");
    params.insert("DESIGNATORDISPLAYMODE", "0");
    params.insert("PRIMITIVELOCK", "TRUE");
    params.insert("POLYGONTYPE", "Polygon");
    params.insert("POUROVER", "FALSE");
    params.insert("REMOVEDEAD", "FALSE");
    params.insert("GRIDSIZE", "10mil");
    params.insert("TRACKWIDTH", "10mil");
    params.insert("HATCHSTYLE", "None");
    params.insert("USEOCTAGONS", "FALSE");
    params.insert("MINPRIMLENGTH", "3mil");

    params.insert("SHEETX", "1000mil");
    params.insert("SHEETY", "1000mil");
    params.insert("SHEETWIDTH", "10000mil");
    params.insert("SHEETHEIGHT", "8000mil");
    params.insert("SHOWSHEET", "FALSE");
    params.insert("LOCKSHEET", "TRUE");
    params.insert("SPLITLINECOUNT", "0");

    // RECORD=Board terminates this section; further records (outline,
    // stack) chain via embedded `\r` separators on later RECORD=Board keys.
    params.insert("RECORD", "Board");

    for i in 1..=16 {
        params.insert(&format!("PLANE{i}NETNAME"), "(No Net)");
    }

    // Sentinel unit-square outline; callers usually overwrite VX/VY
    // with real board geometry before this runs.
    populate_default_pcbdoc_outline(params);

    populate_default_pcbdoc_v9_stack(params);
}

/// Emit a rectangular board outline as `KIND{i}` / `VX{i}` / `VY{i}` segments.
fn populate_default_pcbdoc_outline(params: &mut ParameterMap) {
    let verts = [
        ("0mil", "0mil"),
        ("3937.0079mil", "0mil"),
        ("3937.0079mil", "3937.0079mil"),
        ("0mil", "3937.0079mil"),
    ];
    for (i, (vx, vy)) in verts.iter().enumerate() {
        params.insert(&format!("KIND{i}"), "0");
        params.insert(&format!("VX{i}"), *vx);
        params.insert(&format!("VY{i}"), *vy);
        params.insert(&format!("CX{i}"), "0mil");
        params.insert(&format!("CY{i}"), "0mil");
        params.insert(&format!("SA{i}"), " 0.00000000000000E+0000");
        params.insert(&format!("EA{i}"), " 0.00000000000000E+0000");
        params.insert(&format!("R{i}"), "0mil");
    }
}

/// Build the Board6 entry list: user-supplied verbatim first, then any
/// default keys the user didn't already cover. When the user already
/// supplies a V9/V8 layer stack (signalled by `V9_MASTERSTACK_STYLE`),
/// the fallback stack's sub-keys are skipped wholesale — cherry-picking
/// them would inject layer slots that don't match the user's stack.
pub(crate) fn ensure_pcbdoc_board_parameters_complete(
    user: Option<&[(String, String)]>,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(user) = user {
        for (k, v) in user {
            seen.insert(k.to_ascii_uppercase());
            out.push((k.clone(), v.clone()));
        }
    }
    let user_has_stack = seen.contains("V9_MASTERSTACK_STYLE");

    let mut defaults = ParameterMap::new();
    populate_default_pcbdoc_parameters(&mut defaults);
    for (name, value, _is_utf8) in defaults.iter() {
        if seen.contains(&name.to_ascii_uppercase()) {
            continue;
        }
        if user_has_stack && is_v9_v8_stack_key(name) {
            continue;
        }
        out.push((name.to_string(), value.to_string()));
    }
    out
}

fn is_v9_v8_stack_key(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    n.starts_with("V9_MASTERSTACK")
        || n.starts_with("V9_STACK_LAYER")
        || n.starts_with("V9_CACHE_LAYER")
        || n.starts_with("V9_SUBSTACK")
        || n.starts_with("V9_TRACEIMPEDANCE")
        || n.starts_with("LAYERMASTERSTACK")
        || n.starts_with("LAYERSUBSTACK_V8")
        || n.starts_with("LAYER_V8_")
        || n == "LAYERSTACKSTYLE"
}

/// 2-layer fallback V9 / V8 / V7 stack, filled in when the caller
/// didn't supply one.
fn populate_default_pcbdoc_v9_stack(params: &mut ParameterMap) {
    type LayerSpec = (&'static str, u32, &'static [(&'static str, &'static str)]);
    let stack: &[LayerSpec] = &[
        ("Top Paste", 16_973_832, &[("USEDBYPRIMS", "FALSE")]),
        ("Top Overlay", 16_973_830, &[("USEDBYPRIMS", "TRUE")]),
        (
            "Top Solder",
            16_973_834,
            &[
                ("USEDBYPRIMS", "FALSE"),
                ("DIELTYPE", "3"),
                ("DIELCONST", "3.500"),
                ("DIELHEIGHT", "0.4mil"),
                ("DIELMATERIAL", "Solder Resist"),
                ("COVERLAY_EXPANSION", "0mil"),
            ],
        ),
        (
            "Top Layer",
            16_777_217,
            &[
                ("USEDBYPRIMS", "TRUE"),
                ("COPTHICK", "1.4mil"),
                ("COMPONENTPLACEMENT", "1"),
            ],
        ),
        (
            "Dielectric 1",
            17_039_361,
            &[
                ("USEDBYPRIMS", "FALSE"),
                ("DIELTYPE", "0"),
                ("DIELCONST", "4.800"),
                ("DIELHEIGHT", "12.6mil"),
                ("DIELMATERIAL", "FR-4"),
            ],
        ),
        (
            "Bottom Layer",
            16_842_751,
            &[
                ("USEDBYPRIMS", "TRUE"),
                ("COPTHICK", "1.4mil"),
                ("COMPONENTPLACEMENT", "2"),
            ],
        ),
        (
            "Bottom Solder",
            16_973_835,
            &[
                ("USEDBYPRIMS", "FALSE"),
                ("DIELTYPE", "3"),
                ("DIELCONST", "3.500"),
                ("DIELHEIGHT", "0.4mil"),
                ("DIELMATERIAL", "Solder Resist"),
                ("COVERLAY_EXPANSION", "0mil"),
            ],
        ),
        ("Bottom Overlay", 16_973_831, &[("USEDBYPRIMS", "FALSE")]),
        ("Bottom Paste", 16_973_833, &[("USEDBYPRIMS", "FALSE")]),
    ];

    if !params.contains_key("V9_MASTERSTACK_STYLE") {
        params.insert("V9_MASTERSTACK_STYLE", "0");
        params.insert(
            "V9_MASTERSTACK_ID",
            "{00000000-0000-0000-0000-000000000001}",
        );
        params.insert("V9_MASTERSTACK_NAME", "Master layer stack");
        params.insert("V9_MASTERSTACK_SHOWTOPDIELECTRIC", "FALSE");
        params.insert("V9_MASTERSTACK_SHOWBOTTOMDIELECTRIC", "FALSE");
        params.insert("V9_MASTERSTACK_ISFLEX", "FALSE");

        for (i, (name, layer_id, extras)) in stack.iter().enumerate() {
            let prefix = format!("V9_STACK_LAYER{i}");
            params.insert(
                format!("{prefix}_ID").as_str(),
                stable_layer_guid(b"doc_stack", i),
            );
            params.insert(format!("{prefix}_NAME").as_str(), (*name).to_string());
            params.insert(format!("{prefix}_LAYERID").as_str(), layer_id.to_string());
            for (k, v) in *extras {
                params.insert(format!("{prefix}_{k}").as_str(), (*v).to_string());
            }
        }
    }
}

fn current_date_time_us() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days_since_epoch = (secs / 86_400) as i64;
    let secs_in_day = (secs % 86_400) as u32;
    let (y, m, d) = days_to_ymd(days_since_epoch);
    let date = format!("{}/{}/{:04}", m, d, y);
    let h24 = secs_in_day / 3600;
    let mi = (secs_in_day % 3600) / 60;
    let s = secs_in_day % 60;
    let ampm = if h24 < 12 { "AM" } else { "PM" };
    let h12 = match h24 % 12 {
        0 => 12,
        h => h,
    };
    let time = format!("{}:{:02}:{:02} {}", h12, mi, s, ampm);
    (date, time)
}

fn current_date_time() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days_since_epoch = (secs / 86_400) as i64;
    let secs_in_day = (secs % 86_400) as u32;
    let (y, m, d) = days_to_ymd(days_since_epoch);
    let date = format!("{:02}/{:02}/{:04}", d, m, y);
    let h = secs_in_day / 3600;
    let mi = (secs_in_day % 3600) / 60;
    let s = secs_in_day % 60;
    let time = format!("{:02}:{:02}:{:02}", h, mi, s);
    (date, time)
}

/// Days since `1970-01-01` → `(year, month, day)`. Howard Hinnant's algorithm —
/// works for any year ≥ -32767.
fn days_to_ymd(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Build a deterministic GUID for layer entries when generating defaults from
/// scratch. The output looks like a real GUID but is stable per-namespace+index
/// so writes are reproducible.
fn stable_layer_guid(namespace: &[u8], index: usize) -> String {
    use std::hash::{BuildHasher, Hash, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    namespace.hash(&mut hasher);
    index.hash(&mut hasher);
    let h = hasher.finish();
    let lo = h as u32;
    let hi = (h >> 32) as u32;
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}",
        lo,
        (hi & 0xFFFF) as u16,
        ((hi >> 16) & 0xFFFF) as u16,
        ((lo ^ hi) & 0xFFFF) as u16,
        u64::from(lo) ^ u64::from(hi),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn populate_emits_master_stack_and_layers() {
        let mut params = ParameterMap::new();
        populate_default_pcblib_parameters(&mut params, 0);
        assert!(params.contains_key("HEADER"));
        assert!(params.contains_key("V9_MASTERSTACK_STYLE"));
        assert!(params.contains_key("V9_STACK_LAYER0_NAME"));
        assert!(params.contains_key("V9_STACK_LAYER8_NAME")); // bottom paste
        assert_eq!(
            params.get("V9_STACK_LAYER3_NAME"),
            Some("Top Layer"),
            "layer 3 is Top Layer in 2-layer stack"
        );
        assert_eq!(params.get("V9_STACK_LAYER4_NAME"), Some("Dielectric 1"));
        assert_eq!(params.get("V9_STACK_LAYER5_NAME"), Some("Bottom Layer"));
        assert_eq!(
            params.get("V9_MASTERSTACK_NAME"),
            Some("Master layer stack")
        );
    }

    #[test]
    fn ensure_complete_only_fills_missing() {
        let mut user = BTreeMap::new();
        user.insert("HEADER".to_string(), "Custom".to_string());
        user.insert("KIND".to_string(), "MyKind".to_string());

        let mut out = ParameterMap::new();
        ensure_pcblib_parameters_complete(&user, 0, &mut out);

        // User-supplied values are preserved untouched.
        assert_eq!(out.get("HEADER"), Some("Custom"));
        assert_eq!(out.get("KIND"), Some("MyKind"));
        // Missing defaults still get filled in.
        assert!(out.contains_key("V9_MASTERSTACK_NAME"));
        assert!(out.contains_key("V9_STACK_LAYER0_NAME"));
    }

    #[test]
    fn date_format_is_dd_mm_yyyy() {
        let (date, _time) = current_date_time();
        let parts: Vec<&str> = date.split('/').collect();
        assert_eq!(parts.len(), 3, "date has three /-separated parts");
        assert_eq!(parts[0].len(), 2, "day is two digits");
        assert_eq!(parts[1].len(), 2, "month is two digits");
        assert_eq!(parts[2].len(), 4, "year is four digits");
    }

    #[test]
    fn days_to_ymd_known_values() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        assert_eq!(days_to_ymd(31), (1970, 2, 1));
        assert_eq!(days_to_ymd(365), (1971, 1, 1));
        // 56 years × 365 + 14 leap days = 20454 → 2026-01-01.
        assert_eq!(days_to_ymd(20454), (2026, 1, 1));
        assert_eq!(days_to_ymd(20467), (2026, 1, 14));
        // Leap-year sanity: 2024-02-29 is day 19782 (1970-01-01 + 19782).
        assert_eq!(days_to_ymd(19782), (2024, 2, 29));
    }
}
