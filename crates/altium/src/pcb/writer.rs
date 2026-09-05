//! Writer for `.PcbLib` and `.PcbDoc` files.

use std::collections::BTreeMap;
use std::io::{Cursor, Seek, Write};
use std::path::Path;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use tokio::io::AsyncWrite;

use super::binary::{
    PrimitiveFlags, layer_byte_to_name, layer_name_to_byte, patch_common_prefix, patch_f64,
    patch_i32, patch_point, patch_u8, write_common_prefix_full, write_coord_point,
};
use super::component::Component;
use super::document::Document;
use super::library::Library;
use super::primitives::{Arc, ComponentBody, Fill, Pad, Region, Text, Track, Via};
use crate::binary::BinaryWriter;
use crate::compound::CompoundFile;
use crate::coord::Coord;
use crate::encoding;
use crate::error::Result;
use crate::parameter::ParameterMap;

// Library writer

impl Library {
    /// Serialise this library to a `.PcbLib` byte buffer.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut cf = CompoundFile::create()?;
        let mut section_keys: BTreeMap<String, String> = self.section_keys.clone();
        ensure_section_keys(self, &mut section_keys);

        write_file_header(&mut cf, self)?;
        write_section_keys_stream(&mut cf, self, &section_keys)?;
        write_library_storage(&mut cf, self, &section_keys)?;
        write_default_root_stubs(&mut cf, self)?;
        write_additional_streams(&mut cf, "", &self.additional_root_streams)?;
        cf.into_bytes()
    }

    /// Write to disk as a `.PcbLib`.
    pub async fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = self.to_bytes()?;
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    /// Write to any `AsyncWrite`.
    pub async fn write_async<W>(&self, mut writer: W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        use tokio::io::AsyncWriteExt;
        let bytes = self.to_bytes()?;
        writer.write_all(&bytes).await?;
        writer.flush().await?;
        Ok(())
    }
}

fn section_key_from_name(name: &str) -> String {
    let trimmed = if name.len() > 31 { &name[..31] } else { name };
    trimmed.replace('/', "_")
}

fn ensure_section_keys(library: &Library, section_keys: &mut BTreeMap<String, String>) {
    for component in &library.components {
        if section_keys.contains_key(&component.name) {
            continue;
        }
        let mangled = section_key_from_name(&component.name);
        if mangled != component.name {
            section_keys.insert(component.name.clone(), mangled);
        }
    }
}

fn write_file_header(cf: &mut CompoundFile, library: &Library) -> Result<()> {
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);

    let version_text = "PCB 6.0 Binary Library File";
    let version_bytes = encoding::encode(version_text);
    bw.write_i32(version_bytes.len() as i32)?;
    bw.write_u8(version_bytes.len() as u8)?;
    bw.write_bytes(&version_bytes)?;

    bw.write_f64(5.01)?;

    let unique_id = if library.unique_id.is_empty() {
        "AAAAAAAA"
    } else {
        library.unique_id.as_str()
    };
    let id_bytes = encoding::encode(unique_id);
    bw.write_i32(id_bytes.len() as i32)?;
    bw.write_u8(id_bytes.len() as u8)?;
    bw.write_bytes(&id_bytes)?;

    cf.write_stream("FileHeader", &buf.into_inner())?;
    Ok(())
}

fn write_section_keys_stream(
    cf: &mut CompoundFile,
    library: &Library,
    section_keys: &BTreeMap<String, String>,
) -> Result<()> {
    let mut needs: Vec<&Component> = library
        .components
        .iter()
        .filter(|c| section_keys.contains_key(&c.name))
        .collect();
    if needs.is_empty() {
        return Ok(());
    }
    // Altium's stream order is its own insertion order; replay the order the
    // file had for the names it covers, then anything new in component order.
    let rank = |name: &str| {
        library
            .section_key_order
            .iter()
            .position(|n| n == name)
            .unwrap_or(usize::MAX)
    };
    needs.sort_by_key(|c| rank(&c.name));

    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    bw.write_i32(needs.len() as i32)?;
    for component in needs {
        bw.write_pascal_string_block(&component.name)?;
        let key = section_keys
            .get(&component.name)
            .cloned()
            .unwrap_or_default();
        bw.write_pascal_string_block(&key)?;
    }
    cf.write_stream("SectionKeys", &buf.into_inner())?;
    Ok(())
}

fn write_library_storage(
    cf: &mut CompoundFile,
    library: &Library,
    section_keys: &BTreeMap<String, String>,
) -> Result<()> {
    cf.create_storage("Library")?;
    write_storage_header(cf, "Library/Header", 1)?;
    write_library_data(cf, library)?;
    for component in &library.components {
        let section_key = section_keys
            .get(&component.name)
            .cloned()
            .unwrap_or_else(|| component.name.clone());
        write_footprint(cf, component, &section_key)?;
    }
    write_library_models(cf, library)?;
    write_default_library_stubs(cf, library)?;
    // Drop a TOC carried in from an older export; it is regenerated above.
    let extra: BTreeMap<String, Vec<u8>> = library
        .additional_library_streams
        .iter()
        .filter(|(k, _)| !k.starts_with("ComponentParamsTOC/"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    write_additional_streams(cf, "Library", &extra)?;
    Ok(())
}

fn write_storage_header(cf: &mut CompoundFile, path: &str, record_count: i32) -> Result<()> {
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    bw.write_i32(record_count)?;
    cf.write_stream(path, &buf.into_inner())?;
    Ok(())
}

fn write_library_data(cf: &mut CompoundFile, library: &Library) -> Result<()> {
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);

    let mut params = ParameterMap::new();
    if let Some(stored) = &library.library_parameters {
        super::defaults::ensure_pcblib_parameters_complete(
            stored,
            library.components.len(),
            &mut params,
        );
    } else {
        super::defaults::populate_default_pcblib_parameters(&mut params, library.components.len());
        params.insert("WEIGHT", library.components.len().to_string());
    }
    write_c_string_param_block(&mut bw, &params)?;

    bw.write_u32(library.components.len() as u32)?;
    for component in &library.components {
        bw.write_pascal_string_block(&component.name)?;
    }

    cf.write_stream("Library/Data", &buf.into_inner())?;
    Ok(())
}

fn write_footprint(cf: &mut CompoundFile, component: &Component, section_key: &str) -> Result<()> {
    cf.create_storage(section_key)?;
    let primitive_count = component.pads.len()
        + component.tracks.len()
        + component.vias.len()
        + component.arcs.len()
        + component.texts.len()
        + component.fills.len()
        + component.regions.len()
        + component.component_bodies.len();
    write_storage_header(cf, &format!("{section_key}/Header"), primitive_count as i32)?;
    write_footprint_parameters(cf, component, section_key)?;
    write_footprint_wide_strings(cf, component, section_key)?;
    write_footprint_data(cf, component, section_key)?;
    write_default_footprint_stubs(cf, component, section_key)?;
    write_additional_streams(cf, section_key, &component.additional_streams)?;
    Ok(())
}

/// Per-footprint `PrimitiveGuids` + `UniqueIDPrimitiveInformation` stubs:
/// empty u32(0) header + zero-byte data.
fn write_default_footprint_stubs(
    cf: &mut CompoundFile,
    component: &Component,
    section_key: &str,
) -> Result<()> {
    // A footprint read from disk carries its own `PrimitiveGuids`; Altium does
    // not always give such a footprint a `UniqueIDPrimitiveInformation`
    // storage, so only a from-scratch footprint gets that stub.
    let from_disk = component
        .additional_streams
        .keys()
        .any(|k| k.starts_with("PrimitiveGuids/"));
    for sub in ["PrimitiveGuids", "UniqueIDPrimitiveInformation"] {
        if from_disk && sub == "UniqueIDPrimitiveInformation" {
            continue;
        }
        let header_path = format!("{section_key}/{sub}/Header");
        let data_path = format!("{section_key}/{sub}/Data");
        if !component
            .additional_streams
            .contains_key(&format!("{sub}/Header"))
        {
            cf.write_stream(&header_path, &0u32.to_le_bytes())?;
        }
        if !component
            .additional_streams
            .contains_key(&format!("{sub}/Data"))
        {
            cf.write_stream(&data_path, &[])?;
        }
    }
    Ok(())
}

fn write_footprint_parameters(
    cf: &mut CompoundFile,
    component: &Component,
    section_key: &str,
) -> Result<()> {
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);

    let mut params = ParameterMap::new();
    params.insert("PATTERN", component.name.clone());
    // Altium writes HEIGHT as a mil string ("31.4961mil"); a bare integer
    // is not read back as internal units.
    params.insert(
        "HEIGHT",
        super::doc_codec::format_mil_coord(component.height),
    );
    if let Some(d) = &component.description {
        params.insert("DESCRIPTION", d.clone());
    }
    // Altium's key order: PATTERN, HEIGHT, DESCRIPTION, GRIDSNGUIDE, ITEMGUID,
    // REVISIONGUID, COMPONENTKIND, AREA, then anything else.
    let extra = &component.additional_parameters;
    if let Some(v) = extra.get("GRIDSNGUIDE") {
        params.insert("GRIDSNGUIDE", v.clone());
    }
    if let Some(g) = &component.item_guid {
        params.insert("ITEMGUID", g.clone());
    }
    if let Some(g) = &component.item_revision_guid {
        params.insert("REVISIONGUID", g.clone());
    }
    for key in ["COMPONENTKIND", "AREA"] {
        if let Some(v) = extra.get(key) {
            params.insert(key, v.clone());
        }
    }
    for (k, v) in extra {
        if !params.contains_key(k) {
            params.insert(k, v.clone());
        }
    }
    write_c_string_param_block(&mut bw, &params)?;

    cf.write_stream(format!("{section_key}/Parameters"), &buf.into_inner())?;
    Ok(())
}

fn write_footprint_wide_strings(
    cf: &mut CompoundFile,
    component: &Component,
    section_key: &str,
) -> Result<()> {
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    let mut params = ParameterMap::new();
    for (i, text) in component.texts.iter().enumerate() {
        let encoded = text
            .text
            .chars()
            .map(|c| (c as u32).to_string())
            .collect::<Vec<_>>()
            .join(",");
        params.insert(format!("ENCODEDTEXT{i}").as_str(), encoded);
    }
    write_c_string_param_block(&mut bw, &params)?;
    cf.write_stream(format!("{section_key}/WideStrings"), &buf.into_inner())?;
    Ok(())
}

fn write_footprint_data(
    cf: &mut CompoundFile,
    component: &Component,
    section_key: &str,
) -> Result<()> {
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);

    bw.write_pascal_string_block(&component.name)?;

    let counts = [
        component.arcs.len(),
        component.pads.len(),
        component.vias.len(),
        component.tracks.len(),
        component.texts.len(),
        component.fills.len(),
        component.regions.len(),
        component.component_bodies.len(),
    ];
    let ids = [1u8, 2, 3, 4, 5, 6, 11, 12];
    // Replay the source file's interleaving when it is still consistent with
    // the primitive lists; otherwise fall back to grouped-by-kind order.
    let order: Vec<u8> = if !component.primitive_order.is_empty()
        && ids.iter().zip(counts).all(|(id, n)| {
            component
                .primitive_order
                .iter()
                .filter(|x| *x == id)
                .count()
                == n
        })
        && component.primitive_order.iter().all(|x| ids.contains(x))
    {
        component.primitive_order.clone()
    } else {
        ids.iter()
            .zip(counts)
            .flat_map(|(id, n)| std::iter::repeat(*id).take(n))
            .collect()
    };
    let mut next = [0usize; 8];
    for id in order {
        let slot = ids.iter().position(|x| *x == id).unwrap();
        let i = next[slot];
        next[slot] += 1;
        bw.write_u8(id)?;
        match id {
            1 => write_arc(&mut bw, &component.arcs[i])?,
            2 => write_pad(&mut bw, &component.pads[i])?,
            3 => write_via(&mut bw, &component.vias[i])?,
            4 => write_track(&mut bw, &component.tracks[i])?,
            5 => write_text(&mut bw, &component.texts[i], i as i32)?,
            6 => write_fill(&mut bw, &component.fills[i])?,
            11 => write_region(&mut bw, &component.regions[i])?,
            _ => write_component_body(&mut bw, &component.component_bodies[i])?,
        }
    }

    cf.write_stream(format!("{section_key}/Data"), &buf.into_inner())?;
    Ok(())
}

fn write_library_models(cf: &mut CompoundFile, library: &Library) -> Result<()> {
    cf.create_storage("Library/Models")?;
    write_storage_header(cf, "Library/Models/Header", library.models.len() as i32)?;

    let mut data = Vec::<u8>::new();
    for model in &library.models {
        // Altium puts an unnamed model's empty NAME first instead of last.
        let param_str = format!(
            "{}EMBED={}|MODELSOURCE={}|ID={}|ROTX={:.3}|ROTY={:.3}|ROTZ={:.3}|DZ={}|CHECKSUM={}{}",
            if model.name.is_empty() { "|NAME=|" } else { "" },
            if model.is_embedded { "TRUE" } else { "FALSE" },
            model.model_source,
            format_model_id(&model.id),
            model.rotation_x,
            model.rotation_y,
            model.rotation_z,
            model.dz,
            model.checksum,
            if model.name.is_empty() {
                String::new()
            } else {
                format!("|NAME={}", model.name)
            },
        );
        let mut bytes = encoding::encode(&param_str);
        bytes.push(0);
        let len = bytes.len() as i32;
        data.extend_from_slice(&len.to_le_bytes());
        data.extend_from_slice(&bytes);
    }
    cf.write_stream("Library/Models/Data", &data)?;

    for (i, model) in library.models.iter().enumerate() {
        let payload = if model.step_data.is_empty() {
            Vec::new()
        } else {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            if model.step_data_is_latin1 {
                // One char per original byte (see `Model3d::step_data_is_latin1`).
                let bytes: Vec<u8> = model.step_data.chars().map(|c| c as u8).collect();
                encoder.write_all(&bytes)?;
            } else {
                encoder.write_all(model.step_data.as_bytes())?;
            }
            encoder.finish()?
        };
        cf.write_stream(format!("Library/Models/{i}"), &payload)?;
    }
    Ok(())
}

fn write_additional_streams(
    cf: &mut CompoundFile,
    base: &str,
    streams: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    for (key, data) in streams {
        let path = if base.is_empty() {
            key.clone()
        } else {
            format!("{base}/{key}")
        };
        cf.write_stream(path, data)?;
    }
    Ok(())
}

fn write_default_root_stubs(cf: &mut CompoundFile, library: &Library) -> Result<()> {
    if !contains_root_stream(library, "FileVersionInfo/Header") {
        cf.write_stream("FileVersionInfo/Header", &u32_le_bytes(1))?;
    }
    if !contains_root_stream(library, "FileVersionInfo/Data") {
        cf.write_stream("FileVersionInfo/Data", &default_file_version_info_data())?;
    }
    Ok(())
}

fn write_default_library_stubs(cf: &mut CompoundFile, library: &Library) -> Result<()> {
    if !contains_library_stream(library, "EmbeddedFonts") {
        cf.write_stream("Library/EmbeddedFonts", &u32_le_bytes(0))?;
    }
    if !contains_library_stream(library, "LayerKindMapping/Header") {
        cf.write_stream("Library/LayerKindMapping/Header", &u32_le_bytes(1))?;
    }
    if !contains_library_stream(library, "LayerKindMapping/Data") {
        cf.write_stream(
            "Library/LayerKindMapping/Data",
            &default_layer_kind_mapping_data(),
        )?;
    }
    if !contains_library_stream(library, "ModelsNoEmbed/Header") {
        cf.write_stream("Library/ModelsNoEmbed/Header", &u32_le_bytes(0))?;
    }
    if !contains_library_stream(library, "ModelsNoEmbed/Data") {
        cf.write_stream("Library/ModelsNoEmbed/Data", &[])?;
    }
    if !contains_library_stream(library, "PadViaLibrary/Header") {
        cf.write_stream("Library/PadViaLibrary/Header", &u32_le_bytes(0))?;
    }
    if !contains_library_stream(library, "PadViaLibrary/Data") {
        cf.write_stream(
            "Library/PadViaLibrary/Data",
            &default_pad_via_library_data(),
        )?;
    }
    if !contains_library_stream(library, "Textures/Header") {
        cf.write_stream("Library/Textures/Header", &u32_le_bytes(0))?;
    }
    if !contains_library_stream(library, "Textures/Data") {
        cf.write_stream("Library/Textures/Data", &[])?;
    }
    // Always derived from the components (a carried copy would go stale as
    // soon as a footprint is added, removed or renamed). Altium's header is
    // the record count, which is 1: the whole table is one block.
    cf.write_stream("Library/ComponentParamsTOC/Header", &u32_le_bytes(1))?;
    cf.write_stream(
        "Library/ComponentParamsTOC/Data",
        &default_component_params_toc_data(library),
    )?;
    Ok(())
}

/// `Library/ComponentParamsTOC/Data` exactly as Altium writes it: one
/// length-prefixed block holding a `Name=|Pad Count=|Height=|Description=`
/// line per footprint (CRLF-terminated, height in mils with trailing zeros
/// trimmed and no unit) followed by a single NUL.
fn default_component_params_toc_data(library: &Library) -> Vec<u8> {
    let mut text = Vec::<u8>::new();
    for component in &library.components {
        let name = component.name.replace('|', "/");
        let pad_count = component.pads.len();
        let height = super::doc_codec::format_mil_coord(component.height);
        let height = height.strip_suffix("mil").unwrap_or(&height);
        let description = component
            .description
            .clone()
            .unwrap_or_default()
            .replace('|', "/");
        let line = format!(
            "Name={name}|Pad Count={pad_count}|Height={height}|Description={description}\r\n",
        );
        text.extend_from_slice(&encoding::encode(&line));
    }
    text.push(0);
    let mut data = Vec::<u8>::new();
    data.extend_from_slice(&(text.len() as u32).to_le_bytes());
    data.extend_from_slice(&text);
    data
}

fn contains_root_stream(library: &Library, key: &str) -> bool {
    library.additional_root_streams.contains_key(key)
}

fn contains_library_stream(library: &Library, key: &str) -> bool {
    library.additional_library_streams.contains_key(key)
}

fn u32_le_bytes(n: u32) -> [u8; 4] {
    n.to_le_bytes()
}

fn default_layer_kind_mapping_data() -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&[0x31, 0x00, 0x2E, 0x00, 0x30, 0x00, 0x00, 0x00]);
    out.extend_from_slice(&[0u8; 8]);
    out
}

fn default_pad_via_library_data() -> Vec<u8> {
    let body = b"|PADVIALIBRARY.LIBRARYID={4F499044-D65D-4DA6-9273-7355F5C6609C}\
|PADVIALIBRARY.LIBRARYNAME=<Local>\
|PADVIALIBRARY.DISPLAYUNITS=1\0";
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    out
}

fn default_file_version_info_data() -> Vec<u8> {
    // Migration-history list ending at Release 20.0. An outdated terminal
    // entry triggers Altium's "File in Newer Format" report on every open.
    const ENTRIES: &[(&str, &str, &str)] = &[
        (
            "6.3",
            "<b>CAUTION</b> - Via connections to both hatched and solid signal layer polygons are now controlled by the polygon connect style rule. Re-pouring polygons may result in physical copper differences.",
            "",
        ),
        (
            "6.6",
            "",
            "<b>CAUTION</b> - File contains Rounded Rectangular pads not supported by this version of the software. These pads have been converted to the Round shape.",
        ),
        (
            "6.8",
            "",
            "<b>CAUTION</b> - File contains one or more Solid Regions containing boundary arcs. These arcs have been converted to linear segments that approximate the arc.",
        ),
        (
            "6.8",
            "",
            "<b>CAUTION</b> - File contains one or more Component Bodies containing boundary arcs. These arcs have been converted to linear segments that approximate the arc.",
        ),
        (
            "6.8",
            "",
            "<b>CAUTION</b> - File contains one or more Matched Length Rules. Rule atributes have been changed. Rule does not support pattern related attributes (amplitude, gap) anymore they are treated as tool attibutes instead.Rule is enhanced with subscoping attributes - allowing checking between nets in the same differential pair, between differential pairs as well as other electrical objects",
        ),
        (
            "6.8",
            "",
            "<b>CAUTION</b> - Board cutout objects introduced. Be aware that if your design contains board cutouts, they cannot be read in previous versions.",
        ),
        (
            "6.8",
            "",
            "<b>CAUTION</b> - New type of text - barcode text was introduced. Be aware that if your design contains barcodes they cannot be read in previous versions.",
        ),
        (
            "6.8",
            "",
            "<b>CAUTION</b> - Polygon/Layer dependent connect style rule for pads and vias was introduced. First scope should define pads/vias while 2nd scope should define polygons.Second scope is not readable in versions prior to 6.8 and is assumed to be 'All'.",
        ),
        (
            "6.9",
            "",
            "<b>CAUTION</b> - File contains one or more Component Bodies containing embedded STEP models. These models have be discarded.",
        ),
        (
            "6.9",
            "",
            "<b>CAUTION</b> - File contains one or more Components with pads with Pad Jumper IDs. The pads Pad Jumper ID fields have been discarded.",
        ),
        (
            "7.0",
            "",
            "<b>CAUTION</b> - File may contain Component Bodies with linked STEP models. These models will be discarded.",
        ),
        (
            "Winter 09",
            "",
            "<b>CAUTION</b> - Vias support varying diameters across layerstack. If this feature is used in design, extra values will be discarded.",
        ),
        (
            "Winter 09",
            "",
            "<b>CAUTION</b> - File may contain pads with hole offsets. Hole offset information will be discarded.",
        ),
        (
            "Winter 09",
            "",
            "<b>CAUTION</b> - File contains new manufacturing rules. Hole To Hole clearance, Minimum solder mask sliver, Silkscreen Over Exposed Copper and Silkscreen To Silkscreen Clearance rules were introduced in Altium Designer Winter 09.These rules will be discarded.",
        ),
        (
            "Winter 09",
            "",
            "<b>CAUTION</b> - 3D models now support texturing.If used in design these textures will be discarded.",
        ),
        (
            "Summer 09",
            "<b>CAUTION</b> - File contains old violation objects. These violations are no longer supported & will not be loaded. Please run DRC after opening this file in order to refresh the violations.",
            "<b>CAUTION</b> - File contains new custom violations that replaced the old violation objects. These violations were introduced in Altium Designer Summer 09. The new custom violations will be discarded.",
        ),
        (
            "Summer 09",
            "",
            "<b>CAUTION</b> - Support was added for 32 Mechanical Layers. Objects on mechanical layers beyond 16 have been moved to Mechanical Layer 16.",
        ),
        (
            "Summer 09",
            "<b>CAUTION</b> - Existing testpoint rules and settings are used as fabrication testpoint information.",
            "<b>CAUTION</b> - File contains assembly testpoint rules and/or settings.  Assembly testpoint information will be discarded.",
        ),
        (
            "Release 10",
            "",
            "<b>CAUTION</b> - New Custom Grids and Guides were introduced. Be aware that your design might contain Custom Grids and Guides that cannot be read in previous versions. ",
        ),
        (
            "Release 10",
            "",
            "<b>CAUTION</b> - New Structured Clusters were introduced. Be aware that your design might contain Structured Clusters that cannot be read in previous versions. ",
        ),
        (
            "Release 10",
            "",
            "<b>CAUTION</b> - New PCB 3D Movie Manager was introduced. Be aware that your design might contain 3D PCB movie that cannot be read in previous versions. ",
        ),
        (
            "Release 10 update 1",
            "",
            "<b>CAUTION</b> - New Clearance Rule subscopes targeting differential pairs  were introduced. Be aware that your design might contain Clearance Rules using those subscopes that cannot be read in previous versions. ",
        ),
        (
            "Release 10 update 15",
            "",
            "<b>CAUTION</b> - Support of Solder Mask and Paste Mask expansions for Tracks, Arcs, Fills and Regions was introduced. Be aware that your design might contain Solder Mask and Paste Mask expansions for these types of primitives that cannot be read in the version of Altium Designer you are currently using. ",
        ),
        (
            "Release 12",
            "<b>CAUTION</b> - Air Gap Width previously controlled by Clearance rule is now controlled by Polygon Connect Style rule's newly introduced Air Gap Width (set to default value). Suggest reviewing each Polygon Connect Style rule's Air Gap Width attribute for correctness.",
            "<b>CAUTION</b> - Air Gap Width previously controlled by Clearance rule is now controlled by Polygon Connect Style rule's newly introduced Air Gap Width (set to default value). Suggest reviewing each Polygon Connect Style rule's Air Gap Width attribute for correctness.",
        ),
        (
            "Release 13",
            "<b>CAUTION</b> - Silkscreen Over Component Pads Rules are converted to Silk To Solder Mask Clearance Rules. Suggest examining rule scopes for accuracy.",
            "<b>CAUTION</b> - Silk To Solder Mask Clearance Rules are converted to Silkscreen Over Component Pads Rules.",
        ),
        (
            "Release 14",
            "",
            "<b>CAUTION</b> - The Differential Pairs Routing rule added support for control of the width. Be aware that these widths must be manually entered as Width rules in this version.",
        ),
        (
            "Release 15",
            "",
            "<b>CAUTION</b> - Support of separate solder masks for top & bottom of pads added.",
        ),
        (
            "Release 15.1",
            "",
            "<b>CAUTION</b> - Support Multi-line PCB Text added.",
        ),
        (
            "Release 16.0",
            "",
            "<b>CAUTION</b> - Pad/Via hole size tolerance value added.",
        ),
        (
            "Release 17.0",
            "",
            "<b>CAUTION</b> - Component parameters added.",
        ),
        (
            "Release 17.0",
            "",
            "<b>CAUTION</b> - Support of backdrilling",
        ),
        (
            "Release 17.1",
            "",
            "<b>CAUTION</b> - Support of waived violations",
        ),
        (
            "Release 17.1",
            "",
            "<b>CAUTION</b> - Support of object specific keepouts",
        ),
        ("Release 20.0", "", ""),
    ];
    fn codepoints(s: &str) -> String {
        s.chars()
            .map(|c| (c as u32).to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
    let mut body = String::new();
    body.push_str(&format!("|COUNT={}", ENTRIES.len()));
    for (i, (ver, fwd, bk)) in ENTRIES.iter().enumerate() {
        body.push_str(&format!("|VER{i}={}", codepoints(ver)));
        body.push_str(&format!("|FWDMSG{i}={}", codepoints(fwd)));
        body.push_str(&format!("|BKMSG{i}={}", codepoints(bk)));
    }
    let mut bytes = body.into_bytes();
    bytes.push(0);
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&bytes);
    out
}

fn write_c_string_param_block<W: Write + Seek>(
    bw: &mut BinaryWriter<W>,
    params: &ParameterMap,
) -> Result<()> {
    bw.write_block(|w| {
        let mut bytes = Vec::<u8>::new();
        crate::parameter::write_block_bytes(&mut bytes, params, '|');
        w.write_bytes(&bytes)?;
        w.write_u8(0)?;
        Ok(())
    })
}

/// As [`write_c_string_param_block`] but without the leading separator —
/// the form Altium uses for region and component-body parameter strings.
fn write_c_string_param_block_bare<W: Write + Seek>(
    bw: &mut BinaryWriter<W>,
    params: &ParameterMap,
) -> Result<()> {
    bw.write_block(|w| {
        let mut bytes = Vec::<u8>::new();
        crate::parameter::write_block_bytes(&mut bytes, params, '|');
        let start = usize::from(bytes.first() == Some(&b'|'));
        w.write_bytes(&bytes[start..])?;
        w.write_u8(0)?;
        Ok(())
    })
}

/// Component-body parameter block. Altium writes `ARCRESOLUTION` twice: once
/// after `UNIONINDEX` and again right after `BODYPROJECTION`; a map cannot hold
/// the duplicate, so it is spliced in here.
fn write_body_param_block<W: Write + Seek>(
    bw: &mut BinaryWriter<W>,
    params: &ParameterMap,
) -> Result<()> {
    bw.write_block(|w| {
        let mut bytes = Vec::<u8>::new();
        crate::parameter::write_block_bytes(&mut bytes, params, '|');
        let start = usize::from(bytes.first() == Some(&b'|'));
        let mut body = bytes[start..].to_vec();
        if let Some(res) = params.get("ARCRESOLUTION") {
            let key = b"|BODYPROJECTION=";
            if let Some(pos) = body.windows(key.len()).position(|w| w == key) {
                let end = body[pos + 1..]
                    .iter()
                    .position(|&b| b == b'|')
                    .map_or(body.len(), |i| pos + 1 + i);
                let dup = format!("|ARCRESOLUTION={res}");
                body.splice(end..end, dup.bytes());
            }
        }
        w.write_bytes(&body)?;
        w.write_u8(0)?;
        Ok(())
    })
}

// Primitive writers

fn write_arc<W: Write + Seek>(bw: &mut BinaryWriter<W>, arc: &Arc) -> Result<()> {
    if let Some(raw) = &arc.raw_record {
        let mut b = raw.clone();
        patch_common_prefix(
            &mut b,
            arc.layer as u8,
            PrimitiveFlags {
                is_locked: arc.is_locked,
                is_tenting_top: arc.is_tenting_top,
                is_tenting_bottom: arc.is_tenting_bottom,
                is_keepout: arc.is_keepout,
                extra: arc.flags_extra,
                is_polygon_outline: arc.is_polygon_outline,
                ..PrimitiveFlags::default()
            },
            arc.net_index as i32,
            arc.component_index,
        );
        patch_point(&mut b, 13, arc.center);
        patch_i32(&mut b, 21, arc.radius.to_raw());
        patch_f64(&mut b, 25, arc.start_angle);
        patch_f64(&mut b, 33, arc.end_angle);
        patch_i32(&mut b, 41, arc.width.to_raw());
        return bw.write_block(|w| {
            w.write_bytes(&b)?;
            Ok(())
        });
    }
    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: arc.is_locked,
            is_tenting_top: arc.is_tenting_top,
            is_tenting_bottom: arc.is_tenting_bottom,
            is_keepout: arc.is_keepout,
            extra: arc.flags_extra,
            is_polygon_outline: arc.is_polygon_outline,
            ..PrimitiveFlags::default()
        }
        .encode();
        write_common_prefix_full(
            w,
            arc.layer as u8,
            flags,
            arc.net_index as i32,
            arc.component_index,
        )?;
        write_coord_point(w, arc.center)?;
        w.write_i32(arc.radius.to_raw())?;
        w.write_f64(arc.start_angle)?;
        w.write_f64(arc.end_angle)?;
        w.write_i32(arc.width.to_raw())?;
        // Altium's 60-byte arc record: sub-polygon index, pad byte, union
        // index, V7 layer id, reserved word.
        w.write_u16(0)?;
        w.write_u8(0)?;
        w.write_u32(0)?;
        w.write_u32(v7_layer_id(arc.layer))?;
        w.write_u32(0)?;
        Ok(())
    })
}

fn write_pad<W: Write + Seek>(bw: &mut BinaryWriter<W>, pad: &Pad) -> Result<()> {
    bw.write_pascal_string_block(pad.designator.as_deref().unwrap_or(""))?;
    bw.write_block(|w| {
        w.write_bytes(&pad.reserved_block_after_designator)?;
        Ok(())
    })?;
    bw.write_pascal_string_block(&pad.net_string_block)?;
    bw.write_block(|w| {
        w.write_bytes(&pad.reserved_block_after_net_string)?;
        Ok(())
    })?;

    // Raw path: patch the mutable fields into the original main block.
    if let Some(raw) = &pad.raw_record {
        let mut b = raw.clone();
        patch_common_prefix(
            &mut b,
            pad.layer as u8,
            PrimitiveFlags {
                is_locked: pad.is_locked,
                is_tenting_top: pad.is_tenting_top,
                is_tenting_bottom: pad.is_tenting_bottom,
                is_keepout: pad.is_keepout,
                extra: pad.flags_extra,
                is_testpoint_fab_top: pad.is_testpoint_fab_top,
                is_testpoint_fab_bottom: pad.is_testpoint_fab_bottom,
                ..PrimitiveFlags::default()
            },
            pad.net_index as i32,
            pad.component_index,
        );
        patch_point(&mut b, 13, pad.location);
        patch_f64(&mut b, 52, pad.rotation);
        bw.write_block(|w| {
            w.write_bytes(&b)?;
            Ok(())
        })?;
        // Mirror the original size/shape block exactly — including the
        // empty-block case (header only), which some pads carry.
        return match &pad.raw_size_shape {
            Some(ss) => bw.write_block(|w| {
                w.write_bytes(ss)?;
                Ok(())
            }),
            None => bw.write_block(|_| Ok(())),
        };
    }

    // Pad main block is 202 bytes; the trailing bytes after our typed
    // fields are zero-padded. Writing a shorter block desyncs subsequent
    // records in Pads6.
    const PAD_MAIN_BLOCK_TOTAL: usize = 202;
    bw.write_block(|w| {
        let start = w.position()?;
        let flags = PrimitiveFlags {
            is_locked: pad.is_locked,
            is_tenting_top: pad.is_tenting_top,
            is_tenting_bottom: pad.is_tenting_bottom,
            is_keepout: pad.is_keepout,
            extra: pad.flags_extra,
            is_testpoint_fab_top: pad.is_testpoint_fab_top,
            is_testpoint_fab_bottom: pad.is_testpoint_fab_bottom,
            ..PrimitiveFlags::default()
        }
        .encode();
        write_common_prefix_full(
            w,
            pad.layer as u8,
            flags,
            pad.net_index as i32,
            pad.component_index,
        )?;
        write_coord_point(w, pad.location)?;
        write_coord_point(w, pad.size_top)?;
        write_coord_point(w, pad.size_middle)?;
        write_coord_point(w, pad.size_bottom)?;
        w.write_i32(pad.hole_size.to_raw())?;
        w.write_u8(i32::from(pad.shape_top) as u8)?;
        w.write_u8(i32::from(pad.shape_middle) as u8)?;
        w.write_u8(i32::from(pad.shape_bottom) as u8)?;
        w.write_f64(pad.rotation)?;
        w.write_u8(if pad.is_plated { 1 } else { 0 })?;
        w.write_u8(0)?;
        w.write_u8(pad.mode as u8)?;
        w.write_u8(pad.power_plane_connect_style as u8)?;
        w.write_i32(pad.relief_air_gap.to_raw())?;
        w.write_i32(pad.relief_conductor_width.to_raw())?;
        w.write_i16(pad.relief_entries as i16)?;
        w.write_i32(pad.power_plane_clearance.to_raw())?;
        w.write_i32(pad.power_plane_relief_expansion.to_raw())?;
        w.write_i32(0)?;
        w.write_i32(pad.paste_mask_expansion.to_raw())?;
        w.write_i32(pad.solder_mask_expansion.to_raw())?;
        w.write_fill(0, 7)?;
        w.write_u8(if pad.paste_mask_expansion.to_raw() != 0 {
            2
        } else {
            0
        })?;
        w.write_u8(if pad.solder_mask_expansion.to_raw() != 0 {
            2
        } else {
            0
        })?;
        w.write_u8(pad.drill_type as u8)?;
        w.write_i16(0)?;
        w.write_i32(0)?;
        w.write_i16(pad.jumper_id as i16)?;
        w.write_i16(0)?;
        let written = (w.position()? - start) as usize;
        if written < PAD_MAIN_BLOCK_TOTAL {
            w.write_fill(0, PAD_MAIN_BLOCK_TOTAL - written)?;
        }
        Ok(())
    })?;

    write_pad_size_shape_block(bw, pad)
}

/// The 596-byte size/shape block — Altium reads it on every pad
/// regardless of `mode`, and omitting it desyncs Pads6.
fn write_pad_size_shape_block<W: Write + Seek>(bw: &mut BinaryWriter<W>, pad: &Pad) -> Result<()> {
    bw.write_block(|w| {
        for v in &pad.layer_x_sizes {
            w.write_i32(*v)?;
        }
        for v in &pad.layer_y_sizes {
            w.write_i32(*v)?;
        }
        for v in &pad.internal_layer_shapes {
            w.write_u8(*v)?;
        }
        w.write_u8(0)?;
        w.write_u8(i32::from(pad.hole_type) as u8)?;
        w.write_i32(pad.hole_slot_length)?;
        w.write_f64(pad.hole_rotation)?;
        for v in &pad.offset_x_from_hole_center {
            w.write_i32(*v)?;
        }
        for v in &pad.offset_y_from_hole_center {
            w.write_i32(*v)?;
        }
        w.write_u8(pad.has_rounded_rect_byte)?;
        for v in &pad.per_layer_shapes {
            w.write_u8(*v)?;
        }
        for v in &pad.per_layer_corner_radii {
            w.write_u8(*v)?;
        }
        Ok(())
    })?;
    Ok(())
}

fn write_via<W: Write + Seek>(bw: &mut BinaryWriter<W>, via: &Via) -> Result<()> {
    if let Some(raw) = &via.raw_record {
        let mut b = raw.clone();
        patch_common_prefix(
            &mut b,
            via.layer as u8,
            PrimitiveFlags {
                is_locked: via.is_locked,
                is_tenting_top: via.is_tenting_top,
                is_tenting_bottom: via.is_tenting_bottom,
                is_keepout: via.is_keepout,
                extra: via.flags_extra,
                is_testpoint_fab_top: via.is_testpoint_fab_top,
                is_testpoint_fab_bottom: via.is_testpoint_fab_bottom,
                ..PrimitiveFlags::default()
            },
            via.net_index as i32,
            via.component_index,
        );
        patch_point(&mut b, 13, via.location);
        patch_i32(&mut b, 21, via.diameter.to_raw());
        patch_i32(&mut b, 25, via.hole_size.to_raw());
        patch_u8(&mut b, 29, via.start_layer as u8);
        patch_u8(&mut b, 30, via.end_layer as u8);
        return bw.write_block(|w| {
            w.write_bytes(&b)?;
            Ok(())
        });
    }
    // Via record block is 360 bytes; trailing bytes after our typed
    // fields are zero-padded. A shorter block desyncs Vias6.
    const VIA_BLOCK_TOTAL: usize = 360;
    bw.write_block(|w| {
        let start = w.position()?;
        let flags = PrimitiveFlags {
            is_locked: via.is_locked,
            is_tenting_top: via.is_tenting_top,
            is_tenting_bottom: via.is_tenting_bottom,
            is_keepout: via.is_keepout,
            extra: via.flags_extra,
            is_testpoint_fab_top: via.is_testpoint_fab_top,
            is_testpoint_fab_bottom: via.is_testpoint_fab_bottom,
            ..PrimitiveFlags::default()
        }
        .encode();
        write_common_prefix_full(
            w,
            via.layer as u8,
            flags,
            via.net_index as i32,
            via.component_index,
        )?;
        write_coord_point(w, via.location)?;
        w.write_i32(via.diameter.to_raw())?;
        w.write_i32(via.hole_size.to_raw())?;
        w.write_u8(via.start_layer as u8)?;
        w.write_u8(via.end_layer as u8)?;
        w.write_u8(0)?;
        w.write_i32(via.thermal_relief_air_gap.to_raw())?;
        w.write_u8(via.thermal_relief_conductors as u8)?;
        w.write_u8(0)?;
        w.write_i32(via.thermal_relief_conductors_width.to_raw())?;
        w.write_i32(via.power_plane_clearance.to_raw())?;
        w.write_i32(via.power_plane_relief_expansion.to_raw())?;
        w.write_i32(0)?;
        w.write_i32(via.solder_mask_expansion.to_raw())?;
        w.write_bytes(&via.reserved_block_8)?;
        w.write_u8(if via.solder_mask_expansion_manual {
            2
        } else {
            0
        })?;
        w.write_u8(via.reserved_byte_after_mask_flag)?;
        w.write_i16(0)?;
        w.write_i32(0)?;
        w.write_u8(via.mode as u8)?;
        for v in &via.diameters {
            w.write_i32(v.to_raw())?;
        }
        w.write_i16(via.trailing_reserved_i16)?;
        w.write_i32(via.trailing_reserved_i32)?;
        let written = (w.position()? - start) as usize;
        if written < VIA_BLOCK_TOTAL {
            w.write_fill(0, VIA_BLOCK_TOTAL - written)?;
        }
        Ok(())
    })
}

fn write_track<W: Write + Seek>(bw: &mut BinaryWriter<W>, track: &Track) -> Result<()> {
    if let Some(raw) = &track.raw_record {
        let mut b = raw.clone();
        patch_common_prefix(
            &mut b,
            track.layer as u8,
            PrimitiveFlags {
                is_locked: track.is_locked,
                is_tenting_top: track.is_tenting_top,
                is_tenting_bottom: track.is_tenting_bottom,
                is_keepout: track.is_keepout,
                extra: track.flags_extra,
                is_polygon_outline: track.is_polygon_outline,
                ..PrimitiveFlags::default()
            },
            track.net_index as i32,
            track.component_index,
        );
        patch_point(&mut b, 13, track.start);
        patch_point(&mut b, 21, track.end);
        patch_i32(&mut b, 29, track.width.to_raw());
        return bw.write_block(|w| {
            w.write_bytes(&b)?;
            Ok(())
        });
    }
    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: track.is_locked,
            is_tenting_top: track.is_tenting_top,
            is_tenting_bottom: track.is_tenting_bottom,
            is_keepout: track.is_keepout,
            extra: track.flags_extra,
            is_polygon_outline: track.is_polygon_outline,
            ..PrimitiveFlags::default()
        }
        .encode();
        write_common_prefix_full(
            w,
            track.layer as u8,
            flags,
            track.net_index as i32,
            track.component_index,
        )?;
        write_coord_point(w, track.start)?;
        write_coord_point(w, track.end)?;
        w.write_i32(track.width.to_raw())?;
        // Altium's 49-byte track record: sub-polygon index (0 in libraries),
        // a pad byte, the union index, another pad byte, the V7 layer id and
        // a reserved word. KiCad's importer reads the first three
        // unconditionally.
        w.write_u16(0)?;
        w.write_u8(0)?;
        w.write_u32(0)?;
        w.write_u8(0)?;
        w.write_u32(v7_layer_id(track.layer))?;
        w.write_u32(0)?;
        Ok(())
    })
}

fn write_text<W: Write + Seek>(
    bw: &mut BinaryWriter<W>,
    text: &Text,
    wide_string_index: i32,
) -> Result<()> {
    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: text.is_locked,
            is_tenting_top: text.is_tenting_top,
            is_tenting_bottom: text.is_tenting_bottom,
            is_keepout: text.is_keepout,
            extra: text.flags_extra,
            ..PrimitiveFlags::default()
        }
        .encode();
        write_common_prefix_full(
            w,
            text.layer as u8,
            flags,
            text.net_index as i32,
            text.component_index,
        )?;
        write_coord_point(w, text.location)?;
        w.write_i32(text.height.to_raw())?;
        w.write_i16(i32::from(text.stroke_font) as i16)?;
        w.write_f64(text.rotation)?;
        w.write_u8(if text.is_mirrored { 1 } else { 0 })?;
        w.write_i32(text.stroke_width.to_raw())?;

        // Offsets 40..252 follow Altium's canonical 252-byte text record
        // (the same layout KiCad's ATEXT6 importer reads). Altium only
        // honours the justification byte at 132 when the
        // `justification_valid` flag at 240 is set.
        w.write_u8(if text.is_comment { 1 } else { 0 })?; // 40
        w.write_u8(if text.is_designator { 1 } else { 0 })?; // 41
        w.write_u8(text.char_set as u8)?; // 42
        w.write_u8(i32::from(text.text_kind) as u8)?; // 43 base font type
        w.write_u8(if text.font_bold { 1 } else { 0 })?; // 44
        w.write_u8(if text.font_italic { 1 } else { 0 })?; // 45
        w.write_font_name(text.font_name.as_deref().unwrap_or("Arial"))?; // 46..110
        w.write_u8(if text.is_inverted { 1 } else { 0 })?; // 110
        w.write_i32(text.inverted_border.to_raw())?; // 111
        w.write_i32(wide_string_index)?; // 115
        w.write_i32(text.union_index)?; // 119
        w.write_u8(if text.use_inverted_rectangle { 1 } else { 0 })?; // 123
        w.write_i32(text.inverted_rect_width.to_raw())?; // 124
        w.write_i32(text.inverted_rect_height.to_raw())?; // 128
        w.write_u8(text.justification.to_pcb_autoposition())?; // 132
        w.write_i32(text.inverted_rect_text_offset.to_raw())?; // 133
        w.write_i32(text.bar_code_full_width.to_raw())?; // 137
        w.write_i32(text.bar_code_full_height.to_raw())?; // 141
        w.write_i32(text.bar_code_x_margin.to_raw())?; // 145
        w.write_i32(text.bar_code_y_margin.to_raw())?; // 149
        w.write_i32(text.bar_code_min_width.to_raw())?; // 153
        w.write_u8(text.bar_code_kind as u8)?; // 157
        w.write_u8(text.bar_code_render_mode as u8)?; // 158
        w.write_u8(if text.bar_code_inverted { 1 } else { 0 })?; // 159
        w.write_u8(i32::from(text.text_kind) as u8)?; // 160 authoritative kind
        w.write_font_name(text.bar_code_font_name.as_deref().unwrap_or("Arial"))?; // 161..225
        w.write_u8(if text.bar_code_show_text { 1 } else { 0 })?; // 225
        w.write_u32(if text.layer_v7 != 0 {
            text.layer_v7
        } else {
            v7_layer_id(text.layer)
        })?; // 226
        w.write_u8(if text.is_frame { 1 } else { 0 })?; // 230
        w.write_u8(if text.is_offset_border { 1 } else { 0 })?; // 231
        w.write_i32(i32::MIN)?; // 232 reserved
        w.write_i32(i32::MIN)?; // 236 reserved
        w.write_u8(if text.justification_valid { 1 } else { 0 })?; // 240
        w.write_u8(if text.advance_snapping { 1 } else { 0 })?; // 241
        w.write_i16(0)?; // 242
        w.write_i32(text.snap_point_x.to_raw())?; // 244
        w.write_i32(text.snap_point_y.to_raw())?; // 248
        Ok(())
    })?;
    bw.write_pascal_string_block(&text.text)
}

/// Altium's V7 layer identifier for a legacy layer byte (the value stored in
/// the extended tails of text/fill/arc records and in `LAYER_V8_*LAYERID`).
pub(crate) fn v7_layer_id(layer: i32) -> u32 {
    match layer {
        32 => 0x0100_FFFF,
        1..=31 => 0x0100_0000 + layer as u32,
        39..=54 => 0x0101_0000 + (layer - 38) as u32,
        57..=72 => 0x0102_0000 + (layer - 56) as u32,
        33 => 0x0103_0006,
        34 => 0x0103_0007,
        35 => 0x0103_0008,
        36 => 0x0103_0009,
        37 => 0x0103_000A,
        38 => 0x0103_000B,
        55 => 0x0103_000C,
        56 => 0x0103_000D,
        73 => 0x0103_000E,
        _ => 0x0103_000F, // multi-layer (also the fallback)
    }
}

fn write_fill<W: Write + Seek>(bw: &mut BinaryWriter<W>, fill: &Fill) -> Result<()> {
    if let Some(raw) = &fill.raw_record {
        let mut b = raw.clone();
        patch_common_prefix(
            &mut b,
            fill.layer as u8,
            PrimitiveFlags {
                is_locked: fill.is_locked,
                is_tenting_top: fill.is_tenting_top,
                is_tenting_bottom: fill.is_tenting_bottom,
                is_keepout: fill.is_keepout,
                extra: fill.flags_extra,
                ..PrimitiveFlags::default()
            },
            fill.net_index as i32,
            fill.component_index,
        );
        patch_point(&mut b, 13, fill.corner1);
        patch_point(&mut b, 21, fill.corner2);
        patch_f64(&mut b, 29, fill.rotation);
        return bw.write_block(|w| {
            w.write_bytes(&b)?;
            Ok(())
        });
    }
    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: fill.is_locked,
            is_tenting_top: fill.is_tenting_top,
            is_tenting_bottom: fill.is_tenting_bottom,
            is_keepout: fill.is_keepout,
            extra: fill.flags_extra,
            ..PrimitiveFlags::default()
        }
        .encode();
        write_common_prefix_full(
            w,
            fill.layer as u8,
            flags,
            fill.net_index as i32,
            fill.component_index,
        )?;
        write_coord_point(w, fill.corner1)?;
        write_coord_point(w, fill.corner2)?;
        w.write_f64(fill.rotation)?;
        // Altium's 50-byte fill record: sub-polygon index, pad byte, a
        // reserved word, the V7 layer id and a reserved word.
        w.write_u16(0)?;
        w.write_u8(0)?;
        w.write_u16(0)?;
        w.write_u32(v7_layer_id(fill.layer))?;
        w.write_u32(0)?;
        Ok(())
    })
}

/// Rebuild a region record from its raw bytes: patch the common prefix,
/// keep the parameter block verbatim, re-emit outline + holes from the
/// typed fields, and append whatever followed the geometry unchanged.
/// Returns `None` when the raw buffer doesn't parse (caller falls back to
/// the structured writer).
fn splice_region_raw(raw: &[u8], region: &Region) -> Option<Vec<u8>> {
    if raw.len() < 22 {
        return None;
    }
    let plen = u32::from_le_bytes(raw[18..22].try_into().ok()?) as usize;
    let geo_start = 22usize.checked_add(plen)?;
    if geo_start + 4 > raw.len() {
        return None;
    }
    // Walk the original geometry to find the residual tail.
    let mut off = geo_start;
    let read_u32 = |o: usize| -> Option<usize> {
        raw.get(o..o + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
    };
    let n = read_u32(off)?;
    off = off.checked_add(4 + n * 16)?;
    for _ in 0..region.holes.len() {
        if off + 4 > raw.len() {
            break;
        }
        let n = read_u32(off)?;
        off = off.checked_add(4 + n * 16)?;
    }
    if off > raw.len() {
        return None;
    }

    let mut b = raw[..geo_start].to_vec();
    patch_common_prefix(
        &mut b,
        region.layer as u8,
        PrimitiveFlags {
            is_locked: region.is_locked,
            is_tenting_top: region.is_tenting_top,
            is_tenting_bottom: region.is_tenting_bottom,
            is_keepout: region.is_keepout,
            extra: region.flags_extra,
            is_teardrop: region.is_teardrop,
            ..PrimitiveFlags::default()
        },
        region.net_index as i32,
        region.component_index,
    );
    // Original vertices are f64 with sub-integer precision the typed model
    // can't hold. When the geometry is unchanged (to integer precision),
    // reuse the original bytes verbatim; only a real transform rebuilds.
    let geometry_unchanged = {
        let mut o = geo_start;
        let mut matches = true;
        let mut check_list = |o: &mut usize, verts: &[crate::coord::CoordPoint]| -> bool {
            let Some(n) = read_u32(*o) else { return false };
            *o += 4;
            if n != verts.len() {
                return false;
            }
            for p in verts {
                let (Some(xb), Some(yb)) = (raw.get(*o..*o + 8), raw.get(*o + 8..*o + 16)) else {
                    return false;
                };
                let x = f64::from_le_bytes(xb.try_into().unwrap()) as i32;
                let y = f64::from_le_bytes(yb.try_into().unwrap()) as i32;
                if x != p.x.to_raw() || y != p.y.to_raw() {
                    return false;
                }
                *o += 16;
            }
            true
        };
        if !check_list(&mut o, &region.outline) {
            matches = false;
        }
        if matches {
            for hole in &region.holes {
                if !check_list(&mut o, hole) {
                    matches = false;
                    break;
                }
            }
        }
        matches
    };
    if geometry_unchanged {
        b.extend_from_slice(&raw[geo_start..]);
        return Some(b);
    }

    let mut push_verts = |b: &mut Vec<u8>, verts: &[crate::coord::CoordPoint]| {
        b.extend_from_slice(&(verts.len() as u32).to_le_bytes());
        for p in verts {
            b.extend_from_slice(&(p.x.to_raw() as f64).to_le_bytes());
            b.extend_from_slice(&(p.y.to_raw() as f64).to_le_bytes());
        }
    };
    push_verts(&mut b, &region.outline);
    for hole in &region.holes {
        push_verts(&mut b, hole);
    }
    b.extend_from_slice(&raw[off..]);
    Some(b)
}

fn write_region<W: Write + Seek>(bw: &mut BinaryWriter<W>, region: &Region) -> Result<()> {
    // Raw path: patched prefix + original params, with the geometry
    // (outline + holes) rebuilt from the typed fields and any residual
    // tail bytes preserved.
    if let Some(raw) = &region.raw_record {
        if let Some(body) = splice_region_raw(raw, region) {
            return bw.write_block(|w| {
                w.write_bytes(&body)?;
                Ok(())
            });
        }
    }
    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: region.is_locked,
            is_tenting_top: region.is_tenting_top,
            is_tenting_bottom: region.is_tenting_bottom,
            is_keepout: region.is_keepout,
            extra: region.flags_extra,
            is_teardrop: region.is_teardrop,
            ..PrimitiveFlags::default()
        }
        .encode();
        write_common_prefix_full(
            w,
            region.layer as u8,
            flags,
            region.net_index as i32,
            region.component_index,
        )?;
        w.write_u32(0)?;
        w.write_u8(0)?;

        let mut params = ParameterMap::new();
        params.insert("V7_LAYER", layer_byte_to_name(region.layer as u8));
        params.insert("NAME", region.name.clone().unwrap_or_else(|| " ".into()));
        params.insert("KIND", region.kind.to_string());
        params.insert("SUBPOLYINDEX", region.sub_poly_index.to_string());
        params.insert("UNIONINDEX", region.union_index.to_string());
        params.insert(
            "ARCRESOLUTION",
            format!(
                "{}mil",
                if region.arc_resolution == 0.0 {
                    0.5
                } else {
                    region.arc_resolution
                }
            ),
        );
        params.insert(
            "ISSHAPEBASED",
            if region.is_shape_based {
                "TRUE"
            } else {
                "FALSE"
            },
        );
        if let Some(net) = &region.net {
            params.insert("NET", net.clone());
        }
        if let Some(uid) = &region.unique_id {
            params.insert("UNIQUEID", uid.clone());
        }
        // Mask/relief/power-plane numeric parameters: only emit when
        // the field is non-default so we don't pollute regions that
        // never carried them in the source file.
        if region.paste_mask_expansion.to_raw() != 0 {
            params.insert(
                "PASTEMASKEXPANSION",
                region.paste_mask_expansion.to_raw().to_string(),
            );
        }
        if region.solder_mask_expansion.to_raw() != 0 {
            params.insert(
                "SOLDERMASKEXPANSION",
                region.solder_mask_expansion.to_raw().to_string(),
            );
        }
        if region.cavity_height.to_raw() != 0 {
            params.insert("CAVITYHEIGHT", region.cavity_height.to_raw().to_string());
        }
        if region.power_plane_clearance.to_raw() != 0 {
            params.insert(
                "POWERPLANECLEARANCE",
                region.power_plane_clearance.to_raw().to_string(),
            );
        }
        if region.power_plane_connect_style != 0 {
            params.insert(
                "POWERPLANECONNECTSTYLE",
                region.power_plane_connect_style.to_string(),
            );
        }
        if region.power_plane_relief_expansion.to_raw() != 0 {
            params.insert(
                "POWERPLANERELIEFEXPANSION",
                region.power_plane_relief_expansion.to_raw().to_string(),
            );
        }
        if region.relief_air_gap.to_raw() != 0 {
            params.insert("RELIEFAIRGAP", region.relief_air_gap.to_raw().to_string());
        }
        if region.relief_conductor_width.to_raw() != 0 {
            params.insert(
                "RELIEFCONDUCTORWIDTH",
                region.relief_conductor_width.to_raw().to_string(),
            );
        }
        if region.relief_entries != 0 {
            params.insert("RELIEFENTRIES", region.relief_entries.to_string());
        }
        if region.hole_count != 0 {
            params.insert("HOLECOUNT", region.hole_count.to_string());
        }
        if region.total_vertex_count != 0 {
            params.insert("TOTALVERTEXCOUNT", region.total_vertex_count.to_string());
        }
        if region.area != 0 {
            params.insert("AREA", region.area.to_string());
        }
        if region.arc_approximation.to_raw() != 0 {
            params.insert(
                "ARCAPPROXIMATION",
                region.arc_approximation.to_raw().to_string(),
            );
        }
        // Booleans: `enabled` defaults to true, so emit only when false
        // (matches the "absent means default" file convention). The
        // rest default to false and emit only when true.
        if !region.enabled {
            params.insert("ENABLED", "FALSE");
        }
        if region.user_routed {
            params.insert("USERROUTED", "TRUE");
        }
        if region.is_free_primitive {
            params.insert("ISFREEPRIM", "TRUE");
        }
        if region.is_electrical_prim {
            params.insert("ISELECTRICALPRIM", "TRUE");
        }
        if region.is_pre_route {
            params.insert("ISPREROUTE", "TRUE");
        }
        if region.tear_drop {
            params.insert("TEARDROP", "TRUE");
        }
        if region.polygon_outline {
            params.insert("POLYGONOUTLINE", "TRUE");
        }
        if region.is_tenting {
            params.insert("ISTENTING", "TRUE");
        }
        if region.is_testpoint_top {
            params.insert("ISTESTPOINTTOP", "TRUE");
        }
        if region.is_testpoint_bottom {
            params.insert("ISTESTPOINTBOTTOM", "TRUE");
        }
        if region.is_assy_testpoint_top {
            params.insert("ISASSEMBLYTESTPOINTTOP", "TRUE");
        }
        if region.is_assy_testpoint_bottom {
            params.insert("ISASSEMBLYTESTPOINTBOTTOM", "TRUE");
        }
        if region.is_hidden {
            params.insert("ISHIDDEN", "TRUE");
        }
        if region.allow_global_edit {
            params.insert("ALLOWGLOBALEDIT", "TRUE");
        }
        if region.moveable {
            params.insert("MOVEABLE", "TRUE");
        }
        if region.is_simple_region {
            params.insert("ISSIMPLEREGION", "TRUE");
        }
        if region.virtual_cutout {
            params.insert("VIRTUALCUTOUT", "TRUE");
        }
        if let Some(extra) = &region.additional_parameters {
            for (k, v) in extra {
                params.insert(k, v.clone());
            }
        }
        write_c_string_param_block(w, &params)?;

        w.write_u32(region.outline.len() as u32)?;
        for pt in &region.outline {
            w.write_f64(pt.x.to_raw() as f64)?;
            w.write_f64(pt.y.to_raw() as f64)?;
        }
        Ok(())
    })
}

fn write_component_body<W: Write + Seek>(
    bw: &mut BinaryWriter<W>,
    body: &ComponentBody,
) -> Result<()> {
    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: body.is_locked,
            is_tenting_top: body.is_tenting_top,
            is_tenting_bottom: body.is_tenting_bottom,
            is_keepout: body.is_keepout,
            extra: body.flags_extra,
            ..PrimitiveFlags::default()
        }
        .encode();
        let layer = layer_name_to_byte(&body.layer_name);
        write_common_prefix_full(w, layer, flags, body.net_index as i32, body.component_index)?;
        w.write_u32(0)?;
        w.write_u8(0)?;

        // Key order follows what Altium writes for a component body.
        let extra = body.additional_parameters.clone().unwrap_or_default();
        let mut params = ParameterMap::new();
        params.insert("V7_LAYER", body.layer_name.clone());
        params.insert(
            "NAME",
            body.name
                .clone()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| " ".to_string()),
        );
        params.insert("KIND", body.kind.to_string());
        params.insert("SUBPOLYINDEX", body.sub_poly_index.to_string());
        params.insert("UNIONINDEX", body.union_index.to_string());
        params.insert(
            "ARCRESOLUTION",
            format!(
                "{}mil",
                if body.arc_resolution == 0.0 {
                    0.5
                } else {
                    body.arc_resolution
                }
            ),
        );
        params.insert(
            "ISSHAPEBASED",
            if body.is_shape_based { "TRUE" } else { "FALSE" },
        );
        // Coord-valued parameters use Altium's mil-string form.
        params.insert(
            "CAVITYHEIGHT",
            super::doc_codec::format_mil_coord(body.cavity_height),
        );
        params.insert(
            "STANDOFFHEIGHT",
            super::doc_codec::format_mil_coord(body.standoff_height),
        );
        params.insert(
            "OVERALLHEIGHT",
            super::doc_codec::format_mil_coord(body.overall_height),
        );
        params.insert("BODYPROJECTION", body.body_projection.to_string());
        params.insert("BODYCOLOR3D", body.body_color_3d.to_string());
        params.insert("BODYOPACITY3D", format!("{:.3}", body.body_opacity_3d));
        if let Some(v) = extra.get("BODYOVERRIDECOLOR") {
            params.insert("BODYOVERRIDECOLOR", v.clone());
        }
        params.insert("IDENTIFIER", body.identifier.clone().unwrap_or_default());
        params.insert("TEXTURE", body.texture.clone().unwrap_or_default());
        for key in [
            "TEXTURECENTERX",
            "TEXTURECENTERY",
            "TEXTURESIZEX",
            "TEXTURESIZEY",
            "TEXTUREROTATION",
        ] {
            if let Some(v) = extra.get(key) {
                params.insert(key, v.clone());
            }
        }
        params.insert(
            "MODELID",
            format_model_id(body.model_id.as_deref().unwrap_or_default()),
        );
        params.insert("MODEL.CHECKSUM", (body.model_checksum as u32).to_string());
        params.insert(
            "MODEL.EMBED",
            if body.model_embed { "TRUE" } else { "FALSE" },
        );
        params.insert("MODEL.NAME", body.model_name.clone().unwrap_or_default());
        params.insert(
            "MODEL.2D.X",
            super::doc_codec::format_mil_coord(body.model_2d_location.x),
        );
        params.insert(
            "MODEL.2D.Y",
            super::doc_codec::format_mil_coord(body.model_2d_location.y),
        );
        params.insert(
            "MODEL.2D.ROTATION",
            format!("{:.3}", body.model_2d_rotation),
        );
        params.insert("MODEL.3D.ROTX", format!("{:.3}", body.model_3d_rot_x));
        params.insert("MODEL.3D.ROTY", format!("{:.3}", body.model_3d_rot_y));
        params.insert("MODEL.3D.ROTZ", format!("{:.3}", body.model_3d_rot_z));
        params.insert(
            "MODEL.3D.DZ",
            super::doc_codec::format_mil_coord(body.model_3d_dz),
        );
        // Snap points sit between MODEL.3D.DZ and MODEL.MODELTYPE.
        if let Some(v) = extra.get("MODEL.SNAPCOUNT") {
            params.insert("MODEL.SNAPCOUNT", v.clone());
            for i in 0.. {
                let keys = [
                    format!("MODEL.S{i}X"),
                    format!("MODEL.S{i}Y"),
                    format!("MODEL.S{i}Z"),
                ];
                if !extra.contains_key(&keys[0]) {
                    break;
                }
                for k in &keys {
                    if let Some(v) = extra.get(k) {
                        params.insert(k, v.clone());
                    }
                }
            }
        }
        params.insert("MODEL.MODELTYPE", body.model_type.to_string());
        // Older bodies have no MODEL.MODELSOURCE at all; only write what was there.
        if let Some(src) = &body.model_source {
            params.insert("MODEL.MODELSOURCE", src.clone());
        }
        // Extruded bodies: height range, then the contour in vertex order.
        for key in [
            "MODEL.EXTRUDED.MINZ",
            "MODEL.EXTRUDED.MAXZ",
            "MAINCONTOURVERTEXCOUNT",
        ] {
            if let Some(v) = extra.get(key) {
                params.insert(key, v.clone());
            }
        }
        if let Some(n) = extra
            .get("MAINCONTOURVERTEXCOUNT")
            .and_then(|v| v.parse::<usize>().ok())
        {
            for i in 0..n {
                for prefix in ["KIND", "VX", "VY", "CX", "CY", "SA", "EA", "R"] {
                    let key = format!("{prefix}{i}");
                    if let Some(v) = extra.get(&key) {
                        params.insert(&key, v.clone());
                    }
                }
            }
        }
        // Anything else carried in additional_parameters (e.g. the extruded
        // body MINZ/MAXZ pair) overrides the above or is appended.
        for (k, v) in &extra {
            params.insert(k, v.clone());
        }
        // Altium writes the body parameter string without a leading `|`.
        write_body_param_block(w, &params)?;

        w.write_u32(body.outline.len() as u32)?;
        for pt in &body.outline {
            w.write_f64(pt.x.to_raw() as f64)?;
            w.write_f64(pt.y.to_raw() as f64)?;
        }
        Ok(())
    })
}

/// Normalise a UUID into Altium's canonical `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`
/// form. Non-UUID inputs are returned unchanged.
fn format_model_id(raw: &str) -> String {
    let trimmed = raw.trim().trim_start_matches('{').trim_end_matches('}');
    let hex_only: String = trimmed.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex_only.len() != 32 {
        return raw.to_string();
    }
    let h = hex_only.to_ascii_uppercase();
    format!(
        "{{{}-{}-{}-{}-{}}}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

// Document writer (PcbDoc)

impl Document {
    /// Serialise this document to a `.PcbDoc` byte buffer.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut cf = CompoundFile::create()?;

        let mut doc = self.clone();
        resolve_primitive_net_indexes(&mut doc);

        // Entries consumed from this map as we go land in their canonical
        // OLE position; the trailing `write_additional_streams` call
        // catches anything that didn't.
        let mut leftover = doc.additional_streams.clone();

        write_doc_file_headers(&mut cf, &doc, &mut leftover)?;
        write_doc_storages_in_canonical_order(&mut cf, &doc, &mut leftover)?;
        write_additional_streams(&mut cf, "", &leftover)?;

        cf.into_bytes()
    }

    /// Write to disk as a `.PcbDoc`.
    pub async fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = self.to_bytes()?;
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    /// Write to any `AsyncWrite`.
    pub async fn write_async<W>(&self, mut writer: W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        use tokio::io::AsyncWriteExt;
        let bytes = self.to_bytes()?;
        writer.write_all(&bytes).await?;
        writer.flush().await?;
        Ok(())
    }
}

/// Fill each primitive's `net_index` from its `net` name. Indexing is
/// 1-based on disk; `0` means "no net", and unknown names collapse to `0`.
fn resolve_primitive_net_indexes(document: &mut Document) {
    let lookup: std::collections::HashMap<&str, u16> = document
        .nets
        .iter()
        .enumerate()
        .filter_map(|(i, net)| {
            let idx = (i + 1) as u32;
            (idx <= 0xFFFE).then_some((net.name.as_str(), idx as u16))
        })
        .collect();
    let resolve = |name: &Option<String>| -> u16 {
        name.as_deref()
            .and_then(|n| lookup.get(n).copied())
            .unwrap_or(0)
    };
    for t in &mut document.tracks {
        t.net_index = resolve(&t.net);
    }
    for a in &mut document.arcs {
        a.net_index = resolve(&a.net);
    }
    for p in &mut document.pads {
        p.net_index = resolve(&p.net);
    }
    for v in &mut document.vias {
        v.net_index = resolve(&v.net);
    }
    for f in &mut document.fills {
        f.net_index = resolve(&f.net);
    }
    for r in &mut document.regions {
        r.net_index = resolve(&r.net);
    }
}

fn write_doc_board(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    cf.create_storage("Board6")?;
    write_storage_header(cf, "Board6/Header", 1)?;

    let entries: Vec<(String, String)> = super::defaults::ensure_pcbdoc_board_parameters_complete(
        document.board_parameters.as_ref().map(|v| v.as_slice()),
    );

    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    // Emit verbatim `|k=v|…\x00` so repeated `RECORD=Board` markers and
    // embedded `\r` separators round-trip byte-for-byte.
    bw.write_block(|w| {
        let mut body = Vec::<u8>::new();
        for (k, v) in &entries {
            body.push(b'|');
            body.extend_from_slice(crate::encoding::encode(k).as_ref());
            body.push(b'=');
            body.extend_from_slice(crate::encoding::encode(v).as_ref());
        }
        body.push(0);
        w.write_bytes(&body)?;
        Ok(())
    })?;
    cf.write_stream("Board6/Data", &buf.into_inner())?;
    Ok(())
}

/// Emit `FileHeaderSix` (75-byte modern marker) and `FileHeader` (24-byte
/// legacy stub). Order matters — `FileHeaderSix` lands at SID 1, which is
/// what marks the file as modern PcbDoc. Round-trips reuse `additional_streams`
/// bytes verbatim when present.
fn write_doc_file_headers(
    cf: &mut CompoundFile,
    _document: &Document,
    leftover: &mut std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    if let Some(bytes) = leftover.remove("FileHeaderSix") {
        cf.write_stream("FileHeaderSix", &bytes)?;
    } else {
        let mut buf = Cursor::new(Vec::<u8>::new());
        let mut bw = BinaryWriter::new(&mut buf);
        let version_text = "PCB 6.0 Binary File";
        let version_bytes = encoding::encode(version_text);
        bw.write_i32(version_bytes.len() as i32)?;
        bw.write_u8(version_bytes.len() as u8)?;
        bw.write_bytes(&version_bytes)?;
        bw.write_f64(5.01)?;
        let guid = stable_document_guid();
        let id_bytes = encoding::encode(&guid);
        bw.write_i32(id_bytes.len() as i32)?;
        bw.write_u8(id_bytes.len() as u8)?;
        bw.write_bytes(&id_bytes)?;
        cf.write_stream("FileHeaderSix", &buf.into_inner())?;
    }

    if let Some(bytes) = leftover.remove("FileHeader") {
        cf.write_stream("FileHeader", &bytes)?;
    } else {
        let mut legacy: Vec<u8> = Vec::with_capacity(24);
        legacy.extend_from_slice(&19i32.to_le_bytes());
        for ch in "PCB 5.0 Bi".chars() {
            let v = ch as u32;
            legacy.push((v & 0xff) as u8);
            legacy.push(((v >> 8) & 0xff) as u8);
        }
        debug_assert_eq!(legacy.len(), 24);
        cf.write_stream("FileHeader", &legacy)?;
    }
    Ok(())
}

/// Per-session GUID for from-scratch documents, derived from the clock.
fn stable_document_guid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xDEAD_BEEF_CAFE_BABE);
    let a = (nanos as u32).wrapping_mul(0x9E37_79B9);
    let b = ((nanos >> 32) as u32) ^ a;
    let c = (nanos.wrapping_mul(0xBF58_476D_1CE4_E5B9) >> 16) as u16;
    let d = ((nanos.wrapping_mul(0x94D0_49BB_1331_11EB)) >> 48) as u16;
    let e = nanos.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}",
        a,
        (b & 0xFFFF) as u16,
        ((b >> 16) & 0xFFFF) as u16,
        c ^ d,
        e & 0xFFFF_FFFF_FFFF,
    )
}

/// Create every storage and stream in the canonical order a freshly-saved
/// Altium PcbDoc emits. Each entry pulls from `leftover` (round-trip case)
/// or falls back to a default stub, removing the key from `leftover` so the
/// trailing `write_additional_streams` doesn't double-write it.
fn write_doc_storages_in_canonical_order(
    cf: &mut CompoundFile,
    doc: &Document,
    leftover: &mut std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    let default_advanced_placer = default_param_block(&[
        ("RECORD", "AdvancedPlacerOptions"),
        ("PLACELARGECLEAR", "50mil"),
        ("PLACESMALLCLEAR", "20mil"),
        ("PLACEUSEROTATION", "TRUE"),
        ("PLACEUSELAYERSWAP", "FALSE"),
        ("PLACEBYPASSNET1", ""),
        ("PLACEBYPASSNET2", ""),
        ("PLACEUSEADVANCEDPLACE", "TRUE"),
        ("PLACEUSEGROUPING", "TRUE"),
    ]);
    let default_drc = default_param_block(&[
        ("RECORD", "DesignRuleCheckerOptions"),
        ("DOMAKEDRCFILE", "FALSE"),
        ("DOMAKEDRCERRORLIST", "FALSE"),
        ("DOSUBNETDETAILS", "TRUE"),
        ("REPORTFILENAME", ""),
        ("EXTERNALNETLISTFILENAME", ""),
        ("CHECKEXTERNALNETLIST", "FALSE"),
        ("MAXVIOLATIONCOUNT", "500"),
        ("REPORTDRILLEDSMTPADS", "FALSE"),
        ("REPORTINVALIDMULTILAYERPADS", "TRUE"),
    ]);
    let default_pin_swap = default_param_block(&[
        ("RECORD", "PinSwapOptions"),
        ("QUIET", "FALSE"),
        ("APPROXIMATEPINPOSITIONS", "FALSE"),
        ("ALLOWPARTIALLYROUTEDCONNECTIONS", "TRUE"),
        ("VIAPENALTYSTATE", "TRUE"),
        ("CROSSOVERRATIO", "50"),
        ("VIAPENALTYVALUE", "0"),
        ("IGNORENETS", ""),
        ("IGNORENETCLASSES", ""),
        ("IGNORECOMPONENTS", ""),
        ("IGNOREDIFFERENTIALPAIRS", ""),
        ("HEURISTICNAME", ""),
        ("HEURISTICONOFFSTATE", ""),
        ("HEURISTICWEIGHTVALUE", ""),
    ]);
    let default_padvia_cache = default_param_block(&[
        (
            "PADVIALIBRARY.LIBRARYID",
            "{00000000-0000-0000-0000-000000000001}",
        ),
        ("PADVIALIBRARY.LIBRARYNAME", "<Local>"),
        ("PADVIALIBRARY.DISPLAYUNITS", "1"),
    ]);
    let layer_kind_data = default_layer_kind_mapping_data();
    let fvi_data = default_file_version_info_data();

    // Pre-Board6.
    emit_or_consume(cf, leftover, "Texts", 3, &[])?;
    emit_or_consume(cf, leftover, "EmbeddedFonts6", 0, &[])?;

    write_doc_board(cf, doc)?;

    emit_or_consume(
        cf,
        leftover,
        "Advanced Placer Options6",
        0,
        &default_advanced_placer,
    )?;
    emit_or_consume(
        cf,
        leftover,
        "Design Rule Checker Options6",
        0,
        &default_drc,
    )?;

    write_doc_classes(cf, doc)?;
    emit_or_consume(cf, leftover, "Classes6", 0, &[])?;
    write_doc_nets(cf, doc)?;
    emit_or_consume(cf, leftover, "Nets6", 0, &[])?;
    write_doc_components(cf, doc)?;
    emit_or_consume(cf, leftover, "Components6", 0, &[])?;
    write_doc_polygons(cf, doc)?;
    emit_or_consume(cf, leftover, "Polygons6", 0, &[])?;
    emit_or_consume(cf, leftover, "Dimensions6", 0, &[])?;
    emit_or_consume(cf, leftover, "Coordinates6", 0, &[])?;
    write_doc_embedded_boards(cf, doc)?;
    emit_or_consume(cf, leftover, "EmbeddedBoards6", 0, &[])?;
    emit_or_consume(cf, leftover, "Connections6", 0, &[])?;
    write_doc_rules(cf, doc)?;
    emit_or_consume(cf, leftover, "Rules6", 0, &[])?;
    emit_or_consume(cf, leftover, "FromTos6", 0, &[])?;
    write_doc_differential_pairs(cf, doc)?;
    emit_or_consume(cf, leftover, "DifferentialPairs6", 0, &[])?;
    // Rooms are written only when present — Altium rejects an empty
    // Rooms6 stub.
    if !doc.rooms.is_empty() {
        write_doc_rooms(cf, doc)?;
    }
    emit_or_consume(cf, leftover, "Embeddeds6", 0, &[])?;
    write_doc_arcs(cf, doc)?;
    emit_or_consume(cf, leftover, "Arcs6", 0, &[])?;
    write_doc_pads(cf, doc)?;
    emit_or_consume(cf, leftover, "Pads6", 0, &[])?;
    write_doc_vias(cf, doc)?;
    emit_or_consume(cf, leftover, "Vias6", 0, &[])?;
    write_doc_tracks(cf, doc)?;
    emit_or_consume(cf, leftover, "Tracks6", 0, &[])?;
    write_doc_texts(cf, doc)?;
    emit_or_consume(cf, leftover, "Texts6", 0, &[])?;
    write_doc_fills(cf, doc)?;
    emit_or_consume(cf, leftover, "Fills6", 0, &[])?;
    write_doc_regions(cf, doc)?;
    emit_or_consume(cf, leftover, "Regions6", 0, &[])?;
    write_doc_component_bodies(cf, doc)?;
    emit_or_consume(cf, leftover, "ComponentBodies6", 0, &[])?;
    emit_or_consume(cf, leftover, "Pin Swap Options6", 0, &default_pin_swap)?;
    write_doc_wide_strings(cf, doc)?;
    emit_or_consume(cf, leftover, "WideStrings6", 0, &[])?;
    emit_or_consume(cf, leftover, "ShapeBasedRegions6", 0, &[])?;
    emit_or_consume(cf, leftover, "ShapeBasedComponentBodies6", 0, &[])?;
    emit_or_consume(cf, leftover, "Models", 0, &[])?;
    emit_or_consume(cf, leftover, "ModelsNoEmbed", 0, &[])?;
    emit_or_consume(cf, leftover, "Textures", 0, &[])?;
    emit_or_consume(cf, leftover, "ExtendedPrimitiveInformation", 0, &[])?;
    emit_or_consume(cf, leftover, "UnionNames", 0, &[])?;
    emit_or_consume(cf, leftover, "SmartUnions", 0, &[])?;
    emit_or_consume(cf, leftover, "BoardRegions", 0, &[])?;
    emit_or_consume(cf, leftover, "UniqueIDPrimitiveInformation", 0, &[])?;
    emit_or_consume(cf, leftover, "PinPairsSection", 0, &[])?;
    emit_or_consume(cf, leftover, "SignalClasses", 0, &[])?;
    emit_or_consume(cf, leftover, "PadViaLibrary", 0, &[])?;
    emit_or_consume(cf, leftover, "PadViaLibraryCache", 0, &default_padvia_cache)?;
    emit_or_consume(cf, leftover, "PadViaLibraryLinks", 0, &[])?;
    emit_or_consume(cf, leftover, "PrimitiveParameters", 0, &[])?;
    emit_or_consume(cf, leftover, "WaivedViolations", 0, &[])?;
    emit_or_consume(cf, leftover, "LayerKindMapping", 1, &layer_kind_data)?;
    emit_or_consume(cf, leftover, "ConstraintManager", 0, &[])?;
    emit_or_consume(cf, leftover, "PadViaCacheLibraryLinksSection", 0, &[])?;
    emit_or_consume(cf, leftover, "PrimitiveGuids", 0, &[])?;
    emit_or_consume(cf, leftover, "FileVersionInfo", 1, &fvi_data)?;

    Ok(())
}

/// Write `path` using `leftover` Header/Data bytes when present, otherwise
/// the supplied defaults. If a typed writer already created the storage,
/// leave it alone but still consume matching `leftover` entries so they
/// don't get re-emitted later.
fn emit_or_consume(
    cf: &mut CompoundFile,
    leftover: &mut std::collections::BTreeMap<String, Vec<u8>>,
    path: &str,
    default_header: u32,
    default_data: &[u8],
) -> Result<()> {
    let header_key = format!("{path}/Header");
    let data_key = format!("{path}/Data");

    if cf.is_storage(path) {
        leftover.remove(&header_key);
        leftover.remove(&data_key);
        return Ok(());
    }

    cf.create_storage(path)?;
    if let Some(bytes) = leftover.remove(&header_key) {
        cf.write_stream(&header_key, &bytes)?;
    } else {
        cf.write_stream(&header_key, &default_header.to_le_bytes())?;
    }
    if let Some(bytes) = leftover.remove(&data_key) {
        cf.write_stream(&data_key, &bytes)?;
    } else {
        cf.write_stream(&data_key, default_data)?;
    }
    Ok(())
}

#[allow(dead_code)]
fn write_doc_default_root_stubs(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    if !document
        .additional_streams
        .contains_key("FileVersionInfo/Header")
    {
        cf.write_stream("FileVersionInfo/Header", &u32_le_bytes(1))?;
    }
    if !document
        .additional_streams
        .contains_key("FileVersionInfo/Data")
    {
        cf.write_stream("FileVersionInfo/Data", &default_file_version_info_data())?;
    }

    // Each stub is a 4-byte u32 record-count + a (usually empty) data payload.
    let mut emit = |path: &str, header: u32, data: &[u8]| -> Result<()> {
        if document
            .additional_streams
            .contains_key(&format!("{}/Header", path))
        {
            return Ok(());
        }
        if cf.is_storage(path) {
            return Ok(());
        }
        cf.create_storage(path)?;
        cf.write_stream(format!("{path}/Header"), &header.to_le_bytes())?;
        cf.write_stream(format!("{path}/Data"), data)?;
        Ok(())
    };

    // Storage names below mirror what Altium's own writer produces. Names
    // it doesn't emit (CornerRadiusChamfer, CustomShapes, DrillManager,
    // LettersGeometry, Rooms6) were tried and rejected.
    for name in [
        "Arcs6",
        "Pads6",
        "Vias6",
        "Tracks6",
        "Texts6",
        "Fills6",
        "Regions6",
        "ComponentBodies6",
        "Components6",
        "Nets6",
        "Polygons6",
        "Rules6",
        "Classes6",
        "DifferentialPairs6",
        "EmbeddedBoards6",
        "WideStrings6",
        "Dimensions6",
        "Coordinates6",
        "Connections6",
        "FromTos6",
        "Embeddeds6",
        "ShapeBasedRegions6",
        "ShapeBasedComponentBodies6",
        "ModelsNoEmbed",
        "Textures",
        "ExtendedPrimitiveInformation",
        "UnionNames",
        "SmartUnions",
        "BoardRegions",
        "UniqueIDPrimitiveInformation",
        "PinPairsSection",
        "SignalClasses",
        "PadViaLibrary",
        "PadViaLibraryLinks",
        "PrimitiveParameters",
        "WaivedViolations",
        "ConstraintManager",
        "PrimitiveGuids",
        "EmbeddedFonts6",
        "Models",
    ] {
        emit(name, 0, &[])?;
    }

    // Option-block storages with default parameter content.
    emit(
        "Advanced Placer Options6",
        0,
        &default_param_block(&[
            ("RECORD", "AdvancedPlacerOptions"),
            ("PLACELARGECLEAR", "50mil"),
            ("PLACESMALLCLEAR", "20mil"),
            ("PLACEUSEROTATION", "TRUE"),
            ("PLACEUSELAYERSWAP", "FALSE"),
            ("PLACEBYPASSNET1", ""),
            ("PLACEBYPASSNET2", ""),
            ("PLACEUSEADVANCEDPLACE", "TRUE"),
            ("PLACEUSEGROUPING", "TRUE"),
        ]),
    )?;
    emit(
        "Design Rule Checker Options6",
        0,
        &default_param_block(&[
            ("RECORD", "DesignRuleCheckerOptions"),
            ("DOMAKEDRCFILE", "FALSE"),
            ("DOMAKEDRCERRORLIST", "FALSE"),
            ("DOSUBNETDETAILS", "TRUE"),
            ("REPORTFILENAME", ""),
            ("EXTERNALNETLISTFILENAME", ""),
            ("CHECKEXTERNALNETLIST", "FALSE"),
            ("MAXVIOLATIONCOUNT", "500"),
            ("REPORTDRILLEDSMTPADS", "FALSE"),
            ("REPORTINVALIDMULTILAYERPADS", "TRUE"),
        ]),
    )?;
    emit(
        "Pin Swap Options6",
        0,
        &default_param_block(&[
            ("RECORD", "PinSwapOptions"),
            ("QUIET", "FALSE"),
            ("APPROXIMATEPINPOSITIONS", "FALSE"),
            ("ALLOWPARTIALLYROUTEDCONNECTIONS", "TRUE"),
            ("VIAPENALTYSTATE", "TRUE"),
            ("CROSSOVERRATIO", "50"),
            ("VIAPENALTYVALUE", "0"),
            ("IGNORENETS", ""),
            ("IGNORENETCLASSES", ""),
            ("IGNORECOMPONENTS", ""),
            ("IGNOREDIFFERENTIALPAIRS", ""),
            ("HEURISTICNAME", ""),
            ("HEURISTICONOFFSTATE", ""),
            ("HEURISTICWEIGHTVALUE", ""),
        ]),
    )?;
    emit(
        "PadViaLibraryCache",
        0,
        &default_param_block(&[
            (
                "PADVIALIBRARY.LIBRARYID",
                "{00000000-0000-0000-0000-000000000001}",
            ),
            ("PADVIALIBRARY.LIBRARYNAME", "<Local>"),
            ("PADVIALIBRARY.DISPLAYUNITS", "1"),
        ]),
    )?;
    emit("LayerKindMapping", 1, &default_layer_kind_mapping_data())?;

    // Legacy `Texts` storage — Altium expects it present, but tolerates
    // an empty Data payload.
    if !cf.is_storage("Texts") {
        cf.create_storage("Texts")?;
        cf.write_stream("Texts/Header", &3u32.to_le_bytes())?;
        cf.write_stream("Texts/Data", &[])?;
    }

    Ok(())
}

fn default_param_block(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut body = String::new();
    for (k, v) in entries {
        body.push('|');
        body.push_str(k);
        body.push('=');
        body.push_str(v);
    }
    let mut bytes = body.into_bytes();
    bytes.push(0);
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&bytes);
    out
}

fn write_doc_wide_strings(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    if document.texts.is_empty() {
        return Ok(());
    }
    cf.create_storage("WideStrings6")?;
    write_storage_header(cf, "WideStrings6/Header", 1)?;
    // Documents store wide strings as a binary index table:
    // [u32 index][u32 byte_len][UTF-16LE chars + NUL], one entry per text.
    // (The ENCODEDTEXT parameter-map flavor is only used inside footprint
    // sections.) Empty strings carry no bytes, not even the NUL.
    let mut out = Vec::<u8>::new();
    for (i, text) in document.texts.iter().enumerate() {
        out.extend_from_slice(&(i as u32).to_le_bytes());
        if text.text.is_empty() {
            out.extend_from_slice(&0u32.to_le_bytes());
            continue;
        }
        let units: Vec<u16> = text.text.encode_utf16().chain(std::iter::once(0)).collect();
        out.extend_from_slice(&((units.len() * 2) as u32).to_le_bytes());
        for unit in units {
            out.extend_from_slice(&unit.to_le_bytes());
        }
    }
    cf.write_stream("WideStrings6/Data", &out)?;
    Ok(())
}

macro_rules! write_doc_collection {
    ($name:ident, $field:ident, $storage:literal, $tag:literal, $writer:ident) => {
        fn $name(cf: &mut CompoundFile, document: &Document) -> Result<()> {
            if document.$field.is_empty() {
                return Ok(());
            }
            cf.create_storage($storage)?;
            write_storage_header(
                cf,
                concat!($storage, "/Header"),
                document.$field.len() as i32,
            )?;
            let mut buf = Cursor::new(Vec::<u8>::new());
            let mut bw = BinaryWriter::new(&mut buf);
            for item in &document.$field {
                bw.write_u8($tag)?;
                $writer(&mut bw, item)?;
            }
            cf.write_stream(concat!($storage, "/Data"), &buf.into_inner())?;
            Ok(())
        }
    };
}

write_doc_collection!(write_doc_arcs, arcs, "Arcs6", 1, write_arc);
write_doc_collection!(write_doc_pads, pads, "Pads6", 2, write_pad);
write_doc_collection!(write_doc_vias, vias, "Vias6", 3, write_via);
write_doc_collection!(write_doc_tracks, tracks, "Tracks6", 4, write_track);
write_doc_collection!(write_doc_fills, fills, "Fills6", 6, write_fill);
write_doc_collection!(write_doc_regions, regions, "Regions6", 11, write_region);
write_doc_collection!(
    write_doc_component_bodies,
    component_bodies,
    "ComponentBodies6",
    12,
    write_component_body
);

/// Writes a `*6/Data` stream as a sequence of C-string parameter blocks, one
/// per item, with the standard `Header` companion stream.
fn write_param_storage<I, F>(
    cf: &mut CompoundFile,
    storage: &str,
    items: I,
    fill_one: F,
) -> Result<()>
where
    I: IntoIterator,
    F: Fn(&I::Item, &mut ParameterMap),
{
    write_param_storage_with_prefix(cf, storage, items, fill_one, None)
}

/// Like `write_param_storage` but allows a per-record version word that
/// gets emitted before each record's size header.
///
/// Some param-record storages (Rules6 in particular) carry a `u16`
/// "record version" word in front of every record's `u32 size` prefix.
/// The reader auto-detects this in `detect_record_prefix` and the
/// per-record bytes have to round-trip — without them Altium fires the
/// "catastrophic error" dialog on file open because every rule record
/// after the first is read at the wrong offset.
fn write_param_storage_with_prefix<I, F>(
    cf: &mut CompoundFile,
    storage: &str,
    items: I,
    fill_one: F,
    record_version: Option<u16>,
) -> Result<()>
where
    I: IntoIterator,
    F: Fn(&I::Item, &mut ParameterMap),
{
    let collected: Vec<_> = items.into_iter().collect();
    if collected.is_empty() {
        return Ok(());
    }
    cf.create_storage(storage)?;
    write_storage_header(cf, &format!("{storage}/Header"), collected.len() as i32)?;
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    for item in collected.iter() {
        if let Some(v) = record_version {
            bw.write_u16(v)?;
        }
        let mut params = ParameterMap::new();
        fill_one(item, &mut params);
        write_c_string_param_block(&mut bw, &params)?;
    }
    cf.write_stream(format!("{storage}/Data"), &buf.into_inner())?;
    Ok(())
}

fn write_doc_nets(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    write_param_storage(cf, "Nets6", document.nets.iter(), |net, p| {
        super::doc_codec::net_to_params(net, p);
    })
}

fn write_doc_components(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    write_param_storage(cf, "Components6", document.components.iter(), |c, p| {
        super::doc_codec::component_to_params(c, p);
    })
}

fn write_doc_polygons(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    write_param_storage(cf, "Polygons6", document.polygons.iter(), |poly, p| {
        super::doc_codec::polygon_to_params(poly, p);
    })
}

fn write_doc_rules(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    // Each Rules6 record is preceded by a u16 rule-type-code word. The
    // value is rule-kind-specific (e.g. UnpouredPolygon = 62,
    // SilkToSilkClearance = 55, FanoutControl = 49). We preserve the
    // exact code each rule was read with via `rule.rule_type_code`.
    if document.rules.is_empty() {
        return Ok(());
    }
    cf.create_storage("Rules6")?;
    write_storage_header(cf, "Rules6/Header", document.rules.len() as i32)?;
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    for rule in &document.rules {
        bw.write_u16(rule.rule_type_code)?;
        let mut params = ParameterMap::new();
        super::doc_codec::rule_to_params(rule, &mut params);
        write_c_string_param_block(&mut bw, &params)?;
    }
    cf.write_stream("Rules6/Data", &buf.into_inner())?;
    Ok(())
}

fn write_doc_classes(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    write_param_storage(cf, "Classes6", document.classes.iter(), |c, p| {
        super::doc_codec::object_class_to_params(c, p);
    })
}

fn write_doc_differential_pairs(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    write_param_storage(
        cf,
        "DifferentialPairs6",
        document.differential_pairs.iter(),
        |d, p| {
            super::doc_codec::differential_pair_to_params(d, p);
        },
    )
}

fn write_doc_rooms(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    write_param_storage(cf, "Rooms6", document.rooms.iter(), |r, p| {
        super::doc_codec::room_to_params(r, p);
    })
}

fn write_doc_embedded_boards(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    write_param_storage(
        cf,
        "EmbeddedBoards6",
        document.embedded_boards.iter(),
        |b, p| {
            super::doc_codec::embedded_board_to_params(b, p);
        },
    )
}

fn write_doc_texts(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    if document.texts.is_empty() {
        return Ok(());
    }
    cf.create_storage("Texts6")?;
    write_storage_header(cf, "Texts6/Header", document.texts.len() as i32)?;
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    for (i, text) in document.texts.iter().enumerate() {
        bw.write_u8(5)?;
        write_text(&mut bw, text, i as i32)?;
    }
    cf.write_stream("Texts6/Data", &buf.into_inner())?;
    Ok(())
}

#[allow(dead_code)]
fn coord_to_string(c: Coord) -> String {
    c.to_raw().to_string()
}
