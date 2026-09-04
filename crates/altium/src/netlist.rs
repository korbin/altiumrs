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

use crate::coord::{Coord, CoordPoint};
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

/// Options for schematic netlist extraction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchNetlistOptions {
    /// Emit every sheet-symbol entry as a connection whose designator is
    /// the sheet symbol's name (RECORD=32 annotation, else the typed
    /// sheet/file name) and whose pad is the entry name. Lets a parent
    /// sheet's netlist show which child-sheet entries share a wire.
    pub include_sheet_entries: bool,
    /// Emit every port a net touches as a pseudo-connection with designator
    /// [`PORT_DESIGNATOR`] and pad = port name (harness entries report
    /// `<bundle>.<entry>`), so a flattener can bind child-sheet nets to the
    /// parent's sheet entries.
    pub include_ports: bool,
}

/// Designator of the pseudo-connections emitted by
/// [`SchNetlistOptions::include_ports`].
pub const PORT_DESIGNATOR: &str = "PORT";

/// Snap radius for port ends: one DXP unit (10 mil).
const PORT_SNAP_RAW: i64 = crate::sch::binary::RAW_PER_DXP as i64;

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

    /// Build a netlist from a `.SchDoc` with default options.
    pub fn from_sch_document(doc: &sch::Document) -> Self {
        Self::from_sch_document_with(doc, &SchNetlistOptions::default())
    }

    /// Build a netlist from a `.SchDoc` by tracing the wire graph.
    ///
    /// Endpoints (pin connectors, wire vertices, junctions, net-label
    /// anchors, power-port anchors, port ends, harness-connector entry
    /// points) are collected onto a position-keyed graph. Wire segments,
    /// bus entries and signal-harness lines produce edges. Net labels,
    /// power ports and ports name the nets they touch; a harness
    /// connector's entries are named `<bundle>.<entry>`, where the bundle
    /// is the harness port (or harness sheet entry) that its primary
    /// connection reaches, falling back to the harness type text. Anything
    /// left unnamed gets an auto-generated `N00001`-style identifier; auto-
    /// named single-pin nets are dropped, but a pin that only reaches a port
    /// is kept under the port's name because the port carries it off-sheet.
    ///
    /// With [`SchNetlistOptions::include_sheet_entries`], every sheet-symbol
    /// entry is added as a pseudo-connection (designator = the sheet
    /// symbol's name, pad = entry name) so a parent sheet's netlist shows
    /// which child entries are wired together. Bus-tap inheritance is not
    /// modelled.
    pub fn from_sch_document_with(doc: &sch::Document, options: &SchNetlistOptions) -> Self {
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
            // RECORD=47 map definers on the current PCB implementation fan a
            // schematic pin designator out to one or more footprint pads
            // (e.g. a single hidden GND pin covering every ground pad).
            let mut pad_map: HashMap<&str, &Vec<String>> = HashMap::new();
            for imp in &comp.implementations {
                let is_pcb = imp
                    .model_type
                    .as_deref()
                    .is_some_and(|t| t.eq_ignore_ascii_case("PCBLIB"));
                if !is_pcb || !imp.is_current {
                    continue;
                }
                for md in &imp.map_definers {
                    if let Some(intf) = md.designator_interface.as_deref() {
                        if !md.designator_implementations.is_empty() {
                            pad_map.insert(intf, &md.designator_implementations);
                        }
                    }
                }
            }
            for pin in &comp.pins {
                // Multi-part symbols place each part as its own component
                // record carrying the FULL pin list; only pins owned by this
                // placed part (and its display mode) are electrically present
                // here. OWNERPARTID <= 0 means "belongs to every part".
                if pin.common.owner_part_id > 0
                    && comp.current_part_id > 0
                    && pin.common.owner_part_id != comp.current_part_id
                {
                    continue;
                }
                if comp.display_mode_count > 1
                    && pin.common.owner_part_display_mode >= 0
                    && pin.common.owner_part_display_mode != comp.display_mode
                {
                    continue;
                }
                let world = pin_world_endpoint(comp, pin);
                let pad = pin
                    .designator
                    .clone()
                    .or_else(|| pin.name.clone())
                    .unwrap_or_default();
                if pad.is_empty() {
                    continue;
                }
                match pad_map.get(pad.as_str()) {
                    Some(pads) => {
                        for mapped in pads.iter() {
                            pin_endpoints.push((designator.clone(), mapped.clone(), world));
                        }
                    }
                    None => pin_endpoints.push((designator.clone(), pad, world)),
                }
            }
        }

        // Sheet-symbol entries as pseudo-pins (opt-in): lets a parent sheet's
        // netlist say which child-sheet entries share a wire.
        if options.include_sheet_entries {
            for (idx, sym) in doc.sheet_symbols.iter().enumerate() {
                let name = sheet_symbol_name(doc, sym, idx);
                for entry in &sym.entries {
                    let entry_name = entry.name.trim();
                    if entry_name.is_empty() {
                        continue;
                    }
                    pin_endpoints.push((
                        name.clone(),
                        entry_name.to_string(),
                        sheet_entry_endpoint(sym, entry),
                    ));
                }
            }
        }

        // Ports connect at either end (Altium does the same), so both ends
        // are graph nodes. Harness-typed ports carry bundles, not nets: they
        // only take part in the harness graph below. Auto-sized ports keep a
        // fractional `LOCATION.X`, so an end can miss the wire it was dropped
        // on by a fraction of a DXP unit; Altium still connects them, so port
        // ends snap to the nearest hard node within one DXP unit.
        let mut port_nodes: Vec<(usize, CoordPoint)> = Vec::new();
        for (i, port) in doc.ports.iter().enumerate() {
            if port.harness_type.as_deref().is_some_and(|t| !t.trim().is_empty()) {
                continue;
            }
            for p in port_endpoints(port) {
                port_nodes.push((i, p));
            }
        }

        // Harness-connector entry points join the wire graph; the
        // connector's primary connection point joins the harness graph.
        let mut harness_entry_nodes: Vec<(usize, usize, CoordPoint)> = Vec::new();
        for (ci, hc) in doc.harness_connectors.iter().enumerate() {
            for (ei, e) in hc.entries.iter().enumerate() {
                harness_entry_nodes.push((ci, ei, harness_entry_endpoint(hc, e)));
            }
        }

        // Signal-harness lines are traced like wires: on a parent sheet they
        // are the only thing joining two harness-typed sheet entries.
        let segments: Vec<&[CoordPoint]> = doc
            .wires
            .iter()
            .map(|w| w.vertices.as_slice())
            .chain(doc.signal_harnesses.iter().map(|s| s.vertices.as_slice()))
            .collect();

        // Hard nodes: everything with an exact electrical position.
        let mut nodes: Vec<CoordPoint> = Vec::new();
        nodes.extend(pin_endpoints.iter().map(|(_, _, p)| *p));
        nodes.extend(doc.junctions.iter().map(|j| j.location));
        nodes.extend(doc.net_labels.iter().map(|l| l.location));
        nodes.extend(doc.power_objects.iter().map(|p| p.location));
        nodes.extend(harness_entry_nodes.iter().map(|(_, _, p)| *p));
        for be in &doc.bus_entries {
            nodes.push(be.location);
            nodes.push(be.corner);
        }
        for verts in &segments {
            nodes.extend(verts.iter().copied());
        }
        nodes.sort_by_key(|p| (p.x.to_raw(), p.y.to_raw()));
        nodes.dedup();
        for (_, p) in port_nodes.iter_mut() {
            *p = snap_to_nodes(*p, &nodes, PORT_SNAP_RAW);
        }
        nodes.extend(port_nodes.iter().map(|(_, p)| *p));
        nodes.sort_by_key(|p| (p.x.to_raw(), p.y.to_raw()));
        nodes.dedup();

        // Ports as pseudo-pins (opt-in): each net then lists the ports it
        // touches, so a flattener can bind a child sheet's nets to the
        // parent's sheet entries. Harness entries are added as
        // `<bundle>.<entry>` once bundle names are known (below).
        if options.include_ports {
            for (i, p) in &port_nodes {
                let name = doc.ports[*i].name.trim();
                if !name.is_empty() {
                    pin_endpoints.push((PORT_DESIGNATOR.to_string(), name.to_string(), *p));
                }
            }
        }

        let mut uf = UnionFind::default();
        for p in &nodes {
            uf.add(*p);
        }
        for verts in &segments {
            for window in verts.windows(2) {
                uf.union(window[0], window[1]);
            }
        }
        // Bus entries connect two points by definition (wire-to-bus join).
        for be in &doc.bus_entries {
            uf.union(be.location, be.corner);
        }

        // A node that lands in the INTERIOR of a wire segment is connected
        // to that wire even though no shared vertex exists — Altium treats
        // any point on a wire as connected. Matching only exact vertices
        // splits nets: the classic symptom is a labelled net losing every
        // member on the far side of a junction.
        for verts in &segments {
            for seg in verts.windows(2) {
                let (a, b) = (seg[0], seg[1]);
                for p in &nodes {
                    if point_on_segment_interior(*p, a, b) {
                        uf.union(*p, a);
                    }
                }
            }
        }

        // Names: net labels, power ports and ports anchor names onto
        // positions (in that priority order).
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
        for (i, p) in &port_nodes {
            let name = doc.ports[*i].name.trim().to_string();
            if !name.is_empty() {
                let root = uf.find(*p);
                named_nets.entry(root).or_insert(name);
            }
        }

        // Harness bundles: a connector's primary connection point reaches its
        // carrier — a harness port, a harness-typed sheet entry, or a wire /
        // signal-harness line carrying a net label — directly or over
        // signal-harness lines and wires. Altium names the bundle after the
        // net label on the carrier when there is one, else the port / sheet
        // entry it lands on; the connector's entries become
        // `<bundle>.<entry>`. Harness port ends get the same one-DXP snap as
        // plain ports.
        if !doc.harness_connectors.is_empty() {
            let mut huf = UnionFind::default();
            let mut hnodes: Vec<CoordPoint> = Vec::new();
            let primaries: Vec<CoordPoint> = doc
                .harness_connectors
                .iter()
                .map(harness_primary_point)
                .collect();
            hnodes.extend(primaries.iter().copied());
            // (priority, bundle name, anchor point, (sheet symbol, entry) if
            // the anchor is a sheet entry). Lower priority wins the name.
            let mut anchors: Vec<(u8, String, CoordPoint, Option<(String, String)>)> = Vec::new();
            for label in &doc.net_labels {
                let text = label.text.trim();
                if !text.is_empty() {
                    anchors.push((0, text.to_string(), label.location, None));
                }
                hnodes.push(label.location);
            }
            for (idx, sym) in doc.sheet_symbols.iter().enumerate() {
                let mut sym_name: Option<String> = None;
                for entry in &sym.entries {
                    if !entry.harness_type.as_deref().is_some_and(|t| !t.trim().is_empty()) {
                        continue;
                    }
                    let p = sheet_entry_endpoint(sym, entry);
                    hnodes.push(p);
                    let name = sym_name
                        .get_or_insert_with(|| sheet_symbol_name(doc, sym, idx))
                        .clone();
                    let entry_name = entry.name.trim().to_string();
                    anchors.push((2, entry_name.clone(), p, Some((name, entry_name))));
                }
            }
            hnodes.extend(doc.junctions.iter().map(|j| j.location));
            for verts in &segments {
                hnodes.extend(verts.iter().copied());
            }
            hnodes.sort_by_key(|p| (p.x.to_raw(), p.y.to_raw()));
            hnodes.dedup();
            for port in &doc.ports {
                if !port.harness_type.as_deref().is_some_and(|t| !t.trim().is_empty()) {
                    continue;
                }
                for p in port_endpoints(port) {
                    let p = snap_to_nodes(p, &hnodes, PORT_SNAP_RAW);
                    anchors.push((1, port.name.trim().to_string(), p, None));
                }
            }
            hnodes.extend(anchors.iter().map(|(_, _, p, _)| *p));
            hnodes.sort_by_key(|p| (p.x.to_raw(), p.y.to_raw()));
            hnodes.dedup();
            for p in &hnodes {
                huf.add(*p);
            }
            for verts in &segments {
                for window in verts.windows(2) {
                    huf.union(window[0], window[1]);
                }
            }
            for verts in &segments {
                for seg in verts.windows(2) {
                    let (a, b) = (seg[0], seg[1]);
                    for p in &hnodes {
                        if point_on_segment_interior(*p, a, b) {
                            huf.union(*p, a);
                        }
                    }
                }
            }
            // Same-named labels continue a bundle across the sheet, exactly
            // as they do for plain nets.
            let mut label_first: HashMap<&str, CoordPoint> = HashMap::new();
            for label in &doc.net_labels {
                let text = label.text.trim();
                if text.is_empty() {
                    continue;
                }
                match label_first.get(text) {
                    Some(first) => huf.union(*first, label.location),
                    None => {
                        label_first.insert(text, label.location);
                    }
                }
            }
            for (ci, hc) in doc.harness_connectors.iter().enumerate() {
                let root = huf.find(primaries[ci]);
                let mut group: Vec<&(u8, String, CoordPoint, Option<(String, String)>)> = anchors
                    .iter()
                    .filter(|(_, name, p, _)| !name.is_empty() && huf.find(*p) == root)
                    .collect();
                group.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
                let bundle = group
                    .first()
                    .map(|(_, name, _, _)| name.clone())
                    .or_else(|| {
                        hc.harness_type
                            .as_ref()
                            .map(|t| t.text.trim().to_string())
                            .filter(|t| !t.is_empty())
                    })
                    .unwrap_or_else(|| format!("HARNESS{}", ci + 1));
                let sheet_anchors: Vec<(String, String)> = group
                    .iter()
                    .filter_map(|(_, _, _, se)| se.clone())
                    .collect();
                for (c, ei, p) in &harness_entry_nodes {
                    if *c != ci {
                        continue;
                    }
                    let entry = hc.entries[*ei].name.trim();
                    if entry.is_empty() {
                        continue;
                    }
                    let root = uf.find(*p);
                    named_nets.entry(root).or_insert(format!("{bundle}.{entry}"));
                    if options.include_ports {
                        pin_endpoints.push((
                            PORT_DESIGNATOR.to_string(),
                            format!("{bundle}.{entry}"),
                            *p,
                        ));
                    }
                    // A connector fanned out from child sheets' harness
                    // entries: report each signal against every such sheet
                    // symbol as `<entry>.<signal>` so a flattener can bind
                    // it to the child's `<port>.<signal>`.
                    if options.include_sheet_entries {
                        for (sym, sym_entry) in &sheet_anchors {
                            pin_endpoints.push((sym.clone(), format!("{sym_entry}.{entry}"), *p));
                        }
                    }
                }
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
        // does the same unless `NetlistSinglePinNets=1`. Named single-pin
        // nets stay: a pin whose only neighbour is a port or harness entry
        // is connected off-sheet.
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
                // A port touching nothing is not a net.
                if connections.len() == 1 && connections[0].designator == PORT_DESIGNATOR {
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

/// The component's live designator — what the UI and silkscreen show.
///
/// Prefers the dedicated designator Text child (`is_designator`). The
/// `source_designator` field is only the last ECO-sync snapshot and goes
/// stale when the schematic is re-annotated without re-linking components
/// (seen in the wild: an entire board one annotation era behind, including
/// duplicate stale names).
pub(crate) fn pcb_designator(comp: &pcb::Component) -> String {
    if let Some(t) = comp.texts.iter().find(|t| {
        t.is_designator && !t.text.trim().is_empty() && !t.text.trim().starts_with('.')
    }) {
        return t.text.trim().to_string();
    }
    comp.source_designator
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| comp.name.clone())
}

/// True when `p` lies strictly inside segment `a`..`b` (collinear, within
/// the bounding box, not an endpoint). Exact integer arithmetic on raw
/// coordinate units — works for orthogonal and angled wires alike.
fn point_on_segment_interior(p: CoordPoint, a: CoordPoint, b: CoordPoint) -> bool {
    if p == a || p == b {
        return false;
    }
    let (px, py) = (i64::from(p.x.to_raw()), i64::from(p.y.to_raw()));
    let (ax, ay) = (i64::from(a.x.to_raw()), i64::from(a.y.to_raw()));
    let (bx, by) = (i64::from(b.x.to_raw()), i64::from(b.y.to_raw()));
    let cross = (bx - ax) * (py - ay) - (by - ay) * (px - ax);
    if cross != 0 {
        return false;
    }
    px >= ax.min(bx) && px <= ax.max(bx) && py >= ay.min(by) && py <= ay.max(by)
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
    // Some files store the footprint as a parameter; otherwise it is the
    // CURRENT PCBLIB implementation (a symbol can carry a whole family of
    // footprint models — the first one listed is rarely the fitted one).
    for p in &comp.parameters {
        if p.name.eq_ignore_ascii_case("Footprint") && !p.value.is_empty() {
            return Some(p.value.clone());
        }
    }
    let is_pcb = |i: &&sch::Implementation| {
        i.model_type
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case("PCBLIB"))
    };
    comp.implementations
        .iter()
        .filter(is_pcb)
        .find(|i| i.is_current)
        .or_else(|| comp.implementations.iter().find(is_pcb))
        .or_else(|| comp.implementations.first())
        .and_then(|i| i.model_name.clone())
}

/// Name a sheet symbol the way Altium's compiler does: the RECORD=32
/// sheet-name annotation, which Altium parks one grid (10 DXP) above the
/// symbol's top-left corner. Falls back to the nearest annotation within
/// two grids, then the typed sheet/file name, then `SHEET<n>`.
fn sheet_symbol_name(
    doc: &sch::Document,
    sym: &sch::primitives::SheetSymbol,
    idx: usize,
) -> String {
    const RAW_PER_DXP: i64 = crate::sch::binary::RAW_PER_DXP as i64;
    let sx = sym.location.x.to_raw() as i64;
    let sy = sym.location.y.to_raw() as i64 + 10 * RAW_PER_DXP;
    let mut best: Option<(i64, String)> = None;
    for ann in &doc.sheet_name_annotations {
        let parse = |key: &str| ann.get(key).and_then(|v| v.trim().parse::<i64>().ok());
        let (Some(ax), Some(ay), Some(text)) = (parse("Location.X"), parse("Location.Y"), ann.get("Text")) else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let d = (ax * RAW_PER_DXP - sx).abs() + (ay * RAW_PER_DXP - sy).abs();
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, text.to_string()));
        }
    }
    if let Some((d, name)) = best {
        if d <= 20 * RAW_PER_DXP {
            return name;
        }
    }
    sym.sheet_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| sym.file_name.as_deref().map(str::trim).filter(|s| !s.is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| format!("SHEET{}", idx + 1))
}

/// World position of a sheet entry: `location` is the symbol's top-left,
/// `y_size` extends downward, and `distance_from_top` is already a
/// coordinate (the codec scales Altium's 100-mil slot count).
fn sheet_entry_endpoint(
    sym: &sch::primitives::SheetSymbol,
    e: &sch::primitives::SheetEntry,
) -> CoordPoint {
    edge_point(sym.location, sym.x_size, sym.y_size, e.side, e.distance_from_top)
}

fn harness_entry_endpoint(
    hc: &sch::primitives::HarnessConnector,
    e: &sch::primitives::HarnessEntry,
) -> CoordPoint {
    edge_point(hc.location, hc.x_size, hc.y_size, e.side, e.distance_from_top)
}

fn harness_primary_point(hc: &sch::primitives::HarnessConnector) -> CoordPoint {
    edge_point(
        hc.location,
        hc.x_size,
        hc.y_size,
        hc.side,
        hc.primary_connection_position,
    )
}

/// A point `dist` along a box edge: side 0 = left, 1 = right (measured
/// down from the top), 2 = top, 3 = bottom (measured right from the left).
fn edge_point(top_left: CoordPoint, x_size: Coord, y_size: Coord, side: i32, dist: Coord) -> CoordPoint {
    let (x, y) = (top_left.x, top_left.y);
    match side {
        1 => CoordPoint::new(x + x_size, y - dist),
        2 => CoordPoint::new(x + dist, y),
        3 => CoordPoint::new(x + dist, y - y_size),
        _ => CoordPoint::new(x, y - dist),
    }
}

/// Move `p` onto the nearest of `nodes` (sorted by `(x, y)`) within
/// `radius` raw units on both axes, or leave it alone.
fn snap_to_nodes(p: CoordPoint, nodes: &[CoordPoint], radius: i64) -> CoordPoint {
    let key = (p.x.to_raw(), p.y.to_raw());
    if nodes
        .binary_search_by_key(&key, |n| (n.x.to_raw(), n.y.to_raw()))
        .is_ok()
    {
        return p;
    }
    let mut best: Option<(i64, CoordPoint)> = None;
    for n in nodes {
        let dx = (n.x.to_raw() as i64 - key.0 as i64).abs();
        let dy = (n.y.to_raw() as i64 - key.1 as i64).abs();
        if dx > radius || dy > radius {
            continue;
        }
        let d = dx * dx + dy * dy;
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, *n));
        }
    }
    best.map_or(p, |(_, n)| n)
}

/// Both connection ends of a port. Styles 0..=3 are horizontal (location is
/// the left end, the port extends `width` to the right); 4..=7 are vertical
/// (location is the bottom end).
fn port_endpoints(port: &sch::primitives::Port) -> [CoordPoint; 2] {
    if port.style >= 4 {
        [
            port.location,
            CoordPoint::new(port.location.x, port.location.y + port.width),
        ]
    } else {
        [
            port.location,
            CoordPoint::new(port.location.x + port.width, port.location.y),
        ]
    }
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
    #[test]
    fn schdoc_netlist_filters_pins_of_other_parts() {
        // Multi-part symbol: each placed part record carries ALL pins; only
        // pins with OWNERPARTID == current_part_id belong to this placement.
        let mut doc = sch::Document::default();
        let mut own = pin_at("1", coord_pt(0.0, 0.0), PinOrientation::Right, 10.0);
        own.common.owner_part_id = 1;
        let mut foreign = pin_at("2", coord_pt(50.0, 0.0), PinOrientation::Left, 10.0);
        foreign.common.owner_part_id = 2;
        let mut comp = placed("U1", "MULTI", vec![own, foreign]);
        comp.current_part_id = 1;
        doc.components.push(comp);
        doc.components.push(placed(
            "R1",
            "R0402",
            vec![pin_at("1", coord_pt(50.0, 0.0), PinOrientation::Left, 10.0)],
        ));
        // wire touching the foreign pin's endpoint (40,0)..(10,0)
        let mut w = Wire::default();
        w.vertices = vec![coord_pt(10.0, 0.0), coord_pt(40.0, 0.0)];
        doc.wires.push(w);

        let nl = Netlist::from_sch_document(&doc);
        // U1 pin 2 belongs to part 2 and must NOT appear anywhere.
        for net in &nl.nets {
            for c in &net.connections {
                assert!(!(c.designator == "U1" && c.pad == "2"), "phantom pin from other part");
            }
        }
    }

    #[test]
    fn schdoc_netlist_expands_record47_pin_maps() {
        // A pin whose designator maps to several footprint pads (RECORD=47)
        // contributes every mapped pad to the net.
        use crate::sch::implementation::{Implementation, MapDefiner};
        let mut comp = placed(
            "U1",
            "MODULE",
            vec![pin_at("1", coord_pt(0.0, 0.0), PinOrientation::Right, 10.0)],
        );
        let mut imp = Implementation::default();
        imp.model_type = Some("PCBLIB".into());
        imp.is_current = true;
        let mut md = MapDefiner::default();
        md.designator_interface = Some("1".into());
        md.designator_implementations = vec!["1".into(), "2".into(), "18".into()];
        imp.map_definers.push(md);
        comp.implementations.push(imp);

        let mut doc = sch::Document::default();
        doc.components.push(comp);
        doc.components.push(placed(
            "R1",
            "R0402",
            vec![pin_at("1", coord_pt(50.0, 0.0), PinOrientation::Left, 10.0)],
        ));
        let mut w = Wire::default();
        w.vertices = vec![coord_pt(10.0, 0.0), coord_pt(40.0, 0.0)];
        doc.wires.push(w);

        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1);
        let pads: Vec<&str> = nl.nets[0]
            .connections
            .iter()
            .filter(|c| c.designator == "U1")
            .map(|c| c.pad.as_str())
            .collect();
        assert_eq!(pads, vec!["1", "18", "2"], "mapped pads expanded (sorted)");
    }

    #[test]
    fn schdoc_netlist_connects_label_on_segment_interior() {
        // The label sits in the middle of the wire (not on a vertex) — it
        // must still name the net.
        let mut doc = sch::Document::default();
        doc.components.push(placed(
            "R1",
            "R0402",
            vec![pin_at("1", coord_pt(0.0, 0.0), PinOrientation::Right, 10.0)],
        ));
        let mut w = Wire::default();
        w.vertices = vec![coord_pt(10.0, 0.0), coord_pt(100.0, 0.0)];
        doc.wires.push(w);
        let mut label = NetLabel::default();
        label.text = "MID_NET".into();
        label.location = coord_pt(55.0, 0.0);
        doc.net_labels.push(label);

        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "MID_NET");
    }

    #[test]
    fn schdoc_netlist_joins_wires_at_t_junction() {
        // Wire B ends in the interior of wire A: the two nets must merge
        // even though A has no vertex at the junction point.
        let mut doc = sch::Document::default();
        doc.components.push(placed(
            "R1",
            "R0402",
            vec![pin_at("1", coord_pt(0.0, 0.0), PinOrientation::Right, 10.0)],
        ));
        doc.components.push(placed(
            "R2",
            "R0402",
            vec![pin_at("1", coord_pt(50.0, 50.0), PinOrientation::Down, 10.0)],
        ));
        let mut a = Wire::default();
        a.vertices = vec![coord_pt(10.0, 0.0), coord_pt(100.0, 0.0)];
        doc.wires.push(a);
        let mut b = Wire::default();
        b.vertices = vec![coord_pt(50.0, 40.0), coord_pt(50.0, 0.0)];
        doc.wires.push(b);

        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1, "T-junction merges both wires");
        assert_eq!(nl.nets[0].connections.len(), 2);
    }

    #[test]
    fn pcbdoc_netlist_prefers_live_designator_text() {
        // source_designator is a stale ECO-sync snapshot; the designator
        // Text child is what the UI/silkscreen shows.
        let mut doc = pcb::Document::default();
        let mut comp = pcb::Component::new("R0402");
        comp.source_designator = Some("R99".into());
        let mut t = crate::pcb::primitives::Text::default();
        t.text = "R1".into();
        t.is_designator = true;
        comp.texts.push(t);
        let mut p1 = Pad::default();
        p1.designator = Some("1".into());
        p1.net = Some("VCC".into());
        comp.pads.push(p1);
        doc.components.push(comp);

        let nl = Netlist::from_pcb_document(&doc);
        assert_eq!(nl.components[0].designator, "R1");
        assert_eq!(nl.nets[0].connections[0].designator, "R1");
    }

    fn dxp(v: f64) -> Coord {
        Coord::from_mils(v * 10.0)
    }

    fn dxp_pt(x: f64, y: f64) -> CoordPoint {
        CoordPoint::new(dxp(x), dxp(y))
    }

    #[test]
    fn schdoc_netlist_names_net_from_port_and_keeps_single_pin_port_net() {
        // J1-D16 wired to nothing but a port: Altium compiles that as the
        // net MODC_PWR_EN leaving the sheet, so it must not be filtered as
        // a single-pin net. The port touches the wire at its RIGHT end.
        let mut doc = sch::Document::default();
        doc.components.push(placed(
            "J1",
            "KRIA-J1",
            vec![pin_at("D16", coord_pt(0.0, 0.0), PinOrientation::Right, 10.0)],
        ));
        let mut w = Wire::default();
        w.vertices = vec![coord_pt(10.0, 0.0), coord_pt(100.0, 0.0)];
        doc.wires.push(w);
        let mut port = sch::primitives::Port::default();
        port.name = "MODC_PWR_EN".into();
        port.location = coord_pt(100.0, 0.0);
        port.width = Coord::from_mils(50.0);
        doc.ports.push(port);

        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "MODC_PWR_EN");
        assert_eq!(nl.nets[0].connections.len(), 1);
        assert_eq!(nl.nets[0].connections[0].designator, "J1");
        assert_eq!(nl.nets[0].connections[0].pad, "D16");

        // Same port, wire arriving at its LEFT end instead.
        doc.ports[0].location = coord_pt(50.0, 0.0);
        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "MODC_PWR_EN");
    }

    #[test]
    fn schdoc_netlist_label_outranks_port_for_net_name() {
        let mut doc = sch::Document::default();
        doc.components.push(placed(
            "R1",
            "R0402",
            vec![pin_at("1", coord_pt(0.0, 0.0), PinOrientation::Right, 10.0)],
        ));
        let mut w = Wire::default();
        w.vertices = vec![coord_pt(10.0, 0.0), coord_pt(100.0, 0.0)];
        doc.wires.push(w);
        let mut label = NetLabel::default();
        label.text = "RESET_N".into();
        label.location = coord_pt(50.0, 0.0);
        doc.net_labels.push(label);
        let mut port = sch::primitives::Port::default();
        port.name = "SYS_RESET_N".into();
        port.location = coord_pt(100.0, 0.0);
        port.width = Coord::from_mils(50.0);
        doc.ports.push(port);
        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "RESET_N");
    }

    #[test]
    fn schdoc_netlist_names_harness_entries_bundle_dot_entry() {
        // Mirrors a real AuxConnector sheet: harness port CLK (LVDS) at
        // (400,630) DXP, width 50, ends at (450,630) = the connector's
        // primary point (top-left (450,650), 20 DXP down the left edge).
        // Entries D_N / D_P sit on the right edge in slots 1 / 2 ->
        // (490,640) / (490,630); wires run from there to J15 pins 4 / 6.
        let mut doc = sch::Document::default();
        doc.components.push(placed(
            "J15",
            "DF40C-30DS",
            vec![
                pin_at("4", dxp_pt(540.0, 640.0), PinOrientation::Left, 200.0),
                pin_at("6", dxp_pt(540.0, 630.0), PinOrientation::Left, 200.0),
            ],
        ));
        for y in [640.0, 630.0] {
            let mut w = Wire::default();
            w.vertices = vec![dxp_pt(490.0, y), dxp_pt(520.0, y)];
            doc.wires.push(w);
        }
        let mut port = sch::primitives::Port::default();
        port.name = "CLK".into();
        port.harness_type = Some("LVDS".into());
        port.location = dxp_pt(400.0, 630.0);
        port.width = dxp(50.0);
        doc.ports.push(port);

        let mut hc = sch::primitives::HarnessConnector::default();
        hc.location = dxp_pt(450.0, 650.0);
        hc.x_size = dxp(40.0);
        hc.y_size = dxp(40.0);
        hc.primary_connection_position = dxp(20.0);
        hc.side = 0;
        for (name, slot) in [("D_N", 1.0), ("D_P", 2.0)] {
            let mut e = sch::primitives::HarnessEntry::default();
            e.name = name.into();
            e.side = 1;
            e.distance_from_top = dxp(10.0 * slot);
            hc.entries.push(e);
        }
        let mut ht = sch::primitives::HarnessType::default();
        ht.text = "LVDS".into();
        hc.harness_type = Some(ht);
        doc.harness_connectors.push(hc);

        let nl = Netlist::from_sch_document(&doc);
        let names: Vec<&str> = nl.nets.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["CLK.D_N", "CLK.D_P"]);
        assert_eq!(nl.nets[0].connections[0].pad, "4");
        assert_eq!(nl.nets[1].connections[0].pad, "6");

        // With ports as pseudo-connections the harness entry shows up as
        // `<bundle>.<entry>` on the PORT designator.
        let nl = Netlist::from_sch_document_with(
            &doc,
            &SchNetlistOptions {
                include_ports: true,
                ..Default::default()
            },
        );
        let dn = nl.nets.iter().find(|n| n.name == "CLK.D_N").unwrap();
        assert!(dn
            .connections
            .iter()
            .any(|c| c.designator == PORT_DESIGNATOR && c.pad == "CLK.D_N"));

        // An auto-sized harness port whose end misses the primary point by
        // a fraction of a DXP unit still names the bundle (real case:
        // GEM3_MDIO at x = 97.42, width 62 vs connector edge at 160).
        doc.ports[0].location = CoordPoint::new(Coord::from_raw(39_942_244), dxp(630.0));
        doc.ports[0].width = dxp(50.0);
        let nl = Netlist::from_sch_document(&doc);
        let names: Vec<&str> = nl.nets.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["CLK.D_N", "CLK.D_P"]);
    }

    #[test]
    fn schdoc_netlist_binds_connector_on_sheet_entry_per_signal() {
        // Parent sheet: sheet symbol GNSS (top-left (600,500), 100x100) with
        // a harness-typed entry UART on its left edge, slot 3 -> (600,470).
        // A harness connector (top-left (500,480), 100x30, primary on the
        // right edge 10 down -> (600,470)) touches it directly; its entry
        // RXD on the left edge, slot 1 -> (500,470), is wired to a label.
        let mut doc = sch::Document::default();
        let mut sym = sch::primitives::SheetSymbol::default();
        sym.location = dxp_pt(600.0, 500.0);
        sym.x_size = dxp(100.0);
        sym.y_size = dxp(100.0);
        let mut e = sch::primitives::SheetEntry::default();
        e.name = "UART".into();
        e.side = 0;
        e.distance_from_top = dxp(30.0);
        e.harness_type = Some("UART".into());
        sym.entries.push(e);
        doc.sheet_symbols.push(sym);
        let mut ann = BTreeMap::new();
        ann.insert("Location.X".to_string(), "600".to_string());
        ann.insert("Location.Y".to_string(), "510".to_string());
        ann.insert("Text".to_string(), "GNSS".to_string());
        doc.sheet_name_annotations.push(ann);

        let mut hc = sch::primitives::HarnessConnector::default();
        hc.location = dxp_pt(500.0, 480.0);
        hc.x_size = dxp(100.0); // right edge at x = 600, on the sheet entry
        hc.y_size = dxp(30.0);
        hc.primary_connection_position = dxp(10.0);
        hc.side = 1;
        let mut he = sch::primitives::HarnessEntry::default();
        he.name = "RXD".into();
        he.side = 0;
        he.distance_from_top = dxp(10.0);
        hc.entries.push(he);
        doc.harness_connectors.push(hc);
        doc.components.push(placed(
            "R1",
            "R0402",
            vec![pin_at("1", dxp_pt(400.0, 470.0), PinOrientation::Right, 1000.0)],
        ));

        let nl = Netlist::from_sch_document_with(
            &doc,
            &SchNetlistOptions {
                include_sheet_entries: true,
                include_ports: true,
            },
        );
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "UART.RXD");
        check(&nl);

        // Same, but the connector sits away from the symbol and a plain wire
        // carrying the label GNSS_UART joins its primary point to the sheet
        // entry (how Altium users usually draw it). The label names the
        // bundle: nets become GNSS_UART.RXD, and the sheet-entry pseudo-pin
        // stays keyed by the child's port name, UART.RXD.
        doc.harness_connectors[0].x_size = dxp(40.0); // primary now at (540,470)
        // Two disjoint stubs joined only by the shared label text, as when
        // the connector sits on the far side of the sheet.
        for (x0, x1, lx) in [(540.0, 560.0, 550.0), (580.0, 600.0, 590.0)] {
            let mut w = Wire::default();
            w.vertices = vec![dxp_pt(x0, 470.0), dxp_pt(x1, 470.0)];
            doc.wires.push(w);
            let mut label = NetLabel::default();
            label.text = "GNSS_UART".into();
            label.location = dxp_pt(lx, 470.0);
            doc.net_labels.push(label);
        }
        let nl = Netlist::from_sch_document_with(
            &doc,
            &SchNetlistOptions {
                include_sheet_entries: true,
                include_ports: true,
            },
        );
        let names: Vec<&str> = nl.nets.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["GNSS_UART", "GNSS_UART.RXD"], "{names:?}");
        let signal = nl.nets.iter().find(|n| n.name == "GNSS_UART.RXD").unwrap();
        let conns: Vec<(String, String)> = signal
            .connections
            .iter()
            .map(|c| (c.designator.clone(), c.pad.clone()))
            .collect();
        assert!(conns.contains(&("GNSS".to_string(), "UART.RXD".to_string())), "{conns:?}");
        assert!(conns.contains(&(PORT_DESIGNATOR.to_string(), "GNSS_UART.RXD".to_string())));
        assert!(conns.contains(&("R1".to_string(), "1".to_string())));
        return;

        fn check(nl: &Netlist) {
        let conns: Vec<(String, String)> = nl.nets[0]
            .connections
            .iter()
            .map(|c| (c.designator.clone(), c.pad.clone()))
            .collect();
        assert!(conns.contains(&("GNSS".to_string(), "UART.RXD".to_string())), "{conns:?}");
        assert!(conns.contains(&(PORT_DESIGNATOR.to_string(), "UART.RXD".to_string())));
        assert!(conns.contains(&("R1".to_string(), "1".to_string())));
        }
    }

    #[test]
    fn schdoc_netlist_harness_bundle_falls_back_to_type_and_follows_signal_harness_lines() {
        let mut doc = sch::Document::default();
        doc.components.push(placed(
            "U1",
            "LMK",
            vec![pin_at("16", dxp_pt(0.0, 90.0), PinOrientation::Right, 500.0)],
        ));
        // Connector top-left (50,100), 30x30, right-side primary at 15 down,
        // entry SDA on the LEFT edge, slot 1 -> (50, 90): the pin lands there.
        let mut hc = sch::primitives::HarnessConnector::default();
        hc.location = dxp_pt(50.0, 100.0);
        hc.x_size = dxp(30.0);
        hc.y_size = dxp(30.0);
        hc.primary_connection_position = dxp(15.0);
        hc.side = 1;
        let mut e = sch::primitives::HarnessEntry::default();
        e.name = "SDA".into();
        e.side = 0;
        e.distance_from_top = dxp(10.0);
        hc.entries.push(e);
        let mut ht = sch::primitives::HarnessType::default();
        ht.text = "I2C".into();
        hc.harness_type = Some(ht);
        doc.harness_connectors.push(hc);

        // No port reachable: bundle name is the harness type.
        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "I2C.SDA");

        // A signal-harness line from the primary point (80,85) to a harness
        // port at (200,85): the bundle takes the port's name.
        let mut sh = sch::primitives::SignalHarness::default();
        sh.vertices = vec![dxp_pt(80.0, 85.0), dxp_pt(200.0, 85.0)];
        doc.signal_harnesses.push(sh);
        let mut port = sch::primitives::Port::default();
        port.name = "CARRIER_I2C".into();
        port.harness_type = Some("I2C".into());
        port.location = dxp_pt(200.0, 85.0);
        port.width = dxp(60.0);
        doc.ports.push(port);
        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "CARRIER_I2C.SDA");
    }

    #[test]
    fn schdoc_netlist_snaps_off_grid_port_end_onto_wire() {
        // Real case: auto-sized port ONBOARD_1PPS at x = 983.22 DXP, width
        // 76, so its right end lands 0.78 DXP short of the wire end at
        // x = 1060. Altium still connects it.
        let mut doc = sch::Document::default();
        doc.components.push(placed(
            "U1",
            "LMK",
            vec![pin_at("39", dxp_pt(1110.0, 650.0), PinOrientation::Left, 200.0)],
        ));
        let mut w = Wire::default();
        w.vertices = vec![dxp_pt(1060.0, 650.0), dxp_pt(1090.0, 650.0)];
        doc.wires.push(w);
        let mut port = sch::primitives::Port::default();
        port.name = "ONBOARD_1PPS".into();
        port.location = CoordPoint::new(Coord::from_raw(98_322_196), dxp(650.0));
        port.width = dxp(76.0);
        doc.ports.push(port);
        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "ONBOARD_1PPS");
        assert_eq!(nl.nets[0].connections[0].pad, "39");

        // Beyond one DXP unit it stays disconnected (single auto net dropped).
        doc.ports[0].location = CoordPoint::new(Coord::from_raw(98_000_000), dxp(650.0));
        assert!(Netlist::from_sch_document(&doc).nets.is_empty());
    }

    #[test]
    fn schdoc_netlist_can_emit_ports_as_pseudo_connections() {
        let mut doc = sch::Document::default();
        doc.components.push(placed(
            "R1",
            "R0402",
            vec![pin_at("1", coord_pt(0.0, 0.0), PinOrientation::Right, 10.0)],
        ));
        let mut w = Wire::default();
        w.vertices = vec![coord_pt(10.0, 0.0), coord_pt(100.0, 0.0)];
        doc.wires.push(w);
        let mut port = sch::primitives::Port::default();
        port.name = "RESET_N".into();
        port.location = coord_pt(100.0, 0.0);
        port.width = Coord::from_mils(50.0);
        doc.ports.push(port);
        // A second port touching nothing must not produce a net.
        let mut lonely = sch::primitives::Port::default();
        lonely.name = "LONELY".into();
        lonely.location = coord_pt(500.0, 500.0);
        lonely.width = Coord::from_mils(50.0);
        doc.ports.push(lonely);

        let nl = Netlist::from_sch_document_with(
            &doc,
            &SchNetlistOptions {
                include_ports: true,
                ..Default::default()
            },
        );
        assert_eq!(nl.nets.len(), 1);
        let conns: Vec<(String, String)> = nl.nets[0]
            .connections
            .iter()
            .map(|c| (c.designator.clone(), c.pad.clone()))
            .collect();
        assert_eq!(
            conns,
            vec![
                (PORT_DESIGNATOR.to_string(), "RESET_N".to_string()),
                ("R1".to_string(), "1".to_string()),
            ]
        );
    }

    #[test]
    fn schdoc_netlist_can_emit_sheet_entries_as_connections() {
        // Two sheet symbols; a right-side entry of the first (slot 3 ->
        // (300,470)) is wired to a left-side entry of the second (slot 3 ->
        // (600,470)). Names come from RECORD=32 annotations one grid above
        // each symbol's top-left corner.
        let mut doc = sch::Document::default();
        let mut a = sch::primitives::SheetSymbol::default();
        a.location = dxp_pt(100.0, 500.0);
        a.x_size = dxp(200.0);
        a.y_size = dxp(300.0);
        let mut ea = sch::primitives::SheetEntry::default();
        ea.name = "RESET_N".into();
        ea.side = 1;
        ea.distance_from_top = dxp(30.0);
        a.entries.push(ea);
        doc.sheet_symbols.push(a);
        let mut b = sch::primitives::SheetSymbol::default();
        b.location = dxp_pt(600.0, 500.0);
        b.x_size = dxp(200.0);
        b.y_size = dxp(300.0);
        let mut eb = sch::primitives::SheetEntry::default();
        eb.name = "SYS_RESET_N".into();
        eb.side = 0;
        eb.distance_from_top = dxp(30.0);
        b.entries.push(eb);
        doc.sheet_symbols.push(b);
        for (x, y, text) in [(100, 510, "IO240_1"), (600, 510, "SUPERVISOR")] {
            let mut ann = BTreeMap::new();
            ann.insert("Location.X".to_string(), x.to_string());
            ann.insert("Location.Y".to_string(), y.to_string());
            ann.insert("Text".to_string(), text.to_string());
            doc.sheet_name_annotations.push(ann);
        }
        let mut w = Wire::default();
        w.vertices = vec![dxp_pt(300.0, 470.0), dxp_pt(600.0, 470.0)];
        doc.wires.push(w);

        assert!(Netlist::from_sch_document(&doc).nets.is_empty());
        let nl = Netlist::from_sch_document_with(
            &doc,
            &SchNetlistOptions {
                include_sheet_entries: true,
                ..Default::default()
            },
        );
        assert_eq!(nl.nets.len(), 1);
        let conns: Vec<(String, String)> = nl.nets[0]
            .connections
            .iter()
            .map(|c| (c.designator.clone(), c.pad.clone()))
            .collect();
        assert_eq!(
            conns,
            vec![
                ("IO240_1".to_string(), "RESET_N".to_string()),
                ("SUPERVISOR".to_string(), "SYS_RESET_N".to_string()),
            ]
        );
    }

    #[test]
    fn schdoc_netlist_footprint_prefers_current_pcblib_model() {
        let mut comp = placed("MH1", "SMTSO30", vec![]);
        for (name, current) in [("SMTSO3030", false), ("SMTSO3050", true), ("SMTSO3060", false)] {
            let mut imp = sch::Implementation::default();
            imp.model_name = Some(name.into());
            imp.model_type = Some("PCBLIB".into());
            imp.is_current = current;
            comp.implementations.push(imp);
        }
        let mut doc = sch::Document::default();
        doc.components.push(comp);
        let nl = Netlist::from_sch_document(&doc);
        assert_eq!(nl.components[0].footprint.as_deref(), Some("SMTSO3050"));
    }
}
