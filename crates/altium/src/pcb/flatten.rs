//! Flatten an embedded-board hierarchy into a single [`Document`].
//!
//! Given a parent `.PcbDoc` that references one or more sub-boards via
//! [`EmbeddedBoard`], the flattener pulls each sub-document's primitives
//! into the parent — translated, rotated, and mirrored according to the
//! reference — producing a single self-contained document with no embedded
//! references left.
//!
//! ## Placement semantics
//!
//! The record's `X`/`Y` is where the sub-board's own board origin lands in
//! the parent's absolute frame, with `rotation`/`mirror` applied about
//! that anchor:
//!
//! ```text
//! p_parent = R(rotation) · M(mirror) · (p_child − child_origin) + (X, Y)
//! ```
//!
//! `X1`..`Y2` is only the cached bounding box of the placed instance.
//!
//! This is the data-side analogue of the recursive renderer in
//! [`crate::render::pcb`]: the renderer draws the same tree to pixels; this
//! module produces the merged data structure for non-rendering use cases
//! (DRC, BOM extraction, geometry queries, exporters).

use super::component::Component;
use super::document::Document;
use super::embedded::{BoardLoader, EmbeddedBoard};
use super::polygon::{Polygon, PolygonVertex};
use super::primitives::{Arc, ComponentBody, Fill, Pad, Region, Text, Track, Via};
use crate::coord::{Coord, CoordPoint};
use crate::diagnostic::Diagnostic;

/// Same recursion ceiling the renderer uses; protects against cycles.
const MAX_FLATTEN_DEPTH: u8 = 8;

/// 2×3 affine world transform: linear part `[a b; c d]` plus translation `[tx; ty]`.
///
/// We use a general affine rather than "rotate + mirror + translate" because
/// nested embedded boards compose transforms multiplicatively — a rotation
/// nested under a mirrored parent isn't expressible in the simpler form.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WorldTransform {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

impl WorldTransform {
    pub(crate) const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// Transform for one array instance:
    /// `p ↦ R(rotation)·M(mirror)·(p − child_origin) + translation`.
    fn from_embedded_instance(
        board: &EmbeddedBoard,
        translation: CoordPoint,
        child_origin: CoordPoint,
    ) -> Self {
        let theta = board.rotation.to_radians();
        let (sin_t, cos_t) = theta.sin_cos();
        let mx = if board.mirror_flag { -1.0 } else { 1.0 };
        // Combined linear part: rotation × mirror_x. Mirror is applied first
        // (closer to the source primitives), rotation second.
        let (a, b, c, d) = (cos_t * mx, -sin_t, sin_t * mx, cos_t);
        let ox = child_origin.x.to_raw() as f64;
        let oy = child_origin.y.to_raw() as f64;
        Self {
            a,
            b,
            c,
            d,
            tx: translation.x.to_raw() as f64 - (a * ox + b * oy),
            ty: translation.y.to_raw() as f64 - (c * ox + d * oy),
        }
    }

    /// `self ∘ inner` — applies `inner` first, then `self`.
    fn compose(&self, inner: &Self) -> Self {
        Self {
            a: self.a * inner.a + self.b * inner.c,
            b: self.a * inner.b + self.b * inner.d,
            c: self.c * inner.a + self.d * inner.c,
            d: self.c * inner.b + self.d * inner.d,
            tx: self.a * inner.tx + self.b * inner.ty + self.tx,
            ty: self.c * inner.tx + self.d * inner.ty + self.ty,
        }
    }

    fn apply_point(&self, p: CoordPoint) -> CoordPoint {
        let x = p.x.to_raw() as f64;
        let y = p.y.to_raw() as f64;
        CoordPoint::new(
            Coord::from_raw((self.a * x + self.b * y + self.tx).round() as i32),
            Coord::from_raw((self.c * x + self.d * y + self.ty).round() as i32),
        )
    }

    /// The angle (degrees, `[0, 360)`) a direction at `deg` maps to under
    /// the linear part. Used for arc sweep endpoints.
    fn apply_angle(&self, deg: f64) -> f64 {
        let r = deg.to_radians();
        let (s, c) = r.sin_cos();
        let x = self.a * c + self.b * s;
        let y = self.c * c + self.d * s;
        y.atan2(x).to_degrees().rem_euclid(360.0)
    }

    /// Update a *stored* rotation field: the `r` in the decomposition
    /// `linear·R(old) = R(r)·M^flip`. The caller toggles the primitive's
    /// mirror flag when flipped.
    fn stored_rotation(&self, old: f64) -> f64 {
        let new = if self.is_flipped() {
            // linear = R(r)·M ⇒ a = −cos r, c = −sin r.
            (-self.c).atan2(-self.a).to_degrees() - old
        } else {
            self.c.atan2(self.a).to_degrees() + old
        };
        // Altium stores rotations in [0, 360).
        new.rem_euclid(360.0)
    }

    /// `true` when the linear part has negative determinant — i.e. the
    /// transform involves an odd number of reflections.
    fn is_flipped(&self) -> bool {
        (self.a * self.d - self.b * self.c) < 0.0
    }
}

// ─── Per-primitive transform appliers ──────────────────────────────────────

fn apply_to_pad(t: &WorldTransform, pad: &mut Pad) {
    pad.location = t.apply_point(pad.location);
    pad.rotation = t.stored_rotation(pad.rotation);
    // Pad sizes/shape arrays are pad-local dimensions, not world coords;
    // they don't need transforming.
}

fn apply_to_track(t: &WorldTransform, track: &mut Track) {
    track.start = t.apply_point(track.start);
    track.end = t.apply_point(track.end);
}

/// Sweep endpoints under the transform. A reflection reverses the CCW
/// sweep, so the endpoints swap; full circles must survive normalization.
fn apply_to_sweep(t: &WorldTransform, start_angle: &mut f64, end_angle: &mut f64) {
    let full_circle = (*end_angle - *start_angle).abs() >= 359.995;
    if t.is_flipped() {
        let (s, e) = (*start_angle, *end_angle);
        *start_angle = t.apply_angle(e);
        *end_angle = t.apply_angle(s);
    } else {
        *start_angle = t.apply_angle(*start_angle);
        *end_angle = t.apply_angle(*end_angle);
    }
    if full_circle {
        *end_angle = *start_angle + 360.0;
    }
}

fn apply_to_arc(t: &WorldTransform, arc: &mut Arc) {
    arc.center = t.apply_point(arc.center);
    apply_to_sweep(t, &mut arc.start_angle, &mut arc.end_angle);
}

fn apply_to_via(t: &WorldTransform, via: &mut Via) {
    via.location = t.apply_point(via.location);
}

fn apply_to_text(t: &WorldTransform, text: &mut Text) {
    text.location = t.apply_point(text.location);
    text.rotation = t.stored_rotation(text.rotation);
    if t.is_flipped() {
        text.is_mirrored = !text.is_mirrored;
    }
}

fn apply_to_fill(t: &WorldTransform, fill: &mut Fill) {
    fill.corner1 = t.apply_point(fill.corner1);
    fill.corner2 = t.apply_point(fill.corner2);
    fill.rotation = t.stored_rotation(fill.rotation);
}

fn apply_to_region(t: &WorldTransform, region: &mut Region) {
    for p in &mut region.outline {
        *p = t.apply_point(*p);
    }
    for hole in &mut region.holes {
        for p in hole {
            *p = t.apply_point(*p);
        }
    }
}

fn apply_to_polygon(t: &WorldTransform, poly: &mut Polygon) {
    poly.origin = t.apply_point(poly.origin);
    for v in &mut poly.vertices {
        apply_to_polygon_vertex(t, v);
    }
}

fn apply_to_polygon_vertex(t: &WorldTransform, v: &mut PolygonVertex) {
    v.point = t.apply_point(v.point);
    if v.kind != 0 {
        v.arc_center = t.apply_point(v.arc_center);
        apply_to_sweep(t, &mut v.start_angle, &mut v.end_angle);
    }
}

fn apply_to_body(t: &WorldTransform, body: &mut ComponentBody) {
    for p in &mut body.outline {
        *p = t.apply_point(*p);
    }
    body.model_2d_location = t.apply_point(body.model_2d_location);
    body.model_2d_rotation = t.stored_rotation(body.model_2d_rotation);
}

fn apply_to_component(t: &WorldTransform, comp: &mut Component) {
    let new_xy = t.apply_point(CoordPoint::new(comp.x, comp.y));
    comp.x = new_xy.x;
    comp.y = new_xy.y;
    comp.rotation = t.stored_rotation(comp.rotation);
    if t.is_flipped() {
        comp.flipped_on_layer = !comp.flipped_on_layer;
    }
    // The component's owned primitive lists are clones of document-level
    // primitives in the same absolute frame — keep them in step.
    for p in &mut comp.pads {
        apply_to_pad(t, p);
    }
    for p in &mut comp.tracks {
        apply_to_track(t, p);
    }
    for p in &mut comp.arcs {
        apply_to_arc(t, p);
    }
    for p in &mut comp.vias {
        apply_to_via(t, p);
    }
    for p in &mut comp.texts {
        apply_to_text(t, p);
    }
    for p in &mut comp.fills {
        apply_to_fill(t, p);
    }
    for p in &mut comp.regions {
        apply_to_region(t, p);
    }
    for p in &mut comp.component_bodies {
        apply_to_body(t, p);
    }
}

/// Apply `t` to an embedded board record we keep in the output (depth
/// limit, unresolved): placement point, cached bounding box, and the
/// rotation/mirror pair.
fn apply_to_embedded(t: &WorldTransform, b: &mut EmbeddedBoard) {
    let xy = t.apply_point(CoordPoint::new(b.x_location, b.y_location));
    b.x_location = xy.x;
    b.y_location = xy.y;
    let p1 = t.apply_point(CoordPoint::new(b.x1_location, b.y1_location));
    let p2 = t.apply_point(CoordPoint::new(b.x2_location, b.y2_location));
    b.x1_location = p1.x.min(p2.x);
    b.y1_location = p1.y.min(p2.y);
    b.x2_location = p1.x.max(p2.x);
    b.y2_location = p1.y.max(p2.y);
    b.rotation = t.stored_rotation(b.rotation);
    if t.is_flipped() {
        b.mirror_flag = !b.mirror_flag;
    }
    // col/row_spacing is a magnitude along the instance axes — preserved
    // under rotation/mirror/translation (we never scale).
}

// ─── Recursive walker ──────────────────────────────────────────────────────

pub(crate) fn flatten_into(
    src: &Document,
    transform: &WorldTransform,
    loader: &dyn BoardLoader,
    depth: u8,
    out: &mut Document,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Where this instance's components will start in the merged list —
    // every merged primitive's `component_index` shifts by this much.
    let comp_base = out.components.len() as i32;
    let remap_component = |ci: i32| if ci >= 0 { ci + comp_base } else { ci };

    // Merge nets by name first, recording where each of `src`'s nets lands
    // so primitive `net_index` values (1-based, 0 = none) can be remapped.
    let mut net_map: Vec<u16> = Vec::with_capacity(src.nets.len() + 1);
    net_map.push(0);
    for net in &src.nets {
        let pos = match out.nets.iter().position(|n| n.name == net.name) {
            Some(pos) => pos,
            None => {
                out.nets.push(net.clone());
                out.nets.len() - 1
            }
        };
        net_map.push((pos + 1) as u16);
    }
    // Out-of-range source indices (corrupt or subnet-tagged) map to "no
    // net" rather than aliasing an arbitrary merged net.
    let remap_net = |ni: u16| net_map.get(ni as usize).copied().unwrap_or(0);

    // Free primitives (document-level lists also carry the component-owned
    // ones; ownership is preserved through the remapped index).
    for x in &src.pads {
        let mut p = x.clone();
        apply_to_pad(transform, &mut p);
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
        out.pads.push(p);
    }
    for x in &src.tracks {
        let mut p = x.clone();
        apply_to_track(transform, &mut p);
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
        out.tracks.push(p);
    }
    for x in &src.arcs {
        let mut p = x.clone();
        apply_to_arc(transform, &mut p);
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
        out.arcs.push(p);
    }
    for x in &src.vias {
        let mut p = x.clone();
        apply_to_via(transform, &mut p);
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
        out.vias.push(p);
    }
    for x in &src.texts {
        let mut p = x.clone();
        apply_to_text(transform, &mut p);
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
        out.texts.push(p);
    }
    for x in &src.fills {
        let mut p = x.clone();
        apply_to_fill(transform, &mut p);
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
        out.fills.push(p);
    }
    for x in &src.regions {
        let mut p = x.clone();
        apply_to_region(transform, &mut p);
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
        out.regions.push(p);
    }
    for x in &src.component_bodies {
        let mut p = x.clone();
        apply_to_body(transform, &mut p);
        p.component_index = remap_component(p.component_index);
        out.component_bodies.push(p);
    }
    for x in &src.polygons {
        let mut p = x.clone();
        apply_to_polygon(transform, &mut p);
        out.polygons.push(p);
    }

    // Components — preserved as components (not torn apart). This keeps
    // designators/refs intact for downstream consumers. Their owned
    // primitive clones get the same index/net remap as the document lists.
    for comp in &src.components {
        let mut c = comp.clone();
        apply_to_component(transform, &mut c);
        remap_component_children(&mut c, &remap_component, &remap_net);
        out.components.push(c);
    }

    // Rules, classes etc. are name-based metadata; appended as-is so
    // cross-board rules survive.
    out.rules.extend(src.rules.iter().cloned());
    out.classes.extend(src.classes.iter().cloned());
    out.differential_pairs
        .extend(src.differential_pairs.iter().cloned());
    out.rooms.extend(src.rooms.iter().cloned());

    // Embedded board references: recurse if resolvable, else preserve.
    for board in &src.embedded_boards {
        if depth >= MAX_FLATTEN_DEPTH {
            let mut b = board.clone();
            apply_to_embedded(transform, &mut b);
            out.embedded_boards.push(b);
            diagnostics.push(Diagnostic::warning(format!(
                "embedded board {:?} kept unflattened: max recursion depth ({}) reached",
                board.document_path.as_deref().unwrap_or("<unset>"),
                MAX_FLATTEN_DEPTH
            )));
            continue;
        }

        match board.resolve_with(loader) {
            Ok(sub) => {
                if board.origin_mode != 0 {
                    diagnostics.push(Diagnostic::warning(format!(
                        "embedded board {:?}: ORIGINMODE={} (\"link location to \
                         embedded board origin\" variant) has not been verified \
                         against Altium; placement may be off",
                        board.document_path.as_deref().unwrap_or("<unset>"),
                        board.origin_mode
                    )));
                }
                if board.mirror_flag {
                    diagnostics.push(Diagnostic::warning(format!(
                        "embedded board {:?}: mirrored placement is geometric \
                         only — primitives are not moved to the opposite \
                         layer side",
                        board.document_path.as_deref().unwrap_or("<unset>")
                    )));
                }
                let child_origin = sub.board_origin();
                let cols = board.col_count.max(1);
                let rows = board.row_count.max(1);
                let mut merged_streams = false;
                for col in 0..cols {
                    for row in 0..rows {
                        let translation = CoordPoint::new(
                            board.x_location + board.col_spacing * col,
                            board.y_location + board.row_spacing * row,
                        );
                        let local =
                            WorldTransform::from_embedded_instance(board, translation, child_origin);
                        let composed = transform.compose(&local);
                        let bases = PrimBases::capture(out);
                        flatten_into(&sub, &composed, loader, depth + 1, out, diagnostics);
                        // Union / per-primitive info streams merge once, keyed
                        // to the first instance's primitive indices.
                        if !merged_streams {
                            merge_side_streams(&sub, out, &bases, diagnostics);
                            merged_streams = true;
                        }
                    }
                }
                if cols * rows > 1
                    && sub
                        .additional_streams
                        .get("UniqueIDPrimitiveInformation/Data")
                        .is_some_and(|d| !d.is_empty())
                {
                    diagnostics.push(Diagnostic::warning(format!(
                        "embedded board {:?}: per-primitive id/union records were \
                         merged for the first array instance only",
                        board.document_path.as_deref().unwrap_or("<unset>")
                    )));
                }
            }
            Err(e) => {
                let mut b = board.clone();
                apply_to_embedded(transform, &mut b);
                out.embedded_boards.push(b);
                diagnostics.push(Diagnostic::warning(format!(
                    "embedded board {:?} not flattened: {}",
                    board.document_path.as_deref().unwrap_or("<unset>"),
                    e
                )));
            }
        }
    }
}

/// Remap `component_index` / `net_index` on a component's owned primitive
/// clones to the merged document's tables.
fn remap_component_children(
    comp: &mut Component,
    remap_component: &dyn Fn(i32) -> i32,
    remap_net: &dyn Fn(u16) -> u16,
) {
    for p in &mut comp.pads {
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
    }
    for p in &mut comp.tracks {
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
    }
    for p in &mut comp.arcs {
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
    }
    for p in &mut comp.vias {
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
    }
    for p in &mut comp.texts {
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
    }
    for p in &mut comp.fills {
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
    }
    for p in &mut comp.regions {
        p.component_index = remap_component(p.component_index);
        p.net_index = remap_net(p.net_index);
    }
    for p in &mut comp.component_bodies {
        p.component_index = remap_component(p.component_index);
    }
}


// ─── Side-stream merging (unions, per-primitive info) ──────────────────────
//
// Several storages outside the primitive streams reference primitives by
// index or carry union (via stitching / grouping) definitions. The parent's
// copies are cloned wholesale; a resolved sub-board's copies are merged
// here with indices shifted by where its primitives landed.

/// Primitive counts in `out` before one sub-board instance is appended.
struct PrimBases {
    pads: usize,
    vias: usize,
    tracks: usize,
    arcs: usize,
    texts: usize,
    fills: usize,
    regions: usize,
    bodies: usize,
    polygons: usize,
    components: usize,
}

impl PrimBases {
    fn capture(out: &Document) -> Self {
        Self {
            pads: out.pads.len(),
            vias: out.vias.len(),
            tracks: out.tracks.len(),
            arcs: out.arcs.len(),
            texts: out.texts.len(),
            fills: out.fills.len(),
            regions: out.regions.len(),
            bodies: out.component_bodies.len(),
            polygons: out.polygons.len(),
            components: out.components.len(),
        }
    }

    fn for_object(&self, name: &str) -> Option<usize> {
        Some(match name {
            "Pad" => self.pads,
            "Via" => self.vias,
            "Track" => self.tracks,
            "Arc" => self.arcs,
            "Text" => self.texts,
            "Fill" => self.fills,
            "Region" => self.regions,
            "ComponentBody" => self.bodies,
            "Polygon" => self.polygons,
            "Component" => self.components,
            _ => return None,
        })
    }
}

fn stream_u32(data: &[u8], off: usize) -> Option<usize> {
    data.get(off..off + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
}

/// Iterate `[u32 len][body]` records of a param-record stream.
fn param_records(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while let Some(len) = stream_u32(data, off) {
        let len = len & 0x00FF_FFFF;
        let Some(body) = data.get(off + 4..off + 4 + len) else {
            break;
        };
        out.push(body);
        off += 4 + len;
    }
    out
}

fn push_record(out: &mut Vec<u8>, body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
}

/// Find `key` (e.g. `b"|PRIMITIVEOBJECTID="`) and return its value span.
fn find_value(body: &[u8], key: &[u8]) -> Option<(usize, usize)> {
    let start = body
        .windows(key.len())
        .position(|w| w.eq_ignore_ascii_case(key))?
        + key.len();
    let mut end = start;
    while end < body.len() && body[end] != b'|' && body[end] != 0 {
        end += 1;
    }
    Some((start, end))
}

/// Rewrite index references in one record body: `PRIMITIVEINDEX=<n>`
/// (typed by `PRIMITIVEOBJECTID=<Type>`) and any `ID=<Type>#<n>`.
fn remap_record_indices(body: &[u8], bases: &PrimBases) -> Vec<u8> {
    let mut out = body.to_vec();
    if let (Some((os, oe)), Some((is_, ie))) = (
        find_value(&out, b"|PRIMITIVEOBJECTID="),
        find_value(&out, b"|PRIMITIVEINDEX="),
    ) {
        let obj = String::from_utf8_lossy(&out[os..oe]).to_string();
        if let Some(base) = bases.for_object(&obj) {
            if let Ok(n) = String::from_utf8_lossy(&out[is_..ie]).parse::<usize>() {
                out.splice(is_..ie, (n + base).to_string().into_bytes());
            }
        }
    }
    // `ID=<Type>#<n>` occurrences.
    let mut search = 0usize;
    loop {
        let Some(rel) = out[search..]
            .windows(4)
            .position(|w| w.eq_ignore_ascii_case(b"|ID="))
        else {
            break;
        };
        let vstart = search + rel + 4;
        let mut vend = vstart;
        while vend < out.len() && out[vend] != b'|' && out[vend] != 0 {
            vend += 1;
        }
        let value = String::from_utf8_lossy(&out[vstart..vend]).to_string();
        if let Some((ty, num)) = value.split_once('#') {
            if let (Some(base), Ok(n)) = (bases.for_object(ty), num.parse::<usize>()) {
                let replacement = format!("{ty}#{}", n + base).into_bytes();
                let new_end = vstart + replacement.len();
                out.splice(vstart..vend, replacement);
                search = new_end;
                continue;
            }
        }
        search = vend;
    }
    out
}

/// `[count][(id u32, byte_len u32, utf16 name) × count]`.
fn parse_union_names(data: &[u8]) -> Option<Vec<(u32, Vec<u8>)>> {
    let count = stream_u32(data, 0)?;
    let mut off = 4usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let id = stream_u32(data, off)? as u32;
        let len = stream_u32(data, off + 4)?;
        let name = data.get(off + 8..off + 8 + len)?.to_vec();
        entries.push((id, name));
        off += 8 + len;
    }
    Some(entries)
}

fn serialize_union_names(entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (id, name) in entries {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name);
    }
    out
}

fn stream_header_count(doc: &Document, storage: &str) -> u32 {
    doc.additional_streams
        .get(&format!("{storage}/Header"))
        .and_then(|d| d.get(..4))
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .unwrap_or(0)
}

fn set_stream(out: &mut Document, storage: &str, data: Vec<u8>, header: u32) {
    out.additional_streams
        .insert(format!("{storage}/Data"), data);
    out.additional_streams
        .insert(format!("{storage}/Header"), header.to_le_bytes().to_vec());
}

fn merge_side_streams(
    sub: &Document,
    out: &mut Document,
    bases: &PrimBases,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let sub_data = |storage: &str| {
        sub.additional_streams
            .get(&format!("{storage}/Data"))
            .filter(|d| !d.is_empty())
    };

    // UnionNames: concatenate entries; ids should already be distinct.
    if let Some(sub_un) = sub_data("UnionNames") {
        let mut entries = out
            .additional_streams
            .get("UnionNames/Data")
            .and_then(|d| parse_union_names(d))
            .unwrap_or_default();
        if let Some(sub_entries) = parse_union_names(sub_un) {
            for (id, name) in sub_entries {
                if entries.iter().any(|(eid, en)| *eid == id && *en == name) {
                    continue;
                }
                if entries.iter().any(|(eid, _)| *eid == id) {
                    diagnostics.push(Diagnostic::warning(format!(
                        "union id {id} exists in both parent and sub-board; \
                         group membership may be merged in the output"
                    )));
                }
                entries.push((id, name));
            }
            set_stream(out, "UnionNames", serialize_union_names(&entries), 1);
        }
    }

    // SmartUnions: append the sub-board's definitions, skipping records
    // that already exist byte-for-byte (a panel typically inherits the
    // same project-level group definitions as its sub-board).
    if let Some(sub_su) = sub_data("SmartUnions") {
        let mut data = out
            .additional_streams
            .get("SmartUnions/Data")
            .cloned()
            .unwrap_or_default();
        let existing: Vec<Vec<u8>> = param_records(&data).iter().map(|r| r.to_vec()).collect();
        let mut count = existing.len() as u32;
        for rec in param_records(sub_su) {
            if existing.iter().any(|e| e == rec) {
                continue;
            }
            if let Some((s, e)) = find_value(rec, b"|UNIONINDEX=") {
                let idx = &rec[s..e];
                if existing
                    .iter()
                    .any(|ex| find_value(ex, b"|UNIONINDEX=").is_some_and(|(a, b)| &ex[a..b] == idx))
                {
                    diagnostics.push(Diagnostic::warning(format!(
                        "union index {} defined differently in parent and \
                         sub-board; both definitions kept",
                        String::from_utf8_lossy(idx)
                    )));
                }
            }
            push_record(&mut data, rec);
            count += 1;
        }
        set_stream(out, "SmartUnions", data, count);
    }

    // Index-keyed per-primitive streams: append with indices shifted.
    for storage in ["UniqueIDPrimitiveInformation", "PrimitiveParameters"] {
        if let Some(sub_stream) = sub_data(storage) {
            let mut data = out
                .additional_streams
                .get(&format!("{storage}/Data"))
                .cloned()
                .unwrap_or_default();
            for rec in param_records(sub_stream) {
                push_record(&mut data, &remap_record_indices(rec, bases));
            }
            // Each side's header counts its own semantic units (top-level
            // entries for PrimitiveParameters, records for
            // UniqueIDPrimitiveInformation) — the merged header is the sum.
            let header =
                stream_header_count(out, storage) + stream_header_count(sub, storage);
            set_stream(out, storage, data, header);
        }
    }

    // Embedded 3D models: append the sub-board's `Models/Data` records and
    // renumber its `Models/<i>` streams after the parent's, skipping model
    // GUIDs the parent already embeds (bodies reference models by GUID).
    if let Some(sub_md) = sub_data("Models") {
        let model_record_id = |rec: &[u8]| -> Option<Vec<u8>> {
            let mut probe = Vec::with_capacity(rec.len() + 1);
            probe.push(b'|');
            probe.extend_from_slice(rec);
            find_value(&probe, b"|ID=").map(|(s, e)| probe[s..e].to_ascii_uppercase())
        };
        let mut data = out
            .additional_streams
            .get("Models/Data")
            .cloned()
            .unwrap_or_default();
        let mut existing_ids: Vec<Vec<u8>> = param_records(&data)
            .iter()
            .filter_map(|r| model_record_id(r))
            .collect();
        let mut idx = 0usize;
        while out.additional_streams.contains_key(&format!("Models/{idx}")) {
            idx += 1;
        }
        for (i, rec) in param_records(sub_md).iter().enumerate() {
            let Some(stream) = sub.additional_streams.get(&format!("Models/{i}")) else {
                continue;
            };
            let id = model_record_id(rec);
            if let Some(id) = &id {
                if existing_ids.contains(id) {
                    continue;
                }
                existing_ids.push(id.clone());
            }
            push_record(&mut data, rec);
            out.additional_streams
                .insert(format!("Models/{idx}"), stream.clone());
            idx += 1;
        }
        set_stream(out, "Models", data, idx as u32);
    }

    // PrimitiveGuids is positional and can't be merged; Altium will
    // regenerate GUIDs for the sub-board's primitives.
    if sub_data("PrimitiveGuids").is_some() {
        diagnostics.push(Diagnostic::warning(
            "sub-board PrimitiveGuids not merged (positional format); Altium \
             will regenerate primitive GUIDs"
                .to_string(),
        ));
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default, clippy::identity_op)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64) -> CoordPoint {
        CoordPoint::new(Coord::from_mils(x), Coord::from_mils(y))
    }

    #[test]
    fn identity_is_no_op() {
        let p = pt(10.0, 20.0);
        let q = WorldTransform::IDENTITY.apply_point(p);
        assert_eq!(p, q);
    }

    #[test]
    fn translate_only_shifts_points() {
        let t = WorldTransform {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 100.0 * 10_000.0,
            ty: 50.0 * 10_000.0,
        };
        let q = t.apply_point(pt(5.0, 5.0));
        assert_eq!(q, pt(105.0, 55.0));
    }

    #[test]
    fn zero_placement_and_zero_origin_is_identity() {
        // The real-world default: X=Y=0 with a child whose board origin is
        // (0,0) reproduces the child at identical absolute coordinates.
        let b = EmbeddedBoard::default();
        let t = WorldTransform::from_embedded_instance(&b, pt(0.0, 0.0), pt(0.0, 0.0));
        let p = pt(1234.5, 678.9);
        assert_eq!(t.apply_point(p), p);
    }

    #[test]
    fn child_origin_is_subtracted() {
        // A child whose board origin sits at (100, 200) placed at X=Y=0:
        // the child's origin point must land at (0, 0).
        let b = EmbeddedBoard::default();
        let t = WorldTransform::from_embedded_instance(&b, pt(0.0, 0.0), pt(100.0, 200.0));
        assert_eq!(t.apply_point(pt(100.0, 200.0)), pt(0.0, 0.0));
        assert_eq!(t.apply_point(pt(150.0, 200.0)), pt(50.0, 0.0));
    }

    #[test]
    fn rotation_90_takes_x_to_y() {
        let mut b = EmbeddedBoard::default();
        b.rotation = 90.0;
        let t = WorldTransform::from_embedded_instance(&b, pt(0.0, 0.0), pt(0.0, 0.0));
        // (10, 0) -> (0, 10)
        let q = t.apply_point(pt(10.0, 0.0));
        let dx = (q.x.to_raw() - 0).abs();
        let dy = (q.y.to_raw() - Coord::from_mils(10.0).to_raw()).abs();
        assert!(dx < 5 && dy < 5, "got {q:?}");
    }

    #[test]
    fn rotation_pivots_about_child_origin() {
        // Child origin (100, 0), rotation 90°, placed at (0, 0): a point
        // 10 mil right of the origin ends up 10 mil above the placement.
        let mut b = EmbeddedBoard::default();
        b.rotation = 90.0;
        let t = WorldTransform::from_embedded_instance(&b, pt(0.0, 0.0), pt(100.0, 0.0));
        let q = t.apply_point(pt(110.0, 0.0));
        let dx = (q.x.to_raw() - 0).abs();
        let dy = (q.y.to_raw() - Coord::from_mils(10.0).to_raw()).abs();
        assert!(dx < 5 && dy < 5, "got {q:?}");
    }

    #[test]
    fn mirror_flips_x() {
        let mut b = EmbeddedBoard::default();
        b.mirror_flag = true;
        let t = WorldTransform::from_embedded_instance(&b, pt(0.0, 0.0), pt(0.0, 0.0));
        let q = t.apply_point(pt(10.0, 5.0));
        assert_eq!(q, pt(-10.0, 5.0));
        assert!(t.is_flipped(), "mirror should produce negative determinant");
    }

    #[test]
    fn compose_translates_then_rotates() {
        // Inner translates by (10, 0); outer rotates 90°.
        let inner = WorldTransform {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 10.0 * 10_000.0,
            ty: 0.0,
        };
        let mut outer_board = EmbeddedBoard::default();
        outer_board.rotation = 90.0;
        let outer =
            WorldTransform::from_embedded_instance(&outer_board, pt(0.0, 0.0), pt(0.0, 0.0));
        let composed = outer.compose(&inner);
        // Inner takes (0,0) → (10,0). Outer rotates that 90° → (0,10).
        let q = composed.apply_point(pt(0.0, 0.0));
        let dx = (q.x.to_raw() - 0).abs();
        let dy = (q.y.to_raw() - Coord::from_mils(10.0).to_raw()).abs();
        assert!(dx < 5 && dy < 5, "expected ~(0, 10), got {q:?}");
    }

    #[test]
    fn apply_angle_matches_rotation() {
        let mut b = EmbeddedBoard::default();
        b.rotation = 90.0;
        let t = WorldTransform::from_embedded_instance(&b, pt(0.0, 0.0), pt(0.0, 0.0));
        assert!((t.apply_angle(30.0) - 120.0).abs() < 1e-9);
        assert!((t.stored_rotation(30.0) - 120.0).abs() < 1e-9);
    }

    #[test]
    fn flipped_sweep_mirrors_and_swaps() {
        // Pure x-mirror maps an arc from 10°..80° to 100°..170°
        // (α ↦ 180 − α with endpoints swapped to keep the sweep CCW).
        let mut b = EmbeddedBoard::default();
        b.mirror_flag = true;
        let t = WorldTransform::from_embedded_instance(&b, pt(0.0, 0.0), pt(0.0, 0.0));
        let (mut s, mut e) = (10.0, 80.0);
        apply_to_sweep(&t, &mut s, &mut e);
        assert!((s - 100.0).abs() < 1e-9, "start {s}");
        assert!((e - 170.0).abs() < 1e-9, "end {e}");
    }

    #[test]
    fn full_circle_sweep_survives_any_transform() {
        for (rot, mirror) in [(0.0, false), (90.0, false), (0.0, true), (45.0, true)] {
            let mut b = EmbeddedBoard::default();
            b.rotation = rot;
            b.mirror_flag = mirror;
            let t = WorldTransform::from_embedded_instance(&b, pt(0.0, 0.0), pt(0.0, 0.0));
            let (mut s, mut e) = (0.0, 360.0);
            apply_to_sweep(&t, &mut s, &mut e);
            assert!(
                (e - s - 360.0).abs() < 1e-9,
                "rot={rot} mirror={mirror}: {s}..{e} is no longer a full circle"
            );
        }
    }

    #[test]
    fn stored_rotation_under_flip() {
        // Mirror then rotate 90°: an object stored at 30° decomposes to
        // R(90)·M·R(30) = R(60)·M — the stored angle becomes 60°.
        let mut b = EmbeddedBoard::default();
        b.mirror_flag = true;
        b.rotation = 90.0;
        let t = WorldTransform::from_embedded_instance(&b, pt(0.0, 0.0), pt(0.0, 0.0));
        assert!((t.stored_rotation(30.0) - 60.0).abs() < 1e-9);
    }
}
