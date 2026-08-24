//! Top-level PCB document (`.PcbDoc`).

use std::collections::BTreeMap;
use std::path::Path;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::component::Component;
use super::embedded::{BoardLoader, EmbeddedBoard, FileBoardLoader};
use super::layer::LayerStack;
use super::polygon::Polygon;
use super::primitives::{Arc, ComponentBody, Fill, Net, Pad, Region, Text, Track, Via};
use super::rule::{DifferentialPair, ObjectClass, Room, Rule};
use crate::coord::CoordRect;
use crate::diagnostic::Diagnostic;
use crate::error::Result;

/// A pad-shaped-region pair representing one custom-shape EP.
///
/// Returned by [`Document::custom_shape_pads`]. The pad carries the
/// metadata Altium tracks on `Pad6` (designator, net, rotation); the
/// region carries the actual polygon outline.
#[derive(Debug, Clone, Copy)]
pub struct CustomShapePad<'a> {
    pub pad: &'a Pad,
    pub region: &'a Region,
}

/// A PCB document.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Document {
    /// Warnings collected during parsing.
    pub diagnostics: Vec<Diagnostic>,

    pub components: Vec<Component>,
    pub pads: Vec<Pad>,
    pub vias: Vec<Via>,
    pub tracks: Vec<Track>,
    pub arcs: Vec<Arc>,
    pub texts: Vec<Text>,
    pub fills: Vec<Fill>,
    pub regions: Vec<Region>,
    pub component_bodies: Vec<ComponentBody>,
    pub polygons: Vec<Polygon>,
    pub nets: Vec<Net>,
    pub embedded_boards: Vec<EmbeddedBoard>,
    pub rules: Vec<Rule>,
    pub classes: Vec<ObjectClass>,
    pub differential_pairs: Vec<DifferentialPair>,
    pub rooms: Vec<Room>,

    /// Raw `Board6` parameters in on-disk order. Stored as `Vec` not
    /// `Map` because Board6 has repeated `RECORD=Board` section markers
    /// and `\r` separators embedded in values that a map would lose.
    pub board_parameters: Option<Vec<(String, String)>>,

    /// Additional storages/streams we don't model. Keys use
    /// `"StorageName/StreamName"` (or just `"StreamName"` for root streams).
    #[cfg_attr(feature = "serde", serde(default, with = "crate::serde_bytes::b64_map"))]
    pub additional_streams: BTreeMap<String, Vec<u8>>,
}

impl Document {
    /// Layer stack derived from `board_parameters`. Returns `None` if absent.
    pub fn layer_stack(&self) -> Option<LayerStack> {
        let params = self.board_parameters.as_ref()?;
        LayerStack::from_board_parameters(params)
    }

    /// The document's display unit (`DISPLAYUNIT` in `Board6`): `Some(0)`
    /// = metric, `Some(1)` = imperial, `None` when absent. Display-only —
    /// stored coordinates are unit-agnostic raw values, and every
    /// coordinate parameter this library writes carries an explicit `mil`
    /// suffix, so files are correct in either mode. Preserved verbatim on
    /// write via `board_parameters`.
    pub fn display_unit(&self) -> Option<i32> {
        self.board_parameters.as_ref()?.iter().find_map(|(k, v)| {
            k.eq_ignore_ascii_case("DISPLAYUNIT")
                .then(|| v.trim().parse().ok())
                .flatten()
        })
    }

    /// The board origin (`Design » Origin` in Altium; `ORIGINX`/`ORIGINY`
    /// in `Board6`), in absolute workspace coordinates. `(0, 0)` when unset
    /// or unparsable. Every coordinate Altium *displays* is relative to
    /// this point; everything stored in the file is absolute.
    pub fn board_origin(&self) -> crate::coord::CoordPoint {
        let mut origin = crate::coord::CoordPoint::default();
        if let Some(params) = &self.board_parameters {
            for (k, v) in params {
                if k.eq_ignore_ascii_case("ORIGINX") {
                    if let Ok(c) = crate::coord::Coord::parse_altium(v) {
                        origin.x = c;
                    }
                } else if k.eq_ignore_ascii_case("ORIGINY") {
                    if let Ok(c) = crate::coord::Coord::parse_altium(v) {
                        origin.y = c;
                    }
                }
            }
        }
        origin
    }

    /// Embedded 3D models from the document's root `Models` storage
    /// (`Models/Data` metadata + zlib-compressed `Models/<i>` streams),
    /// with the STEP text decompressed. Empty when the document embeds no
    /// models.
    pub fn embedded_models(&self) -> Result<Vec<super::Model3d>> {
        let metas = match self.additional_streams.get("Models/Data") {
            Some(data) => super::model3d::parse_models_data(data),
            None => Vec::new(),
        };
        super::model3d::build_models(&metas, |i| {
            self.additional_streams.get(&format!("Models/{i}")).cloned()
        })
    }

    /// Bounding box of every primitive directly referenced by the document.
    pub fn bounds(&self) -> CoordRect {
        let mut acc = CoordRect::EMPTY;
        for p in &self.pads {
            acc = acc.union(p.bounds());
        }
        for v in &self.vias {
            acc = acc.union(v.bounds());
        }
        for t in &self.tracks {
            acc = acc.union(t.bounds());
        }
        for a in &self.arcs {
            acc = acc.union(a.bounds());
        }
        for x in &self.texts {
            acc = acc.union(x.bounds());
        }
        for f in &self.fills {
            acc = acc.union(f.bounds());
        }
        for r in &self.regions {
            acc = acc.union(r.bounds());
        }
        acc
    }

    /// Pair custom-shape pads (placeholder `Pad6` records) with the
    /// `Region6` outlines that carry their actual geometry.
    ///
    /// Altium emits a custom-shape EP as two records: a `Pad6` that
    /// carries designator / net / rotation, and a `Region6` on the same
    /// copper layer with `ISSHAPEBASED=TRUE` and `SUBPOLYINDEX=-1` that
    /// carries the polygon outline. Pairing matches a shape-based
    /// region to a pad by (1) matching `component_index`, (2) matching
    /// `layer`, and (3) the pad's location falling inside the region's
    /// bounding box.
    ///
    /// The result includes regions parented to components (libraries
    /// keep regions inside `Component`, documents keep them at the top
    /// level — both are searched).
    ///
    /// Unmatched shape-based regions are dropped silently; the caller
    /// usually wants only the (pad, region) pairs for downstream
    /// rendering or paste-mask derivation.
    pub fn custom_shape_pads(&self) -> Vec<CustomShapePad<'_>> {
        let mut out: Vec<CustomShapePad<'_>> = Vec::new();
        let candidate_pads: Vec<&Pad> = self
            .pads
            .iter()
            .chain(self.components.iter().flat_map(|c| c.pads.iter()))
            .collect();
        let candidate_regions = self
            .regions
            .iter()
            .chain(self.components.iter().flat_map(|c| c.regions.iter()));

        for region in candidate_regions {
            if !region.is_shape_based || region.sub_poly_index != -1 {
                continue;
            }
            let bbox = region.bounds();
            if bbox.is_empty() {
                continue;
            }
            let matched = candidate_pads.iter().find(|pad| {
                pad.component_index == region.component_index
                    && pad.layer == region.layer
                    && bbox.contains(pad.location)
            });
            if let Some(pad) = matched {
                // Cheap de-dup: the same Pad shows up in both `doc.pads`
                // and `component.pads` (the reader pushes a clone), so
                // skip if we've already paired this pad pointer.
                let pad_ptr = (*pad) as *const Pad;
                if out
                    .iter()
                    .any(|existing| std::ptr::eq(existing.pad, pad_ptr))
                {
                    continue;
                }
                out.push(CustomShapePad { pad, region });
            }
        }
        out
    }

    /// Resolve every entry in [`embedded_boards`](Self::embedded_boards) by
    /// reading and parsing the referenced sub-document from disk.
    ///
    /// `parent_dir` is normally the directory the parent `.PcbDoc` was loaded
    /// from. The returned vector parallels `self.embedded_boards`: each entry
    /// is `Ok(sub_document)` if the reference resolved and parsed, `Err(...)`
    /// otherwise — failures don't stop the others.
    ///
    /// ```no_run
    /// # async fn run() -> altium::Result<()> {
    /// use altium::pcb;
    /// let doc = pcb::Document::read("Power Adapter Panel.PcbDoc").await?;
    /// for (i, result) in doc
    ///     .resolve_embedded_boards_at("/path/to/parent_dir")
    ///     .await
    ///     .into_iter()
    ///     .enumerate()
    /// {
    ///     match result {
    ///         Ok(sub) => println!("board {i}: {} components", sub.components.len()),
    ///         Err(e)  => eprintln!("board {i} failed: {e}"),
    ///     }
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn resolve_embedded_boards_at(
        &self,
        parent_dir: impl AsRef<Path>,
    ) -> Vec<Result<Document>> {
        let loader = FileBoardLoader::new(parent_dir.as_ref().to_path_buf());
        self.resolve_embedded_boards_with(&loader)
    }

    /// Resolve every embedded board via a custom [`BoardLoader`]. Same shape
    /// as [`resolve_embedded_boards_at`](Self::resolve_embedded_boards_at)
    /// but lets the caller plug in caching / search-path / in-memory
    /// strategies.
    pub fn resolve_embedded_boards_with(
        &self,
        loader: &dyn BoardLoader,
    ) -> Vec<Result<Document>> {
        self.embedded_boards
            .iter()
            .map(|board| board.resolve_with(loader))
            .collect()
    }

    /// Recursively dereference every embedded sub-board and merge its
    /// primitives into a new self-contained [`Document`].
    ///
    /// For each [`EmbeddedBoard`] entry:
    /// - Resolve through `loader` (e.g. [`FileBoardLoader`] for filesystem
    ///   lookup).
    /// - For each instance in the `col_count` × `row_count` array, apply the
    ///   per-instance origin (`X1+col*COLSPACING`, `Y1+row*ROWSPACING`) plus
    ///   the embedded `rotation` and `mirror_flag` to every primitive of the
    ///   sub-document, then append.
    /// - Recurse into the sub-document's own embedded boards, up to a fixed
    ///   depth ceiling that protects against cycles.
    /// - Sub-document `nets` are deduped by name; `rules`, `classes`,
    ///   `differential_pairs`, `rooms` are appended.
    /// - Components keep their identity (designators, refs) — only their
    ///   `(x, y, rotation)` is transformed.
    ///
    /// References that fail to resolve (loader returns `None`, parse error,
    /// recursion cap hit) are preserved on the result with their bounding
    /// box transformed into the parent's frame, and an entry is added to
    /// `result.diagnostics`.
    ///
    /// ```no_run
    /// # async fn run() -> altium::Result<()> {
    /// use altium::pcb::{self, FileBoardLoader};
    /// let parent = pcb::Document::read("Power Adapter Panel.PcbDoc").await?;
    /// let loader = FileBoardLoader::new("/path/to/parent_dir");
    /// let flat = parent.flatten_embedded_boards_with(&loader);
    /// println!("{} pads after flatten", flat.pads.len());
    /// # Ok(()) }
    /// ```
    pub fn flatten_embedded_boards_with(&self, loader: &dyn BoardLoader) -> Document {
        let mut out = Document {
            // Carry over the metadata that doesn't transform (and that we
            // don't merge into per-primitive walk).
            board_parameters: self.board_parameters.clone(),
            additional_streams: self.additional_streams.clone(),
            ..Default::default()
        };
        let mut diagnostics = Vec::new();

        super::flatten::flatten_into(
            self,
            &super::flatten::WorldTransform::IDENTITY,
            loader,
            0,
            &mut out,
            &mut diagnostics,
        );

        out.diagnostics = diagnostics;
        out
    }

    /// Filesystem-backed wrapper around
    /// [`flatten_embedded_boards_with`](Self::flatten_embedded_boards_with).
    pub async fn flatten_embedded_boards_at(
        &self,
        parent_dir: impl AsRef<Path>,
    ) -> Document {
        let loader = super::embedded::FileBoardLoader::new(parent_dir.as_ref().to_path_buf());
        self.flatten_embedded_boards_with(&loader)
    }
}
