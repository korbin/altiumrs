//! Schematic symbol library (`.SchLib`).

use std::collections::BTreeMap;

use indexmap::IndexMap;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::component::Component;
use crate::diagnostic::Diagnostic;

/// Collection of schematic symbols.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Library {
    pub diagnostics: Vec<Diagnostic>,
    pub components: Vec<Component>,
    pub section_keys: BTreeMap<String, String>,
    /// Parameters from the root `FileHeader` parameter block. `IndexMap`
    /// preserves insertion order — Altium parses the FileHeader sequentially
    /// and the order matters (e.g. font slots must be emitted as
    /// `FontIdCount, Size1, FontName1, Size2, FontName2, ...`).
    pub file_header_parameters: IndexMap<String, String>,
    /// Override for the symbol-wide font. When `Some`, the writer emits an
    /// extra font slot (slot 2) with this name plus a rotated mirror
    /// (slot 3, `Rotation3=90`). When `None`, the FileHeader carries a
    /// single "Times New Roman" slot.
    pub font_override: Option<String>,
    /// Embedded image bytes from the `Storage` stream, in the order the file
    /// stored them. Image records reference these positionally.
    pub embedded_images: Vec<Vec<u8>>,
    /// Raw `Storage` stream bytes preserved for round-trip when we can't
    /// re-emit them losslessly.
    pub raw_storage_stream: Option<Vec<u8>>,
    /// Other root-level streams we don't model.
    pub additional_root_streams: BTreeMap<String, Vec<u8>>,
}

impl Library {
    pub fn component(&self, name: &str) -> Option<&Component> {
        self.components
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }
}
