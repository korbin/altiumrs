//! Enumerations used throughout PCB and schematic data.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Generates a `#[repr(i32)]` enum together with `TryFrom<i32>` and `From<Self> for i32`.
///
/// The first variant is treated as the default for `Default::default()`.
macro_rules! repr_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$vmeta:meta])*
                $variant:ident = $value:expr
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(i32)]
        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
        $vis enum $name {
            $(
                $(#[$vmeta])*
                $variant = $value
            ),+
        }

        impl TryFrom<i32> for $name {
            type Error = i32;
            fn try_from(value: i32) -> ::core::result::Result<Self, i32> {
                match value {
                    $($value => Ok($name::$variant),)+
                    other => Err(other),
                }
            }
        }

        impl From<$name> for i32 {
            fn from(value: $name) -> i32 {
                value as i32
            }
        }

        impl Default for $name {
            fn default() -> Self {
                let arr = [$($name::$variant),+];
                arr[0]
            }
        }
    };
}

repr_enum! {
    /// Diagnostic severity for warnings emitted during reading or writing.
    pub enum DiagnosticSeverity {
        Info = 0,
        Warning = 1,
        Error = 2,
    }
}

repr_enum! {
    /// Stroke / dash style for a line.
    pub enum LineStyle {
        Solid = 0,
        Dash = 1,
        Dot = 2,
        DashDot = 3,
        DashDotDot = 4,
    }
}

repr_enum! {
    /// Schematic-specific subset of [`LineStyle`].
    pub enum SchLineStyle {
        Solid = 0,
        Dashed = 1,
        Dotted = 2,
        DashDotted = 3,
    }
}

repr_enum! {
    /// Schematic pin direction.
    pub enum PinOrientation {
        Right = 0,
        Up = 1,
        Left = 2,
        Down = 3,
    }
}

repr_enum! {
    /// Electrical role of a schematic pin.
    pub enum PinElectricalType {
        Input = 0,
        InputOutput = 1,
        Output = 2,
        OpenCollector = 3,
        Passive = 4,
        HiZ = 5,
        OpenEmitter = 6,
        Power = 7,
    }
}

/// Copper pad outline shape.
///
/// Numeric values match Altium's on-disk shape byte for the four known
/// variants (1/2/3/9). Unrecognised bytes round-trip through
/// [`PadShape::Unknown`] rather than being silently mapped to `Round`.
///
/// `ChamferedRectangle` and `CustomShape` aren't standalone shape bytes —
/// Altium signals them via boolean flags on the pad record
/// (`has_corner_radius_chamfer` / `has_custom_chamfered_rectangle` for the
/// first, `has_custom_shapes` for the second). They surface as logical
/// variants so consumers can pattern-match on shape semantically; the
/// reader promotes a base byte + flag combination into the right variant,
/// and [`PadShape::to_i32`] folds them back to their on-disk base byte for
/// writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PadShape {
    /// Default / unset (raw byte 0).
    NoShape,
    /// Round / oval / ellipse (raw byte 1).
    Round,
    /// Rectangular (raw byte 2).
    Rectangular,
    /// Octagonal (raw byte 3).
    Octagonal,
    /// Rounded rectangle (raw byte 9). Corner radius lives in
    /// `corner_radius_percentage` or `per_layer_corner_radii`.
    RoundedRectangle,
    /// Rounded rectangle whose corners are chamfered (cut at 45°) instead
    /// of filleted. Shares the on-disk byte with `RoundedRectangle`; the
    /// distinction comes from the pad's chamfer flags.
    ChamferedRectangle,
    /// Free-form outline whose vertices live in an associated `Region6`
    /// record. Altium writes a placeholder pad (typically `Round`) and
    /// the actual shape comes from the linked region.
    CustomShape,
    /// Raw shape byte that doesn't match any known variant — preserved
    /// verbatim so it round-trips through the writer.
    Unknown(i32),
}

impl PadShape {
    /// Map a raw on-disk shape byte (as i32) to a variant. Bytes outside
    /// the known set become [`PadShape::Unknown`] rather than silently
    /// collapsing to `Round`.
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => Self::NoShape,
            1 => Self::Round,
            2 => Self::Rectangular,
            3 => Self::Octagonal,
            9 => Self::RoundedRectangle,
            other => Self::Unknown(other),
        }
    }

    /// On-disk shape byte for this variant. `ChamferedRectangle` writes
    /// the `RoundedRectangle` byte (chamfered-ness lives in the pad's
    /// flag fields); `CustomShape` writes the `Round` byte (Altium's
    /// placeholder convention).
    pub fn to_i32(self) -> i32 {
        match self {
            Self::NoShape => 0,
            Self::Round => 1,
            Self::Rectangular => 2,
            Self::Octagonal => 3,
            Self::RoundedRectangle => 9,
            Self::ChamferedRectangle => 9,
            Self::CustomShape => 1,
            Self::Unknown(v) => v,
        }
    }
}

impl Default for PadShape {
    fn default() -> Self {
        Self::Round
    }
}

impl From<PadShape> for i32 {
    fn from(shape: PadShape) -> i32 {
        shape.to_i32()
    }
}

impl TryFrom<i32> for PadShape {
    type Error = core::convert::Infallible;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(Self::from_raw(value))
    }
}

repr_enum! {
    /// Drilled hole shape for a pad.
    pub enum PadHoleType {
        Round = 0,
        Square = 1,
        Slot = 2,
    }
}

repr_enum! {
    /// Variant of PCB text rendering.
    pub enum PcbTextKind {
        Stroke = 0,
        TrueType = 1,
        BarCode = 2,
    }
}

repr_enum! {
    /// Built-in stroke font for PCB stroke text.
    pub enum PcbStrokeFont {
        Default = 0,
        SansSerif = 1,
        Serif = 3,
    }
}

repr_enum! {
    /// Anchor point for text rendering, indexed by row × column.
    ///
    /// Values 0-2 are the bottom row, 3-5 the middle, 6-8 the top.
    pub enum TextJustification {
        BottomLeft = 0,
        BottomCenter = 1,
        BottomRight = 2,
        MiddleLeft = 3,
        MiddleCenter = 4,
        MiddleRight = 5,
        TopLeft = 6,
        TopCenter = 7,
        TopRight = 8,
    }
}

repr_enum! {
    /// Schematic power port glyph.
    pub enum PowerPortStyle {
        Circle = 0,
        Arrow = 1,
        Bar = 2,
        Wave = 3,
        PowerGround = 4,
        SignalGround = 5,
        Earth = 6,
        GostArrow = 7,
        GostPowerGround = 8,
        GostEarth = 9,
        GostBar = 10,
    }
}

/// Horizontal text alignment within a render context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TextHAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical text alignment within a render context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TextVAlign {
    Top,
    Middle,
    #[default]
    Bottom,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_i32() {
        for variant in [
            PinElectricalType::Input,
            PinElectricalType::Power,
            PinElectricalType::OpenEmitter,
        ] {
            let raw: i32 = variant.into();
            assert_eq!(PinElectricalType::try_from(raw).unwrap(), variant);
        }
    }

    #[test]
    fn unknown_value_is_err() {
        // `PadShape` no longer errors on unknown bytes — it preserves them
        // via the `Unknown` variant so they round-trip through the writer.
        assert_eq!(PadShape::from_raw(99), PadShape::Unknown(99));
        assert_eq!(i32::from(PadShape::Unknown(99)), 99);
        assert_eq!(LineStyle::try_from(-1), Err(-1));
    }

    #[test]
    fn defaults() {
        assert_eq!(DiagnosticSeverity::default(), DiagnosticSeverity::Info);
        assert_eq!(LineStyle::default(), LineStyle::Solid);
        assert_eq!(PinElectricalType::default(), PinElectricalType::Input);
    }
}
