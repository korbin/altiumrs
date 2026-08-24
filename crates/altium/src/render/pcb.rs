//! PCB component renderer.

use std::f64::consts::PI;

use super::context::{RenderContext, RenderOptions, TextAnchorH, TextAnchorV, TextStyle};
use super::layer_colors;
use super::svg::SvgContext;
use super::transform::CoordTransform;
use crate::color::Color;
use crate::coord::{CoordPoint, CoordRect};
use crate::enums::PadShape;
use crate::pcb::embedded::{BoardLoader, EmbeddedBoard};
use crate::pcb::primitives::{Arc, ComponentBody, Fill, Pad, Region, Text, Track, Via};
use crate::pcb::{Component, Document, binary as pcb_binary};

/// Maximum embedded-board recursion depth. Past this we draw a placeholder
/// instead of resolving, which prevents infinite loops if a sub-board ever
/// references its own ancestor.
const MAX_EMBEDDED_DEPTH: u8 = 8;

fn layer_color(layer: i32) -> Color {
    layer_colors::color_for(layer)
}

fn draw_fill<C: RenderContext>(ctx: &mut C, t: &CoordTransform, fill: &Fill) {
    let (x1, y1) = t.world_to_screen(fill.corner1);
    let (x2, y2) = t.world_to_screen(fill.corner2);
    let x = x1.min(x2);
    let y = y1.min(y2);
    let w = (x2 - x1).abs();
    let h = (y2 - y1).abs();
    if fill.rotation != 0.0 {
        let cx = x + w / 2.0;
        let cy = y + h / 2.0;
        ctx.save_state();
        ctx.translate(cx, cy);
        ctx.rotate(fill.rotation);
        ctx.rectangle(
            -w / 2.0,
            -h / 2.0,
            w,
            h,
            0.0,
            None,
            Some(layer_color(fill.layer)),
        );
        ctx.restore_state();
    } else {
        ctx.rectangle(x, y, w, h, 0.0, None, Some(layer_color(fill.layer)));
    }
}

fn draw_region<C: RenderContext>(ctx: &mut C, t: &CoordTransform, region: &Region) {
    if region.outline.is_empty() {
        return;
    }
    let pts: Vec<(f64, f64)> = region
        .outline
        .iter()
        .map(|p| t.world_to_screen(*p))
        .collect();
    ctx.polygon(&pts, 0.0, None, Some(layer_color(region.layer)));
}

fn draw_arc<C: RenderContext>(ctx: &mut C, t: &CoordTransform, arc: &Arc) {
    let (cx, cy) = t.world_to_screen(arc.center);
    let r = t.scale_value(arc.radius);
    let w = t.scale_value(arc.width).max(0.5);
    if (arc.end_angle - arc.start_angle).abs() >= 359.99 {
        ctx.circle(cx, cy, r, w, Some(layer_color(arc.layer)), None);
    } else {
        ctx.arc(
            cx,
            cy,
            r,
            arc.start_angle,
            arc.end_angle,
            w,
            layer_color(arc.layer),
        );
    }
}

fn draw_track<C: RenderContext>(ctx: &mut C, t: &CoordTransform, track: &Track) {
    let (x1, y1) = t.world_to_screen(track.start);
    let (x2, y2) = t.world_to_screen(track.end);
    let w = t.scale_value(track.width).max(0.5);
    ctx.line(x1, y1, x2, y2, w, layer_color(track.layer));
}

/// Draw the eight-sided polygon for a `PadShape::Octagonal` pad.
///
/// Cuts the corners by 1/3 of the smallest dimension — matches Altium's
/// renderer.
fn octagonal_pad_points(cx: f64, cy: f64, w: f64, h: f64) -> [(f64, f64); 8] {
    let cut = (w.min(h) / 3.0).max(0.0);
    let half_w = w / 2.0;
    let half_h = h / 2.0;
    [
        (cx - half_w + cut, cy - half_h),
        (cx + half_w - cut, cy - half_h),
        (cx + half_w, cy - half_h + cut),
        (cx + half_w, cy + half_h - cut),
        (cx + half_w - cut, cy + half_h),
        (cx - half_w + cut, cy + half_h),
        (cx - half_w, cy + half_h - cut),
        (cx - half_w, cy - half_h + cut),
    ]
}

fn draw_pad<C: RenderContext>(ctx: &mut C, t: &CoordTransform, pad: &Pad, draw_designator: bool) {
    let (cx, cy) = t.world_to_screen(pad.location);
    let w = t.scale_value(pad.size_top.x);
    let h = t.scale_value(pad.size_top.y);
    if w < 0.5 && h < 0.5 {
        return;
    }
    let color = layer_color(pad.layer);

    let rotation = pad.rotation;
    let needs_rotation = rotation != 0.0;
    if needs_rotation {
        ctx.save_state();
        ctx.translate(cx, cy);
        ctx.rotate(rotation);
    }
    let (px, py) = if needs_rotation { (0.0, 0.0) } else { (cx, cy) };

    // Rounded-rectangle / chamfered-rectangle corner radius is
    // `corner_radius_percentage` % of half the smaller side. The reader
    // populates this from the SS block's per-layer corner-radius array
    // (top-layer slot), so it's authoritative for file-loaded pads. For
    // builder-constructed pads it's the default 50.
    let rr_radius = {
        let pct = pad.corner_radius_percentage.clamp(0, 100) as f64 / 100.0;
        (w.min(h) / 2.0) * pct
    };
    match pad.shape_top {
        PadShape::Round => {
            // "Round" in Altium = ellipse / oval / circle depending on aspect ratio.
            ctx.ellipse(px, py, w / 2.0, h / 2.0, 0.0, None, Some(color));
        }
        PadShape::Rectangular => {
            ctx.rectangle(px - w / 2.0, py - h / 2.0, w, h, 0.0, None, Some(color));
        }
        PadShape::Octagonal => {
            let pts = octagonal_pad_points(px, py, w, h);
            ctx.polygon(&pts, 0.0, None, Some(color));
        }
        PadShape::RoundedRectangle => {
            ctx.rounded_rectangle(
                px - w / 2.0,
                py - h / 2.0,
                w,
                h,
                rr_radius,
                rr_radius,
                0.0,
                None,
                Some(color),
            );
        }
        PadShape::ChamferedRectangle => {
            // Octagon with chamfer cut = corner-radius percentage of
            // half the smaller side. Falls back to a plain rectangle
            // when the chamfer would be zero.
            if rr_radius <= 0.0 {
                ctx.rectangle(px - w / 2.0, py - h / 2.0, w, h, 0.0, None, Some(color));
            } else {
                let cut = rr_radius.min(w.min(h) / 2.0);
                let half_w = w / 2.0;
                let half_h = h / 2.0;
                let pts = [
                    (px - half_w + cut, py - half_h),
                    (px + half_w - cut, py - half_h),
                    (px + half_w, py - half_h + cut),
                    (px + half_w, py + half_h - cut),
                    (px + half_w - cut, py + half_h),
                    (px - half_w + cut, py + half_h),
                    (px - half_w, py + half_h - cut),
                    (px - half_w, py - half_h + cut),
                ];
                ctx.polygon(&pts, 0.0, None, Some(color));
            }
        }
        PadShape::CustomShape => {
            // Outline lives in an associated Region6; without a back-
            // reference here, drawing the bbox is the best we can do
            // and still keeps the pad visible / hit-testable.
            ctx.rectangle(px - w / 2.0, py - h / 2.0, w, h, 0.0, None, Some(color));
        }
        PadShape::NoShape | PadShape::Unknown(_) => {
            // Unmodelled shape byte — render the bbox so the pad is at
            // least visible, rather than silently downgrading to an
            // ellipse (the pre-fix behaviour).
            ctx.rectangle(px - w / 2.0, py - h / 2.0, w, h, 0.0, None, Some(color));
        }
    }

    // Through-hole — honour HoleType (Round / Square / Slot) and HoleRotation
    // independently of pad rotation. Mirrors C# PcbComponentRenderer.RenderPad
    // (lines 147-162).
    if pad.hole_size.to_raw() > 0 {
        use crate::enums::PadHoleType;
        let hr = t.scale_value(pad.hole_size) / 2.0;
        let hole_color = layer_color(81); // pad-hole layer
        if pad.hole_rotation != 0.0 {
            ctx.save_state();
            ctx.translate(px, py);
            ctx.rotate(pad.hole_rotation);
        }
        let (hx, hy) = if pad.hole_rotation != 0.0 {
            (0.0, 0.0)
        } else {
            (px, py)
        };
        match pad.hole_type {
            PadHoleType::Round => {
                ctx.circle(hx, hy, hr, 0.0, None, Some(hole_color));
            }
            PadHoleType::Square => {
                ctx.rectangle(
                    hx - hr,
                    hy - hr,
                    hr * 2.0,
                    hr * 2.0,
                    0.0,
                    None,
                    Some(hole_color),
                );
            }
            PadHoleType::Slot => {
                let slot_len = t
                    .scale_value(crate::Coord::from_raw(pad.hole_slot_length))
                    .max(hr * 2.0);
                ctx.rounded_rectangle(
                    hx - slot_len / 2.0,
                    hy - hr,
                    slot_len,
                    hr * 2.0,
                    hr,
                    hr,
                    0.0,
                    None,
                    Some(hole_color),
                );
            }
        }
        if pad.hole_rotation != 0.0 {
            ctx.restore_state();
        }
    }
    if needs_rotation {
        ctx.restore_state();
    }

    if draw_designator {
        if let Some(d) = &pad.designator {
            // Designator color: through-hole = orange, SMD = light blue.
            // Matches C# PcbComponentRenderer:178-181.
            let designator_color = if pad.hole_size.to_raw() > 0 {
                Color::from_argb_u32(0xFFE38F00) // orange
            } else {
                Color::from_argb_u32(0xFFB5B5FF) // light blue
            };
            let style = TextStyle {
                h_anchor: TextAnchorH::Center,
                v_anchor: TextAnchorV::Middle,
                // Designator stays unrotated relative to the pad (Altium
                // convention: keep designators readable regardless of pad
                // orientation).
                rotation: 0.0,
                ..TextStyle::default()
            };
            ctx.text_styled(cx, cy, h.clamp(8.0, 24.0), d, designator_color, style);
        }
    }
}

fn draw_via<C: RenderContext>(ctx: &mut C, t: &CoordTransform, via: &Via) {
    let (cx, cy) = t.world_to_screen(via.location);
    let r = t.scale_value(via.diameter) / 2.0;
    if r < 0.3 {
        return;
    }
    // Cross-layer vias show two half-pies coloured by the from/to layers.
    // Mirrors C# PcbComponentRenderer.RenderVia (lines 236-244).
    let from_color = layer_color(via.start_layer);
    let to_color = layer_color(via.end_layer);
    if from_color == to_color {
        ctx.circle(cx, cy, r, 0.0, None, Some(from_color));
    } else {
        // Left half: 90° → 270° (going counter-clockwise through 180°).
        draw_half_pie(ctx, cx, cy, r, 90.0, 270.0, from_color);
        // Right half: -90° → 90° (counter-clockwise through 0°).
        draw_half_pie(ctx, cx, cy, r, -90.0, 90.0, to_color);
    }
    if via.hole_size.to_raw() > 0 {
        let hr = t.scale_value(via.hole_size) / 2.0;
        ctx.circle(cx, cy, hr, 0.0, None, Some(Color::rgb(0x20, 0x20, 0x20)));
    }
}

/// Draw a half-disk as a polygon: centre + arc samples between `start`/`end`.
fn draw_half_pie<C: RenderContext>(
    ctx: &mut C,
    cx: f64,
    cy: f64,
    r: f64,
    start_deg: f64,
    end_deg: f64,
    color: Color,
) {
    const STEPS: u32 = 24;
    let mut pts = Vec::with_capacity(STEPS as usize + 2);
    pts.push((cx, cy));
    for i in 0..=STEPS {
        let t = i as f64 / STEPS as f64;
        let a = (start_deg + (end_deg - start_deg) * t).to_radians();
        pts.push((cx + r * a.cos(), cy - r * a.sin()));
    }
    ctx.polygon(&pts, 0.0, None, Some(color));
}

fn draw_body<C: RenderContext>(ctx: &mut C, t: &CoordTransform, body: &ComponentBody) {
    if body.outline.is_empty() {
        return;
    }
    let pts: Vec<(f64, f64)> = body.outline.iter().map(|p| t.world_to_screen(*p)).collect();
    // Translucent fill + opaque outline. Matches C#
    // PcbComponentRenderer.RenderComponentBody fill colour 0x80808080 + stroke 0xFF808080.
    let stroke_c = Color::rgb(0x80, 0x80, 0x80);
    let fill_c = Color::rgba(0x80, 0x80, 0x80, 0x80);
    ctx.polygon(&pts, 1.0, Some(stroke_c), Some(fill_c));
}

fn draw_text<C: RenderContext>(ctx: &mut C, t: &CoordTransform, text: &Text) {
    use crate::enums::PcbTextKind;
    let (x, y) = t.world_to_screen(text.location);
    // Stroke fonts: pen overshoots glyph bounding box by stroke_width;
    // TrueType fonts use the glyph height directly. Matches C#
    // PcbComponentRenderer.RenderText (lines 328-331).
    let mut h = t.scale_value(text.height);
    if matches!(text.text_kind, PcbTextKind::Stroke) {
        h += t.scale_value(text.stroke_width);
    }
    let h = h.max(8.0);
    let mut style = TextStyle {
        bold: text.font_bold,
        italic: text.font_italic,
        h_anchor: justification_to_h(text.justification),
        v_anchor: justification_to_v(text.justification),
        rotation: if text.is_mirrored {
            -text.rotation
        } else {
            text.rotation
        },
    };
    // Mirroring flips horizontally — equivalent to rotating 180° around the
    // vertical axis, which we approximate as negating rotation. For small
    // glyph counts this is close enough; full mirror requires a -1 x-scale.
    if text.is_mirrored {
        style.rotation += 180.0;
    }
    ctx.text_styled(x, y, h, &text.text, layer_color(text.layer), style);
}

fn justification_to_h(j: crate::enums::TextJustification) -> TextAnchorH {
    use crate::enums::TextJustification as J;
    match j {
        J::BottomLeft | J::MiddleLeft | J::TopLeft => TextAnchorH::Left,
        J::BottomCenter | J::MiddleCenter | J::TopCenter => TextAnchorH::Center,
        J::BottomRight | J::MiddleRight | J::TopRight => TextAnchorH::Right,
    }
}

fn justification_to_v(j: crate::enums::TextJustification) -> TextAnchorV {
    use crate::enums::TextJustification as J;
    match j {
        J::BottomLeft | J::BottomCenter | J::BottomRight => TextAnchorV::Bottom,
        J::MiddleLeft | J::MiddleCenter | J::MiddleRight => TextAnchorV::Middle,
        J::TopLeft | J::TopCenter | J::TopRight => TextAnchorV::Top,
    }
}

/// One PCB primitive in a single ordered draw list.
///
/// `body` doesn't have a layer byte — it carries `layer_name`. We resolve to a
/// layer byte for priority sorting via [`super::binary::layer_name_to_byte`].
enum PcbPrim<'a> {
    Fill(&'a Fill),
    Region(&'a Region),
    Arc(&'a Arc),
    Track(&'a Track),
    Via(&'a Via),
    Pad(&'a Pad, bool /* draw_designator */),
    Body(&'a ComponentBody),
    Text(&'a Text),
}

impl PcbPrim<'_> {
    fn layer(&self) -> i32 {
        match self {
            Self::Fill(f) => f.layer,
            Self::Region(r) => r.layer,
            Self::Arc(a) => a.layer,
            Self::Track(t) => t.layer,
            Self::Via(_) => 74, // multi-layer
            Self::Pad(p, _) => p.layer,
            Self::Body(b) => {
                if b.layer != 0 {
                    b.layer
                } else {
                    pcb_binary::layer_name_to_byte(&b.layer_name) as i32
                }
            }
            Self::Text(t) => t.layer,
        }
    }

    /// Within-priority tiebreaker — lower draws first. Fills below tracks
    /// below pads below text, mirroring Altium's intuitive copper layering.
    fn type_order(&self) -> u8 {
        match self {
            Self::Fill(_) => 0,
            Self::Region(_) => 1,
            Self::Arc(_) => 2,
            Self::Track(_) => 3,
            Self::Via(_) => 4,
            Self::Pad(_, _) => 5,
            Self::Body(_) => 6,
            Self::Text(_) => 7,
        }
    }

    fn priority_key(&self) -> (i32, u8) {
        (layer_colors::draw_priority(self.layer()), self.type_order())
    }

    fn draw<C: RenderContext>(&self, ctx: &mut C, t: &CoordTransform) {
        match self {
            Self::Fill(f) => draw_fill(ctx, t, f),
            Self::Region(r) => draw_region(ctx, t, r),
            Self::Arc(a) => draw_arc(ctx, t, a),
            Self::Track(tr) => draw_track(ctx, t, tr),
            Self::Via(v) => draw_via(ctx, t, v),
            Self::Pad(p, draw_des) => draw_pad(ctx, t, p, *draw_des),
            Self::Body(b) => draw_body(ctx, t, b),
            Self::Text(tx) => draw_text(ctx, t, tx),
        }
    }
}

fn collect_component<'a>(
    component: &'a Component,
    out: &mut Vec<PcbPrim<'a>>,
    draw_designators: bool,
) {
    out.extend(component.fills.iter().map(PcbPrim::Fill));
    out.extend(component.regions.iter().map(PcbPrim::Region));
    out.extend(component.arcs.iter().map(PcbPrim::Arc));
    out.extend(component.tracks.iter().map(PcbPrim::Track));
    out.extend(component.vias.iter().map(PcbPrim::Via));
    out.extend(
        component
            .pads
            .iter()
            .map(|p| PcbPrim::Pad(p, draw_designators)),
    );
    out.extend(component.component_bodies.iter().map(PcbPrim::Body));
    out.extend(component.texts.iter().map(PcbPrim::Text));
}

fn collect_document<'a>(document: &'a Document, out: &mut Vec<PcbPrim<'a>>) {
    out.extend(document.fills.iter().map(PcbPrim::Fill));
    out.extend(document.regions.iter().map(PcbPrim::Region));
    out.extend(document.arcs.iter().map(PcbPrim::Arc));
    out.extend(document.tracks.iter().map(PcbPrim::Track));
    out.extend(document.vias.iter().map(PcbPrim::Via));
    out.extend(document.pads.iter().map(|p| PcbPrim::Pad(p, false)));
    out.extend(document.component_bodies.iter().map(PcbPrim::Body));
    out.extend(document.texts.iter().map(PcbPrim::Text));
    for c in &document.components {
        collect_component(c, out, false);
    }
}

pub fn render_component<C: RenderContext>(ctx: &mut C, component: &Component, t: &CoordTransform) {
    let _ = PI;
    let mut prims = Vec::with_capacity(
        component.fills.len()
            + component.regions.len()
            + component.arcs.len()
            + component.tracks.len()
            + component.vias.len()
            + component.pads.len()
            + component.component_bodies.len()
            + component.texts.len(),
    );
    collect_component(component, &mut prims, true);
    prims.sort_by_key(|p| p.priority_key());
    for p in &prims {
        p.draw(ctx, t);
    }
}

pub fn render_document<C: RenderContext>(ctx: &mut C, document: &Document, t: &CoordTransform) {
    render_document_inner(ctx, document, t, None, 0);
}

/// Render with embedded sub-boards resolved through `loader`. Each
/// `EmbeddedBoard` reference is dereferenced (recursively, depth-limited) and
/// drawn in place, including array replication via
/// `col_count`/`row_count`/`col_spacing`/`row_spacing` and any
/// `rotation`/`mirror_flag`. References that fail to resolve fall back to a
/// labelled placeholder rectangle.
pub fn render_document_with_loader<C: RenderContext>(
    ctx: &mut C,
    document: &Document,
    t: &CoordTransform,
    loader: &dyn BoardLoader,
) {
    render_document_inner(ctx, document, t, Some(loader), 0);
}

fn render_document_inner<C: RenderContext>(
    ctx: &mut C,
    document: &Document,
    t: &CoordTransform,
    loader: Option<&dyn BoardLoader>,
    depth: u8,
) {
    // Parent primitives first, sorted by Altium layer-stack draw priority.
    let mut prims = Vec::new();
    collect_document(document, &mut prims);
    prims.sort_by_key(|p| p.priority_key());
    for p in &prims {
        p.draw(ctx, t);
    }

    // Then each embedded board on top — either as a recursive sub-render or a
    // placeholder. Using a fixed parent-then-child order assumes the stack-up
    // matches across the boundary; with mixed stack-ups you'd want to merge
    // and re-sort priorities, which we deliberately don't do.
    for board in &document.embedded_boards {
        draw_embedded_board(ctx, t, board, loader, depth);
    }
}

fn draw_embedded_board<C: RenderContext>(
    ctx: &mut C,
    t: &CoordTransform,
    board: &EmbeddedBoard,
    loader: Option<&dyn BoardLoader>,
    depth: u8,
) {
    // Resolve the sub-document if a loader is wired up and we're under the
    // recursion ceiling. Anything else falls back to a placeholder.
    let sub_doc = if depth < MAX_EMBEDDED_DEPTH {
        match loader {
            Some(l) => board.resolve_with(l).ok(),
            None => None,
        }
    } else {
        None
    };

    let cols = board.col_count.max(1);
    let rows = board.row_count.max(1);

    for col in 0..cols {
        for row in 0..rows {
            // Placement semantics (same as `pcb::flatten`): the record's
            // `X`/`Y` is where the sub-board's own board origin lands, with
            // rotation/mirror applied about that anchor. `X1..Y2` is only
            // the cached bounding box.
            let inst_translation = CoordPoint::new(
                board.x_location + board.col_spacing * col,
                board.y_location + board.row_spacing * row,
            );
            let inst_translation_screen = t.world_to_screen(inst_translation);

            match &sub_doc {
                Some(sub) => {
                    // Screen-space composition of
                    //   p_parent = R·M·(p_child − child_origin) + translation:
                    // translate to the instance point, rotate/mirror, then
                    // shift the child's board origin onto that anchor.
                    let child_origin_screen = t.world_to_screen(sub.board_origin());
                    ctx.save_state();
                    ctx.translate(inst_translation_screen.0, inst_translation_screen.1);
                    if board.rotation != 0.0 {
                        ctx.rotate(board.rotation);
                    }
                    if board.mirror_flag {
                        ctx.scale(-1.0, 1.0);
                    }
                    ctx.translate(-child_origin_screen.0, -child_origin_screen.1);

                    render_document_inner(ctx, sub, t, loader, depth + 1);
                    ctx.restore_state();
                }
                None => {
                    // Placeholder — the cached bounding box from X1/Y1/X2/Y2,
                    // shifted by the per-instance array offset.
                    let dx = board.col_spacing * col;
                    let dy = board.row_spacing * row;
                    let p1 = CoordPoint::new(board.x1_location + dx, board.y1_location + dy);
                    let p2 = CoordPoint::new(board.x2_location + dx, board.y2_location + dy);
                    draw_embedded_placeholder(ctx, t, board, p1, p2);
                }
            }
        }
    }
}

fn draw_embedded_placeholder<C: RenderContext>(
    ctx: &mut C,
    t: &CoordTransform,
    board: &EmbeddedBoard,
    world_p1: CoordPoint,
    world_p2: CoordPoint,
) {
    let (sx1, sy1) = t.world_to_screen(world_p1);
    let (sx2, sy2) = t.world_to_screen(world_p2);
    let x = sx1.min(sx2);
    let y = sy1.min(sy2);
    let w = (sx2 - sx1).abs();
    let h = (sy2 - sy1).abs();
    if w < 1.0 || h < 1.0 {
        return;
    }

    // Visible but subdued — dashed-feeling stroke + faint fill — so a missing
    // sibling stands out without dominating the parent draw.
    let stroke = layer_color(board.layer);
    let fill = Color {
        r: stroke.r,
        g: stroke.g,
        b: stroke.b,
        a: 0x20,
    };
    ctx.rectangle(x, y, w, h, 2.0, Some(stroke), Some(fill));

    // Diagonals to make the "this is a placeholder" intent unambiguous.
    ctx.line(x, y, x + w, y + h, 1.0, stroke);
    ctx.line(x, y + h, x + w, y, 1.0, stroke);

    // Label with the document_path so the user can tell which sub-board is
    // missing without inspecting the model.
    if let Some(path) = &board.document_path {
        if !path.is_empty() {
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            let style = TextStyle {
                h_anchor: TextAnchorH::Center,
                v_anchor: TextAnchorV::Middle,
                ..Default::default()
            };
            ctx.text_styled(cx, cy, 12.0, path, stroke, style);
        }
    }
}

impl Component {
    pub fn render_svg(&self, options: RenderOptions) -> String {
        let bounds = self.bounds();
        let t = CoordTransform::fit(bounds, options.width, options.height, options.padding);
        let mut ctx = SvgContext::new(options);
        render_component(&mut ctx, self, &t);
        ctx.finish()
    }

    pub fn render_png(&self, options: RenderOptions) -> Result<Vec<u8>, String> {
        let bounds = self.bounds();
        let t = CoordTransform::fit(bounds, options.width, options.height, options.padding);
        let mut ctx = super::raster::TinySkiaContext::new(options);
        render_component(&mut ctx, self, &t);
        ctx.encode_png()
    }
}

impl Document {
    /// Render to SVG. Embedded board references draw as labelled placeholders
    /// — wire up a [`BoardLoader`] via [`render_svg_with_loader`](Self::render_svg_with_loader)
    /// to fully render sub-documents.
    pub fn render_svg(&self, options: RenderOptions) -> String {
        let bounds = self.render_bounds();
        let t = CoordTransform::fit(bounds, options.width, options.height, options.padding);
        let mut ctx = SvgContext::new(options);
        render_document(&mut ctx, self, &t);
        ctx.finish()
    }

    pub fn render_png(&self, options: RenderOptions) -> Result<Vec<u8>, String> {
        let bounds = self.render_bounds();
        let t = CoordTransform::fit(bounds, options.width, options.height, options.padding);
        let mut ctx = super::raster::TinySkiaContext::new(options);
        render_document(&mut ctx, self, &t);
        ctx.encode_png()
    }

    /// Render to SVG, recursively dereferencing every [`EmbeddedBoard`]
    /// through `loader`. References that don't resolve fall back to a
    /// labelled placeholder so the failure is visible rather than silent.
    pub fn render_svg_with_loader(
        &self,
        options: RenderOptions,
        loader: &dyn BoardLoader,
    ) -> String {
        let bounds = self.render_bounds();
        let t = CoordTransform::fit(bounds, options.width, options.height, options.padding);
        let mut ctx = SvgContext::new(options);
        render_document_with_loader(&mut ctx, self, &t, loader);
        ctx.finish()
    }

    /// PNG counterpart to [`render_svg_with_loader`](Self::render_svg_with_loader).
    pub fn render_png_with_loader(
        &self,
        options: RenderOptions,
        loader: &dyn BoardLoader,
    ) -> Result<Vec<u8>, String> {
        let bounds = self.render_bounds();
        let t = CoordTransform::fit(bounds, options.width, options.height, options.padding);
        let mut ctx = super::raster::TinySkiaContext::new(options);
        render_document_with_loader(&mut ctx, self, &t, loader);
        ctx.encode_png()
    }

    /// World-space rectangle that bounds everything the document will draw,
    /// including embedded-board reference rectangles (so sub-content fits
    /// the auto-fit transform without clipping).
    pub fn render_bounds(&self) -> CoordRect {
        let mut acc = self.bounds();
        for comp in &self.components {
            acc = acc.union(comp.bounds());
        }
        for board in &self.embedded_boards {
            acc = acc.union(embedded_array_bounds(board));
        }
        acc
    }
}

/// World-space bounds of every instance of an embedded board's array.
fn embedded_array_bounds(board: &EmbeddedBoard) -> CoordRect {
    let cols = board.col_count.max(1);
    let rows = board.row_count.max(1);
    let dx = board.col_spacing * (cols - 1).max(0);
    let dy = board.row_spacing * (rows - 1).max(0);
    CoordRect::from_xyxy(
        board.x1_location.min(board.x2_location),
        board.y1_location.min(board.y2_location),
        board.x1_location.max(board.x2_location) + dx,
        board.y1_location.max(board.y2_location) + dy,
    )
}
