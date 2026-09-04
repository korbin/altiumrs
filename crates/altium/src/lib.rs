//! Read and write Altium Designer files (`.SchLib`, `.SchDoc`, `.PcbLib`, `.PcbDoc`, `.BomDoc`).

extern crate self as altium;

pub mod binary;
pub mod bom;
pub mod color;
pub mod compound;
pub mod coord;
pub mod diagnostic;
pub mod dto;
pub mod encoding;
pub mod enums;
pub mod error;
pub mod file;
pub mod intlib;
pub mod libpkg;
pub mod netlist;
pub mod parameter;
pub mod pnp;
pub mod pcb;
#[cfg(feature = "render")]
pub mod render;
pub mod sch;
#[cfg(feature = "serde")]
pub mod serde_bytes;

/// Derives `from_params` / `to_params` on a struct of named fields.
pub use altium_derive::AltiumRecord;
pub use bom::{BomDocument, BomHeader, BomItem, BomRecord};
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
pub use intlib::{
    CrossRefRecord, CrossReferenceFootprint, CrossReferenceSymbol, CrossReferenceTable,
    IntegratedLibrary, NamedLibrary, flatten_cross_reference_table, parse_cross_reference,
    parse_cross_reference_table, parse_parameters_bin, serialise_cross_reference,
    serialise_parameters_bin,
};
pub use netlist::{
    NetConnection, Netlist, NetlistComponent, NetlistNet, NetlistSource, PORT_DESIGNATOR, SchNetlistOptions,
};
pub use libpkg::{DocumentReference, LibraryPackage, SplitResult};
