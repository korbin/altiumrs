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
//! `Version.Txt`, `Parameters   .bin`, and `LibCrossRef.Txt` aren't
//! round-tripped through typed fields yet — they're held as raw bytes so
//! re-emission is byte-faithful enough for Altium to re-read. The bundled
//! `.SchLib` / `.PcbLib` files round-trip through their own typed parsers.
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
    pub cross_reference: Option<Vec<u8>>,
    /// Raw bytes of `Parameters   .bin`. Preserved verbatim for round-trip.
    pub parameters_bin: Option<Vec<u8>>,
    /// Streams not handled by any typed field above. Datasheets, simulation
    /// models, or unrecognised layout variants land here under their full
    /// stream path.
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
