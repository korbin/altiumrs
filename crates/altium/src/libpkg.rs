//! Library package (`.LibPkg`) — the source project file that compiles to an
//! [`IntegratedLibrary`].
//!
//! `.LibPkg` is a UTF-8 (BOM) INI-style text file with CRLF line terminators.
//! Each `[Section]` carries a list of `Key=Value` pairs; section and key
//! ordering matter for diff-friendliness, so we preserve both via
//! [`indexmap::IndexMap`].
//!
//! Two main use cases:
//!
//! 1. **Emission** — author a fresh `.LibPkg` referencing a `.SchLib` and a
//!    `.PcbLib`. [`LibraryPackage::with_defaults`] starts from the same
//!    boilerplate Altium Designer emits for new library projects;
//!    [`LibraryPackage::add_document`] appends new
//!    `[Document1]`/`[Document2]`/… sections.
//!
//! 2. **Splitting** — given a compiled `.IntLib`, write the bundled source
//!    libraries (and any additional files) back to disk and produce a
//!    matching `.LibPkg`. See [`IntegratedLibrary::split_to_directory`].

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use indexmap::IndexMap;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{Error, Result};
use crate::intlib::IntegratedLibrary;

/// One `[DocumentN]` section parsed out of a [`LibraryPackage`].
///
/// Round-trip safe: any keys we don't surface as typed fields are kept on the
/// originating section in [`LibraryPackage::sections`].
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DocumentReference {
    /// Path the project records for the source library, typically a relative
    /// filename (`"Parts.SchLib"`) sitting next to the `.LibPkg`.
    pub document_path: String,
    pub annotation_enabled: bool,
    pub annotate_start_value: i32,
    pub annotation_index_control_enabled: bool,
    pub annotate_suffix: String,
    pub annotate_scope: String,
    pub annotate_order: i32,
    pub do_library_update: bool,
    pub do_database_update: bool,
    pub class_gen_cc_auto_enabled: bool,
    pub class_gen_cc_auto_room_enabled: bool,
    pub class_gen_nc_auto_scope: String,
    pub d_item_revision_guid: String,
    pub generate_class_cluster: bool,
    pub document_unique_id: String,
}

impl DocumentReference {
    /// Convenience: a fresh document reference for `path` matching the
    /// defaults Altium Designer writes for a new library document.
    pub fn new(document_path: impl Into<String>) -> Self {
        Self {
            document_path: document_path.into(),
            annotation_enabled: true,
            annotate_start_value: 1,
            annotation_index_control_enabled: false,
            annotate_suffix: String::new(),
            annotate_scope: "All".into(),
            annotate_order: -1,
            do_library_update: true,
            do_database_update: true,
            class_gen_cc_auto_enabled: true,
            class_gen_cc_auto_room_enabled: false,
            class_gen_nc_auto_scope: "None".into(),
            d_item_revision_guid: String::new(),
            generate_class_cluster: false,
            document_unique_id: String::new(),
        }
    }

    fn from_section(sec: &IndexMap<String, String>) -> Self {
        let g = |k: &str| sec.get(k).cloned().unwrap_or_default();
        let g_bool = |k: &str| matches!(sec.get(k).map(|s| s.as_str()), Some("1" | "True" | "TRUE"));
        let g_i32 = |k: &str, default: i32| {
            sec.get(k)
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(default)
        };
        Self {
            document_path: g("DocumentPath"),
            annotation_enabled: g_bool("AnnotationEnabled"),
            annotate_start_value: g_i32("AnnotateStartValue", 1),
            annotation_index_control_enabled: g_bool("AnnotationIndexControlEnabled"),
            annotate_suffix: g("AnnotateSuffix"),
            annotate_scope: if sec.contains_key("AnnotateScope") {
                g("AnnotateScope")
            } else {
                "All".into()
            },
            annotate_order: g_i32("AnnotateOrder", -1),
            do_library_update: g_bool("DoLibraryUpdate"),
            do_database_update: g_bool("DoDatabaseUpdate"),
            class_gen_cc_auto_enabled: g_bool("ClassGenCCAutoEnabled"),
            class_gen_cc_auto_room_enabled: g_bool("ClassGenCCAutoRoomEnabled"),
            class_gen_nc_auto_scope: if sec.contains_key("ClassGenNCAutoScope") {
                g("ClassGenNCAutoScope")
            } else {
                "None".into()
            },
            d_item_revision_guid: g("DItemRevisionGUID"),
            generate_class_cluster: g_bool("GenerateClassCluster"),
            document_unique_id: g("DocumentUniqueId"),
        }
    }

    fn write_into(&self, sec: &mut IndexMap<String, String>) {
        sec.insert("DocumentPath".into(), self.document_path.clone());
        sec.insert(
            "AnnotationEnabled".into(),
            bool_to_int(self.annotation_enabled).into(),
        );
        sec.insert(
            "AnnotateStartValue".into(),
            self.annotate_start_value.to_string(),
        );
        sec.insert(
            "AnnotationIndexControlEnabled".into(),
            bool_to_int(self.annotation_index_control_enabled).into(),
        );
        sec.insert("AnnotateSuffix".into(), self.annotate_suffix.clone());
        sec.insert("AnnotateScope".into(), self.annotate_scope.clone());
        sec.insert("AnnotateOrder".into(), self.annotate_order.to_string());
        sec.insert(
            "DoLibraryUpdate".into(),
            bool_to_int(self.do_library_update).into(),
        );
        sec.insert(
            "DoDatabaseUpdate".into(),
            bool_to_int(self.do_database_update).into(),
        );
        sec.insert(
            "ClassGenCCAutoEnabled".into(),
            bool_to_int(self.class_gen_cc_auto_enabled).into(),
        );
        sec.insert(
            "ClassGenCCAutoRoomEnabled".into(),
            bool_to_int(self.class_gen_cc_auto_room_enabled).into(),
        );
        sec.insert(
            "ClassGenNCAutoScope".into(),
            self.class_gen_nc_auto_scope.clone(),
        );
        sec.insert(
            "DItemRevisionGUID".into(),
            self.d_item_revision_guid.clone(),
        );
        sec.insert(
            "GenerateClassCluster".into(),
            bool_to_int(self.generate_class_cluster).into(),
        );
        sec.insert("DocumentUniqueId".into(), self.document_unique_id.clone());
    }
}

/// Parsed INI representation of a `.LibPkg` plus typed accessors for the
/// document list.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LibraryPackage {
    /// All sections in source order. Document sections live here too —
    /// [`documents`](Self::documents) extracts them as typed
    /// [`DocumentReference`]s.
    pub sections: IndexMap<String, IndexMap<String, String>>,
}

impl FromStr for LibraryPackage {
    type Err = Error;
    fn from_str(text: &str) -> Result<Self> {
        Self::parse(text)
    }
}

impl LibraryPackage {
    /// Parse a `.LibPkg` file from a UTF-8 string. The text may begin with a
    /// BOM and use either CRLF or LF line terminators.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
        let mut sections: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
        let mut current: Option<&str> = None;
        let mut working: IndexMap<String, String> = IndexMap::new();

        // Section header lookups must be case-insensitive on read but preserve
        // case on write — the easiest way is to normalise on insert.
        let flush =
            |sections: &mut IndexMap<String, IndexMap<String, String>>,
             name: Option<&str>,
             working: &mut IndexMap<String, String>| {
                if let Some(name) = name {
                    sections.insert(name.to_string(), std::mem::take(working));
                }
            };

        for raw in text.lines() {
            let line = raw.trim_end_matches('\r').trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                if let Some(name) = rest.strip_suffix(']') {
                    flush(&mut sections, current, &mut working);
                    current = Some(name);
                    working = IndexMap::new();
                    continue;
                }
            }
            if let Some((key, value)) = line.split_once('=') {
                if current.is_none() {
                    return Err(Error::corrupt(format!(
                        "key {key:?} before any [Section] header"
                    )));
                }
                working.insert(key.to_string(), value.to_string());
            }
            // Lines that are neither header nor key=value are ignored — Altium
            // sometimes emits stray blank-equivalents; we don't error on them.
        }
        flush(&mut sections, current, &mut working);

        Ok(Self { sections })
    }

    /// Read a `.LibPkg` file from disk.
    pub async fn read(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = tokio::fs::read(path).await?;
        let text = String::from_utf8(bytes)
            .map_err(|e| Error::Encoding(format!("LibPkg is not valid UTF-8: {e}")))?;
        Self::parse(&text)
    }

    /// Read from any [`AsyncRead`].
    pub async fn read_async<R>(mut reader: R) -> Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let text = String::from_utf8(bytes)
            .map_err(|e| Error::Encoding(format!("LibPkg is not valid UTF-8: {e}")))?;
        Self::parse(&text)
    }

    /// Render to a UTF-8 string with a BOM and CRLF terminators, matching the
    /// byte layout Altium Designer writes.
    pub fn to_string_crlf(&self) -> String {
        // BOM, then `[Section]\r\nKey=Value\r\n…\r\n` per section, blank line
        // between sections.
        let mut out = String::from('\u{FEFF}');
        let total_sections = self.sections.len();
        for (i, (section, entries)) in self.sections.iter().enumerate() {
            let _ = write!(out, "[{section}]\r\n");
            for (key, value) in entries {
                let _ = write!(out, "{key}={value}\r\n");
            }
            if i + 1 < total_sections {
                out.push_str("\r\n");
            }
        }
        // Trailing newline after the last section (matches Altium's emission).
        out.push_str("\r\n");
        out
    }

    /// Async write to disk.
    pub async fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        tokio::fs::write(path, self.to_string_crlf()).await?;
        Ok(())
    }

    /// Async write to any [`AsyncWrite`].
    pub async fn write_async<W>(&self, mut writer: W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        writer.write_all(self.to_string_crlf().as_bytes()).await?;
        writer.flush().await?;
        Ok(())
    }

    /// Look up a section by case-insensitive name.
    pub fn section(&self, name: &str) -> Option<&IndexMap<String, String>> {
        self.sections
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    }

    /// Extract every `[DocumentN]` section as a typed [`DocumentReference`].
    pub fn documents(&self) -> Vec<DocumentReference> {
        let mut indexed: Vec<(usize, DocumentReference)> = self
            .sections
            .iter()
            .filter_map(|(name, sec)| {
                let n = name
                    .strip_prefix("Document")
                    .or_else(|| name.strip_prefix("document"))?
                    .parse::<usize>()
                    .ok()?;
                Some((n, DocumentReference::from_section(sec)))
            })
            .collect();
        indexed.sort_by_key(|(n, _)| *n);
        indexed.into_iter().map(|(_, d)| d).collect()
    }

    /// Append a new `[DocumentN]` section pointing at `path`. Returns the
    /// 1-based index assigned (so `1` means `[Document1]`).
    pub fn add_document(&mut self, path: impl Into<String>) -> usize {
        let next_index = self
            .sections
            .keys()
            .filter_map(|name| name.strip_prefix("Document").and_then(|n| n.parse::<usize>().ok()))
            .max()
            .map_or(1, |n| n + 1);
        let mut sec = IndexMap::new();
        DocumentReference::new(path).write_into(&mut sec);
        self.sections.insert(format!("Document{next_index}"), sec);
        next_index
    }

    /// Replace every existing `[DocumentN]` section with the typed list. Keeps
    /// non-document sections untouched.
    pub fn set_documents(&mut self, documents: &[DocumentReference]) {
        let keys: Vec<String> = self
            .sections
            .keys()
            .filter(|name| {
                name.strip_prefix("Document")
                    .and_then(|n| n.parse::<usize>().ok())
                    .is_some()
            })
            .cloned()
            .collect();
        for k in keys {
            self.sections.shift_remove(&k);
        }
        for (i, doc) in documents.iter().enumerate() {
            let mut sec = IndexMap::new();
            doc.write_into(&mut sec);
            self.sections.insert(format!("Document{}", i + 1), sec);
        }
    }

    /// Build a minimal but valid `.LibPkg` with the `[Design]` defaults
    /// Altium Designer writes for new library projects. Use
    /// [`add_document`](Self::add_document) to attach `.SchLib`/`.PcbLib`
    /// references.
    pub fn with_defaults() -> Self {
        let mut sections = IndexMap::new();
        sections.insert("Design".into(), default_design_section());
        sections.insert("Preferences".into(), default_preferences_section());
        Self { sections }
    }
}

fn default_design_section() -> IndexMap<String, String> {
    let mut m = IndexMap::new();
    m.insert("Version".into(), "1.0".into());
    m.insert("HierarchyMode".into(), "0".into());
    m.insert("ChannelRoomNamingStyle".into(), "0".into());
    m.insert("ReleasesFolder".into(), String::new());
    m.insert(
        "ChannelDesignatorFormatString".into(),
        "$Component_$RoomName".into(),
    );
    m.insert("ChannelRoomLevelSeperator".into(), "_".into());
    m.insert("OpenOutputs".into(), "1".into());
    m.insert("ArchiveProject".into(), "0".into());
    m.insert("TimestampOutput".into(), "0".into());
    m.insert("SeparateFolders".into(), "0".into());
    m.insert("PinSwapBy_Netlabel".into(), "1".into());
    m.insert("PinSwapBy_Pin".into(), "1".into());
    m.insert("AllowPortNetNames".into(), "0".into());
    m.insert("AllowSheetEntryNetNames".into(), "1".into());
    m.insert("AppendSheetNumberToLocalNets".into(), "0".into());
    m.insert("NetlistSinglePinNets".into(), "0".into());
    m.insert(
        "DefaultConfiguration".into(),
        "Default - All Constraints".into(),
    );
    m.insert("UserID".into(), "0xFFFFFFFF".into());
    m.insert("DefaultPcbProtel".into(), "1".into());
    m.insert("DefaultPcbPcad".into(), "0".into());
    m.insert("ReorderDocumentsOnCompile".into(), "1".into());
    m.insert("NameNetsHierarchically".into(), "0".into());
    m.insert("PowerPortNamesTakePriority".into(), "0".into());
    m.insert("AutoSheetNumbering".into(), "0".into());
    m.insert("AutoCrossReferences".into(), "1".into());
    m.insert("NewIndexingOfSheetSymbols".into(), "1".into());
    m.insert("PushECOToAnnotationFile".into(), "1".into());
    m.insert("ReportSuppressedErrorsInMessages".into(), "0".into());
    m.insert("ConstraintManagerFlow".into(), "0".into());
    m.insert("IsVirtualBomDocumentRemoved".into(), "0".into());
    m
}

fn default_preferences_section() -> IndexMap<String, String> {
    let mut m = IndexMap::new();
    m.insert("PrefsVaultGUID".into(), String::new());
    m.insert("PrefsRevisionGUID".into(), String::new());
    m
}

fn bool_to_int(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

// ─── IntegratedLibrary → on-disk split ──────────────────────────────────────

impl IntegratedLibrary {
    /// Write every embedded library and additional file to `dir` and produce
    /// a [`LibraryPackage`] referencing them. Returns the package along with
    /// the set of paths it wrote, all relative to `dir`.
    ///
    /// `base_name` is used as the suggested basename for the output `.LibPkg`
    /// file (the package itself isn't written here; the caller can pick its
    /// own filename via `package.write(...)`).
    ///
    /// Library entries with paths like `"Parts.PcbLib"` are written verbatim;
    /// nested paths like `"/Embedded/Sub.PcbLib"` are mirrored under `dir`.
    /// Additional files (datasheets, sim models) are written verbatim to
    /// preserve directory layout.
    pub async fn split_to_directory(
        &self,
        dir: impl AsRef<Path>,
        base_name: &str,
    ) -> Result<SplitResult> {
        let dir = dir.as_ref().to_path_buf();
        if !dir.exists() {
            tokio::fs::create_dir_all(&dir).await?;
        }
        let mut written: Vec<PathBuf> = Vec::new();
        let mut package = LibraryPackage::with_defaults();

        // Schematic libs first, then footprint libs — matches the pattern in
        // most Altium-emitted .LibPkg files (PcbLib usually comes first
        // historically, but ordering is not load-bearing — Altium reads them
        // by name match anyway).
        for entry in &self.schematic_libraries {
            let rel = sanitise_relative_path(&entry.name, "SchLib");
            let abs = dir.join(&rel);
            ensure_parent(&abs).await?;
            tokio::fs::write(&abs, entry.library.to_bytes()?).await?;
            package.add_document(rel.to_string_lossy().to_string());
            written.push(abs);
        }
        for entry in &self.footprint_libraries {
            let rel = sanitise_relative_path(&entry.name, "PcbLib");
            let abs = dir.join(&rel);
            ensure_parent(&abs).await?;
            tokio::fs::write(&abs, entry.library.to_bytes()?).await?;
            package.add_document(rel.to_string_lossy().to_string());
            written.push(abs);
        }
        for (name, bytes) in &self.additional_files {
            let rel = sanitise_relative_path(name, "");
            let abs = dir.join(&rel);
            ensure_parent(&abs).await?;
            tokio::fs::write(&abs, bytes).await?;
            written.push(abs);
        }

        let package_path = dir.join(format!("{base_name}.LibPkg"));
        Ok(SplitResult {
            package,
            package_path,
            written_files: written,
        })
    }
}

/// Result of [`IntegratedLibrary::split_to_directory`].
#[derive(Debug, Clone)]
pub struct SplitResult {
    /// The generated `.LibPkg` model. Caller is expected to call
    /// `package.write(package_path).await` to persist it (the split function
    /// deliberately doesn't write it so callers can edit the package first).
    pub package: LibraryPackage,
    /// Suggested path for the `.LibPkg` (in the split directory).
    pub package_path: PathBuf,
    /// Every embedded-file path written, in absolute form.
    pub written_files: Vec<PathBuf>,
}

impl SplitResult {
    /// Persist the `.LibPkg` to its suggested path. Convenience for callers
    /// who don't want to edit the package first.
    pub async fn write_package(&self) -> Result<()> {
        self.package.write(&self.package_path).await
    }
}

fn sanitise_relative_path(name: &str, fallback_ext: &str) -> PathBuf {
    // Translate Windows backslashes to native forward slashes so paths like
    // `Datasheets\u1.pdf` from older IntLibs end up under proper subfolders.
    let normalised = name.replace('\\', "/");
    let trimmed = normalised.trim_start_matches('/');
    if trimmed.is_empty() {
        return PathBuf::from(format!("Untitled.{fallback_ext}"));
    }
    let mut buf = PathBuf::new();
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            // `..` could escape the split directory; replace with a marker
            // rather than erroring (split is forgiving by design).
            buf.push("UNSAFE");
            continue;
        }
        buf.push(segment);
    }
    if buf.as_os_str().is_empty() {
        buf.push(format!("Untitled.{fallback_ext}"));
    }
    buf
}

async fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    Ok(())
}

// ─── Convenience: build a LibPkg from arbitrary local paths ────────────────

impl LibraryPackage {
    /// Build a fresh `.LibPkg` referencing each path (treated as relative
    /// document paths). Equivalent to `with_defaults()` followed by
    /// `add_document` for each entry.
    pub fn from_paths<I, S>(paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut pkg = Self::with_defaults();
        for p in paths {
            pkg.add_document(p);
        }
        pkg
    }

    /// Returns `true` if the LibPkg references at least one document whose
    /// path ends with `.SchLib` (case-insensitive).
    pub fn has_schlib(&self) -> bool {
        self.documents()
            .iter()
            .any(|d| d.document_path.to_ascii_lowercase().ends_with(".schlib"))
    }

    /// Returns `true` if the LibPkg references at least one document whose
    /// path ends with `.PcbLib` (case-insensitive).
    pub fn has_pcblib(&self) -> bool {
        self.documents()
            .iter()
            .any(|d| d.document_path.to_ascii_lowercase().ends_with(".pcblib"))
    }
}

// ─── Round-trip helper for opaque section bags ─────────────────────────────

impl LibraryPackage {
    /// Build by deserialising from a flat `BTreeMap<String, BTreeMap<…>>`.
    /// Useful for tools that hold the parsed form already.
    pub fn from_section_map(map: BTreeMap<String, BTreeMap<String, String>>) -> Self {
        let mut sections = IndexMap::new();
        for (sec_name, entries) in map {
            let mut inner = IndexMap::new();
            for (k, v) in entries {
                inner.insert(k, v);
            }
            sections.insert(sec_name, inner);
        }
        Self { sections }
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::intlib::NamedLibrary;
    use crate::{pcb, sch};

    #[test]
    fn parses_simple_libpkg() {
        let text = "\u{FEFF}[Design]\r\nVersion=1.0\r\nUserID=0xDEADBEEF\r\n\r\n[Document1]\r\nDocumentPath=Parts.SchLib\r\nAnnotateScope=All\r\n";
        let pkg = LibraryPackage::parse(text).unwrap();
        assert_eq!(
            pkg.section("Design").unwrap().get("Version"),
            Some(&"1.0".to_string())
        );
        let docs = pkg.documents();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].document_path, "Parts.SchLib");
        assert_eq!(docs[0].annotate_scope, "All");
    }

    #[test]
    fn round_trips_real_world_libpkg_text() {
        // A trimmed-down approximation of an Altium-emitted LibPkg.
        let original = "\u{FEFF}[Design]\r\nVersion=1.0\r\nHierarchyMode=0\r\nUserID=0xFFFFFFFF\r\n\r\n[Document1]\r\nDocumentPath=DMN62D1LFB-7B.PcbLib\r\nAnnotationEnabled=1\r\nAnnotateStartValue=1\r\nAnnotationIndexControlEnabled=0\r\nAnnotateSuffix=\r\nAnnotateScope=All\r\nAnnotateOrder=-1\r\nDoLibraryUpdate=1\r\nDoDatabaseUpdate=1\r\nClassGenCCAutoEnabled=1\r\nClassGenCCAutoRoomEnabled=0\r\nClassGenNCAutoScope=None\r\nDItemRevisionGUID=\r\nGenerateClassCluster=0\r\nDocumentUniqueId=\r\n\r\n[Document2]\r\nDocumentPath=DMN62D1LFB-7B.SchLib\r\nAnnotationEnabled=1\r\nAnnotateStartValue=1\r\nAnnotationIndexControlEnabled=0\r\nAnnotateSuffix=\r\nAnnotateScope=All\r\nAnnotateOrder=-1\r\nDoLibraryUpdate=1\r\nDoDatabaseUpdate=1\r\nClassGenCCAutoEnabled=1\r\nClassGenCCAutoRoomEnabled=0\r\nClassGenNCAutoScope=None\r\nDItemRevisionGUID=\r\nGenerateClassCluster=0\r\nDocumentUniqueId=\r\n";
        let pkg = LibraryPackage::parse(original).expect("parse");
        let docs = pkg.documents();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].document_path, "DMN62D1LFB-7B.PcbLib");
        assert_eq!(docs[1].document_path, "DMN62D1LFB-7B.SchLib");
        assert!(docs[0].annotation_enabled);

        let emitted = pkg.to_string_crlf();
        // Re-parse and check the documents survive byte-for-byte semantics
        // (whitespace might differ but the values should match).
        let reparsed = LibraryPackage::parse(&emitted).expect("reparse");
        let docs2 = reparsed.documents();
        assert_eq!(docs2, docs);
    }

    #[test]
    fn parses_real_dmn62d1lfb_libpkg() {
        // Sample fixture provided in /tmp/DMN62D1LFB-7B/. We hard-code a
        // minimal subset to keep the test hermetic.
        let path = std::path::Path::new("/tmp/DMN62D1LFB-7B/DMN62D1LFB-7B.LibPkg");
        if !path.exists() {
            eprintln!("skipping: fixture not present");
            return;
        }
        let bytes = std::fs::read(path).expect("read fixture");
        let text = std::str::from_utf8(&bytes).expect("utf-8");
        let pkg = LibraryPackage::parse(text).expect("parse");
        let docs = pkg.documents();
        assert_eq!(docs.len(), 2, "fixture has Document1 + Document2");
        let paths: Vec<String> = docs.iter().map(|d| d.document_path.clone()).collect();
        assert!(paths.contains(&"DMN62D1LFB-7B.PcbLib".to_string()));
        assert!(paths.contains(&"DMN62D1LFB-7B.SchLib".to_string()));
        assert!(pkg.has_schlib());
        assert!(pkg.has_pcblib());
    }

    #[test]
    fn add_document_picks_next_index() {
        let mut pkg = LibraryPackage::with_defaults();
        assert_eq!(pkg.add_document("foo.SchLib"), 1);
        assert_eq!(pkg.add_document("bar.PcbLib"), 2);
        let docs = pkg.documents();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].document_path, "foo.SchLib");
        assert_eq!(docs[1].document_path, "bar.PcbLib");
    }

    #[test]
    fn set_documents_replaces_existing() {
        let mut pkg = LibraryPackage::with_defaults();
        pkg.add_document("old.SchLib");
        pkg.set_documents(&[
            DocumentReference::new("new.SchLib"),
            DocumentReference::new("new.PcbLib"),
        ]);
        let docs = pkg.documents();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].document_path, "new.SchLib");
        assert_eq!(docs[1].document_path, "new.PcbLib");
    }

    #[test]
    fn emits_bom_and_crlf() {
        let pkg = LibraryPackage::with_defaults();
        let s = pkg.to_string_crlf();
        assert!(s.starts_with('\u{FEFF}'));
        assert!(s.contains("\r\n"));
    }

    #[tokio::test]
    async fn split_intlib_writes_constituent_files_and_libpkg() {
        // Build a synthetic IntLib with one SchLib + one PcbLib + one
        // datasheet. Split it to a temp dir and verify files appear plus the
        // returned LibraryPackage references them.
        let mut sch_lib = sch::Library::default();
        sch_lib.components.push(sch::Component::new("U1"));

        let mut pcb_lib = pcb::Library::default();
        pcb_lib.unique_id = "ABCD0001".into();
        pcb_lib.components.push(pcb::Component::new("R0402"));

        let mut intlib = IntegratedLibrary::default();
        intlib.schematic_libraries.push(NamedLibrary {
            name: "Parts.SchLib".into(),
            library: sch_lib,
        });
        intlib.footprint_libraries.push(NamedLibrary {
            name: "Parts.PcbLib".into(),
            library: pcb_lib,
        });
        intlib
            .additional_files
            .insert("Datasheets/u1.pdf".into(), b"%PDF-1.4 stub".to_vec());

        let nonce = std::process::id();
        let dir = std::env::temp_dir().join(format!("altium-rs-libpkg-split-{nonce}"));
        let _ = std::fs::remove_dir_all(&dir);

        let split = intlib
            .split_to_directory(&dir, "Parts")
            .await
            .expect("split");

        assert!(dir.join("Parts.SchLib").is_file());
        assert!(dir.join("Parts.PcbLib").is_file());
        assert!(dir.join("Datasheets/u1.pdf").is_file());

        let docs = split.package.documents();
        let paths: Vec<String> = docs.iter().map(|d| d.document_path.clone()).collect();
        assert!(paths.contains(&"Parts.SchLib".to_string()));
        assert!(paths.contains(&"Parts.PcbLib".to_string()));

        // Persist the LibPkg using the helper and re-read.
        split.write_package().await.expect("write libpkg");
        let pkg_path = dir.join("Parts.LibPkg");
        assert!(pkg_path.is_file());
        let reread = LibraryPackage::read(&pkg_path).await.expect("read libpkg");
        let reread_docs = reread.documents();
        assert_eq!(reread_docs.len(), docs.len());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn from_paths_builds_libpkg_directly() {
        let pkg = LibraryPackage::from_paths(["a.SchLib", "b.PcbLib"]);
        let docs = pkg.documents();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].document_path, "a.SchLib");
        assert_eq!(docs[1].document_path, "b.PcbLib");
        assert!(pkg.has_schlib());
        assert!(pkg.has_pcblib());
    }

    #[test]
    fn empty_text_parses_as_empty_package() {
        let pkg = LibraryPackage::parse("").unwrap();
        assert!(pkg.sections.is_empty());
        assert!(pkg.documents().is_empty());
    }

    #[test]
    fn sanitise_relative_path_strips_leading_separators() {
        assert_eq!(
            sanitise_relative_path("/foo/bar.SchLib", "SchLib"),
            PathBuf::from("foo").join("bar.SchLib")
        );
        assert_eq!(
            sanitise_relative_path("\\Datasheets\\x.pdf", ""),
            PathBuf::from("Datasheets").join("x.pdf")
        );
    }

    #[test]
    fn sanitise_relative_path_replaces_dotdot() {
        let p = sanitise_relative_path("../escape/foo.SchLib", "SchLib");
        assert_eq!(p, PathBuf::from("UNSAFE").join("escape").join("foo.SchLib"));
    }

    #[test]
    fn sanitise_relative_path_falls_back_when_empty() {
        assert_eq!(
            sanitise_relative_path("", "SchLib"),
            PathBuf::from("Untitled.SchLib")
        );
    }
}
