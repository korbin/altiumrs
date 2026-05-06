//! Fixed-point coordinate types. One raw unit is 1/10 000 of a mil.

use std::fmt;
use std::num::ParseFloatError;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};
use std::str::FromStr;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

const RAW_PER_MIL: f64 = 10_000.0;
const MILS_PER_MM: f64 = 39.370_078_740_157_48;
const MILS_PER_INCH: f64 = 1000.0;
const RAW_PER_MIL_I: i64 = 10_000;

/// Fixed-point length in 1/10 000 mil quanta.
///
/// Construct with [`Coord::from_mils`], [`Coord::from_mm`], or [`Coord::from_raw`].
/// Convert back with [`Coord::to_mils`], [`Coord::to_mm`], or [`Coord::to_raw`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct Coord(pub i32);

impl Coord {
    /// Zero length.
    pub const ZERO: Self = Self(0);
    /// Minimum representable length.
    pub const MIN: Self = Self(i32::MIN);
    /// Maximum representable length.
    pub const MAX: Self = Self(i32::MAX);

    /// Wrap a raw `i32` as a coordinate.
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    /// Build from a value in mils. Rounded to the nearest raw unit.
    pub fn from_mils(mils: f64) -> Self {
        Self((mils * RAW_PER_MIL).round() as i32)
    }

    /// Build from a value in millimetres.
    pub fn from_mm(mm: f64) -> Self {
        Self::from_mils(mm * MILS_PER_MM)
    }

    /// Build from a value in inches.
    pub fn from_inches(inches: f64) -> Self {
        Self::from_mils(inches * MILS_PER_INCH)
    }

    /// The raw value (1 raw = 0.0001 mil).
    pub const fn to_raw(self) -> i32 {
        self.0
    }

    /// Convert to mils.
    pub fn to_mils(self) -> f64 {
        self.0 as f64 / RAW_PER_MIL
    }

    /// Convert to millimetres.
    pub fn to_mm(self) -> f64 {
        self.to_mils() / MILS_PER_MM
    }

    /// Convert to inches.
    pub fn to_inches(self) -> f64 {
        self.to_mils() / MILS_PER_INCH
    }

    /// Absolute value (saturating at `MAX`).
    pub const fn abs(self) -> Self {
        Self(self.0.saturating_abs())
    }

    /// The smaller of two lengths.
    pub const fn min(self, other: Self) -> Self {
        if self.0 < other.0 { self } else { other }
    }

    /// The larger of two lengths.
    pub const fn max(self, other: Self) -> Self {
        if self.0 > other.0 { self } else { other }
    }

    /// Parse the Altium-style decimal form (`"10.5mil"`, `"4mm"`, `"-3.5"`).
    /// A bare number is treated as raw units.
    pub fn parse_altium(s: &str) -> Result<Self, ParseCoordError> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(ParseCoordError::Empty);
        }
        if let Some(rest) = strip_unit_suffix(trimmed, "mil") {
            let v: f64 = rest.parse().map_err(ParseCoordError::Float)?;
            return Ok(Self::from_mils(v));
        }
        if let Some(rest) = strip_unit_suffix(trimmed, "mm") {
            let v: f64 = rest.parse().map_err(ParseCoordError::Float)?;
            return Ok(Self::from_mm(v));
        }
        if let Some(rest) =
            strip_unit_suffix(trimmed, "inch").or_else(|| strip_unit_suffix(trimmed, "in"))
        {
            let v: f64 = rest.parse().map_err(ParseCoordError::Float)?;
            return Ok(Self::from_inches(v));
        }
        // Bare integer → raw.
        if let Ok(raw) = trimmed.parse::<i32>() {
            return Ok(Self(raw));
        }
        Err(ParseCoordError::UnknownFormat)
    }
}

fn strip_unit_suffix<'a>(s: &'a str, unit: &str) -> Option<&'a str> {
    let bytes = s.as_bytes();
    let unit_bytes = unit.as_bytes();
    if bytes.len() >= unit_bytes.len() {
        let split = bytes.len() - unit_bytes.len();
        // The unit string is ASCII; a multi-byte UTF-8 char ending exactly at
        // `bytes.len()` would put `split` inside that codepoint, so verify the
        // boundary before slicing as a `&str` (otherwise indexing panics).
        if s.is_char_boundary(split) && s[split..].eq_ignore_ascii_case(unit) {
            return Some(s[..split].trim_end());
        }
    }
    None
}

/// Errors from [`Coord::parse_altium`] / `<Coord as FromStr>`.
#[derive(Debug, thiserror::Error)]
pub enum ParseCoordError {
    #[error("empty input")]
    Empty,
    #[error("invalid number: {0}")]
    Float(#[from] ParseFloatError),
    #[error("expected a number with optional 'mil', 'mm', or 'inch' suffix")]
    UnknownFormat,
}

impl FromStr for Coord {
    type Err = ParseCoordError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_altium(s)
    }
}

impl fmt::Display for Coord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let raw = self.0;
        let abs = i64::from(raw).unsigned_abs();
        let whole = abs / RAW_PER_MIL_I as u64;
        let frac = (abs % RAW_PER_MIL_I as u64) as u32;
        let sign = if raw < 0 { "-" } else { "" };
        if frac == 0 {
            write!(f, "{sign}{whole}mil")
        } else {
            let buf = format!("{frac:04}");
            let trimmed = buf.trim_end_matches('0');
            write!(f, "{sign}{whole}.{trimmed}mil")
        }
    }
}

impl Add for Coord {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Coord {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl Neg for Coord {
    type Output = Self;
    fn neg(self) -> Self {
        Self(self.0.saturating_neg())
    }
}

impl AddAssign for Coord {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl SubAssign for Coord {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul<i32> for Coord {
    type Output = Self;
    fn mul(self, rhs: i32) -> Self {
        Self(self.0.saturating_mul(rhs))
    }
}

impl Div<i32> for Coord {
    type Output = Self;
    fn div(self, rhs: i32) -> Self {
        Self(self.0 / rhs)
    }
}

/// 2D point in coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CoordPoint {
    pub x: Coord,
    pub y: Coord,
}

impl CoordPoint {
    pub const ZERO: Self = Self {
        x: Coord::ZERO,
        y: Coord::ZERO,
    };

    pub const fn new(x: Coord, y: Coord) -> Self {
        Self { x, y }
    }

    /// Build a point from millimetres on both axes.
    pub fn mm(x: f64, y: f64) -> Self {
        Self {
            x: Coord::from_mm(x),
            y: Coord::from_mm(y),
        }
    }

    /// Build a point from mils on both axes.
    pub fn mils(x: f64, y: f64) -> Self {
        Self {
            x: Coord::from_mils(x),
            y: Coord::from_mils(y),
        }
    }

    /// Euclidean distance to another point, in raw units.
    pub fn distance_to(self, other: Self) -> f64 {
        let dx = (other.x.0 as f64) - (self.x.0 as f64);
        let dy = (other.y.0 as f64) - (self.y.0 as f64);
        (dx * dx + dy * dy).sqrt()
    }
}

impl Add for CoordPoint {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for CoordPoint {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

/// Axis-aligned bounding rectangle with optional emptiness sentinel.
///
/// `CoordRect::EMPTY` is the identity for [`Self::union`] — unioning anything
/// with `EMPTY` returns the other operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CoordRect {
    pub min: CoordPoint,
    pub max: CoordPoint,
}

impl Default for CoordRect {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl CoordRect {
    /// The empty rectangle. `min > max` on both axes — chosen so that
    /// `EMPTY.union(other)` is `other`.
    pub const EMPTY: Self = Self {
        min: CoordPoint {
            x: Coord::MAX,
            y: Coord::MAX,
        },
        max: CoordPoint {
            x: Coord::MIN,
            y: Coord::MIN,
        },
    };

    /// Build a rectangle from two opposite corners. Order does not matter.
    pub fn from_corners(a: CoordPoint, b: CoordPoint) -> Self {
        Self {
            min: CoordPoint::new(a.x.min(b.x), a.y.min(b.y)),
            max: CoordPoint::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// Build a rectangle from raw boundary values.
    pub fn from_xyxy(x1: Coord, y1: Coord, x2: Coord, y2: Coord) -> Self {
        Self::from_corners(CoordPoint::new(x1, y1), CoordPoint::new(x2, y2))
    }

    /// Build centred at `center` with the given width and height.
    pub fn from_center(center: CoordPoint, width: Coord, height: Coord) -> Self {
        let half_w = width / 2;
        let half_h = height / 2;
        Self {
            min: CoordPoint::new(center.x - half_w, center.y - half_h),
            max: CoordPoint::new(center.x + half_w, center.y + half_h),
        }
    }

    /// Whether this rectangle is the empty sentinel (`min > max`).
    pub const fn is_empty(self) -> bool {
        self.min.x.0 > self.max.x.0 || self.min.y.0 > self.max.y.0
    }

    pub fn width(self) -> Coord {
        if self.is_empty() {
            Coord::ZERO
        } else {
            self.max.x - self.min.x
        }
    }

    pub fn height(self) -> Coord {
        if self.is_empty() {
            Coord::ZERO
        } else {
            self.max.y - self.min.y
        }
    }

    /// Smallest rectangle that contains both `self` and `other`.
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Self {
            min: CoordPoint::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: CoordPoint::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// Smallest rectangle that contains both `self` and `point`.
    pub fn union_point(self, point: CoordPoint) -> Self {
        if self.is_empty() {
            return Self {
                min: point,
                max: point,
            };
        }
        Self {
            min: CoordPoint::new(self.min.x.min(point.x), self.min.y.min(point.y)),
            max: CoordPoint::new(self.max.x.max(point.x), self.max.y.max(point.y)),
        }
    }

    /// Whether `point` lies within this rectangle (inclusive of edges).
    pub fn contains(self, point: CoordPoint) -> bool {
        !self.is_empty()
            && point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_to_mil_round_trips() {
        let c = Coord::from_mils(123.0);
        assert_eq!(c.to_raw(), 1_230_000);
        assert_eq!(c.to_mils(), 123.0);
    }

    #[test]
    fn from_to_mm_round_trips() {
        let one_inch = Coord::from_mm(25.4);
        // 25.4 mm = 1 inch = 1000 mil = 10_000_000 raw, allow 1 raw rounding.
        assert!((one_inch.to_raw() - 10_000_000).abs() <= 1);
        assert!((one_inch.to_inches() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn arithmetic() {
        let a = Coord::from_mils(10.0);
        let b = Coord::from_mils(2.5);
        assert_eq!((a + b).to_mils(), 12.5);
        assert_eq!((a - b).to_mils(), 7.5);
        assert_eq!((-a).to_mils(), -10.0);
        assert_eq!((a * 3).to_mils(), 30.0);
        assert_eq!((a / 4).to_mils(), 2.5);

        let mut c = a;
        c += b;
        assert_eq!(c.to_mils(), 12.5);
        c -= b;
        assert_eq!(c.to_mils(), 10.0);
    }

    #[test]
    fn ordering() {
        assert!(Coord::from_mils(1.0) < Coord::from_mils(2.0));
        assert!(Coord::from_mils(1.0) <= Coord::from_mils(1.0));
        assert_eq!(
            Coord::from_mils(1.0).max(Coord::from_mils(2.0)),
            Coord::from_mils(2.0)
        );
        assert_eq!(
            Coord::from_mils(1.0).min(Coord::from_mils(2.0)),
            Coord::from_mils(1.0)
        );
    }

    #[test]
    fn abs() {
        assert_eq!(Coord::from_mils(-5.0).abs(), Coord::from_mils(5.0));
        assert_eq!(Coord::from_mils(5.0).abs(), Coord::from_mils(5.0));
    }

    #[test]
    fn display_round_trip() {
        let cases = [10, 1, 0, -10, 12345, -1, 1_000_000];
        for &raw in &cases {
            let c = Coord::from_raw(raw);
            let s = c.to_string();
            let parsed: Coord = s.parse().expect("parse");
            assert_eq!(parsed, c, "round trip failed for raw={raw} string={s}");
        }
    }

    #[test]
    fn display_basic_forms() {
        assert_eq!(Coord::from_mils(10.0).to_string(), "10mil");
        assert_eq!(Coord::from_mils(0.5).to_string(), "0.5mil");
        assert_eq!(Coord::from_mils(-0.0001).to_string(), "-0.0001mil");
        assert_eq!(Coord::ZERO.to_string(), "0mil");
    }

    #[test]
    fn parse_altium_forms() {
        assert_eq!(
            Coord::parse_altium("10mil").unwrap(),
            Coord::from_mils(10.0)
        );
        assert_eq!(
            Coord::parse_altium("10.5mil").unwrap(),
            Coord::from_mils(10.5)
        );
        assert_eq!(
            Coord::parse_altium("  10mil  ").unwrap(),
            Coord::from_mils(10.0)
        );
        assert_eq!(
            Coord::parse_altium("25.4mm").unwrap().to_inches().round(),
            1.0
        );
        assert_eq!(
            Coord::parse_altium("0.5inch").unwrap(),
            Coord::from_inches(0.5)
        );
        assert_eq!(Coord::parse_altium("-3").unwrap(), Coord::from_raw(-3));
    }

    #[test]
    fn parse_altium_errors() {
        assert!(matches!(
            Coord::parse_altium("").unwrap_err(),
            ParseCoordError::Empty
        ));
        assert!(matches!(
            Coord::parse_altium("hello").unwrap_err(),
            ParseCoordError::UnknownFormat
        ));
    }

    #[test]
    fn coord_point_distance() {
        let a = CoordPoint::mils(0.0, 0.0);
        let b = CoordPoint::mils(3.0, 4.0);
        assert!((a.distance_to(b) / RAW_PER_MIL - 5.0).abs() < 1e-6);
    }

    #[test]
    fn rect_from_corners_normalises() {
        let r = CoordRect::from_corners(CoordPoint::mils(10.0, 20.0), CoordPoint::mils(0.0, 5.0));
        assert_eq!(r.min, CoordPoint::mils(0.0, 5.0));
        assert_eq!(r.max, CoordPoint::mils(10.0, 20.0));
        assert_eq!(r.width(), Coord::from_mils(10.0));
        assert_eq!(r.height(), Coord::from_mils(15.0));
    }

    #[test]
    fn rect_from_center() {
        let r = CoordRect::from_center(
            CoordPoint::mils(10.0, 20.0),
            Coord::from_mils(4.0),
            Coord::from_mils(6.0),
        );
        assert_eq!(r.min, CoordPoint::mils(8.0, 17.0));
        assert_eq!(r.max, CoordPoint::mils(12.0, 23.0));
    }

    #[test]
    fn rect_empty_unions() {
        let empty = CoordRect::EMPTY;
        assert!(empty.is_empty());
        assert_eq!(empty.width(), Coord::ZERO);
        let r = CoordRect::from_corners(CoordPoint::ZERO, CoordPoint::mils(1.0, 1.0));
        assert_eq!(empty.union(r), r);
        assert_eq!(r.union(empty), r);
    }

    #[test]
    fn rect_union() {
        let a = CoordRect::from_corners(CoordPoint::ZERO, CoordPoint::mils(1.0, 1.0));
        let b = CoordRect::from_corners(CoordPoint::mils(2.0, -1.0), CoordPoint::mils(3.0, 4.0));
        let u = a.union(b);
        assert_eq!(u.min, CoordPoint::mils(0.0, -1.0));
        assert_eq!(u.max, CoordPoint::mils(3.0, 4.0));
    }

    #[test]
    fn rect_union_point_extends_empty() {
        let p = CoordPoint::mils(1.0, 2.0);
        let r = CoordRect::EMPTY.union_point(p);
        assert_eq!(r.min, p);
        assert_eq!(r.max, p);
    }

    #[test]
    fn rect_contains() {
        let r = CoordRect::from_corners(CoordPoint::ZERO, CoordPoint::mils(10.0, 10.0));
        assert!(r.contains(CoordPoint::mils(5.0, 5.0)));
        assert!(r.contains(CoordPoint::ZERO));
        assert!(r.contains(CoordPoint::mils(10.0, 10.0)));
        assert!(!r.contains(CoordPoint::mils(11.0, 5.0)));
        assert!(!CoordRect::EMPTY.contains(CoordPoint::ZERO));
    }
}
