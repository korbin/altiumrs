//! 3D STEP model embedded in a PCB library.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A 3D model (typically STEP) referenced by [`super::ComponentBody::model_id`].
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Model3d {
    /// GUID linking back to a `ComponentBody`.
    pub id: String,
    /// Original filename (e.g. `"FOO.step"`).
    pub name: String,
    /// Whether the model bytes are embedded in the library file.
    pub is_embedded: bool,
    /// Source kind (typically `"Undefined"` for embedded models).
    pub model_source: String,
    pub rotation_x: f64,
    pub rotation_y: f64,
    pub rotation_z: f64,
    pub dz: i32,
    /// Altium's proprietary checksum (preserved verbatim for round-trip).
    pub checksum: i32,
    /// STEP text (ISO-10303-21). Decompressed on read; compressed on write.
    pub step_data: String,
}
