//! Wrapper over the [`cfb`] crate.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// In-memory compound file.
///
/// Backed by a `Cursor<Vec<u8>>`. Path arguments accept both `&str` and `&Path`
/// using `/` as the separator (`"Library/Data"`, `"Arcs6/Data"`).
pub struct CompoundFile {
    inner: cfb::CompoundFile<Cursor<Vec<u8>>>,
}

/// Information about a single entry inside a storage.
#[derive(Debug, Clone)]
pub struct EntryInfo {
    pub name: String,
    pub path: PathBuf,
    pub is_storage: bool,
    pub is_stream: bool,
    /// Stream length in bytes (zero for storages).
    pub len: u64,
}

impl CompoundFile {
    /// Parse an existing compound file from a byte buffer.
    pub fn open(bytes: Vec<u8>) -> Result<Self> {
        let inner = cfb::CompoundFile::open(Cursor::new(bytes)).map_err(map_cfb_err)?;
        Ok(Self { inner })
    }

    /// Create a fresh, empty compound file. Emits CFB v3 (512-byte sectors);
    /// Altium's PcbDoc / SchDoc loaders don't accept v4.
    pub fn create() -> Result<Self> {
        let inner = cfb::CompoundFile::create_with_version(
            cfb::Version::V3,
            Cursor::new(Vec::new()),
        )
        .map_err(map_cfb_err)?;
        Ok(Self { inner })
    }

    /// Whether `path` resolves to either a storage or a stream.
    pub fn exists(&self, path: impl AsRef<Path>) -> bool {
        self.inner.exists(path)
    }

    /// Whether `path` is a storage.
    pub fn is_storage(&self, path: impl AsRef<Path>) -> bool {
        match self.inner.entry(path) {
            Ok(e) => e.is_storage(),
            Err(_) => false,
        }
    }

    /// Whether `path` is a stream.
    pub fn is_stream(&self, path: impl AsRef<Path>) -> bool {
        match self.inner.entry(path) {
            Ok(e) => e.is_stream(),
            Err(_) => false,
        }
    }

    /// Read the full contents of a stream into a `Vec<u8>`.
    pub fn read_stream(&mut self, path: impl AsRef<Path>) -> Result<Vec<u8>> {
        use std::io::Read;
        let path = path.as_ref();
        if !self.inner.exists(path) {
            return Err(Error::corrupt_in(
                "stream not found",
                path.display().to_string(),
            ));
        }
        let mut stream = self.inner.open_stream(path).map_err(map_cfb_err)?;
        let mut buf = Vec::with_capacity(stream.len() as usize);
        stream.read_to_end(&mut buf).map_err(Error::Io)?;
        Ok(buf)
    }

    /// Read a stream if it exists, otherwise return `None`.
    pub fn try_read_stream(&mut self, path: impl AsRef<Path>) -> Result<Option<Vec<u8>>> {
        let path = path.as_ref();
        if self.is_stream(path) {
            Ok(Some(self.read_stream(path)?))
        } else {
            Ok(None)
        }
    }

    /// List the immediate children of a storage. The root storage's path is `""` or `"/"`.
    pub fn list_children(&self, path: impl AsRef<Path>) -> Result<Vec<EntryInfo>> {
        let path = path.as_ref();
        let entries: Vec<EntryInfo> = self
            .inner
            .read_storage(path)
            .map_err(map_cfb_err)?
            .map(|entry| EntryInfo {
                name: entry.name().to_string(),
                path: entry.path().to_path_buf(),
                is_storage: entry.is_storage(),
                is_stream: entry.is_stream(),
                len: entry.len(),
            })
            .collect();
        Ok(entries)
    }

    /// Create a storage; intermediate parents are created automatically.
    pub fn create_storage(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.inner.create_storage_all(path).map_err(map_cfb_err)
    }

    /// Write `data` to a stream, creating or overwriting it.
    pub fn write_stream(&mut self, path: impl AsRef<Path>, data: &[u8]) -> Result<()> {
        use std::io::Write;
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !self.inner.exists(parent) {
                self.inner.create_storage_all(parent).map_err(map_cfb_err)?;
            }
        }
        let mut stream = self.inner.create_stream(path).map_err(map_cfb_err)?;
        stream.write_all(data).map_err(Error::Io)?;
        stream.flush().map_err(Error::Io)?;
        Ok(())
    }

    /// Remove a stream.
    pub fn remove_stream(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.inner.remove_stream(path).map_err(map_cfb_err)
    }

    /// Remove a storage and everything beneath it.
    pub fn remove_storage_all(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.inner.remove_storage_all(path).map_err(map_cfb_err)
    }

    /// Serialise the whole compound file back to a byte buffer.
    pub fn into_bytes(mut self) -> Result<Vec<u8>> {
        self.inner.flush().map_err(Error::Io)?;
        Ok(self.inner.into_inner().into_inner())
    }
}

fn map_cfb_err(err: std::io::Error) -> Error {
    // The cfb crate expresses everything as io::Error. Most common errors from
    // it are `InvalidData` for structural problems and `NotFound` for missing
    // entries — surface them as `Corrupt` so callers can match by category.
    use std::io::ErrorKind::*;
    match err.kind() {
        InvalidData | UnexpectedEof => Error::Corrupt {
            message: err.to_string(),
            stream: None,
            path: None,
        },
        _ => Error::Io(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_sample() -> Vec<u8> {
        let mut cf = CompoundFile::create().unwrap();
        cf.create_storage("Library").unwrap();
        cf.write_stream("Library/Header", b"hdr-bytes").unwrap();
        cf.write_stream("Library/Data", b"the data").unwrap();
        cf.write_stream("FileHeader", &[0u8; 8]).unwrap();
        cf.into_bytes().unwrap()
    }

    #[test]
    fn round_trip_through_bytes() {
        let bytes = build_sample();
        let mut cf = CompoundFile::open(bytes).unwrap();

        assert!(cf.is_stream("FileHeader"));
        assert!(cf.is_storage("Library"));
        assert!(!cf.is_stream("Library"));
        assert!(!cf.exists("Missing"));

        assert_eq!(cf.read_stream("Library/Data").unwrap(), b"the data");
        assert_eq!(
            cf.try_read_stream("Library/Header").unwrap().unwrap(),
            b"hdr-bytes"
        );
        assert!(cf.try_read_stream("Library/Missing").unwrap().is_none());

        let children: Vec<_> = cf.list_children("Library").unwrap();
        assert_eq!(children.len(), 2);
        let names: Vec<&str> = children.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Header"));
        assert!(names.contains(&"Data"));
        for entry in &children {
            assert!(entry.is_stream);
        }
    }

    #[test]
    fn write_creates_intermediate_storages() {
        let mut cf = CompoundFile::create().unwrap();
        cf.write_stream("Foo/Bar/Baz", b"deep").unwrap();
        assert_eq!(cf.read_stream("Foo/Bar/Baz").unwrap(), b"deep");
        assert!(cf.is_storage("Foo"));
        assert!(cf.is_storage("Foo/Bar"));
    }

    #[test]
    fn remove_stream_then_recreate() {
        let bytes = build_sample();
        let mut cf = CompoundFile::open(bytes).unwrap();
        cf.remove_stream("Library/Data").unwrap();
        assert!(!cf.exists("Library/Data"));
        cf.write_stream("Library/Data", b"replaced").unwrap();
        assert_eq!(cf.read_stream("Library/Data").unwrap(), b"replaced");
    }

    #[test]
    fn missing_stream_yields_corrupt_error() {
        let bytes = build_sample();
        let mut cf = CompoundFile::open(bytes).unwrap();
        let err = cf.read_stream("Library/Missing").unwrap_err();
        match err {
            Error::Corrupt { .. } | Error::Io(_) => {}
            _ => panic!("expected Corrupt or Io, got {err:?}"),
        }
    }
}
