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

repr_enum! {
    /// Copper pad outline shape.
    pub enum PadShape {
        Round = 1,
        Rectangular = 2,
        Octagonal = 3,
        RoundedRectangle = 9,
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
        assert_eq!(PadShape::try_from(99), Err(99));
        assert_eq!(LineStyle::try_from(-1), Err(-1));
    }

    #[test]
    fn defaults() {
        assert_eq!(DiagnosticSeverity::default(), DiagnosticSeverity::Info);
        assert_eq!(LineStyle::default(), LineStyle::Solid);
        assert_eq!(PinElectricalType::default(), PinElectricalType::Input);
    }
}
