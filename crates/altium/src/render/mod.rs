//! Rendering for [`crate::pcb::Component`] and [`crate::sch::Component`].

pub mod context;
pub mod layer_colors;
pub mod overline;
pub mod pcb;
pub mod raster;
pub mod sch;
pub mod svg;
pub mod transform;

pub use context::{RenderContext, RenderOptions, TextAnchorH, TextAnchorV, TextStyle};
pub use raster::TinySkiaContext;
pub use svg::SvgContext;
pub use transform::CoordTransform;
