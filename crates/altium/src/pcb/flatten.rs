//! Flatten an embedded-board hierarchy into a single [`Document`].
//!
//! Given a parent `.PcbDoc` that references one or more sub-boards via
//! [`EmbeddedBoard`], the flattener pulls each sub-document's primitives
//! into the parent — translated, rotated, and mirrored according to the
//! reference — producing a single self-contained document with no embedded
//! references left.
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

    /// Build the transform that places a sub-board instance at `instance_origin`
    /// with the embedded board's `rotation` and `mirror_flag` applied.
    fn from_embedded_instance(board: &EmbeddedBoard, instance_origin: CoordPoint) -> Self {
        let theta = board.rotation.to_radians();
        let (sin_t, cos_t) = theta.sin_cos();
        let mx = if board.mirror_flag { -1.0 } else { 1.0 };
        // Combined linear part: rotation × mirror_x. Mirror is applied first
        // (closer to the source primitives), rotation second.
        Self {
            a: cos_t * mx,
            b: -sin_t,
            c: sin_t * mx,
            d: cos_t,
            tx: instance_origin.x.to_raw() as f64,
            ty: instance_origin.y.to_raw() as f64,
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

    /// Rotation extracted from the linear part, in degrees. For primitives
    /// that store a rotation field (Pad, Text, Component, Fill, …), we add
    /// this to their existing rotation.
    fn rotation_deg(&self) -> f64 {
        self.c.atan2(self.a).to_degrees()
    }

    /// `true` when the linear part has negative determinant — i.e. the
    /// transform involves an odd number of reflections.
    fn is_flipped(&self) -> bool {
        (self.a * self.d - self.b * self.c) < 0.0
    }

    /// Length-preserving scale factor of the linear part. For pure
    /// rotation/mirror this is 1; included so callers that need to scale
    /// world-space distances (e.g. an array's `col_spacing`) are correct in
    /// theory. We never produce non-unit scales today.
    fn linear_scale(&self) -> f64 {
        ((self.a * self.a + self.c * self.c).sqrt()
            + (self.b * self.b + self.d * self.d).sqrt())
            * 0.5
    }
}

// ─── Per-primitive transform appliers ──────────────────────────────────────

fn apply_to_pad(t: &WorldTransform, pad: &mut Pad) {
    pad.location = t.apply_point(pad.location);
    pad.rotation += t.rotation_deg();
    if t.is_flipped() {
        // Mirroring across x flips the rotation direction.
        pad.rotation = -pad.rotation;
    }
    // Pad sizes/shape arrays are pad-local dimensions, not world coords;
    // they don't need transforming.
}

fn apply_to_track(t: &WorldTransform, track: &mut Track) {
    track.start = t.apply_point(track.start);
    track.end = t.apply_point(track.end);
}

fn apply_to_arc(t: &WorldTransform, arc: &mut Arc) {
    arc.center = t.apply_point(arc.center);
    let rot = t.rotation_deg();
    arc.start_angle += rot;
    arc.end_angle += rot;
    if t.is_flipped() {
        // Mirror reverses sweep direction.
        let s = arc.start_angle;
        arc.start_angle = -arc.end_angle;
        arc.end_angle = -s;
    }
}

fn apply_to_via(t: &WorldTransform, via: &mut Via) {
    via.location = t.apply_point(via.location);
}

fn apply_to_text(t: &WorldTransform, text: &mut Text) {
    text.location = t.apply_point(text.location);
    text.rotation += t.rotation_deg();
}

fn apply_to_fill(t: &WorldTransform, fill: &mut Fill) {
    fill.corner1 = t.apply_point(fill.corner1);
    fill.corner2 = t.apply_point(fill.corner2);
    fill.rotation += t.rotation_deg();
}

fn apply_to_region(t: &WorldTransform, region: &mut Region) {
    for p in &mut region.outline {
        *p = t.apply_point(*p);
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
        let rot = t.rotation_deg();
        v.start_angle += rot;
        v.end_angle += rot;
        if t.is_flipped() {
            let s = v.start_angle;
            v.start_angle = -v.end_angle;
            v.end_angle = -s;
        }
    }
}

fn apply_to_body(t: &WorldTransform, body: &mut ComponentBody) {
    for p in &mut body.outline {
        *p = t.apply_point(*p);
    }
    body.model_2d_location = t.apply_point(body.model_2d_location);
    body.model_2d_rotation += t.rotation_deg();
}

fn apply_to_component(t: &WorldTransform, comp: &mut Component) {
    let new_xy = t.apply_point(CoordPoint::new(comp.x, comp.y));
    comp.x = new_xy.x;
    comp.y = new_xy.y;
    comp.rotation += t.rotation_deg();
    if t.is_flipped() {
        comp.flipped_on_layer = !comp.flipped_on_layer;
        comp.rotation = -comp.rotation;
    }
}

/// Apply `t` to the bounding box / array origin of an embedded board record
/// when we keep it in the output (depth limit, unresolved).
fn apply_to_embedded(t: &WorldTransform, b: &mut EmbeddedBoard) {
    let p1 = t.apply_point(CoordPoint::new(b.x1_location, b.y1_location));
    let p2 = t.apply_point(CoordPoint::new(b.x2_location, b.y2_location));
    b.x1_location = p1.x;
    b.y1_location = p1.y;
    b.x2_location = p2.x;
    b.y2_location = p2.y;
    b.rotation += t.rotation_deg();
    if t.is_flipped() {
        b.mirror_flag = !b.mirror_flag;
    }
    // col/row_spacing is a magnitude along the local x/y axes — under pure
    // rotation+translation+mirror the magnitude is preserved. We don't scale,
    // so no adjustment is needed.
    let _ = t.linear_scale(); // currently unused; reference for future extension
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
    // Free primitives.
    for x in &src.pads {
        let mut p = x.clone();
        apply_to_pad(transform, &mut p);
        out.pads.push(p);
    }
    for x in &src.tracks {
        let mut p = x.clone();
        apply_to_track(transform, &mut p);
        out.tracks.push(p);
    }
    for x in &src.arcs {
        let mut p = x.clone();
        apply_to_arc(transform, &mut p);
        out.arcs.push(p);
    }
    for x in &src.vias {
        let mut p = x.clone();
        apply_to_via(transform, &mut p);
        out.vias.push(p);
    }
    for x in &src.texts {
        let mut p = x.clone();
        apply_to_text(transform, &mut p);
        out.texts.push(p);
    }
    for x in &src.fills {
        let mut p = x.clone();
        apply_to_fill(transform, &mut p);
        out.fills.push(p);
    }
    for x in &src.regions {
        let mut p = x.clone();
        apply_to_region(transform, &mut p);
        out.regions.push(p);
    }
    for x in &src.component_bodies {
        let mut p = x.clone();
        apply_to_body(transform, &mut p);
        out.component_bodies.push(p);
    }
    for x in &src.polygons {
        let mut p = x.clone();
        apply_to_polygon(transform, &mut p);
        out.polygons.push(p);
    }

    // Components — preserved as components (not torn apart). This keeps
    // designators/refs intact for downstream consumers.
    for comp in &src.components {
        let mut c = comp.clone();
        apply_to_component(transform, &mut c);
        out.components.push(c);
    }

    // Nets, rules, classes etc. are name-based metadata. We dedupe nets by
    // name; everything else is appended as-is so cross-board rules survive.
    for net in &src.nets {
        if !out.nets.iter().any(|n| n.name == net.name) {
            out.nets.push(net.clone());
        }
    }
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
                let cols = board.col_count.max(1);
                let rows = board.row_count.max(1);
                for col in 0..cols {
                    for row in 0..rows {
                        let inst_origin = CoordPoint::new(
                            board.x1_location + board.col_spacing * col,
                            board.y1_location + board.row_spacing * row,
                        );
                        let local = WorldTransform::from_embedded_instance(board, inst_origin);
                        let composed = transform.compose(&local);
                        flatten_into(&sub, &composed, loader, depth + 1, out, diagnostics);
                    }
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
    fn rotation_90_takes_x_to_y() {
        let mut b = EmbeddedBoard::default();
        b.rotation = 90.0;
        let t = WorldTransform::from_embedded_instance(&b, pt(0.0, 0.0));
        // (1, 0) -> (0, 1)
        let q = t.apply_point(pt(10.0, 0.0));
        // Tolerate rounding: should be very close to (0, 10).
        let dx = (q.x.to_raw() - 0).abs();
        let dy = (q.y.to_raw() - Coord::from_mils(10.0).to_raw()).abs();
        assert!(dx < 5 && dy < 5, "got {q:?}");
    }

    #[test]
    fn mirror_flips_x() {
        let mut b = EmbeddedBoard::default();
        b.mirror_flag = true;
        let t = WorldTransform::from_embedded_instance(&b, pt(0.0, 0.0));
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
        let outer = WorldTransform::from_embedded_instance(&outer_board, pt(0.0, 0.0));
        let composed = outer.compose(&inner);
        // Inner takes (0,0) → (10,0). Outer rotates that 90° → (0,10).
        let q = composed.apply_point(pt(0.0, 0.0));
        let dx = (q.x.to_raw() - 0).abs();
        let dy = (q.y.to_raw() - Coord::from_mils(10.0).to_raw()).abs();
        assert!(dx < 5 && dy < 5, "expected ~(0, 10), got {q:?}");
    }
}
