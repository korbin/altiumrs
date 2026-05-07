//! Embedded board placeholder primitive (a sub-board referenced from the
//! parent design), plus the [`BoardLoader`] strategy used to resolve a
//! reference back to its sub-document on disk.

use std::path::{Path, PathBuf};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::document::Document;
use crate::coord::Coord;
use crate::error::{Error, Result};

/// A reference to another PCB document embedded into this one.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EmbeddedBoard {
    pub document_path: Option<String>,
    pub layer: i32,
    pub rotation: f64,
    pub mirror_flag: bool,
    pub origin_mode: i32,
    pub scale: f64,
    pub col_count: i32,
    pub col_spacing: Coord,
    pub row_count: i32,
    pub row_spacing: Coord,
    pub unique_id: Option<String>,
    pub enabled: bool,
    pub is_keepout: bool,
    pub is_electrical_prim: bool,
    pub is_pre_route: bool,
    pub tear_drop: bool,
    pub polygon_outline: bool,
    pub user_routed: bool,
    pub union_index: i32,
    pub is_tenting: bool,
    pub is_tenting_top: bool,
    pub is_tenting_bottom: bool,
    pub is_testpoint_top: bool,
    pub is_testpoint_bottom: bool,
    pub is_assy_testpoint_top: bool,
    pub is_assy_testpoint_bottom: bool,
    pub power_plane_clearance: Coord,
    pub power_plane_connect_style: i32,
    pub power_plane_relief_expansion: Coord,
    pub relief_air_gap: Coord,
    pub relief_conductor_width: Coord,
    pub relief_entries: i32,
    pub solder_mask_expansion: Coord,
    pub is_viewport: bool,
    pub viewport_title: Option<String>,
    pub viewport_visible: bool,
    pub title_font_color: i32,
    pub title_font_name: Option<String>,
    pub title_font_size: i32,
    pub title_object: i32,
    pub transmit_board_shape: bool,
    pub transmit_dimensions: bool,
    pub transmit_drill_table: bool,
    pub transmit_layers_enabled_top: bool,
    pub transmit_layer_stack_table: bool,
    pub transmit_parameters_count: i32,
    pub allow_global_edit: bool,
    pub moveable: bool,
    pub paste_mask_expansion: Coord,
    pub is_hidden: bool,
    pub x1_location: Coord,
    pub y1_location: Coord,
    pub x2_location: Coord,
    pub y2_location: Coord,
}

// ─── Sub-document resolution ────────────────────────────────────────────────
//
// The `document_path` field stores a relative reference to another `.PcbDoc`
// file (e.g. `"USB Power Adapter.PcbDoc"` for a board sitting next to the
// parent). Altium itself stores nothing else — the sub-board is loaded from
// disk at open time. The methods below give callers the same capability:
// turn a relative reference into a fully-parsed [`pcb::Document`](Document).

/// Strategy for finding the bytes of an embedded sub-board given the
/// `DOCUMENTPATH` reference Altium recorded.
///
/// Implementors plug in caching, alternate search paths, or in-memory loaders
/// for tests. The built-in [`FileBoardLoader`] handles the common case of
/// looking next to the parent file plus a list of fallback directories, with
/// case-insensitive filename matching for portability across filesystems.
pub trait BoardLoader {
    /// Look up `document_path` and return its bytes, or `None` if the loader
    /// has nothing matching that reference. Errors only on I/O failure for a
    /// path that *was* matched.
    fn load(&self, document_path: &str) -> Result<Option<Vec<u8>>>;
}

/// Filesystem-backed [`BoardLoader`] that resolves relative paths against
/// `root` plus zero or more fallback directories.
#[derive(Debug, Clone)]
pub struct FileBoardLoader {
    root: PathBuf,
    extra: Vec<PathBuf>,
}

impl FileBoardLoader {
    /// Build a loader whose primary search root is the directory containing
    /// the *parent* `.PcbDoc`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            extra: Vec::new(),
        }
    }

    /// Add a fallback directory. Searched after `root`, in insertion order.
    /// Useful for project-level "library" directories or shared sub-board
    /// repositories.
    pub fn with_search_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.extra.push(path.into());
        self
    }
}

impl BoardLoader for FileBoardLoader {
    fn load(&self, document_path: &str) -> Result<Option<Vec<u8>>> {
        let mut roots: Vec<&Path> = vec![&self.root];
        roots.extend(self.extra.iter().map(|p| p.as_path()));
        for root in roots {
            if let Some(path) = resolve_in_dir(root, document_path) {
                return Ok(Some(std::fs::read(&path)?));
            }
        }
        Ok(None)
    }
}

impl EmbeddedBoard {
    /// Compute where the referenced sub-board *would* live on disk relative
    /// to `parent_dir` (typically the directory containing the parent
    /// `.PcbDoc`). Pure path-arithmetic — no I/O, no existence check.
    ///
    /// Translates Windows backslashes so files written on Windows resolve on
    /// Unix too. Returns `None` when `document_path` is empty or unset.
    pub fn resolved_path(&self, parent_dir: impl AsRef<Path>) -> Option<PathBuf> {
        let raw = self.document_path.as_deref().filter(|s| !s.is_empty())?;
        let normalized = raw.replace('\\', "/");
        let candidate = Path::new(&normalized);
        if candidate.is_absolute() {
            Some(candidate.to_path_buf())
        } else {
            Some(parent_dir.as_ref().join(candidate))
        }
    }

    /// Read and parse the referenced sub-board from disk.
    ///
    /// `parent_dir` is the directory the parent `.PcbDoc` came from — the
    /// stored `DOCUMENTPATH` is resolved relative to it. Falls back to a
    /// case-insensitive directory scan when the literal path doesn't exist
    /// (Altium files authored on Windows may carry casing that differs from
    /// the file as it ended up on a Unix filesystem).
    pub async fn resolve_at(&self, parent_dir: impl AsRef<Path>) -> Result<Document> {
        let loader = FileBoardLoader::new(parent_dir.as_ref().to_path_buf());
        self.resolve_with(&loader)
    }

    /// Resolve via any [`BoardLoader`] — in-memory caches, custom search
    /// strategies, etc.
    pub fn resolve_with(&self, loader: &dyn BoardLoader) -> Result<Document> {
        let path = self
            .document_path
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                Error::corrupt("embedded board has no DOCUMENTPATH to resolve")
            })?;
        let bytes = loader.load(path)?.ok_or_else(|| {
            Error::corrupt(format!("embedded sub-board not found: {path}"))
        })?;
        self.resolve_from_bytes(bytes)
    }

    /// Parse already-loaded sub-board bytes into a [`Document`]. Tagged to
    /// the embedded board only nominally — there's no validation that the
    /// document and the embedded reference agree.
    pub fn resolve_from_bytes(&self, bytes: Vec<u8>) -> Result<Document> {
        Document::from_bytes(bytes)
    }
}

/// Find a file in `dir` matching `document_path`. Tries the literal path
/// first, then a case-insensitive scan of the directory entries. Returns the
/// resolved path if it exists, `None` otherwise.
fn resolve_in_dir(dir: &Path, document_path: &str) -> Option<PathBuf> {
    let normalized = document_path.replace('\\', "/");
    let rel = Path::new(&normalized);

    // Absolute paths (e.g. `D:\projects\foo.PcbDoc`) are tried as-is, then we
    // fall back to the basename only inside `dir`. Most absolute paths in
    // real files don't exist outside the original author's machine.
    let primary = if rel.is_absolute() {
        rel.to_path_buf()
    } else {
        dir.join(rel)
    };
    if primary.is_file() {
        return Some(primary);
    }

    // Try the basename inside `dir`.
    let basename = rel.file_name()?;
    let basename_path = dir.join(basename);
    if basename_path.is_file() {
        return Some(basename_path);
    }

    // Case-insensitive scan.
    let target = basename.to_string_lossy();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().eq_ignore_ascii_case(target.as_ref()) {
                return Some(entry.path());
            }
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn resolved_path_joins_relative_with_parent_dir() {
        let mut b = EmbeddedBoard::default();
        b.document_path = Some("Sub.PcbDoc".into());
        let p = b.resolved_path("/tmp/projects").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/projects/Sub.PcbDoc"));
    }

    #[test]
    fn resolved_path_translates_backslashes() {
        let mut b = EmbeddedBoard::default();
        b.document_path = Some("subdir\\Inner.PcbDoc".into());
        let p = b.resolved_path("/tmp/projects").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/projects/subdir/Inner.PcbDoc"));
    }

    #[test]
    fn resolved_path_keeps_absolute() {
        let mut b = EmbeddedBoard::default();
        b.document_path = Some("/abs/path/Sub.PcbDoc".into());
        let p = b.resolved_path("/tmp/projects").unwrap();
        assert_eq!(p, PathBuf::from("/abs/path/Sub.PcbDoc"));
    }

    #[test]
    fn resolved_path_none_when_unset() {
        let b = EmbeddedBoard::default();
        assert!(b.resolved_path("/tmp").is_none());
    }

    #[test]
    fn resolved_path_none_when_empty() {
        let mut b = EmbeddedBoard::default();
        b.document_path = Some(String::new());
        assert!(b.resolved_path("/tmp").is_none());
    }

    #[test]
    fn resolve_in_dir_falls_back_to_case_insensitive() {
        // Real exemplar: Altium files authored on Windows may carry "Sub.PCBDOC"
        // while the on-disk file is "Sub.PcbDoc". The scan handles that.
        let dir = std::env::temp_dir().join(format!(
            "altium-rs-emb-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let canonical = dir.join("MixedCase.PcbDoc");
        std::fs::write(&canonical, b"placeholder").unwrap();

        // Lookup using a different casing should still find the file.
        let resolved = resolve_in_dir(&dir, "MIXEDCASE.PCBDOC");
        assert_eq!(resolved.as_ref(), Some(&canonical));

        let _ = std::fs::remove_file(&canonical);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn resolve_with_loader_errors_when_path_unset() {
        struct NeverCalled;
        impl BoardLoader for NeverCalled {
            fn load(&self, _: &str) -> Result<Option<Vec<u8>>> {
                panic!("loader should not be consulted")
            }
        }
        let b = EmbeddedBoard::default();
        let err = b.resolve_with(&NeverCalled).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("DOCUMENTPATH"), "got: {msg}");
    }

    #[test]
    fn resolve_with_loader_errors_when_loader_returns_none() {
        struct NotFound;
        impl BoardLoader for NotFound {
            fn load(&self, _: &str) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
        }
        let mut b = EmbeddedBoard::default();
        b.document_path = Some("ghost.PcbDoc".into());
        let err = b.resolve_with(&NotFound).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("ghost.PcbDoc"), "got: {msg}");
    }
}
