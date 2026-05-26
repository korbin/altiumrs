//! Reader for `.SchLib` and `.SchDoc` files.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;

use tokio::io::AsyncRead;

use super::binary::{SchRecordType, decode_binary_pin, read_compressed_storage};
use super::codec;
use super::component::{Component, RawRecord};
use super::document::Document;
use super::implementation::Implementation;
use super::library::Library;
use super::primitives::Pin;
use crate::binary::BinaryReader;
use crate::compound::CompoundFile;
use crate::encoding;
use crate::error::Result;
use crate::parameter::ParameterMap;

// Library reader

impl Library {
    /// Parse a `.SchLib` file from an in-memory buffer.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut cf = CompoundFile::open(bytes)?;
        let mut library = Self::default();

        let component_names = read_file_header(&mut cf, &mut library)?;
        let section_keys = read_section_keys(&mut cf)?;
        library.section_keys = section_keys.clone();

        for name in component_names {
            let section_key = section_keys
                .get(&name)
                .cloned()
                .unwrap_or_else(|| section_key_from_name(&name));
            if let Some(component) = read_component(&mut cf, &section_key)? {
                library.components.push(component);
            }
        }

        // Storage stream: embedded images. Try parsing; on failure preserve raw.
        if let Some(bytes) = cf.try_read_stream("Storage")? {
            if let Some(images) = parse_storage_image_data(&bytes) {
                // Store on the library AND assign to matching SchImage records
                // in order.
                let mut iter = images.iter();
                'outer: for component in &mut library.components {
                    for image in &mut component.images {
                        if image.embed_image {
                            if let Some(data) = iter.next() {
                                image.image_data = Some(data.clone());
                            } else {
                                break 'outer;
                            }
                        }
                    }
                }
                library.embedded_images = images;
            } else {
                library.raw_storage_stream = Some(bytes);
            }
        }

        preserve_root_streams(&mut cf, &mut library)?;
        Ok(library)
    }

    pub async fn read(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = tokio::fs::read(path).await?;
        Self::from_bytes(bytes)
    }

    pub async fn read_async<R>(mut reader: R) -> Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Self::from_bytes(bytes)
    }
}

fn section_key_from_name(name: &str) -> String {
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

fn read_file_header(cf: &mut CompoundFile, library: &mut Library) -> Result<Vec<String>> {
    let Some(data) = cf.try_read_stream("FileHeader")? else {
        return Ok(Vec::new());
    };
    let mut br = BinaryReader::new(Cursor::new(data))?;
    let raw = read_param_block(&mut br)?;
    let params = ParameterMap::parse(&raw);

    for (name, value, _) in params.iter() {
        library
            .file_header_parameters
            .insert(name.to_string(), value.to_string());
    }

    let mut names = Vec::new();
    if br.has_more()? {
        let count = br.read_u32()?;
        for _ in 0..count {
            let name = br.read_pascal_string_block()?;
            if !name.is_empty() {
                names.push(name);
            }
        }
    } else {
        // Fall back to LIBREFn parameters.
        if let Some(count) = params.get_i32("COMPCOUNT") {
            for i in 0..count {
                let key = format!("LIBREF{i}");
                if let Some(name) = params.get(&key) {
                    names.push(name.to_string());
                }
            }
        }
    }
    Ok(names)
}

fn read_section_keys(cf: &mut CompoundFile) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    let Some(data) = cf.try_read_stream("SectionKeys")? else {
        return Ok(map);
    };
    if data.is_empty() {
        return Ok(map);
    }
    let mut br = BinaryReader::new(Cursor::new(data))?;
    let raw = read_param_block(&mut br)?;
    let params = ParameterMap::parse(&raw);
    let Some(count) = params.get_i32("KEYCOUNT") else {
        return Ok(map);
    };
    for i in 0..count {
        let lref = format!("LIBREF{i}");
        let skey = format!("SECTIONKEY{i}");
        if let (Some(libref), Some(section_key)) = (params.get(&lref), params.get(&skey)) {
            map.insert(libref.to_string(), section_key.to_string());
        }
    }
    Ok(map)
}

fn preserve_root_streams(cf: &mut CompoundFile, library: &mut Library) -> Result<()> {
    let known: &[&str] = &["FileHeader", "SectionKeys", "Storage"];
    let entries = cf.list_children("/")?;
    for entry in entries {
        if known.iter().any(|n| entry.name.eq_ignore_ascii_case(n)) {
            continue;
        }
        if entry.is_storage {
            if library
                .components
                .iter()
                .any(|c| storage_matches_component(&entry.name, c, &library.section_keys))
            {
                continue;
            }
            let inner = cf.list_children(&entry.name)?;
            for sub in inner {
                if sub.is_stream {
                    let data = cf.read_stream(format!("{}/{}", entry.name, sub.name))?;
                    library
                        .additional_root_streams
                        .insert(format!("{}/{}", entry.name, sub.name), data);
                }
            }
        } else if entry.is_stream {
            let data = cf.read_stream(&entry.name)?;
            library.additional_root_streams.insert(entry.name, data);
        }
    }
    Ok(())
}

fn storage_matches_component(
    storage_name: &str,
    component: &Component,
    section_keys: &BTreeMap<String, String>,
) -> bool {
    let key = section_keys
        .get(&component.name)
        .cloned()
        .unwrap_or_else(|| section_key_from_name(&component.name));
    storage_name.eq_ignore_ascii_case(&key)
}

fn read_component(cf: &mut CompoundFile, section_key: &str) -> Result<Option<Component>> {
    if !cf.is_storage(section_key) {
        return Ok(None);
    }
    let Some(data) = cf.try_read_stream(format!("{section_key}/Data"))? else {
        return Ok(None);
    };

    let mut component = Component::default();

    let mut pin_frac: Option<Vec<u8>> = None;
    let mut pin_symbol_line_width: Option<Vec<u8>> = None;
    let mut additional_streams: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // Capture auxiliary streams.
    let known: &[&str] = &["Data", "PinFrac", "PinSymbolLineWidth"];
    let entries = cf.list_children(section_key)?;
    for entry in entries {
        if entry.name.eq_ignore_ascii_case("Data") {
            continue;
        }
        if entry.name.eq_ignore_ascii_case("PinFrac") && entry.is_stream {
            pin_frac = Some(cf.read_stream(format!("{section_key}/PinFrac"))?);
            continue;
        }
        if entry.name.eq_ignore_ascii_case("PinSymbolLineWidth") && entry.is_stream {
            pin_symbol_line_width =
                Some(cf.read_stream(format!("{section_key}/PinSymbolLineWidth"))?);
            continue;
        }
        if known.iter().any(|n| entry.name.eq_ignore_ascii_case(n)) {
            continue;
        }
        if entry.is_stream {
            let bytes = cf.read_stream(format!("{section_key}/{}", entry.name))?;
            additional_streams.insert(entry.name, bytes);
        } else {
            let inner = cf.list_children(format!("{section_key}/{}", entry.name))?;
            for sub in inner {
                if sub.is_stream {
                    let bytes =
                        cf.read_stream(format!("{section_key}/{}/{}", entry.name, sub.name))?;
                    additional_streams.insert(format!("{}/{}", entry.name, sub.name), bytes);
                }
            }
        }
    }
    component.additional_streams = additional_streams;

    // Read every record from Data.
    let mut br = BinaryReader::new(Cursor::new(data))?;
    let mut is_first = true;
    let mut current_implementation_owner: Option<usize> = None;

    while br.has_more()? {
        let (flag, body) = br.read_block_with_flags()?;
        if body.is_empty() {
            continue;
        }
        let params = if flag == 0x01 {
            // Binary pin record.
            match decode_binary_pin(&body)? {
                Some(p) => p,
                None => {
                    component.raw_records.push(RawRecord { flag, bytes: body });
                    continue;
                }
            }
        } else {
            let raw = decode_param_body(&body);
            ParameterMap::parse(&raw)
        };

        let record = params.get_i32("RECORD");

        if is_first {
            if record == Some(SchRecordType::Component as i32) {
                codec::component_apply_record(&mut component, &params);
            } else {
                // Header didn't start with a Component record; preserve.
                component.raw_records.push(RawRecord { flag, bytes: body });
            }
            is_first = false;
            continue;
        }

        let dispatched = match record.and_then(SchRecordType::from_i32) {
            Some(SchRecordType::Pin) => {
                component.pins.push(codec::pin_from_params(&params));
                true
            }
            Some(SchRecordType::Symbol) => {
                component.symbols.push(codec::symbol_from_params(&params));
                true
            }
            Some(SchRecordType::Label) => {
                component.labels.push(codec::label_from_params(&params));
                true
            }
            Some(SchRecordType::Bezier) => {
                component.beziers.push(codec::bezier_from_params(&params));
                true
            }
            Some(SchRecordType::Polyline) => {
                component
                    .polylines
                    .push(codec::polyline_from_params(&params));
                true
            }
            Some(SchRecordType::Polygon) => {
                component.polygons.push(codec::polygon_from_params(&params));
                true
            }
            Some(SchRecordType::Ellipse) => {
                component.ellipses.push(codec::ellipse_from_params(&params));
                true
            }
            Some(SchRecordType::Pie) => {
                component.pies.push(codec::pie_from_params(&params));
                true
            }
            Some(SchRecordType::RoundedRectangle) => {
                component
                    .rounded_rectangles
                    .push(codec::rounded_rectangle_from_params(&params));
                true
            }
            Some(SchRecordType::EllipticalArc) => {
                component
                    .elliptical_arcs
                    .push(codec::elliptical_arc_from_params(&params));
                true
            }
            Some(SchRecordType::Arc) => {
                component.arcs.push(codec::arc_from_params(&params));
                true
            }
            Some(SchRecordType::Line) => {
                component.lines.push(codec::line_from_params(&params));
                true
            }
            Some(SchRecordType::Rectangle) => {
                component
                    .rectangles
                    .push(codec::rectangle_from_params(&params));
                true
            }
            Some(SchRecordType::PowerObject) => {
                component
                    .power_objects
                    .push(codec::power_object_from_params(&params));
                true
            }
            Some(SchRecordType::NetLabel) => {
                component
                    .net_labels
                    .push(codec::net_label_from_params(&params));
                true
            }
            Some(SchRecordType::Wire) => {
                component.wires.push(codec::wire_from_params(&params));
                true
            }
            Some(SchRecordType::TextFrame) => {
                component
                    .text_frames
                    .push(codec::text_frame_from_params(&params));
                true
            }
            Some(SchRecordType::Junction) => {
                component
                    .junctions
                    .push(codec::junction_from_params(&params));
                true
            }
            Some(SchRecordType::Image) => {
                component.images.push(codec::image_from_params(&params));
                true
            }
            Some(SchRecordType::Designator) | Some(SchRecordType::Parameter) => {
                component
                    .parameters
                    .push(codec::parameter_from_params(&params));
                true
            }
            Some(SchRecordType::Implementation) => {
                let mut impl_ = codec::implementation_from_params(&params);
                impl_.common.owner_index = component.implementations.len() as i32;
                component.implementations.push(impl_);
                current_implementation_owner = Some(component.implementations.len() - 1);
                true
            }
            Some(SchRecordType::MapDefiner) => {
                let map = codec::map_definer_from_params(&params);
                if let Some(idx) = current_implementation_owner {
                    if let Some(impl_) = component.implementations.get_mut(idx) {
                        impl_.map_definers.push(map);
                    }
                }
                true
            }
            Some(SchRecordType::ImplementationList)
            | Some(SchRecordType::MapDefinerList)
            | Some(SchRecordType::ImplementationParameters) => {
                // Container markers: tracked implicitly by ordering.
                true
            }
            // Records that appear only on documents (not libraries):
            Some(SchRecordType::SheetSymbol)
            | Some(SchRecordType::SheetEntry)
            | Some(SchRecordType::Port)
            | Some(SchRecordType::NoErc)
            | Some(SchRecordType::Bus)
            | Some(SchRecordType::BusEntry)
            | Some(SchRecordType::Blanket)
            | Some(SchRecordType::ParameterSet)
            | Some(SchRecordType::Component) => false,
            None => false,
        };

        if !dispatched {
            component.raw_records.push(RawRecord { flag, bytes: body });
        }
    }

    // Pull Designator/Comment out of typed parameters.
    for p in &component.parameters {
        if p.name.eq_ignore_ascii_case("Designator") {
            component.designator_prefix = Some(p.value.clone());
        } else if p.name.eq_ignore_ascii_case("Comment") {
            component.comment = Some(p.value.clone());
        }
    }

    if let Some(bytes) = pin_frac {
        apply_pin_frac(&mut component.pins, &bytes);
    }
    if let Some(bytes) = pin_symbol_line_width {
        apply_pin_symbol_line_width(&mut component.pins, &bytes);
    }

    Ok(Some(component))
}

fn apply_pin_frac(pins: &mut [Pin], data: &[u8]) {
    let _ = read_compressed_storage(data, |name, decoded| {
        if decoded.len() < 12 {
            return Ok(());
        }
        let Some(idx) = name.parse::<usize>().ok() else {
            return Ok(());
        };
        let Some(pin) = pins.get_mut(idx) else {
            return Ok(());
        };
        let frac_x = i32::from_le_bytes(decoded[0..4].try_into().unwrap());
        let frac_y = i32::from_le_bytes(decoded[4..8].try_into().unwrap());
        let frac_len = i32::from_le_bytes(decoded[8..12].try_into().unwrap());
        if frac_x != 0 {
            pin.location.x = crate::Coord::from_raw(pin.location.x.to_raw() + frac_x);
        }
        if frac_y != 0 {
            pin.location.y = crate::Coord::from_raw(pin.location.y.to_raw() + frac_y);
        }
        if frac_len != 0 {
            pin.length = crate::Coord::from_raw(pin.length.to_raw() + frac_len);
        }
        Ok(())
    });
}

fn apply_pin_symbol_line_width(pins: &mut [Pin], data: &[u8]) {
    let _ = read_compressed_storage(data, |name, decoded| {
        if decoded.len() < 4 {
            return Ok(());
        }
        let Some(idx) = name.parse::<usize>().ok() else {
            return Ok(());
        };
        let Some(pin) = pins.get_mut(idx) else {
            return Ok(());
        };
        // Unicode parameter block: i32 inner_size + UTF-16LE body.
        let inner_size = i32::from_le_bytes(decoded[0..4].try_into().unwrap());
        if inner_size <= 0 || decoded.len() < 4 + inner_size as usize {
            return Ok(());
        }
        let body = &decoded[4..4 + inner_size as usize];
        let mut units = Vec::with_capacity(body.len() / 2);
        for chunk in body.chunks_exact(2) {
            units.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
        let s = String::from_utf16(&units).unwrap_or_default();
        let params = ParameterMap::parse(&s);
        if let Some(lw) = params.get_i32("SYMBOL_LINEWIDTH") {
            pin.symbol_line_width = lw;
        }
        Ok(())
    });
}

fn decode_param_body(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    encoding::decode(&bytes[..end])
}

fn read_param_block<R: Read + std::io::Seek>(br: &mut BinaryReader<R>) -> Result<String> {
    let bytes = br.read_block()?;
    Ok(decode_param_body(&bytes))
}

// Storage stream parsing (embedded images)

fn parse_storage_image_data(data: &[u8]) -> Option<Vec<Vec<u8>>> {
    if data.is_empty() {
        return None;
    }
    let mut images = Vec::new();
    let result = read_compressed_storage(data, |_name, decoded| {
        images.push(decoded.to_vec());
        Ok(())
    });
    result.ok().map(|()| images)
}

// Document reader (.SchDoc)

const SCH_DOC_KNOWN_STREAMS: &[&str] = &["FileHeader", "Storage", "Additional"];

impl Document {
    /// Parse a `.SchDoc` from an in-memory buffer.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut cf = CompoundFile::open(bytes)?;
        let mut document = Self::default();

        if let Some(data) = cf.try_read_stream("FileHeader")? {
            read_document_records(&mut document, &data)?;
        }

        let entries = cf.list_children("/")?;
        for entry in entries {
            if SCH_DOC_KNOWN_STREAMS
                .iter()
                .any(|n| entry.name.eq_ignore_ascii_case(n))
            {
                continue;
            }
            if entry.is_stream {
                let data = cf.read_stream(&entry.name)?;
                document.additional_streams.insert(entry.name, data);
            } else {
                let inner = cf.list_children(&entry.name)?;
                for sub in inner {
                    if sub.is_stream {
                        let data = cf.read_stream(format!("{}/{}", entry.name, sub.name))?;
                        document
                            .additional_streams
                            .insert(format!("{}/{}", entry.name, sub.name), data);
                    }
                }
            }
        }

        Ok(document)
    }

    pub async fn read(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = tokio::fs::read(path).await?;
        Self::from_bytes(bytes)
    }

    pub async fn read_async<R>(mut reader: R) -> Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Self::from_bytes(bytes)
    }
}

fn read_document_records(document: &mut Document, data: &[u8]) -> Result<()> {
    let mut br = BinaryReader::new(Cursor::new(data.to_vec()))?;
    let mut current_component: Option<usize> = None;
    let mut current_implementation: Option<(usize, usize)> = None; // (component_idx, impl_idx)
    let mut sheet_settings: Option<BTreeMap<String, String>> = None;
    let mut header_parameters: Option<BTreeMap<String, String>> = None;
    let mut template_record: Option<BTreeMap<String, String>> = None;
    let mut sheet_name_annotations: Vec<BTreeMap<String, String>> = Vec::new();
    let mut sheet_filename_annotations: Vec<BTreeMap<String, String>> = Vec::new();

    while br.has_more()? {
        let (flag, body) = br.read_block_with_flags()?;
        if body.is_empty() {
            continue;
        }
        let params = if flag == 0x01 {
            match decode_binary_pin(&body)? {
                Some(p) => p,
                None => {
                    document.raw_records.push(RawRecord { flag, bytes: body });

                    continue;
                }
            }
        } else {
            let raw = decode_param_body(&body);
            ParameterMap::parse(&raw)
        };

        let record = params.get_i32("RECORD");

        // The schematic stream has up to two distinct "header" records before
        // any primitive: a file-header parameter block (no `RECORD` key,
        // contains `HEADER=Protel for Windows…`) and the sheet header
        // (`RECORD=31`). Either may appear at index 0; the sheet header may
        // also appear immediately after the file header. Route both into
        // dedicated typed fields, then continue dispatching.
        if record.is_none() && header_parameters.is_none() {
            let mut typed = BTreeMap::new();
            for (n, v, _) in params.iter() {
                typed.insert(n.to_string(), v.to_string());
            }
            header_parameters = Some(typed);

            continue;
        }
        if record == Some(31) && sheet_settings.is_none() {
            let mut typed = BTreeMap::new();
            for (n, v, _) in params.iter() {
                typed.insert(n.to_string(), v.to_string());
            }
            sheet_settings = Some(typed);

            continue;
        }

        // Sheet-level annotation/template records that appear before the
        // first component. These have no entry in `SchRecordType`; we type
        // them via dedicated `Document` fields for round-trip ordering.
        match record {
            Some(39) if template_record.is_none() => {
                let mut typed = BTreeMap::new();
                for (n, v, _) in params.iter() {
                    typed.insert(n.to_string(), v.to_string());
                }
                template_record = Some(typed);
                continue;
            }
            Some(32) => {
                let mut typed = BTreeMap::new();
                for (n, v, _) in params.iter() {
                    typed.insert(n.to_string(), v.to_string());
                }
                sheet_name_annotations.push(typed);
                continue;
            }
            Some(33) => {
                let mut typed = BTreeMap::new();
                for (n, v, _) in params.iter() {
                    typed.insert(n.to_string(), v.to_string());
                }
                sheet_filename_annotations.push(typed);
                continue;
            }
            _ => {}
        }

        let mut handled = true;
        match record.and_then(SchRecordType::from_i32) {
            Some(SchRecordType::Component) => {
                let mut component = Component::default();
                codec::component_apply_record(&mut component, &params);
                document.components.push(component);
                current_component = Some(document.components.len() - 1);
                current_implementation = None;
            }
            Some(SchRecordType::Pin) => {
                if let Some(idx) = current_component {
                    document.components[idx]
                        .pins
                        .push(codec::pin_from_params(&params));
                } else {
                    handled = false;
                }
            }
            Some(SchRecordType::Symbol) => {
                if let Some(idx) = current_component {
                    document.components[idx]
                        .symbols
                        .push(codec::symbol_from_params(&params));
                } else {
                    document.symbols.push(codec::symbol_from_params(&params));
                }
            }
            Some(SchRecordType::Label) => {
                document.labels.push(codec::label_from_params(&params));
            }
            Some(SchRecordType::Bezier) => {
                document.beziers.push(codec::bezier_from_params(&params));
            }
            Some(SchRecordType::Polyline) => {
                document
                    .polylines
                    .push(codec::polyline_from_params(&params));
            }
            Some(SchRecordType::Polygon) => {
                document.polygons.push(codec::polygon_from_params(&params));
            }
            Some(SchRecordType::Ellipse) => {
                document.ellipses.push(codec::ellipse_from_params(&params));
            }
            Some(SchRecordType::Pie) => {
                document.pies.push(codec::pie_from_params(&params));
            }
            Some(SchRecordType::RoundedRectangle) => {
                document
                    .rounded_rectangles
                    .push(codec::rounded_rectangle_from_params(&params));
            }
            Some(SchRecordType::EllipticalArc) => {
                document
                    .elliptical_arcs
                    .push(codec::elliptical_arc_from_params(&params));
            }
            Some(SchRecordType::Arc) => {
                document.arcs.push(codec::arc_from_params(&params));
            }
            Some(SchRecordType::Line) => {
                document.lines.push(codec::line_from_params(&params));
            }
            Some(SchRecordType::Rectangle) => {
                document
                    .rectangles
                    .push(codec::rectangle_from_params(&params));
            }
            Some(SchRecordType::SheetSymbol) => {
                document
                    .sheet_symbols
                    .push(codec::sheet_symbol_from_params(&params));
            }
            Some(SchRecordType::SheetEntry) => {
                let entry = codec::sheet_entry_from_params(&params);
                if let Some(parent) = document.sheet_symbols.last_mut() {
                    parent.entries.push(entry.clone());
                }
                document.sheet_entries.push(entry);
            }
            Some(SchRecordType::PowerObject) => {
                document
                    .power_objects
                    .push(codec::power_object_from_params(&params));
            }
            Some(SchRecordType::Port) => {
                document.ports.push(codec::port_from_params(&params));
            }
            Some(SchRecordType::NoErc) => {
                document.no_ercs.push(codec::no_erc_from_params(&params));
            }
            Some(SchRecordType::NetLabel) => {
                document
                    .net_labels
                    .push(codec::net_label_from_params(&params));
            }
            Some(SchRecordType::Bus) => {
                document.buses.push(codec::bus_from_params(&params));
            }
            Some(SchRecordType::Wire) => {
                document.wires.push(codec::wire_from_params(&params));
            }
            Some(SchRecordType::TextFrame) => {
                document
                    .text_frames
                    .push(codec::text_frame_from_params(&params));
            }
            Some(SchRecordType::Junction) => {
                document
                    .junctions
                    .push(codec::junction_from_params(&params));
            }
            Some(SchRecordType::Image) => {
                document.images.push(codec::image_from_params(&params));
            }
            Some(SchRecordType::BusEntry) => {
                document
                    .bus_entries
                    .push(codec::bus_entry_from_params(&params));
            }
            Some(SchRecordType::Designator) | Some(SchRecordType::Parameter) => {
                let p = codec::parameter_from_params(&params);
                if let Some(idx) = current_component {
                    document.components[idx].parameters.push(p);
                } else {
                    document.parameters.push(p);
                }
            }
            Some(SchRecordType::ParameterSet) => {
                document
                    .parameter_sets
                    .push(codec::parameter_set_from_params(&params));
            }
            Some(SchRecordType::Blanket) => {
                document.blankets.push(codec::blanket_from_params(&params));
            }
            Some(SchRecordType::Implementation) => {
                if let Some(idx) = current_component {
                    let mut impl_ = codec::implementation_from_params(&params);
                    impl_.common.owner_index =
                        document.components[idx].implementations.len() as i32;
                    document.components[idx].implementations.push(impl_);
                    current_implementation =
                        Some((idx, document.components[idx].implementations.len() - 1));
                } else {
                    handled = false;
                }
            }
            Some(SchRecordType::MapDefiner) => {
                let map = codec::map_definer_from_params(&params);
                if let Some((cidx, iidx)) = current_implementation {
                    if let Some(impl_) = document
                        .components
                        .get_mut(cidx)
                        .and_then(|c| c.implementations.get_mut(iidx))
                    {
                        impl_.map_definers.push(map);
                    }
                } else {
                    handled = false;
                }
            }
            Some(SchRecordType::ImplementationList)
            | Some(SchRecordType::MapDefinerList)
            | Some(SchRecordType::ImplementationParameters) => {
                // Container markers.
            }
            None => handled = false,
        }

        if !handled {
            document.raw_records.push(RawRecord { flag, bytes: body });
        }
    }

    document.header_parameters = header_parameters;
    document.sheet_settings = sheet_settings;
    document.template_record = template_record;
    document.sheet_name_annotations = sheet_name_annotations;
    document.sheet_filename_annotations = sheet_filename_annotations;
    Ok(())
}

#[allow(dead_code)]
fn _force_imports(_x: Implementation) {}
