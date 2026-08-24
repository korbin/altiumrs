//! Schematic symbol/component: pins plus graphical primitives.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::implementation::Implementation;
use super::primitives::{
    Arc, Bezier, Ellipse, EllipticalArc, Image, Junction, Label, Line, NetLabel, Parameter, Pie,
    Pin, Polygon, Polyline, PowerObject, PrimitiveCommon, Rectangle, RoundedRectangle, Symbol,
    TextFrame, Wire,
};
use crate::coord::{Coord, CoordPoint, CoordRect};

/// Raw on-disk record for a schematic component's `Data` stream.
///
/// `flag` is the high-byte flag in the size header (`0x01` for binary pin
/// records, `0x00` for parameter strings), `bytes` is the body verbatim.
/// The reader keeps these so that round-tripping unmodelled records is exact.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RawRecord {
    pub flag: u8,
    #[cfg_attr(feature = "serde", serde(default, with = "crate::serde_bytes::b64"))]
    pub bytes: Vec<u8>,
}

/// A schematic symbol / component definition.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Component {
    pub name: String,
    pub description: Option<String>,
    pub comment: Option<String>,
    pub designator_prefix: Option<String>,
    pub part_count: i32,

    pub location: CoordPoint,
    pub orientation: i32,
    pub current_part_id: i32,
    pub display_mode_count: i32,
    pub display_mode: i32,
    pub show_hidden_pins: bool,
    pub show_hidden_fields: bool,
    pub library_path: Option<String>,
    pub source_library_name: Option<String>,
    pub lib_reference: Option<String>,
    pub design_item_id: Option<String>,
    pub component_kind: i32,

    pub override_colors: bool,
    pub color: i32,
    pub area_color: i32,
    pub designator_locked: bool,
    pub part_id_locked: bool,
    pub symbol_reference: Option<String>,
    pub sheet_part_file_name: Option<String>,
    pub target_file_name: Option<String>,
    pub alias_list: Option<String>,
    pub all_pin_count: i32,
    pub graphically_locked: bool,
    pub pins_moveable: bool,
    pub pin_color: i32,

    pub database_library_name: Option<String>,
    pub database_table_name: Option<String>,
    pub library_identifier: Option<String>,
    pub vault_guid: Option<String>,
    pub item_guid: Option<String>,
    pub revision_guid: Option<String>,
    pub disabled: bool,
    pub dimmed: bool,
    pub configuration_parameters: Option<String>,
    pub configurator_name: Option<String>,
    pub not_used_b_table_name: Option<String>,
    pub owner_part_id: i32,
    pub unique_id: Option<String>,
    pub display_field_names: bool,
    pub is_mirrored: bool,
    pub is_unmanaged: bool,
    pub is_user_configurable: bool,
    pub lib_identifier_kind: i32,
    pub owner_part_display_mode: i32,
    pub revision_details: Option<String>,
    pub revision_hrid: Option<String>,
    pub revision_state: Option<String>,
    pub revision_status: Option<String>,
    pub symbol_item_guid: Option<String>,
    pub symbol_items_guid: Option<String>,
    pub symbol_revision_guid: Option<String>,
    pub symbol_vault_guid: Option<String>,
    pub use_db_table_name: bool,
    pub use_library_name: bool,
    pub variant_option: i32,
    pub vault_hrid: Option<String>,
    pub generic_component_template_guid: Option<String>,

    pub pins: Vec<Pin>,
    pub lines: Vec<Line>,
    pub rectangles: Vec<Rectangle>,
    pub rounded_rectangles: Vec<RoundedRectangle>,
    pub polygons: Vec<Polygon>,
    pub polylines: Vec<Polyline>,
    pub ellipses: Vec<Ellipse>,
    pub elliptical_arcs: Vec<EllipticalArc>,
    pub pies: Vec<Pie>,
    pub arcs: Vec<Arc>,
    pub beziers: Vec<Bezier>,
    pub labels: Vec<Label>,
    pub net_labels: Vec<NetLabel>,
    pub junctions: Vec<Junction>,
    pub parameters: Vec<Parameter>,
    pub text_frames: Vec<TextFrame>,
    pub images: Vec<Image>,
    pub symbols: Vec<Symbol>,
    pub power_objects: Vec<PowerObject>,
    pub wires: Vec<Wire>,
    pub implementations: Vec<Implementation>,

    /// Records read from the `Data` stream, kept verbatim so round-tripping is
    /// byte-for-byte (within the data block).
    pub raw_records: Vec<RawRecord>,
    /// Streams beneath the component's storage that we don't model (e.g.
    /// `PinFrac`, `PinSymbolLineWidth`, `PinTextData`).
    #[cfg_attr(feature = "serde", serde(default, with = "crate::serde_bytes::b64_map"))]
    pub additional_streams: BTreeMap<String, Vec<u8>>,
}

impl Default for Component {
    fn default() -> Self {
        // Defaults match the values real Altium emits for fresh symbols. Without
        // these, the editor renders the symbol body in black and shows the
        // "Library Path: <unset>" warning even on round-trip.
        Self {
            name: String::new(),
            description: None,
            comment: None,
            designator_prefix: None,
            part_count: 1,

            location: CoordPoint::default(),
            orientation: 0,
            current_part_id: 1,
            display_mode_count: 1,
            display_mode: 0,
            show_hidden_pins: false,
            show_hidden_fields: false,
            library_path: Some("*".into()),
            source_library_name: Some("*".into()),
            lib_reference: None,
            design_item_id: None,
            component_kind: 0,

            override_colors: false,
            color: 128,             // 0x80 — dark border colour
            area_color: 11_599_871, // 0xB1FFFF — light yellow body fill
            designator_locked: false,
            part_id_locked: true,
            symbol_reference: None,
            sheet_part_file_name: Some("*".into()),
            target_file_name: Some("*".into()),
            alias_list: None,
            all_pin_count: 0,
            graphically_locked: false,
            pins_moveable: false,
            pin_color: 0,

            database_library_name: None,
            database_table_name: None,
            library_identifier: None,
            vault_guid: None,
            item_guid: None,
            revision_guid: None,
            disabled: false,
            dimmed: false,
            configuration_parameters: None,
            configurator_name: None,
            not_used_b_table_name: None,
            owner_part_id: -1,
            unique_id: None,
            display_field_names: false,
            is_mirrored: false,
            is_unmanaged: false,
            is_user_configurable: false,
            lib_identifier_kind: 0,
            owner_part_display_mode: 0,
            revision_details: None,
            revision_hrid: None,
            revision_state: None,
            revision_status: None,
            symbol_item_guid: None,
            symbol_items_guid: None,
            symbol_revision_guid: None,
            symbol_vault_guid: None,
            use_db_table_name: false,
            use_library_name: false,
            variant_option: 0,
            vault_hrid: None,
            generic_component_template_guid: None,

            pins: Vec::new(),
            lines: Vec::new(),
            rectangles: Vec::new(),
            rounded_rectangles: Vec::new(),
            polygons: Vec::new(),
            polylines: Vec::new(),
            ellipses: Vec::new(),
            elliptical_arcs: Vec::new(),
            pies: Vec::new(),
            arcs: Vec::new(),
            beziers: Vec::new(),
            labels: Vec::new(),
            net_labels: Vec::new(),
            junctions: Vec::new(),
            parameters: Vec::new(),
            text_frames: Vec::new(),
            images: Vec::new(),
            symbols: Vec::new(),
            power_objects: Vec::new(),
            wires: Vec::new(),
            implementations: Vec::new(),

            raw_records: Vec::new(),
            additional_streams: BTreeMap::new(),
        }
    }
}

impl Component {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Append the canonical Designator (`U?` / `R?` / …) and Comment
    /// parameters that real Altium-saved symbols always carry. The
    /// Designator goes out as a typed `RECORD=34` (the writer routes any
    /// `Parameter` whose name is `Designator` to that header tag); the
    /// Comment is a `RECORD=41` referencing the `Manufacturer Part`
    /// parameter via the `=Manufacturer Part` syntax. Both are placed at
    /// Altium's standard offsets near the top-left of the symbol.
    ///
    /// `prefix` is the bare letter (`"U"`, `"R"`, …); the `?` suffix is
    /// added automatically. `font_id` indexes the FileHeader font slot —
    /// pass 1 for the default Times New Roman slot, or 2 when a custom
    /// font has been registered via `Library::font_override`.
    pub fn add_default_labels(&mut self, prefix: &str, font_id: i32) {
        let designator_text = if prefix.is_empty() {
            "U?".to_string()
        } else {
            format!("{prefix}?")
        };
        self.parameters.push(Parameter {
            name: "Designator".to_string(),
            value: designator_text,
            location: CoordPoint::new(Coord::from_mils(-50.0), Coord::from_mils(50.0)),
            is_visible: true,
            is_read_only: true,
            font_id,
            color: 0x80_0000, // BGR for Altium's classic dark-red Designator/Comment.
            common: PrimitiveCommon {
                owner_part_id: -1,
                is_not_accessible: false,
                ..Default::default()
            },
            ..Default::default()
        });
        self.parameters.push(Parameter {
            name: "Comment".to_string(),
            value: "=Manufacturer Part".to_string(),
            location: CoordPoint::new(Coord::from_mils(-50.0), Coord::from_mils(-150.0)),
            is_visible: true,
            font_id,
            color: 0x80_0000,
            common: PrimitiveCommon {
                owner_part_id: -1,
                is_not_accessible: false,
                ..Default::default()
            },
            ..Default::default()
        });
    }

    /// Bounding box across every graphical primitive owned by the component.
    pub fn bounds(&self) -> CoordRect {
        let mut acc = CoordRect::EMPTY;
        for p in &self.pins {
            acc = acc.union(p.bounds());
        }
        for r in &self.rectangles {
            acc = acc.union(CoordRect::from_corners(r.corner1, r.corner2));
        }
        for r in &self.rounded_rectangles {
            acc = acc.union(CoordRect::from_corners(r.corner1, r.corner2));
        }
        for line in &self.lines {
            acc = acc.union(CoordRect::from_corners(line.start, line.end));
        }
        for poly in &self.polygons {
            for v in &poly.vertices {
                acc = acc.union_point(*v);
            }
        }
        for poly in &self.polylines {
            for v in &poly.vertices {
                acc = acc.union_point(*v);
            }
        }
        for arc in &self.arcs {
            let r = arc.radius;
            acc = acc.union(CoordRect::from_xyxy(
                arc.center.x - r,
                arc.center.y - r,
                arc.center.x + r,
                arc.center.y + r,
            ));
        }
        for e in &self.ellipses {
            acc = acc.union(CoordRect::from_xyxy(
                e.center.x - e.radius_x,
                e.center.y - e.radius_y,
                e.center.x + e.radius_x,
                e.center.y + e.radius_y,
            ));
        }
        acc
    }
}
