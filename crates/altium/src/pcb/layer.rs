//! PCB layer-stack data parsed from the `Board6` parameter dictionary.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::coord::Coord;

/// What a stack layer is, by its layer id.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum StackLayerKind {
    /// A copper layer: signal (`0x0100_xxxx`) or internal plane
    /// (`0x0101_xxxx`).
    Copper,
    /// A dielectric (core or prepreg): `0x0104_xxxx`.
    Dielectric,
    /// A solder mask: `0x0103_000A` top, `0x0103_000B` bottom.
    SolderMask,
    /// A paste mask: `0x0103_0008` top, `0x0103_0009` bottom.
    Paste,
    /// A silkscreen (overlay): `0x0103_0006` top, `0x0103_0007` bottom.
    Overlay,
    /// Any other layer, or a stack read from the V7 keys (which carry no
    /// layer id).
    #[default]
    Other,
}

impl StackLayerKind {
    /// The kind a V9 `LAYERID` names.
    pub fn of_layer_id(id: i64) -> Self {
        match id {
            0x0100_0000..=0x0101_FFFF => Self::Copper,
            0x0104_0000..=0x0104_FFFF => Self::Dielectric,
            0x0103_000A | 0x0103_000B => Self::SolderMask,
            0x0103_0008 | 0x0103_0009 => Self::Paste,
            0x0103_0006 | 0x0103_0007 => Self::Overlay,
            _ => Self::Other,
        }
    }
}

/// A dielectric constant as the file writes it (`DIELCONST`, e.g. `4.310`),
/// in thousandths: exact to the three digits Altium writes, rounded half
/// away from zero past them. An integer, so a stack compares and hashes
/// exactly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Permittivity(pub i32);

impl Permittivity {
    /// The permittivity a decimal text states (`4.31`, `-0.5`, `4`);
    /// `None` for text that is not a decimal number.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty() && fraction.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let whole: i64 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
        let mut thousandths: i64 = 0;
        let mut digits = fraction.bytes().map(|b| i64::from(b - b'0'));
        for _ in 0..3 {
            thousandths = thousandths * 10 + digits.next().unwrap_or(0);
        }
        let round_up = digits.next().is_some_and(|d| d >= 5);
        let magnitude = whole.checked_mul(1000)?.checked_add(thousandths + i64::from(round_up))?;
        i32::try_from(if negative { -magnitude } else { magnitude }).ok().map(Self)
    }

    /// Its value.
    pub fn to_f64(self) -> f64 {
        f64::from(self.0) / 1000.0
    }
}

/// A dielectric's type (`DIELTYPE`), as Altium's stack manager writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DielectricType {
    /// No dielectric (0).
    None,
    /// A core (1).
    Core,
    /// A prepreg (2).
    Prepreg,
    /// A solder mask or coverlay (3).
    SolderMask,
    /// Another stored value, kept verbatim.
    Unknown(i32),
}

impl DielectricType {
    /// The type a stored `DIELTYPE` integer names.
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Core,
            2 => Self::Prepreg,
            3 => Self::SolderMask,
            other => Self::Unknown(other),
        }
    }
}

/// Which side components are placed on from a copper layer
/// (`COMPONENTPLACEMENT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ComponentPlacement {
    /// None (0).
    None,
    /// From the top (1).
    Top,
    /// From the bottom (2).
    Bottom,
    /// Another stored value, kept verbatim.
    Unknown(i32),
}

impl ComponentPlacement {
    /// The placement a stored integer names.
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Top,
            2 => Self::Bottom,
            other => Self::Unknown(other),
        }
    }
}

/// A single entry in the PCB layer stack.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LayerEntry {
    /// 1-based layer index (V7), or the 0-based stack position (V9).
    pub index: i32,
    pub name: String,
    /// Index of the previous layer in the chain (V7 only).
    pub previous_index: i32,
    /// Index of the next layer in the chain (V7 only).
    pub next_index: i32,
    /// Whether the layer states a copper thickness other than `0`.
    pub copper_enabled: bool,
    /// The dielectric's material (`DIELMATERIAL`, e.g. `S1000-2M`, or a
    /// mask's `SM-002`); empty where none is stated.
    pub dielectric_material: String,
    /// Color (Altium BGR-packed).
    pub color: i32,
    /// The V9 `LAYERID` (e.g. `0x0100_0001` top copper); `0` for a V7 stack.
    pub layer_id: i64,
    /// What the layer is, by its id ([`StackLayerKind::Other`] for V7).
    pub kind: StackLayerKind,
    /// The copper's thickness (`COPTHICK`), where stated.
    pub copper_thickness: Option<Coord>,
    /// The dielectric's (or a mask's) thickness (`DIELHEIGHT`), where stated.
    pub dielectric_height: Option<Coord>,
    /// The dielectric's type (`DIELTYPE`), where stated.
    pub dielectric_type: Option<DielectricType>,
    /// The dielectric constant (`DIELCONST`), where stated.
    pub dielectric_constant: Option<Permittivity>,
    /// Whether any primitive is on the layer (`USEDBYPRIMS`), where stated.
    pub used_by_primitives: Option<bool>,
    /// Which side components are placed on from this copper
    /// (`COMPONENTPLACEMENT`), where stated.
    pub component_placement: Option<ComponentPlacement>,
}

/// Ordered top-to-bottom layer stack derived from `Board6` parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LayerStack {
    pub layers: Vec<LayerEntry>,
}

impl LayerStack {
    /// Build a stack from the `Board6` parameters: the V9 stack
    /// (`V9_STACK_LAYER{n}_*`, top first, as Altium 18 and later write it)
    /// where the file has one, else the V7 chain ([`Self::from_v7_parameters`]).
    pub fn from_board_parameters(parameters: &[(String, String)]) -> Option<Self> {
        Self::from_v9_parameters(parameters).or_else(|| Self::from_v7_parameters(parameters))
    }

    /// Build a stack from the V9 keys `V9_STACK_LAYER{n}_NAME`, `_LAYERID`,
    /// `_COPTHICK`, `_DIELTYPE`, `_DIELCONST`, `_DIELHEIGHT`,
    /// `_DIELMATERIAL`, `_USEDBYPRIMS` and `_COMPONENTPLACEMENT`, `n` from
    /// 0 up while a name is stated, top first. The first value of each key
    /// is read (a key a sub-stack repeats with a `{GUID}` infix is another
    /// key). A value that does not parse is left unstated.
    pub fn from_v9_parameters(parameters: &[(String, String)]) -> Option<Self> {
        let mut lookup: std::collections::HashMap<&str, &str> =
            std::collections::HashMap::new();
        for (k, v) in parameters {
            lookup.entry(k.as_str()).or_insert(v.as_str());
        }
        let mut layers = Vec::new();
        for n in 0.. {
            let get = |field: &str| lookup.get(format!("V9_STACK_LAYER{n}_{field}").as_str()).copied();
            let Some(name) = get("NAME") else {
                break;
            };
            let layer_id = get("LAYERID").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
            let coord = |field: &str| get(field).and_then(|v| Coord::parse_altium(v).ok());
            let copper_thickness = coord("COPTHICK");
            layers.push(LayerEntry {
                index: n,
                name: name.to_string(),
                previous_index: 0,
                next_index: 0,
                copper_enabled: copper_thickness.is_some_and(|t| t != Coord::ZERO),
                dielectric_material: get("DIELMATERIAL").unwrap_or_default().to_string(),
                color: get("COLOR").and_then(|v| v.trim().parse().ok()).unwrap_or(0),
                layer_id,
                kind: StackLayerKind::of_layer_id(layer_id),
                copper_thickness,
                dielectric_height: coord("DIELHEIGHT"),
                dielectric_type: get("DIELTYPE")
                    .and_then(|v| v.trim().parse().ok())
                    .map(DielectricType::from_raw),
                dielectric_constant: get("DIELCONST").and_then(Permittivity::parse),
                used_by_primitives: get("USEDBYPRIMS").map(|v| v.eq_ignore_ascii_case("TRUE")),
                component_placement: get("COMPONENTPLACEMENT")
                    .and_then(|v| v.trim().parse().ok())
                    .map(ComponentPlacement::from_raw),
            });
        }
        (!layers.is_empty()).then_some(Self { layers })
    }

    /// Build a stack by scanning `V7_LAYERnNAME` keys and following the
    /// previous/next chain. Duplicate keys are last-write-wins.
    pub fn from_v7_parameters(parameters: &[(String, String)]) -> Option<Self> {
        let mut lookup: std::collections::HashMap<&str, &str> =
            std::collections::HashMap::new();
        for (k, v) in parameters {
            lookup.insert(k.as_str(), v.as_str());
        }
        let mut entries = std::collections::BTreeMap::<i32, LayerEntry>::new();
        for i in 1..=100 {
            let key_name = format!("V7_LAYER{i}NAME");
            let Some(&name) = lookup.get(key_name.as_str()) else {
                continue;
            };
            let mut entry = LayerEntry {
                index: i,
                name: name.to_string(),
                ..LayerEntry::default()
            };
            if let Some(&prev) = lookup.get(format!("V7_LAYER{i}PREV").as_str()) {
                if let Ok(v) = prev.parse() {
                    entry.previous_index = v;
                }
            }
            if let Some(&next) = lookup.get(format!("V7_LAYER{i}NEXT").as_str()) {
                if let Ok(v) = next.parse() {
                    entry.next_index = v;
                }
            }
            if let Some(&cop) = lookup.get(format!("V7_LAYER{i}COPTHICK").as_str()) {
                entry.copper_enabled = cop != "0";
                entry.copper_thickness = Coord::parse_altium(cop).ok();
            }
            if let Some(&diel) = lookup.get(format!("V7_LAYER{i}DIELTYPE").as_str()) {
                entry.dielectric_type = diel.trim().parse().ok().map(DielectricType::from_raw);
            }
            if let Some(&material) = lookup.get(format!("V7_LAYER{i}DIELMATERIAL").as_str()) {
                entry.dielectric_material = material.to_string();
            }
            if let Some(&height) = lookup.get(format!("V7_LAYER{i}DIELHEIGHT").as_str()) {
                entry.dielectric_height = Coord::parse_altium(height).ok();
            }
            if let Some(&constant) = lookup.get(format!("V7_LAYER{i}DIELCONST").as_str()) {
                entry.dielectric_constant = Permittivity::parse(constant);
            }
            if let Some(&color) = lookup.get(format!("V7_LAYER{i}COLOR").as_str()) {
                if let Ok(v) = color.parse() {
                    entry.color = v;
                }
            }
            entries.insert(i, entry);
        }
        if entries.is_empty() {
            return None;
        }

        // Walk the chain starting from the entry with no valid predecessor.
        let first = entries
            .values()
            .find(|e| e.previous_index == 0 || !entries.contains_key(&e.previous_index))?
            .clone();

        let mut ordered = Vec::with_capacity(entries.len());
        let mut current = Some(first);
        let mut visited = std::collections::HashSet::new();
        while let Some(entry) = current {
            if !visited.insert(entry.index) {
                break;
            }
            let next = entries.get(&entry.next_index).cloned();
            ordered.push(entry);
            current = next;
        }
        // Append any orphaned entries we didn't reach via the chain.
        if ordered.len() < entries.len() {
            for entry in entries.into_values() {
                if !visited.contains(&entry.index) {
                    ordered.push(entry);
                }
            }
        }

        Some(Self { layers: ordered })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_permittivity_is_exact_to_three_digits() {
        assert_eq!(Permittivity::parse("4.310"), Some(Permittivity(4310)));
        assert_eq!(Permittivity::parse("4.31"), Some(Permittivity(4310)));
        assert_eq!(Permittivity::parse("4"), Some(Permittivity(4000)));
        assert_eq!(Permittivity::parse("3.0005"), Some(Permittivity(3001)));
        assert_eq!(Permittivity::parse("3.0004"), Some(Permittivity(3000)));
        assert_eq!(Permittivity::parse("-0.0005"), Some(Permittivity(-1)));
        assert_eq!(Permittivity::parse("four"), None);
        assert_eq!(Permittivity::parse("."), None);
    }

    #[test]
    fn a_v9_stack_reads_its_layers_top_first() {
        let p: Vec<(String, String)> = [
            ("V9_STACK_LAYER0_NAME", "Top Solder"),
            ("V9_STACK_LAYER0_LAYERID", "16973834"),
            ("V9_STACK_LAYER0_DIELTYPE", "3"),
            ("V9_STACK_LAYER0_DIELHEIGHT", "1mil"),
            ("V9_STACK_LAYER1_NAME", "Top Layer"),
            ("V9_STACK_LAYER1_LAYERID", "16777217"),
            ("V9_STACK_LAYER1_COPTHICK", "1.378mil"),
            ("V9_STACK_LAYER1_COMPONENTPLACEMENT", "1"),
            ("V9_STACK_LAYER2_NAME", "Core"),
            ("V9_STACK_LAYER2_LAYERID", "17039362"),
            ("V9_STACK_LAYER2_DIELTYPE", "1"),
            ("V9_STACK_LAYER2_DIELCONST", "4.310"),
            ("V9_STACK_LAYER2_DIELMATERIAL", "FR-4"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
        let stack = LayerStack::from_board_parameters(&p).unwrap();
        assert_eq!(stack.layers.len(), 3);
        assert_eq!(stack.layers[0].kind, StackLayerKind::SolderMask);
        assert_eq!(stack.layers[0].dielectric_type, Some(DielectricType::SolderMask));
        assert_eq!(stack.layers[1].copper_thickness, Some(Coord::from_mils(1.378)));
        assert_eq!(stack.layers[1].component_placement, Some(ComponentPlacement::Top));
        assert_eq!(stack.layers[2].dielectric_constant, Some(Permittivity(4310)));
        assert_eq!(stack.layers[2].dielectric_material, "FR-4");
        assert_eq!(stack.clone(), stack);
    }
}
