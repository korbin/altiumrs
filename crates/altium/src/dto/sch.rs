//! Schematic record DTOs.

use altium_derive::AltiumRecord;

use crate::sch::primitives::PrimitiveCommon;

/// Defines a schematic-record DTO with the standard ownership / visibility
/// fields shared by every record type, plus any record-specific fields.
macro_rules! sch_record {
    (
        $(#[$meta:meta])*
        $kind:literal => $name:ident { $($body:tt)* }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq, AltiumRecord)]
        #[altium(record = $kind)]
        pub struct $name {
            #[altium(name = "OWNERINDEX")]
            pub owner_index: i32,
            #[altium(name = "ISNOTACCESIBLE")]
            pub is_not_accessible: bool,
            #[altium(name = "INDEXINSHEET")]
            pub index_in_sheet: i32,
            #[altium(name = "OWNERPARTID")]
            pub owner_part_id: i32,
            #[altium(name = "OWNERPARTDISPLAYMODE")]
            pub owner_part_display_mode: i32,
            #[altium(name = "GRAPHICALLYLOCKED")]
            pub graphically_locked: bool,
            #[altium(name = "DISABLED")]
            pub disabled: bool,
            #[altium(name = "DIMMED")]
            pub dimmed: bool,
            #[altium(name = "UNIQUEID")]
            pub unique_id: Option<String>,
            $($body)*
        }

        impl $name {
            /// Copy the nine shared fields out of `common` into this DTO.
            pub fn apply_common_from(&mut self, common: &PrimitiveCommon) {
                self.owner_index = common.owner_index;
                self.is_not_accessible = common.is_not_accessible;
                self.index_in_sheet = common.index_in_sheet;
                self.owner_part_id = common.owner_part_id;
                self.owner_part_display_mode = common.owner_part_display_mode;
                self.graphically_locked = common.graphically_locked;
                self.disabled = common.disabled;
                self.dimmed = common.dimmed;
                self.unique_id = common.unique_id.clone();
            }

            /// Build a `PrimitiveCommon` from this DTO's nine shared fields.
            pub fn extract_common(&self) -> PrimitiveCommon {
                PrimitiveCommon {
                    owner_index: self.owner_index,
                    is_not_accessible: self.is_not_accessible,
                    index_in_sheet: self.index_in_sheet,
                    owner_part_id: self.owner_part_id,
                    owner_part_display_mode: self.owner_part_display_mode,
                    graphically_locked: self.graphically_locked,
                    disabled: self.disabled,
                    dimmed: self.dimmed,
                    unique_id: self.unique_id.clone(),
                }
            }
        }
    };
}

sch_record! {
    /// Component (record 1).
    "1" => ComponentDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "LIBREFERENCE")] pub lib_reference: Option<String>,
        #[altium(name = "COMPONENTDESCRIPTION")] pub component_description: Option<String>,
        #[altium(name = "PARTCOUNT")] pub part_count: i32,
        #[altium(name = "DISPLAYMODECOUNT")] pub display_mode_count: i32,
        #[altium(name = "DISPLAYMODE")] pub display_mode: i32,
        #[altium(name = "ORIENTATION")] pub orientation: i32,
        #[altium(name = "CURRENTPARTID")] pub current_part_id: i32,
        #[altium(name = "SHOWHIDDENPINS")] pub show_hidden_pins: bool,
        #[altium(name = "LIBRARYPATH")] pub library_path: Option<String>,
        #[altium(name = "SOURCELIBRARYNAME")] pub source_library_name: Option<String>,
        #[altium(name = "SHEETPARTFILENAME")] pub sheet_part_file_name: Option<String>,
        #[altium(name = "TARGETFILENAME")] pub target_file_name: Option<String>,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "OVERIDECOLORS")] pub override_colors: bool,
        #[altium(name = "DESIGNATORPREFIX")] pub designator_prefix: Option<String>,
        #[altium(name = "DESIGNATORLOCKED")] pub designator_locked: bool,
        #[altium(name = "PARTIDLOCKED")] pub part_id_locked: bool,
        #[altium(name = "DESIGNITEMID")] pub design_item_id: Option<String>,
        #[altium(name = "COMPONENTKIND")] pub component_kind: i32,
        #[altium(name = "ALIASLIST")] pub alias_list: Option<String>,
        #[altium(name = "ALLPINCOUNT")] pub all_pin_count: i32,
        #[altium(name = "SYMBOLREFERENCE")] pub symbol_reference: String,
        #[altium(name = "DATABASELIBRARYNAME")] pub database_library_name: Option<String>,
        #[altium(name = "DATABASETABLENAME")] pub database_table_name: Option<String>,
        #[altium(name = "LIBRARYIDENTIFIER")] pub library_identifier: Option<String>,
        #[altium(name = "VAULTGUID")] pub vault_guid: Option<String>,
        #[altium(name = "ITEMGUID")] pub item_guid: Option<String>,
        #[altium(name = "REVISIONGUID")] pub revision_guid: Option<String>,
        #[altium(name = "PINSMOVEABLE")] pub pins_moveable: bool,
        #[altium(name = "PINCOLOR")] pub pin_color: i32,
        #[altium(name = "SHOWHIDDENFIELDS")] pub show_hidden_fields: bool,
        #[altium(name = "NOTUSEDBTABLENAME")] pub not_used_b_table_name: Option<String>,
        #[altium(name = "CONFIGURATIONPARAMETERS")] pub configuration_parameters: Option<String>,
        #[altium(name = "CONFIGURATORNAME")] pub configurator_name: Option<String>,
        #[altium(name = "DISPLAYFIELDNAMES")] pub display_field_names: bool,
        #[altium(name = "ISMIRRORED")] pub is_mirrored: bool,
        #[altium(name = "ISUNMANAGED")] pub is_unmanaged: bool,
        #[altium(name = "ISUSERCONFIGURABLE")] pub is_user_configurable: bool,
        #[altium(name = "LIBIDENTIFIERKIND")] pub lib_identifier_kind: i32,
        #[altium(name = "REVISIONDETAILS")] pub revision_details: Option<String>,
        #[altium(name = "REVISIONHRID")] pub revision_hrid: Option<String>,
        #[altium(name = "REVISIONSTATE")] pub revision_state: Option<String>,
        #[altium(name = "REVISIONSTATUS")] pub revision_status: Option<String>,
        #[altium(name = "SYMBOLITEMGUID")] pub symbol_item_guid: Option<String>,
        #[altium(name = "SYMBOLITEMSGUID")] pub symbol_items_guid: Option<String>,
        #[altium(name = "SYMBOLREVISIONGUID")] pub symbol_revision_guid: Option<String>,
        #[altium(name = "SYMBOLVAULTGUID")] pub symbol_vault_guid: Option<String>,
        #[altium(name = "USEDBTABLENAME")] pub use_db_table_name: bool,
        #[altium(name = "USELIBRARYNAME")] pub use_library_name: bool,
        #[altium(name = "VARIANTOPTION")] pub variant_option: i32,
        #[altium(name = "VAULTHRID")] pub vault_hrid: Option<String>,
        #[altium(name = "GENERICCOMPONENTTEMPLATEGUID")] pub generic_component_template_guid: Option<String>,
    }
}

sch_record! {
    /// Pin (record 2).
    "2" => PinDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "NAME")] pub name: Option<String>,
        #[altium(name = "DESIGNATOR")] pub designator: Option<String>,
        #[altium(name = "DESCRIPTION")] pub description: Option<String>,
        #[altium(name = "PINLENGTH")] pub pin_length: i32,
        #[altium(name = "PINLENGTH_FRAC")] pub pin_length_frac: i32,
        #[altium(name = "PINCONGLOMERATE")] pub pin_conglomerate: i32,
        #[altium(name = "ELECTRICAL")] pub electrical: i32,
        #[altium(name = "FORMALTYPE")] pub formal_type: i32,
        #[altium(name = "SYMBOL_INNEREDGE")] pub symbol_inner_edge: i32,
        #[altium(name = "SYMBOL_OUTEREDGE")] pub symbol_outer_edge: i32,
        #[altium(name = "SYMBOL_INSIDE")] pub symbol_inside: i32,
        #[altium(name = "SYMBOL_OUTSIDE")] pub symbol_outside: i32,
        #[altium(name = "SYMBOL_LINEWIDTH")] pub symbol_line_width: i32,
        #[altium(name = "SWAPIDPART")] pub swap_id_part: i32,
        #[altium(name = "PINPROPAGATIONDELAY")] pub pin_propagation_delay: f64,
        #[altium(name = "DESIGNATOR.CUSTOMFONTID")] pub designator_custom_font_id: i32,
        #[altium(name = "NAME.CUSTOMFONTID")] pub name_custom_font_id: i32,
        #[altium(name = "WIDTH")] pub width: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "DEFAULTVALUE")] pub default_value: Option<String>,
        #[altium(name = "ISHIDDEN")] pub is_hidden: bool,
        #[altium(name = "DESIGNATOR.CUSTOMCOLOR")] pub designator_custom_color: i32,
        #[altium(name = "DESIGNATOR.CUSTOMPOSITION.MARGIN")] pub designator_custom_position_margin: i32,
        #[altium(name = "DESIGNATOR.CUSTOMPOSITION.ROTATIONANCHOR")] pub designator_custom_position_rotation_anchor: i32,
        #[altium(name = "DESIGNATOR.CUSTOMPOSITION.ROTATIONRELATIVE")] pub designator_custom_position_rotation_relative: bool,
        #[altium(name = "DESIGNATOR.FONTMODE")] pub designator_font_mode: i32,
        #[altium(name = "DESIGNATOR.POSITIONMODE")] pub designator_position_mode: i32,
        #[altium(name = "NAME.CUSTOMCOLOR")] pub name_custom_color: i32,
        #[altium(name = "NAME.CUSTOMPOSITION.MARGIN")] pub name_custom_position_margin: i32,
        #[altium(name = "NAME.CUSTOMPOSITION.ROTATIONANCHOR")] pub name_custom_position_rotation_anchor: i32,
        #[altium(name = "NAME.CUSTOMPOSITION.ROTATIONRELATIVE")] pub name_custom_position_rotation_relative: bool,
        #[altium(name = "NAME.FONTMODE")] pub name_font_mode: i32,
        #[altium(name = "NAME.POSITIONMODE")] pub name_position_mode: i32,
        #[altium(name = "SWAPID_PAIR")] pub swap_id_pair: Option<String>,
        #[altium(name = "SWAPID_PARTPIN")] pub swap_id_part_pin: Option<String>,
        #[altium(name = "SWAPID_PIN")] pub swap_id_pin: Option<String>,
        #[altium(name = "HIDDENNETNAME")] pub hidden_net_name: Option<String>,
        #[altium(name = "PINPACKAGELENGTH")] pub pin_package_length: i32,
    }
}

sch_record! {
    /// Symbol reference (record 3).
    "3" => SymbolDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "SYMBOL")] pub symbol: i32,
        #[altium(name = "ISMIRRORED")] pub is_mirrored: bool,
        #[altium(name = "ORIENTATION")] pub orientation: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "SCALEFACTOR")] pub scale_factor: i32,
    }
}

sch_record! {
    /// Label (record 4).
    "4" => LabelDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "TEXT")] pub text: Option<String>,
        #[altium(name = "FONTID")] pub font_id: i32,
        #[altium(name = "ORIENTATION")] pub orientation: i32,
        #[altium(name = "JUSTIFICATION")] pub justification: i32,
        #[altium(name = "ISMIRRORED")] pub is_mirrored: bool,
        #[altium(name = "ISHIDDEN")] pub is_hidden: bool,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
    }
}

sch_record! {
    /// Bezier curve (record 5).
    "5" => BezierDto {
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "LOCATIONCOUNT")] pub location_count: i32,
        // Cubic-bezier control point shortcuts (4 points). Codec iterates
        // raw `X{i}/Y{i}` for points beyond these.
        #[altium(name = "X1")] pub x1: i32,
        #[altium(name = "X1_FRAC")] pub x1_frac: i32,
        #[altium(name = "Y1")] pub y1: i32,
        #[altium(name = "Y1_FRAC")] pub y1_frac: i32,
        #[altium(name = "X2")] pub x2: i32,
        #[altium(name = "X2_FRAC")] pub x2_frac: i32,
        #[altium(name = "Y2")] pub y2: i32,
        #[altium(name = "Y2_FRAC")] pub y2_frac: i32,
        #[altium(name = "X3")] pub x3: i32,
        #[altium(name = "X3_FRAC")] pub x3_frac: i32,
        #[altium(name = "Y3")] pub y3: i32,
        #[altium(name = "Y3_FRAC")] pub y3_frac: i32,
        #[altium(name = "X4")] pub x4: i32,
        #[altium(name = "X4_FRAC")] pub x4_frac: i32,
        #[altium(name = "Y4")] pub y4: i32,
        #[altium(name = "Y4_FRAC")] pub y4_frac: i32,
    }
}

sch_record! {
    /// Polyline (record 6).
    "6" => PolylineDto {
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "LINESTYLE")] pub line_style: i32,
        #[altium(name = "ENDLINESHAPE")] pub end_line_shape: i32,
        #[altium(name = "STARTLINESHAPE")] pub start_line_shape: i32,
        #[altium(name = "LINESHAPESIZE")] pub line_shape_size: i32,
        #[altium(name = "TRANSPARENT")] pub transparent: bool,
        #[altium(name = "LOCATIONCOUNT")] pub location_count: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
        #[altium(name = "X1")] pub x1: i32,
        #[altium(name = "X1_FRAC")] pub x1_frac: i32,
        #[altium(name = "Y1")] pub y1: i32,
        #[altium(name = "Y1_FRAC")] pub y1_frac: i32,
        #[altium(name = "X2")] pub x2: i32,
        #[altium(name = "X2_FRAC")] pub x2_frac: i32,
        #[altium(name = "Y2")] pub y2: i32,
        #[altium(name = "Y2_FRAC")] pub y2_frac: i32,
        #[altium(name = "X3")] pub x3: i32,
        #[altium(name = "X3_FRAC")] pub x3_frac: i32,
        #[altium(name = "Y3")] pub y3: i32,
        #[altium(name = "Y3_FRAC")] pub y3_frac: i32,
        #[altium(name = "X4")] pub x4: i32,
        #[altium(name = "X4_FRAC")] pub x4_frac: i32,
        #[altium(name = "Y4")] pub y4: i32,
        #[altium(name = "Y4_FRAC")] pub y4_frac: i32,
    }
}

sch_record! {
    /// Polygon (record 7).
    "7" => PolygonDto {
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
        #[altium(name = "TRANSPARENT")] pub transparent: bool,
        #[altium(name = "LOCATIONCOUNT")] pub location_count: i32,
        // Shortcut fields for the first four vertices — matches the C# DTO's
        // schema. Our reader/writer iterates raw `X{i}/Y{i}` keys, so these
        // are functionally redundant but kept for round-trip parameter parity.
        #[altium(name = "X1")] pub x1: i32,
        #[altium(name = "X1_FRAC")] pub x1_frac: i32,
        #[altium(name = "Y1")] pub y1: i32,
        #[altium(name = "Y1_FRAC")] pub y1_frac: i32,
        #[altium(name = "X2")] pub x2: i32,
        #[altium(name = "X2_FRAC")] pub x2_frac: i32,
        #[altium(name = "Y2")] pub y2: i32,
        #[altium(name = "Y2_FRAC")] pub y2_frac: i32,
        #[altium(name = "X3")] pub x3: i32,
        #[altium(name = "X3_FRAC")] pub x3_frac: i32,
        #[altium(name = "Y3")] pub y3: i32,
        #[altium(name = "Y3_FRAC")] pub y3_frac: i32,
        #[altium(name = "X4")] pub x4: i32,
        #[altium(name = "X4_FRAC")] pub x4_frac: i32,
        #[altium(name = "Y4")] pub y4: i32,
        #[altium(name = "Y4_FRAC")] pub y4_frac: i32,
    }
}

sch_record! {
    /// Ellipse (record 8).
    "8" => EllipseDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "RADIUS")] pub radius: i32,
        #[altium(name = "RADIUS_FRAC")] pub radius_frac: i32,
        #[altium(name = "SECONDARYRADIUS")] pub secondary_radius: i32,
        #[altium(name = "SECONDARYRADIUS_FRAC")] pub secondary_radius_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
        #[altium(name = "TRANSPARENT")] pub transparent: bool,
    }
}

sch_record! {
    /// Pie (record 9).
    "9" => PieDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "RADIUS")] pub radius: i32,
        #[altium(name = "RADIUS_FRAC")] pub radius_frac: i32,
        #[altium(name = "STARTANGLE")] pub start_angle: f64,
        #[altium(name = "ENDANGLE")] pub end_angle: f64,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
        #[altium(name = "TRANSPARENT")] pub transparent: bool,
    }
}

sch_record! {
    /// Rounded rectangle (record 10).
    "10" => RoundedRectangleDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "CORNER.X")] pub corner_x: i32,
        #[altium(name = "CORNER.X_FRAC")] pub corner_x_frac: i32,
        #[altium(name = "CORNER.Y")] pub corner_y: i32,
        #[altium(name = "CORNER.Y_FRAC")] pub corner_y_frac: i32,
        #[altium(name = "CORNERXRADIUS")] pub corner_x_radius: i32,
        #[altium(name = "CORNERYRADIUS")] pub corner_y_radius: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "LINESTYLE")] pub line_style: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
        #[altium(name = "TRANSPARENT")] pub transparent: bool,
    }
}

sch_record! {
    /// Elliptical arc (record 11).
    "11" => EllipticalArcDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "RADIUS")] pub radius: i32,
        #[altium(name = "RADIUS_FRAC")] pub radius_frac: i32,
        #[altium(name = "SECONDARYRADIUS")] pub secondary_radius: i32,
        #[altium(name = "SECONDARYRADIUS_FRAC")] pub secondary_radius_frac: i32,
        #[altium(name = "STARTANGLE")] pub start_angle: f64,
        #[altium(name = "ENDANGLE")] pub end_angle: f64,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "LINEWIDTH_FRAC")] pub line_width_frac: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
    }
}

sch_record! {
    /// Arc (record 12).
    "12" => ArcDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "RADIUS")] pub radius: i32,
        #[altium(name = "RADIUS_FRAC")] pub radius_frac: i32,
        #[altium(name = "STARTANGLE")] pub start_angle: f64,
        #[altium(name = "ENDANGLE")] pub end_angle: f64,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
    }
}

sch_record! {
    /// Line (record 13).
    "13" => LineDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "CORNER.X")] pub corner_x: i32,
        #[altium(name = "CORNER.X_FRAC")] pub corner_x_frac: i32,
        #[altium(name = "CORNER.Y")] pub corner_y: i32,
        #[altium(name = "CORNER.Y_FRAC")] pub corner_y_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "LINESTYLE")] pub line_style: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
    }
}

sch_record! {
    /// Rectangle (record 14).
    "14" => RectangleDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "CORNER.X")] pub corner_x: i32,
        #[altium(name = "CORNER.X_FRAC")] pub corner_x_frac: i32,
        #[altium(name = "CORNER.Y")] pub corner_y: i32,
        #[altium(name = "CORNER.Y_FRAC")] pub corner_y_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
        #[altium(name = "LINESTYLE")] pub line_style: i32,
        #[altium(name = "TRANSPARENT")] pub transparent: bool,
    }
}

sch_record! {
    /// Sheet symbol (record 15).
    "15" => SheetSymbolDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "XSIZE")] pub x_size: i32,
        #[altium(name = "YSIZE")] pub y_size: i32,
        #[altium(name = "ISMIRRORED")] pub is_mirrored: bool,
        #[altium(name = "FILENAME")] pub file_name: Option<String>,
        #[altium(name = "SHEETNAME")] pub sheet_name: Option<String>,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
        #[altium(name = "SHOWHIDDENFIELDS")] pub show_hidden_fields: bool,
        #[altium(name = "SYMBOLTYPE")] pub symbol_type: i32,
        #[altium(name = "DESIGNITEMID")] pub design_item_id: Option<String>,
        #[altium(name = "ITEMGUID")] pub item_guid: Option<String>,
        #[altium(name = "LIBIDENTIFIERKIND")] pub lib_identifier_kind: i32,
        #[altium(name = "LIBRARYIDENTIFIER")] pub library_identifier: Option<String>,
        #[altium(name = "REVISIONGUID")] pub revision_guid: Option<String>,
        #[altium(name = "SOURCELIBNAME")] pub source_library_name: Option<String>,
        #[altium(name = "VAULTGUID")] pub vault_guid: Option<String>,
    }
}

sch_record! {
    /// Sheet entry (record 16).
    "16" => SheetEntryDto {
        #[altium(name = "SIDE")] pub side: i32,
        #[altium(name = "DISTANCEFROMTOP")] pub distance_from_top: i32,
        #[altium(name = "NAME")] pub name: Option<String>,
        #[altium(name = "IOTYPE")] pub io_type: i32,
        #[altium(name = "STYLE")] pub style: i32,
        #[altium(name = "ARROWKIND")] pub arrow_kind: i32,
        #[altium(name = "HARNESSTYPE")] pub harness_type: Option<String>,
        #[altium(name = "FONTID")] pub font_id: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "TEXTCOLOR")] pub text_color: i32,
        #[altium(name = "TEXTSTYLE")] pub text_style: i32,
        #[altium(name = "HARNESSCOLOR")] pub harness_color: i32,
    }
}

sch_record! {
    /// Power port (record 17).
    "17" => PowerObjectDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "TEXT")] pub text: Option<String>,
        #[altium(name = "STYLE")] pub style: i32,
        #[altium(name = "ORIENTATION")] pub orientation: i32,
        #[altium(name = "SHOWNETNAME")] pub show_net_name: bool,
        #[altium(name = "ISCROSSSHEETCONNECTOR")] pub is_cross_sheet_connector: bool,
        #[altium(name = "FONTID")] pub font_id: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "ISCUSTOMSTYLE")] pub is_custom_style: bool,
        #[altium(name = "ISMIRRORED")] pub is_mirrored: bool,
        #[altium(name = "JUSTIFICATION")] pub justification: i32,
    }
}

sch_record! {
    /// Port (record 18).
    "18" => PortDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "NAME")] pub name: Option<String>,
        #[altium(name = "IOTYPE")] pub io_type: i32,
        #[altium(name = "STYLE")] pub style: i32,
        #[altium(name = "ALIGNMENT")] pub alignment: i32,
        #[altium(name = "WIDTH")] pub width: i32,
        #[altium(name = "HEIGHT")] pub height: i32,
        #[altium(name = "BORDERWIDTH")] pub border_width: i32,
        #[altium(name = "AUTOSIZE")] pub auto_size: bool,
        #[altium(name = "CONNECTEDEND")] pub connected_end: i32,
        #[altium(name = "CROSSREFERENCE")] pub cross_reference: Option<String>,
        #[altium(name = "SHOWNETNAME")] pub show_net_name: bool,
        #[altium(name = "HARNESSTYPE")] pub harness_type: Option<String>,
        #[altium(name = "HARNESSCOLOR")] pub harness_color: i32,
        #[altium(name = "ISCUSTOMSTYLE")] pub is_custom_style: bool,
        #[altium(name = "FONTID")] pub font_id: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "TEXTCOLOR")] pub text_color: i32,
    }
}

sch_record! {
    /// No-ERC marker (record 22).
    "22" => NoErcDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "ORIENTATION")] pub orientation: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "ISACTIVE")] pub is_active: bool,
        #[altium(name = "SYMBOL")] pub symbol: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "SUPPRESSALL")] pub suppress_all: bool,
        #[altium(name = "ERRORKINDSET_TOSUPPRESS")] pub error_kind_set_to_suppress: Option<String>,
    }
}

sch_record! {
    /// Net label (record 25).
    "25" => NetLabelDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "TEXT")] pub text: Option<String>,
        #[altium(name = "FONTID")] pub font_id: i32,
        #[altium(name = "ORIENTATION")] pub orientation: i32,
        #[altium(name = "JUSTIFICATION")] pub justification: i32,
        #[altium(name = "ISMIRRORED")] pub is_mirrored: bool,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
    }
}

sch_record! {
    /// Bus (record 26).
    "26" => BusDto {
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "LINESTYLE")] pub line_style: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "LOCATIONCOUNT")] pub location_count: i32,
    }
}

sch_record! {
    /// Wire (record 27).
    "27" => WireDto {
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "LINESTYLE")] pub line_style: i32,
        #[altium(name = "LOCATIONCOUNT")] pub location_count: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
        #[altium(name = "TRANSPARENT")] pub transparent: bool,
        #[altium(name = "AUTOWIRE")] pub auto_wire: bool,
        #[altium(name = "UNDERLINECOLOR")] pub underline_color: i32,
        // Wire endpoint shortcuts (start + end). Codec iterates raw
        // `X{i}/Y{i}` for any additional vertices.
        #[altium(name = "X1")] pub x1: i32,
        #[altium(name = "X1_FRAC")] pub x1_frac: i32,
        #[altium(name = "Y1")] pub y1: i32,
        #[altium(name = "Y1_FRAC")] pub y1_frac: i32,
        #[altium(name = "X2")] pub x2: i32,
        #[altium(name = "X2_FRAC")] pub x2_frac: i32,
        #[altium(name = "Y2")] pub y2: i32,
        #[altium(name = "Y2_FRAC")] pub y2_frac: i32,
    }
}

sch_record! {
    /// Text frame (record 28).
    "28" => TextFrameDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "CORNER.X")] pub corner_x: i32,
        #[altium(name = "CORNER.X_FRAC")] pub corner_x_frac: i32,
        #[altium(name = "CORNER.Y")] pub corner_y: i32,
        #[altium(name = "CORNER.Y_FRAC")] pub corner_y_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "TEXTCOLOR")] pub text_color: i32,
        #[altium(name = "TEXTMARGIN")] pub text_margin: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "LINESTYLE")] pub line_style: i32,
        #[altium(name = "TRANSPARENT")] pub transparent: bool,
        #[altium(name = "TEXT")] pub text: Option<String>,
        #[altium(name = "FONTID")] pub font_id: i32,
        #[altium(name = "ORIENTATION")] pub orientation: i32,
        #[altium(name = "ALIGNMENT")] pub alignment: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
        #[altium(name = "SHOWBORDER")] pub show_border: bool,
        #[altium(name = "WORDWRAP")] pub word_wrap: bool,
        #[altium(name = "CLIPTORECT")] pub clip_to_rect: bool,
    }
}

sch_record! {
    /// Junction (record 29).
    "29" => JunctionDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "LOCKED")] pub locked: bool,
        #[altium(name = "SIZE")] pub size: i32,
    }
}

sch_record! {
    /// Image (record 30).
    "30" => ImageDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "CORNER.X")] pub corner_x: i32,
        #[altium(name = "CORNER.X_FRAC")] pub corner_x_frac: i32,
        #[altium(name = "CORNER.Y")] pub corner_y: i32,
        #[altium(name = "CORNER.Y_FRAC")] pub corner_y_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "KEEPASPECT")] pub keep_aspect: bool,
        #[altium(name = "EMBEDIMAGE")] pub embed_image: bool,
        #[altium(name = "FILENAME")] pub file_name: Option<String>,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
        #[altium(name = "LINESTYLE")] pub line_style: i32,
        #[altium(name = "TRANSPARENT")] pub transparent: bool,
        #[altium(name = "SHOWBORDER")] pub show_border: bool,
    }
}

sch_record! {
    /// Bus entry (record 37).
    "37" => BusEntryDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "CORNER.X")] pub corner_x: i32,
        #[altium(name = "CORNER.X_FRAC")] pub corner_x_frac: i32,
        #[altium(name = "CORNER.Y")] pub corner_y: i32,
        #[altium(name = "CORNER.Y_FRAC")] pub corner_y_frac: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "COLOR")] pub color: i32,
    }
}

sch_record! {
    /// Parameter (record 41).
    "41" => ParameterDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "NAME")] pub name: Option<String>,
        #[altium(name = "TEXT")] pub text: Option<String>,
        #[altium(name = "DESCRIPTION")] pub description: Option<String>,
        #[altium(name = "PARAMTYPE")] pub param_type: i32,
        #[altium(name = "FONTID")] pub font_id: i32,
        #[altium(name = "ORIENTATION")] pub orientation: i32,
        #[altium(name = "JUSTIFICATION")] pub justification: i32,
        #[altium(name = "SHOWNAME")] pub show_name: bool,
        #[altium(name = "ISMIRRORED")] pub is_mirrored: bool,
        #[altium(name = "ISHIDDEN")] pub is_hidden: bool,
        #[altium(name = "READONLYSTATE")] pub read_only_state: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "AUTOPOSITION")] pub auto_position: i32,
        #[altium(name = "ISCONFIGURABLE")] pub is_configurable: bool,
        #[altium(name = "ISRULE")] pub is_rule: bool,
        #[altium(name = "ISSYSTEMPARAMETER")] pub is_system_parameter: bool,
        #[altium(name = "TEXTHORZANCHOR")] pub text_horz_anchor: i32,
        #[altium(name = "TEXTVERTANCHOR")] pub text_vert_anchor: i32,
        #[altium(name = "HIDENAME")] pub hide_name: bool,
        #[altium(name = "ALLOWDATABASESYNCHRONIZE")] pub allow_database_synchronize: bool,
        #[altium(name = "ALLOWLIBRARYSYNCHRONIZE")] pub allow_library_synchronize: bool,
        #[altium(name = "NAMEISREADONLY")] pub name_is_read_only: bool,
        #[altium(name = "PHYSICALDESIGNATOR")] pub physical_designator: Option<String>,
        #[altium(name = "VALUEISREADONLY")] pub value_is_read_only: bool,
        #[altium(name = "VARIANTOPTION")] pub variant_option: Option<String>,
    }
}

sch_record! {
    /// Parameter set (record 43).
    "43" => ParameterSetDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "ORIENTATION")] pub orientation: i32,
        #[altium(name = "STYLE")] pub style: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "NAME")] pub name: Option<String>,
        #[altium(name = "SHOWHIDDENFIELDS")] pub show_hidden_fields: bool,
        #[altium(name = "BORDERWIDTH")] pub border_width: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
    }
}

sch_record! {
    /// Blanket (record 225).
    "225" => BlanketDto {
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "LINESTYLE")] pub line_style: i32,
        #[altium(name = "ISSOLID")] pub is_solid: bool,
        #[altium(name = "TRANSPARENT")] pub transparent: bool,
        #[altium(name = "ISCOLLAPSED")] pub is_collapsed: bool,
        #[altium(name = "LOCATIONCOUNT")] pub location_count: i32,
        // Vertex shortcuts; codec iterates raw `X{i}/Y{i}` for additional vertices.
        #[altium(name = "X1")] pub x1: i32,
        #[altium(name = "X1_FRAC")] pub x1_frac: i32,
        #[altium(name = "Y1")] pub y1: i32,
        #[altium(name = "Y1_FRAC")] pub y1_frac: i32,
        #[altium(name = "X2")] pub x2: i32,
        #[altium(name = "X2_FRAC")] pub x2_frac: i32,
        #[altium(name = "Y2")] pub y2: i32,
        #[altium(name = "Y2_FRAC")] pub y2_frac: i32,
        #[altium(name = "X3")] pub x3: i32,
        #[altium(name = "X3_FRAC")] pub x3_frac: i32,
        #[altium(name = "Y3")] pub y3: i32,
        #[altium(name = "Y3_FRAC")] pub y3_frac: i32,
        #[altium(name = "X4")] pub x4: i32,
        #[altium(name = "X4_FRAC")] pub x4_frac: i32,
        #[altium(name = "Y4")] pub y4: i32,
        #[altium(name = "Y4_FRAC")] pub y4_frac: i32,
    }
}

sch_record! {
    /// Harness connector (record 215): the box that fans a signal harness
    /// out into its member nets. `LOCATION` is the top-left corner.
    "215" => HarnessConnectorDto {
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "XSIZE")] pub x_size: i32,
        #[altium(name = "YSIZE")] pub y_size: i32,
        /// Distance (DXP units) of the primary connection point from the
        /// top of the side named by `HARNESSCONNECTORSIDE`.
        #[altium(name = "PRIMARYCONNECTIONPOSITION")] pub primary_connection_position: i32,
        #[altium(name = "HARNESSCONNECTORSIDE")] pub harness_connector_side: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
    }
}

sch_record! {
    /// Harness entry (record 216): one named signal on a harness connector.
    /// `DISTANCEFROMTOP` counts 100-mil grid slots, like a sheet entry.
    "216" => HarnessEntryDto {
        #[altium(name = "OWNERINDEXADDITIONALLIST")] pub owner_index_additional_list: bool,
        #[altium(name = "SIDE")] pub side: i32,
        #[altium(name = "DISTANCEFROMTOP")] pub distance_from_top: i32,
        #[altium(name = "DISTANCEFROMTOP_FRAC")] pub distance_from_top_frac: i32,
        #[altium(name = "NAME")] pub name: Option<String>,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "AREACOLOR")] pub area_color: i32,
        #[altium(name = "TEXTCOLOR")] pub text_color: i32,
        #[altium(name = "TEXTFONTID")] pub text_font_id: i32,
        #[altium(name = "TEXTSTYLE")] pub text_style: Option<String>,
    }
}

sch_record! {
    /// Harness type label (record 217): the harness-type text attached to a
    /// harness connector.
    "217" => HarnessTypeDto {
        #[altium(name = "OWNERINDEXADDITIONALLIST")] pub owner_index_additional_list: bool,
        #[altium(name = "LOCATION.X")] pub location_x: i32,
        #[altium(name = "LOCATION.X_FRAC")] pub location_x_frac: i32,
        #[altium(name = "LOCATION.Y")] pub location_y: i32,
        #[altium(name = "LOCATION.Y_FRAC")] pub location_y_frac: i32,
        #[altium(name = "TEXT")] pub text: Option<String>,
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "FONTID")] pub font_id: i32,
        #[altium(name = "ISHIDDEN")] pub is_hidden: bool,
        #[altium(name = "NOTAUTOPOSITION")] pub not_auto_position: bool,
    }
}

sch_record! {
    /// Signal harness (record 218): a polyline carrying a bundle of nets
    /// between ports, sheet entries and harness connectors.
    "218" => SignalHarnessDto {
        #[altium(name = "COLOR")] pub color: i32,
        #[altium(name = "LINEWIDTH")] pub line_width: i32,
        #[altium(name = "LOCATIONCOUNT")] pub location_count: i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parameter::ParameterMap;

    #[test]
    fn pin_round_trip() {
        let pin = PinDto {
            owner_index: 5,
            location_x: 100,
            location_y: -50,
            name: Some("VCC".into()),
            designator: Some("1".into()),
            pin_length: 30,
            pin_conglomerate: 0x18,
            electrical: 7,
            unique_id: Some("ABC".into()),
            ..PinDto::default()
        };
        let mut params = ParameterMap::new();
        pin.to_params(&mut params);
        let parsed = PinDto::from_params(&params);
        assert_eq!(parsed, pin);
    }

    #[test]
    fn arc_record_constant() {
        assert_eq!(ArcDto::RECORD, "12");
    }

    #[test]
    fn label_round_trip_with_strings() {
        let label = LabelDto {
            location_x: 1,
            location_y: 2,
            text: Some("hi".into()),
            font_id: 3,
            justification: 4,
            ..LabelDto::default()
        };
        let mut params = ParameterMap::new();
        label.to_params(&mut params);
        let parsed = LabelDto::from_params(&params);
        assert_eq!(parsed, label);
    }
}
