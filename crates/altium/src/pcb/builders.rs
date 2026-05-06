//! Fluent builders for the most-used PCB primitives.

#![allow(clippy::field_reassign_with_default)]

use super::primitives::{Arc, Pad, Track, Via};
use crate::coord::{Coord, CoordPoint};
use crate::enums::PadShape;

// PadBuilder

/// Fluent builder for [`Pad`]. Defaults: round, top layer, plated, unrotated.
pub struct PadBuilder {
    pad: Pad,
}

impl Default for PadBuilder {
    fn default() -> Self {
        let mut pad = Pad::default();
        pad.shape_top = PadShape::Round;
        pad.shape_middle = PadShape::Round;
        pad.shape_bottom = PadShape::Round;
        pad.layer = 1;
        pad.is_plated = true;
        Self { pad }
    }
}

impl PadBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn at(mut self, x: Coord, y: Coord) -> Self {
        self.pad.location = CoordPoint::new(x, y);
        self
    }

    pub fn size(mut self, w: Coord, h: Coord) -> Self {
        let p = CoordPoint::new(w, h);
        self.pad.size_top = p;
        self.pad.size_middle = p;
        self.pad.size_bottom = p;
        self
    }

    pub fn shape(mut self, shape: PadShape) -> Self {
        self.pad.shape_top = shape;
        self.pad.shape_middle = shape;
        self.pad.shape_bottom = shape;
        self
    }

    pub fn hole_size(mut self, hole: Coord) -> Self {
        self.pad.hole_size = hole;
        self
    }

    pub fn rotation(mut self, degrees: f64) -> Self {
        self.pad.rotation = degrees;
        self
    }

    pub fn designator(mut self, designator: impl Into<String>) -> Self {
        self.pad.designator = Some(designator.into());
        self
    }

    pub fn layer(mut self, layer: i32) -> Self {
        self.pad.layer = layer;
        self
    }

    pub fn plated(mut self, plated: bool) -> Self {
        self.pad.is_plated = plated;
        self
    }

    pub fn build(self) -> Pad {
        self.pad
    }
}

// TrackBuilder

pub struct TrackBuilder {
    track: Track,
}

impl Default for TrackBuilder {
    fn default() -> Self {
        let mut track = Track::default();
        track.layer = 1;
        track.width = Coord::from_mils(10.0);
        Self { track }
    }
}

impl TrackBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from(mut self, x: Coord, y: Coord) -> Self {
        self.track.start = CoordPoint::new(x, y);
        self
    }

    pub fn to(mut self, x: Coord, y: Coord) -> Self {
        self.track.end = CoordPoint::new(x, y);
        self
    }

    pub fn width(mut self, w: Coord) -> Self {
        self.track.width = w;
        self
    }

    pub fn layer(mut self, layer: i32) -> Self {
        self.track.layer = layer;
        self
    }

    pub fn build(self) -> Track {
        self.track
    }
}

// ViaBuilder

pub struct ViaBuilder {
    via: Via,
}

impl Default for ViaBuilder {
    fn default() -> Self {
        let mut via = Via::default();
        via.start_layer = 1;
        via.end_layer = 32;
        via.layer = 74;
        via.is_plated = true;
        Self { via }
    }
}

impl ViaBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn at(mut self, x: Coord, y: Coord) -> Self {
        self.via.location = CoordPoint::new(x, y);
        self
    }

    pub fn diameter(mut self, d: Coord) -> Self {
        self.via.diameter = d;
        self
    }

    pub fn hole_size(mut self, h: Coord) -> Self {
        self.via.hole_size = h;
        self
    }

    pub fn layers(mut self, from: i32, to: i32) -> Self {
        self.via.start_layer = from;
        self.via.end_layer = to;
        self
    }

    pub fn build(self) -> Via {
        self.via
    }
}

// ArcBuilder

pub struct ArcBuilder {
    arc: Arc,
}

impl Default for ArcBuilder {
    fn default() -> Self {
        let mut arc = Arc::default();
        arc.layer = 1;
        arc.start_angle = 0.0;
        arc.end_angle = 360.0;
        arc.width = Coord::from_mils(10.0);
        Self { arc }
    }
}

impl ArcBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn at(mut self, x: Coord, y: Coord) -> Self {
        self.arc.center = CoordPoint::new(x, y);
        self
    }

    pub fn radius(mut self, r: Coord) -> Self {
        self.arc.radius = r;
        self
    }

    pub fn angles(mut self, start: f64, end: f64) -> Self {
        self.arc.start_angle = start;
        self.arc.end_angle = end;
        self
    }

    pub fn width(mut self, w: Coord) -> Self {
        self.arc.width = w;
        self
    }

    pub fn layer(mut self, layer: i32) -> Self {
        self.arc.layer = layer;
        self
    }

    pub fn build(self) -> Arc {
        self.arc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_builder_sets_defaults_and_overrides() {
        let pad = PadBuilder::new()
            .at(Coord::from_mils(10.0), Coord::from_mils(20.0))
            .size(Coord::from_mils(40.0), Coord::from_mils(60.0))
            .shape(PadShape::Rectangular)
            .designator("1")
            .layer(1)
            .build();
        assert_eq!(pad.designator.as_deref(), Some("1"));
        assert_eq!(pad.shape_top, PadShape::Rectangular);
        assert_eq!(pad.size_top.x, Coord::from_mils(40.0));
        assert!(pad.is_plated, "default plated=true");
    }

    #[test]
    fn track_builder_chains_cleanly() {
        let track = TrackBuilder::new()
            .from(Coord::ZERO, Coord::ZERO)
            .to(Coord::from_mils(100.0), Coord::ZERO)
            .width(Coord::from_mils(8.0))
            .layer(32)
            .build();
        assert_eq!(track.layer, 32);
        assert_eq!(track.width, Coord::from_mils(8.0));
    }

    #[test]
    fn via_builder_uses_through_via_default() {
        let via = ViaBuilder::new()
            .at(Coord::ZERO, Coord::ZERO)
            .diameter(Coord::from_mils(28.0))
            .hole_size(Coord::from_mils(14.0))
            .build();
        assert_eq!(via.start_layer, 1);
        assert_eq!(via.end_layer, 32);
        assert!(via.is_plated);
    }

    #[test]
    fn arc_builder_full_circle_default() {
        let arc = ArcBuilder::new()
            .at(Coord::ZERO, Coord::ZERO)
            .radius(Coord::from_mils(20.0))
            .build();
        assert_eq!(arc.start_angle, 0.0);
        assert_eq!(arc.end_angle, 360.0);
    }
}
