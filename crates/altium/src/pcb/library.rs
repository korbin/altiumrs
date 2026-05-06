//! Top-level PCB library (`.PcbLib`): a collection of named footprints.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::component::Component;
use super::model3d::Model3d;
use crate::diagnostic::Diagnostic;

/// A PCB footprint library.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Library {
    pub diagnostics: Vec<Diagnostic>,

    /// 8-character random identifier Altium creates for each library.
    pub unique_id: String,

    pub components: Vec<Component>,

    /// Embedded 3D models (referenced from `ComponentBody::model_id`).
    pub models: Vec<Model3d>,

    /// Section keys mapping component name → OLE storage name (used when names
    /// exceed 31 chars or contain `/`).
    pub section_keys: BTreeMap<String, String>,

    /// Raw library-level parameter dictionary.
    pub library_parameters: Option<BTreeMap<String, String>>,

    /// Additional streams beneath `Library/` (e.g. `ComponentParamsTOC`,
    /// `EmbeddedFonts`, `PadViaLibrary`).
    pub additional_library_streams: BTreeMap<String, Vec<u8>>,
    /// Additional root-level storages (e.g. `FileVersionInfo`).
    pub additional_root_streams: BTreeMap<String, Vec<u8>>,
}

impl Library {
    /// Look up a component by its case-insensitive name.
    pub fn component(&self, name: &str) -> Option<&Component> {
        self.components
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// Set the font name on every `Text` primitive across every component.
    pub fn set_text_font(&mut self, font: impl Into<String>) {
        let font = font.into();
        for component in &mut self.components {
            for text in &mut component.texts {
                text.font_name = Some(font.clone());
            }
        }
    }
}
