//! Reader for `.PcbLib` and `.PcbDoc` files.

#![allow(clippy::field_reassign_with_default)]

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Seek};
use std::path::Path;

use indexmap::IndexMap;
use tokio::io::AsyncRead;

use super::binary::{ObjectId, PrimitiveFlags, read_common_prefix, read_coord_point};
use super::component::Component;
use super::library::Library;
use super::primitives::{Arc, ComponentBody, Fill, Pad, Region, Text, Track, Via};
use crate::binary::BinaryReader;
use crate::compound::CompoundFile;
use crate::coord::{Coord, CoordPoint};
use crate::diagnostic::Diagnostic;
use crate::encoding;
use crate::enums::{PadHoleType, PadShape, PcbStrokeFont, PcbTextKind, TextJustification};
use crate::error::{Error, Result};
use crate::parameter::ParameterMap;

// Library reader

impl Library {
    /// Parse a `.PcbLib` file from an in-memory buffer.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut cf = CompoundFile::open(bytes)?;
        let mut library = Self::default();
        let mut diagnostics = Vec::new();

        read_file_header(&mut cf, &mut library, &mut diagnostics)?;
        preserve_root_streams(&mut cf, &mut library)?;
        let (section_keys, section_key_order) = read_section_keys(&mut cf)?;
        library.section_keys = section_keys.clone();
        library.section_key_order = section_key_order;
        read_library(&mut cf, &mut library, &section_keys, &mut diagnostics)?;

        library.diagnostics = diagnostics;
        Ok(library)
    }

    /// Read a `.PcbLib` from disk.
    pub async fn read(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = tokio::fs::read(path).await?;
        Self::from_bytes(bytes)
    }

    /// Read a `.PcbLib` from any `AsyncRead`.
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

fn read_file_header(
    cf: &mut CompoundFile,
    library: &mut Library,
    _diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let Some(data) = cf.try_read_stream("FileHeader")? else {
        return Ok(());
    };
    let mut br = BinaryReader::new(Cursor::new(data))?;
    let block_len = br.read_i32()?;
    if block_len <= 0 {
        return Ok(());
    }
    // Version string (Pascal short).
    let str_len = br.read_u8()? as u32;
    br.skip(u64::from(str_len))?;
    let consumed = 4u64 + 1 + u64::from(str_len);
    let block_total = 4u64 + block_len as u64;
    if consumed < block_total {
        br.skip(block_total - consumed)?;
    }

    // Three trailing Pascal short strings.
    if br.has_more()? {
        let len = br.read_u8()? as u32;
        if len > 0 {
            br.skip(u64::from(len))?;
        }
    }
    if br.has_more()? {
        let len = br.read_u8()? as u32;
        if len > 0 {
            br.skip(u64::from(len))?;
        }
    }
    if br.has_more()? {
        let len = br.read_u8()? as usize;
        if len > 0 {
            let mut buf = vec![0u8; len];
            br.read_exact(&mut buf)?;
            library.unique_id = encoding::decode(&buf);
        }
    }
    Ok(())
}

fn preserve_root_streams(cf: &mut CompoundFile, library: &mut Library) -> Result<()> {
    if cf.is_storage("FileVersionInfo") {
        let entries = cf.list_children("FileVersionInfo")?;
        for entry in entries {
            if entry.is_stream {
                let data = cf.read_stream(format!("FileVersionInfo/{}", entry.name))?;
                library
                    .additional_root_streams
                    .insert(format!("FileVersionInfo/{}", entry.name), data);
            }
        }
    }
    Ok(())
}

fn read_section_keys(cf: &mut CompoundFile) -> Result<(BTreeMap<String, String>, Vec<String>)> {
    let mut map = BTreeMap::new();
    let mut order = Vec::new();
    let Some(data) = cf.try_read_stream("SectionKeys")? else {
        return Ok((map, order));
    };
    let mut br = BinaryReader::new(Cursor::new(data))?;
    let count = br.read_i32()?;
    for _ in 0..count {
        // Both fields are `[i32 size][u8 len][bytes]` blocks — the same
        // shape `write_section_keys_stream` emits and Altium itself uses.
        let lib_ref = br.read_pascal_string_block()?;
        let section_key = br.read_pascal_string_block()?;
        order.push(lib_ref.clone());
        map.insert(lib_ref, section_key);
    }
    Ok((map, order))
}

fn read_library(
    cf: &mut CompoundFile,
    library: &mut Library,
    section_keys: &BTreeMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    if !cf.is_storage("Library") {
        return Err(Error::corrupt_in("missing Library storage", "Library"));
    }
    // The Header stream is metadata only; we ignore its contents.
    let data = cf.read_stream("Library/Data")?;
    let mut br = BinaryReader::new(Cursor::new(data))?;

    // Library-level parameters
    let header_params = read_param_map(&mut br)?;
    if !header_params.is_empty() {
        library.library_parameters = Some(header_params);
    }

    // Component count + entries.
    let count = br.read_u32()?;
    for _ in 0..count {
        let ref_name = br.read_pascal_string_block()?;
        let section_key = section_keys
            .get(&ref_name)
            .cloned()
            .unwrap_or_else(|| section_key_from_name(&ref_name));
        if let Some(component) = read_footprint(cf, &section_key, diagnostics)? {
            library.components.push(component);
        }
    }

    // Preserve unknown library children.
    // ComponentParamsTOC is a derived table (name, pad count, height and
    // description of every footprint) and is regenerated on write.
    let known_children: &[&str] = &["Header", "Data", "Models", "ComponentParamsTOC"];
    let entries = cf.list_children("Library")?;
    for entry in entries {
        if known_children
            .iter()
            .any(|n| entry.name.eq_ignore_ascii_case(n))
        {
            continue;
        }
        if entry.is_stream {
            let data = cf.read_stream(format!("Library/{}", entry.name))?;
            library.additional_library_streams.insert(entry.name, data);
        } else {
            let inner = cf.list_children(format!("Library/{}", entry.name))?;
            for sub in inner {
                if sub.is_stream {
                    let data = cf.read_stream(format!("Library/{}/{}", entry.name, sub.name))?;
                    library
                        .additional_library_streams
                        .insert(format!("{}/{}", entry.name, sub.name), data);
                }
            }
        }
    }

    if cf.is_storage("Library/Models") {
        read_models(cf, library)?;
    }

    Ok(())
}

fn read_models(cf: &mut CompoundFile, library: &mut Library) -> Result<()> {
    let metas = match cf.try_read_stream("Library/Models/Data")? {
        Some(data) => super::model3d::parse_models_data(&data),
        None => Vec::new(),
    };
    library.models = super::model3d::build_models(&metas, |i| {
        cf.try_read_stream(format!("Library/Models/{i}"))
            .ok()
            .flatten()
    })?;
    Ok(())
}

fn strip_nulls(buf: &[u8]) -> Vec<u8> {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf[..end].to_vec()
}

fn section_key_from_name(name: &str) -> String {
    let trimmed = if name.len() > 31 { &name[..31] } else { name };
    trimmed.replace('/', "_")
}

fn read_footprint(
    cf: &mut CompoundFile,
    section_key: &str,
    _diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<Component>> {
    if !cf.is_storage(section_key) {
        return Ok(None);
    }
    let mut component = Component::default();

    if let Some(data) = cf.try_read_stream(format!("{section_key}/Parameters"))? {
        let mut br = BinaryReader::new(Cursor::new(data))?;
        let params = read_param_map(&mut br)?;
        apply_component_parameters(&mut component, &params);
    }

    let wide_strings = read_wide_strings(cf, section_key)?;

    // Preserve additional component-level streams.
    let known: &[&str] = &["Header", "Parameters", "WideStrings", "Data"];
    let entries = cf.list_children(section_key)?;
    for entry in entries {
        if known.iter().any(|n| entry.name.eq_ignore_ascii_case(n)) {
            continue;
        }
        if entry.is_stream {
            let data = cf.read_stream(format!("{section_key}/{}", entry.name))?;
            component.additional_streams.insert(entry.name, data);
        } else {
            let inner = cf.list_children(format!("{section_key}/{}", entry.name))?;
            for sub in inner {
                if sub.is_stream {
                    let data =
                        cf.read_stream(format!("{section_key}/{}/{}", entry.name, sub.name))?;
                    component
                        .additional_streams
                        .insert(format!("{}/{}", entry.name, sub.name), data);
                }
            }
        }
    }

    if let Some(data) = cf.try_read_stream(format!("{section_key}/Data"))? {
        let mut br = BinaryReader::new(Cursor::new(data))?;
        let pattern = br.read_pascal_string_block()?;
        if component.name.is_empty() {
            component.name = pattern;
        }
        while br.has_more()? {
            let id = br.read_u8()?;
            match ObjectId::from_byte(id) {
                Some(ObjectId::Arc) => {
                    if let Some(arc) = read_arc(&mut br)? {
                        component.primitive_order.push(id);
                        component.arcs.push(arc);
                    }
                }
                Some(ObjectId::Pad) => {
                    if let Some(pad) = read_pad(&mut br)? {
                        component.primitive_order.push(id);
                        component.pads.push(pad);
                    }
                }
                Some(ObjectId::Via) => {
                    if let Some(via) = read_via(&mut br)? {
                        component.primitive_order.push(id);
                        component.vias.push(via);
                    }
                }
                Some(ObjectId::Track) => {
                    if let Some(track) = read_track(&mut br)? {
                        component.primitive_order.push(id);
                        component.tracks.push(track);
                    }
                }
                Some(ObjectId::Text) => {
                    if let Some(text) = read_text(&mut br, &wide_strings)? {
                        component.primitive_order.push(id);
                        component.texts.push(text);
                    }
                }
                Some(ObjectId::Fill) => {
                    if let Some(fill) = read_fill(&mut br)? {
                        component.primitive_order.push(id);
                        component.fills.push(fill);
                    }
                }
                Some(ObjectId::Region) => {
                    if let Some(region) = read_region(&mut br)? {
                        component.primitive_order.push(id);
                        component.regions.push(region);
                    }
                }
                Some(ObjectId::ComponentBody) => {
                    if let Some(body) = read_component_body(&mut br)? {
                        component.primitive_order.push(id);
                        component.component_bodies.push(body);
                    }
                }
                None => {
                    br.skip_block()?;
                }
            }
        }
    }

    let order = component.primitive_order.clone();
    super::guids::absorb_tables(&mut component, &order);

    Ok(Some(component))
}

fn read_wide_strings(cf: &mut CompoundFile, section_key: &str) -> Result<Vec<String>> {
    let Some(data) = cf.try_read_stream(format!("{section_key}/WideStrings"))? else {
        return Ok(Vec::new());
    };
    let mut br = BinaryReader::new(Cursor::new(data))?;
    let map = read_param_map(&mut br)?;
    let mut out = Vec::new();
    for i in 0.. {
        let key = format!("ENCODEDTEXT{i}");
        let Some(encoded) = map.get(&key) else {
            break;
        };
        out.push(decode_wide_string(encoded));
    }
    Ok(out)
}

fn decode_wide_string(encoded: &str) -> String {
    encoded
        .split(',')
        .filter_map(|p| p.parse::<u32>().ok())
        .filter_map(char::from_u32)
        .collect()
}

fn apply_component_parameters(component: &mut Component, params: &ParameterMap) {
    let mut consumed = Vec::<&str>::new();
    if let Some(v) = params.get("PATTERN") {
        component.name = v.to_string();
        consumed.push("PATTERN");
    }
    if let Some(v) = params.get("DESCRIPTION") {
        component.description = Some(v.to_string());
        consumed.push("DESCRIPTION");
    }
    if let Some(v) = params.get("HEIGHT") {
        if let Ok(c) = v.parse::<Coord>() {
            component.height = c;
        }
        consumed.push("HEIGHT");
    }
    if let Some(v) = params.get("ITEMGUID") {
        component.item_guid = Some(v.to_string());
        consumed.push("ITEMGUID");
    }
    if let Some(v) = params.get("REVISIONGUID") {
        component.item_revision_guid = Some(v.to_string());
        consumed.push("REVISIONGUID");
    }
    component.additional_parameters = extract_remaining_parameters(params, &consumed);
}

fn extract_remaining_parameters(
    params: &ParameterMap,
    consumed: &[&str],
) -> BTreeMap<String, String> {
    let mut consumed_upper: Vec<String> = consumed.iter().map(|s| s.to_ascii_uppercase()).collect();
    consumed_upper.sort();
    let mut out = BTreeMap::new();
    for (name, value, _) in params.iter() {
        if consumed_upper
            .binary_search(&name.to_ascii_uppercase())
            .is_err()
        {
            out.insert(name.to_string(), value.to_string());
        }
    }
    out
}

fn read_param_map<R: Read + Seek>(br: &mut BinaryReader<R>) -> Result<ParameterMap> {
    let bytes = br.read_block()?;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    Ok(ParameterMap::parse_bytes(&bytes[..end], b'|'))
}

// Primitives

fn read_arc<R: Read + Seek>(br: &mut BinaryReader<R>) -> Result<Option<Arc>> {
    let body = br.read_block()?;
    if body.is_empty() {
        return Ok(None);
    }
    let size = body.len() as u32;
    let mut cur = BinaryReader::new(std::io::Cursor::new(body.as_slice()))?;
    let br = &mut cur;
    let start = 0u64;
    let cp = read_common_prefix(br)?;
    let (layer, flags_bits, net_idx, ci) = (cp.layer, cp.flags, cp.net_index, cp.component_index);
    let center = read_coord_point(br)?;
    let radius = Coord::from_raw(br.read_i32()?);
    let start_angle = br.read_f64()?;
    let end_angle = br.read_f64()?;
    let width = Coord::from_raw(br.read_i32()?);

    let consumed = br.position()? - start;
    let remaining = u64::from(size) - consumed;
    if remaining > 0 {
        br.skip(remaining)?;
    }

    let pf = PrimitiveFlags::decode(flags_bits);
    let mut arc = Arc {
        center,
        radius,
        start_angle,
        end_angle,
        width,
        layer: i32::from(layer),
        component_index: ci,
        ..Arc::default()
    };
    arc.net_index = if net_idx >= 0 { net_idx as u16 } else { 0 };
    arc.is_locked = pf.is_locked;
    arc.is_tenting_top = pf.is_tenting_top;
    arc.is_tenting_bottom = pf.is_tenting_bottom;
    arc.is_keepout = pf.is_keepout;
    arc.flags_extra = pf.extra;
    arc.is_polygon_outline = pf.is_polygon_outline;
    arc.raw_record = Some(body);
    Ok(Some(arc))
}

fn read_pad<R: Read + Seek>(br: &mut BinaryReader<R>) -> Result<Option<Pad>> {
    let designator = br.read_pascal_string_block()?;
    let reserved_block_after_designator = br.read_block()?;
    let net_string_block = br.read_pascal_string_block()?;
    let reserved_block_after_net_string = br.read_block()?;

    let main_body = br.read_block()?;
    if main_body.is_empty() {
        return Ok(None);
    }
    let size = main_body.len() as u32;
    let mut main_cur = BinaryReader::new(std::io::Cursor::new(main_body.as_slice()))?;
    let outer_br = br;
    let br = &mut main_cur;

    let start = 0u64;
    let cp = read_common_prefix(br)?;
    let (layer, flags_bits, net_idx, component_index) =
        (cp.layer, cp.flags, cp.net_index, cp.component_index);

    let location = read_coord_point(br)?;
    let size_top = read_coord_point(br)?;
    let size_middle = read_coord_point(br)?;
    let size_bottom = read_coord_point(br)?;
    let hole_size = Coord::from_raw(br.read_i32()?);
    let mut shape_top = br.read_u8()?;
    let mut shape_middle = br.read_u8()?;
    let mut shape_bottom = br.read_u8()?;
    let rotation = br.read_f64()?;
    let is_plated = br.read_u8()? != 0;

    // Extended fields (offsets 61..114).
    let block_size = u64::from(size);
    let mut stack_mode = 0i32;
    let mut power_plane_connect_style = 0u8;
    let mut relief_air_gap = 0i32;
    let mut relief_conductor_width = 0i32;
    let mut relief_entries = 4i16;
    let mut power_plane_clearance = 0i32;
    let mut power_plane_relief_expansion = 0i32;
    let mut paste_mask_expansion = 0i32;
    let mut solder_mask_expansion = 0i32;
    let mut drill_type = 0u8;
    let mut jumper_id = 0i16;

    if block_size - (br.position()? - start) >= 25 {
        br.skip(1)?; // offset 61
        stack_mode = i32::from(br.read_u8()?);
        power_plane_connect_style = br.read_u8()?;
        relief_air_gap = br.read_i32()?;
        relief_conductor_width = br.read_i32()?;
        relief_entries = br.read_i16()?;
        power_plane_clearance = br.read_i32()?;
        power_plane_relief_expansion = br.read_i32()?;
        br.skip(4)?;
    }
    if block_size - (br.position()? - start) >= 8 {
        paste_mask_expansion = br.read_i32()?;
        solder_mask_expansion = br.read_i32()?;
    }
    if block_size - (br.position()? - start) >= 16 {
        br.skip(9)?;
        drill_type = br.read_u8()?;
        br.skip(6)?;
    }
    if block_size - (br.position()? - start) >= 4 {
        jumper_id = br.read_i16()?;
        br.skip(2)?;
    }
    let r = block_size - (br.position()? - start);
    if r > 0 {
        br.skip(r)?;
    }

    // 596-byte size/shape block — read from the outer stream; the main
    // block cursor is exhausted.
    let ss_body = outer_br.read_block()?;
    let ss_size = ss_body.len() as u64;
    let mut ss_cur = BinaryReader::new(std::io::Cursor::new(ss_body.as_slice()))?;
    let br = &mut ss_cur;
    let has_size_shape_block = ss_size > 0;

    let mut layer_x_sizes = [0i32; 29];
    let mut layer_y_sizes = [0i32; 29];
    let mut internal_layer_shapes = [0u8; 29];
    let mut hole_shape_byte = 0u8;
    let mut hole_slot_length = 0i32;
    let mut hole_rotation = 0f64;
    let mut offset_x = [0i32; 32];
    let mut offset_y = [0i32; 32];
    let mut has_rounded_rect_byte = 0u8;
    let mut per_layer_shapes = [0u8; 32];
    let mut per_layer_corner_radii = [0u8; 32];

    if ss_size >= 596 {
        let ss_start = br.position()?;
        for slot in &mut layer_x_sizes {
            *slot = br.read_i32()?;
        }
        for slot in &mut layer_y_sizes {
            *slot = br.read_i32()?;
        }
        for slot in &mut internal_layer_shapes {
            *slot = br.read_u8()?;
        }
        br.skip(1)?;
        hole_shape_byte = br.read_u8()?;
        hole_slot_length = br.read_i32()?;
        hole_rotation = br.read_f64()?;
        for slot in &mut offset_x {
            *slot = br.read_i32()?;
        }
        for slot in &mut offset_y {
            *slot = br.read_i32()?;
        }
        has_rounded_rect_byte = br.read_u8()?;
        for slot in &mut per_layer_shapes {
            *slot = br.read_u8()?;
        }
        for slot in &mut per_layer_corner_radii {
            *slot = br.read_u8()?;
        }
        let consumed = br.position()? - ss_start;
        if ss_size > consumed {
            br.skip(ss_size - consumed)?;
        }
    } else if ss_size > 0 {
        br.skip(ss_size)?;
    }

    if has_rounded_rect_byte != 0 {
        shape_top = per_layer_shapes[0];
        shape_middle = per_layer_shapes[1];
        shape_bottom = if stack_mode == 0 {
            shape_top
        } else {
            per_layer_shapes[31]
        };
    }

    let mut pad = Pad::default();
    pad.designator = if designator.is_empty() {
        None
    } else {
        Some(designator)
    };
    pad.location = location;
    pad.size_top = size_top;
    pad.size_middle = size_middle;
    pad.size_bottom = size_bottom;
    pad.hole_size = hole_size;
    pad.shape_top = PadShape::from_raw(i32::from(shape_top));
    pad.shape_middle = PadShape::from_raw(i32::from(shape_middle));
    pad.shape_bottom = PadShape::from_raw(i32::from(shape_bottom));
    // Preserve the raw shape bytes so consumers can detect custom /
    // unmodelled shape values (anything outside {1,2,3,9}). The
    // `top_shape` / `mid_shape` / `bot_shape` legacy fields are
    // otherwise default-zero and unused.
    pad.top_shape = i32::from(shape_top);
    pad.mid_shape = i32::from(shape_middle);
    pad.bot_shape = i32::from(shape_bottom);
    // Mirror size_top into the legacy `top_x_size` / `top_y_size`
    // so downstream code that prefers the i32-typed fields gets
    // the right values too.
    pad.top_x_size = size_top.x;
    pad.top_y_size = size_top.y;
    pad.mid_x_size = size_middle.x;
    pad.mid_y_size = size_middle.y;
    pad.bot_x_size = size_bottom.x;
    pad.bot_y_size = size_bottom.y;
    pad.rotation = rotation;
    pad.is_plated = is_plated;
    pad.layer = i32::from(layer);
    pad.component_index = component_index;
    pad.mode = stack_mode;
    pad.power_plane_connect_style = i32::from(power_plane_connect_style);
    pad.relief_air_gap = Coord::from_raw(relief_air_gap);
    pad.relief_conductor_width = Coord::from_raw(relief_conductor_width);
    pad.relief_entries = i32::from(relief_entries);
    pad.power_plane_clearance = Coord::from_raw(power_plane_clearance);
    pad.power_plane_relief_expansion = Coord::from_raw(power_plane_relief_expansion);
    pad.paste_mask_expansion = Coord::from_raw(paste_mask_expansion);
    pad.solder_mask_expansion = Coord::from_raw(solder_mask_expansion);
    pad.drill_type = i32::from(drill_type);
    pad.jumper_id = i32::from(jumper_id);
    pad.layer_x_sizes = layer_x_sizes;
    pad.layer_y_sizes = layer_y_sizes;
    pad.internal_layer_shapes = internal_layer_shapes;
    pad.hole_type = PadHoleType::try_from(i32::from(hole_shape_byte)).unwrap_or(PadHoleType::Round);
    pad.hole_slot_length = hole_slot_length;
    pad.hole_rotation = hole_rotation;
    pad.offset_x_from_hole_center = offset_x;
    pad.offset_y_from_hole_center = offset_y;
    pad.has_rounded_rect_byte = has_rounded_rect_byte;
    pad.per_layer_shapes = per_layer_shapes;
    pad.per_layer_corner_radii = per_layer_corner_radii;
    pad.has_size_shape_block = has_size_shape_block;
    pad.reserved_block_after_designator = reserved_block_after_designator;
    pad.reserved_block_after_net_string = reserved_block_after_net_string;
    pad.net_string_block = net_string_block;

    pad.net_index = if net_idx >= 0 { net_idx as u16 } else { 0 };

    let pf = PrimitiveFlags::decode(flags_bits);
    pad.is_locked = pf.is_locked;
    pad.is_tenting_top = pf.is_tenting_top;
    pad.is_tenting_bottom = pf.is_tenting_bottom;
    pad.is_keepout = pf.is_keepout;
    pad.flags_extra = pf.extra;
    pad.is_testpoint_fab_top = pf.is_testpoint_fab_top;
    pad.is_testpoint_fab_bottom = pf.is_testpoint_fab_bottom;

    // Top-layer corner-radius percentage. Altium stores per-layer
    // values in the SS block's `per_layer_corner_radii` array (0..100,
    // % of half the smaller side). The legacy `corner_radius_percentage`
    // global isn't a separate disk field — populate it from the top
    // slot so consumers (renderer included) can read either source.
    pad.corner_radius_percentage = i32::from(per_layer_corner_radii[0]);

    // SMD vs through-hole derivation. Altium doesn't store these as
    // explicit bits; consumers (paste-mask tooling, BOM exporters)
    // compute them from layer + hole presence. A pad with no drilled
    // hole on a single signal layer (top = 1, bottom = 32) is SMT.
    let layer_i32 = i32::from(layer);
    let has_drill = hole_size.to_raw() > 0;
    pad.is_surface_mount = !has_drill && matches!(layer_i32, 1 | 32);

    // Paste-mask coverage. Altium opens the paste layer wherever the
    // pad has copper. Without a "paste disabled" bit in the flags
    // byte, top-layer pads (1) and multi-layer pads (74) get top
    // paste; bottom-layer (32) and multi-layer get bottom paste.
    // Tenting is solder mask, not paste mask — orthogonal.
    pad.is_top_paste_enabled = matches!(layer_i32, 1 | 74);
    pad.is_bottom_paste_enabled = matches!(layer_i32, 32 | 74);

    // Per-layer shape overrides. `has_rounded_rect_byte` is the
    // Altium signal that the SS-block per-layer shape/radius arrays
    // are authoritative (otherwise the global shape byte applies to
    // every layer).
    pad.has_rounded_rectangular_shapes = has_rounded_rect_byte != 0;
    // Per-layer corner radii diverge → user customised them in the
    // pad editor (mixed corner radii across the stack).
    let top_radius = per_layer_corner_radii[0];
    pad.has_custom_rounded_rectangle = has_rounded_rect_byte != 0
        && per_layer_corner_radii
            .iter()
            .take(32)
            .any(|&r| r != top_radius);
    // Shape byte the library doesn't model (anything outside the
    // canonical {1,2,3,9}) → custom-shape pad whose outline is
    // typically carried in an associated Region6 record.
    pad.has_custom_shapes = matches!(pad.shape_top, PadShape::Unknown(_) | PadShape::CustomShape);

    pad.raw_record = Some(main_body);
    pad.raw_size_shape = if ss_body.is_empty() {
        None
    } else {
        Some(ss_body)
    };
    Ok(Some(pad))
}

fn read_via<R: Read + Seek>(br: &mut BinaryReader<R>) -> Result<Option<Via>> {
    let body = br.read_block()?;
    if body.is_empty() {
        return Ok(None);
    }
    let size = body.len() as u32;
    let mut cur = BinaryReader::new(std::io::Cursor::new(body.as_slice()))?;
    let br = &mut cur;
    let start = 0u64;
    let cp = read_common_prefix(br)?;
    let (layer, flags_bits, net_idx, ci) = (cp.layer, cp.flags, cp.net_index, cp.component_index);
    let location = read_coord_point(br)?;
    let diameter = Coord::from_raw(br.read_i32()?);
    let hole_size = Coord::from_raw(br.read_i32()?);
    let from_layer = br.read_u8()?;
    let to_layer = br.read_u8()?;

    let mut via = Via {
        location,
        diameter,
        hole_size,
        start_layer: i32::from(from_layer),
        end_layer: i32::from(to_layer),
        layer: i32::from(layer),
        net_index: if net_idx >= 0 { net_idx as u16 } else { 0 },
        component_index: ci,
        ..Via::default()
    };
    let pf = PrimitiveFlags::decode(flags_bits);
    via.is_locked = pf.is_locked;
    via.is_tenting_top = pf.is_tenting_top;
    via.is_tenting_bottom = pf.is_tenting_bottom;
    via.is_keepout = pf.is_keepout;
    via.flags_extra = pf.extra;
    via.is_testpoint_fab_top = pf.is_testpoint_fab_top;
    via.is_testpoint_fab_bottom = pf.is_testpoint_fab_bottom;

    let consumed = br.position()? - start;
    let block_size = u64::from(size);
    if consumed < block_size {
        br.skip(1)?;
        via.thermal_relief_air_gap = Coord::from_raw(br.read_i32()?);
        via.thermal_relief_conductors = i32::from(br.read_u8()?);
        br.skip(1)?;
        via.thermal_relief_conductors_width = Coord::from_raw(br.read_i32()?);
        via.power_plane_clearance = Coord::from_raw(br.read_i32()?);
        via.power_plane_relief_expansion = Coord::from_raw(br.read_i32()?);
        br.skip(4)?;
        via.solder_mask_expansion = Coord::from_raw(br.read_i32()?);

        // Capture the 8 reserved bytes Altium emits as [0,0,0,1,1,1,1,0]
        // so non-canonical files round-trip exactly.
        br.read_exact(&mut via.reserved_block_8)?;
        let solder_mask_manual = br.read_u8()?;
        via.solder_mask_expansion_manual = solder_mask_manual == 2;
        via.reserved_byte_after_mask_flag = br.read_u8()?;
        br.skip(2)?;
        br.skip(4)?;
        via.mode = i32::from(br.read_u8()?);
        for slot in &mut via.diameters {
            *slot = Coord::from_raw(br.read_i32()?);
        }
        via.trailing_reserved_i16 = br.read_i16()?;
        via.trailing_reserved_i32 = br.read_i32()?;

        let consumed = br.position()? - start;
        if block_size > consumed {
            br.skip(block_size - consumed)?;
        }
    }

    via.raw_record = Some(body);
    Ok(Some(via))
}

fn read_track<R: Read + Seek>(br: &mut BinaryReader<R>) -> Result<Option<Track>> {
    let body = br.read_block()?;
    if body.is_empty() {
        return Ok(None);
    }
    let size = body.len() as u32;
    let mut cur = BinaryReader::new(std::io::Cursor::new(body.as_slice()))?;
    let br = &mut cur;
    let start = 0u64;
    let cp = read_common_prefix(br)?;
    let (layer, flags_bits, net_idx, ci) = (cp.layer, cp.flags, cp.net_index, cp.component_index);
    let start_x = br.read_i32()?;
    let start_y = br.read_i32()?;
    let end_x = br.read_i32()?;
    let end_y = br.read_i32()?;
    let width = Coord::from_raw(br.read_i32()?);

    // Post-core tail: `[u16 subPolyIndex][u8 pad]`. The component index
    // lives in the common prefix, not here.
    let mut subnet_index = 0u16;
    let block_size = u64::from(size);
    let consumed = br.position()? - start;
    if consumed + 3 <= block_size {
        subnet_index = br.read_u16()?;
        br.skip(1)?;
    }
    let consumed = br.position()? - start;
    if block_size > consumed {
        br.skip(block_size - consumed)?;
    }

    // Prefer the post-core sub-net index when present; otherwise fall back to
    // the common-prefix net index. Either is the binary "no net" sentinel
    // 0xFFFF if absent.
    let net_index = if subnet_index != 0 && subnet_index != 0xFFFF {
        subnet_index
    } else if net_idx >= 0 {
        net_idx as u16
    } else {
        0
    };

    let mut track = Track {
        start: CoordPoint::new(Coord::from_raw(start_x), Coord::from_raw(start_y)),
        end: CoordPoint::new(Coord::from_raw(end_x), Coord::from_raw(end_y)),
        width,
        layer: i32::from(layer),
        net_index,
        component_index: ci,
        ..Track::default()
    };
    let pf = PrimitiveFlags::decode(flags_bits);
    track.is_locked = pf.is_locked;
    track.is_tenting_top = pf.is_tenting_top;
    track.is_tenting_bottom = pf.is_tenting_bottom;
    track.is_keepout = pf.is_keepout;
    track.flags_extra = pf.extra;
    track.is_polygon_outline = pf.is_polygon_outline;
    track.raw_record = Some(body);
    Ok(Some(track))
}

fn read_text<R: Read + Seek>(
    br: &mut BinaryReader<R>,
    wide_strings: &[String],
) -> Result<Option<Text>> {
    let (_flags_byte, size) = br.read_block_header()?;
    if size == 0 {
        return Ok(None);
    }
    let start = br.position()?;
    let cp = read_common_prefix(br)?;
    let (layer, flags_bits, net_idx, ci) = (cp.layer, cp.flags, cp.net_index, cp.component_index);

    let corner1 = read_coord_point(br)?;
    let height = br.read_i32()?;
    let stroke_font = br.read_i16()?;
    let rotation = br.read_f64()?;
    let mirrored = br.read_u8()? != 0;
    let stroke_width = br.read_i32()?;

    let mut text_kind = PcbTextKind::Stroke;
    let mut font_bold = false;
    let mut font_italic = false;
    let mut font_name: Option<String> = None;
    let barcode_lr_margin = 0i32;
    let barcode_tb_margin = 0i32;
    let mut font_inverted = false;
    let mut font_inverted_border = 0i32;
    let mut wide_string_index = -1i32;
    let mut font_inverted_rect = false;
    let mut font_inverted_rect_width = 0i32;
    let mut font_inverted_rect_height = 0i32;
    let mut font_inverted_rect_justification = 0u8;
    let mut font_inverted_rect_text_offset = 0i32;
    let mut is_comment = false;
    let mut is_designator = false;
    let mut union_index = 0i32;
    let mut has_extended_tail = false;
    let mut bar_code_full_width = 0i32;
    let mut bar_code_full_height = 0i32;
    let mut bar_code_x_margin = 0i32;
    let mut bar_code_y_margin = 0i32;
    let mut bar_code_min_width = 0i32;
    let mut bar_code_kind = 0u8;
    let mut bar_code_render_mode = 0u8;
    let mut bar_code_inverted = false;
    let mut bar_code_font_name: Option<String> = None;
    let mut bar_code_show_text = false;
    let mut is_frame = false;
    let mut is_offset_border = false;
    let mut v7_tail = 0u32;
    let mut justification_valid = false;
    let mut advance_snapping = false;
    let mut snap_point_x = 0i32;
    let mut snap_point_y = 0i32;

    let block_size = u64::from(size);
    if block_size >= 123 {
        // Two component-role flag bytes precede the extension proper (same
        // layout KiCad's ATEXT6 importer reads): the dedicated Comment and
        // Name (designator) texts of a component are marked here.
        is_comment = br.read_u8()? != 0;
        is_designator = br.read_u8()? != 0;
        br.read_u8()?; // ext
        text_kind = PcbTextKind::try_from(i32::from(br.read_u8()?)).unwrap_or(PcbTextKind::Stroke);
        font_bold = br.read_u8()? != 0;
        font_italic = br.read_u8()? != 0;
        font_name = Some(br.read_font_name()?);
        // The inverted/wide-string fields follow the font name directly
        // (subrecord offsets 110..137, matching real Altium output and
        // KiCad's importer); barcode fields live in an optional tail that
        // the trailing skip below consumes.
        font_inverted = br.read_u8()? != 0;
        font_inverted_border = br.read_i32()?;
        wide_string_index = br.read_i32()?;
        union_index = br.read_i32()?;
        font_inverted_rect = br.read_u8()? != 0;
        font_inverted_rect_width = br.read_i32()?;
        font_inverted_rect_height = br.read_i32()?;
        font_inverted_rect_justification = br.read_u8()?;
        font_inverted_rect_text_offset = br.read_i32()?;
    }
    // Altium writes a 252-byte record: barcode block, the authoritative
    // text-kind byte, V7 layer id, frame flags, the justification-valid flag
    // and the snap point (offsets 137..252, the same layout KiCad's ATEXT6
    // importer reads).
    if block_size >= 252 {
        has_extended_tail = true;
        bar_code_full_width = br.read_i32()?; // 137
        bar_code_full_height = br.read_i32()?; // 141
        bar_code_x_margin = br.read_i32()?; // 145
        bar_code_y_margin = br.read_i32()?; // 149
        bar_code_min_width = br.read_i32()?; // 153
        bar_code_kind = br.read_u8()?; // 157
        bar_code_render_mode = br.read_u8()?; // 158
        bar_code_inverted = br.read_u8()? != 0; // 159
        let kind_byte = br.read_u8()?; // 160 authoritative text kind
        text_kind = PcbTextKind::try_from(i32::from(kind_byte)).unwrap_or(text_kind);
        bar_code_font_name = Some(br.read_font_name()?); // 161..225
        bar_code_show_text = br.read_u8()? != 0; // 225
        v7_tail = br.read_u32()?; // 226 V7 layer id
        is_frame = br.read_u8()? != 0; // 230
        is_offset_border = br.read_u8()? != 0; // 231
        br.skip(8)?; // 232 two reserved i32 (0x8000_0000)
        justification_valid = br.read_u8()? != 0; // 240
        advance_snapping = br.read_u8()? != 0; // 241
        br.skip(2)?; // 242
        snap_point_x = br.read_i32()?; // 244
        snap_point_y = br.read_i32()?; // 248
    }

    let consumed = br.position()? - start;
    if block_size > consumed {
        br.skip(block_size - consumed)?;
    }

    let ascii_text = br.read_pascal_string_block()?;
    let text_value = if (0..wide_strings.len() as i32).contains(&wide_string_index) {
        wide_strings[wide_string_index as usize].clone()
    } else {
        ascii_text
    };
    if text_value.is_empty() {
        return Ok(None);
    }

    let mut text = Text {
        text: text_value,
        location: corner1,
        height: Coord::from_raw(height),
        stroke_width: Coord::from_raw(stroke_width),
        rotation,
        layer: i32::from(layer),
        net_index: if net_idx >= 0 { net_idx as u16 } else { 0 },
        component_index: ci,
        is_mirrored: mirrored,
        ..Text::default()
    };
    text.stroke_font =
        PcbStrokeFont::try_from(i32::from(stroke_font)).unwrap_or(PcbStrokeFont::Default);
    text.text_kind = text_kind;
    text.is_truetype = matches!(text_kind, PcbTextKind::TrueType);
    text.font_bold = font_bold;
    text.font_italic = font_italic;
    text.font_name = font_name;
    text.is_inverted = font_inverted;
    text.inverted_border = Coord::from_raw(font_inverted_border);
    text.use_inverted_rectangle = font_inverted_rect;
    text.inverted_rect_width = Coord::from_raw(font_inverted_rect_width);
    text.inverted_rect_height = Coord::from_raw(font_inverted_rect_height);
    // Offset 132 is Altium's `TTextAutoposition` byte (0 = manual) — the one
    // justification the format carries, for both the inverted-rect frame and
    // (when `justification_valid` is set) the text itself.
    let justification = TextJustification::from_pcb_autoposition(font_inverted_rect_justification)
        .unwrap_or(TextJustification::BottomLeft);
    text.justification = justification;
    text.inverted_rect_justification = justification;
    text.inverted_rect_text_offset = Coord::from_raw(font_inverted_rect_text_offset);
    text.barcode_lr_margin = Coord::from_raw(barcode_lr_margin);
    text.barcode_tb_margin = Coord::from_raw(barcode_tb_margin);
    text.wide_string_index = wide_string_index;
    text.is_comment = is_comment;
    text.is_designator = is_designator;
    text.union_index = union_index;
    if has_extended_tail {
        text.bar_code_full_width = Coord::from_raw(bar_code_full_width);
        text.bar_code_full_height = Coord::from_raw(bar_code_full_height);
        text.bar_code_x_margin = Coord::from_raw(bar_code_x_margin);
        text.bar_code_y_margin = Coord::from_raw(bar_code_y_margin);
        text.bar_code_min_width = Coord::from_raw(bar_code_min_width);
        text.bar_code_kind = i32::from(bar_code_kind);
        text.bar_code_render_mode = i32::from(bar_code_render_mode);
        text.bar_code_inverted = bar_code_inverted;
        text.bar_code_font_name = bar_code_font_name;
        text.bar_code_show_text = bar_code_show_text;
        text.is_frame = is_frame;
        text.is_offset_border = is_offset_border;
        text.justification_valid = justification_valid;
        text.advance_snapping = advance_snapping;
        text.snap_point_x = Coord::from_raw(snap_point_x);
        text.snap_point_y = Coord::from_raw(snap_point_y);
    }

    let pf = PrimitiveFlags::decode(flags_bits);
    text.is_locked = pf.is_locked;
    text.is_tenting_top = pf.is_tenting_top;
    text.is_tenting_bottom = pf.is_tenting_bottom;
    text.is_keepout = pf.is_keepout;
    text.flags_extra = pf.extra;
    // Mechanical layers 17..32 do not fit the layer byte (Altium clamps it to
    // Mechanical 16) and live only in the V7 id; keep it when it disagrees.
    text.layer_v7 = if v7_tail == super::writer::v7_layer_id(text.layer) {
        0
    } else {
        v7_tail
    };
    Ok(Some(text))
}

fn read_fill<R: Read + Seek>(br: &mut BinaryReader<R>) -> Result<Option<Fill>> {
    let body = br.read_block()?;
    if body.is_empty() {
        return Ok(None);
    }
    let size = body.len() as u32;
    let mut cur = BinaryReader::new(std::io::Cursor::new(body.as_slice()))?;
    let br = &mut cur;
    let start = 0u64;
    let cp = read_common_prefix(br)?;
    let (layer, flags_bits, net_idx, ci) = (cp.layer, cp.flags, cp.net_index, cp.component_index);
    let corner1 = read_coord_point(br)?;
    let corner2 = read_coord_point(br)?;
    let rotation = br.read_f64()?;

    let consumed = br.position()? - start;
    let block_size = u64::from(size);
    if block_size > consumed {
        br.skip(block_size - consumed)?;
    }

    let mut fill = Fill {
        corner1,
        corner2,
        layer: i32::from(layer),
        rotation,
        component_index: ci,
        ..Fill::default()
    };
    fill.net_index = if net_idx >= 0 { net_idx as u16 } else { 0 };
    let pf = PrimitiveFlags::decode(flags_bits);
    fill.is_locked = pf.is_locked;
    fill.is_tenting_top = pf.is_tenting_top;
    fill.is_tenting_bottom = pf.is_tenting_bottom;
    fill.is_keepout = pf.is_keepout;
    fill.flags_extra = pf.extra;
    fill.raw_record = Some(body);
    Ok(Some(fill))
}

fn read_region<R: Read + Seek>(br: &mut BinaryReader<R>) -> Result<Option<Region>> {
    let body = br.read_block()?;
    if body.is_empty() {
        return Ok(None);
    }
    let size = body.len() as u32;
    let mut cur = BinaryReader::new(std::io::Cursor::new(body.as_slice()))?;
    let br = &mut cur;
    let start = 0u64;
    let cp = read_common_prefix(br)?;
    let (layer, flags_bits, net_idx, ci) = (cp.layer, cp.flags, cp.net_index, cp.component_index);
    br.skip(4)?; // reserved u32
    br.skip(1)?; // reserved u8

    let parameters = read_param_map(br)?;
    let vertex_count = br.read_u32()?;

    let mut region = Region::default();
    region.layer = i32::from(layer);
    region.net_index = if net_idx >= 0 { net_idx as u16 } else { 0 };
    region.component_index = ci;
    region.kind = parameters.get_i32("KIND").unwrap_or(0);
    for _ in 0..vertex_count {
        let x = Coord::from_raw(br.read_f64()? as i32);
        let y = Coord::from_raw(br.read_f64()? as i32);
        region.outline.push(CoordPoint::new(x, y));
    }

    // Hole outlines follow the main outline, one [u32 n][n x (f64,f64)]
    // list each; HOLECOUNT in the param block is authoritative.
    let hole_count = parameters.get_i32("HOLECOUNT").unwrap_or(0).max(0);
    for _ in 0..hole_count {
        let remaining = u64::from(size).saturating_sub(br.position()? - start);
        if remaining < 4 {
            break;
        }
        let n = br.read_u32()? as u64;
        let remaining = u64::from(size).saturating_sub(br.position()? - start);
        if n * 16 > remaining {
            break;
        }
        let mut hole = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let x = Coord::from_raw(br.read_f64()? as i32);
            let y = Coord::from_raw(br.read_f64()? as i32);
            hole.push(CoordPoint::new(x, y));
        }
        region.holes.push(hole);
    }

    let consumed = br.position()? - start;
    let block_size = u64::from(size);
    if block_size > consumed {
        br.skip(block_size - consumed)?;
    }

    if let Some(v) = parameters.get("NET") {
        region.net = Some(v.to_string());
    }
    if let Some(v) = parameters.get("UNIQUEID") {
        region.unique_id = Some(v.to_string());
    }
    if let Some(v) = parameters.get("NAME") {
        region.name = Some(v.to_string());
    }

    // Numeric / coord parameters. Region6 stores coords as raw i32
    // (same convention `read_component_body` uses).
    if let Some(v) = parameters.get_i32("SUBPOLYINDEX") {
        region.sub_poly_index = v;
    }
    if let Some(v) = parameters.get_i32("UNIONINDEX") {
        region.union_index = v;
    }
    if let Some(v) = parameters.get_f64("ARCRESOLUTION") {
        region.arc_resolution = v;
    }
    if let Some(v) = parameters.get_i32("PASTEMASKEXPANSION") {
        region.paste_mask_expansion = Coord::from_raw(v);
    }
    if let Some(v) = parameters.get_i32("SOLDERMASKEXPANSION") {
        region.solder_mask_expansion = Coord::from_raw(v);
    }
    if let Some(v) = parameters.get_i32("CAVITYHEIGHT") {
        region.cavity_height = Coord::from_raw(v);
    }
    if let Some(v) = parameters.get_i32("POWERPLANECLEARANCE") {
        region.power_plane_clearance = Coord::from_raw(v);
    }
    if let Some(v) = parameters.get_i32("POWERPLANECONNECTSTYLE") {
        region.power_plane_connect_style = v;
    }
    if let Some(v) = parameters.get_i32("POWERPLANERELIEFEXPANSION") {
        region.power_plane_relief_expansion = Coord::from_raw(v);
    }
    if let Some(v) = parameters.get_i32("RELIEFAIRGAP") {
        region.relief_air_gap = Coord::from_raw(v);
    }
    if let Some(v) = parameters.get_i32("RELIEFCONDUCTORWIDTH") {
        region.relief_conductor_width = Coord::from_raw(v);
    }
    if let Some(v) = parameters.get_i32("RELIEFENTRIES") {
        region.relief_entries = v;
    }
    if let Some(v) = parameters.get_i32("HOLECOUNT") {
        region.hole_count = v;
    }
    if let Some(v) = parameters.get_i32("TOTALVERTEXCOUNT") {
        region.total_vertex_count = v;
    }
    if let Some(v) = parameters.get_i64("AREA") {
        region.area = v;
    }
    if let Some(v) = parameters.get_i32("ARCAPPROXIMATION") {
        region.arc_approximation = Coord::from_raw(v);
    }

    // Booleans. Altium uses explicit `TRUE`/`FALSE` strings (case
    // insensitive); absence means "leave default". Several of these
    // (e.g. `ENABLED`) default to true and only appear in the file
    // when they're false.
    if let Some(v) = parameters.get("ISSHAPEBASED") {
        region.is_shape_based = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ENABLED") {
        region.enabled = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("USERROUTED") {
        region.user_routed = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ISFREEPRIM") {
        region.is_free_primitive = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ISELECTRICALPRIM") {
        region.is_electrical_prim = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ISPREROUTE") {
        region.is_pre_route = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("TEARDROP") {
        region.tear_drop = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("POLYGONOUTLINE") {
        region.polygon_outline = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ISTENTING") {
        region.is_tenting = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ISTESTPOINTTOP") {
        region.is_testpoint_top = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ISTESTPOINTBOTTOM") {
        region.is_testpoint_bottom = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ISASSEMBLYTESTPOINTTOP") {
        region.is_assy_testpoint_top = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ISASSEMBLYTESTPOINTBOTTOM") {
        region.is_assy_testpoint_bottom = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ISHIDDEN") {
        region.is_hidden = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ALLOWGLOBALEDIT") {
        region.allow_global_edit = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("MOVEABLE") {
        region.moveable = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("ISSIMPLEREGION") {
        region.is_simple_region = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = parameters.get("VIRTUALCUTOUT") {
        region.virtual_cutout = v.eq_ignore_ascii_case("TRUE");
    }

    let pf = PrimitiveFlags::decode(flags_bits);
    region.is_locked = pf.is_locked;
    region.is_tenting_top = pf.is_tenting_top;
    region.is_tenting_bottom = pf.is_tenting_bottom;
    region.is_keepout = pf.is_keepout;
    region.flags_extra = pf.extra;
    region.is_teardrop = pf.is_teardrop;

    let consumed_keys = [
        "KIND",
        "NET",
        "UNIQUEID",
        "NAME",
        "SUBPOLYINDEX",
        "UNIONINDEX",
        "ARCRESOLUTION",
        "PASTEMASKEXPANSION",
        "SOLDERMASKEXPANSION",
        "CAVITYHEIGHT",
        "POWERPLANECLEARANCE",
        "POWERPLANECONNECTSTYLE",
        "POWERPLANERELIEFEXPANSION",
        "RELIEFAIRGAP",
        "RELIEFCONDUCTORWIDTH",
        "RELIEFENTRIES",
        "HOLECOUNT",
        "TOTALVERTEXCOUNT",
        "AREA",
        "ARCAPPROXIMATION",
        "ISSHAPEBASED",
        "ENABLED",
        "USERROUTED",
        "ISFREEPRIM",
        "ISELECTRICALPRIM",
        "ISPREROUTE",
        "TEARDROP",
        "POLYGONOUTLINE",
        "ISTENTING",
        "ISTESTPOINTTOP",
        "ISTESTPOINTBOTTOM",
        "ISASSEMBLYTESTPOINTTOP",
        "ISASSEMBLYTESTPOINTBOTTOM",
        "ISHIDDEN",
        "ALLOWGLOBALEDIT",
        "MOVEABLE",
        "ISSIMPLEREGION",
        "VIRTUALCUTOUT",
    ];
    let extra = extract_remaining_parameters(&parameters, &consumed_keys);
    if !extra.is_empty() {
        region.additional_parameters = Some(extra);
    }
    region.raw_record = Some(body);
    Ok(Some(region))
}

fn read_component_body<R: Read + Seek>(br: &mut BinaryReader<R>) -> Result<Option<ComponentBody>> {
    let (_flags_byte, size) = br.read_block_header()?;
    if size == 0 {
        return Ok(None);
    }
    let start = br.position()?;
    let cp = read_common_prefix(br)?;
    let (layer, flags_bits, net_idx, ci) = (cp.layer, cp.flags, cp.net_index, cp.component_index);
    br.skip(4)?;
    br.skip(1)?;

    let parameters = read_param_map(br)?;
    let vertex_count = br.read_u32()?;

    let mut body = ComponentBody::default();
    body.layer = i32::from(layer);
    body.net_index = if net_idx >= 0 { net_idx as u16 } else { 0 };
    body.component_index = ci;
    for _ in 0..vertex_count {
        let x = Coord::from_raw(br.read_f64()? as i32);
        let y = Coord::from_raw(br.read_f64()? as i32);
        body.outline.push(CoordPoint::new(x, y));
    }

    let consumed = br.position()? - start;
    let block_size = u64::from(size);
    if block_size > consumed {
        br.skip(block_size - consumed)?;
    }

    if let Some(v) = parameters.get("V7_LAYER") {
        body.layer_name = v.to_string();
    }
    if let Some(v) = parameters.get("NAME") {
        body.name = Some(v.to_string());
    }
    if let Some(v) = parameters.get_i32("KIND") {
        body.kind = v;
    }
    if let Some(v) = parameters.get_i32("SUBPOLYINDEX") {
        body.sub_poly_index = v;
    }
    if let Some(v) = parameters.get_i32("UNIONINDEX") {
        body.union_index = v;
    }
    // Coord-valued body parameters are mil strings ("-62.9921mil"), not
    // raw integers.
    let get_coord = |key: &str| -> Option<Coord> {
        parameters
            .get(key)
            .and_then(super::doc_codec::parse_coord_loose)
    };
    if let Some(v) = get_coord("ARCRESOLUTION") {
        body.arc_resolution = v.to_mils();
    } else if let Some(v) = parameters.get_f64("ARCRESOLUTION") {
        body.arc_resolution = v;
    }
    if let Some(v) = parameters.get("ISSHAPEBASED") {
        body.is_shape_based = v.eq_ignore_ascii_case("TRUE");
    }
    if let Some(v) = get_coord("CAVITYHEIGHT") {
        body.cavity_height = v;
    }
    if let Some(v) = get_coord("STANDOFFHEIGHT") {
        body.standoff_height = v;
    }
    if let Some(v) = get_coord("OVERALLHEIGHT") {
        body.overall_height = v;
    }
    if let Some(v) = parameters.get_i32("BODYCOLOR3D") {
        body.body_color_3d = v;
    }
    if let Some(v) = parameters.get_f64("BODYOPACITY3D") {
        body.body_opacity_3d = v;
    }
    if let Some(v) = parameters.get("MODELID") {
        body.model_id = Some(v.to_string());
    }
    if let Some(v) = parameters.get("MODEL.EMBED") {
        body.model_embed = v.eq_ignore_ascii_case("TRUE");
    }
    if parameters.contains_key("MODEL.2D.X") || parameters.contains_key("MODEL.2D.Y") {
        let m2dx = get_coord("MODEL.2D.X").unwrap_or(Coord::ZERO);
        let m2dy = get_coord("MODEL.2D.Y").unwrap_or(Coord::ZERO);
        body.model_2d_location = CoordPoint::new(m2dx, m2dy);
    }
    if let Some(v) = parameters.get_f64("MODEL.2D.ROTATION") {
        body.model_2d_rotation = v;
    }
    if let Some(v) = parameters.get_f64("MODEL.3D.ROTX") {
        body.model_3d_rot_x = v;
    }
    if let Some(v) = parameters.get_f64("MODEL.3D.ROTY") {
        body.model_3d_rot_y = v;
    }
    if let Some(v) = parameters.get_f64("MODEL.3D.ROTZ") {
        body.model_3d_rot_z = v;
    }
    if let Some(v) = get_coord("MODEL.3D.DZ") {
        body.model_3d_dz = v;
    }
    // Altium writes the body checksum as an unsigned decimal (the model
    // record stores the same bits signed); keep the bit pattern.
    if let Some(v) = parameters
        .get("MODEL.CHECKSUM")
        .and_then(|s| s.trim().parse::<i64>().ok())
    {
        body.model_checksum = v as u32 as i32;
    }
    if let Some(v) = parameters.get("MODEL.NAME") {
        body.model_name = Some(v.to_string());
    }
    if let Some(v) = parameters.get_i32("MODEL.MODELTYPE") {
        body.model_type = v;
    }
    // Absent on bodies written by older Altium versions; keep it absent.
    body.model_source = parameters.get("MODEL.MODELSOURCE").map(|v| v.to_string());
    if let Some(v) = parameters.get_i32("BODYPROJECTION") {
        body.body_projection = v;
    }
    if let Some(v) = parameters.get("IDENTIFIER") {
        body.identifier = Some(v.to_string());
    }
    if let Some(v) = parameters.get("TEXTURE") {
        body.texture = Some(v.to_string());
    }

    let pf = PrimitiveFlags::decode(flags_bits);
    body.is_locked = pf.is_locked;
    body.is_tenting_top = pf.is_tenting_top;
    body.is_tenting_bottom = pf.is_tenting_bottom;
    body.is_keepout = pf.is_keepout;
    body.flags_extra = pf.extra;

    let consumed_keys = [
        "V7_LAYER",
        "NAME",
        "KIND",
        "SUBPOLYINDEX",
        "UNIONINDEX",
        "ARCRESOLUTION",
        "ISSHAPEBASED",
        "CAVITYHEIGHT",
        "STANDOFFHEIGHT",
        "OVERALLHEIGHT",
        "BODYCOLOR3D",
        "BODYOPACITY3D",
        "BODYPROJECTION",
        "MODELID",
        "MODEL.EMBED",
        "MODEL.2D.X",
        "MODEL.2D.Y",
        "MODEL.2D.ROTATION",
        "MODEL.3D.ROTX",
        "MODEL.3D.ROTY",
        "MODEL.3D.ROTZ",
        "MODEL.3D.DZ",
        "MODEL.CHECKSUM",
        "MODEL.NAME",
        "MODEL.MODELTYPE",
        "MODEL.MODELSOURCE",
        "IDENTIFIER",
        "TEXTURE",
    ];
    let extra = extract_remaining_parameters(&parameters, &consumed_keys);
    if !extra.is_empty() {
        body.additional_parameters = Some(extra);
    }

    Ok(Some(body))
}

// Document reader (PcbDoc)

use super::document::Document;

const PCB_DOC_KNOWN_STORAGES: &[&str] = &[
    "FileHeader",
    "Board6",
    "Nets6",
    "Arcs6",
    "Pads6",
    "Vias6",
    "Tracks6",
    "Texts6",
    "Fills6",
    "Regions6",
    "ComponentBodies6",
    "Polygons6",
    "Components6",
    "WideStrings6",
    "EmbeddedBoards6",
    "Rules6",
    "Classes6",
    "DifferentialPairs6",
    "Rooms6",
];

impl Document {
    /// Parse a `.PcbDoc` file from an in-memory buffer.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut cf = CompoundFile::open(bytes)?;
        let mut document = Self::default();
        let mut diagnostics = Vec::new();

        let wide_strings = read_doc_wide_strings(&mut cf)?;
        read_board(&mut cf, &mut document)?;
        read_nets(&mut cf, &mut document)?;
        read_components(&mut cf, &mut document)?;
        read_doc_primitive_streams(&mut cf, &mut document, &wide_strings, &mut diagnostics)?;
        read_polygons(&mut cf, &mut document)?;
        read_rules(&mut cf, &mut document)?;
        read_classes(&mut cf, &mut document)?;
        read_differential_pairs(&mut cf, &mut document)?;
        read_rooms(&mut cf, &mut document)?;
        read_embedded_boards(&mut cf, &mut document)?;
        resolve_net_names(&mut document);
        assign_primitives_to_components(&mut document);
        read_doc_additional_streams(&mut cf, &mut document, &mut diagnostics)?;

        document.diagnostics = diagnostics;
        Ok(document)
    }

    /// Read a `.PcbDoc` from disk.
    pub async fn read(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = tokio::fs::read(path).await?;
        Self::from_bytes(bytes)
    }

    /// Read a `.PcbDoc` from any `AsyncRead`.
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

fn read_doc_wide_strings(cf: &mut CompoundFile) -> Result<Vec<String>> {
    let Some(data) = cf.try_read_stream("WideStrings6/Data")? else {
        return Ok(Vec::new());
    };
    if data.is_empty() {
        return Ok(Vec::new());
    }
    // Older writers (and footprint sections) used the ENCODEDTEXT parameter
    // map; documents proper use a binary [u32 index][u32 len][UTF-16LE]
    // table. A parameter block always starts its payload with '|'.
    if data.len() >= 5 && data[4] == b'|' {
        let mut br = BinaryReader::new(Cursor::new(data))?;
        let map = read_param_map(&mut br)?;
        let mut out = Vec::new();
        for i in 0.. {
            let key = format!("ENCODEDTEXT{i}");
            let Some(encoded) = map.get(&key) else {
                break;
            };
            out.push(decode_wide_string(encoded));
        }
        return Ok(out);
    }
    let mut out = Vec::<String>::new();
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let index = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        let len = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        pos += 8;
        let text = if len <= 2 {
            // Empty strings carry no bytes, not even the NUL.
            String::new()
        } else {
            if pos + len > data.len() {
                break;
            }
            let units: Vec<u16> = data[pos..pos + len - 2]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            pos += len;
            String::from_utf16_lossy(&units)
        };
        if index >= out.len() {
            out.resize(index + 1, String::new());
        }
        out[index] = text;
    }
    Ok(out)
}

fn read_board(cf: &mut CompoundFile, document: &mut Document) -> Result<()> {
    let Some(data) = cf.try_read_stream("Board6/Data")? else {
        return Ok(());
    };
    if data.is_empty() {
        return Ok(());
    }
    // Split on `|` directly instead of `ParameterMap` — the latter dedupes
    // by key and would drop Board6's repeated `RECORD=Board` markers.
    let mut br = BinaryReader::new(Cursor::new(data))?;
    let raw = br.read_c_string_block()?;
    let mut typed: Vec<(String, String)> = Vec::new();
    for part in raw.split('|') {
        if let Some((k, v)) = part.split_once('=') {
            typed.push((k.to_string(), v.to_string()));
        }
    }
    document.board_parameters = Some(typed);
    Ok(())
}

fn read_doc_primitive_streams(
    cf: &mut CompoundFile,
    document: &mut Document,
    wide_strings: &[String],
    _diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    if let Some(data) = cf.try_read_stream("Arcs6/Data")? {
        let mut br = BinaryReader::new(Cursor::new(data))?;
        while br.has_more()? {
            let id = br.read_u8()?;
            if id != ObjectId::Arc as u8 {
                br.skip_block()?;
                continue;
            }
            if let Some(arc) = read_arc(&mut br)? {
                document.arcs.push(arc);
            }
        }
    }

    if let Some(data) = cf.try_read_stream("Pads6/Data")? {
        let mut br = BinaryReader::new(Cursor::new(data))?;
        while br.has_more()? {
            let id = br.read_u8()?;
            if id != ObjectId::Pad as u8 {
                br.skip_block()?;
                continue;
            }
            if let Some(pad) = read_pad(&mut br)? {
                document.pads.push(pad);
            }
        }
    }

    if let Some(data) = cf.try_read_stream("Vias6/Data")? {
        let mut br = BinaryReader::new(Cursor::new(data))?;
        while br.has_more()? {
            let id = br.read_u8()?;
            if id != ObjectId::Via as u8 {
                br.skip_block()?;
                continue;
            }
            if let Some(via) = read_via(&mut br)? {
                document.vias.push(via);
            }
        }
    }

    if let Some(data) = cf.try_read_stream("Tracks6/Data")? {
        let mut br = BinaryReader::new(Cursor::new(data))?;
        while br.has_more()? {
            let id = br.read_u8()?;
            if id != ObjectId::Track as u8 {
                br.skip_block()?;
                continue;
            }
            if let Some(track) = read_track(&mut br)? {
                document.tracks.push(track);
            }
        }
    }

    if let Some(data) = cf.try_read_stream("Texts6/Data")? {
        let mut br = BinaryReader::new(Cursor::new(data))?;
        while br.has_more()? {
            let id = br.read_u8()?;
            if id != ObjectId::Text as u8 {
                br.skip_block()?;
                continue;
            }
            if let Some(text) = read_text(&mut br, wide_strings)? {
                document.texts.push(text);
            }
        }
    }

    if let Some(data) = cf.try_read_stream("Fills6/Data")? {
        let mut br = BinaryReader::new(Cursor::new(data))?;
        while br.has_more()? {
            let id = br.read_u8()?;
            if id != ObjectId::Fill as u8 {
                br.skip_block()?;
                continue;
            }
            if let Some(fill) = read_fill(&mut br)? {
                document.fills.push(fill);
            }
        }
    }

    if let Some(data) = cf.try_read_stream("Regions6/Data")? {
        let mut br = BinaryReader::new(Cursor::new(data))?;
        while br.has_more()? {
            let id = br.read_u8()?;
            if id != ObjectId::Region as u8 {
                br.skip_block()?;
                continue;
            }
            if let Some(region) = read_region(&mut br)? {
                document.regions.push(region);
            }
        }
    }

    if let Some(data) = cf.try_read_stream("ComponentBodies6/Data")? {
        let mut br = BinaryReader::new(Cursor::new(data))?;
        while br.has_more()? {
            let id = br.read_u8()?;
            if id != ObjectId::ComponentBody as u8 {
                br.skip_block()?;
                continue;
            }
            if let Some(body) = read_component_body(&mut br)? {
                document.component_bodies.push(body);
            }
        }
    }

    Ok(())
}

/// Read each parameter-block in a `*6/Data` stream and pass it to a visitor.
///
/// `Rules6` (and a handful of other storages in some Altium versions) use a
/// 6-byte per-record header (`[u16 flags][u32 size]`) instead of the standard
/// 4-byte header. The reader autodetects the per-record header size by
/// looking at the first few bytes; once chosen, it sticks with that layout
/// for the whole stream.
fn for_each_param_record(
    cf: &mut CompoundFile,
    storage: &str,
    mut visit: impl FnMut(&ParameterMap) -> Result<()>,
) -> Result<()> {
    for_each_param_record_with_prefix(cf, storage, |_prefix, params| visit(params))
}

fn for_each_param_record_with_prefix(
    cf: &mut CompoundFile,
    storage: &str,
    visit: impl FnMut(u16, &ParameterMap) -> Result<()>,
) -> Result<()> {
    let path = format!("{storage}/Data");
    let Some(data) = cf.try_read_stream(&path)? else {
        return Ok(());
    };
    if data.is_empty() {
        return Ok(());
    }
    let prefix = detect_record_prefix(&data);
    parse_param_records(&data, prefix, visit)
}

/// Returns the per-record header padding (`0` for normal blocks, `2` when each
/// record carries a 2-byte version word in front of its size header).
fn detect_record_prefix(data: &[u8]) -> usize {
    fn block_size(buf: &[u8], offset: usize) -> Option<u32> {
        if offset + 4 > buf.len() {
            return None;
        }
        let raw = u32::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ]);
        Some(raw & 0x00FF_FFFF)
    }
    // A reasonable parameter block is < 1 MiB; anything larger is almost
    // certainly a misaligned read.
    const MAX_REASONABLE: u32 = 1 << 20;
    if let Some(size) = block_size(data, 0) {
        if size > 0 && size < MAX_REASONABLE && (4 + size as usize) <= data.len() {
            return 0;
        }
    }
    if let Some(size) = block_size(data, 2) {
        if size > 0 && size < MAX_REASONABLE && (6 + size as usize) <= data.len() {
            return 2;
        }
    }
    0
}

fn parse_param_records(
    data: &[u8],
    prefix: usize,
    mut visit: impl FnMut(u16, &ParameterMap) -> Result<()>,
) -> Result<()> {
    let mut pos = 0usize;
    while pos + prefix + 4 <= data.len() {
        let prefix_word: u16 = if prefix == 2 {
            u16::from_le_bytes([data[pos], data[pos + 1]])
        } else {
            0
        };
        pos += prefix;
        let size = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
            & 0x00FF_FFFF;
        pos += 4;
        if size == 0 {
            continue;
        }
        let end = pos + size as usize;
        if end > data.len() {
            return Ok(()); // Truncated — accept what we have.
        }
        let body = &data[pos..end];
        let null = body.iter().position(|&b| b == 0).unwrap_or(body.len());
        let params = ParameterMap::parse_bytes(&body[..null], b'|');
        pos = end;
        if params.is_empty() {
            continue;
        }
        visit(prefix_word, &params)?;
    }
    Ok(())
}

fn read_nets(cf: &mut CompoundFile, document: &mut Document) -> Result<()> {
    let mut nets = Vec::new();
    for_each_param_record(cf, "Nets6", |params| {
        nets.push(super::doc_codec::net_from_params(params));
        Ok(())
    })?;
    document.nets = nets;
    Ok(())
}

fn read_components(cf: &mut CompoundFile, document: &mut Document) -> Result<()> {
    let mut components = Vec::new();
    for_each_param_record(cf, "Components6", |params| {
        components.push(super::doc_codec::component_from_params(params));
        Ok(())
    })?;
    document.components = components;
    Ok(())
}

fn read_polygons(cf: &mut CompoundFile, document: &mut Document) -> Result<()> {
    let mut polygons = Vec::new();
    for_each_param_record(cf, "Polygons6", |params| {
        polygons.push(super::doc_codec::polygon_from_params(params));
        Ok(())
    })?;
    document.polygons = polygons;
    Ok(())
}

fn read_rules(cf: &mut CompoundFile, document: &mut Document) -> Result<()> {
    let mut rules = Vec::new();
    for_each_param_record_with_prefix(cf, "Rules6", |prefix, params| {
        let mut r = super::doc_codec::rule_from_params(params);
        r.rule_type_code = prefix;
        rules.push(r);
        Ok(())
    })?;
    document.rules = rules;
    Ok(())
}

fn read_classes(cf: &mut CompoundFile, document: &mut Document) -> Result<()> {
    let mut classes = Vec::new();
    for_each_param_record(cf, "Classes6", |params| {
        classes.push(super::doc_codec::object_class_from_params(params));
        Ok(())
    })?;
    document.classes = classes;
    Ok(())
}

fn read_differential_pairs(cf: &mut CompoundFile, document: &mut Document) -> Result<()> {
    let mut diff_pairs = Vec::new();
    for_each_param_record(cf, "DifferentialPairs6", |params| {
        diff_pairs.push(super::doc_codec::differential_pair_from_params(params));
        Ok(())
    })?;
    document.differential_pairs = diff_pairs;
    Ok(())
}

fn read_rooms(cf: &mut CompoundFile, document: &mut Document) -> Result<()> {
    let mut rooms = Vec::new();
    for_each_param_record(cf, "Rooms6", |params| {
        rooms.push(super::doc_codec::room_from_params(params));
        Ok(())
    })?;
    document.rooms = rooms;
    Ok(())
}

fn read_embedded_boards(cf: &mut CompoundFile, document: &mut Document) -> Result<()> {
    let mut boards = Vec::new();
    for_each_param_record(cf, "EmbeddedBoards6", |params| {
        boards.push(super::doc_codec::embedded_board_from_params(params));
        Ok(())
    })?;
    document.embedded_boards = boards;
    Ok(())
}

/// Map binary `net_index` values back to net names from `Nets6`. Indices are
/// 1-based on disk (`0xFFFF` / `0` mean "no net"); the names list is 0-based.
fn resolve_net_names(document: &mut Document) {
    if document.nets.is_empty() {
        return;
    }
    let lookup = |idx: u16| -> Option<String> {
        if idx == 0 {
            return None;
        }
        document
            .nets
            .get((idx - 1) as usize)
            .map(|n| n.name.clone())
    };
    for arc in &mut document.arcs {
        if arc.net.is_none() {
            arc.net = lookup(arc.net_index);
        }
    }
    for pad in &mut document.pads {
        if pad.net.is_none() {
            pad.net = lookup(pad.net_index);
        }
    }
    for track in &mut document.tracks {
        if track.net.is_none() {
            track.net = lookup(track.net_index);
        }
    }
    for via in &mut document.vias {
        if via.net.is_none() {
            via.net = lookup(via.net_index);
        }
    }
    for fill in &mut document.fills {
        if fill.net.is_none() {
            fill.net = lookup(fill.net_index);
        }
    }
    for region in &mut document.regions {
        if region.net.is_none() {
            region.net = lookup(region.net_index);
        }
    }
}

/// Resolve each primitive's `component_index` (from the binary common
/// prefix) to its parent component, populating the component's owned
/// primitive lists. The document-level lists remain the source of truth on
/// write; the per-component lists are convenience clones for bounds /
/// footprint-style access.
fn assign_primitives_to_components(document: &mut Document) {
    if document.components.is_empty() {
        return;
    }
    let count = document.components.len() as i32;
    macro_rules! assign {
        ($list:ident) => {
            for prim in &document.$list {
                let idx = prim.component_index;
                if (0..count).contains(&idx) {
                    document.components[idx as usize].$list.push(prim.clone());
                }
            }
        };
    }
    assign!(pads);
    assign!(tracks);
    assign!(arcs);
    assign!(vias);
    assign!(texts);
    assign!(fills);
    assign!(regions);
    assign!(component_bodies);
}

fn read_doc_additional_streams(
    cf: &mut CompoundFile,
    document: &mut Document,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let mut additional: IndexMap<String, Vec<u8>> = IndexMap::new();
    let known: Vec<String> = PCB_DOC_KNOWN_STORAGES
        .iter()
        .map(|s| s.to_ascii_uppercase())
        .collect();

    let entries = cf.list_children("/")?;
    for entry in entries {
        if known.contains(&entry.name.to_ascii_uppercase()) {
            continue;
        }
        if entry.is_storage {
            let inner = cf.list_children(&entry.name)?;
            for sub in inner {
                if sub.is_stream {
                    match cf.read_stream(format!("{}/{}", entry.name, sub.name)) {
                        Ok(data) => {
                            additional.insert(format!("{}/{}", entry.name, sub.name), data);
                        }
                        Err(e) => {
                            diagnostics.push(Diagnostic::warning(format!(
                                "failed to read {}/{}: {e}",
                                entry.name, sub.name
                            )));
                        }
                    }
                }
            }
        } else if entry.is_stream {
            match cf.read_stream(&entry.name) {
                Ok(data) => {
                    additional.insert(entry.name, data);
                }
                Err(e) => {
                    diagnostics.push(Diagnostic::warning(format!(
                        "failed to read {}: {e}",
                        entry.name
                    )));
                }
            }
        }
    }

    document.additional_streams = additional.into_iter().collect();
    Ok(())
}
