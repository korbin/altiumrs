//! tiny-skia-backed raster context with cosmic-text-driven text rendering.

use std::sync::OnceLock;

use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Stretch, Style as CtStyle, SwashCache,
    Weight,
};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

use super::context::{RenderContext, RenderOptions, TextAnchorH, TextAnchorV, TextStyle};
use crate::color::Color;

pub struct TinySkiaContext {
    pixmap: Pixmap,
    /// Current transform stack. The active transform is the last entry.
    stack: Vec<Transform>,
}

impl TinySkiaContext {
    pub fn new(options: RenderOptions) -> Self {
        let mut pixmap = Pixmap::new(options.width, options.height).expect("non-zero size");
        if let Some(bg) = options.background {
            pixmap.fill(to_skia(bg));
        }
        Self {
            pixmap,
            stack: vec![Transform::identity()],
        }
    }

    /// Encode the canvas as PNG bytes.
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        self.pixmap
            .encode_png()
            .map_err(|e| format!("png encode: {e}"))
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    fn xform(&self) -> Transform {
        *self.stack.last().expect("transform stack underflow")
    }

    fn xform_mut(&mut self) -> &mut Transform {
        self.stack.last_mut().expect("transform stack underflow")
    }
}

fn to_skia(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn fill_paint(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(to_skia(c));
    p.anti_alias = true;
    p
}

fn stroke(width: f64) -> Stroke {
    Stroke {
        width: width.max(0.1) as f32,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    }
}

// Text rendering via cosmic-text

/// Lazy global font system. cosmic-text loads system fonts on first access;
/// after that subsequent renders are cheap.
fn font_system() -> &'static std::sync::Mutex<FontSystem> {
    static FS: OnceLock<std::sync::Mutex<FontSystem>> = OnceLock::new();
    FS.get_or_init(|| std::sync::Mutex::new(FontSystem::new()))
}

fn swash_cache() -> &'static std::sync::Mutex<SwashCache> {
    static SC: OnceLock<std::sync::Mutex<SwashCache>> = OnceLock::new();
    SC.get_or_init(|| std::sync::Mutex::new(SwashCache::new()))
}

impl RenderContext for TinySkiaContext {
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, width: f64, color: Color) {
        let mut pb = PathBuilder::new();
        pb.move_to(x1 as f32, y1 as f32);
        pb.line_to(x2 as f32, y2 as f32);
        let Some(path) = pb.finish() else { return };
        let xform = self.xform();
        self.pixmap
            .stroke_path(&path, &fill_paint(color), &stroke(width), xform, None);
    }

    fn rectangle(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        stroke_width: f64,
        stroke_color: Option<Color>,
        fill: Option<Color>,
    ) {
        let mut pb = PathBuilder::new();
        pb.push_rect(
            tiny_skia::Rect::from_xywh(x as f32, y as f32, w as f32, h as f32)
                .unwrap_or(tiny_skia::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap()),
        );
        let Some(path) = pb.finish() else { return };
        let xform = self.xform();
        if let Some(c) = fill {
            self.pixmap
                .fill_path(&path, &fill_paint(c), FillRule::Winding, xform, None);
        }
        if let Some(c) = stroke_color {
            self.pixmap
                .stroke_path(&path, &fill_paint(c), &stroke(stroke_width), xform, None);
        }
    }

    fn rounded_rectangle(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        rx: f64,
        ry: f64,
        stroke_width: f64,
        stroke_color: Option<Color>,
        fill: Option<Color>,
    ) {
        let rx = rx.min(w * 0.5).max(0.0) as f32;
        let ry = ry.min(h * 0.5).max(0.0) as f32;
        if rx <= 0.0 || ry <= 0.0 {
            return self.rectangle(x, y, w, h, stroke_width, stroke_color, fill);
        }
        let mut pb = PathBuilder::new();
        let x = x as f32;
        let y = y as f32;
        let w = w as f32;
        let h = h as f32;
        // Approximate the rounded corners with elliptical arcs (4-segment SVG-style).
        pb.move_to(x + rx, y);
        pb.line_to(x + w - rx, y);
        pb.quad_to(x + w, y, x + w, y + ry);
        pb.line_to(x + w, y + h - ry);
        pb.quad_to(x + w, y + h, x + w - rx, y + h);
        pb.line_to(x + rx, y + h);
        pb.quad_to(x, y + h, x, y + h - ry);
        pb.line_to(x, y + ry);
        pb.quad_to(x, y, x + rx, y);
        pb.close();
        let Some(path) = pb.finish() else { return };
        let xform = self.xform();
        if let Some(c) = fill {
            self.pixmap
                .fill_path(&path, &fill_paint(c), FillRule::Winding, xform, None);
        }
        if let Some(c) = stroke_color {
            self.pixmap
                .stroke_path(&path, &fill_paint(c), &stroke(stroke_width), xform, None);
        }
    }

    fn circle(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        stroke_width: f64,
        stroke_color: Option<Color>,
        fill: Option<Color>,
    ) {
        let mut pb = PathBuilder::new();
        pb.push_circle(cx as f32, cy as f32, radius as f32);
        let Some(path) = pb.finish() else { return };
        let xform = self.xform();
        if let Some(c) = fill {
            self.pixmap
                .fill_path(&path, &fill_paint(c), FillRule::Winding, xform, None);
        }
        if let Some(c) = stroke_color {
            self.pixmap
                .stroke_path(&path, &fill_paint(c), &stroke(stroke_width), xform, None);
        }
    }

    fn ellipse(
        &mut self,
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        stroke_width: f64,
        stroke_color: Option<Color>,
        fill: Option<Color>,
    ) {
        // Approximate the ellipse with a polyline.
        const STEPS: usize = 64;
        let mut pb = PathBuilder::new();
        for i in 0..=STEPS {
            let t = (i as f64 / STEPS as f64) * std::f64::consts::TAU;
            let x = cx + rx * t.cos();
            let y = cy - ry * t.sin();
            if i == 0 {
                pb.move_to(x as f32, y as f32);
            } else {
                pb.line_to(x as f32, y as f32);
            }
        }
        pb.close();
        let Some(path) = pb.finish() else { return };
        let xform = self.xform();
        if let Some(c) = fill {
            self.pixmap
                .fill_path(&path, &fill_paint(c), FillRule::Winding, xform, None);
        }
        if let Some(c) = stroke_color {
            self.pixmap
                .stroke_path(&path, &fill_paint(c), &stroke(stroke_width), xform, None);
        }
    }

    fn arc(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        stroke_width: f64,
        color: Color,
    ) {
        const STEP: f64 = 5.0;
        let mut pb = PathBuilder::new();
        let mut first = true;
        let start = start_angle.min(end_angle);
        let end = start_angle.max(end_angle);
        let mut a = start;
        while a <= end {
            let r = a.to_radians();
            let x = cx + radius * r.cos();
            let y = cy - radius * r.sin();
            if first {
                pb.move_to(x as f32, y as f32);
                first = false;
            } else {
                pb.line_to(x as f32, y as f32);
            }
            a += STEP;
        }
        let r = end.to_radians();
        pb.line_to(
            (cx + radius * r.cos()) as f32,
            (cy - radius * r.sin()) as f32,
        );
        let Some(path) = pb.finish() else { return };
        let xform = self.xform();
        self.pixmap.stroke_path(
            &path,
            &fill_paint(color),
            &stroke(stroke_width),
            xform,
            None,
        );
    }

    fn elliptical_arc(
        &mut self,
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        start_angle: f64,
        end_angle: f64,
        stroke_width: f64,
        color: Color,
    ) {
        const STEP: f64 = 5.0;
        let mut pb = PathBuilder::new();
        let mut first = true;
        let start = start_angle.min(end_angle);
        let end = start_angle.max(end_angle);
        let mut a = start;
        while a <= end {
            let r = a.to_radians();
            let x = cx + rx * r.cos();
            let y = cy - ry * r.sin();
            if first {
                pb.move_to(x as f32, y as f32);
                first = false;
            } else {
                pb.line_to(x as f32, y as f32);
            }
            a += STEP;
        }
        let r = end.to_radians();
        pb.line_to((cx + rx * r.cos()) as f32, (cy - ry * r.sin()) as f32);
        let Some(path) = pb.finish() else { return };
        let xform = self.xform();
        self.pixmap.stroke_path(
            &path,
            &fill_paint(color),
            &stroke(stroke_width),
            xform,
            None,
        );
    }

    fn polyline(&mut self, points: &[(f64, f64)], stroke_width: f64, color: Color) {
        if points.is_empty() {
            return;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(points[0].0 as f32, points[0].1 as f32);
        for (x, y) in &points[1..] {
            pb.line_to(*x as f32, *y as f32);
        }
        let Some(path) = pb.finish() else { return };
        let xform = self.xform();
        self.pixmap.stroke_path(
            &path,
            &fill_paint(color),
            &stroke(stroke_width),
            xform,
            None,
        );
    }

    fn polygon(
        &mut self,
        points: &[(f64, f64)],
        stroke_width: f64,
        stroke_color: Option<Color>,
        fill: Option<Color>,
    ) {
        if points.is_empty() {
            return;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(points[0].0 as f32, points[0].1 as f32);
        for (x, y) in &points[1..] {
            pb.line_to(*x as f32, *y as f32);
        }
        pb.close();
        let Some(path) = pb.finish() else { return };
        let xform = self.xform();
        if let Some(c) = fill {
            self.pixmap
                .fill_path(&path, &fill_paint(c), FillRule::Winding, xform, None);
        }
        if let Some(c) = stroke_color {
            self.pixmap
                .stroke_path(&path, &fill_paint(c), &stroke(stroke_width), xform, None);
        }
    }

    fn cubic_bezier(
        &mut self,
        p0: (f64, f64),
        p1: (f64, f64),
        p2: (f64, f64),
        p3: (f64, f64),
        width: f64,
        color: Color,
    ) {
        let mut pb = PathBuilder::new();
        pb.move_to(p0.0 as f32, p0.1 as f32);
        pb.cubic_to(
            p1.0 as f32,
            p1.1 as f32,
            p2.0 as f32,
            p2.1 as f32,
            p3.0 as f32,
            p3.1 as f32,
        );
        let Some(path) = pb.finish() else { return };
        let xform = self.xform();
        self.pixmap
            .stroke_path(&path, &fill_paint(color), &stroke(width), xform, None);
    }

    fn text_styled(
        &mut self,
        x: f64,
        y: f64,
        size: f64,
        content: &str,
        color: Color,
        style: TextStyle,
    ) {
        if content.is_empty() || size <= 0.0 {
            return;
        }
        let mut fs = match font_system().lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let mut sc = match swash_cache().lock() {
            Ok(g) => g,
            Err(_) => return,
        };

        let metrics = Metrics::new(size as f32, (size * 1.2) as f32);
        let mut buffer = Buffer::new(&mut fs, metrics);
        let attrs = Attrs::new()
            .family(Family::SansSerif)
            .weight(if style.bold {
                Weight::BOLD
            } else {
                Weight::NORMAL
            })
            .style(if style.italic {
                CtStyle::Italic
            } else {
                CtStyle::Normal
            })
            .stretch(Stretch::Normal);
        buffer.set_text(&mut fs, content, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut fs, true);

        // Measure the laid-out text.
        let mut max_x: f32 = 0.0;
        let mut min_y: f32 = f32::INFINITY;
        let mut max_y: f32 = f32::NEG_INFINITY;
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                max_x = max_x.max(glyph.x + glyph.w);
                min_y = min_y.min(run.line_y - run.line_top);
                max_y = max_y.max(run.line_y + (run.line_height - run.line_top));
            }
        }
        let text_w = max_x;
        let text_h = if min_y.is_finite() {
            max_y - min_y
        } else {
            size as f32
        };

        // Compute the origin in the rotated frame.
        let dx = match style.h_anchor {
            TextAnchorH::Left => 0.0,
            TextAnchorH::Center => -text_w * 0.5,
            TextAnchorH::Right => -text_w,
        };
        let dy = match style.v_anchor {
            TextAnchorV::Top => 0.0,
            TextAnchorV::Middle => -text_h * 0.5,
            TextAnchorV::Baseline => -size as f32,
            TextAnchorV::Bottom => -text_h,
        };

        let canvas_w = self.pixmap.width() as f32;
        let canvas_h = self.pixmap.height() as f32;

        // Build the world-space transform: caller-frame translate(x,y) →
        // rotate(-rotation) → glyph-local translate(dx, dy) → blit.
        let local = Transform::from_translate(x as f32, y as f32)
            .pre_rotate(-style.rotation as f32)
            .pre_translate(dx, dy);
        let world = self.xform().pre_concat(local);

        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                if let Some(image) = sc.get_image(&mut fs, physical.cache_key) {
                    let placement = image.placement;
                    let img_w = placement.width as i32;
                    let img_h = placement.height as i32;
                    if img_w == 0 || img_h == 0 || image.data.is_empty() {
                        continue;
                    }
                    // Splat the glyph at integer offsets — we already baked
                    // rotation and anchoring into `world`, so for anti-aliased
                    // upright text we just blit to (gx, gy).
                    let gx = physical.x as f32 + placement.left as f32;
                    let gy = physical.y as f32 - placement.top as f32;
                    blit_glyph(
                        &mut self.pixmap,
                        gx,
                        gy,
                        img_w,
                        img_h,
                        &image.data,
                        match image.content {
                            cosmic_text::SwashContent::Mask => GlyphFormat::Mask,
                            cosmic_text::SwashContent::Color => GlyphFormat::Color,
                            cosmic_text::SwashContent::SubpixelMask => GlyphFormat::Mask,
                        },
                        color,
                        world,
                        canvas_w,
                        canvas_h,
                    );
                }
            }
        }
    }

    fn save_state(&mut self) {
        let cur = self.xform();
        self.stack.push(cur);
    }

    fn restore_state(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    fn translate(&mut self, dx: f64, dy: f64) {
        *self.xform_mut() = self
            .xform_mut()
            .pre_concat(Transform::from_translate(dx as f32, dy as f32));
    }

    fn rotate(&mut self, degrees: f64) {
        // Caller-side convention: ccw positive (math). Skia is cw positive.
        *self.xform_mut() = self
            .xform_mut()
            .pre_concat(Transform::from_rotate(-degrees as f32));
    }

    fn scale(&mut self, sx: f64, sy: f64) {
        *self.xform_mut() = self
            .xform_mut()
            .pre_concat(Transform::from_scale(sx as f32, sy as f32));
    }
}

#[derive(Clone, Copy)]
enum GlyphFormat {
    Mask,
    Color,
}

#[allow(clippy::too_many_arguments)]
fn blit_glyph(
    pixmap: &mut Pixmap,
    gx: f32,
    gy: f32,
    img_w: i32,
    img_h: i32,
    data: &[u8],
    fmt: GlyphFormat,
    color: Color,
    world: Transform,
    canvas_w: f32,
    canvas_h: f32,
) {
    let canvas_w_i = canvas_w as i32;
    let canvas_h_i = canvas_h as i32;
    let stride = pixmap.width() as usize;
    let pixel_data = pixmap.data_mut();

    let mapped = |sx: f32, sy: f32| -> (f32, f32) {
        let mut p = tiny_skia::Point { x: sx, y: sy };
        world.map_point(&mut p);
        (p.x, p.y)
    };

    for j in 0..img_h {
        for i in 0..img_w {
            let (alpha, glyph_color) = match fmt {
                GlyphFormat::Mask => {
                    let idx = (j * img_w + i) as usize;
                    if idx >= data.len() {
                        continue;
                    }
                    (data[idx], color)
                }
                GlyphFormat::Color => {
                    let idx = (j * img_w + i) as usize * 4;
                    if idx + 3 >= data.len() {
                        continue;
                    }
                    let r = data[idx];
                    let g = data[idx + 1];
                    let b = data[idx + 2];
                    let a = data[idx + 3];
                    (a, Color::rgba(r, g, b, color.a))
                }
            };
            if alpha == 0 {
                continue;
            }
            let (px, py) = mapped(gx + i as f32, gy + j as f32);
            let pxi = px.round() as i32;
            let pyi = py.round() as i32;
            if pxi < 0 || pyi < 0 || pxi >= canvas_w_i || pyi >= canvas_h_i {
                continue;
            }
            let dst_idx = (pyi as usize) * stride * 4 + (pxi as usize) * 4;
            if dst_idx + 3 >= pixel_data.len() {
                continue;
            }
            // Source color over dest with glyph alpha.
            let sa = (alpha as u32 * glyph_color.a as u32) / 255;
            if sa == 0 {
                continue;
            }
            let inv = 255 - sa;
            for (k, ch) in [glyph_color.r, glyph_color.g, glyph_color.b]
                .iter()
                .enumerate()
            {
                let src = (*ch as u32 * sa) / 255;
                let dst = pixel_data[dst_idx + k] as u32;
                pixel_data[dst_idx + k] = (src + (dst * inv) / 255) as u8;
            }
            let dst_a = pixel_data[dst_idx + 3] as u32;
            pixel_data[dst_idx + 3] = (sa + (dst_a * inv) / 255) as u8;
        }
    }
}
