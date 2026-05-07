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

    /// Raw `Board6` parameters preserved verbatim. `None` means no `Board6`
    /// storage was present (which is allowed for trivial documents).
    pub board_parameters: Option<BTreeMap<String, String>>,

    /// Additional storages/streams we don't model. Keys use
    /// `"StorageName/StreamName"` (or just `"StreamName"` for root streams).
    pub additional_streams: BTreeMap<String, Vec<u8>>,
}

impl Document {
    /// Layer stack derived from `board_parameters`. Returns `None` if absent.
    pub fn layer_stack(&self) -> Option<LayerStack> {
        let params = self.board_parameters.as_ref()?;
        LayerStack::from_board_parameters(params)
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
