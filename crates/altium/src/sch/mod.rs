//! Schematic document and library data model.

pub mod binary;
pub mod codec;
pub mod component;
pub mod document;
pub mod implementation;
pub mod library;
pub mod primitives;
pub mod reader;
pub mod writer;

pub use component::{Component, RawRecord};
pub use document::Document;
pub use implementation::{Implementation, MapDefiner};
pub use library::Library;
pub use primitives::{
    Arc, Bezier, Blanket, Bus, BusEntry, Ellipse, EllipticalArc, HarnessConnector, HarnessEntry,
    HarnessType, Image, Junction, Label, Line, NetLabel, SignalHarness, NoErc, Parameter, ParameterSet, Pie, Pin, Polygon, Polyline, Port, PowerObject,
    Rectangle, RoundedRectangle, SheetEntry, SheetSymbol, Symbol, TextFrame, Wire,
};
