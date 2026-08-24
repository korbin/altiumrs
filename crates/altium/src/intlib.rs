//! Integrated library (`.IntLib`) reader/writer.
//!
//! Layout of an Altium-emitted `.IntLib` (verified against fixtures from
//! Altium Designer Vault exports):
//!
//! ```text
//! root/
//!   SchLib/
//!     0.schlib                  -- [u8 flag=0x02][zlib bytes of inner SchLib CFB]
//!     1.schlib                  -- if more than one source SchLib was compiled
//!   PCBLib/
//!     0.pcblib                  -- [u8 flag=0x02][zlib bytes of inner PcbLib CFB]
//!   Version.Txt                 -- [u8 0x00][i32 LE version, typically 2]
//!   Parameters   .bin           -- [u8 0x00][repeating [u32 LE len][len bytes]]
//!                                  one block per component (symbol + footprints).
//!                                  Note the trailing spaces in the stream name!
//!   LibCrossRef.Txt             -- [u8 0x00][repeating [u32 LE len][len bytes]]
//!                                  cross-reference between symbols and footprints.
//! ```
//!
//! `Version.Txt` is decoded as an integer; `Parameters   .bin` and
//! `LibCrossRef.Txt` are stored as raw bytes for round-trip with typed
//! decoders ([`parameters_blocks`](IntegratedLibrary::parameters_blocks),
//! [`cross_reference_records`](IntegratedLibrary::cross_reference_records))
//! exposed as accessors on `IntegratedLibrary`. The bundled `.SchLib` /
//! `.PcbLib` files round-trip through their own typed parsers.
//!
//! For older or hand-rolled IntLibs that don't follow this layout (e.g. a
//! single root-level CFB-shaped stream), we fall back to a magic-byte sniff:
//! any stream whose bytes start with the OLE compound-file signature is
//! tried as both a SchLib and a PcbLib.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::path::Path;

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::compound::CompoundFile;
use crate::diagnostic::Diagnostic;
use crate::error::{Error, Result};
use crate::{pcb, sch};

/// First eight bytes of every OLE compound file. Used as a fallback magic-byte
/// sniff for ad-hoc IntLib layouts that don't follow Altium's standard.
const CFB_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Flag byte that prefixes zlib-compressed library streams (`0.schlib` /
/// `0.pcblib`) inside the `SchLib/` / `PCBLib/` storages.
const FLAG_ZLIB: u8 = 0x02;

/// Default IntLib version emitted by recent Altium Designer.
const DEFAULT_VERSION: i32 = 2;

/// Storage path that holds compiled schematic libraries.
const SCHLIB_STORAGE: &str = "SchLib";
/// Storage path that holds compiled footprint libraries.
const PCBLIB_STORAGE: &str = "PCBLib";

/// Stream name for the compiled-library version metadata.
const VERSION_STREAM: &str = "Version.Txt";
/// Stream name for the binary parameters file. The trailing spaces are part
/// of the actual on-disk name and must be preserved.
const PARAMETERS_STREAM: &str = "Parameters   .bin";
/// Stream name for the cross-reference metadata.
const CROSSREF_STREAM: &str = "LibCrossRef.Txt";

/// An embedded library named by its stream name inside the outer IntLib.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct NamedLibrary<T> {
    /// Stream name without the leading storage component (e.g. `"0.schlib"`).
    /// On a fresh-from-Altium IntLib the value reflects the storage slot,
    /// not the original source filename — that information lives in
    /// [`IntegratedLibrary::cross_reference`] and isn't decoded yet.
    pub name: String,
    pub library: T,
}

/// Compiled integrated library. Open with
/// [`from_bytes`](Self::from_bytes) / [`read`](Self::read), inspect the
/// embedded [`schematic_libraries`](Self::schematic_libraries) and
/// [`footprint_libraries`](Self::footprint_libraries), write back via
/// [`to_bytes`](Self::to_bytes) / [`write`](Self::write).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IntegratedLibrary {
    /// Diagnostics collected during parsing.
    pub diagnostics: Vec<Diagnostic>,
    /// Version stored in `Version.Txt`. Defaults to [`DEFAULT_VERSION`]
    /// (`2`) when authored from scratch.
    pub version: i32,
    /// `.SchLib` files embedded in the container.
    pub schematic_libraries: Vec<NamedLibrary<sch::Library>>,
    /// `.PcbLib` files embedded in the container.
    pub footprint_libraries: Vec<NamedLibrary<pcb::Library>>,
    /// Raw bytes of `LibCrossRef.Txt`. Preserved verbatim for round-trip.
    #[cfg_attr(feature = "serde", serde(default, with = "crate::serde_bytes::b64_opt"))]
    pub cross_reference: Option<Vec<u8>>,
    /// Raw bytes of `Parameters   .bin`. Preserved verbatim for round-trip.
    #[cfg_attr(feature = "serde", serde(default, with = "crate::serde_bytes::b64_opt"))]
    pub parameters_bin: Option<Vec<u8>>,
    /// Streams not handled by any typed field above. Datasheets, simulation
    /// models, or unrecognised layout variants land here under their full
    /// stream path.
    #[cfg_attr(feature = "serde", serde(default, with = "crate::serde_bytes::b64_map"))]
    pub additional_files: BTreeMap<String, Vec<u8>>,
    /// Parameter block from a root-level `FileHeader` stream, when the
    /// IntLib carries one (rare; produced by some non-Altium tooling).
    pub manifest: BTreeMap<String, String>,
}

impl Default for IntegratedLibrary {
    fn default() -> Self {
        Self {
            diagnostics: Vec::new(),
            version: DEFAULT_VERSION,
            schematic_libraries: Vec::new(),
            footprint_libraries: Vec::new(),
            cross_reference: None,
            parameters_bin: None,
            additional_files: BTreeMap::new(),
            manifest: BTreeMap::new(),
        }
    }
}

impl IntegratedLibrary {
    /// Parse a `.IntLib` byte buffer.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut cf = CompoundFile::open(bytes)?;
        let mut intlib = Self::default();
        let mut consumed: Vec<String> = Vec::new();

        // Version.Txt — `[u8 leading=00][i32 LE version]`
        if let Some(data) = try_read_stream(&mut cf, VERSION_STREAM)? {
            consumed.push(format!("/{VERSION_STREAM}"));
            if data.len() >= 5 {
                intlib.version =
                    i32::from_le_bytes([data[1], data[2], data[3], data[4]]);
            } else {
                intlib.diagnostics.push(Diagnostic::warning_in(
                    format!("Version.Txt is only {} bytes; expected 5", data.len()),
                    VERSION_STREAM,
                ));
            }
        }

        if let Some(data) = try_read_stream(&mut cf, CROSSREF_STREAM)? {
            consumed.push(format!("/{CROSSREF_STREAM}"));
            intlib.cross_reference = Some(data);
        }
        if let Some(data) = try_read_stream(&mut cf, PARAMETERS_STREAM)? {
            consumed.push(format!("/{PARAMETERS_STREAM}"));
            intlib.parameters_bin = Some(data);
        }

        // Older / non-standard IntLibs sometimes carry a root FileHeader
        // parameter block. We support that for forward compatibility.
        if let Some(data) = try_read_stream(&mut cf, "FileHeader")? {
            consumed.push("/FileHeader".to_string());
            if let Ok(map) = parse_file_header_block(&data) {
                intlib.manifest = map;
            } else {
                intlib
                    .additional_files
                    .insert("/FileHeader".to_string(), data);
            }
        }

        // Standard layout: SchLib/ and PCBLib/ storages with numbered streams.
        read_storage_libraries(
            &mut cf,
            SCHLIB_STORAGE,
            &mut intlib.schematic_libraries,
            &mut intlib.diagnostics,
            &mut consumed,
            |bytes| sch::Library::from_bytes(bytes).map(|l| Box::new(l) as Box<dyn std::any::Any>),
        )?;
        read_storage_libraries(
            &mut cf,
            PCBLIB_STORAGE,
            &mut intlib.footprint_libraries,
            &mut intlib.diagnostics,
            &mut consumed,
            |bytes| pcb::Library::from_bytes(bytes).map(|l| Box::new(l) as Box<dyn std::any::Any>),
        )?;

        // Walk anything else; fall back to magic-byte sniffing so older IntLib
        // shapes still surface their libraries.
        let consumed_set: std::collections::HashSet<&str> =
            consumed.iter().map(String::as_str).collect();
        let mut to_visit: Vec<String> = vec!["/".into()];
        while let Some(current) = to_visit.pop() {
            let entries = match cf.list_children(&current) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for entry in entries {
                let child_path = if current == "/" {
                    format!("/{}", entry.name)
                } else {
                    format!("{}/{}", current, entry.name)
                };
                if entry.is_storage {
                    // The standard storages are already drained; descend into
                    // anything else for the heuristic sweep.
                    if !entry.name.eq_ignore_ascii_case(SCHLIB_STORAGE)
                        && !entry.name.eq_ignore_ascii_case(PCBLIB_STORAGE)
                    {
                        to_visit.push(child_path.clone());
                    }
                    continue;
                }
                if !entry.is_stream || consumed_set.contains(child_path.as_str()) {
                    continue;
                }
                let bytes = match cf.read_stream(&child_path) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                heuristic_classify(&child_path, bytes, &mut intlib);
            }
        }

        Ok(intlib)
    }

    /// Async read from disk.
    pub async fn read(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = tokio::fs::read(path).await?;
        Self::from_bytes(bytes)
    }

    /// Async read from any [`AsyncRead`].
    pub async fn read_async<R>(mut reader: R) -> Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Self::from_bytes(bytes)
    }

    /// Serialise back to a `.IntLib` byte buffer.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut cf = CompoundFile::create()?;

        // Version.Txt
        let version = if self.version <= 0 {
            DEFAULT_VERSION
        } else {
            self.version
        };
        let mut version_buf = Vec::with_capacity(5);
        version_buf.push(0x00);
        version_buf.extend_from_slice(&version.to_le_bytes());
        cf.write_stream(VERSION_STREAM, &version_buf)?;

        if let Some(data) = &self.cross_reference {
            cf.write_stream(CROSSREF_STREAM, data)?;
        }
        if let Some(data) = &self.parameters_bin {
            cf.write_stream(PARAMETERS_STREAM, data)?;
        }

        if !self.manifest.is_empty() {
            cf.write_stream("FileHeader", &serialise_file_header_block(&self.manifest)?)?;
        }

        if !self.schematic_libraries.is_empty() {
            cf.create_storage(SCHLIB_STORAGE)?;
            for (i, entry) in self.schematic_libraries.iter().enumerate() {
                let inner = entry.library.to_bytes()?;
                let stream_name = numbered_stream_name(&entry.name, i, "schlib");
                let path = format!("{SCHLIB_STORAGE}/{stream_name}");
                cf.write_stream(&path, &compress_library_bytes(&inner)?)?;
            }
        }
        if !self.footprint_libraries.is_empty() {
            cf.create_storage(PCBLIB_STORAGE)?;
            for (i, entry) in self.footprint_libraries.iter().enumerate() {
                let inner = entry.library.to_bytes()?;
                let stream_name = numbered_stream_name(&entry.name, i, "pcblib");
                let path = format!("{PCBLIB_STORAGE}/{stream_name}");
                cf.write_stream(&path, &compress_library_bytes(&inner)?)?;
            }
        }

        for (path, data) in &self.additional_files {
            let normalised = path.trim_start_matches('/');
            cf.write_stream(normalised, data)?;
        }

        cf.into_bytes()
    }

    /// Async write to disk.
    pub async fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = self.to_bytes()?;
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    /// Async write to any [`AsyncWrite`].
    pub async fn write_async<W>(&self, mut writer: W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let bytes = self.to_bytes()?;
        writer.write_all(&bytes).await?;
        writer.flush().await?;
        Ok(())
    }

    /// Find an embedded SchLib by its name (case-insensitive).
    pub fn schematic_library(&self, name: &str) -> Option<&sch::Library> {
        self.schematic_libraries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
            .map(|e| &e.library)
    }

    /// Find an embedded PcbLib by its name (case-insensitive).
    pub fn footprint_library(&self, name: &str) -> Option<&pcb::Library> {
        self.footprint_libraries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
            .map(|e| &e.library)
    }

    /// Decode the raw [`parameters_bin`](Self::parameters_bin) bytes into a
    /// list of `|`-delimited parameter strings, one per component (one symbol
    /// block followed by N footprint blocks). Trailing NULs are stripped.
    pub fn parameters_blocks(&self) -> Result<Vec<String>> {
        match &self.parameters_bin {
            Some(bytes) => parse_parameters_bin(bytes),
            None => Ok(Vec::new()),
        }
    }

    /// Replace [`parameters_bin`](Self::parameters_bin) with a freshly
    /// serialised buffer built from `blocks`.
    pub fn set_parameters_blocks(&mut self, blocks: &[String]) {
        self.parameters_bin = Some(serialise_parameters_bin(blocks));
    }

    /// Decode the raw [`cross_reference`](Self::cross_reference) bytes into a
    /// flat token stream of [`CrossRefRecord`]s. Tokens preserve order; the
    /// caller can re-group them or just inspect them.
    pub fn cross_reference_records(&self) -> Result<Vec<CrossRefRecord>> {
        match &self.cross_reference {
            Some(bytes) => parse_cross_reference(bytes),
            None => Ok(Vec::new()),
        }
    }

    /// Replace [`cross_reference`](Self::cross_reference) with a freshly
    /// serialised buffer built from `records`.
    pub fn set_cross_reference_records(&mut self, records: &[CrossRefRecord]) {
        self.cross_reference = Some(serialise_cross_reference(records));
    }

    /// Decode the raw [`cross_reference`](Self::cross_reference) bytes into a
    /// structured [`CrossReferenceTable`] — one entry per symbol with each
    /// symbol's description, source paths, and footprint variants linked up
    /// as typed fields.
    ///
    /// Returns `Ok(default)` when `cross_reference` is `None`.
    pub fn cross_reference_table(&self) -> Result<CrossReferenceTable> {
        let records = self.cross_reference_records()?;
        if records.is_empty() {
            return Ok(CrossReferenceTable::default());
        }
        parse_cross_reference_table(&records)
    }

    /// Replace [`cross_reference`](Self::cross_reference) with bytes
    /// regenerated from the typed [`CrossReferenceTable`]. Inverse of
    /// [`cross_reference_table`](Self::cross_reference_table).
    pub fn set_cross_reference_table(&mut self, table: &CrossReferenceTable) {
        let records = flatten_cross_reference_table(table);
        self.set_cross_reference_records(&records);
    }
}

/// One token of a parsed `LibCrossRef.Txt` stream. The on-disk format mixes
/// bare 32-bit tags (kind/count fields) and length-prefixed pascal strings;
/// callers can scan a `Vec<CrossRefRecord>` and group records by
/// tag-then-N-strings based on their own conventions.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CrossRefRecord {
    /// A 32-bit tag / count value with no associated payload.
    Tag(u32),
    /// A length-prefixed pascal string. Stored as a Rust `String`; the
    /// on-disk encoding is windows-1252 but every value we've seen is pure
    /// ASCII (file paths, component names, library kinds).
    String(String),
}

// ─── Parameters   .bin codec ────────────────────────────────────────────────

/// Parse the body of `Parameters   .bin`.
///
/// Layout: `[u8 leading][repeating [u32 LE len][len bytes]]`. Each payload is
/// a `|`-delimited parameter string ending in a NUL byte. We strip the NUL on
/// the way out so the resulting strings are clean Rust text.
pub fn parse_parameters_bin(data: &[u8]) -> Result<Vec<String>> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    // The leading byte is some kind of file-format flag (always `0x00` in
    // every fixture we've seen). Tolerate other values rather than erroring —
    // round-trip stays the same as long as we re-emit the same byte.
    let mut i = 1usize;
    let mut out = Vec::new();
    while i + 4 <= data.len() {
        let len = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        i += 4;
        if i + len > data.len() {
            return Err(Error::corrupt(format!(
                "Parameters .bin block at offset {}: length {} overruns buffer ({})",
                i - 4,
                len,
                data.len()
            )));
        }
        let mut payload = data[i..i + len].to_vec();
        if payload.last() == Some(&0) {
            payload.pop();
        }
        out.push(crate::encoding::decode(&payload));
        i += len;
    }
    Ok(out)
}

/// Inverse of [`parse_parameters_bin`]. Re-emits each block as
/// `[u32 LE len][payload bytes][0x00]` and prepends the leading `0x00`
/// flag byte that Altium writes.
pub fn serialise_parameters_bin(blocks: &[String]) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(1 + blocks.iter().map(|s| 5 + s.len()).sum::<usize>());
    out.push(0x00);
    for block in blocks {
        let mut payload = crate::encoding::encode(block);
        payload.push(0);
        let len = payload.len() as u32;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&payload);
    }
    out
}

// ─── LibCrossRef.Txt codec ──────────────────────────────────────────────────

/// Parse the body of `LibCrossRef.Txt`.
///
/// Layout: `[u8 leading][repeating record]` where each record is either a
/// 4-byte u32 tag, or a pascal-string block:
///
/// ```text
/// pascal_string_block := [u32 LE outer_size][u8 pascal_len][pascal_len bytes]
/// outer_size = pascal_len + 1
/// ```
///
/// We disambiguate by trying the pascal-string-block shape first: if the next
/// u32 satisfies `next_byte == u32 - 1` and the implied payload fits and is
/// printable ASCII, we treat it as a string. Otherwise it's a bare tag.
pub fn parse_cross_reference(data: &[u8]) -> Result<Vec<CrossRefRecord>> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let mut i = 1usize; // skip leading byte
    let mut out = Vec::new();
    while i + 4 <= data.len() {
        let outer = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        if outer >= 1 && (outer as usize) <= 1024 && i + 4 < data.len() {
            let pascal_len = data[i + 4] as u32;
            if pascal_len + 1 == outer && i + 4 + 1 + pascal_len as usize <= data.len() {
                let payload = &data[i + 5..i + 5 + pascal_len as usize];
                if is_mostly_printable(payload) {
                    out.push(CrossRefRecord::String(crate::encoding::decode(payload)));
                    i += 4 + 1 + pascal_len as usize;
                    continue;
                }
            }
        }
        out.push(CrossRefRecord::Tag(outer));
        i += 4;
    }
    Ok(out)
}

// ─── Higher-level cross-reference table ────────────────────────────────────
//
// Above the flat token stream sits a structured "cross-reference table" that
// the IntLib uses to map every symbol to its description, source SchLib
// filename, and footprint variants — each annotated with both the internal
// IntLib path (e.g. `:\PCBLib\0.pcblib`) and the original source filename.
//
// On-disk grammar (verified against an Altium-emitted fixture):
//
// ```text
// table   := symbol*
// symbol  := Tag(1) <libref> <internal_schlib_path>
//            Tag(1) <description> <source_schlib_path>
//            Tag(4) footprint+
// footprint := <name> <kind>                   # kind = "PCBLIB"
//              Tag(1) <internal_pcblib_path> <source_pcblib_path>
// ```
//
// `<…>` denotes a [`CrossRefRecord::String`]. The grammar may need to expand
// for multi-symbol IntLibs once we have such a fixture; the current decoder
// errors out cleanly if the bytes don't match.

/// Structured form of a `LibCrossRef.Txt` stream — one entry per symbol.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CrossReferenceTable {
    pub symbols: Vec<CrossReferenceSymbol>,
}

/// One symbol's cross-reference: where it lives inside the IntLib, where its
/// source SchLib came from, and which footprints are linked to it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CrossReferenceSymbol {
    /// Symbol library reference (e.g. `"CMP-04913-000051-1"`).
    pub libref: String,
    /// Path inside the IntLib's CFB tree (e.g. `":\\SchLib\\0.schlib"`).
    pub internal_schlib_path: String,
    /// Human-readable description (e.g. `"IC PWR MGMT BATTERY MGMT"`).
    pub description: String,
    /// Original source SchLib filename, often a UNC path.
    pub source_schlib_path: String,
    /// Footprint variants associated with this symbol.
    pub footprints: Vec<CrossReferenceFootprint>,
}

/// One footprint variant for a symbol.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CrossReferenceFootprint {
    /// Footprint name (e.g. `"FP-RGE0024H-IPC_A"`).
    pub name: String,
    /// Kind tag — `"PCBLIB"` in every fixture we've seen.
    pub kind: String,
    /// Path inside the IntLib's CFB tree (e.g. `":\\PCBLib\\0.pcblib"`).
    pub internal_pcblib_path: String,
    /// Original source PcbLib filename, often a UNC path.
    pub source_pcblib_path: String,
}

/// Decode a token stream into the structured cross-reference table. Errors if
/// the tokens don't match the expected grammar.
pub fn parse_cross_reference_table(tokens: &[CrossRefRecord]) -> Result<CrossReferenceTable> {
    let mut symbols = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let (sym, consumed) = parse_one_symbol(&tokens[i..])?;
        symbols.push(sym);
        i += consumed;
    }
    Ok(CrossReferenceTable { symbols })
}

/// Inverse of [`parse_cross_reference_table`]. Flattens the typed records
/// back to a token stream that [`serialise_cross_reference`] can encode.
pub fn flatten_cross_reference_table(table: &CrossReferenceTable) -> Vec<CrossRefRecord> {
    let mut out = Vec::with_capacity(table.symbols.len() * 8);
    for sym in &table.symbols {
        out.push(CrossRefRecord::Tag(1));
        out.push(CrossRefRecord::String(sym.libref.clone()));
        out.push(CrossRefRecord::String(sym.internal_schlib_path.clone()));
        out.push(CrossRefRecord::Tag(1));
        out.push(CrossRefRecord::String(sym.description.clone()));
        out.push(CrossRefRecord::String(sym.source_schlib_path.clone()));
        out.push(CrossRefRecord::Tag(4));
        for fp in &sym.footprints {
            out.push(CrossRefRecord::String(fp.name.clone()));
            out.push(CrossRefRecord::String(fp.kind.clone()));
            out.push(CrossRefRecord::Tag(1));
            out.push(CrossRefRecord::String(fp.internal_pcblib_path.clone()));
            out.push(CrossRefRecord::String(fp.source_pcblib_path.clone()));
        }
    }
    out
}

fn parse_one_symbol(toks: &[CrossRefRecord]) -> Result<(CrossReferenceSymbol, usize)> {
    let take_pair_with_tag = |start: usize, tag: u32| -> Result<(String, String, usize)> {
        if start + 3 > toks.len() {
            return Err(Error::corrupt(format!(
                "LibCrossRef: expected Tag({tag})+String+String at offset {start} but stream ended",
            )));
        }
        match (&toks[start], &toks[start + 1], &toks[start + 2]) {
            (CrossRefRecord::Tag(t), CrossRefRecord::String(a), CrossRefRecord::String(b))
                if *t == tag =>
            {
                Ok((a.clone(), b.clone(), start + 3))
            }
            other => Err(Error::corrupt(format!(
                "LibCrossRef: expected Tag({tag})+String+String at offset {start}, got {other:?}",
            ))),
        }
    };

    let (libref, internal_schlib, mut i) = take_pair_with_tag(0, 1)?;
    let (description, source_schlib, mut j) = take_pair_with_tag(i, 1)?;
    i = j;

    if i >= toks.len() {
        return Err(Error::corrupt(
            "LibCrossRef: symbol ended before footprint marker (Tag(4))",
        ));
    }
    match &toks[i] {
        CrossRefRecord::Tag(4) => i += 1,
        other => {
            return Err(Error::corrupt(format!(
                "LibCrossRef: expected Tag(4) at offset {i}, got {other:?}",
            )));
        }
    }

    let mut footprints = Vec::new();
    while i + 5 <= toks.len() {
        // Footprint = String String Tag(1) String String. Anything else means
        // we've reached the next symbol (or the end).
        let pattern = (
            &toks[i],
            &toks[i + 1],
            &toks[i + 2],
            &toks[i + 3],
            &toks[i + 4],
        );
        let fp = match pattern {
            (
                CrossRefRecord::String(name),
                CrossRefRecord::String(kind),
                CrossRefRecord::Tag(1),
                CrossRefRecord::String(internal),
                CrossRefRecord::String(source),
            ) => CrossReferenceFootprint {
                name: name.clone(),
                kind: kind.clone(),
                internal_pcblib_path: internal.clone(),
                source_pcblib_path: source.clone(),
            },
            _ => break,
        };
        footprints.push(fp);
        i += 5;
    }
    j = i;

    Ok((
        CrossReferenceSymbol {
            libref,
            internal_schlib_path: internal_schlib,
            description,
            source_schlib_path: source_schlib,
            footprints,
        },
        j,
    ))
}

/// Inverse of [`parse_cross_reference`].
pub fn serialise_cross_reference(records: &[CrossRefRecord]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + records.len() * 8);
    out.push(0x00);
    for record in records {
        match record {
            CrossRefRecord::Tag(value) => {
                out.extend_from_slice(&value.to_le_bytes());
            }
            CrossRefRecord::String(text) => {
                let bytes = crate::encoding::encode(text);
                let pascal_len = bytes.len();
                if pascal_len > u8::MAX as usize {
                    // The on-disk format only allocates a u8 pascal length;
                    // truncate strings longer than 255 bytes. In practice we
                    // never see them — every fixture string is < 100 chars.
                    let truncated = &bytes[..u8::MAX as usize];
                    let outer = (truncated.len() as u32) + 1;
                    out.extend_from_slice(&outer.to_le_bytes());
                    out.push(truncated.len() as u8);
                    out.extend_from_slice(truncated);
                } else {
                    let outer = (pascal_len as u32) + 1;
                    out.extend_from_slice(&outer.to_le_bytes());
                    out.push(pascal_len as u8);
                    out.extend_from_slice(&bytes);
                }
            }
        }
    }
    out
}

/// True if every byte is either printable ASCII (`0x20..=0x7E`) or NUL.
/// LibCrossRef strings are file paths and identifiers; tag values that
/// happen to look like length-prefixes are filtered out by this guard.
fn is_mostly_printable(payload: &[u8]) -> bool {
    payload
        .iter()
        .all(|&c| (0x20..=0x7E).contains(&c) || c == 0)
}

fn try_read_stream(cf: &mut CompoundFile, name: &str) -> Result<Option<Vec<u8>>> {
    cf.try_read_stream(name)
}

/// Walk `storage_name` looking for streams named `*.{schlib,pcblib}`. For each
/// hit, strip the flag byte, zlib-decompress, and parse via `parse`.
fn read_storage_libraries<L>(
    cf: &mut CompoundFile,
    storage_name: &str,
    out: &mut Vec<NamedLibrary<L>>,
    diagnostics: &mut Vec<Diagnostic>,
    consumed: &mut Vec<String>,
    parse: impl Fn(Vec<u8>) -> Result<Box<dyn std::any::Any>>,
) -> Result<()>
where
    L: 'static,
{
    if !cf.is_storage(storage_name) {
        return Ok(());
    }
    let entries = cf.list_children(storage_name)?;
    let expected_ext = if storage_name.eq_ignore_ascii_case(SCHLIB_STORAGE) {
        "schlib"
    } else {
        "pcblib"
    };
    for entry in entries {
        let path = format!("/{}/{}", storage_name, entry.name);
        if !entry.is_stream {
            continue;
        }
        // Streams use lowercase extensions. Be tolerant on read.
        if !entry.name.to_ascii_lowercase().ends_with(expected_ext) {
            continue;
        }
        consumed.push(path.clone());
        let bytes = cf.read_stream(format!("{storage_name}/{}", entry.name))?;
        let decompressed = match decompress_library_bytes(&bytes) {
            Ok(v) => v,
            Err(e) => {
                diagnostics.push(Diagnostic::warning_in(
                    format!("failed to decompress library stream: {e}"),
                    path,
                ));
                continue;
            }
        };
        match parse(decompressed) {
            Ok(any) => {
                let lib = match any.downcast::<L>() {
                    Ok(b) => *b,
                    Err(_) => {
                        diagnostics.push(Diagnostic::warning_in(
                            "downcast failed after parse",
                            path,
                        ));
                        continue;
                    }
                };
                out.push(NamedLibrary {
                    name: entry.name.clone(),
                    library: lib,
                });
            }
            Err(e) => {
                diagnostics.push(Diagnostic::warning_in(
                    format!("library parse failed: {e}"),
                    path,
                ));
            }
        }
    }
    Ok(())
}

fn decompress_library_bytes(data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Err(Error::corrupt("empty library stream"));
    }
    let flag = data[0];
    if flag != FLAG_ZLIB {
        return Err(Error::corrupt(format!(
            "unsupported library compression flag {flag:#x} (expected {FLAG_ZLIB:#x})"
        )));
    }
    use std::io::Read;
    let mut decoder = ZlibDecoder::new(&data[1..]);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| Error::corrupt(format!("zlib decompression failed: {e}")))?;
    Ok(out)
}

fn compress_library_bytes(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() / 4 + 16);
    out.push(FLAG_ZLIB);
    let mut encoder = ZlibEncoder::new(&mut out, Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| Error::corrupt(format!("zlib compression failed: {e}")))?;
    encoder
        .finish()
        .map_err(|e| Error::corrupt(format!("zlib finish failed: {e}")))?;
    Ok(out)
}

fn numbered_stream_name(existing: &str, fallback_index: usize, ext: &str) -> String {
    // Only keep `existing` if it's already canonical — `<integer>.<ext>` —
    // otherwise generate a numbered name from the slot index so Altium reads
    // it back. Names like `"Parts.SchLib"` round-trip as `"0.schlib"`.
    if !existing.is_empty() {
        let lower = existing.to_ascii_lowercase();
        let suffix = format!(".{ext}");
        if let Some(stem) = lower.strip_suffix(&suffix) {
            if !stem.is_empty() && stem.chars().all(|c| c.is_ascii_digit()) {
                return existing.to_string();
            }
        }
    }
    format!("{fallback_index}.{ext}")
}

fn parse_file_header_block(data: &[u8]) -> Result<BTreeMap<String, String>> {
    use crate::binary::BinaryReader;
    use crate::parameter::ParameterMap;
    let mut br = BinaryReader::new(Cursor::new(data.to_vec()))?;
    let len = br.read_i32()?;
    if len <= 0 {
        return Ok(BTreeMap::new());
    }
    let mut bytes = vec![0u8; len as usize];
    br.read_exact(&mut bytes)?;
    if bytes.last() == Some(&0) {
        bytes.pop();
    }
    let text = crate::encoding::decode(&bytes);
    let params = ParameterMap::parse(&text);
    let mut out = BTreeMap::new();
    for (name, value, _) in params.iter() {
        out.insert(name.to_string(), value.to_string());
    }
    Ok(out)
}

fn serialise_file_header_block(manifest: &BTreeMap<String, String>) -> Result<Vec<u8>> {
    use crate::binary::BinaryWriter;
    let mut text = String::new();
    let mut first = true;
    for (k, v) in manifest {
        if !first {
            text.push('|');
        }
        first = false;
        text.push_str(k);
        text.push('=');
        text.push_str(v);
    }
    let mut bytes = crate::encoding::encode(&text);
    bytes.push(0);
    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut bw = BinaryWriter::new(&mut buf);
    bw.write_i32(bytes.len() as i32)?;
    bw.write_bytes(&bytes)?;
    Ok(buf.into_inner())
}

fn heuristic_classify(path: &str, bytes: Vec<u8>, out: &mut IntegratedLibrary) {
    if bytes.len() < CFB_MAGIC.len() || bytes[..CFB_MAGIC.len()] != CFB_MAGIC {
        out.additional_files.insert(path.to_string(), bytes);
        return;
    }
    // Try PCB first, then Sch — both keep the bytes if the parse fails so the
    // stream still survives round-trip through `additional_files`.
    let lower = path.to_ascii_lowercase();
    let pcb_first = lower.contains("pcb") || !lower.contains("sch");
    type Attempt = Box<dyn Fn(Vec<u8>) -> Option<DetectedLibrary>>;
    let attempts: [Attempt; 2] = if pcb_first {
        [
            Box::new(|b| pcb::Library::from_bytes(b).ok().map(DetectedLibrary::Pcb)),
            Box::new(|b| sch::Library::from_bytes(b).ok().map(DetectedLibrary::Sch)),
        ]
    } else {
        [
            Box::new(|b| sch::Library::from_bytes(b).ok().map(DetectedLibrary::Sch)),
            Box::new(|b| pcb::Library::from_bytes(b).ok().map(DetectedLibrary::Pcb)),
        ]
    };
    let trimmed_name = path.trim_start_matches('/').to_string();
    for attempt in attempts {
        if let Some(detected) = attempt(bytes.clone()) {
            match detected {
                DetectedLibrary::Sch(lib) => out.schematic_libraries.push(NamedLibrary {
                    name: trimmed_name,
                    library: lib,
                }),
                DetectedLibrary::Pcb(lib) => out.footprint_libraries.push(NamedLibrary {
                    name: trimmed_name,
                    library: lib,
                }),
            }
            return;
        }
    }
    out.additional_files.insert(path.to_string(), bytes);
}

enum DetectedLibrary {
    Sch(sch::Library),
    Pcb(pcb::Library),
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn version_round_trip() {
        let mut intlib = IntegratedLibrary::default();
        intlib.version = 7;
        let bytes = intlib.to_bytes().unwrap();
        let parsed = IntegratedLibrary::from_bytes(bytes).unwrap();
        assert_eq!(parsed.version, 7);
    }

    #[test]
    fn round_trips_real_world_layout() {
        let mut sch_lib = sch::Library::default();
        sch_lib.components.push(sch::Component::new("U1"));
        let mut pcb_lib = pcb::Library::default();
        pcb_lib.unique_id = "ROUNDID0".into();
        pcb_lib.components.push(pcb::Component::new("R0402"));

        let mut intlib = IntegratedLibrary::default();
        intlib.schematic_libraries.push(NamedLibrary {
            name: "0.schlib".into(),
            library: sch_lib,
        });
        intlib.footprint_libraries.push(NamedLibrary {
            name: "0.pcblib".into(),
            library: pcb_lib,
        });
        intlib.cross_reference = Some(b"\x00\x00\x00\x00\x00".to_vec());
        intlib.parameters_bin = Some(b"\x00".to_vec());

        let bytes = intlib.to_bytes().expect("write");
        let parsed = IntegratedLibrary::from_bytes(bytes).expect("read");

        assert_eq!(parsed.version, DEFAULT_VERSION);
        assert_eq!(parsed.schematic_libraries.len(), 1);
        assert_eq!(parsed.schematic_libraries[0].name, "0.schlib");
        assert_eq!(parsed.schematic_libraries[0].library.components.len(), 1);
        assert_eq!(parsed.footprint_libraries.len(), 1);
        assert_eq!(parsed.footprint_libraries[0].name, "0.pcblib");
        assert_eq!(parsed.footprint_libraries[0].library.unique_id, "ROUNDID0");
        assert_eq!(parsed.cross_reference.as_deref(), Some(&[0u8; 5][..]));
        assert_eq!(parsed.parameters_bin.as_deref(), Some(&[0u8][..]));
    }

    #[test]
    fn renames_unnumbered_streams_on_write() {
        let mut sch_lib = sch::Library::default();
        sch_lib.components.push(sch::Component::new("U1"));
        let mut intlib = IntegratedLibrary::default();
        intlib.schematic_libraries.push(NamedLibrary {
            name: "Parts.SchLib".into(), // not the canonical numbered form
            library: sch_lib,
        });
        let bytes = intlib.to_bytes().unwrap();
        let parsed = IntegratedLibrary::from_bytes(bytes).unwrap();
        // The numbered name comes back from the storage walk.
        assert_eq!(parsed.schematic_libraries.len(), 1);
        assert!(
            parsed.schematic_libraries[0].name.eq_ignore_ascii_case("0.schlib"),
            "non-canonical input names are normalised; got {:?}",
            parsed.schematic_libraries[0].name
        );
    }

    #[test]
    fn additional_files_pass_through() {
        let mut intlib = IntegratedLibrary::default();
        intlib
            .additional_files
            .insert("Datasheets/u1.pdf".into(), b"%PDF-1.4 stub".to_vec());
        let bytes = intlib.to_bytes().unwrap();
        let parsed = IntegratedLibrary::from_bytes(bytes).unwrap();
        // The CFB walker reports paths with a leading '/', adjust expectation.
        let lookup = parsed
            .additional_files
            .keys()
            .find(|k| k.ends_with("u1.pdf"))
            .expect("datasheet preserved");
        assert!(parsed.additional_files[lookup].starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn parameters_bin_round_trips_byte_stable() {
        // Format: [0x00 leading][[u32 LE len][len bytes ending in NUL]...]
        let blocks = vec![
            "Comment=R10K|Designator=R1|Library Reference=R10K|Pin Count=2".to_string(),
            "Height=0|Pad Count=2".to_string(),
            "Height=0|Pad Count=2".to_string(),
        ];
        let bytes = serialise_parameters_bin(&blocks);
        // Quick sanity: leading byte + 3 blocks of (4 + payload + NUL).
        assert_eq!(bytes[0], 0x00);
        // Round-trip back.
        let parsed = parse_parameters_bin(&bytes).expect("parse");
        assert_eq!(parsed, blocks);
        // And the bytes are byte-stable across re-emit.
        let bytes2 = serialise_parameters_bin(&parsed);
        assert_eq!(bytes, bytes2);
    }

    #[test]
    fn parameters_bin_handles_empty_input() {
        assert_eq!(parse_parameters_bin(&[]).unwrap(), Vec::<String>::new());
        assert_eq!(parse_parameters_bin(&[0x00]).unwrap(), Vec::<String>::new());
        // Round-trip empty.
        let bytes = serialise_parameters_bin(&[]);
        assert_eq!(bytes, vec![0x00]);
    }

    #[test]
    fn parameters_bin_rejects_overrun_length() {
        // Length declares more bytes than are present.
        let mut bytes = vec![0x00];
        bytes.extend_from_slice(&100_u32.to_le_bytes());
        bytes.extend_from_slice(b"short");
        let err = parse_parameters_bin(&bytes).unwrap_err();
        assert!(err.to_string().contains("overruns"));
    }

    #[test]
    fn cross_reference_round_trips_byte_stable() {
        // Build a synthetic block matching the shape we observed in real
        // IntLibs: Tag(1), 2 strings, Tag(1), 2 strings, Tag(4), 2 strings.
        let records = vec![
            CrossRefRecord::Tag(1),
            CrossRefRecord::String("CMP-R10K".into()),
            CrossRefRecord::String(":\\SchLib\\0.schlib".into()),
            CrossRefRecord::Tag(1),
            CrossRefRecord::String("Resistor".into()),
            CrossRefRecord::String("\\\\Server\\Parts\\Components.SchLib".into()),
            CrossRefRecord::Tag(4),
            CrossRefRecord::String("FP-R0402".into()),
            CrossRefRecord::String("PCBLIB".into()),
        ];
        let bytes = serialise_cross_reference(&records);
        let parsed = parse_cross_reference(&bytes).expect("parse");
        assert_eq!(parsed, records);
        let bytes2 = serialise_cross_reference(&parsed);
        assert_eq!(bytes, bytes2, "encoding is deterministic");
    }

    #[test]
    fn cross_reference_disambiguates_strings_from_tags() {
        // Construct a stream that has a tag value (`1`) followed by a string
        // (`"hi"`). The tag's u32 looks nothing like a pascal-string outer
        // size (which is at least 2) so we should classify correctly.
        let records = vec![
            CrossRefRecord::Tag(1),
            CrossRefRecord::String("hi".into()),
        ];
        let bytes = serialise_cross_reference(&records);
        let parsed = parse_cross_reference(&bytes).expect("parse");
        assert_eq!(parsed, records);

        // Edge case: a tag that COULD look like a pascal-string outer size
        // (e.g. `Tag(3)`) is fine as long as the byte after isn't `2`. Our
        // parser tries the string interpretation first, falls back if it
        // doesn't match `pascal_len + 1 == outer`.
        let records = vec![
            CrossRefRecord::Tag(3),
            CrossRefRecord::String("ab".into()),
        ];
        let bytes = serialise_cross_reference(&records);
        let parsed = parse_cross_reference(&bytes).expect("parse");
        assert_eq!(parsed, records);
    }

    #[test]
    fn cross_reference_handles_real_fixture_bytes() {
        // Hard-code the leading 56 bytes of a real Altium-emitted
        // LibCrossRef.Txt to verify our parser produces the expected
        // sequence (Tag(1), "CMP-04913-000051-1", ":\SchLib\0.schlib",
        // Tag(1), "IC PWR MGMT BATTERY MGMT", …).
        let bytes: &[u8] = &[
            0x00, // leading
            0x01, 0x00, 0x00, 0x00, // Tag(1)
            0x13, 0x00, 0x00, 0x00, // outer = 19
            0x12, // pascal_len = 18
            b'C', b'M', b'P', b'-', b'0', b'4', b'9', b'1', b'3', b'-',
            b'0', b'0', b'0', b'0', b'5', b'1', b'-', b'1', // 18 bytes
            0x12, 0x00, 0x00, 0x00, // outer = 18
            0x11, // pascal_len = 17
            b':', b'\\', b'S', b'c', b'h', b'L', b'i', b'b', b'\\',
            b'0', b'.', b's', b'c', b'h', b'l', b'i', b'b', // 17 bytes
        ];
        let parsed = parse_cross_reference(bytes).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], CrossRefRecord::Tag(1));
        assert_eq!(
            parsed[1],
            CrossRefRecord::String("CMP-04913-000051-1".into())
        );
        assert_eq!(
            parsed[2],
            CrossRefRecord::String(":\\SchLib\\0.schlib".into())
        );
    }

    #[test]
    fn parameters_blocks_accessor_round_trips_through_intlib() {
        let mut intlib = IntegratedLibrary::default();
        let blocks = vec![
            "Comment=X|Designator=X1|Pin Count=4".to_string(),
            "Height=0|Pad Count=4".to_string(),
        ];
        intlib.set_parameters_blocks(&blocks);
        // The accessor decodes the same data back.
        let read = intlib.parameters_blocks().unwrap();
        assert_eq!(read, blocks);
        // And empty parameters_bin yields empty.
        intlib.parameters_bin = None;
        assert!(intlib.parameters_blocks().unwrap().is_empty());
    }

    #[test]
    fn cross_reference_records_accessor_round_trips_through_intlib() {
        let mut intlib = IntegratedLibrary::default();
        let records = vec![
            CrossRefRecord::Tag(1),
            CrossRefRecord::String("CMP-X".into()),
            CrossRefRecord::String(":\\SchLib\\0.schlib".into()),
        ];
        intlib.set_cross_reference_records(&records);
        let read = intlib.cross_reference_records().unwrap();
        assert_eq!(read, records);
        intlib.cross_reference = None;
        assert!(intlib.cross_reference_records().unwrap().is_empty());
    }

    #[test]
    fn cross_reference_table_round_trips_through_tokens() {
        // Single-symbol table with three footprints.
        let table = CrossReferenceTable {
            symbols: vec![CrossReferenceSymbol {
                libref: "CMP-X".into(),
                internal_schlib_path: ":\\SchLib\\0.schlib".into(),
                description: "Test Component".into(),
                source_schlib_path: "\\\\Server\\Parts\\Components.SchLib".into(),
                footprints: vec![
                    CrossReferenceFootprint {
                        name: "FP-A".into(),
                        kind: "PCBLIB".into(),
                        internal_pcblib_path: ":\\PCBLib\\0.pcblib".into(),
                        source_pcblib_path: "\\\\Server\\Parts\\Components.PcbLib".into(),
                    },
                    CrossReferenceFootprint {
                        name: "FP-B".into(),
                        kind: "PCBLIB".into(),
                        internal_pcblib_path: ":\\PCBLib\\0.pcblib".into(),
                        source_pcblib_path: "\\\\Server\\Parts\\Components.PcbLib".into(),
                    },
                    CrossReferenceFootprint {
                        name: "FP-C".into(),
                        kind: "PCBLIB".into(),
                        internal_pcblib_path: ":\\PCBLib\\0.pcblib".into(),
                        source_pcblib_path: "\\\\Server\\Parts\\Components.PcbLib".into(),
                    },
                ],
            }],
        };
        let tokens = flatten_cross_reference_table(&table);
        let reparsed = parse_cross_reference_table(&tokens).expect("reparse");
        assert_eq!(reparsed, table);
        // And the serialized bytes round-trip too.
        let bytes = serialise_cross_reference(&tokens);
        let tokens2 = parse_cross_reference(&bytes).expect("parse bytes");
        let reparsed2 = parse_cross_reference_table(&tokens2).expect("reparse2");
        assert_eq!(reparsed2, table);
    }

    #[test]
    fn cross_reference_table_handles_empty() {
        let table = CrossReferenceTable::default();
        let tokens = flatten_cross_reference_table(&table);
        assert!(tokens.is_empty());
        let reparsed = parse_cross_reference_table(&tokens).unwrap();
        assert_eq!(reparsed, table);
    }

    #[test]
    fn cross_reference_table_errors_on_missing_tag1_prefix() {
        // Bad: starts directly with a String — should be Tag(1).
        let tokens = vec![
            CrossRefRecord::String("oops".into()),
            CrossRefRecord::String("nope".into()),
        ];
        assert!(parse_cross_reference_table(&tokens).is_err());
    }

    #[test]
    fn cross_reference_table_errors_on_missing_tag4_marker() {
        // libref + description present but no Tag(4) before footprints.
        let tokens = vec![
            CrossRefRecord::Tag(1),
            CrossRefRecord::String("CMP-X".into()),
            CrossRefRecord::String(":\\SchLib\\0.schlib".into()),
            CrossRefRecord::Tag(1),
            CrossRefRecord::String("desc".into()),
            CrossRefRecord::String("src".into()),
            // Missing Tag(4)
            CrossRefRecord::Tag(99),
        ];
        let err = parse_cross_reference_table(&tokens).unwrap_err();
        assert!(err.to_string().contains("Tag(4)"));
    }

    #[test]
    fn cross_reference_table_accessor_round_trips_through_intlib() {
        let mut intlib = IntegratedLibrary::default();
        let table = CrossReferenceTable {
            symbols: vec![CrossReferenceSymbol {
                libref: "CMP-Y".into(),
                internal_schlib_path: ":\\SchLib\\0.schlib".into(),
                description: "Y".into(),
                source_schlib_path: "Y.SchLib".into(),
                footprints: vec![CrossReferenceFootprint {
                    name: "FP-Y".into(),
                    kind: "PCBLIB".into(),
                    internal_pcblib_path: ":\\PCBLib\\0.pcblib".into(),
                    source_pcblib_path: "Y.PcbLib".into(),
                }],
            }],
        };
        intlib.set_cross_reference_table(&table);
        let read = intlib.cross_reference_table().unwrap();
        assert_eq!(read, table);
    }

    #[test]
    fn cross_reference_table_supports_zero_footprints() {
        // A symbol with no footprints — Tag(4) immediately followed by the
        // next symbol or end-of-stream.
        let table = CrossReferenceTable {
            symbols: vec![CrossReferenceSymbol {
                libref: "Footless".into(),
                internal_schlib_path: ":\\SchLib\\0.schlib".into(),
                description: "no footprints".into(),
                source_schlib_path: "x.SchLib".into(),
                footprints: vec![],
            }],
        };
        let tokens = flatten_cross_reference_table(&table);
        let reparsed = parse_cross_reference_table(&tokens).expect("parse");
        assert_eq!(reparsed, table);
    }

    #[test]
    fn cross_reference_table_supports_multi_symbol() {
        // Two consecutive symbols, each with one footprint.
        let table = CrossReferenceTable {
            symbols: vec![
                CrossReferenceSymbol {
                    libref: "A".into(),
                    internal_schlib_path: ":\\SchLib\\0.schlib".into(),
                    description: "first".into(),
                    source_schlib_path: "A.SchLib".into(),
                    footprints: vec![CrossReferenceFootprint {
                        name: "FP-A".into(),
                        kind: "PCBLIB".into(),
                        internal_pcblib_path: ":\\PCBLib\\0.pcblib".into(),
                        source_pcblib_path: "A.PcbLib".into(),
                    }],
                },
                CrossReferenceSymbol {
                    libref: "B".into(),
                    internal_schlib_path: ":\\SchLib\\1.schlib".into(),
                    description: "second".into(),
                    source_schlib_path: "B.SchLib".into(),
                    footprints: vec![CrossReferenceFootprint {
                        name: "FP-B".into(),
                        kind: "PCBLIB".into(),
                        internal_pcblib_path: ":\\PCBLib\\1.pcblib".into(),
                        source_pcblib_path: "B.PcbLib".into(),
                    }],
                },
            ],
        };
        let tokens = flatten_cross_reference_table(&table);
        let reparsed = parse_cross_reference_table(&tokens).expect("parse");
        assert_eq!(reparsed, table);
    }

    #[test]
    fn lookups_are_case_insensitive() {
        let mut sch_lib = sch::Library::default();
        sch_lib.components.push(sch::Component::new("U1"));
        let mut intlib = IntegratedLibrary::default();
        intlib.schematic_libraries.push(NamedLibrary {
            name: "MIXEDCASE.SchLib".into(),
            library: sch_lib,
        });
        assert!(intlib.schematic_library("mixedcase.schlib").is_some());
        assert!(intlib.schematic_library("nope").is_none());
    }
}
