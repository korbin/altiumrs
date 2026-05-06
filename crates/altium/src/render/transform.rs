//! World ↔ screen coordinate transform for rendering.

use crate::coord::{Coord, CoordPoint, CoordRect};

/// Maps from internal `Coord` units (1 raw = 0.0001 mil) to screen pixels.
#[derive(Debug, Clone, Copy)]
pub struct CoordTransform {
    /// Pixels per raw unit.
    pub scale: f64,
    /// World x at the canvas left edge after centring.
    pub origin_x: i32,
    /// World y at the canvas top edge after centring.
    pub origin_y: i32,
    /// Pixel offset from the canvas's `0,0` to where the world box starts.
    pub offset_x: f64,
    pub offset_y: f64,
    /// Output width in pixels.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// If true, screen y grows downward (default for image output).
    pub flip_y: bool,
}

impl CoordTransform {
    /// Build a transform that fits `bounds` into `(width, height)` with `padding`
    /// pixels of margin around the box. The world box is centred in the canvas.
    pub fn fit(bounds: CoordRect, width: u32, height: u32, padding: u32) -> Self {
        if bounds.is_empty() || width == 0 || height == 0 {
            return Self {
                scale: 1.0,
                origin_x: 0,
                origin_y: 0,
                offset_x: 0.0,
                offset_y: 0.0,
                width,
                height,
                flip_y: true,
            };
        }
        let inner_w = (width.saturating_sub(2 * padding)) as f64;
        let inner_h = (height.saturating_sub(2 * padding)) as f64;
        let bw = bounds.width().to_raw().max(1) as f64;
        let bh = bounds.height().to_raw().max(1) as f64;
        let sx = inner_w / bw;
        let sy = inner_h / bh;
        let scale = sx.min(sy);

        let drawn_w = bw * scale;
        let drawn_h = bh * scale;
        let offset_x = (width as f64 - drawn_w) / 2.0;
        let offset_y = (height as f64 - drawn_h) / 2.0;

        Self {
            scale,
            origin_x: bounds.min.x.to_raw(),
            origin_y: bounds.max.y.to_raw(),
            offset_x,
            offset_y,
            width,
            height,
            flip_y: true,
        }
    }

    pub fn world_to_screen(&self, point: CoordPoint) -> (f64, f64) {
        let dx = (point.x.to_raw() - self.origin_x) as f64 * self.scale;
        let dy = if self.flip_y {
            (self.origin_y - point.y.to_raw()) as f64 * self.scale
        } else {
            (point.y.to_raw() - self.origin_y) as f64 * self.scale
        };
        (self.offset_x + dx, self.offset_y + dy)
    }

    pub fn scale_value(&self, value: Coord) -> f64 {
        value.to_raw() as f64 * self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_handles_empty_bounds() {
        let t = CoordTransform::fit(CoordRect::EMPTY, 100, 100, 0);
        assert_eq!(t.width, 100);
    }

    #[test]
    fn world_to_screen_top_left() {
        let bounds = CoordRect::from_xyxy(
            Coord::from_mils(0.0),
            Coord::from_mils(0.0),
            Coord::from_mils(10.0),
            Coord::from_mils(10.0),
        );
        let t = CoordTransform::fit(bounds, 100, 100, 0);
        // Top-left in world = (0, 10), should map to (0, 0) on screen.
        let (sx, sy) = t.world_to_screen(CoordPoint::new(
            Coord::from_mils(0.0),
            Coord::from_mils(10.0),
        ));
        assert!(sx.abs() < 0.001);
        assert!(sy.abs() < 0.001);
    }

    #[test]
    fn world_to_screen_centres() {
        // Bounds 10x10, canvas 100x50: drawn 50x50, x-padding 25.
        let bounds = CoordRect::from_xyxy(
            Coord::from_mils(0.0),
            Coord::from_mils(0.0),
            Coord::from_mils(10.0),
            Coord::from_mils(10.0),
        );
        let t = CoordTransform::fit(bounds, 100, 50, 0);
        let (sx, _) = t.world_to_screen(CoordPoint::new(
            Coord::from_mils(0.0),
            Coord::from_mils(10.0),
        ));
        assert!((sx - 25.0).abs() < 0.5);
    }
}
