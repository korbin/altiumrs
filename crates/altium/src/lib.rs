//! Read and write Altium Designer files (`.SchLib`, `.SchDoc`, `.PcbLib`, `.PcbDoc`).

extern crate self as altium;

pub mod binary;
pub mod color;
pub mod compound;
pub mod coord;
pub mod diagnostic;
pub mod dto;
pub mod encoding;
pub mod enums;
pub mod error;
pub mod file;
pub mod parameter;
pub mod pcb;
#[cfg(feature = "render")]
pub mod render;
pub mod sch;

/// Derives `from_params` / `to_params` on a struct of named fields.
pub use altium_derive::AltiumRecord;
pub use color::Color;
pub use coord::{Coord, CoordPoint, CoordRect, ParseCoordError};
pub use diagnostic::Diagnostic;
pub use enums::{
    DiagnosticSeverity, LineStyle, PadHoleType, PadShape, PcbStrokeFont, PcbTextKind,
    PinElectricalType, PinOrientation, PowerPortStyle, SchLineStyle, TextHAlign, TextJustification,
    TextVAlign,
};
pub use error::{Error, Result};
pub use file::{AltiumFile, AltiumFileKind, open};
