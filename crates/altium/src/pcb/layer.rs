//! PCB layer-stack data parsed from the `Board6` parameter dictionary.

use std::collections::BTreeMap;

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
    /// Build a stack by scanning a `Board6` parameter map for `V7_LAYERnNAME`
    /// keys and following the previous/next chain.
    pub fn from_board_parameters(parameters: &BTreeMap<String, String>) -> Option<Self> {
        let mut entries = std::collections::BTreeMap::<i32, LayerEntry>::new();
        for i in 1..=100 {
            let key_name = format!("V7_LAYER{i}NAME");
            let Some(name) = parameters.get(&key_name) else {
                continue;
            };
            let mut entry = LayerEntry {
                index: i,
                name: name.clone(),
                ..LayerEntry::default()
            };
            if let Some(prev) = parameters
                .get(&format!("V7_LAYER{i}PREV"))
                .and_then(|s| s.parse().ok())
            {
                entry.previous_index = prev;
            }
            if let Some(next) = parameters
                .get(&format!("V7_LAYER{i}NEXT"))
                .and_then(|s| s.parse().ok())
            {
                entry.next_index = next;
            }
            if let Some(cop) = parameters.get(&format!("V7_LAYER{i}COPTHICK")) {
                entry.copper_enabled = cop != "0";
            }
            if let Some(diel) = parameters.get(&format!("V7_LAYER{i}DIELTYPE")) {
                entry.dielectric_material = diel.clone();
            }
            if let Some(color) = parameters
                .get(&format!("V7_LAYER{i}COLOR"))
                .and_then(|s| s.parse().ok())
            {
                entry.color = color;
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
