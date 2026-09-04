//! Writer for `.SchLib` and `.SchDoc` files.

use std::collections::BTreeMap;
use std::io::{Cursor, Seek, Write};
use std::path::Path;

use tokio::io::AsyncWrite;

use super::binary::{coord_to_dxp_frac, write_compressed_storage};
use super::codec;
use super::component::{Component, RawRecord};
use super::document::Document;
use super::library::Library;
use super::primitives::Pin;
use crate::binary::BinaryWriter;
use crate::compound::CompoundFile;
use crate::error::Result;
use crate::parameter::ParameterMap;

// Library writer

impl Library {
    /// Serialise to a `.SchLib` byte buffer.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut cf = CompoundFile::create()?;

        // Resolve a storage section key for each component, honoring any
        // explicit overrides in `self.section_keys` and otherwise deriving
        // one from the component name (sanitizing illegal OLE / Altium chars).
        let mut effective_section_keys: BTreeMap<String, String> = self.section_keys.clone();
        for component in &self.components {
            if effective_section_keys.contains_key(&component.name) {
                continue;
            }
            let derived = section_key_from_name(&component.name);
            if derived != component.name {
                effective_section_keys.insert(component.name.clone(), derived);
            }
        }

        write_file_header(&mut cf, self)?;
        if !effective_section_keys.is_empty() {
            write_section_keys(&mut cf, &effective_section_keys)?;
        }

        for component in &self.components {
            let section_key = effective_section_keys
                .get(&component.name)
                .cloned()
                .unwrap_or_else(|| component.name.clone());
            write_component(&mut cf, component, &section_key)?;
        }

        write_storage_stream(&mut cf, self)?;

        for (path, data) in &self.additional_root_streams {
            cf.write_stream(path, data)?;
        }

        cf.into_bytes()
    }

    pub async fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = self.to_bytes()?;
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

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
    // OLE compound files and Altium itself reject several characters in
    // storage stream names. Replace any of `/ \ : ! * ? " < > |` with `_`
    // before truncating to the 31-char OLE limit.
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '!' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    if sanitized.len() > 31 {
        sanitized.chars().take(31).collect()
    } else {
        sanitized
    }
}

fn write_file_header(cf: &mut CompoundFile, library: &Library) -> Result<()> {
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);

    let mut params = ParameterMap::new();
    populate_default_file_header(&mut params, library);
    for (k, v) in &library.file_header_parameters {
        params.insert(k, v.clone());
    }
    write_c_string_param_block(&mut bw, &params)?;

    if params.get("COMPCOUNT").is_none() {
        bw.write_u32(library.components.len() as u32)?;
        for component in &library.components {
            bw.write_pascal_string_block(&component.name)?;
        }
    }

    cf.write_stream("FileHeader", &buf.into_inner())?;
    Ok(())
}

fn populate_default_file_header(params: &mut ParameterMap, library: &Library) {
    params.insert(
        "HEADER",
        "Protel for Windows - Schematic Library Editor Binary File Version 5.0",
    );
    params.insert("Weight", (library.components.len() as i32 + 1).to_string());
    params.insert("MinorVersion", "9");
    params.insert("UniqueID", "AAAAAAAA");

    if let Some(custom) = &library.font_override {
        params.insert("FontIdCount", "3");
        params.insert("Size1", "10");
        params.insert("FontName1", "Times New Roman");
        params.insert("Size2", "10");
        params.insert("FontName2", custom.clone());
        params.insert("Size3", "10");
        params.insert("Rotation3", "90");
        params.insert("FontName3", custom.clone());
    } else {
        params.insert("FontIdCount", "1");
        params.insert("Size1", "10");
        params.insert("FontName1", "Times New Roman");
    }

    params.insert("UseMBCS", "T");
    params.insert("IsBOC", "T");
    params.insert("SheetStyle", "9");
    params.insert("BorderOn", "T");
    params.insert("SheetNumberSpaceSize", "12");
    params.insert("AreaColor", "16317695");
    params.insert("SnapGridOn", "T");
    params.insert("SnapGridSize", "10");
    params.insert("VisibleGridOn", "T");
    params.insert("VisibleGridSize", "10");
    params.insert("CustomX", "18000");
    params.insert("CustomY", "18000");
    params.insert("UseCustomSheet", "T");
    params.insert("ReferenceZonesOn", "T");
    params.insert("Display_Unit", "0");

    params.insert("CompCount", library.components.len().to_string());
    for (i, component) in library.components.iter().enumerate() {
        params.insert(format!("LibRef{i}").as_str(), component.name.clone());
        if let Some(desc) = &component.description {
            params.insert(format!("CompDescr{i}").as_str(), desc.clone());
        }
        let parts = component.part_count.max(1) + 1;
        params.insert(format!("PartCount{i}").as_str(), parts.to_string());
    }
}

fn write_section_keys(
    cf: &mut CompoundFile,
    section_keys: &BTreeMap<String, String>,
) -> Result<()> {
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    let mut params = ParameterMap::new();
    params.insert("KEYCOUNT", section_keys.len().to_string());
    for (i, (libref, key)) in section_keys.iter().enumerate() {
        params.insert(format!("LIBREF{i}").as_str(), libref.clone());
        params.insert(format!("SECTIONKEY{i}").as_str(), key.clone());
    }
    write_c_string_param_block(&mut bw, &params)?;
    cf.write_stream("SectionKeys", &buf.into_inner())?;
    Ok(())
}

fn write_component(cf: &mut CompoundFile, component: &Component, section_key: &str) -> Result<()> {
    cf.create_storage(section_key)?;

    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);

    write_component_records(&mut bw, component)?;

    cf.write_stream(format!("{section_key}/Data"), &buf.into_inner())?;

    // Auxiliary streams from typed pins.
    if let Some(pin_frac) = build_pin_frac(&component.pins)? {
        cf.write_stream(format!("{section_key}/PinFrac"), &pin_frac)?;
    }
    if let Some(pin_lw) = build_pin_symbol_line_width(&component.pins)? {
        cf.write_stream(format!("{section_key}/PinSymbolLineWidth"), &pin_lw)?;
    }
    if let Some(pin_text) = build_pin_text_data(&component.pins)? {
        cf.write_stream(format!("{section_key}/PinTextData"), &pin_text)?;
    }

    for (name, data) in &component.additional_streams {
        cf.write_stream(format!("{section_key}/{name}"), data)?;
    }
    Ok(())
}

fn write_component_records<W: Write + Seek>(
    bw: &mut BinaryWriter<W>,
    component: &Component,
) -> Result<()> {
    // RECORD=1 (Component) goes first.
    let mut params = ParameterMap::new();
    params.insert("RECORD", "1");
    codec::component_to_record_params(component, &mut params);
    emit_param_record(bw, &params)?;

    // Body shapes are emitted before pins so the body renders underneath.
    for rect in &component.rectangles {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "14");
        codec::rectangle_to_params(rect, &mut p);
        emit_param_record(bw, &p)?;
    }
    for bz in &component.beziers {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "5");
        codec::bezier_to_params(bz, &mut p);
        emit_param_record(bw, &p)?;
    }
    for poly in &component.polylines {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "6");
        codec::polyline_to_params(poly, &mut p);
        emit_param_record(bw, &p)?;
    }
    for poly in &component.polygons {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "7");
        codec::polygon_to_params(poly, &mut p);
        emit_param_record(bw, &p)?;
    }
    for e in &component.ellipses {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "8");
        codec::ellipse_to_params(e, &mut p);
        emit_param_record(bw, &p)?;
    }
    for pie in &component.pies {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "9");
        codec::pie_to_params(pie, &mut p);
        emit_param_record(bw, &p)?;
    }
    for r in &component.rounded_rectangles {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "10");
        codec::rounded_rectangle_to_params(r, &mut p);
        emit_param_record(bw, &p)?;
    }
    for ea in &component.elliptical_arcs {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "11");
        codec::elliptical_arc_to_params(ea, &mut p);
        emit_param_record(bw, &p)?;
    }
    for arc in &component.arcs {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "12");
        codec::arc_to_params(arc, &mut p);
        emit_param_record(bw, &p)?;
    }
    for line in &component.lines {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "13");
        codec::line_to_params(line, &mut p);
        emit_param_record(bw, &p)?;
    }

    // Pins go out as binary records (flag 0x01); per-pin font/color overrides
    // are carried in the sibling PinTextData stream.
    for pin in &component.pins {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "2");
        codec::pin_to_params(pin, &mut p);
        let body = super::binary::encode_binary_pin(&p);
        bw.write_block_with_flags(0x01, |w| {
            w.write_bytes(&body)?;
            Ok(())
        })?;
    }
    for sym in &component.symbols {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "3");
        codec::symbol_to_params(sym, &mut p);
        emit_param_record(bw, &p)?;
    }
    for label in &component.labels {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "4");
        codec::label_to_params(label, &mut p);
        emit_param_record(bw, &p)?;
    }
    for po in &component.power_objects {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "17");
        codec::power_object_to_params(po, &mut p);
        emit_param_record(bw, &p)?;
    }
    for nl in &component.net_labels {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "25");
        codec::net_label_to_params(nl, &mut p);
        emit_param_record(bw, &p)?;
    }
    for w in &component.wires {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "27");
        codec::wire_to_params(w, &mut p);
        emit_param_record(bw, &p)?;
    }
    for tf in &component.text_frames {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "28");
        codec::text_frame_to_params(tf, &mut p);
        emit_param_record(bw, &p)?;
    }
    for j in &component.junctions {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "29");
        codec::junction_to_params(j, &mut p);
        emit_param_record(bw, &p)?;
    }
    for img in &component.images {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "30");
        codec::image_to_params(img, &mut p);
        emit_param_record(bw, &p)?;
    }
    for param in &component.parameters {
        let mut p = ParameterMap::new();
        // Real Altium emits the canonical Designator parameter as a typed
        // RECORD=34 record (matching SchRecordType::Designator). All other
        // parameters use RECORD=41. The byte layout is identical, so we just
        // pick the right header tag and route through the same codec.
        let record_id = if param.name.eq_ignore_ascii_case("Designator") {
            "34"
        } else {
            "41"
        };
        p.insert("RECORD", record_id);
        codec::parameter_to_params(param, &mut p);
        emit_param_record(bw, &p)?;
    }

    // Implementation hierarchy: ImplementationList → Implementation* →
    //   MapDefinerList → MapDefiner* → ImplementationParameters
    if !component.implementations.is_empty() {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "44");
        emit_param_record(bw, &p)?;
        for impl_ in &component.implementations {
            let mut p = ParameterMap::new();
            codec::implementation_to_params(impl_, &mut p);
            emit_param_record(bw, &p)?;
            if !impl_.map_definers.is_empty() {
                let mut p = ParameterMap::new();
                p.insert("RECORD", "46");
                emit_param_record(bw, &p)?;
                for map in &impl_.map_definers {
                    let mut p = ParameterMap::new();
                    codec::map_definer_to_params(map, &mut p);
                    emit_param_record(bw, &p)?;
                }
            }
            let mut p = ParameterMap::new();
            p.insert("RECORD", "48");
            emit_param_record(bw, &p)?;
        }
    }

    // Replay any unrecognised raw records last.
    for record in &component.raw_records {
        emit_raw_record(bw, record)?;
    }
    Ok(())
}

fn emit_param_record<W: Write + Seek>(
    bw: &mut BinaryWriter<W>,
    params: &ParameterMap,
) -> Result<()> {
    bw.write_block_with_flags(0, |w| {
        let mut bytes = Vec::<u8>::new();
        crate::parameter::write_block_bytes(&mut bytes, params, '|');
        w.write_bytes(&bytes)?;
        w.write_u8(0)?;
        Ok(())
    })
}

fn emit_raw_record<W: Write + Seek>(bw: &mut BinaryWriter<W>, record: &RawRecord) -> Result<()> {
    bw.write_block_with_flags(record.flag, |w| {
        w.write_bytes(&record.bytes)?;
        Ok(())
    })
}

// Auxiliary streams

fn build_pin_frac(pins: &[Pin]) -> Result<Option<Vec<u8>>> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, pin) in pins.iter().enumerate() {
        let (_, fx) = coord_to_dxp_frac(pin.location.x);
        let (_, fy) = coord_to_dxp_frac(pin.location.y);
        let (_, fl) = coord_to_dxp_frac(pin.length);
        if fx == 0 && fy == 0 && fl == 0 {
            continue;
        }
        let mut body = Vec::with_capacity(12);
        body.extend_from_slice(&fx.to_le_bytes());
        body.extend_from_slice(&fy.to_le_bytes());
        body.extend_from_slice(&fl.to_le_bytes());
        entries.push((i.to_string(), body));
    }
    if entries.is_empty() {
        return Ok(None);
    }
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    let mut header = ParameterMap::new();
    header.insert("HEADER", "PinFrac");
    header.insert("Weight", entries.len().to_string());
    write_compressed_storage(&mut bw, &header, &entries)?;
    Ok(Some(buf.into_inner()))
}

/// Per-pin `PinTextData` stream: 14 bytes per pin (7 for designator, 7 for
/// name) carrying the custom font/color toggles, zlib-compressed.
fn build_pin_text_data(pins: &[Pin]) -> Result<Option<Vec<u8>>> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, pin) in pins.iter().enumerate() {
        let designator_segment =
            pin_text_data_segment(pin.designator_font_mode, pin.designator_custom_font_id);
        let name_segment = pin_text_data_segment(pin.name_font_mode, pin.name_custom_font_id);
        if designator_segment == [0u8; 7] && name_segment == [0u8; 7] {
            continue;
        }
        let mut body = Vec::with_capacity(14);
        body.extend_from_slice(&designator_segment);
        body.extend_from_slice(&name_segment);
        entries.push((i.to_string(), body));
    }
    if entries.is_empty() {
        return Ok(None);
    }
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    let mut header = ParameterMap::new();
    header.insert("HEADER", "PinTextData");
    header.insert("Weight", entries.len().to_string());
    write_compressed_storage(&mut bw, &header, &entries)?;
    Ok(Some(buf.into_inner()))
}

fn pin_text_data_segment(font_mode: i32, custom_font_id: i32) -> [u8; 7] {
    if font_mode == 0 || custom_font_id == 0 {
        return [0u8; 7];
    }
    // 0x10 = "custom font enabled" bit; remaining bytes are color + position.
    [0x10, custom_font_id as u8, 0, 0, 0, 0, 0]
}

fn build_pin_symbol_line_width(pins: &[Pin]) -> Result<Option<Vec<u8>>> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, pin) in pins.iter().enumerate() {
        if pin.symbol_line_width == 0 {
            continue;
        }
        let body = format!("|SYMBOL_LINEWIDTH={}", pin.symbol_line_width);
        let utf16: Vec<u16> = body.encode_utf16().collect();
        let mut body_bytes = Vec::with_capacity(4 + utf16.len() * 2);
        body_bytes.extend_from_slice(&((utf16.len() * 2) as i32).to_le_bytes());
        for u in utf16 {
            body_bytes.extend_from_slice(&u.to_le_bytes());
        }
        entries.push((i.to_string(), body_bytes));
    }
    if entries.is_empty() {
        return Ok(None);
    }
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    let mut header = ParameterMap::new();
    header.insert("HEADER", "PinSymbolLineWidth");
    header.insert("Weight", entries.len().to_string());
    write_compressed_storage(&mut bw, &header, &entries)?;
    Ok(Some(buf.into_inner()))
}

fn write_storage_stream(cf: &mut CompoundFile, library: &Library) -> Result<()> {
    // Prefer rebuilding from the per-image byte data; fall back to raw passthrough.
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut idx = 0;
    for component in &library.components {
        for image in &component.images {
            if image.embed_image {
                if let Some(bytes) = &image.image_data {
                    entries.push((idx.to_string(), bytes.clone()));
                    idx += 1;
                }
            }
        }
    }
    if !entries.is_empty() {
        let mut buf = Cursor::new(Vec::<u8>::new());
        let mut bw = BinaryWriter::new(&mut buf);
        let mut header = ParameterMap::new();
        header.insert("HEADER", "Icon storage");
        header.insert("Weight", entries.len().to_string());
        write_compressed_storage(&mut bw, &header, &entries)?;
        cf.write_stream("Storage", &buf.into_inner())?;
        return Ok(());
    }
    if let Some(raw) = &library.raw_storage_stream {
        cf.write_stream("Storage", raw)?;
        return Ok(());
    }

    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    let mut header = ParameterMap::new();
    header.insert("HEADER", "Icon storage");
    write_c_string_param_block(&mut bw, &header)?;
    cf.write_stream("Storage", &buf.into_inner())?;
    Ok(())
}

// Document writer (.SchDoc)

impl Document {
    /// Serialise to a `.SchDoc` byte buffer.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut cf = CompoundFile::create()?;

        let mut buf = Cursor::new(Vec::<u8>::new());
        let mut bw = BinaryWriter::new(&mut buf);

        if let Some(header) = &self.header_parameters {
            let mut p = ParameterMap::new();
            for (k, v) in header {
                p.insert(k, v.clone());
            }
            emit_param_record(&mut bw, &p)?;
        }
        if let Some(sheet) = &self.sheet_settings {
            let mut p = ParameterMap::new();
            p.insert("RECORD", "31");
            for (k, v) in sheet {
                p.insert(k, v.clone());
            }
            emit_param_record(&mut bw, &p)?;
        }
        if let Some(template) = &self.template_record {
            let mut p = ParameterMap::new();
            p.insert("RECORD", "39");
            for (k, v) in template {
                p.insert(k, v.clone());
            }
            emit_param_record(&mut bw, &p)?;
        }
        for ann in &self.sheet_name_annotations {
            let mut p = ParameterMap::new();
            p.insert("RECORD", "32");
            for (k, v) in ann {
                p.insert(k, v.clone());
            }
            emit_param_record(&mut bw, &p)?;
        }
        for ann in &self.sheet_filename_annotations {
            let mut p = ParameterMap::new();
            p.insert("RECORD", "33");
            for (k, v) in ann {
                p.insert(k, v.clone());
            }
            emit_param_record(&mut bw, &p)?;
        }

        // Top-level primitives that don't belong to a component.
        for label in &self.labels {
            emit_typed(&mut bw, "4", |p| codec::label_to_params(label, p))?;
        }
        for sym in &self.symbols {
            emit_typed(&mut bw, "3", |p| codec::symbol_to_params(sym, p))?;
        }
        for bz in &self.beziers {
            emit_typed(&mut bw, "5", |p| codec::bezier_to_params(bz, p))?;
        }
        for poly in &self.polylines {
            emit_typed(&mut bw, "6", |p| codec::polyline_to_params(poly, p))?;
        }
        for poly in &self.polygons {
            emit_typed(&mut bw, "7", |p| codec::polygon_to_params(poly, p))?;
        }
        for e in &self.ellipses {
            emit_typed(&mut bw, "8", |p| codec::ellipse_to_params(e, p))?;
        }
        for pie in &self.pies {
            emit_typed(&mut bw, "9", |p| codec::pie_to_params(pie, p))?;
        }
        for r in &self.rounded_rectangles {
            emit_typed(&mut bw, "10", |p| codec::rounded_rectangle_to_params(r, p))?;
        }
        for ea in &self.elliptical_arcs {
            emit_typed(&mut bw, "11", |p| codec::elliptical_arc_to_params(ea, p))?;
        }
        for arc in &self.arcs {
            emit_typed(&mut bw, "12", |p| codec::arc_to_params(arc, p))?;
        }
        for line in &self.lines {
            emit_typed(&mut bw, "13", |p| codec::line_to_params(line, p))?;
        }
        for rect in &self.rectangles {
            emit_typed(&mut bw, "14", |p| codec::rectangle_to_params(rect, p))?;
        }
        for sym in &self.sheet_symbols {
            emit_typed(&mut bw, "15", |p| codec::sheet_symbol_to_params(sym, p))?;
            for entry in &sym.entries {
                emit_typed(&mut bw, "16", |p| codec::sheet_entry_to_params(entry, p))?;
            }
        }
        for po in &self.power_objects {
            emit_typed(&mut bw, "17", |p| codec::power_object_to_params(po, p))?;
        }
        for port in &self.ports {
            emit_typed(&mut bw, "18", |p| codec::port_to_params(port, p))?;
        }
        for n in &self.no_ercs {
            emit_typed(&mut bw, "22", |p| codec::no_erc_to_params(n, p))?;
        }
        for nl in &self.net_labels {
            emit_typed(&mut bw, "25", |p| codec::net_label_to_params(nl, p))?;
        }
        for bus in &self.buses {
            emit_typed(&mut bw, "26", |p| codec::bus_to_params(bus, p))?;
        }
        for w in &self.wires {
            emit_typed(&mut bw, "27", |p| codec::wire_to_params(w, p))?;
        }
        for tf in &self.text_frames {
            emit_typed(&mut bw, "28", |p| codec::text_frame_to_params(tf, p))?;
        }
        for j in &self.junctions {
            emit_typed(&mut bw, "29", |p| codec::junction_to_params(j, p))?;
        }
        for img in &self.images {
            emit_typed(&mut bw, "30", |p| codec::image_to_params(img, p))?;
        }
        for be in &self.bus_entries {
            emit_typed(&mut bw, "37", |p| codec::bus_entry_to_params(be, p))?;
        }
        for param in &self.parameters {
            emit_typed(&mut bw, "41", |p| codec::parameter_to_params(param, p))?;
        }
        for ps in &self.parameter_sets {
            emit_typed(&mut bw, "43", |p| codec::parameter_set_to_params(ps, p))?;
        }
        for b in &self.blankets {
            emit_typed(&mut bw, "225", |p| codec::blanket_to_params(b, p))?;
        }

        // Components and their owned children.
        for component in &self.components {
            write_component_records(&mut bw, component)?;
        }

        // Replay any unhandled raw records last.
        for record in &self.raw_records {
            emit_raw_record(&mut bw, record)?;
        }

        cf.write_stream("FileHeader", &buf.into_inner())?;

        // Harness connectors (215) with their entries (216) / type label
        // (217) and signal harnesses (218) live in the separate `Additional`
        // stream, behind a header whose `Weight` is the record count.
        if self.additional_header_parameters.is_some()
            || !self.harness_connectors.is_empty()
            || !self.signal_harnesses.is_empty()
        {
            let mut abuf = Cursor::new(Vec::<u8>::new());
            {
                let mut abw = BinaryWriter::new(&mut abuf);
                let count: usize = self
                    .harness_connectors
                    .iter()
                    .map(|hc| 1 + hc.entries.len() + usize::from(hc.harness_type.is_some()))
                    .sum::<usize>()
                    + self.signal_harnesses.len();
                let mut header = ParameterMap::new();
                match &self.additional_header_parameters {
                    Some(h) => {
                        for (k, v) in h {
                            header.insert(k, v.clone());
                        }
                    }
                    None => header.insert(
                        "HEADER",
                        "Protel for Windows - Schematic Capture Binary File Version 5.0",
                    ),
                }
                header.insert("Weight", count.to_string());
                emit_param_record(&mut abw, &header)?;
                for hc in &self.harness_connectors {
                    emit_typed(&mut abw, "215", |p| codec::harness_connector_to_params(hc, p))?;
                    for entry in &hc.entries {
                        emit_typed(&mut abw, "216", |p| codec::harness_entry_to_params(entry, p))?;
                    }
                    if let Some(ht) = &hc.harness_type {
                        emit_typed(&mut abw, "217", |p| codec::harness_type_to_params(ht, p))?;
                    }
                }
                for sh in &self.signal_harnesses {
                    emit_typed(&mut abw, "218", |p| codec::signal_harness_to_params(sh, p))?;
                }
            }
            cf.write_stream("Additional", &abuf.into_inner())?;
        }

        for (path, data) in &self.additional_streams {
            cf.write_stream(path, data)?;
        }

        cf.into_bytes()
    }

    pub async fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = self.to_bytes()?;
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

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

fn emit_typed<W, F>(bw: &mut BinaryWriter<W>, record: &str, fill: F) -> Result<()>
where
    W: Write + Seek,
    F: FnOnce(&mut ParameterMap),
{
    let mut params = ParameterMap::new();
    params.insert("RECORD", record);
    fill(&mut params);
    emit_param_record(bw, &params)
}

// Helpers

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
