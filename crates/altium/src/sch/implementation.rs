//! Implementation links: connect a schematic symbol to its PCB footprint,
//! simulation model, or signal-integrity model.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::primitives::PrimitiveCommon;

/// One implementation entry on a [`super::Component`].
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Implementation {
    pub description: Option<String>,
    pub model_name: Option<String>,
    /// Typically `"PCBLIB"`, `"SIM"`, `"SI"`.
    pub model_type: Option<String>,
    pub data_file_kinds: Vec<String>,
    pub data_file_entities: Vec<String>,
    pub is_current: bool,
    /// `IntegratedModel=T`: the model lives in the same IntLib/vault item.
    #[cfg_attr(feature = "serde", serde(default))]
    pub integrated_model: bool,
    /// `DatabaseModel=T`: the model came from a database/vault library.
    #[cfg_attr(feature = "serde", serde(default))]
    pub database_model: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub model_item_guid: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub model_revision_guid: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub model_vault_guid: Option<String>,
    pub map_definers: Vec<MapDefiner>,
    pub common: PrimitiveCommon,
}

/// A single mapping from a schematic-pin designator to one or more
/// implementation-side designator names.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MapDefiner {
    pub designator_interface: Option<String>,
    pub designator_implementations: Vec<String>,
    pub is_trivial: bool,
    pub common: PrimitiveCommon,
}
