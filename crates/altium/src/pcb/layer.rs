//! PCB layer-stack data parsed from the `Board6` parameter dictionary.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A single entry in the PCB layer stack.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LayerEntry {
    /// 1-based layer index.
    pub index: i32,
    pub name: String,
    /// Index of the previous layer in the chain.
    pub previous_index: i32,
    /// Index of the next layer in the chain.
    pub next_index: i32,
    pub copper_enabled: bool,
    pub dielectric_material: String,
    /// Color (Altium BGR-packed).
    pub color: i32,
}

/// Ordered top-to-bottom layer stack derived from `Board6` parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LayerStack {
    pub layers: Vec<LayerEntry>,
}

impl LayerStack {
    /// Build a stack by scanning `V7_LAYERnNAME` keys and following the
    /// previous/next chain. Duplicate keys are last-write-wins.
    pub fn from_board_parameters(parameters: &[(String, String)]) -> Option<Self> {
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
            }
            if let Some(&diel) = lookup.get(format!("V7_LAYER{i}DIELTYPE").as_str()) {
                entry.dielectric_material = diel.to_string();
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
