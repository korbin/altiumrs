//! PCB design rules, object classes, differential pairs, rooms: parameter-bag
//! types from the `Rules6`, `Classes6`, `DifferentialPairs6`, and `Rooms6`
//! storages.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A design rule: clearance, width, routing topology, etc. The original
/// parameter bag is kept verbatim because rule kinds vary widely.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Rule {
    pub name: String,
    pub rule_kind: String,
    pub enabled: bool,
    pub priority: i32,
    pub comment: String,
    pub unique_id: String,
    pub scope1_expression: String,
    pub scope2_expression: String,
    pub parameters: Vec<(String, String)>,
    /// Per-record version word preceding the `u32 size` prefix; tags the
    /// `RULEKIND` enum (e.g. `UnpouredPolygon` = 62). Round-tripped verbatim.
    pub rule_type_code: u16,
}

/// A named class (net class, component class, etc.) with its members.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ObjectClass {
    pub name: String,
    pub super_class: String,
    pub sub_class: String,
    pub unique_id: String,
    pub kind: String,
    pub enabled: bool,
    pub members: Vec<String>,
    pub parameters: Vec<(String, String)>,
}

/// Pairs a positive and negative net for differential routing.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DifferentialPair {
    pub name: String,
    pub positive_net_name: String,
    pub negative_net_name: String,
    pub unique_id: String,
    pub enabled: bool,
    pub parameters: Vec<(String, String)>,
}

/// Physical placement region.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Room {
    pub name: String,
    pub unique_id: String,
    pub parameters: Vec<(String, String)>,
}
