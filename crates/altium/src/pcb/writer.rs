//! Writer for `.PcbLib` and `.PcbDoc` files.

use std::collections::BTreeMap;
use std::io::{Cursor, Seek, Write};
use std::path::Path;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use tokio::io::AsyncWrite;

use super::binary::{
    PrimitiveFlags, layer_byte_to_name, layer_name_to_byte, write_common_prefix,
    write_common_prefix_full, write_coord_point,
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
    let needs: Vec<&Component> = library
        .components
        .iter()
        .filter(|c| section_keys.contains_key(&c.name))
        .collect();
    if needs.is_empty() {
        return Ok(());
    }

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
    write_additional_streams(cf, "Library", &library.additional_library_streams)?;
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
    }
    params.insert("WEIGHT", library.components.len().to_string());
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
    for sub in ["PrimitiveGuids", "UniqueIDPrimitiveInformation"] {
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
    for (k, v) in &component.additional_parameters {
        params.insert(k, v.clone());
    }
    params.insert("PATTERN", component.name.clone());
    params.insert("HEIGHT", component.height.to_raw().to_string());
    if let Some(d) = &component.description {
        params.insert("DESCRIPTION", d.clone());
    }
    if let Some(g) = &component.item_guid {
        params.insert("ITEMGUID", g.clone());
    }
    if let Some(g) = &component.item_revision_guid {
        params.insert("REVISIONGUID", g.clone());
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

    for arc in &component.arcs {
        bw.write_u8(1)?;
        write_arc(&mut bw, arc)?;
    }
    for pad in &component.pads {
        bw.write_u8(2)?;
        write_pad(&mut bw, pad)?;
    }
    for via in &component.vias {
        bw.write_u8(3)?;
        write_via(&mut bw, via)?;
    }
    for track in &component.tracks {
        bw.write_u8(4)?;
        write_track(&mut bw, track)?;
    }
    for (i, text) in component.texts.iter().enumerate() {
        bw.write_u8(5)?;
        write_text(&mut bw, text, i as i32)?;
    }
    for fill in &component.fills {
        bw.write_u8(6)?;
        write_fill(&mut bw, fill)?;
    }
    for region in &component.regions {
        bw.write_u8(11)?;
        write_region(&mut bw, region)?;
    }
    for body in &component.component_bodies {
        bw.write_u8(12)?;
        write_component_body(&mut bw, body)?;
    }

    cf.write_stream(format!("{section_key}/Data"), &buf.into_inner())?;
    Ok(())
}

fn write_library_models(cf: &mut CompoundFile, library: &Library) -> Result<()> {
    cf.create_storage("Library/Models")?;
    write_storage_header(cf, "Library/Models/Header", library.models.len() as i32)?;

    let mut data = Vec::<u8>::new();
    for model in &library.models {
        let param_str = format!(
            "EMBED={}|MODELSOURCE={}|ID={}|ROTX={:.3}|ROTY={:.3}|ROTZ={:.3}|DZ={}|CHECKSUM={}|NAME={}",
            if model.is_embedded { "TRUE" } else { "FALSE" },
            model.model_source,
            format_model_id(&model.id),
            model.rotation_x,
            model.rotation_y,
            model.rotation_z,
            model.dz,
            model.checksum,
            model.name,
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
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
            encoder.write_all(model.step_data.as_bytes())?;
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
    if !contains_library_stream(library, "ComponentParamsTOC/Header") {
        cf.write_stream(
            "Library/ComponentParamsTOC/Header",
            &u32_le_bytes(library.components.len() as u32),
        )?;
    }
    if !contains_library_stream(library, "ComponentParamsTOC/Data") {
        cf.write_stream(
            "Library/ComponentParamsTOC/Data",
            &default_component_params_toc_data(library),
        )?;
    }
    Ok(())
}

fn default_component_params_toc_data(library: &Library) -> Vec<u8> {
    let mut data = Vec::<u8>::new();
    for component in &library.components {
        let name = component.name.replace('|', "/");
        let pad_count = component.pads.len();
        let height_mil = component.height.to_mils();
        let description = component
            .description
            .clone()
            .unwrap_or_default()
            .replace('|', "/");
        let body = format!(
            "Name={name}|Pad Count={pad_count}|Height={height_mil:.4}|Description={description}\r\n",
        );
        let mut body_bytes = body.into_bytes();
        body_bytes.push(0); // C-string terminator
        let len = body_bytes.len() as u32;
        data.extend_from_slice(&len.to_le_bytes());
        data.extend_from_slice(&body_bytes);
    }
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
    const ENTRIES: &[(&str, &str, &str)] = &[
        (
            "Winter 09",
            "",
            "<b>CAUTION</b> - Vias support varying diameters across layerstack. \
             If this feature is used in design, extra values will be discarded.",
        ),
        (
            "Winter 09",
            "",
            "<b>CAUTION</b> - File may contain pads with hole offsets. \
             Hole offset information will be discarded.",
        ),
        (
            "Winter 09",
            "",
            "<b>CAUTION</b> - 3D models now support texturing.\
             If used in design these textures will be discarded.",
        ),
        (
            "Summer 09",
            "",
            "<b>CAUTION</b> - Support was added for 32 Mechanical Layers. \
             Objects on mechanical layers beyond 16 have been moved to Mechanical Layer 16.",
        ),
        (
            "Release 10",
            "",
            "<b>CAUTION</b> - New Custom Grids and Guides were introduced. \
             Be aware that your design might contain Custom Grids and Guides \
             that cannot be read in previous versions. ",
        ),
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

// Primitive writers

fn write_arc<W: Write + Seek>(bw: &mut BinaryWriter<W>, arc: &Arc) -> Result<()> {
    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: arc.is_locked,
            is_tenting_top: arc.is_tenting_top,
            is_tenting_bottom: arc.is_tenting_bottom,
            is_keepout: arc.is_keepout,
        }
        .encode();
        write_common_prefix_full(w, arc.layer as u8, flags, arc.net_index as i32, -1)?;
        write_coord_point(w, arc.center)?;
        w.write_i32(arc.radius.to_raw())?;
        w.write_f64(arc.start_angle)?;
        w.write_f64(arc.end_angle)?;
        w.write_i32(arc.width.to_raw())?;
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

    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: pad.is_locked,
            is_tenting_top: pad.is_tenting_top,
            is_tenting_bottom: pad.is_tenting_bottom,
            is_keepout: pad.is_keepout,
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
        // Offset 61-85
        w.write_u8(0)?;
        w.write_u8(pad.mode as u8)?;
        w.write_u8(pad.power_plane_connect_style as u8)?;
        w.write_i32(pad.relief_air_gap.to_raw())?;
        w.write_i32(pad.relief_conductor_width.to_raw())?;
        w.write_i16(pad.relief_entries as i16)?;
        w.write_i32(pad.power_plane_clearance.to_raw())?;
        w.write_i32(pad.power_plane_relief_expansion.to_raw())?;
        w.write_i32(0)?;
        // Offset 86-93
        w.write_i32(pad.paste_mask_expansion.to_raw())?;
        w.write_i32(pad.solder_mask_expansion.to_raw())?;
        // Offset 94-100: 7 zero bytes
        w.write_fill(0, 7)?;
        // Offset 101-102: manual mask flags
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
        // Offset 103: drill type
        w.write_u8(pad.drill_type as u8)?;
        // Offset 104-105
        w.write_i16(0)?;
        // Offset 106-109
        w.write_i32(0)?;
        // Offset 110-111: jumper id
        w.write_i16(pad.jumper_id as i16)?;
        // Offset 112-113
        w.write_i16(0)?;
        Ok(())
    })?;

    if !pad.has_size_shape_block {
        bw.write_block(|_| Ok(()))?;
        return Ok(());
    }
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
    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: via.is_locked,
            is_tenting_top: via.is_tenting_top,
            is_tenting_bottom: via.is_tenting_bottom,
            is_keepout: via.is_keepout,
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
        Ok(())
    })
}

fn write_track<W: Write + Seek>(bw: &mut BinaryWriter<W>, track: &Track) -> Result<()> {
    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: track.is_locked,
            is_tenting_top: track.is_tenting_top,
            is_tenting_bottom: track.is_tenting_bottom,
            is_keepout: track.is_keepout,
        }
        .encode();
        write_common_prefix(w, track.layer as u8, flags)?;
        write_coord_point(w, track.start)?;
        write_coord_point(w, track.end)?;
        w.write_i32(track.width.to_raw())?;
        w.write_u16(track.net_index)?;
        w.write_u8(track.component_index)?;
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

        w.write_i16(0)?;
        w.write_u8(0)?;
        w.write_u8(i32::from(text.text_kind) as u8)?;
        w.write_u8(if text.font_bold { 1 } else { 0 })?;
        w.write_u8(if text.font_italic { 1 } else { 0 })?;
        w.write_font_name(text.font_name.as_deref().unwrap_or("Arial"))?;
        w.write_i32(text.barcode_lr_margin.to_raw())?;
        w.write_i32(text.barcode_tb_margin.to_raw())?;
        w.write_i32(0)?;
        w.write_i32(0)?;
        w.write_u8(0)?;
        w.write_u8(0)?;
        w.write_i32(0)?;
        w.write_i16(0)?;
        w.write_i32(1)?;
        w.write_i32(0)?;
        w.write_u8(if text.is_inverted { 1 } else { 0 })?;
        w.write_i32(text.inverted_border.to_raw())?;
        w.write_i32(wide_string_index)?;
        w.write_i32(0)?;
        w.write_u8(if text.use_inverted_rectangle { 1 } else { 0 })?;
        w.write_i32(text.inverted_rect_width.to_raw())?;
        w.write_i32(text.inverted_rect_height.to_raw())?;
        w.write_u8(i32::from(text.inverted_rect_justification) as u8)?;
        w.write_i32(text.inverted_rect_text_offset.to_raw())?;
        Ok(())
    })?;
    bw.write_pascal_string_block(&text.text)
}

fn write_fill<W: Write + Seek>(bw: &mut BinaryWriter<W>, fill: &Fill) -> Result<()> {
    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: fill.is_locked,
            is_tenting_top: fill.is_tenting_top,
            is_tenting_bottom: fill.is_tenting_bottom,
            is_keepout: fill.is_keepout,
        }
        .encode();
        write_common_prefix_full(w, fill.layer as u8, flags, fill.net_index as i32, -1)?;
        write_coord_point(w, fill.corner1)?;
        write_coord_point(w, fill.corner2)?;
        w.write_f64(fill.rotation)?;
        Ok(())
    })
}

fn write_region<W: Write + Seek>(bw: &mut BinaryWriter<W>, region: &Region) -> Result<()> {
    bw.write_block(|w| {
        let flags = PrimitiveFlags {
            is_locked: region.is_locked,
            is_tenting_top: region.is_tenting_top,
            is_tenting_bottom: region.is_tenting_bottom,
            is_keepout: region.is_keepout,
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
        params.insert("SUBPOLYINDEX", "-1");
        params.insert("UNIONINDEX", region.union_index.to_string());
        params.insert("ARCRESOLUTION", "0.5mil");
        params.insert("ISSHAPEBASED", "FALSE");
        if let Some(net) = &region.net {
            params.insert("NET", net.clone());
        }
        if let Some(uid) = &region.unique_id {
            params.insert("UNIQUEID", uid.clone());
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
        }
        .encode();
        let layer = layer_name_to_byte(&body.layer_name);
        write_common_prefix_full(w, layer, flags, body.net_index as i32, body.component_index)?;
        w.write_u32(0)?;
        w.write_u8(0)?;

        let mut params = ParameterMap::new();
        params.insert("V7_LAYER", body.layer_name.clone());
        params.insert("NAME", body.name.clone().unwrap_or_else(|| " ".to_string()));
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
        params.insert("CAVITYHEIGHT", body.cavity_height.to_string());
        params.insert("STANDOFFHEIGHT", body.standoff_height.to_string());
        params.insert("OVERALLHEIGHT", body.overall_height.to_string());
        params.insert("BODYPROJECTION", body.body_projection.to_string());
        params.insert("BODYCOLOR3D", body.body_color_3d.to_string());
        params.insert("BODYOPACITY3D", format!("{:.3}", body.body_opacity_3d));
        if let Some(id) = &body.identifier {
            params.insert("IDENTIFIER", id.clone());
        }
        params.insert(
            "MODELID",
            format_model_id(body.model_id.as_deref().unwrap_or_default()),
        );
        params.insert("MODEL.CHECKSUM", body.model_checksum.to_string());
        params.insert(
            "MODEL.EMBED",
            if body.model_embed { "TRUE" } else { "FALSE" },
        );
        params.insert("MODEL.NAME", body.model_name.clone().unwrap_or_default());
        params.insert("MODEL.2D.X", body.model_2d_location.x.to_string());
        params.insert("MODEL.2D.Y", body.model_2d_location.y.to_string());
        params.insert(
            "MODEL.2D.ROTATION",
            format!("{:.3}", body.model_2d_rotation),
        );
        params.insert("MODEL.3D.ROTX", format!("{:.3}", body.model_3d_rot_x));
        params.insert("MODEL.3D.ROTY", format!("{:.3}", body.model_3d_rot_y));
        params.insert("MODEL.3D.ROTZ", format!("{:.3}", body.model_3d_rot_z));
        params.insert("MODEL.3D.DZ", body.model_3d_dz.to_string());
        params.insert("MODEL.MODELTYPE", body.model_type.to_string());
        params.insert(
            "MODEL.MODELSOURCE",
            body.model_source.clone().unwrap_or_default(),
        );
        if let Some(t) = &body.texture {
            params.insert("TEXTURE", t.clone());
        }
        // additional_parameters override anything above when present.
        if let Some(extra) = &body.additional_parameters {
            for (k, v) in extra {
                params.insert(k, v.clone());
            }
        }
        write_c_string_param_block(w, &params)?;

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

        write_doc_board(&mut cf, self)?;
        write_doc_wide_strings(&mut cf, self)?;
        write_doc_nets(&mut cf, self)?;
        write_doc_components(&mut cf, self)?;
        write_doc_arcs(&mut cf, self)?;
        write_doc_pads(&mut cf, self)?;
        write_doc_vias(&mut cf, self)?;
        write_doc_tracks(&mut cf, self)?;
        write_doc_texts(&mut cf, self)?;
        write_doc_fills(&mut cf, self)?;
        write_doc_regions(&mut cf, self)?;
        write_doc_component_bodies(&mut cf, self)?;
        write_doc_polygons(&mut cf, self)?;
        write_doc_rules(&mut cf, self)?;
        write_doc_classes(&mut cf, self)?;
        write_doc_differential_pairs(&mut cf, self)?;
        write_doc_rooms(&mut cf, self)?;
        write_doc_embedded_boards(&mut cf, self)?;
        write_additional_streams(&mut cf, "", &self.additional_streams)?;

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

fn write_doc_board(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    let Some(params) = &document.board_parameters else {
        return Ok(());
    };
    cf.create_storage("Board6")?;
    write_storage_header(cf, "Board6/Header", 1)?;
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    let mut map = ParameterMap::new();
    for (k, v) in params {
        map.insert(k, v.clone());
    }
    write_c_string_param_block(&mut bw, &map)?;
    cf.write_stream("Board6/Data", &buf.into_inner())?;
    Ok(())
}

fn write_doc_wide_strings(cf: &mut CompoundFile, document: &Document) -> Result<()> {
    if document.texts.is_empty() {
        return Ok(());
    }
    cf.create_storage("WideStrings6")?;
    write_storage_header(cf, "WideStrings6/Header", 1)?;
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    let mut params = ParameterMap::new();
    for (i, text) in document.texts.iter().enumerate() {
        let encoded = text
            .text
            .chars()
            .map(|c| (c as u32).to_string())
            .collect::<Vec<_>>()
            .join(",");
        params.insert(format!("ENCODEDTEXT{i}").as_str(), encoded);
    }
    write_c_string_param_block(&mut bw, &params)?;
    cf.write_stream("WideStrings6/Data", &buf.into_inner())?;
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
    let collected: Vec<_> = items.into_iter().collect();
    if collected.is_empty() {
        return Ok(());
    }
    cf.create_storage(storage)?;
    write_storage_header(cf, &format!("{storage}/Header"), collected.len() as i32)?;
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    for item in collected.iter() {
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
    write_param_storage(cf, "Rules6", document.rules.iter(), |r, p| {
        super::doc_codec::rule_to_params(r, p);
    })
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
