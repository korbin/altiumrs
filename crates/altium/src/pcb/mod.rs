//! PCB document and library data model.

pub mod binary;
pub mod board;
pub mod builders;
pub mod component;
pub(crate) mod defaults;
pub mod doc_codec;
pub mod document;
pub mod embedded;
pub(crate) mod flatten;
pub mod geometry;
pub mod guids;
pub mod layer;
pub mod library;
pub mod model3d;
pub mod padvia;
pub mod polygon;
pub mod primitives;
pub mod reader;
pub mod records;
pub mod lint;
pub mod rule;
pub mod writer;

pub use builders::{ArcBuilder, PadBuilder, TrackBuilder, ViaBuilder};
pub use component::Component;
pub use document::{CustomShapePad, Document};
pub use geometry::offset_polygon;
pub use embedded::{BoardLoader, EmbeddedBoard, FileBoardLoader};
pub use layer::{
    ComponentPlacement, DielectricType, LayerEntry, LayerStack, Permittivity, StackLayerKind,
};
pub use library::Library;
pub use board::{Opacity, ViewColor, ViewConfig2d, ViewConfig3d};
pub use model3d::Model3d;
pub use padvia::{
    PadViaLibraries, PadViaLibraryInfo, PadViaLink, PadViaTemplate, ViaFeature, ViaStructure,
    ViaTemplate, ViaTemplateLink,
};
pub use polygon::{Polygon, PolygonVertex, VertexKind};
pub use primitives::{
    Arc, ComponentBody, Fill, Net, Pad, Region, RegionKind, ShapeBasedRegion, ShapeVertex, Text,
    Track, Via,
};
pub use rule::{DifferentialPair, ObjectClass, Room, Rule};
