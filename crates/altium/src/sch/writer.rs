//! Writer for `.SchLib` and `.SchDoc` files.

use std::collections::BTreeMap;
use std::io::{Cursor, Seek, Write};
#[cfg(feature = "async")]
use std::path::Path;

#[cfg(feature = "async")]
use tokio::io::AsyncWrite;

use super::binary::{
    PinTextCustomisation, coord_to_dxp_frac, encode_pin_text_customisation,
    write_compressed_storage,
};
use super::codec;
use super::component::{Component, RawRecord};
use super::document::Document;
use super::primitives::PrimitiveCommon;
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

    #[cfg(feature = "async")]
    pub async fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = self.to_bytes()?;
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    #[cfg(feature = "async")]
    pub async fn write_async<W>(&self, mut writer: W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        #[cfg(feature = "async")]
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
        // The component list (CompCount, LibRefN, CompDescrN, PartCountN)
        // always describes the components actually written; a header carried
        // over from the source file would otherwise hide added components and
        // keep entries for removed ones.
        if is_component_list_key(k) {
            continue;
        }
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

/// Header keys that enumerate the components and must follow `library.components`.
fn is_component_list_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    if upper == "COMPCOUNT" {
        return true;
    }
    ["LIBREF", "COMPDESCR", "PARTCOUNT"].iter().any(|prefix| {
        upper
            .strip_prefix(prefix)
            .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
    })
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

    write_component_records(&mut bw, component, PinRecordForm::Binary, None)?;

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
    if let Some(delays) = build_pin_propagation_delay(&component.pins)? {
        cf.write_stream(format!("{section_key}/PinPropagationDelay"), &delays)?;
    }

    for (name, data) in &component.additional_streams {
        cf.write_stream(format!("{section_key}/{name}"), data)?;
    }
    Ok(())
}

/// How pin records are emitted. Library component streams use the binary
/// form (text customisation goes to the sidecar streams); documents use the
/// text form with the customisation inline, which is the only form the
/// canvas renders in a document.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PinRecordForm {
    Binary,
    Text,
}

/// Stamp the owning record's index on a child. `None` is the library case,
/// where the component header is record 0 and Altium omits the key.
fn stamp_owner(p: &mut ParameterMap, owner: Option<i32>) {
    if let Some(o) = owner {
        p.insert("OWNERINDEX", o.to_string());
        p.insert("__OWNERRESOLVED", "1");
    }
}

/// True when a top-level primitive was read as the child of some other
/// record: it is emitted after its owner and its `OWNERINDEX` is
/// re-targeted by [`resolve_owner_indices`].
fn is_owned(c: &PrimitiveCommon) -> bool {
    c.source_index.is_some() && c.owner_index != 0
}

/// Second pass over a written record stream. Builds the old-to-new record
/// index map from the reserved `__SOURCEINDEX` keys, rewrites every
/// `OWNERINDEX` that was not stamped structurally, and strips all reserved
/// (`__`-prefixed) keys so nothing synthetic reaches the file.
fn resolve_owner_indices(stream: &[u8]) -> Vec<u8> {
    struct Block {
        flags: u8,
        body: Vec<u8>,
    }
    let mut blocks = Vec::new();
    let mut i = 0usize;
    while i + 4 <= stream.len() {
        let header = u32::from_le_bytes([stream[i], stream[i + 1], stream[i + 2], stream[i + 3]]);
        let len = (header & 0x00ff_ffff) as usize;
        let flags = (header >> 24) as u8;
        blocks.push(Block {
            flags,
            body: stream[i + 4..(i + 4 + len).min(stream.len())].to_vec(),
        });
        i += 4 + len;
    }
    fn fields(body: &[u8]) -> Vec<Vec<u8>> {
        let trimmed = body.strip_suffix(b"\0").unwrap_or(body);
        trimmed.split(|b| *b == b'|').skip(1).map(<[u8]>::to_vec).collect()
    }
    fn key_of(field: &[u8]) -> Vec<u8> {
        field.split(|b| *b == b'=').next().unwrap_or(field).to_ascii_uppercase()
    }
    fn value_of(field: &[u8]) -> &[u8] {
        field.splitn(2, |b| *b == b'=').nth(1).unwrap_or(b"")
    }
    let header_offset = usize::from(
        blocks
            .first()
            .is_some_and(|b| b.flags == 0 && !fields(&b.body).iter().any(|f| key_of(f) == b"RECORD")),
    );
    let mut new_index_of: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for (n, b) in blocks.iter().enumerate() {
        if b.flags != 0 {
            continue;
        }
        for f in fields(&b.body) {
            if key_of(&f) == b"__SOURCEINDEX" {
                if let Ok(v) = std::str::from_utf8(value_of(&f)).unwrap_or("").parse::<i64>() {
                    if v > 0 {
                        new_index_of.insert(v - 1, n as i64 - header_offset as i64);
                    }
                }
            }
        }
    }
    let mut out = Vec::with_capacity(stream.len());
    for b in &blocks {
        let body: Vec<u8> = if b.flags == 0 {
            let fs = fields(&b.body);
            let from_stream = fs.iter().any(|f| key_of(f) == b"__SOURCEINDEX");
            let resolved = fs.iter().any(|f| key_of(f) == b"__OWNERRESOLVED");
            let mut rebuilt = Vec::with_capacity(b.body.len());
            for f in &fs {
                let k = key_of(f);
                if k.starts_with(b"__") {
                    continue;
                }
                rebuilt.push(b'|');
                if k == b"OWNERINDEX" && from_stream && !resolved {
                    let old = std::str::from_utf8(value_of(f)).unwrap_or("").parse::<i64>().ok();
                    if let Some(new) = old.and_then(|o| new_index_of.get(&o)) {
                        rebuilt.extend_from_slice(format!("OWNERINDEX={new}").as_bytes());
                        continue;
                    }
                }
                rebuilt.extend_from_slice(f);
            }
            rebuilt.push(0);
            rebuilt
        } else {
            b.body.clone()
        };
        let header = (u32::from(b.flags) << 24) | (body.len() as u32 & 0x00ff_ffff);
        out.extend_from_slice(&header.to_le_bytes());
        out.extend_from_slice(&body);
    }
    out
}

/// `owner` is the component header's absolute record index when writing
/// into a document stream; children and the implementation chain are
/// numbered from it so every `OWNERINDEX` resolves after re-ordering.
fn write_component_records<W: Write + Seek>(
    bw: &mut BinaryWriter<W>,
    component: &Component,
    pin_form: PinRecordForm,
    owner: Option<i32>,
) -> Result<()> {
    // Altium links implementation records (44..48) by record index. In a
    // library stream the component header sits at index 0, so counting
    // starts at 1; in a document it starts after the header's own index.
    let mut rec_idx: i32 = owner.map_or(1, |o| o + 1);
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
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for bz in &component.beziers {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "5");
        codec::bezier_to_params(bz, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for poly in &component.polylines {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "6");
        codec::polyline_to_params(poly, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for poly in &component.polygons {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "7");
        codec::polygon_to_params(poly, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for e in &component.ellipses {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "8");
        codec::ellipse_to_params(e, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for pie in &component.pies {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "9");
        codec::pie_to_params(pie, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for r in &component.rounded_rectangles {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "10");
        codec::rounded_rectangle_to_params(r, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for ea in &component.elliptical_arcs {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "11");
        codec::elliptical_arc_to_params(ea, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for arc in &component.arcs {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "12");
        codec::arc_to_params(arc, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for line in &component.lines {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "13");
        codec::line_to_params(line, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }

    for pin in &component.pins {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "2");
        codec::pin_to_params(pin, &mut p);
        stamp_owner(&mut p, owner);
        match pin_form {
            PinRecordForm::Text => {
                // Only the binary form carries these; their values are not
                // representable as text fields.
                p.remove("SWAPIDGROUP");
                p.remove("PARTANDSEQUENCE");
                emit_param_record(bw, &p)?;
            }
            PinRecordForm::Binary => {
                let body = super::binary::encode_binary_pin(&p);
                bw.write_block_with_flags(0x01, |w| {
                    w.write_bytes(&body)?;
                    Ok(())
                })?;
            }
        }
        rec_idx += 1;
    }
    for sym in &component.symbols {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "3");
        codec::symbol_to_params(sym, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for label in &component.labels {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "4");
        codec::label_to_params(label, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for po in &component.power_objects {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "17");
        codec::power_object_to_params(po, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for nl in &component.net_labels {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "25");
        codec::net_label_to_params(nl, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for w in &component.wires {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "27");
        codec::wire_to_params(w, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for tf in &component.text_frames {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "28");
        codec::text_frame_to_params(tf, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for j in &component.junctions {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "29");
        codec::junction_to_params(j, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }
    for img in &component.images {
        let mut p = ParameterMap::new();
        p.insert("RECORD", "30");
        codec::image_to_params(img, &mut p);
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
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
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
    }

    // Implementation hierarchy: ImplementationList → Implementation* →
    //   MapDefinerList → MapDefiner* → ImplementationParameters
    if !component.implementations.is_empty() {
        // Altium's layout: 44 (list) → per model: 45 owned by the 44,
        // 46 owned by the 45, 47s owned by the 46, 48 owned by the 45. The
        // OwnerIndex links are record positions; Altium discards models whose
        // MapDefiners are not attached this way.
        let list_idx = rec_idx;
        let mut p = ParameterMap::new();
        p.insert("RECORD", "44");
        stamp_owner(&mut p, owner);
        emit_param_record(bw, &p)?;
        rec_idx += 1;
        for impl_ in &component.implementations {
            let impl_idx = rec_idx;
            let mut p = ParameterMap::new();
            codec::implementation_to_params(impl_, &mut p);
            p.insert("OWNERINDEX", list_idx.to_string());
            emit_param_record(bw, &p)?;
            rec_idx += 1;
            let map_list_idx = rec_idx;
            let mut p = ParameterMap::new();
            p.insert("RECORD", "46");
            p.insert("OWNERINDEX", impl_idx.to_string());
            emit_param_record(bw, &p)?;
            rec_idx += 1;
            for (i, map) in impl_.map_definers.iter().enumerate() {
                let mut p = ParameterMap::new();
                codec::map_definer_to_params(map, &mut p);
                p.insert("OWNERINDEX", map_list_idx.to_string());
                if i > 0 {
                    p.insert("INDEXINSHEET", i.to_string());
                }
                emit_param_record(bw, &p)?;
                rec_idx += 1;
            }
            let mut p = ParameterMap::new();
            p.insert("RECORD", "48");
            p.insert("OWNERINDEX", impl_idx.to_string());
            emit_param_record(bw, &p)?;
            rec_idx += 1;
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

/// Per-pin `PinPropagationDelay` stream: one zlib-compressed entry per pin
/// with a non-zero delay, a `u32` byte length and the UTF-16LE text
/// `|PINPROPAGATIONDELAY=<seconds>` in Altium's fixed-exponent form, as
/// Altium writes it (pins without a delay get no entry).
pub(crate) fn build_pin_propagation_delay(pins: &[Pin]) -> Result<Option<Vec<u8>>> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, pin) in pins.iter().enumerate() {
        if pin.pin_propagation_delay == 0.0 {
            continue;
        }
        let text = format!("|PINPROPAGATIONDELAY={}", codec::altium_exponent_format(pin.pin_propagation_delay));
        let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut body = Vec::with_capacity(4 + utf16.len());
        body.extend_from_slice(&(utf16.len() as u32).to_le_bytes());
        body.extend_from_slice(&utf16);
        entries.push((i.to_string(), body));
    }
    if entries.is_empty() {
        return Ok(None);
    }
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    let mut header = ParameterMap::new();
    header.insert("HEADER", "PinPropagationDelay");
    header.insert("Weight", entries.len().to_string());
    write_compressed_storage(&mut bw, &header, &entries)?;
    Ok(Some(buf.into_inner()))
}

/// Per-pin `PinTextData` stream: one zlib-compressed entry per customised
/// pin holding the designator block then the name block (see
/// [`PinTextCustomisation`] for the layout). Pins with no font / colour /
/// position customisation get no entry, as in Altium-written libraries.
fn build_pin_text_data(pins: &[Pin]) -> Result<Option<Vec<u8>>> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, pin) in pins.iter().enumerate() {
        let designator = pin_text_customisation(
            pin.designator_font_mode,
            pin.designator_custom_font_id,
            pin.designator_custom_color,
            pin.designator_position_mode,
            pin.designator_custom_position_margin,
            pin.designator_custom_position_rotation_anchor,
            pin.designator_custom_position_rotation_relative,
        );
        let name = pin_text_customisation(
            pin.name_font_mode,
            pin.name_custom_font_id,
            pin.name_custom_color,
            pin.name_position_mode,
            pin.name_custom_position_margin,
            pin.name_custom_position_rotation_anchor,
            pin.name_custom_position_rotation_relative,
        );
        if designator.is_default() && name.is_default() {
            continue;
        }
        let mut body = Vec::with_capacity(22);
        encode_pin_text_customisation(&designator, &mut body);
        encode_pin_text_customisation(&name, &mut body);
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

#[allow(clippy::too_many_arguments)]
fn pin_text_customisation(
    font_mode: i32,
    custom_font_id: i32,
    custom_color: i32,
    position_mode: i32,
    margin: i32,
    rotation_anchor: i32,
    rotation_relative: bool,
) -> PinTextCustomisation {
    let custom_font = font_mode != 0 && custom_font_id != 0;
    PinTextCustomisation {
        custom_position: position_mode != 0,
        margin: if position_mode != 0 { margin } else { 0 },
        rotation_anchor: rotation_anchor != 0,
        rotation_relative,
        custom_font,
        font_id: if custom_font { custom_font_id as u8 } else { 0 },
        color: if custom_font { custom_color } else { 0 },
    }
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
        // The header record is not counted in `OWNERINDEX` numbering.
        let hdr = i32::from(self.header_parameters.is_some());
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
        // Top-level records. Free ones go out before the components; ones
        // read as children of another record (a sheet symbol's annotations,
        // a pin's parameters, a template's graphics) go out after their
        // owner, and `resolve_owner_indices` re-targets their `OWNERINDEX`.
        macro_rules! top_level {
            ($phase:expr) => {{
                let owned_phase: bool = $phase;
                let map_is_owned = |m: &BTreeMap<String, String>| {
                    m.keys().any(|k| k.eq_ignore_ascii_case("OWNERINDEX"))
                        && m.keys().any(|k| k.eq_ignore_ascii_case("__SOURCEINDEX"))
                };
                for ann in &self.sheet_name_annotations {
                    if map_is_owned(ann) != owned_phase {
                        continue;
                    }
                    let mut p = ParameterMap::new();
                    p.insert("RECORD", "32");
                    for (k, v) in ann {
                        p.insert(k, v.clone());
                    }
                    emit_param_record(&mut bw, &p)?;
                }
                for ann in &self.sheet_filename_annotations {
                    if map_is_owned(ann) != owned_phase {
                        continue;
                    }
                    let mut p = ParameterMap::new();
                    p.insert("RECORD", "33");
                    for (k, v) in ann {
                        p.insert(k, v.clone());
                    }
                    emit_param_record(&mut bw, &p)?;
                }
        for label in &self.labels {
            if is_owned(&label.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "4", |p| codec::label_to_params(label, p))?;
        }
        for sym in &self.symbols {
            if is_owned(&sym.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "3", |p| codec::symbol_to_params(sym, p))?;
        }
        for bz in &self.beziers {
            if is_owned(&bz.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "5", |p| codec::bezier_to_params(bz, p))?;
        }
        for poly in &self.polylines {
            if is_owned(&poly.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "6", |p| codec::polyline_to_params(poly, p))?;
        }
        for poly in &self.polygons {
            if is_owned(&poly.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "7", |p| codec::polygon_to_params(poly, p))?;
        }
        for e in &self.ellipses {
            if is_owned(&e.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "8", |p| codec::ellipse_to_params(e, p))?;
        }
        for pie in &self.pies {
            if is_owned(&pie.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "9", |p| codec::pie_to_params(pie, p))?;
        }
        for r in &self.rounded_rectangles {
            if is_owned(&r.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "10", |p| codec::rounded_rectangle_to_params(r, p))?;
        }
        for ea in &self.elliptical_arcs {
            if is_owned(&ea.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "11", |p| codec::elliptical_arc_to_params(ea, p))?;
        }
        for arc in &self.arcs {
            if is_owned(&arc.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "12", |p| codec::arc_to_params(arc, p))?;
        }
        for line in &self.lines {
            if is_owned(&line.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "13", |p| codec::line_to_params(line, p))?;
        }
        for rect in &self.rectangles {
            if is_owned(&rect.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "14", |p| codec::rectangle_to_params(rect, p))?;
        }
        for sym in &self.sheet_symbols {
            if is_owned(&sym.common) != owned_phase {
                continue;
            }
            let sym_idx = bw.blocks_written() as i32 - hdr;
            emit_typed(&mut bw, "15", |p| codec::sheet_symbol_to_params(sym, p))?;
            for entry in &sym.entries {
                emit_typed(&mut bw, "16", |p| {
                    codec::sheet_entry_to_params(entry, p);
                    stamp_owner(p, Some(sym_idx));
                })?;
            }
        }
        for po in &self.power_objects {
            if is_owned(&po.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "17", |p| codec::power_object_to_params(po, p))?;
        }
        for port in &self.ports {
            if is_owned(&port.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "18", |p| codec::port_to_params(port, p))?;
        }
        for n in &self.no_ercs {
            if is_owned(&n.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "22", |p| codec::no_erc_to_params(n, p))?;
        }
        for nl in &self.net_labels {
            if is_owned(&nl.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "25", |p| codec::net_label_to_params(nl, p))?;
        }
        for bus in &self.buses {
            if is_owned(&bus.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "26", |p| codec::bus_to_params(bus, p))?;
        }
        for w in &self.wires {
            if is_owned(&w.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "27", |p| codec::wire_to_params(w, p))?;
        }
        for tf in &self.text_frames {
            if is_owned(&tf.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "28", |p| codec::text_frame_to_params(tf, p))?;
        }
        for j in &self.junctions {
            if is_owned(&j.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "29", |p| codec::junction_to_params(j, p))?;
        }
        for img in &self.images {
            if is_owned(&img.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "30", |p| codec::image_to_params(img, p))?;
        }
        for be in &self.bus_entries {
            if is_owned(&be.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "37", |p| codec::bus_entry_to_params(be, p))?;
        }
        for param in &self.parameters {
            if is_owned(&param.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "41", |p| codec::parameter_to_params(param, p))?;
        }
        for ps in &self.parameter_sets {
            if is_owned(&ps.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "43", |p| codec::parameter_set_to_params(ps, p))?;
        }
        for b in &self.blankets {
            if is_owned(&b.common) != owned_phase {
                continue;
            }
            emit_typed(&mut bw, "225", |p| codec::blanket_to_params(b, p))?;
        }

            }};
        }
        top_level!(false);

        // Components and their owned children. Each child names its owner
        // by the owner's absolute record index, counted after the header.
        for component in &self.components {
            let base = bw.blocks_written() as i32 - hdr;
            write_component_records(&mut bw, component, PinRecordForm::Text, Some(base))?;
        }

        top_level!(true);

        // Replay any unhandled raw records last.
        for record in &self.raw_records {
            emit_raw_record(&mut bw, record)?;
        }

        let resolved = resolve_owner_indices(&buf.into_inner());
        cf.write_stream("FileHeader", &resolved)?;

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
                    // Entries and the type label name the connector by its
                    // index within this stream, after the header record.
                    let hc_idx = abw.blocks_written() as i32 - 1;
                    emit_typed(&mut abw, "215", |p| {
                        codec::harness_connector_to_params(hc, p)
                    })?;
                    for entry in &hc.entries {
                        emit_typed(&mut abw, "216", |p| {
                            codec::harness_entry_to_params(entry, p);
                            stamp_owner(p, Some(hc_idx));
                        })?;
                    }
                    if let Some(ht) = &hc.harness_type {
                        emit_typed(&mut abw, "217", |p| {
                            codec::harness_type_to_params(ht, p);
                            stamp_owner(p, Some(hc_idx));
                        })?;
                    }
                }
                for sh in &self.signal_harnesses {
                    emit_typed(&mut abw, "218", |p| codec::signal_harness_to_params(sh, p))?;
                }
            }
            // Same pass as the main stream: strips the reserved `__` keys (the
            // structural `__OWNERRESOLVED` stamps on entries / type labels).
            cf.write_stream("Additional", &resolve_owner_indices(&abuf.into_inner()))?;
        }

        for (path, data) in &self.additional_streams {
            cf.write_stream(path, data)?;
        }

        cf.into_bytes()
    }

    #[cfg(feature = "async")]
    pub async fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = self.to_bytes()?;
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    #[cfg(feature = "async")]
    pub async fn write_async<W>(&self, mut writer: W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        #[cfg(feature = "async")]
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

#[cfg(test)]
mod pin_propagation_delay_tests {
    use super::*;
    use crate::sch::binary::read_compressed_storage;

    /// Entries use the layout Altium writes: a u32 byte length and UTF-16LE
    /// `|PINPROPAGATIONDELAY=5.384400E-011`, keyed by pin index, zero-delay
    /// pins skipped; the reader decodes them back.
    #[test]
    fn stream_matches_altium_layout() {
        let mut pins = vec![Pin::default(), Pin::default(), Pin::default()];
        pins[0].pin_propagation_delay = 5.3844e-11;
        pins[2].pin_propagation_delay = 6.1864e-11;
        let stream = build_pin_propagation_delay(&pins).unwrap().expect("stream");
        let mut entries = Vec::new();
        read_compressed_storage(&stream, |name, decoded| {
            entries.push((name.to_string(), decoded.to_vec()));
            Ok(())
        })
        .unwrap();
        let text = "|PINPROPAGATIONDELAY=5.384400E-011";
        let mut expected = (2 * text.len() as u32).to_le_bytes().to_vec();
        expected.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], ("0".to_string(), expected));
        assert_eq!(entries[1].0, "2");
        let decoded = crate::sch::reader::decode_pin_propagation_delay(&stream, pins.len()).expect("decodes");
        assert_eq!(decoded, vec![(0, 5.3844e-11), (2, 6.1864e-11)]);
        assert!(build_pin_propagation_delay(&[Pin::default()]).unwrap().is_none());
    }
}

