//! SVG output.

use std::fmt::Write;

use super::context::{RenderContext, RenderOptions, TextAnchorH, TextAnchorV, TextStyle};
use crate::color::Color;

/// Accumulates SVG drawing commands as a string.
pub struct SvgContext {
    body: String,
    options: RenderOptions,
    /// Stack of transform group depths so save/restore can close out `<g>`s.
    group_depth: Vec<usize>,
    open_groups: usize,
}

impl SvgContext {
    pub fn new(options: RenderOptions) -> Self {
        Self {
            body: String::new(),
            options,
            group_depth: Vec::new(),
            open_groups: 0,
        }
    }

    pub fn finish(mut self) -> String {
        // Close any unmatched groups to keep the XML well-formed.
        while self.open_groups > 0 {
            self.body.push_str("</g>");
            self.open_groups -= 1;
        }
        let mut out = String::with_capacity(self.body.len() + 256);
        let _ = write!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
            self.options.width, self.options.height, self.options.width, self.options.height
        );
        if let Some(bg) = self.options.background {
            let _ = write!(
                out,
                "<rect width=\"{}\" height=\"{}\" fill=\"{}\"/>",
                self.options.width, self.options.height, bg
            );
        }
        out.push_str(&self.body);
        out.push_str("</svg>");
        out
    }

    fn fill_attr(fill: Option<Color>) -> String {
        match fill {
            Some(c) => format!("fill=\"{c}\""),
            None => "fill=\"none\"".to_string(),
        }
    }

    fn stroke_attr(stroke: Option<Color>, width: f64) -> String {
        match stroke {
            Some(c) => format!("stroke=\"{c}\" stroke-width=\"{width}\""),
            None => "stroke=\"none\"".to_string(),
        }
    }
}

impl RenderContext for SvgContext {
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, width: f64, color: Color) {
        let _ = write!(
            self.body,
            "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" stroke=\"{color}\" stroke-width=\"{width}\" stroke-linecap=\"round\"/>"
        );
    }

    fn rectangle(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        stroke_width: f64,
        stroke: Option<Color>,
        fill: Option<Color>,
    ) {
        let _ = write!(
            self.body,
            "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" {} {}/>",
            Self::fill_attr(fill),
            Self::stroke_attr(stroke, stroke_width)
        );
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
        stroke: Option<Color>,
        fill: Option<Color>,
    ) {
        let _ = write!(
            self.body,
            "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"{rx}\" ry=\"{ry}\" {} {}/>",
            Self::fill_attr(fill),
            Self::stroke_attr(stroke, stroke_width)
        );
    }

    fn circle(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        stroke_width: f64,
        stroke: Option<Color>,
        fill: Option<Color>,
    ) {
        let _ = write!(
            self.body,
            "<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{radius}\" {} {}/>",
            Self::fill_attr(fill),
            Self::stroke_attr(stroke, stroke_width)
        );
    }

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
        let _ = write!(
            self.body,
            "<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\" {} {}/>",
            Self::fill_attr(fill),
            Self::stroke_attr(stroke, stroke_width)
        );
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
        let start_rad = start_angle.to_radians();
        let end_rad = end_angle.to_radians();
        let x1 = cx + radius * start_rad.cos();
        let y1 = cy - radius * start_rad.sin();
        let x2 = cx + radius * end_rad.cos();
        let y2 = cy - radius * end_rad.sin();
        let sweep = end_angle - start_angle;
        let large_arc = if sweep.abs() > 180.0 { 1 } else { 0 };
        let sweep_flag = if sweep >= 0.0 { 0 } else { 1 };
        let _ = write!(
            self.body,
            "<path d=\"M {x1} {y1} A {radius} {radius} 0 {large_arc} {sweep_flag} {x2} {y2}\" stroke=\"{color}\" stroke-width=\"{stroke_width}\" fill=\"none\" stroke-linecap=\"round\"/>"
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
        let start_rad = start_angle.to_radians();
        let end_rad = end_angle.to_radians();
        let x1 = cx + rx * start_rad.cos();
        let y1 = cy - ry * start_rad.sin();
        let x2 = cx + rx * end_rad.cos();
        let y2 = cy - ry * end_rad.sin();
        let sweep = end_angle - start_angle;
        let large_arc = if sweep.abs() > 180.0 { 1 } else { 0 };
        let sweep_flag = if sweep >= 0.0 { 0 } else { 1 };
        let _ = write!(
            self.body,
            "<path d=\"M {x1} {y1} A {rx} {ry} 0 {large_arc} {sweep_flag} {x2} {y2}\" stroke=\"{color}\" stroke-width=\"{stroke_width}\" fill=\"none\" stroke-linecap=\"round\"/>"
        );
    }

    fn polyline(&mut self, points: &[(f64, f64)], stroke_width: f64, color: Color) {
        if points.is_empty() {
            return;
        }
        let mut pts = String::new();
        for (i, (x, y)) in points.iter().enumerate() {
            if i > 0 {
                pts.push(' ');
            }
            let _ = write!(pts, "{x},{y}");
        }
        let _ = write!(
            self.body,
            "<polyline points=\"{pts}\" stroke=\"{color}\" stroke-width=\"{stroke_width}\" fill=\"none\" stroke-linejoin=\"round\"/>"
        );
    }

    fn polygon(
        &mut self,
        points: &[(f64, f64)],
        stroke_width: f64,
        stroke: Option<Color>,
        fill: Option<Color>,
    ) {
        if points.is_empty() {
            return;
        }
        let mut pts = String::new();
        for (i, (x, y)) in points.iter().enumerate() {
            if i > 0 {
                pts.push(' ');
            }
            let _ = write!(pts, "{x},{y}");
        }
        let _ = write!(
            self.body,
            "<polygon points=\"{pts}\" {} {}/>",
            Self::fill_attr(fill),
            Self::stroke_attr(stroke, stroke_width)
        );
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
        let _ = write!(
            self.body,
            "<path d=\"M {} {} C {} {} {} {} {} {}\" stroke=\"{color}\" stroke-width=\"{width}\" fill=\"none\" stroke-linecap=\"round\"/>",
            p0.0, p0.1, p1.0, p1.1, p2.0, p2.1, p3.0, p3.1
        );
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
        let anchor = match style.h_anchor {
            TextAnchorH::Left => "start",
            TextAnchorH::Center => "middle",
            TextAnchorH::Right => "end",
        };
        let baseline = match style.v_anchor {
            TextAnchorV::Top => "text-before-edge",
            TextAnchorV::Middle => "central",
            TextAnchorV::Baseline => "alphabetic",
            TextAnchorV::Bottom => "text-after-edge",
        };
        let weight = if style.bold {
            " font-weight=\"bold\""
        } else {
            ""
        };
        let italic = if style.italic {
            " font-style=\"italic\""
        } else {
            ""
        };
        let transform = if style.rotation != 0.0 {
            // SVG rotates clockwise; flip the sign so positive degrees go ccw.
            format!(" transform=\"rotate({} {x} {y})\"", -style.rotation)
        } else {
            String::new()
        };
        let _ = write!(
            self.body,
            "<text x=\"{x}\" y=\"{y}\" font-size=\"{size}\" fill=\"{color}\" font-family=\"sans-serif\" text-anchor=\"{anchor}\" dominant-baseline=\"{baseline}\"{weight}{italic}{transform}>{}</text>",
            xml_escape(content)
        );
    }

    fn image(&mut self, data: &[u8], x: f64, y: f64, w: f64, h: f64) {
        if data.is_empty() {
            return;
        }
        let mime = guess_image_mime(data);
        let b64 = base64_encode(data);
        let _ = write!(
            self.body,
            "<image x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"none\" href=\"data:{mime};base64,{b64}\"/>"
        );
    }

    fn save_state(&mut self) {
        self.group_depth.push(self.open_groups);
    }

    fn restore_state(&mut self) {
        if let Some(target) = self.group_depth.pop() {
            while self.open_groups > target {
                self.body.push_str("</g>");
                self.open_groups -= 1;
            }
        }
    }

    fn translate(&mut self, dx: f64, dy: f64) {
        let _ = write!(self.body, "<g transform=\"translate({dx} {dy})\">");
        self.open_groups += 1;
    }

    fn rotate(&mut self, degrees: f64) {
        // Caller-side convention is math (ccw positive); SVG is cw positive.
        let _ = write!(self.body, "<g transform=\"rotate({})\">", -degrees);
        self.open_groups += 1;
    }

    fn scale(&mut self, sx: f64, sy: f64) {
        let _ = write!(self.body, "<g transform=\"scale({sx} {sy})\">");
        self.open_groups += 1;
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Sniff the image MIME type from a magic-number header.
fn guess_image_mime(data: &[u8]) -> &'static str {
    if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        "image/png"
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if data.starts_with(b"GIF8") {
        "image/gif"
    } else if data.starts_with(b"BM") {
        "image/bmp"
    } else {
        "application/octet-stream"
    }
}

/// Minimal base64 encoder (no external dep). Used only for inline image data.
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut chunks = data.chunks_exact(3);
    for chunk in chunks.by_ref() {
        let b = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        out.push(CHARS[((b >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((b >> 12) & 0x3F) as usize] as char);
        out.push(CHARS[((b >> 6) & 0x3F) as usize] as char);
        out.push(CHARS[(b & 0x3F) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let b = u32::from(rem[0]) << 16;
            out.push(CHARS[((b >> 18) & 0x3F) as usize] as char);
            out.push(CHARS[((b >> 12) & 0x3F) as usize] as char);
            out.push_str("==");
        }
        2 => {
            let b = (u32::from(rem[0]) << 16) | (u32::from(rem[1]) << 8);
            out.push(CHARS[((b >> 18) & 0x3F) as usize] as char);
            out.push(CHARS[((b >> 12) & 0x3F) as usize] as char);
            out.push(CHARS[((b >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod base64_tests {
    use super::base64_encode;

    #[test]
    fn round_trip_known_strings() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
