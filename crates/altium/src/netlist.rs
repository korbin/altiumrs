//! Netlist extraction from `.PcbDoc` and `.SchDoc` files.
//!
//! A netlist names the electrical connections between components: every pin
//! / pad is associated with a named net, and each net lists every (component,
//! pad/pin) endpoint that's electrically tied together.
//!
//! Two data sources:
//!
//! - **`.PcbDoc`**: explicit. Each [`Pad`](crate::pcb::Pad) carries a `net`
//!   name and a designator; the netlist is just a regroup. This is the
//!   canonical, deterministic source.
//! - **`.SchDoc`**: implicit. Wires connect pin endpoints; net names come
//!   from net labels and power ports. We follow the wire graph with
//!   union-find to derive the connectivity.
//!
//! Output formats: Altium-flavoured Protel `.NET`, KiCad netlist
//! (S-expression), JSON, and tab-separated CSV.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::coord::CoordPoint;
use crate::enums::PinOrientation;
use crate::{pcb, sch};

/// One component referenced by the netlist — a single placed instance with a
/// designator (`R1`, `U2`, …), an optional footprint name, and value /
/// description metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct NetlistComponent {
    pub designator: String,
    pub footprint: Option<String>,
    pub value: Option<String>,
    pub library_reference: Option<String>,
    pub description: Option<String>,
    pub parameters: BTreeMap<String, String>,
}

/// A single endpoint on a net — `(designator, pad/pin name)`.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct NetConnection {
    pub designator: String,
    /// Pin / pad designator (`"1"`, `"GND"`, …).
    pub pad: String,
}

/// A named electrical net.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct NetlistNet {
    pub name: String,
    pub connections: Vec<NetConnection>,
}

/// Where the netlist was extracted from. Drives the small differences in
/// output formats (e.g. Protel writes `[Component]` blocks even for SchDoc
/// netlists, but value/footprint fields may be empty).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum NetlistSource {
    PcbDocument,
    SchDocument,
}

/// A complete netlist: every component, plus every electrical net with its
/// pin/pad endpoints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Netlist {
    pub source: Option<NetlistSource>,
    pub components: Vec<NetlistComponent>,
    pub nets: Vec<NetlistNet>,
}

impl Netlist {
    /// Build a netlist from a `.PcbDoc`. Walks every component's pads and
    /// every free-standing pad in the document; nets are taken from the
    /// `Pad::net` field that the reader resolves from `net_index`.
    pub fn from_pcb_document(doc: &pcb::Document) -> Self {
        let mut components = Vec::new();
        let mut nets_by_name: BTreeMap<String, Vec<NetConnection>> = BTreeMap::new();
        let mut seen_designators: BTreeSet<String> = BTreeSet::new();

        for comp in &doc.components {
            let designator = pcb_designator(comp);
            // Some PcbDoc fixtures repeat the same designator across multiple
            // component records (unlikely, but be defensive). The first wins.
            if !seen_designators.contains(&designator) {
                seen_designators.insert(designator.clone());
                components.push(NetlistComponent {
                    designator: designator.clone(),
                    footprint: comp.pattern.clone(),
                    value: comp.comment.clone(),
                    library_reference: comp.source_lib_reference.clone(),
                    description: comp.description.clone(),
                    parameters: comp.additional_parameters.clone(),
                });
            }
            for pad in &comp.pads {
                push_pad_connection(pad, &designator, &mut nets_by_name);
            }
        }

        // The reader keeps a master list at `doc.pads` AND copies each
        // component-owned pad into `comp.pads` — iterating both directly
        // would double-count every assigned pad and produce phantom
        // `?-N` connections in the netlist. Walk `doc.pads` only for the
        // pads that truly aren't assigned to a component.
        let component_count = doc.components.len() as i32;
        for pad in &doc.pads {
            let assigned = (0..component_count).contains(&pad.component_index);
            if assigned {
                continue;
            }
            push_pad_connection(pad, "?", &mut nets_by_name);
        }

        let nets = nets_by_name
            .into_iter()
            .map(|(name, mut connections)| {
                connections.sort();
                connections.dedup();
                NetlistNet { name, connections }
            })
            .collect();

        Self {
            source: Some(NetlistSource::PcbDocument),
            components,
            nets,
        }
    }

    /// Build a netlist from a `.SchDoc` by tracing the wire graph.
    ///
    /// Endpoints (pin connectors, wire vertices, junctions, net-label
    /// anchors, power-port anchors) are collected onto a position-keyed
    /// graph. Wire segments and junctions produce edges; net labels and
    /// power ports name the connected components they touch. Anything left
    /// unnamed gets an auto-generated `N00001`-style identifier.
    ///
    /// This is a pragmatic implementation that handles the common cases —
    /// not a full Altium-compatible compiler. Bus-tap inheritance, cross-
    /// sheet hierarchical ports, and pin-on-wire-mid-segment touches are
    /// not modelled.
    pub fn from_sch_document(doc: &sch::Document) -> Self {
        let mut components = Vec::new();
        let mut pin_endpoints: Vec<(String, String, CoordPoint)> = Vec::new();

        for comp in &doc.components {
            let designator = sch_designator(comp);
            components.push(NetlistComponent {
                designator: designator.clone(),
                footprint: sch_footprint(comp),
                value: comp.comment.clone(),
                library_reference: comp.lib_reference.clone().or_else(|| Some(comp.name.clone())),
                description: comp.description.clone(),
                parameters: comp
                    .parameters
                    .iter()
                    .map(|p| (p.name.clone(), p.value.clone()))
                    .collect(),
            });
            for pin in &comp.pins {
                let world = pin_world_endpoint(comp, pin);
                let pad = pin
                    .designator
                    .clone()
                    .or_else(|| pin.name.clone())
                    .unwrap_or_default();
                if !pad.is_empty() {
                    pin_endpoints.push((designator.clone(), pad, world));
                }
            }
        }

        let mut uf = UnionFind::default();

        // Each pin endpoint, junction, and net-label/power-port anchor is a
        // node. Wire segments link consecutive vertices.
        for (_, _, p) in &pin_endpoints {
            uf.add(*p);
        }
        for j in &doc.junctions {
            uf.add(j.location);
        }
        for label in &doc.net_labels {
            uf.add(label.location);
        }
        for power in &doc.power_objects {
            uf.add(power.location);
        }

        for wire in &doc.wires {
            for window in wire.vertices.windows(2) {
                uf.add(window[0]);
                uf.add(window[1]);
                uf.union(window[0], window[1]);
            }
        }

        // Bus entries connect two points by definition (wire-to-bus join).
        for be in &doc.bus_entries {
            uf.add(be.location);
            uf.add(be.corner);
            uf.union(be.location, be.corner);
        }

        // Junctions force-merge any nodes already at the junction position;
        // they only matter when the wire graph crosses but no shared vertex
        // exists. Adding the position is sufficient since adjacent nodes are
        // matched by exact CoordPoint equality.
        // (No additional union work — the graph already merged them.)

        // Pin endpoints / labels / power ports may sit at coordinates the
        // wire graph already references. Union them in.
        for (_, _, p) in &pin_endpoints {
            // No-op union to ensure the pin endpoint is reachable as a node.
            uf.add(*p);
            // If a wire vertex is at exactly this position, the union step
            // has already linked them through identical-key insertion.
        }

        // Names: net labels and power ports anchor names onto positions.
        let mut named_nets: HashMap<CoordPoint, String> = HashMap::new();
        for label in &doc.net_labels {
            let root = uf.find(label.location);
            let name = label.text.trim().to_string();
            if !name.is_empty() {
                named_nets.entry(root).or_insert(name);
            }
        }
        for power in &doc.power_objects {
            let root = uf.find(power.location);
            let name = power.text.trim().to_string();
            if !name.is_empty() {
                named_nets.entry(root).or_insert(name);
            }
        }

        // Group pin endpoints by their union-find root, then assign a name.
        // CoordPoint isn't Ord so we use HashMap; the result Vec is sorted
        // by net name afterwards for deterministic output.
        let mut nets_by_root: HashMap<CoordPoint, Vec<NetConnection>> = HashMap::new();
        for (designator, pad, p) in pin_endpoints {
            let root = uf.find(p);
            nets_by_root
                .entry(root)
                .or_default()
                .push(NetConnection { designator, pad });
        }

        // Group connections by name so power ports / net labels with the
        // same name across the sheet merge into a single net (matches how
        // Altium's compiler unifies same-named subnets). Auto-named
        // single-pin "nets" (a pin sitting on nothing) are filtered at the
        // end — they're not electrically meaningful and they bloat the
        // exported netlist with `(\nN00001\nR1-1\n)`-style blocks. Altium
        // does the same unless `NetlistSinglePinNets=1`.
        let mut by_name: BTreeMap<String, Vec<NetConnection>> = BTreeMap::new();
        let mut auto_named: BTreeSet<String> = BTreeSet::new();
        let mut auto_index: usize = 0;
        for (root, connections) in nets_by_root {
            let (name, is_auto) = match named_nets.remove(&root) {
                Some(n) => (n, false),
                None => {
                    auto_index += 1;
                    (format!("N{auto_index:05}"), true)
                }
            };
            if is_auto {
                auto_named.insert(name.clone());
            }
            by_name.entry(name).or_default().extend(connections);
        }
        let mut nets: Vec<NetlistNet> = by_name
            .into_iter()
            .filter_map(|(name, mut connections)| {
                connections.sort();
                connections.dedup();
                if auto_named.contains(&name) && connections.len() < 2 {
                    return None;
                }
                Some(NetlistNet { name, connections })
            })
            .collect();
        nets.sort_by(|a, b| a.name.cmp(&b.name));

        Self {
            source: Some(NetlistSource::SchDocument),
            components,
            nets,
        }
    }

    // ─── Output formats ─────────────────────────────────────────────────────

    /// Render as the Altium-flavoured Protel `.NET` text format.
    ///
    /// Output uses CRLF line terminators to match what Altium Designer
    /// itself writes — many Windows viewers (Notepad, Altium's own
    /// importer) treat LF-only files as a single long line.
    ///
    /// ```text
    /// [
    /// R1
    /// 0805
    /// 10K
    /// ]
    /// ...
    /// (
    /// VCC
    /// R1-1
    /// C2-2
    /// )
    /// ```
    ///
    /// Each `[...]` block is one component (designator / footprint /
    /// comment / description). Each `(...)` block is one electrical net
    /// (name on the second line, then `Designator-Pad` lines).
    pub fn to_protel(&self) -> String {
        let mut out = String::new();
        for comp in &self.components {
            out.push_str("[\r\n");
            out.push_str(&comp.designator);
            out.push_str("\r\n");
            out.push_str(comp.footprint.as_deref().unwrap_or(""));
            out.push_str("\r\n");
            out.push_str(comp.value.as_deref().unwrap_or(""));
            out.push_str("\r\n");
            out.push_str(comp.description.as_deref().unwrap_or(""));
            out.push_str("\r\n");
            out.push_str("]\r\n");
        }
        for net in &self.nets {
            out.push_str("(\r\n");
            out.push_str(&net.name);
            out.push_str("\r\n");
            for conn in &net.connections {
                out.push_str(&conn.designator);
                out.push('-');
                out.push_str(&conn.pad);
                out.push_str("\r\n");
            }
            out.push_str(")\r\n");
        }
        out
    }

    /// Render as KiCad legacy netlist (S-expression). Suitable for import
    /// via `pcbnew --netlist <file>`.
    pub fn to_kicad(&self) -> String {
        let mut out = String::new();
        out.push_str("(export (version D)\n");
        out.push_str("  (components\n");
        for comp in &self.components {
            let _ = writeln!(
                out,
                "    (comp (ref {})\n      (value {})\n      (footprint {}))",
                kicad_quote(&comp.designator),
                kicad_quote(comp.value.as_deref().unwrap_or("")),
                kicad_quote(comp.footprint.as_deref().unwrap_or("")),
            );
        }
        out.push_str("  )\n");
        out.push_str("  (nets\n");
        for (i, net) in self.nets.iter().enumerate() {
            let _ = writeln!(
                out,
                "    (net (code {}) (name {})",
                i + 1,
                kicad_quote(&net.name)
            );
            for conn in &net.connections {
                let _ = writeln!(
                    out,
                    "      (node (ref {}) (pin {}))",
                    kicad_quote(&conn.designator),
                    kicad_quote(&conn.pad),
                );
            }
            out.push_str("    )\n");
        }
        out.push_str("  )\n");
        out.push(')');
        out.push('\n');
        out
    }

    /// Render as a hand-rolled JSON string. The output mirrors the field
    /// names of the Rust types so it can be consumed by any JSON-aware
    /// downstream tool without pulling `serde_json` into this crate as a
    /// non-dev dependency.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        out.push_str("  \"source\": ");
        match self.source {
            Some(NetlistSource::PcbDocument) => out.push_str("\"PcbDocument\""),
            Some(NetlistSource::SchDocument) => out.push_str("\"SchDocument\""),
            None => out.push_str("null"),
        }
        out.push_str(",\n  \"components\": [");
        for (i, comp) in self.components.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("\n    {");
            json_field(&mut out, "designator", &comp.designator, true);
            json_opt_field(&mut out, "footprint", comp.footprint.as_deref());
            json_opt_field(&mut out, "value", comp.value.as_deref());
            json_opt_field(&mut out, "library_reference", comp.library_reference.as_deref());
            json_opt_field(&mut out, "description", comp.description.as_deref());
            out.push_str("\n    }");
        }
        out.push_str("\n  ],\n  \"nets\": [");
        for (i, net) in self.nets.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("\n    {");
            json_field(&mut out, "name", &net.name, true);
            out.push_str(",\n      \"connections\": [");
            for (j, conn) in net.connections.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push_str("\n        {");
                json_field(&mut out, "designator", &conn.designator, true);
                json_field(&mut out, "pad", &conn.pad, false);
                out.push('}');
            }
            out.push_str("\n      ]");
            out.push_str("\n    }");
        }
        out.push_str("\n  ]\n}\n");
        out
    }

    /// Render as tab-separated CSV with one row per `(designator, pad, net)`.
    /// Header row: `Net\tDesignator\tPad`.
    pub fn to_csv(&self) -> String {
        let mut out = String::from("Net\tDesignator\tPad\n");
        for net in &self.nets {
            for conn in &net.connections {
                let _ = writeln!(
                    out,
                    "{}\t{}\t{}",
                    csv_escape(&net.name),
                    csv_escape(&conn.designator),
                    csv_escape(&conn.pad),
                );
            }
        }
        out
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn push_pad_connection(
    pad: &pcb::Pad,
    designator: &str,
    nets_by_name: &mut BTreeMap<String, Vec<NetConnection>>,
) {
    let net_name = match pad.net.as_deref() {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => return, // unconnected pad — skip
    };
    let pad_name = pad.designator.clone().unwrap_or_default();
    if pad_name.is_empty() {
        return;
    }
    nets_by_name.entry(net_name).or_default().push(NetConnection {
        designator: designator.to_string(),
        pad: pad_name,
    });
}

fn pcb_designator(comp: &pcb::Component) -> String {
    comp.source_designator
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| comp.name.clone())
}

fn sch_designator(comp: &sch::Component) -> String {
    // Schematic components carry their designator as a `Designator` parameter
    // when placed on a sheet. Library components fall back to the lib-ref
    // prefix + ?.
    for p in &comp.parameters {
        if p.name.eq_ignore_ascii_case("Designator") && !p.value.is_empty() {
            return p.value.clone();
        }
    }
    let prefix = comp.designator_prefix.clone().unwrap_or_default();
    if prefix.is_empty() {
        comp.name.clone()
    } else {
        format!("{prefix}?")
    }
}

fn sch_footprint(comp: &sch::Component) -> Option<String> {
    // The first Implementation entry's name is conventionally the footprint;
    // some files also store it as a parameter.
    for p in &comp.parameters {
        if p.name.eq_ignore_ascii_case("Footprint") && !p.value.is_empty() {
            return Some(p.value.clone());
        }
    }
    comp.implementations
        .first()
        .and_then(|i| i.model_name.clone())
}

fn json_field(out: &mut String, name: &str, value: &str, leading_comma: bool) {
    if leading_comma {
        out.push_str("\n      ");
    } else {
        out.push_str(",\n      ");
    }
    let _ = write!(out, "\"{}\": \"{}\"", name, json_escape(value));
}

fn json_opt_field(out: &mut String, name: &str, value: Option<&str>) {
    match value {
        Some(v) => {
            out.push_str(",\n      ");
            let _ = write!(out, "\"{}\": \"{}\"", name, json_escape(v));
        }
        None => {
            out.push_str(",\n      ");
            let _ = write!(out, "\"{name}\": null");
        }
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn pin_world_endpoint(comp: &sch::Component, pin: &sch::primitives::Pin) -> CoordPoint {
    // SchDoc pin locations are already in world coordinates — Altium bakes
    // the placement transform into each pin record on the way out. The
    // pin's `location` field is the body-side endpoint of the pin; the
    // wire-side endpoint (where wires actually connect) is offset by
    // `length` along the pin's orientation. We assume `pin.orientation`
    // already accounts for any component rotation/mirroring; if a future
    // fixture surfaces a SchLib-style pin (local coords) the heuristic
    // would need to combine `comp.location/orientation/is_mirrored` here.
    let _ = comp;
    let len = pin.length;
    match pin.orientation {
        PinOrientation::Right => CoordPoint::new(pin.location.x + len, pin.location.y),
        PinOrientation::Up => CoordPoint::new(pin.location.x, pin.location.y + len),
        PinOrientation::Left => CoordPoint::new(pin.location.x - len, pin.location.y),
        PinOrientation::Down => CoordPoint::new(pin.location.x, pin.location.y - len),
    }
}

fn kicad_quote(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quotes =
        s.chars().any(|c| c.is_whitespace() || matches!(c, '(' | ')' | '"' | '\\'));
    if needs_quotes {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn csv_escape(s: &str) -> String {
    s.replace(['\t', '\n'], " ")
}

// ─── Union-find on CoordPoint positions ────────────────────────────────────

#[derive(Default, Debug)]
struct UnionFind {
    parent: HashMap<CoordPoint, CoordPoint>,
}

impl UnionFind {
    fn add(&mut self, p: CoordPoint) {
        self.parent.entry(p).or_insert(p);
    }

    fn find(&mut self, p: CoordPoint) -> CoordPoint {
        // Iterative with path compression.
        let mut current = p;
        let mut steps = Vec::new();
        loop {
            let parent = match self.parent.get(&current).copied() {
                Some(x) => x,
                None => {
                    self.parent.insert(current, current);
                    current
                }
            };
            if parent == current {
                break;
            }
            steps.push(current);
            current = parent;
        }
        for step in steps {
            self.parent.insert(step, current);
        }
        current
    }

    fn union(&mut self, a: CoordPoint, b: CoordPoint) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        // Bias toward the lexicographically smaller root for determinism.
        let (parent, child) = if (ra.x.to_raw(), ra.y.to_raw()) < (rb.x.to_raw(), rb.y.to_raw()) {
            (ra, rb)
        } else {
            (rb, ra)
        };
        self.parent.insert(child, parent);
    }
}


#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::coord::Coord;
    use crate::pcb::Pad;
    use crate::sch::primitives::{NetLabel, Pin, PowerObject, Wire};

    fn coord_pt(x: f64, y: f64) -> CoordPoint {
        CoordPoint::new(Coord::from_mils(x), Coord::from_mils(y))
    }

    #[test]
    fn pcbdoc_netlist_groups_pads_by_net() {
        let mut doc = pcb::Document::default();
        let mut comp = pcb::Component::new("R0402");
        comp.source_designator = Some("R1".into());
        comp.pattern = Some("R0402".into());

        let mut p1 = Pad::default();
        p1.designator = Some("1".into());
        p1.net = Some("VCC".into());
        comp.pads.push(p1);
        let mut p2 = Pad::default();
        p2.designator = Some("2".into());
        p2.net = Some("GND".into());
        comp.pads.push(p2);

        doc.components.push(comp);
        let nl = Netlist::from_pcb_document(&doc);
        assert_eq!(nl.components.len(), 1);
        assert_eq!(nl.components[0].designator, "R1");
        assert_eq!(nl.nets.len(), 2);
        let names: Vec<&str> = nl.nets.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"VCC"));
        assert!(names.contains(&"GND"));
        let vcc = nl.nets.iter().find(|n| n.name == "VCC").unwrap();
        assert_eq!(vcc.connections.len(), 1);
        assert_eq!(vcc.connections[0].designator, "R1");
        assert_eq!(vcc.connections[0].pad, "1");
    }

    #[test]
    fn pcbdoc_netlist_does_not_double_count_assigned_pads() {
        // The PcbDoc reader keeps every pad on `doc.pads` AND clones the
        // assigned ones into `comp.pads`. The netlist extractor must skip
        // the doc-level entry whose `component_index` resolves to a real
        // component — otherwise each pad shows up twice (once with its
        // real designator, once as `?`).
        let mut doc = pcb::Document::default();
        let mut comp = pcb::Component::new("R0402");
        comp.source_designator = Some("R1".into());

        // Two pads both linked to component_index=0 (R1).
        let mut p1 = Pad::default();
        p1.designator = Some("1".into());
        p1.net = Some("VCC".into());
        p1.component_index = 0;
        let mut p2 = Pad::default();
        p2.designator = Some("2".into());
        p2.net = Some("GND".into());
        p2.component_index = 0;

        // Mimic the reader's layout: pads exist on both `doc.pads` and
        // `comp.pads`.
        comp.pads.push(p1.clone());
        comp.pads.push(p2.clone());
        doc.pads.push(p1);
        doc.pads.push(p2);
        doc.components.push(comp);

        let nl = Netlist::from_pcb_document(&doc);
        let vcc = nl.nets.iter().find(|n| n.name == "VCC").unwrap();
        let gnd = nl.nets.iter().find(|n| n.name == "GND").unwrap();
        assert_eq!(
            vcc.connections,
            vec![NetConnection {
                designator: "R1".into(),
                pad: "1".into(),
            }],
            "pad 1 must appear exactly once on VCC"
        );
        assert_eq!(
            gnd.connections,
            vec![NetConnection {
                designator: "R1".into(),
                pad: "2".into(),
            }],
            "pad 2 must appear exactly once on GND"
        );
        assert!(
            !nl.nets
                .iter()
                .any(|n| n.connections.iter().any(|c| c.designator == "?")),
            "no phantom ?-N connections should appear"
        );
    }

    #[test]
    fn pcbdoc_netlist_keeps_truly_unassigned_pads_as_question_mark() {
        // A pad whose component_index points beyond the components list is
        // truly free-standing and should still surface as `?-N`.
        let mut doc = pcb::Document::default();
        doc.components.push(pcb::Component::new("R0402"));
        let mut floater = Pad::default();
        floater.designator = Some("99".into());
        floater.net = Some("VBUS".into());
        floater.component_index = 42; // way past the single component
        doc.pads.push(floater);

        let nl = Netlist::from_pcb_document(&doc);
        let vbus = nl.nets.iter().find(|n| n.name == "VBUS").unwrap();
        assert_eq!(vbus.connections.len(), 1);
        assert_eq!(vbus.connections[0].designator, "?");
        assert_eq!(vbus.connections[0].pad, "99");
    }

    #[test]
    fn pcbdoc_netlist_skips_pads_without_net_or_designator() {
        let mut doc = pcb::Document::default();
        let mut comp = pcb::Component::new("R0402");
        comp.source_designator = Some("R1".into());
        let mut floating = Pad::default();
        floating.designator = Some("1".into()); // no net
        comp.pads.push(floating);
        let mut anonymous = Pad::default();
        anonymous.net = Some("VCC".into()); // no designator
        comp.pads.push(anonymous);
        let mut connected = Pad::default();
        connected.designator = Some("3".into());
        connected.net = Some("VCC".into());
        comp.pads.push(connected);
        doc.components.push(comp);

        let nl = Netlist::from_pcb_document(&doc);
        assert_eq!(nl.nets.len(), 1, "only VCC; floating + anonymous skipped");
        assert_eq!(nl.nets[0].connections.len(), 1);
        assert_eq!(nl.nets[0].connections[0].pad, "3");
    }

    /// Build a placed schematic component with pin locations in world space.
    /// SchDoc records bake placement into the pin location, so we mirror that
    /// in the tests.
    fn placed(designator: &str, lib_ref: &str, pins: Vec<Pin>) -> sch::Component {
        let mut comp = sch::Component::new(lib_ref);
        comp.parameters.push(sch::primitives::Parameter {
            name: "Designator".into(),
            value: designator.into(),
            ..Default::default()
        });
        comp.pins = pins;
        comp
    }

    fn pin_at(designator: &str, world_loc: CoordPoint, dir: PinOrientation, len_mils: f64) -> Pin {
        let mut p = Pin::default();
        p.designator = Some(designator.into());
        p.location = world_loc;
        p.length = Coord::from_mils(len_mils);
        p.orientation = dir;
        p
    }

    #[test]
    fn schdoc_netlist_traces_wire_between_two_pins() {
        // Two pins placed at world coords (0,0) and (50,0). With orientations
        // Right and Left and length 10, their wire-side endpoints are at
        // (10,0) and (40,0). A wire connects them.
        let mut doc = sch::Document::default();
        doc.components.push(placed(
            "R1",
            "R0402",
            vec![pin_at(
                "1",
                coord_pt(0.0, 0.0),
                PinOrientation::Right,
                10.0,
            )],
        ));
        doc.components.push(placed(
            "R2",
            "R0402",
            vec![pin_at(
                "1",
                coord_pt(50.0, 0.0),
                PinOrientation::Left,
                10.0,
            )],
        ));

        let mut w = Wire::default();
        w.vertices = vec![coord_pt(10.0, 0.0), coord_pt(40.0, 0.0)];
        doc.wires.push(w);

        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].connections.len(), 2);
        let designators: Vec<&str> = nl
            .nets[0]
            .connections
            .iter()
            .map(|c| c.designator.as_str())
            .collect();
        assert!(designators.contains(&"R1"));
        assert!(designators.contains(&"R2"));
    }

    #[test]
    fn schdoc_netlist_uses_label_text_as_net_name() {
        let mut doc = sch::Document::default();
        doc.components.push(placed(
            "R1",
            "R0402",
            vec![pin_at(
                "1",
                coord_pt(0.0, 0.0),
                PinOrientation::Right,
                10.0,
            )],
        ));
        let mut label = NetLabel::default();
        label.text = "VCC".into();
        label.location = coord_pt(10.0, 0.0); // pin endpoint
        doc.net_labels.push(label);

        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "VCC");
        assert_eq!(nl.nets[0].connections[0].designator, "R1");
    }

    #[test]
    fn schdoc_netlist_uses_power_object_text_as_net_name() {
        let mut doc = sch::Document::default();
        doc.components.push(placed(
            "R1",
            "R0402",
            vec![pin_at(
                "1",
                coord_pt(0.0, 0.0),
                PinOrientation::Right,
                10.0,
            )],
        ));
        let mut power = PowerObject::default();
        power.text = "GND".into();
        power.location = coord_pt(10.0, 0.0);
        doc.power_objects.push(power);

        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "GND");
    }

    #[test]
    fn protel_format_smoke() {
        let nl = Netlist {
            source: Some(NetlistSource::PcbDocument),
            components: vec![NetlistComponent {
                designator: "R1".into(),
                footprint: Some("0805".into()),
                value: Some("10K".into()),
                description: Some("res".into()),
                ..Default::default()
            }],
            nets: vec![NetlistNet {
                name: "VCC".into(),
                connections: vec![
                    NetConnection {
                        designator: "R1".into(),
                        pad: "1".into(),
                    },
                    NetConnection {
                        designator: "C2".into(),
                        pad: "1".into(),
                    },
                ],
            }],
        };
        let out = nl.to_protel();
        // Altium-compatible Protel netlists use CRLF; many Windows viewers
        // (Notepad, Altium's importer) treat LF-only files as a single
        // long line.
        assert!(out.contains("[\r\nR1\r\n0805\r\n10K\r\nres\r\n]\r\n"));
        assert!(out.contains("(\r\nVCC\r\nR1-1\r\nC2-1\r\n)\r\n"));
        assert!(
            !out.contains('\n')
                || out
                    .as_bytes()
                    .windows(2)
                    .all(|w| w[1] != b'\n' || w[0] == b'\r'),
            "every LF must be preceded by CR"
        );
    }

    #[test]
    fn kicad_format_smoke() {
        let nl = Netlist {
            source: Some(NetlistSource::PcbDocument),
            components: vec![NetlistComponent {
                designator: "R1".into(),
                footprint: Some("0805".into()),
                value: Some("10K".into()),
                ..Default::default()
            }],
            nets: vec![NetlistNet {
                name: "VCC".into(),
                connections: vec![NetConnection {
                    designator: "R1".into(),
                    pad: "1".into(),
                }],
            }],
        };
        let out = nl.to_kicad();
        assert!(out.starts_with("(export (version D)"));
        assert!(out.contains("(comp (ref R1)"));
        assert!(out.contains("(net (code 1) (name VCC)"));
        assert!(out.contains("(node (ref R1) (pin 1))"));
    }

    #[test]
    fn csv_format_round_trips_through_split() {
        let nl = Netlist {
            source: Some(NetlistSource::PcbDocument),
            components: vec![],
            nets: vec![NetlistNet {
                name: "VCC".into(),
                connections: vec![
                    NetConnection {
                        designator: "R1".into(),
                        pad: "1".into(),
                    },
                    NetConnection {
                        designator: "C1".into(),
                        pad: "2".into(),
                    },
                ],
            }],
        };
        let csv = nl.to_csv();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "Net\tDesignator\tPad");
        assert_eq!(lines[1], "VCC\tR1\t1");
        assert_eq!(lines[2], "VCC\tC1\t2");
    }

    #[test]
    fn kicad_quote_escapes_special_chars() {
        assert_eq!(kicad_quote(""), "\"\"");
        assert_eq!(kicad_quote("R1"), "R1");
        assert_eq!(kicad_quote("Net Name"), "\"Net Name\"");
        assert_eq!(kicad_quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(kicad_quote("a\\b"), "\"a\\\\b\"");
    }

    #[test]
    fn union_find_groups_connected_points() {
        let mut uf = UnionFind::default();
        let a = coord_pt(0.0, 0.0);
        let b = coord_pt(10.0, 0.0);
        let c = coord_pt(20.0, 0.0);
        let d = coord_pt(100.0, 100.0);
        uf.add(a);
        uf.add(b);
        uf.add(c);
        uf.add(d);
        uf.union(a, b);
        uf.union(b, c);
        // a, b, c share a root; d is separate.
        assert_eq!(uf.find(a), uf.find(b));
        assert_eq!(uf.find(a), uf.find(c));
        assert_ne!(uf.find(a), uf.find(d));
    }
}
