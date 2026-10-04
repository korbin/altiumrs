//! Reader for `.SchLib` and `.SchDoc` files.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
#[cfg(feature = "async")]
use std::path::Path;

#[cfg(feature = "async")]
use tokio::io::AsyncRead;

use super::binary::{
    PinTextCustomisation, SchRecordType, decode_binary_pin, decode_pin_text_customisation,
    read_compressed_storage,
};
use super::codec;
use super::component::{Component, RawRecord};
use super::document::Document;
use super::implementation::Implementation;
use super::library::Library;
use super::primitives::Pin;
use crate::binary::BinaryReader;
use crate::compound::CompoundFile;
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

    #[cfg(feature = "async")]
    pub async fn read(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = tokio::fs::read(path).await?;
        Self::from_bytes(bytes)
    }

    #[cfg(feature = "async")]
    pub async fn read_async<R>(mut reader: R) -> Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        #[cfg(feature = "async")]
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
    let params = read_param_block(&mut br)?;

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
    let params = read_param_block(&mut br)?;
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
    let mut pin_text_data: Option<Vec<u8>> = None;
    let mut pin_propagation_delay: Option<Vec<u8>> = None;
    let mut additional_streams: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // Capture auxiliary streams.
    let known: &[&str] = &["Data", "PinFrac", "PinSymbolLineWidth", "PinTextData", "PinPropagationDelay"];
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
        if entry.name.eq_ignore_ascii_case("PinTextData") && entry.is_stream {
            pin_text_data = Some(cf.read_stream(format!("{section_key}/PinTextData"))?);
            continue;
        }
        if entry.name.eq_ignore_ascii_case("PinPropagationDelay") && entry.is_stream {
            pin_propagation_delay = Some(cf.read_stream(format!("{section_key}/{}", entry.name))?);
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
            decode_param_body(&body)
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
            | Some(SchRecordType::HarnessConnector)
            | Some(SchRecordType::HarnessEntry)
            | Some(SchRecordType::HarnessType)
            | Some(SchRecordType::SignalHarness)
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
    if let Some(bytes) = pin_text_data {
        apply_pin_text_data(&mut component.pins, &bytes);
    }
    if let Some(bytes) = pin_propagation_delay {
        // Entries that would not be rewritten byte for byte keep the stream raw.
        match decode_pin_propagation_delay(&bytes, component.pins.len()) {
            Some(delays) => {
                for (idx, seconds) in delays {
                    component.pins[idx].pin_propagation_delay = seconds;
                }
            }
            None => {
                component.additional_streams.insert("PinPropagationDelay".to_string(), bytes);
            }
        }
    }

    Ok(Some(component))
}

/// Per-pin font / colour / position customisations. Altium's canvas reads
/// these from the `PinTextData` stream (indexed by pin position), not from
/// the pin record, so this is where the typed `*_font_mode`,
/// `*_custom_font_id`, `*_custom_color` and `*_position_mode` fields come
/// from. The writer regenerates the stream from those fields.
fn apply_pin_text_data(pins: &mut [Pin], data: &[u8]) {
    let _ = read_compressed_storage(data, |name, decoded| {
        let Some(idx) = name.parse::<usize>().ok() else {
            return Ok(());
        };
        let Some(pin) = pins.get_mut(idx) else {
            return Ok(());
        };
        let mut pos = 0usize;
        let Some(designator) = decode_pin_text_customisation(decoded, &mut pos) else {
            return Ok(());
        };
        let Some(name_text) = decode_pin_text_customisation(decoded, &mut pos) else {
            return Ok(());
        };
        apply_pin_text_customisation(
            &designator,
            &mut pin.designator_font_mode,
            &mut pin.designator_custom_font_id,
            &mut pin.designator_custom_color,
            &mut pin.designator_position_mode,
            &mut pin.designator_custom_position_margin,
            &mut pin.designator_custom_position_rotation_anchor,
            &mut pin.designator_custom_position_rotation_relative,
        );
        apply_pin_text_customisation(
            &name_text,
            &mut pin.name_font_mode,
            &mut pin.name_custom_font_id,
            &mut pin.name_custom_color,
            &mut pin.name_position_mode,
            &mut pin.name_custom_position_margin,
            &mut pin.name_custom_position_rotation_anchor,
            &mut pin.name_custom_position_rotation_relative,
        );
        Ok(())
    });
}

#[allow(clippy::too_many_arguments)]
fn apply_pin_text_customisation(
    c: &PinTextCustomisation,
    font_mode: &mut i32,
    custom_font_id: &mut i32,
    custom_color: &mut i32,
    position_mode: &mut i32,
    margin: &mut i32,
    rotation_anchor: &mut i32,
    rotation_relative: &mut bool,
) {
    if c.custom_font {
        *font_mode = 1;
        *custom_font_id = i32::from(c.font_id);
        *custom_color = c.color;
    }
    if c.custom_position {
        *position_mode = 1;
        *margin = c.margin;
    }
    *rotation_anchor = i32::from(c.rotation_anchor);
    *rotation_relative = c.rotation_relative;
}

/// `PinPropagationDelay`: one zlib entry per pin that has a delay, keyed by
/// the pin's index; each entry is a `u32` byte length and UTF-16LE parameter
/// text `|PINPROPAGATIONDELAY=5.384400E-011` (seconds). Returns `None` unless
/// every entry parses and reformats to the same text, so the writer's copy
/// matches the original.
pub(crate) fn decode_pin_propagation_delay(data: &[u8], pin_count: usize) -> Option<Vec<(usize, f64)>> {
    fn entry(name: &str, decoded: &[u8], pin_count: usize) -> Option<(usize, f64)> {
        let idx: usize = name.parse().ok()?;
        if idx >= pin_count {
            return None;
        }
        let len = u32::from_le_bytes(decoded.get(0..4)?.try_into().ok()?) as usize;
        if len % 2 != 0 || decoded.len() != 4 + len {
            return None;
        }
        let units: Vec<u16> = decoded[4..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let text = String::from_utf16(&units).ok()?;
        let value = text.strip_prefix("|PINPROPAGATIONDELAY=")?;
        let seconds: f64 = value.parse().ok()?;
        (codec::altium_exponent_format(seconds) == value).then_some((idx, seconds))
    }
    let mut out = Vec::new();
    let mut all_ok = true;
    let read = read_compressed_storage(data, |name, decoded| {
        match entry(name, decoded, pin_count) {
            Some(e) => out.push(e),
            None => all_ok = false,
        }
        Ok(())
    });
    (read.is_ok() && all_ok).then_some(out)
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

/// Decode a NUL-terminated parameter record, honouring `%UTF8%` twins.
fn decode_param_body(bytes: &[u8]) -> ParameterMap {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    ParameterMap::parse_bytes(&bytes[..end], b'|')
}

fn read_param_block<R: Read + std::io::Seek>(br: &mut BinaryReader<R>) -> Result<ParameterMap> {
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

// `Storage` is not modelled for documents; it is carried in
// `additional_streams` and written back verbatim.
const SCH_DOC_KNOWN_STREAMS: &[&str] = &["FileHeader", "Additional"];

impl Document {
    /// Parse a `.SchDoc` from an in-memory buffer.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut cf = CompoundFile::open(bytes)?;
        let mut document = Self::default();

        if let Some(data) = cf.try_read_stream("FileHeader")? {
            read_document_records(&mut document, &data)?;
        }
        // Altium parks harness connectors (215-217) and signal harnesses
        // (218) in a second record stream with its own header block.
        if let Some(data) = cf.try_read_stream("Additional")? {
            read_record_stream(&mut document, &data, true)?;
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

    #[cfg(feature = "async")]
    pub async fn read(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = tokio::fs::read(path).await?;
        Self::from_bytes(bytes)
    }

    #[cfg(feature = "async")]
    pub async fn read_async<R>(mut reader: R) -> Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        #[cfg(feature = "async")]
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Self::from_bytes(bytes)
    }
}

fn read_document_records(document: &mut Document, data: &[u8]) -> Result<()> {
    read_record_stream(document, data, false)
}

/// Dispatch one record stream into `document`. `additional` selects the
/// `Additional` stream, whose header block is kept separately so it can be
/// written back with its own record count.
fn read_record_stream(document: &mut Document, data: &[u8], additional: bool) -> Result<()> {
    let mut br = BinaryReader::new(Cursor::new(data.to_vec()))?;
    let mut current_component: Option<usize> = None;
    let mut current_implementation: Option<(usize, usize)> = None; // (component_idx, impl_idx)
    // `OWNERINDEX` counts records after the file-header record. Track the
    // position of each component record so owned primitives can be filed
    // under the component they name rather than the one last seen.
    let mut pos: i32 = -1;
    let mut header_offset: i32 = 0;
    let mut component_at: BTreeMap<i32, usize> = BTreeMap::new();
    let mut sheet_settings: Option<BTreeMap<String, String>> = None;
    let mut header_parameters: Option<BTreeMap<String, String>> = None;
    let mut template_record: Option<BTreeMap<String, String>> = None;
    let mut sheet_name_annotations: Vec<BTreeMap<String, String>> = Vec::new();
    let mut sheet_filename_annotations: Vec<BTreeMap<String, String>> = Vec::new();

    while br.has_more()? {
        let (flag, body) = br.read_block_with_flags()?;
        pos += 1;
        if body.is_empty() {
            continue;
        }
        let mut params = if flag == 0x01 {
            match decode_binary_pin(&body)? {
                Some(p) => p,
                None => {
                    document.raw_records.push(RawRecord { flag, bytes: body });

                    continue;
                }
            }
        } else {
            decode_param_body(&body)
        };

        let record = params.get_i32("RECORD");
        let owner_component = params
            .get_i32("OWNERINDEX")
            .and_then(|o| component_at.get(&o).copied());

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
            if pos == 0 {
                header_offset = 1;
            }

            continue;
        }
        // Reserved key carrying the record's own index (+1) so owner
        // references can be re-targeted after the writer re-orders records.
        params.insert("__SOURCEINDEX", (pos - header_offset + 1).to_string());
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
                component_at.insert(pos - header_offset, document.components.len() - 1);
            }
            Some(SchRecordType::Pin) => {
                // Text pins name their owner; binary pins do not and belong
                // to the component last seen.
                if let Some(idx) = owner_component.or(current_component) {
                    document.components[idx]
                        .pins
                        .push(codec::pin_from_params(&params));
                } else {
                    handled = false;
                }
            }
            Some(SchRecordType::Symbol) => {
                if let Some(idx) = owner_component {
                    document.components[idx]
                        .symbols
                        .push(codec::symbol_from_params(&params));
                } else {
                    document.symbols.push(codec::symbol_from_params(&params));
                }
            }
            Some(SchRecordType::Label) => {
                let v = codec::label_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].labels.push(v),
                    None => document.labels.push(v),
                }
            }
            Some(SchRecordType::Bezier) => {
                let v = codec::bezier_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].beziers.push(v),
                    None => document.beziers.push(v),
                }
            }
            Some(SchRecordType::Polyline) => {
                let v = codec::polyline_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].polylines.push(v),
                    None => document.polylines.push(v),
                }
            }
            Some(SchRecordType::Polygon) => {
                let v = codec::polygon_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].polygons.push(v),
                    None => document.polygons.push(v),
                }
            }
            Some(SchRecordType::Ellipse) => {
                let v = codec::ellipse_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].ellipses.push(v),
                    None => document.ellipses.push(v),
                }
            }
            Some(SchRecordType::Pie) => {
                let v = codec::pie_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].pies.push(v),
                    None => document.pies.push(v),
                }
            }
            Some(SchRecordType::RoundedRectangle) => {
                let v = codec::rounded_rectangle_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].rounded_rectangles.push(v),
                    None => document.rounded_rectangles.push(v),
                }
            }
            Some(SchRecordType::EllipticalArc) => {
                let v = codec::elliptical_arc_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].elliptical_arcs.push(v),
                    None => document.elliptical_arcs.push(v),
                }
            }
            Some(SchRecordType::Arc) => {
                let v = codec::arc_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].arcs.push(v),
                    None => document.arcs.push(v),
                }
            }
            Some(SchRecordType::Line) => {
                let v = codec::line_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].lines.push(v),
                    None => document.lines.push(v),
                }
            }
            Some(SchRecordType::Rectangle) => {
                let v = codec::rectangle_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].rectangles.push(v),
                    None => document.rectangles.push(v),
                }
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
                let v = codec::power_object_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].power_objects.push(v),
                    None => document.power_objects.push(v),
                }
            }
            Some(SchRecordType::Port) => {
                document.ports.push(codec::port_from_params(&params));
            }
            Some(SchRecordType::HarnessConnector) => {
                document
                    .harness_connectors
                    .push(codec::harness_connector_from_params(&params));
            }
            Some(SchRecordType::HarnessEntry) => {
                // Entries follow their connector, like sheet entries follow
                // their sheet symbol.
                match document.harness_connectors.last_mut() {
                    Some(parent) => parent
                        .entries
                        .push(codec::harness_entry_from_params(&params)),
                    None => handled = false,
                }
            }
            Some(SchRecordType::HarnessType) => match document.harness_connectors.last_mut() {
                Some(parent) => {
                    parent.harness_type = Some(codec::harness_type_from_params(&params))
                }
                None => handled = false,
            },
            Some(SchRecordType::SignalHarness) => {
                document
                    .signal_harnesses
                    .push(codec::signal_harness_from_params(&params));
            }
            Some(SchRecordType::NoErc) => {
                document.no_ercs.push(codec::no_erc_from_params(&params));
            }
            Some(SchRecordType::NetLabel) => {
                let v = codec::net_label_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].net_labels.push(v),
                    None => document.net_labels.push(v),
                }
            }
            Some(SchRecordType::Bus) => {
                document.buses.push(codec::bus_from_params(&params));
            }
            Some(SchRecordType::Wire) => {
                let v = codec::wire_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].wires.push(v),
                    None => document.wires.push(v),
                }
            }
            Some(SchRecordType::TextFrame) => {
                let v = codec::text_frame_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].text_frames.push(v),
                    None => document.text_frames.push(v),
                }
            }
            Some(SchRecordType::Junction) => {
                let v = codec::junction_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].junctions.push(v),
                    None => document.junctions.push(v),
                }
            }
            Some(SchRecordType::Image) => {
                let v = codec::image_from_params(&params);
                match owner_component {
                    Some(ci) => document.components[ci].images.push(v),
                    None => document.images.push(v),
                }
            }
            Some(SchRecordType::BusEntry) => {
                document
                    .bus_entries
                    .push(codec::bus_entry_from_params(&params));
            }
            Some(SchRecordType::Designator) | Some(SchRecordType::Parameter) => {
                let p = codec::parameter_from_params(&params);
                // A component's parameters always name it by `OWNERINDEX`;
                // one with no owner, or with an owner that is not a
                // component (a pin, a sheet symbol, a parameter set), stays
                // at document level.
                if let Some(idx) = owner_component {
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
                if let Some(ci) = owner_component {
                    current_component = Some(ci);
                }
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

    if additional {
        document.additional_header_parameters = header_parameters;
        if sheet_settings.is_some() {
            document.sheet_settings = sheet_settings;
        }
        if template_record.is_some() {
            document.template_record = template_record;
        }
        document
            .sheet_name_annotations
            .extend(sheet_name_annotations);
        document
            .sheet_filename_annotations
            .extend(sheet_filename_annotations);
    } else {
        document.header_parameters = header_parameters;
        document.sheet_settings = sheet_settings;
        document.template_record = template_record;
        document.sheet_name_annotations = sheet_name_annotations;
        document.sheet_filename_annotations = sheet_filename_annotations;
    }
    Ok(())
}

#[allow(dead_code)]
fn _force_imports(_x: Implementation) {}
