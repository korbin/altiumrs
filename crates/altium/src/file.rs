//! Unified entry point that auto-detects the file kind from the extension.
//!
//! # Examples
//!
//! ```no_run
//! # async fn run() -> altium::Result<()> {
//! use altium::AltiumFile;
//!
//! match AltiumFile::read("MyBoard.PcbDoc").await? {
//!     AltiumFile::PcbDocument(doc) => println!("{} pads", doc.pads.len()),
//!     AltiumFile::PcbLibrary(lib) => println!("{} footprints", lib.components.len()),
//!     AltiumFile::SchDocument(doc) => println!("{} raw records", doc.raw_records.len()),
//!     AltiumFile::SchLibrary(lib) => println!("{} symbols", lib.components.len()),
//!     AltiumFile::IntegratedLibrary(intlib) => println!(
//!         "IntLib: {} sch + {} pcb",
//!         intlib.schematic_libraries.len(),
//!         intlib.footprint_libraries.len(),
//!     ),
//! }
//! # Ok(()) }
//! ```

use std::path::Path;

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::error::{Error, Result};
use crate::intlib::IntegratedLibrary;
use crate::{pcb, sch};

/// The kind of Altium file. Matches the five supported on-disk formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AltiumFileKind {
    /// `.PcbLib`: PCB footprint library.
    PcbLibrary,
    /// `.SchLib`: schematic symbol library.
    SchLibrary,
    /// `.PcbDoc`: PCB document.
    PcbDocument,
    /// `.SchDoc`: schematic document.
    SchDocument,
    /// `.IntLib`: compiled integrated library bundling SchLib + PcbLib.
    IntegratedLibrary,
}

impl AltiumFileKind {
    /// Detect the kind from a file extension. Case-insensitive and tolerant of
    /// a leading `.` (`"pcblib"`, `".PcbLib"`, and `"PCBLIB"` all work).
    pub fn from_extension(ext: &str) -> Option<Self> {
        let ext = ext.trim_start_matches('.');
        if ext.eq_ignore_ascii_case("pcblib") {
            Some(Self::PcbLibrary)
        } else if ext.eq_ignore_ascii_case("schlib") {
            Some(Self::SchLibrary)
        } else if ext.eq_ignore_ascii_case("pcbdoc") {
            Some(Self::PcbDocument)
        } else if ext.eq_ignore_ascii_case("schdoc") {
            Some(Self::SchDocument)
        } else if ext.eq_ignore_ascii_case("intlib") {
            Some(Self::IntegratedLibrary)
        } else {
            None
        }
    }

    /// Detect the kind from a path's extension.
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        path.as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }

    /// The canonical file extension Altium writes for this kind, including the
    /// leading dot (`".PcbLib"`, etc.).
    pub fn extension(self) -> &'static str {
        match self {
            Self::PcbLibrary => ".PcbLib",
            Self::SchLibrary => ".SchLib",
            Self::PcbDocument => ".PcbDoc",
            Self::SchDocument => ".SchDoc",
            Self::IntegratedLibrary => ".IntLib",
        }
    }

    /// `true` for `.PcbLib`, `.SchLib`, and `.IntLib`.
    pub fn is_library(self) -> bool {
        matches!(
            self,
            Self::PcbLibrary | Self::SchLibrary | Self::IntegratedLibrary
        )
    }

    /// `true` for `.PcbDoc` and `.SchDoc`.
    pub fn is_document(self) -> bool {
        matches!(self, Self::PcbDocument | Self::SchDocument)
    }
}

/// An Altium file of any of the four supported kinds.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AltiumFile {
    PcbLibrary(pcb::Library),
    SchLibrary(sch::Library),
    PcbDocument(pcb::Document),
    SchDocument(sch::Document),
    IntegratedLibrary(IntegratedLibrary),
}

impl AltiumFile {
    /// Open an Altium file from disk; the kind is detected from the extension.
    pub async fn read(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let kind = AltiumFileKind::from_path(path).ok_or_else(|| {
            Error::corrupt(format!(
                "unrecognised Altium file extension: {}",
                path.display()
            ))
        })?;
        let bytes = tokio::fs::read(path).await?;
        Self::from_bytes(kind, bytes).map_err(|e| e.with_path(path))
    }

    /// Parse from raw bytes, given an explicit [`AltiumFileKind`].
    pub fn from_bytes(kind: AltiumFileKind, bytes: Vec<u8>) -> Result<Self> {
        Ok(match kind {
            AltiumFileKind::PcbLibrary => Self::PcbLibrary(pcb::Library::from_bytes(bytes)?),
            AltiumFileKind::SchLibrary => Self::SchLibrary(sch::Library::from_bytes(bytes)?),
            AltiumFileKind::PcbDocument => Self::PcbDocument(pcb::Document::from_bytes(bytes)?),
            AltiumFileKind::SchDocument => Self::SchDocument(sch::Document::from_bytes(bytes)?),
            AltiumFileKind::IntegratedLibrary => {
                Self::IntegratedLibrary(IntegratedLibrary::from_bytes(bytes)?)
            }
        })
    }

    /// Parse from any [`AsyncRead`] given an explicit kind.
    pub async fn read_async<R>(kind: AltiumFileKind, mut reader: R) -> Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Self::from_bytes(kind, bytes)
    }

    /// Serialise back to bytes (round-trippable through [`from_bytes`](Self::from_bytes)).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        match self {
            Self::PcbLibrary(lib) => lib.to_bytes(),
            Self::SchLibrary(lib) => lib.to_bytes(),
            Self::PcbDocument(doc) => doc.to_bytes(),
            Self::SchDocument(doc) => doc.to_bytes(),
            Self::IntegratedLibrary(lib) => lib.to_bytes(),
        }
    }

    /// Write to disk. The variant determines the on-disk layout — the path's
    /// extension is **not** rewritten. Make sure it matches `self.kind()`.
    pub async fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = self.to_bytes()?;
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    /// The kind of this file.
    pub fn kind(&self) -> AltiumFileKind {
        match self {
            Self::PcbLibrary(_) => AltiumFileKind::PcbLibrary,
            Self::SchLibrary(_) => AltiumFileKind::SchLibrary,
            Self::PcbDocument(_) => AltiumFileKind::PcbDocument,
            Self::SchDocument(_) => AltiumFileKind::SchDocument,
            Self::IntegratedLibrary(_) => AltiumFileKind::IntegratedLibrary,
        }
    }

    pub fn as_pcb_library(&self) -> Option<&pcb::Library> {
        if let Self::PcbLibrary(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_sch_library(&self) -> Option<&sch::Library> {
        if let Self::SchLibrary(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_pcb_document(&self) -> Option<&pcb::Document> {
        if let Self::PcbDocument(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_sch_document(&self) -> Option<&sch::Document> {
        if let Self::SchDocument(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_integrated_library(&self) -> Option<&IntegratedLibrary> {
        if let Self::IntegratedLibrary(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_pcb_library_mut(&mut self) -> Option<&mut pcb::Library> {
        if let Self::PcbLibrary(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_sch_library_mut(&mut self) -> Option<&mut sch::Library> {
        if let Self::SchLibrary(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_pcb_document_mut(&mut self) -> Option<&mut pcb::Document> {
        if let Self::PcbDocument(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_sch_document_mut(&mut self) -> Option<&mut sch::Document> {
        if let Self::SchDocument(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_integrated_library_mut(&mut self) -> Option<&mut IntegratedLibrary> {
        if let Self::IntegratedLibrary(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn into_pcb_library(self) -> Option<pcb::Library> {
        if let Self::PcbLibrary(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn into_sch_library(self) -> Option<sch::Library> {
        if let Self::SchLibrary(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn into_pcb_document(self) -> Option<pcb::Document> {
        if let Self::PcbDocument(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn into_sch_document(self) -> Option<sch::Document> {
        if let Self::SchDocument(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn into_integrated_library(self) -> Option<IntegratedLibrary> {
        if let Self::IntegratedLibrary(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

impl From<pcb::Library> for AltiumFile {
    fn from(v: pcb::Library) -> Self {
        Self::PcbLibrary(v)
    }
}
impl From<sch::Library> for AltiumFile {
    fn from(v: sch::Library) -> Self {
        Self::SchLibrary(v)
    }
}
impl From<pcb::Document> for AltiumFile {
    fn from(v: pcb::Document) -> Self {
        Self::PcbDocument(v)
    }
}
impl From<sch::Document> for AltiumFile {
    fn from(v: sch::Document) -> Self {
        Self::SchDocument(v)
    }
}
impl From<IntegratedLibrary> for AltiumFile {
    fn from(v: IntegratedLibrary) -> Self {
        Self::IntegratedLibrary(v)
    }
}

/// Open any Altium file with kind detected from the extension. Convenience
/// wrapper around [`AltiumFile::read`].
pub async fn open(path: impl AsRef<Path>) -> Result<AltiumFile> {
    AltiumFile::read(path).await
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn extension_detection_is_case_insensitive_and_dot_tolerant() {
        for ext in ["PcbLib", ".PcbLib", "PCBLIB", "pcblib"] {
            assert_eq!(
                AltiumFileKind::from_extension(ext),
                Some(AltiumFileKind::PcbLibrary)
            );
        }
        for ext in ["SchLib", ".schlib", "SCHLIB"] {
            assert_eq!(
                AltiumFileKind::from_extension(ext),
                Some(AltiumFileKind::SchLibrary)
            );
        }
        for ext in ["PcbDoc", ".PCBDOC"] {
            assert_eq!(
                AltiumFileKind::from_extension(ext),
                Some(AltiumFileKind::PcbDocument)
            );
        }
        for ext in ["SchDoc", ".schdoc"] {
            assert_eq!(
                AltiumFileKind::from_extension(ext),
                Some(AltiumFileKind::SchDocument)
            );
        }
        assert!(AltiumFileKind::from_extension("txt").is_none());
        assert!(AltiumFileKind::from_extension("").is_none());
    }

    #[test]
    fn path_dispatch() {
        let p = PathBuf::from("/x/y/MyLib.PcbLib");
        assert_eq!(
            AltiumFileKind::from_path(&p),
            Some(AltiumFileKind::PcbLibrary)
        );
        let p = PathBuf::from("schematic.SCHDOC");
        assert_eq!(
            AltiumFileKind::from_path(&p),
            Some(AltiumFileKind::SchDocument)
        );
        let p = PathBuf::from("noext");
        assert_eq!(AltiumFileKind::from_path(&p), None);
    }

    #[test]
    fn extension_round_trip() {
        let kinds = [
            AltiumFileKind::PcbLibrary,
            AltiumFileKind::SchLibrary,
            AltiumFileKind::PcbDocument,
            AltiumFileKind::SchDocument,
        ];
        for kind in kinds {
            assert_eq!(AltiumFileKind::from_extension(kind.extension()), Some(kind));
        }
    }

    #[test]
    fn library_vs_document_predicates() {
        assert!(AltiumFileKind::PcbLibrary.is_library());
        assert!(AltiumFileKind::SchLibrary.is_library());
        assert!(!AltiumFileKind::PcbDocument.is_library());
        assert!(AltiumFileKind::PcbDocument.is_document());
        assert!(AltiumFileKind::SchDocument.is_document());
        assert!(!AltiumFileKind::SchLibrary.is_document());
    }

    #[test]
    fn from_bytes_round_trip_pcblib() {
        let mut lib = pcb::Library::default();
        lib.unique_id = "ABCD1234".into();
        lib.components.push(pcb::Component::new("R1"));
        let bytes = lib.to_bytes().unwrap();

        let file = AltiumFile::from_bytes(AltiumFileKind::PcbLibrary, bytes).unwrap();
        assert_eq!(file.kind(), AltiumFileKind::PcbLibrary);
        let lib = file.as_pcb_library().expect("PcbLibrary variant");
        assert_eq!(lib.unique_id, "ABCD1234");
        assert_eq!(lib.components.len(), 1);
    }

    #[test]
    fn from_bytes_round_trip_schlib() {
        let mut lib = sch::Library::default();
        lib.components.push(sch::Component::new("U1"));
        let bytes = lib.to_bytes().unwrap();

        let file = AltiumFile::from_bytes(AltiumFileKind::SchLibrary, bytes).unwrap();
        assert_eq!(file.kind(), AltiumFileKind::SchLibrary);
        assert_eq!(file.as_sch_library().unwrap().components.len(), 1);
    }

    #[test]
    fn from_bytes_round_trip_intlib() {
        use crate::IntegratedLibrary;
        let mut sch_lib = sch::Library::default();
        sch_lib.components.push(sch::Component::new("U1"));
        let mut pcb_lib = pcb::Library::default();
        pcb_lib.unique_id = "INTLIB01".into();
        pcb_lib.components.push(pcb::Component::new("R0402"));

        let mut intlib = IntegratedLibrary::default();
        intlib.schematic_libraries.push(super::super::intlib::NamedLibrary {
            name: "Parts.SchLib".into(),
            library: sch_lib,
        });
        intlib.footprint_libraries.push(super::super::intlib::NamedLibrary {
            name: "Parts.PcbLib".into(),
            library: pcb_lib,
        });
        let bytes = intlib.to_bytes().unwrap();

        let file = AltiumFile::from_bytes(AltiumFileKind::IntegratedLibrary, bytes).unwrap();
        assert_eq!(file.kind(), AltiumFileKind::IntegratedLibrary);
        let intlib = file.as_integrated_library().expect("IntegratedLibrary variant");
        assert_eq!(intlib.schematic_libraries.len(), 1);
        assert_eq!(intlib.footprint_libraries.len(), 1);
    }

    #[test]
    fn intlib_extension_dispatch() {
        use std::path::PathBuf;
        for ext in ["IntLib", ".IntLib", "INTLIB", "intlib"] {
            assert_eq!(
                AltiumFileKind::from_extension(ext),
                Some(AltiumFileKind::IntegratedLibrary)
            );
        }
        assert_eq!(
            AltiumFileKind::from_path(PathBuf::from("Parts.IntLib")),
            Some(AltiumFileKind::IntegratedLibrary)
        );
        assert_eq!(AltiumFileKind::IntegratedLibrary.extension(), ".IntLib");
        assert!(AltiumFileKind::IntegratedLibrary.is_library());
        assert!(!AltiumFileKind::IntegratedLibrary.is_document());
    }

    #[tokio::test]
    async fn read_intlib_from_disk_round_trips_through_cli_dispatch() {
        // Build a synthetic IntLib, write it to a temp path, and verify
        // `AltiumFile::read` (which dispatches via path extension) brings
        // back the IntegratedLibrary variant.
        use crate::IntegratedLibrary;
        let nonce = std::process::id();
        let path = std::env::temp_dir().join(format!("altium-rs-IntLib-{nonce}.IntLib"));
        let _ = std::fs::remove_file(&path);

        let mut sch_lib = sch::Library::default();
        sch_lib.components.push(sch::Component::new("U1"));
        let mut intlib = IntegratedLibrary::default();
        intlib.schematic_libraries.push(super::super::intlib::NamedLibrary {
            name: "Parts.SchLib".into(),
            library: sch_lib,
        });
        intlib.write(&path).await.unwrap();

        let file = open(&path).await.unwrap();
        assert_eq!(file.kind(), AltiumFileKind::IntegratedLibrary);
        let intlib = file.as_integrated_library().unwrap();
        assert_eq!(intlib.schematic_libraries.len(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn from_for_each_variant() {
        let pl: pcb::Library = Default::default();
        let f: AltiumFile = pl.into();
        assert!(f.as_pcb_library().is_some());
        let sl: sch::Library = Default::default();
        let f: AltiumFile = sl.into();
        assert!(f.as_sch_library().is_some());
        let pd: pcb::Document = Default::default();
        let f: AltiumFile = pd.into();
        assert!(f.as_pcb_document().is_some());
        let sd: sch::Document = Default::default();
        let f: AltiumFile = sd.into();
        assert!(f.as_sch_document().is_some());
    }

    #[test]
    fn into_returns_inner_for_matching_variant() {
        let lib = pcb::Library::default();
        let f = AltiumFile::PcbLibrary(lib);
        assert!(f.into_pcb_library().is_some());

        let doc = pcb::Document::default();
        let f = AltiumFile::PcbDocument(doc);
        assert!(f.into_sch_library().is_none());
    }

    #[tokio::test]
    async fn read_unrecognised_extension_errors() {
        let p = PathBuf::from("/tmp/no-such.unknownext");
        let err = AltiumFile::read(&p).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unrecognised"), "got: {msg}");
    }

    #[tokio::test]
    async fn read_then_write_round_trips_pcblib() {
        let nonce = std::process::id();
        let path = std::env::temp_dir().join(format!("altium-rs-Round-{nonce}.PcbLib"));
        let out = std::env::temp_dir().join(format!("altium-rs-Round2-{nonce}.PcbLib"));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&out);

        let mut lib = pcb::Library::default();
        lib.unique_id = "ROUND001".into();
        lib.components.push(pcb::Component::new("Foo"));
        tokio::fs::write(&path, lib.to_bytes().unwrap())
            .await
            .unwrap();

        let file = open(&path).await.unwrap();
        assert_eq!(file.kind(), AltiumFileKind::PcbLibrary);
        let lib2 = file.as_pcb_library().unwrap();
        assert_eq!(lib2.unique_id, "ROUND001");
        assert_eq!(lib2.components.len(), 1);

        file.write(&out).await.unwrap();
        let reread = open(&out).await.unwrap();
        assert_eq!(reread.as_pcb_library().unwrap().unique_id, "ROUND001");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&out);
    }
}
