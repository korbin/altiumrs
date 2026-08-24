//! 3D STEP model embedded in a PCB library or document.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// A 3D model (typically STEP) referenced by [`super::ComponentBody::model_id`].
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Model3d {
    /// GUID linking back to a `ComponentBody`.
    pub id: String,
    /// Original filename (e.g. `"FOO.step"`).
    pub name: String,
    /// Whether the model bytes are embedded in the library file.
    pub is_embedded: bool,
    /// Source kind (typically `"Undefined"` for embedded models).
    pub model_source: String,
    pub rotation_x: f64,
    pub rotation_y: f64,
    pub rotation_z: f64,
    pub dz: i32,
    /// Altium's proprietary checksum (preserved verbatim for round-trip).
    pub checksum: i32,
    /// STEP text (ISO-10303-21). Decompressed on read; compressed on write.
    pub step_data: String,
}

/// Parse a `Models/Data` stream: `[i32 len][param bytes]` records.
pub(crate) fn parse_models_data(data: &[u8]) -> Vec<BTreeMap<String, String>> {
    let mut metas = Vec::new();
    let mut off = 0usize;
    while off + 4 <= data.len() {
        let len = i32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        if len <= 0 {
            break;
        }
        let Some(buf) = data.get(off + 4..off + 4 + len as usize) else {
            break;
        };
        off += 4 + len as usize;
        let stripped: Vec<u8> = buf.iter().copied().filter(|&b| b != 0).collect();
        let s = crate::encoding::decode(&stripped);
        let mut meta = BTreeMap::new();
        for part in s.split('|').filter(|p| !p.is_empty()) {
            if let Some(eq) = part.find('=') {
                meta.insert(part[..eq].to_uppercase(), part[eq + 1..].to_owned());
            }
        }
        metas.push(meta);
    }
    metas
}

/// Build [`Model3d`]s from a parsed `Models/Data` plus a per-index stream
/// lookup returning the zlib-compressed model bytes.
pub(crate) fn build_models(
    metas: &[BTreeMap<String, String>],
    mut get_stream: impl FnMut(usize) -> Option<Vec<u8>>,
) -> Result<Vec<Model3d>> {
    let mut models = Vec::new();
    for i in 0.. {
        let Some(compressed) = get_stream(i) else {
            break;
        };
        let mut model = Model3d::default();
        if let Some(meta) = metas.get(i) {
            if let Some(v) = meta.get("ID") {
                model.id = v.clone();
            }
            if let Some(v) = meta.get("NAME") {
                model.name = v.clone();
            }
            if let Some(v) = meta.get("EMBED") {
                model.is_embedded = v.eq_ignore_ascii_case("TRUE");
            }
            if let Some(v) = meta.get("MODELSOURCE") {
                model.model_source = v.clone();
            }
            if let Some(v) = meta.get("ROTX").and_then(|x| x.parse().ok()) {
                model.rotation_x = v;
            }
            if let Some(v) = meta.get("ROTY").and_then(|x| x.parse().ok()) {
                model.rotation_y = v;
            }
            if let Some(v) = meta.get("ROTZ").and_then(|x| x.parse().ok()) {
                model.rotation_z = v;
            }
            if let Some(v) = meta.get("DZ").and_then(|x| x.parse().ok()) {
                model.dz = v;
            }
            if let Some(v) = meta.get("CHECKSUM").and_then(|x| x.parse().ok()) {
                model.checksum = v;
            }
        }
        if !compressed.is_empty() {
            let step = crate::sch::binary::zlib_decompress(&compressed)?;
            model.step_data = String::from_utf8_lossy(&step).into_owned();
        }
        models.push(model);
    }
    Ok(models)
}
