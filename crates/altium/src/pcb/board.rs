//! Board-level data carried in the `Board6` parameter dictionary: the board
//! outline and the 2D and 3D view configurations.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::polygon::{PolygonVertex, VertexKind};
use crate::color::Color;
use crate::coord::{Coord, CoordPoint};
use crate::error::{Error, Result};

/// The first value of each `Board6` key, keys upper-cased.
fn first_values(parameters: &[(String, String)]) -> BTreeMap<String, &str> {
    let mut out = BTreeMap::new();
    for (key, value) in parameters {
        out.entry(key.to_ascii_uppercase())
            .or_insert(value.as_str());
    }
    out
}

/// The board outline from the `Board6` vertex keys, in order: `KIND{i}`
/// (0 a straight run to the next vertex, 1 an arc), `VX{i}`/`VY{i}` (the
/// vertex), and for an arc `CX{i}`/`CY{i}` (its centre), `SA{i}`/`EA{i}`
/// (its start and end angles in degrees) and `R{i}` (its radius). The last
/// vertex runs back to the first; a closing vertex repeating the first is
/// dropped. `Ok(None)` where the board states no outline.
///
/// # Errors
///
/// [`Error::Corrupt`] (stream `Board6`) where a vertex's key is missing or
/// does not parse: an outline is never read with a stand-in value.
pub fn board_outline(parameters: &[(String, String)]) -> Result<Option<Vec<PolygonVertex>>> {
    let values = first_values(parameters);
    let bad = |key: String, what: &str| {
        Error::corrupt_in(format!("board outline {key}: {what}"), "Board6")
    };
    let coord = |key: String| -> Result<Coord> {
        let text = values.get(&key).ok_or_else(|| bad(key.clone(), "absent"))?;
        Coord::parse_altium(text).map_err(|_| bad(key.clone(), "not a length"))
    };
    let number = |key: String| -> Result<f64> {
        let text = values.get(&key).ok_or_else(|| bad(key.clone(), "absent"))?;
        text.trim()
            .parse::<f64>()
            .map_err(|_| bad(key.clone(), "not a number"))
    };
    let mut vertices = Vec::new();
    for i in 0.. {
        if !values.contains_key(&format!("VX{i}")) {
            break;
        }
        let point = CoordPoint::new(coord(format!("VX{i}"))?, coord(format!("VY{i}"))?);
        let kind = number(format!("KIND{i}"))?;
        let vertex = if kind == 0.0 {
            PolygonVertex::linear(point)
        } else if kind == 1.0 {
            PolygonVertex {
                point,
                kind: VertexKind::Arc,
                arc_center: CoordPoint::new(coord(format!("CX{i}"))?, coord(format!("CY{i}"))?),
                start_angle: number(format!("SA{i}"))?,
                end_angle: number(format!("EA{i}"))?,
                radius: coord(format!("R{i}"))?,
            }
        } else {
            return Err(bad(format!("KIND{i}"), "neither 0 (line) nor 1 (arc)"));
        };
        vertices.push(vertex);
    }
    if vertices.len() > 1 && vertices.first().map(|v| v.point) == vertices.last().map(|v| v.point) {
        vertices.pop();
    }
    Ok((!vertices.is_empty()).then_some(vertices))
}

/// An opacity as the file writes it (`0.820000`), in thousandths: exact to
/// three digits, rounded half away from zero past them. An integer, so a
/// colour compares and hashes exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Opacity(pub i32);

impl Opacity {
    /// Fully opaque.
    pub const OPAQUE: Self = Self(1000);

    /// The opacity a decimal text states; `None` for text that is not a
    /// decimal number.
    pub fn parse(text: &str) -> Option<Self> {
        super::layer::Permittivity::parse(text).map(|value| Self(value.0))
    }

    /// Its value, 0 to 1.
    pub fn to_f64(self) -> f64 {
        f64::from(self.0) / 1000.0
    }
}

/// A colour with the opacity a 3D view draws it at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ViewColor {
    /// The colour (from Altium's BGR-packed integer).
    pub color: Color,
    /// Its opacity ([`Opacity::OPAQUE`] where the configuration states
    /// none).
    pub opacity: Opacity,
}

/// The board's 3D view configuration (`Board6` `3DCONFIGURATION`, its
/// `CFG3D.*` entries): the colours and opacities the 3D view draws the
/// board's materials with, and what it shows. A colour the configuration
/// does not state is `None`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ViewConfig3d {
    /// Whether the 3D view uses the system colours instead of these
    /// (`CFG3D.USESYSCOLORSFOR3D`).
    pub use_system_colors: Option<bool>,
    /// The board core (`CFG3D.BOARDCORECOLOR`, `...OPACITY`).
    pub board_core: Option<ViewColor>,
    /// The prepreg (`CFG3D.BOARDPREPREGCOLOR`, `...OPACITY`).
    pub board_prepreg: Option<ViewColor>,
    /// The top solder mask (`CFG3D.TOPSOLDERMASKCOLOR`, `...OPACITY`).
    pub top_solder_mask: Option<ViewColor>,
    /// The bottom solder mask (`CFG3D.BOTSOLDERMASKCOLOR`, `...OPACITY`).
    pub bottom_solder_mask: Option<ViewColor>,
    /// Exposed copper (`CFG3D.COPPERCOLOR`, `...OPACITY`).
    pub copper: Option<ViewColor>,
    /// The top silkscreen (`CFG3D.TOPSILKSCREENCOLOR`, `...OPACITY`).
    pub top_silkscreen: Option<ViewColor>,
    /// The bottom silkscreen (`CFG3D.BOTSILKSCREENCOLOR`, `...OPACITY`).
    pub bottom_silkscreen: Option<ViewColor>,
    /// Whether the top silkscreen is shown (`CFG3D.SHOWTOPSILKSCREEN`).
    pub show_top_silkscreen: Option<bool>,
    /// Whether the bottom silkscreen is shown (`CFG3D.SHOWBOTSILKSCREEN`).
    pub show_bottom_silkscreen: Option<bool>,
    /// Whether the board core is shown (`CFG3D.SHOWBOARDCORE`).
    pub show_board_core: Option<bool>,
    /// Every `CFG3D.*` entry as written, by key.
    pub entries: BTreeMap<String, String>,
}

/// The board's 2D view configuration (`Board6` `2DCONFIGURATION`, its
/// `CFG2D.*` entries): which layers the 2D view shows.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ViewConfig2d {
    /// Each layer's shown state by its V9 layer id, from the layer hashes of
    /// `CFG2D.TOGGLELAYERS.SET` (`<id>=1` shown, `<id>=0` hidden). A layer
    /// the set does not name is not stated.
    pub layers_shown: BTreeMap<i64, bool>,
    /// Every `CFG2D.*` entry as written, by key.
    pub entries: BTreeMap<String, String>,
}

/// The `KEY=VALUE` entries of a view configuration value, which Altium
/// separates with backquotes.
fn config_entries(value: &str, prefix: &str) -> BTreeMap<String, String> {
    value
        .split('`')
        .filter_map(|item| item.split_once('='))
        .filter(|(key, _)| key.starts_with(prefix))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// The 3D view configuration, where the board states one.
pub fn view_config_3d(parameters: &[(String, String)]) -> Option<ViewConfig3d> {
    let values = first_values(parameters);
    let entries = config_entries(values.get("3DCONFIGURATION")?, "CFG3D.");
    let flag = |key: &str| entries.get(key).map(|v| v.eq_ignore_ascii_case("TRUE"));
    let color = |key: &str| {
        let bgr = entries.get(key)?.trim().parse::<i32>().ok()?;
        let opacity = entries
            .get(&format!("{key}OPACITY"))
            .and_then(|v| Opacity::parse(v))
            .unwrap_or(Opacity::OPAQUE);
        Some(ViewColor {
            color: Color::from_altium_bgr(bgr),
            opacity,
        })
    };
    Some(ViewConfig3d {
        use_system_colors: flag("CFG3D.USESYSCOLORSFOR3D"),
        board_core: color("CFG3D.BOARDCORECOLOR"),
        board_prepreg: color("CFG3D.BOARDPREPREGCOLOR"),
        top_solder_mask: color("CFG3D.TOPSOLDERMASKCOLOR"),
        bottom_solder_mask: color("CFG3D.BOTSOLDERMASKCOLOR"),
        copper: color("CFG3D.COPPERCOLOR"),
        top_silkscreen: color("CFG3D.TOPSILKSCREENCOLOR"),
        bottom_silkscreen: color("CFG3D.BOTSILKSCREENCOLOR"),
        show_top_silkscreen: flag("CFG3D.SHOWTOPSILKSCREEN"),
        show_bottom_silkscreen: flag("CFG3D.SHOWBOTSILKSCREEN"),
        show_board_core: flag("CFG3D.SHOWBOARDCORE"),
        entries,
    })
}

/// The 2D view configuration, where the board states one.
pub fn view_config_2d(parameters: &[(String, String)]) -> Option<ViewConfig2d> {
    let values = first_values(parameters);
    let entries = config_entries(values.get("2DCONFIGURATION")?, "CFG2D.");
    let mut layers_shown = BTreeMap::new();
    if let Some(set) = entries.get("CFG2D.TOGGLELAYERS.SET") {
        // `<group>~<n>_<group>.Include~SerializeLayerHash.Version=2,
        // ClassName=TLayerHash,<id>=<0|1>,...`: every `<id>=<0|1>` pair of
        // every group's hash.
        for pair in set.split([',', '_']) {
            if let Some((id, shown)) = pair.split_once('=') {
                if let (Ok(id), true) = (id.trim().parse::<i64>(), shown == "0" || shown == "1") {
                    layers_shown.insert(id, shown == "1");
                }
            }
        }
    }
    Some(ViewConfig2d {
        layers_shown,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn outline_reads_lines_and_arcs() {
        let p = params(&[
            ("KIND0", "0"),
            ("VX0", "0mil"),
            ("VY0", "0mil"),
            ("KIND1", "1"),
            ("VX1", "100mil"),
            ("VY1", "0mil"),
            ("CX1", "100mil"),
            ("CY1", "10mil"),
            ("SA1", " 2.70000000000000E+0002"),
            ("EA1", " 3.60000000000000E+0002"),
            ("R1", "10mil"),
            ("KIND2", "0"),
            ("VX2", "110mil"),
            ("VY2", "100mil"),
        ]);
        let outline = board_outline(&p).unwrap().unwrap();
        assert_eq!(outline.len(), 3);
        assert_eq!(outline[1].kind, VertexKind::Arc);
        assert_eq!(
            outline[1].arc_center,
            CoordPoint::new(Coord::from_mils(100.0), Coord::from_mils(10.0))
        );
        assert_eq!(outline[1].start_angle, 270.0);
        assert_eq!(outline[1].radius, Coord::from_mils(10.0));
    }

    #[test]
    fn outline_refuses_a_missing_arc_centre() {
        let p = params(&[("KIND0", "1"), ("VX0", "0mil"), ("VY0", "0mil")]);
        assert!(board_outline(&p).is_err());
        assert!(board_outline(&params(&[])).unwrap().is_none());
    }

    #[test]
    fn view_configs_read_colours_and_layer_toggles() {
        let p = params(&[
            (
                "3DCONFIGURATION",
                "CFG3D.USESYSCOLORSFOR3D=FALSE`CFG3D.TOPSOLDERMASKCOLOR=0`CFG3D.TOPSOLDERMASKCOLOROPACITY=1.000000`CFG3D.BOARDCORECOLOR=13491161`CFG3D.BOARDCORECOLOROPACITY=0.820000`CFG3D.SHOWTOPSILKSCREEN=TRUE",
            ),
            (
                "2DCONFIGURATION",
                "CFG2D.TOGGLELAYERS=111`CFG2D.TOGGLELAYERS.SET=Signal.All~0_Signal.Include~SerializeLayerHash.Version=2,ClassName=TLayerHash,16777217=1,16842751=0_Mechanical.All~0",
            ),
        ]);
        let three = view_config_3d(&p).unwrap();
        assert_eq!(three.use_system_colors, Some(false));
        assert_eq!(three.top_solder_mask.unwrap().color, Color::BLACK);
        let core = three.board_core.unwrap();
        assert_eq!(core.color, Color::rgb(0xD9, 0xDB, 0xCD));
        assert_eq!(core.opacity, Opacity(820));
        assert_eq!(three.show_top_silkscreen, Some(true));
        assert!(three.bottom_solder_mask.is_none());
        let two = view_config_2d(&p).unwrap();
        assert_eq!(two.layers_shown.get(&16777217), Some(&true));
        assert_eq!(two.layers_shown.get(&16842751), Some(&false));
        assert_eq!(two.layers_shown.get(&2), None);
    }
}
