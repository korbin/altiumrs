//! 32-bit RGBA color with helpers for the BGR-packed integers used in Altium files.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// 8-bit-per-channel color with full alpha by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Build a fully opaque color from a packed `0x00BBGGRR` integer.
    ///
    /// This matches Altium's on-disk storage: red occupies the low byte,
    /// blue the high byte of the lower 24 bits.
    pub const fn from_altium_bgr(bgr: i32) -> Self {
        let r = (bgr & 0xFF) as u8;
        let g = ((bgr >> 8) & 0xFF) as u8;
        let b = ((bgr >> 16) & 0xFF) as u8;
        Self::rgb(r, g, b)
    }

    /// Pack to the BGR-style integer Altium stores in files.
    pub const fn to_altium_bgr(self) -> i32 {
        (self.r as i32) | ((self.g as i32) << 8) | ((self.b as i32) << 16)
    }

    /// Pack to a `0xAARRGGBB` integer.
    pub const fn to_argb_u32(self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }

    /// Build from a `0xAARRGGBB` integer.
    pub const fn from_argb_u32(argb: u32) -> Self {
        Self {
            a: (argb >> 24) as u8,
            r: (argb >> 16) as u8,
            g: (argb >> 8) as u8,
            b: argb as u8,
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.a == 255 {
            write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            write!(
                f,
                "#{:02x}{:02x}{:02x}{:02x}",
                self.r, self.g, self.b, self.a
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn altium_bgr_round_trip() {
        // Red is in the low byte: red = 0x000000FF.
        let red = Color::from_altium_bgr(0x0000_00FF);
        assert_eq!(red, Color::rgb(255, 0, 0));
        assert_eq!(red.to_altium_bgr(), 0x0000_00FF);

        // Blue is in the high byte: blue = 0x00FF0000.
        let blue = Color::from_altium_bgr(0x00FF_0000);
        assert_eq!(blue, Color::rgb(0, 0, 255));
        assert_eq!(blue.to_altium_bgr(), 0x00FF_0000);

        let mix = Color::from_altium_bgr(0x0080_4020);
        assert_eq!(mix, Color::rgb(0x20, 0x40, 0x80));
    }

    #[test]
    fn argb_round_trip() {
        let c = Color::rgba(0x11, 0x22, 0x33, 0x44);
        assert_eq!(Color::from_argb_u32(c.to_argb_u32()), c);
    }

    #[test]
    fn display() {
        assert_eq!(Color::rgb(0xff, 0x00, 0x80).to_string(), "#ff0080");
        assert_eq!(Color::rgba(0xff, 0x00, 0x80, 0x40).to_string(), "#ff008040");
    }
}
