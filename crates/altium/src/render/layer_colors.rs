//! Default color palette for PCB layers.
//!
//! Layer indices:
//! - 1: Top Layer
//! - 2-31: Mid-Layer 1-30
//! - 32: Bottom Layer
//! - 33: Top Overlay (silkscreen)
//! - 34: Bottom Overlay
//! - 35-36: Top/Bottom Paste
//! - 37-38: Top/Bottom Solder
//! - 39-54: Internal Planes 1-16
//! - 55: Drill Guide
//! - 56: Keepout
//! - 57-72: Mechanical 1-16
//! - 73: Drill Drawing
//! - 74: Multi-Layer
//! - 81: Pad Holes
//! - 82: Via Holes

use crate::color::Color;

/// Returns the canonical Altium color for a PCB layer byte. Falls back to
/// neutral grey for unknown layers.
pub fn color_for(layer: i32) -> Color {
    let argb: u32 = match layer {
        // Signal layers
        1 => 0xFFFF0000,
        2 => 0xFFBC8E00,
        3 => 0xFF70DBFA,
        4 => 0xFF00CC66,
        5 => 0xFF9966FF,
        6 => 0xFF00FFFF,
        7 => 0xFF800080,
        8 => 0xFFFF00FF,
        9 => 0xFF808000,
        10 => 0xFFFFFF00,
        11 => 0xFF808080,
        12 => 0xFFFFFFFF,
        13 => 0xFF800080,
        14 => 0xFF008080,
        15 => 0xFFC0C0C0,
        16 => 0xFF800000,
        17 => 0xFF008000,
        18 => 0xFF00FF00,
        19 => 0xFF000080,
        20 => 0xFF00FFFF,
        21 => 0xFF800080,
        22 => 0xFFFF00FF,
        23 => 0xFF808000,
        24 => 0xFFFFFF00,
        25 => 0xFF808080,
        26 => 0xFFFFFFFF,
        27 => 0xFF800080,
        28 => 0xFF008080,
        29 => 0xFFC0C0C0,
        30 => 0xFF800000,
        31 => 0xFF008000,
        32 => 0xFF0000FF,
        // Overlay
        33 => 0xFFFFFF00,
        34 => 0xFF808000,
        // Paste
        35 => 0xFF808080,
        36 => 0xFF800000,
        // Soldermask
        37 => 0xFF800080,
        38 => 0xFFFF00FF,
        // Internal planes
        39 => 0xFF008000,
        40 => 0xFF800000,
        41 => 0xFF800080,
        42 => 0xFF008080,
        43 => 0xFF008000,
        44 => 0xFF800000,
        45 => 0xFF800080,
        46 => 0xFF008080,
        47 => 0xFF008000,
        48 => 0xFF800000,
        49 => 0xFF800080,
        50 => 0xFF008080,
        51 => 0xFF008000,
        52 => 0xFF800000,
        53 => 0xFF800080,
        54 => 0xFF008080,
        // Utility
        55 => 0xFF800000,
        56 => 0xFFFF00FF,
        // Mechanical
        57 => 0xFFFF00FF,
        58 => 0xFF800080,
        59 => 0xFF008000,
        60 => 0xFF808000,
        61 => 0xFFFF00FF,
        62 => 0xFF800080,
        63 => 0xFF008000,
        64 => 0xFF808000,
        65 => 0xFFFF00FF,
        66 => 0xFF800080,
        67 => 0xFF008000,
        68 => 0xFF808000,
        69 => 0xFFFF00FF,
        70 => 0xFF800080,
        71 => 0xFF008000,
        72 => 0xFF000000,
        // Other
        73 => 0xFFFF002A,
        74 => 0xFFC0C0C0,
        // Holes
        81 => 0xFF009190,
        82 => 0xFF816200,
        _ => 0xFF808080,
    };
    Color::from_argb_u32(argb)
}

/// Z-order for compositing layers; lower numbers paint first (i.e. underneath).
/// Mirrors the C# `LayerColors.GetDrawPriority`.
pub fn draw_priority(layer: i32) -> i32 {
    match layer {
        32 => 0,       // Bottom layer
        38 => 1,       // Bottom solder
        36 => 2,       // Bottom paste
        34 => 3,       // Bottom overlay
        2..=31 => 6,   // Mid layers
        1 => 6,        // Top layer
        37 => 9,       // Top solder
        35 => 7,       // Top paste
        33 => 2,       // Top overlay
        74 => 1,       // Multi-layer
        39..=54 => 11, // Internal planes
        55 => 12,      // Drill guide
        56 => 13,      // Keepout
        57..=72 => 14, // Mechanical
        73 => 15,      // Drill drawing
        81 | 82 => 1,  // Holes
        _ => 50,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_layers_return_distinct_colors() {
        let top = color_for(1);
        let bottom = color_for(32);
        assert_ne!(top, bottom);
        assert_eq!(top.r, 0xFF);
        assert_eq!(bottom.b, 0xFF);
    }

    #[test]
    fn unknown_layer_falls_back_to_grey() {
        assert_eq!(color_for(200), Color::from_argb_u32(0xFF808080));
    }

    #[test]
    fn draw_priority_lower_for_bottom() {
        assert!(draw_priority(32) < draw_priority(1));
    }
}
