//! Schematic component / document renderer.

#![allow(clippy::too_many_arguments)]

use super::context::{RenderContext, RenderOptions, TextAnchorH, TextAnchorV, TextStyle};
use super::svg::SvgContext;
use super::transform::CoordTransform;
use crate::color::Color;
use crate::coord::Coord;
use crate::enums::{PinElectricalType, PinOrientation, PowerPortStyle, TextJustification};
use crate::sch::primitives::{
    Arc, Bezier, Bus, BusEntry, Ellipse, EllipticalArc, Image, Junction, Label, Line, NetLabel,
    Parameter, Pie, Pin, Polygon, Polyline, Port, PowerObject, Rectangle, RoundedRectangle,
    SheetSymbol, TextFrame, Wire,
};
use crate::sch::{Component, Document};

// Default colors (from C# SchComponentRenderer.cs:21-27)

const DEFAULT_PIN: Color = Color::rgb(0x00, 0x00, 0x8B); // DarkBlue
const DEFAULT_WIRE: Color = Color::rgb(0x00, 0x00, 0x8B);
const DEFAULT_JUNCTION: Color = Color::rgb(0x00, 0x00, 0x80); // Navy
const DEFAULT_BUS: Color = Color::rgb(0x00, 0x00, 0x80);
const DEFAULT_LINE: f64 = 1.0;
const DEFAULT_FONT_SIZE: f64 = 10.0;

fn argb_or_default(altium_bgr: i32, default: Color) -> Color {
    if altium_bgr != 0 {
        Color::from_altium_bgr(altium_bgr)
    } else {
        default
    }
}

fn justification_anchors(j: TextJustification) -> (TextAnchorH, TextAnchorV) {
    use TextJustification as J;
    match j {
        J::BottomLeft => (TextAnchorH::Left, TextAnchorV::Bottom),
        J::BottomCenter => (TextAnchorH::Center, TextAnchorV::Bottom),
        J::BottomRight => (TextAnchorH::Right, TextAnchorV::Bottom),
        J::MiddleLeft => (TextAnchorH::Left, TextAnchorV::Middle),
        J::MiddleCenter => (TextAnchorH::Center, TextAnchorV::Middle),
        J::MiddleRight => (TextAnchorH::Right, TextAnchorV::Middle),
        J::TopLeft => (TextAnchorH::Left, TextAnchorV::Top),
        J::TopCenter => (TextAnchorH::Center, TextAnchorV::Top),
        J::TopRight => (TextAnchorH::Right, TextAnchorV::Top),
    }
}

/// Resolve `=Foo` parameter references for a component. Mirrors C#
/// `SchComponentRenderer.ResolveStringIndirection` (lines 1208-1227).
fn resolve_string<'a>(text: &'a str, component: Option<&Component>) -> std::borrow::Cow<'a, str> {
    if !text.starts_with('=') {
        return std::borrow::Cow::Borrowed(text);
    }
    let name = &text[1..];
    if let Some(c) = component {
        for p in &c.parameters {
            if p.name.eq_ignore_ascii_case(name) {
                return std::borrow::Cow::Owned(p.value.clone());
            }
        }
    }
    std::borrow::Cow::Owned(name.to_string())
}

fn line_width_px(idx: i32) -> f64 {
    match idx {
        1 => 3.0,
        2 => 5.0,
        _ => 1.0,
    }
}

// Primitive renderers

fn render_rectangle<C: RenderContext>(ctx: &mut C, t: &CoordTransform, r: &Rectangle) {
    let (x1, y1) = t.world_to_screen(r.corner1);
    let (x2, y2) = t.world_to_screen(r.corner2);
    let x = x1.min(x2);
    let y = y1.min(y2);
    let w = (x2 - x1).abs();
    let h = (y2 - y1).abs();
    let stroke = if r.color != 0 {
        Some(Color::from_altium_bgr(r.color))
    } else {
        Some(Color::BLACK)
    };
    let fill = if r.is_filled && !r.is_transparent {
        Some(Color::from_altium_bgr(r.fill_color))
    } else {
        None
    };
    let lw = t.scale_value(r.line_width).max(DEFAULT_LINE);
    ctx.rectangle(x, y, w, h, lw, stroke, fill);
}

fn render_rounded_rect<C: RenderContext>(ctx: &mut C, t: &CoordTransform, r: &RoundedRectangle) {
    let (x1, y1) = t.world_to_screen(r.corner1);
    let (x2, y2) = t.world_to_screen(r.corner2);
    let x = x1.min(x2);
    let y = y1.min(y2);
    let w = (x2 - x1).abs();
    let h = (y2 - y1).abs();
    let rx = t.scale_value(r.corner_radius_x);
    let ry = t.scale_value(r.corner_radius_y);
    let stroke = if r.color != 0 {
        Some(Color::from_altium_bgr(r.color))
    } else {
        Some(Color::BLACK)
    };
    let fill = if r.is_filled && !r.is_transparent {
        Some(Color::from_altium_bgr(r.fill_color))
    } else {
        None
    };
    ctx.rounded_rectangle(
        x,
        y,
        w,
        h,
        rx,
        ry,
        line_width_px(r.line_width),
        stroke,
        fill,
    );
}

fn render_line<C: RenderContext>(ctx: &mut C, t: &CoordTransform, line: &Line) {
    let (x1, y1) = t.world_to_screen(line.start);
    let (x2, y2) = t.world_to_screen(line.end);
    let color = if line.color != 0 {
        Color::from_altium_bgr(line.color)
    } else {
        Color::BLACK
    };
    let lw = t.scale_value(line.width).max(DEFAULT_LINE);
    ctx.line(x1, y1, x2, y2, lw, color);
}

fn render_arc<C: RenderContext>(ctx: &mut C, t: &CoordTransform, arc: &Arc) {
    let (cx, cy) = t.world_to_screen(arc.center);
    let r = t.scale_value(arc.radius);
    let color = if arc.color != 0 {
        Color::from_altium_bgr(arc.color)
    } else {
        Color::BLACK
    };
    let lw = line_width_px(arc.line_width);
    if (arc.end_angle - arc.start_angle).abs() >= 359.99 {
        ctx.circle(cx, cy, r, lw, Some(color), None);
    } else {
        ctx.arc(cx, cy, r, arc.start_angle, arc.end_angle, lw, color);
    }
}

fn render_elliptical_arc<C: RenderContext>(ctx: &mut C, t: &CoordTransform, ea: &EllipticalArc) {
    let (cx, cy) = t.world_to_screen(ea.center);
    let rx = t.scale_value(ea.primary_radius);
    let ry = t.scale_value(ea.secondary_radius);
    let color = if ea.color != 0 {
        Color::from_altium_bgr(ea.color)
    } else {
        Color::BLACK
    };
    let lw = t.scale_value(ea.line_width).max(DEFAULT_LINE);
    ctx.elliptical_arc(cx, cy, rx, ry, ea.start_angle, ea.end_angle, lw, color);
}

fn render_pie<C: RenderContext>(ctx: &mut C, t: &CoordTransform, p: &Pie) {
    let (cx, cy) = t.world_to_screen(p.center);
    let r = t.scale_value(p.radius);
    let color = if p.color != 0 {
        Color::from_altium_bgr(p.color)
    } else {
        Color::BLACK
    };
    let lw = line_width_px(p.line_width);
    // Approximate pie as an arc + two radii to center.
    ctx.arc(cx, cy, r, p.start_angle, p.end_angle, lw, color);
    let s = p.start_angle.to_radians();
    let e = p.end_angle.to_radians();
    let p1 = (cx + r * s.cos(), cy - r * s.sin());
    let p2 = (cx + r * e.cos(), cy - r * e.sin());
    ctx.line(cx, cy, p1.0, p1.1, lw, color);
    ctx.line(cx, cy, p2.0, p2.1, lw, color);
    if p.is_filled && !p.is_transparent && p.fill_color != 0 {
        // Approximate fill as a polygon of the pie sector.
        const STEPS: u32 = 32;
        let sweep = (p.end_angle - p.start_angle) / STEPS as f64;
        let mut pts = vec![(cx, cy)];
        for i in 0..=STEPS {
            let a = (p.start_angle + sweep * i as f64).to_radians();
            pts.push((cx + r * a.cos(), cy - r * a.sin()));
        }
        ctx.polygon(&pts, 0.0, None, Some(Color::from_altium_bgr(p.fill_color)));
    }
}

fn render_ellipse<C: RenderContext>(ctx: &mut C, t: &CoordTransform, e: &Ellipse) {
    let (cx, cy) = t.world_to_screen(e.center);
    let rx = t.scale_value(e.radius_x);
    let ry = t.scale_value(e.radius_y);
    let color = if e.color != 0 {
        Color::from_altium_bgr(e.color)
    } else {
        Color::BLACK
    };
    let fill = if e.is_filled && !e.is_transparent {
        Some(Color::from_altium_bgr(e.fill_color))
    } else {
        None
    };
    ctx.ellipse(
        cx,
        cy,
        rx,
        ry,
        line_width_px(e.line_width),
        Some(color),
        fill,
    );
}

fn render_polygon<C: RenderContext>(ctx: &mut C, t: &CoordTransform, p: &Polygon) {
    let pts: Vec<_> = p.vertices.iter().map(|v| t.world_to_screen(*v)).collect();
    if pts.is_empty() {
        return;
    }
    let stroke = if p.color != 0 {
        Some(Color::from_altium_bgr(p.color))
    } else {
        Some(Color::BLACK)
    };
    let fill = if p.is_filled && !p.is_transparent {
        Some(Color::from_altium_bgr(p.fill_color))
    } else {
        None
    };
    ctx.polygon(&pts, line_width_px(p.line_width), stroke, fill);
}

fn render_polyline<C: RenderContext>(ctx: &mut C, t: &CoordTransform, p: &Polyline) {
    let pts: Vec<_> = p.vertices.iter().map(|v| t.world_to_screen(*v)).collect();
    if pts.len() < 2 {
        return;
    }
    let color = if p.color != 0 {
        Color::from_altium_bgr(p.color)
    } else {
        Color::BLACK
    };
    let lw = line_width_px(p.line_width);
    ctx.polyline(&pts, lw, color);
    // End shapes — port of `RenderLineEndShape` (C# L431-472).
    let shape_size = (p.line_shape_size as f64 * lw).max(3.0);
    if pts.len() >= 2 && p.start_line_shape != 0 {
        render_end_shape(ctx, pts[0], pts[1], p.start_line_shape, shape_size, color);
    }
    if pts.len() >= 2 && p.end_line_shape != 0 {
        let n = pts.len();
        render_end_shape(
            ctx,
            pts[n - 1],
            pts[n - 2],
            p.end_line_shape,
            shape_size,
            color,
        );
    }
}

fn render_end_shape<C: RenderContext>(
    ctx: &mut C,
    tip: (f64, f64),
    prev: (f64, f64),
    shape: i32,
    size: f64,
    color: Color,
) {
    let (mut dx, mut dy) = (tip.0 - prev.0, tip.1 - prev.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return;
    }
    dx /= len;
    dy /= len;
    let (px, py) = (-dy, dx);
    match shape {
        1 => {
            // Open-V arrow.
            let p1 = (
                tip.0 - dx * size + px * size * 0.5,
                tip.1 - dy * size + py * size * 0.5,
            );
            let p2 = (
                tip.0 - dx * size - px * size * 0.5,
                tip.1 - dy * size - py * size * 0.5,
            );
            ctx.line(tip.0, tip.1, p1.0, p1.1, 1.0, color);
            ctx.line(tip.0, tip.1, p2.0, p2.1, 1.0, color);
        }
        2 => {
            // Filled triangle.
            let pts = [
                tip,
                (
                    tip.0 - dx * size + px * size * 0.5,
                    tip.1 - dy * size + py * size * 0.5,
                ),
                (
                    tip.0 - dx * size - px * size * 0.5,
                    tip.1 - dy * size - py * size * 0.5,
                ),
            ];
            ctx.polygon(&pts, 0.0, None, Some(color));
        }
        3 => {
            // Tail (perpendicular bar).
            ctx.line(
                tip.0 - px * size * 0.5,
                tip.1 - py * size * 0.5,
                tip.0 + px * size * 0.5,
                tip.1 + py * size * 0.5,
                1.0,
                color,
            );
        }
        4 => {
            ctx.line(
                tip.0 - px * size * 0.5,
                tip.1 - py * size * 0.5,
                tip.0 + px * size * 0.5,
                tip.1 + py * size * 0.5,
                2.0,
                color,
            );
        }
        5 => ctx.circle(tip.0, tip.1, size * 0.4, 1.0, Some(color), None),
        6 => ctx.rectangle(
            tip.0 - size * 0.3,
            tip.1 - size * 0.3,
            size * 0.6,
            size * 0.6,
            1.0,
            Some(color),
            None,
        ),
        _ => {}
    }
}

fn render_bezier<C: RenderContext>(ctx: &mut C, t: &CoordTransform, b: &Bezier) {
    if b.control_points.len() < 4 {
        return;
    }
    let color = if b.color != 0 {
        Color::from_altium_bgr(b.color)
    } else {
        Color::BLACK
    };
    let lw = line_width_px(b.line_width);
    let mut i = 0;
    while i + 3 < b.control_points.len() {
        let p0 = t.world_to_screen(b.control_points[i]);
        let p1 = t.world_to_screen(b.control_points[i + 1]);
        let p2 = t.world_to_screen(b.control_points[i + 2]);
        let p3 = t.world_to_screen(b.control_points[i + 3]);
        ctx.cubic_bezier(p0, p1, p2, p3, lw, color);
        i += 3;
    }
}

fn render_wire<C: RenderContext>(ctx: &mut C, t: &CoordTransform, w: &Wire) {
    if w.vertices.len() < 2 {
        return;
    }
    let pts: Vec<_> = w.vertices.iter().map(|v| t.world_to_screen(*v)).collect();
    let color = argb_or_default(w.color, DEFAULT_WIRE);
    ctx.polyline(&pts, line_width_px(w.line_width), color);
}

fn render_bus<C: RenderContext>(ctx: &mut C, t: &CoordTransform, b: &Bus) {
    if b.vertices.len() < 2 {
        return;
    }
    let pts: Vec<_> = b.vertices.iter().map(|v| t.world_to_screen(*v)).collect();
    let color = argb_or_default(b.color, DEFAULT_BUS);
    ctx.polyline(&pts, line_width_px(b.line_width).max(2.0), color);
}

fn render_bus_entry<C: RenderContext>(ctx: &mut C, t: &CoordTransform, e: &BusEntry) {
    let (x1, y1) = t.world_to_screen(e.location);
    let (x2, y2) = t.world_to_screen(e.corner);
    ctx.line(
        x1,
        y1,
        x2,
        y2,
        line_width_px(e.line_width).max(2.0),
        argb_or_default(e.color, DEFAULT_BUS),
    );
}

fn render_junction<C: RenderContext>(ctx: &mut C, t: &CoordTransform, j: &Junction) {
    let (cx, cy) = t.world_to_screen(j.location);
    let mut size = t.scale_value(j.size);
    if size < 2.0 {
        size = 2.0;
    }
    let color = argb_or_default(j.color, DEFAULT_JUNCTION);
    ctx.circle(cx, cy, size / 2.0, 0.0, None, Some(color));
}

fn render_label<C: RenderContext>(
    ctx: &mut C,
    t: &CoordTransform,
    l: &Label,
    component: Option<&Component>,
) {
    let (x, y) = t.world_to_screen(l.location);
    let color = argb_or_default(l.color, Color::BLACK);
    let (h, v) = justification_anchors(l.justification);
    let style = TextStyle {
        h_anchor: h,
        v_anchor: v,
        rotation: l.rotation,
        ..TextStyle::default()
    };
    let resolved = resolve_string(&l.text, component);
    let font_size = DEFAULT_FONT_SIZE;
    if l.is_mirrored || l.rotation != 0.0 {
        ctx.save_state();
        ctx.translate(x, y);
        if l.rotation != 0.0 {
            ctx.rotate(l.rotation);
        }
        if l.is_mirrored {
            ctx.scale(-1.0, 1.0);
        }
        ctx.text_with_overline(0.0, 0.0, font_size, &resolved, color, style);
        ctx.restore_state();
    } else {
        ctx.text_with_overline(x, y, font_size, &resolved, color, style);
    }
}

fn render_net_label<C: RenderContext>(ctx: &mut C, t: &CoordTransform, nl: &NetLabel) {
    let (x, y) = t.world_to_screen(nl.location);
    let color = argb_or_default(nl.color, Color::BLACK);
    let (h, v) = justification_anchors(nl.justification);
    let rotation = nl.orientation as f64 * 90.0;
    let style = TextStyle {
        h_anchor: h,
        v_anchor: v,
        rotation,
        ..TextStyle::default()
    };
    if rotation != 0.0 {
        ctx.save_state();
        ctx.translate(x, y);
        ctx.rotate(rotation);
        ctx.text_with_overline(0.0, 0.0, DEFAULT_FONT_SIZE, &nl.text, color, style);
        ctx.restore_state();
    } else {
        ctx.text_with_overline(x, y, DEFAULT_FONT_SIZE, &nl.text, color, style);
    }
}

fn render_parameter<C: RenderContext>(
    ctx: &mut C,
    t: &CoordTransform,
    p: &Parameter,
    component: Option<&Component>,
) {
    if !p.is_visible {
        return;
    }
    let (x, y) = t.world_to_screen(p.location);
    let color = argb_or_default(p.color, Color::BLACK);
    let (h, v) = justification_anchors(p.justification);
    let rotation = p.orientation as f64 * 90.0;
    let style = TextStyle {
        h_anchor: h,
        v_anchor: v,
        rotation,
        ..TextStyle::default()
    };
    let text = resolve_string(&p.value, component);
    if p.is_mirrored || rotation != 0.0 {
        ctx.save_state();
        ctx.translate(x, y);
        if rotation != 0.0 {
            ctx.rotate(rotation);
        }
        if p.is_mirrored {
            ctx.scale(-1.0, 1.0);
        }
        ctx.text_styled(0.0, 0.0, DEFAULT_FONT_SIZE, &text, color, style);
        ctx.restore_state();
    } else {
        ctx.text_styled(x, y, DEFAULT_FONT_SIZE, &text, color, style);
    }
}

fn render_text_frame<C: RenderContext>(ctx: &mut C, t: &CoordTransform, tf: &TextFrame) {
    let (x1, y1) = t.world_to_screen(tf.corner1);
    let (x2, y2) = t.world_to_screen(tf.corner2);
    let x = x1.min(x2);
    let y = y1.min(y2);
    let w = (x2 - x1).abs();
    let h = (y2 - y1).abs();

    if tf.is_filled && !tf.is_transparent && tf.fill_color != 0 {
        ctx.rectangle(
            x,
            y,
            w,
            h,
            0.0,
            None,
            Some(Color::from_altium_bgr(tf.fill_color)),
        );
    }
    if tf.show_border && tf.border_color != 0 {
        ctx.rectangle(
            x,
            y,
            w,
            h,
            line_width_px(tf.line_width),
            Some(Color::from_altium_bgr(tf.border_color)),
            None,
        );
    }
    if tf.text.is_empty() {
        return;
    }
    // Word wrap: split paragraphs, then break long lines naively. We don't have
    // a measurement primitive, so we approximate width by char-count × size×0.6.
    let padding = 2.0;
    let avail_w = (w - 2.0 * padding).max(8.0);
    let lines = if tf.word_wrap {
        wrap_text(&tf.text, DEFAULT_FONT_SIZE, avail_w)
    } else {
        tf.text.lines().map(|l| l.to_string()).collect()
    };
    let line_height = DEFAULT_FONT_SIZE * 1.2;
    let total_h = line_height * lines.len() as f64;
    let (start_x, h_anchor) = (x + padding, TextAnchorH::Left);
    let mut start_y = y + padding;
    let _ = total_h;
    let color = argb_or_default(tf.text_color, Color::BLACK);
    for line in &lines {
        ctx.text_styled(
            start_x,
            start_y,
            DEFAULT_FONT_SIZE,
            line,
            color,
            TextStyle {
                h_anchor,
                v_anchor: TextAnchorV::Top,
                ..TextStyle::default()
            },
        );
        start_y += line_height;
    }
}

fn wrap_text(text: &str, size: f64, max_width: f64) -> Vec<String> {
    let approx_advance = size * 0.6;
    let mut out = Vec::new();
    for para in text.split(['\r', '\n']) {
        if para.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in para.split_whitespace() {
            let test_len =
                (current.chars().count() + 1 + word.chars().count()) as f64 * approx_advance;
            if test_len > max_width && !current.is_empty() {
                out.push(std::mem::take(&mut current));
                current.push_str(word);
            } else {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out
}

fn render_image<C: RenderContext>(ctx: &mut C, t: &CoordTransform, image: &Image) {
    let (x1, y1) = t.world_to_screen(image.corner1);
    let (x2, y2) = t.world_to_screen(image.corner2);
    let x = x1.min(x2);
    let y = y1.min(y2);
    let w = (x2 - x1).abs();
    let h = (y2 - y1).abs();

    // Embedded image bytes: hand off to the backend (SVG embeds via base64,
    // raster decodes inline if a decoder is wired up).
    if let Some(bytes) = &image.image_data {
        if !bytes.is_empty() {
            ctx.image(bytes, x, y, w, h);
            if image.show_border {
                let border = Color::from_altium_bgr(image.border_color);
                ctx.rectangle(
                    x,
                    y,
                    w,
                    h,
                    line_width_px(image.line_width),
                    Some(border),
                    None,
                );
            }
            return;
        }
    }

    // Placeholder: framed rectangle with X-cross.
    let frame = Color::rgb(0x80, 0x80, 0x80);
    ctx.rectangle(x, y, w, h, 1.0, Some(frame), None);
    ctx.line(x, y, x + w, y + h, 1.0, frame);
    ctx.line(x + w, y, x, y + h, 1.0, frame);
    if image.show_border {
        let border = Color::from_altium_bgr(image.border_color);
        ctx.rectangle(
            x,
            y,
            w,
            h,
            line_width_px(image.line_width),
            Some(border),
            None,
        );
    }
}

fn render_port<C: RenderContext>(ctx: &mut C, t: &CoordTransform, port: &Port) {
    let (x, y) = t.world_to_screen(port.location);
    let w = t.scale_value(port.width).max(40.0);
    let h = t.scale_value(port.height).max(20.0);
    let stroke = argb_or_default(port.color, Color::BLACK);
    let fill = if port.area_color != 0 {
        Some(Color::from_altium_bgr(port.area_color))
    } else {
        None
    };
    // Simplified port: rectangle with the name centered.
    ctx.rectangle(x - w / 2.0, y - h / 2.0, w, h, 1.0, Some(stroke), fill);
    if !port.name.is_empty() {
        ctx.text_styled(
            x,
            y,
            DEFAULT_FONT_SIZE,
            &port.name,
            argb_or_default(port.text_color, Color::BLACK),
            TextStyle {
                h_anchor: TextAnchorH::Center,
                v_anchor: TextAnchorV::Middle,
                ..TextStyle::default()
            },
        );
    }
}

fn render_sheet_symbol<C: RenderContext>(ctx: &mut C, t: &CoordTransform, ss: &SheetSymbol) {
    let (x, y) = t.world_to_screen(ss.location);
    let w = t.scale_value(ss.x_size);
    let h = t.scale_value(ss.y_size);
    let stroke = argb_or_default(ss.color, Color::BLACK);
    let fill = if ss.is_solid && ss.area_color != 0 {
        Some(Color::from_altium_bgr(ss.area_color))
    } else {
        None
    };
    ctx.rectangle(x, y, w, h, line_width_px(ss.line_width), Some(stroke), fill);
    if let Some(name) = &ss.sheet_name {
        ctx.text_styled(
            x,
            y - 4.0,
            DEFAULT_FONT_SIZE,
            name,
            stroke,
            TextStyle {
                h_anchor: TextAnchorH::Left,
                v_anchor: TextAnchorV::Bottom,
                ..TextStyle::default()
            },
        );
    }
    if let Some(file) = &ss.file_name {
        ctx.text_styled(
            x,
            y + h + 4.0,
            DEFAULT_FONT_SIZE,
            file,
            stroke,
            TextStyle {
                h_anchor: TextAnchorH::Left,
                v_anchor: TextAnchorV::Top,
                ..TextStyle::default()
            },
        );
    }

    // Render entries on the rectangle's edges. Side: 0=Left, 1=Right, 2=Top,
    // 3=Bottom. DistanceFromTop/DistanceFromLeft positions the entry along the
    // appropriate edge.
    for entry in &ss.entries {
        let dist = t.scale_value(entry.distance_from_top).max(0.0);
        let entry_color = argb_or_default(entry.color, stroke);
        let (ex, ey, h_anchor) = match entry.side {
            0 => (x, y + dist, TextAnchorH::Left),
            1 => (x + w, y + dist, TextAnchorH::Right),
            2 => (x + dist, y, TextAnchorH::Center),
            _ => (x + dist, y + h, TextAnchorH::Center),
        };
        // Small tick to mark the entry pin position.
        let tick = 4.0;
        match entry.side {
            0 => ctx.line(ex - tick, ey, ex, ey, 1.0, entry_color),
            1 => ctx.line(ex, ey, ex + tick, ey, 1.0, entry_color),
            2 => ctx.line(ex, ey - tick, ex, ey, 1.0, entry_color),
            _ => ctx.line(ex, ey, ex, ey + tick, 1.0, entry_color),
        }
        if !entry.name.is_empty() {
            // Push the label inside the symbol body by a small offset.
            let (label_x, label_y) = match entry.side {
                0 => (ex + tick + 2.0, ey),
                1 => (ex - tick - 2.0, ey),
                2 => (ex, ey + tick + 2.0),
                _ => (ex, ey - tick - 2.0),
            };
            ctx.text_styled(
                label_x,
                label_y,
                DEFAULT_FONT_SIZE * 0.85,
                &entry.name,
                argb_or_default(entry.text_color, entry_color),
                TextStyle {
                    h_anchor,
                    v_anchor: TextAnchorV::Middle,
                    ..TextStyle::default()
                },
            );
        }
    }
}

fn render_pin<C: RenderContext>(ctx: &mut C, t: &CoordTransform, pin: &Pin) {
    if pin.is_hidden {
        return;
    }
    let (sx, sy) = t.world_to_screen(pin.location);
    let pin_len = t.scale_value(pin.length);
    let color = argb_or_default(pin.color, DEFAULT_PIN);

    let (ex, ey) = match pin.orientation {
        PinOrientation::Right => (sx + pin_len, sy),
        PinOrientation::Left => (sx - pin_len, sy),
        PinOrientation::Up => (sx, sy - pin_len),
        PinOrientation::Down => (sx, sy + pin_len),
    };
    ctx.line(sx, sy, ex, ey, DEFAULT_LINE, color);
    render_pin_electrical(ctx, t, pin.electrical_type, sx, sy, pin.orientation, color);

    // Pin name (at body end).
    if pin.show_name {
        if let Some(name) = &pin.name {
            let offset = 3.0;
            let (nx, ny, h) = match pin.orientation {
                PinOrientation::Right => (ex + offset, ey, TextAnchorH::Left),
                PinOrientation::Left => (ex - offset, ey, TextAnchorH::Right),
                PinOrientation::Up => (ex, ey - offset, TextAnchorH::Center),
                PinOrientation::Down => (ex, ey + offset, TextAnchorH::Center),
            };
            ctx.text_with_overline(
                nx,
                ny,
                DEFAULT_FONT_SIZE,
                name,
                color,
                TextStyle {
                    h_anchor: h,
                    v_anchor: TextAnchorV::Middle,
                    ..TextStyle::default()
                },
            );
        }
    }
    // Pin designator.
    if pin.show_designator {
        if let Some(d) = &pin.designator {
            let font = DEFAULT_FONT_SIZE * 0.8;
            let offset = 3.0;
            let (dx, dy, h, v) = match pin.orientation {
                PinOrientation::Right | PinOrientation::Left => (
                    (sx + ex) / 2.0,
                    sy - offset,
                    TextAnchorH::Center,
                    TextAnchorV::Bottom,
                ),
                _ => (
                    sx + offset,
                    (sy + ey) / 2.0,
                    TextAnchorH::Left,
                    TextAnchorV::Middle,
                ),
            };
            ctx.text_styled(
                dx,
                dy,
                font,
                d,
                color,
                TextStyle {
                    h_anchor: h,
                    v_anchor: v,
                    ..TextStyle::default()
                },
            );
        }
    }
}

fn render_pin_electrical<C: RenderContext>(
    ctx: &mut C,
    t: &CoordTransform,
    electrical: PinElectricalType,
    x: f64,
    y: f64,
    orientation: PinOrientation,
    color: Color,
) {
    use PinElectricalType as E;
    if !matches!(electrical, E::Input | E::Output | E::InputOutput) {
        return;
    }
    let arrow_w = t.scale_value(Coord::from_mils(60.0));
    let arrow_h = t.scale_value(Coord::from_mils(20.0));
    let arrow_gap = t.scale_value(Coord::from_mils(70.0));

    let dir = match orientation {
        PinOrientation::Right => 1.0,
        PinOrientation::Left => -1.0,
        _ => 1.0,
    };
    let vdir = match orientation {
        PinOrientation::Up => -1.0,
        PinOrientation::Down => 1.0,
        _ => 1.0,
    };
    let is_vertical = matches!(orientation, PinOrientation::Up | PinOrientation::Down);

    if matches!(electrical, E::Input | E::InputOutput) {
        let pts = if !is_vertical {
            [
                (x, y),
                (x + arrow_w * dir, y - arrow_h),
                (x + arrow_w * dir, y + arrow_h),
            ]
        } else {
            [
                (x, y),
                (x - arrow_h, y + arrow_w * vdir),
                (x + arrow_h, y + arrow_w * vdir),
            ]
        };
        ctx.polygon(&pts, 1.0, Some(color), Some(Color::WHITE));
    }
    if matches!(electrical, E::Output | E::InputOutput) {
        let offset = if matches!(electrical, E::InputOutput) {
            arrow_gap
        } else {
            0.0
        };
        let pts = if !is_vertical {
            let bx = x + offset * dir;
            [
                (bx, y),
                (bx - arrow_w * dir, y - arrow_h),
                (bx - arrow_w * dir, y + arrow_h),
            ]
        } else {
            let by = y + offset * vdir;
            [
                (x, by),
                (x - arrow_h, by - arrow_w * vdir),
                (x + arrow_h, by - arrow_w * vdir),
            ]
        };
        ctx.polygon(&pts, 1.0, Some(color), Some(Color::WHITE));
    }
}

fn render_power_object<C: RenderContext>(
    ctx: &mut C,
    t: &CoordTransform,
    po: &PowerObject,
    component: Option<&Component>,
) {
    let (sx, sy) = t.world_to_screen(po.location);
    let color = argb_or_default(po.color, Color::BLACK);

    ctx.save_state();
    ctx.translate(sx, sy);
    if po.rotation != 0.0 {
        ctx.rotate(po.rotation);
    }
    if po.is_mirrored {
        ctx.scale(-1.0, 1.0);
    }
    const PIN_LEN: f64 = 10.0;
    ctx.line(0.0, 0.0, 0.0, -PIN_LEN, DEFAULT_LINE, color);
    render_power_port_symbol(ctx, po.style, -PIN_LEN, color);
    ctx.restore_state();

    if po.show_net_name && !po.text.is_empty() {
        let net = resolve_string(&po.text, component);
        ctx.save_state();
        ctx.translate(sx, sy);
        if po.rotation != 0.0 {
            ctx.rotate(po.rotation);
        }
        if po.is_mirrored {
            ctx.scale(-1.0, 1.0);
        }
        ctx.text_styled(
            0.0,
            -PIN_LEN - 12.0,
            DEFAULT_FONT_SIZE,
            &net,
            color,
            TextStyle {
                h_anchor: TextAnchorH::Center,
                v_anchor: TextAnchorV::Bottom,
                ..TextStyle::default()
            },
        );
        ctx.restore_state();
    }
}

fn render_power_port_symbol<C: RenderContext>(
    ctx: &mut C,
    style: PowerPortStyle,
    y: f64,
    color: Color,
) {
    use PowerPortStyle as S;
    const S_SIZE: f64 = 8.0;
    let s = S_SIZE;
    match style {
        S::Circle => ctx.circle(0.0, y - s / 2.0, s / 2.0, 1.0, Some(color), None),
        S::Arrow => {
            ctx.line(-s * 0.4, y, 0.0, y - s, 1.0, color);
            ctx.line(s * 0.4, y, 0.0, y - s, 1.0, color);
        }
        S::Bar => {
            ctx.line(-s, y, s, y, 1.0, color);
        }
        S::Wave => {
            for i in 0..8 {
                let x1 = -s + i as f64 * s / 4.0;
                let x2 = -s + (i + 1) as f64 * s / 4.0;
                let y1 = y + (i as f64 * std::f64::consts::PI / 2.0).sin() * s * 0.3;
                let y2 = y + ((i + 1) as f64 * std::f64::consts::PI / 2.0).sin() * s * 0.3;
                ctx.line(x1, y1, x2, y2, 1.0, color);
            }
        }
        S::PowerGround => {
            ctx.line(-s, y, s, y, 1.0, color);
            ctx.line(-s * 0.6, y - s * 0.3, s * 0.6, y - s * 0.3, 1.0, color);
            ctx.line(-s * 0.2, y - s * 0.6, s * 0.2, y - s * 0.6, 1.0, color);
        }
        S::SignalGround => {
            ctx.line(-s, y, s, y, 1.0, color);
            ctx.line(-s, y, 0.0, y - s, 1.0, color);
            ctx.line(s, y, 0.0, y - s, 1.0, color);
        }
        S::Earth => {
            ctx.line(-s, y, s, y, 1.0, color);
            ctx.line(-s * 0.8, y, -s * 0.4, y - s * 0.5, 1.0, color);
            ctx.line(-s * 0.2, y, s * 0.2, y - s * 0.5, 1.0, color);
            ctx.line(s * 0.4, y, s * 0.8, y - s * 0.5, 1.0, color);
        }
        S::GostArrow => {
            ctx.line(0.0, y, 0.0, y - s, 2.0, color);
            ctx.line(-s * 0.3, y - s * 0.6, 0.0, y - s, 2.0, color);
            ctx.line(s * 0.3, y - s * 0.6, 0.0, y - s, 2.0, color);
        }
        S::GostPowerGround => {
            ctx.line(-s, y, s, y, 2.0, color);
            ctx.line(-s * 0.6, y - s * 0.25, s * 0.6, y - s * 0.25, 2.0, color);
            ctx.line(-s * 0.3, y - s * 0.5, s * 0.3, y - s * 0.5, 2.0, color);
        }
        S::GostEarth => {
            ctx.line(-s, y, s, y, 2.0, color);
            for i in -2..=2 {
                let xb = i as f64 * s * 0.4;
                ctx.line(xb, y, xb - s * 0.2, y - s * 0.4, 1.0, color);
            }
        }
        S::GostBar => {
            ctx.line(-s * 1.2, y, s * 1.2, y, 2.0, color);
        }
    }
}

// Entry points

pub fn render_component<C: RenderContext>(ctx: &mut C, component: &Component, t: &CoordTransform) {
    let comp = Some(component);
    // Z-order: shapes/fills first, then lines/arcs, then pins/labels/parameters.
    for r in &component.rectangles {
        render_rectangle(ctx, t, r);
    }
    for r in &component.rounded_rectangles {
        render_rounded_rect(ctx, t, r);
    }
    for poly in &component.polygons {
        render_polygon(ctx, t, poly);
    }
    for e in &component.ellipses {
        render_ellipse(ctx, t, e);
    }
    for ea in &component.elliptical_arcs {
        render_elliptical_arc(ctx, t, ea);
    }
    for pie in &component.pies {
        render_pie(ctx, t, pie);
    }
    for tf in &component.text_frames {
        render_text_frame(ctx, t, tf);
    }
    for line in &component.lines {
        render_line(ctx, t, line);
    }
    for arc in &component.arcs {
        render_arc(ctx, t, arc);
    }
    for poly in &component.polylines {
        render_polyline(ctx, t, poly);
    }
    for bz in &component.beziers {
        render_bezier(ctx, t, bz);
    }
    for w in &component.wires {
        render_wire(ctx, t, w);
    }
    for j in &component.junctions {
        render_junction(ctx, t, j);
    }
    for img in &component.images {
        render_image(ctx, t, img);
    }
    for pin in &component.pins {
        render_pin(ctx, t, pin);
    }
    for label in &component.labels {
        render_label(ctx, t, label, comp);
    }
    for p in &component.parameters {
        render_parameter(ctx, t, p, comp);
    }
    for nl in &component.net_labels {
        render_net_label(ctx, t, nl);
    }
    for po in &component.power_objects {
        render_power_object(ctx, t, po, comp);
    }
}

pub fn render_document<C: RenderContext>(ctx: &mut C, document: &Document, t: &CoordTransform) {
    // Shapes & shapes-with-fill first.
    for r in &document.rectangles {
        render_rectangle(ctx, t, r);
    }
    for r in &document.rounded_rectangles {
        render_rounded_rect(ctx, t, r);
    }
    for poly in &document.polygons {
        render_polygon(ctx, t, poly);
    }
    for e in &document.ellipses {
        render_ellipse(ctx, t, e);
    }
    for ea in &document.elliptical_arcs {
        render_elliptical_arc(ctx, t, ea);
    }
    for pie in &document.pies {
        render_pie(ctx, t, pie);
    }
    for tf in &document.text_frames {
        render_text_frame(ctx, t, tf);
    }
    for img in &document.images {
        render_image(ctx, t, img);
    }
    // Components.
    for c in &document.components {
        render_component(ctx, c, t);
    }
    // Lines/arcs/wires/buses connecting components.
    for line in &document.lines {
        render_line(ctx, t, line);
    }
    for arc in &document.arcs {
        render_arc(ctx, t, arc);
    }
    for poly in &document.polylines {
        render_polyline(ctx, t, poly);
    }
    for bz in &document.beziers {
        render_bezier(ctx, t, bz);
    }
    for bus in &document.buses {
        render_bus(ctx, t, bus);
    }
    for be in &document.bus_entries {
        render_bus_entry(ctx, t, be);
    }
    for w in &document.wires {
        render_wire(ctx, t, w);
    }
    for j in &document.junctions {
        render_junction(ctx, t, j);
    }
    // Sheet structures, ports.
    for ss in &document.sheet_symbols {
        render_sheet_symbol(ctx, t, ss);
    }
    for p in &document.ports {
        render_port(ctx, t, p);
    }
    // Text on top.
    for label in &document.labels {
        render_label(ctx, t, label, None);
    }
    for p in &document.parameters {
        render_parameter(ctx, t, p, None);
    }
    for nl in &document.net_labels {
        render_net_label(ctx, t, nl);
    }
    for po in &document.power_objects {
        render_power_object(ctx, t, po, None);
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
    pub fn render_svg(&self, options: RenderOptions) -> String {
        let bounds = self.bounds();
        let t = CoordTransform::fit(bounds, options.width, options.height, options.padding);
        let mut ctx = SvgContext::new(options);
        render_document(&mut ctx, self, &t);
        ctx.finish()
    }

    pub fn render_png(&self, options: RenderOptions) -> Result<Vec<u8>, String> {
        let bounds = self.bounds();
        let t = CoordTransform::fit(bounds, options.width, options.height, options.padding);
        let mut ctx = super::raster::TinySkiaContext::new(options);
        render_document(&mut ctx, self, &t);
        ctx.encode_png()
    }
}
