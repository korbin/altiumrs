//! Backend-agnostic 2D drawing primitives used by the component renderers.

#![allow(clippy::too_many_arguments)]

use crate::color::Color;

/// Horizontal anchor for a text draw call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAnchorH {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical anchor for a text draw call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAnchorV {
    Top,
    Middle,
    #[default]
    Baseline,
    Bottom,
}

/// Style options for a text draw call.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub h_anchor: TextAnchorH,
    pub v_anchor: TextAnchorV,
    /// Rotation in degrees, anti-clockwise around `(x, y)` (math convention).
    pub rotation: f64,
}

/// Backend-neutral 2D drawing primitives.
///
/// Coordinates are in screen pixels (post-`CoordTransform`). All angles are in
/// degrees, with 0° pointing to +x and positive rotation going counter-
/// clockwise (math convention). Backends are responsible for flipping the y
/// axis if their native coordinate system differs.
pub trait RenderContext {
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, width: f64, color: Color);

    fn rectangle(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        stroke_width: f64,
        stroke: Option<Color>,
        fill: Option<Color>,
    );

    /// Filled / stroked rounded rectangle.
    fn rounded_rectangle(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        rx: f64,
        ry: f64,
        stroke_width: f64,
        stroke: Option<Color>,
        fill: Option<Color>,
    ) {
        // Default: ignore corner radii. Backends that support them override.
        let _ = (rx, ry);
        self.rectangle(x, y, w, h, stroke_width, stroke, fill);
    }

    fn circle(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        stroke_width: f64,
        stroke: Option<Color>,
        fill: Option<Color>,
    );

    /// Stroked or filled ellipse with separate x / y radii.
    fn ellipse(
        &mut self,
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        stroke_width: f64,
        stroke: Option<Color>,
        fill: Option<Color>,
    ) {
        // Default: degenerate to a circle with the average radius.
        self.circle(cx, cy, (rx + ry) * 0.5, stroke_width, stroke, fill);
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
    );

    /// Stroked elliptical arc. Start/end angles in degrees.
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
        let _ = ry;
        self.arc(cx, cy, rx, start_angle, end_angle, stroke_width, color);
    }

    fn polyline(&mut self, points: &[(f64, f64)], stroke_width: f64, color: Color);

    fn polygon(
        &mut self,
        points: &[(f64, f64)],
        stroke_width: f64,
        stroke: Option<Color>,
        fill: Option<Color>,
    );

    /// Cubic Bézier through 4 control points. Default approximates with line
    /// segments; backends with native curves override.
    fn cubic_bezier(
        &mut self,
        p0: (f64, f64),
        p1: (f64, f64),
        p2: (f64, f64),
        p3: (f64, f64),
        width: f64,
        color: Color,
    ) {
        const STEPS: u32 = 24;
        let mut prev = p0;
        for i in 1..=STEPS {
            let t = i as f64 / STEPS as f64;
            let mt = 1.0 - t;
            let x = mt.powi(3) * p0.0
                + 3.0 * mt.powi(2) * t * p1.0
                + 3.0 * mt * t.powi(2) * p2.0
                + t.powi(3) * p3.0;
            let y = mt.powi(3) * p0.1
                + 3.0 * mt.powi(2) * t * p1.1
                + 3.0 * mt * t.powi(2) * p2.1
                + t.powi(3) * p3.1;
            self.line(prev.0, prev.1, x, y, width, color);
            prev = (x, y);
        }
    }

    /// Plain text. Equivalent to [`text_styled`] with the default style.
    fn text(&mut self, x: f64, y: f64, size: f64, content: &str, color: Color) {
        self.text_styled(x, y, size, content, color, TextStyle::default());
    }

    fn text_styled(
        &mut self,
        x: f64,
        y: f64,
        size: f64,
        content: &str,
        color: Color,
        style: TextStyle,
    );

    /// Draw an embedded image (PNG, JPEG, BMP bytes) into a rectangle.
    ///
    /// The default does nothing — backends that can decode raster bytes
    /// override this. Pure-vector backends like SVG can base64-encode the
    /// bytes into a `<image>` element regardless of format.
    fn image(&mut self, _data: &[u8], _x: f64, _y: f64, _w: f64, _h: f64) {}

    /// Draw `content` with overline segments above the characters surrounded
    /// by `\` markers. Uses `text_styled` internally.
    fn text_with_overline(
        &mut self,
        x: f64,
        y: f64,
        size: f64,
        content: &str,
        color: Color,
        style: TextStyle,
    ) {
        // Default: render the cleaned text + a thin bar over each overlined
        // segment. Backends with full text-metrics access can override.
        let segments = super::overline::parse(content);
        let mut current_x = x;
        let approx_advance = size * 0.6;
        for seg in &segments {
            self.text_styled(current_x, y, size, &seg.text, color, style);
            if seg.overline {
                let w = seg.text.chars().count() as f64 * approx_advance;
                self.line(
                    current_x,
                    y - size * 1.05,
                    current_x + w,
                    y - size * 1.05,
                    (size * 0.05).max(0.5),
                    color,
                );
            }
            current_x += seg.text.chars().count() as f64 * approx_advance;
        }
    }

    // Optional state-stack ops
    //
    // The defaults are no-ops so simple backends like SVG don't have to track
    // a transform stack. The raster backend overrides them with real state.

    /// Push the current draw state (transform + clip) onto a stack.
    fn save_state(&mut self) {}

    /// Pop the most recently pushed draw state.
    fn restore_state(&mut self) {}

    /// Translate the current transform by `(dx, dy)`.
    fn translate(&mut self, dx: f64, dy: f64) {
        let _ = (dx, dy);
    }

    /// Rotate the current transform by `degrees` around the current origin.
    fn rotate(&mut self, degrees: f64) {
        let _ = degrees;
    }

    /// Scale the current transform by `(sx, sy)`.
    fn scale(&mut self, sx: f64, sy: f64) {
        let _ = (sx, sy);
    }
}

/// Render-time options shared by raster and vector backends.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Output width in pixels.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// Padding inside the output, in pixels.
    pub padding: u32,
    /// Solid background. `None` keeps the canvas transparent.
    pub background: Option<Color>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 512,
            height: 512,
            padding: 16,
            background: Some(Color::WHITE),
        }
    }
}
