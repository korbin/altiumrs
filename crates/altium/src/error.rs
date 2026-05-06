//! Error type and result alias used throughout the crate.

use std::path::PathBuf;

/// Crate-wide error type.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Wrap a `std::io::Error`.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// File could not be parsed because the structure is malformed.
    #[error("corrupt file{}{}: {message}",
        path.as_ref().map(|p| format!(" {}", p.display())).unwrap_or_default(),
        stream.as_ref().map(|s| format!(" stream {s}")).unwrap_or_default())]
    Corrupt {
        message: String,
        stream: Option<String>,
        path: Option<PathBuf>,
    },

    /// File contained a record kind we don't model yet.
    #[error("unsupported feature: {message}")]
    Unsupported {
        message: String,
        record_type: Option<i32>,
    },

    /// Compound file (OLE) error from the underlying `cfb` crate.
    #[error("compound file: {0}")]
    Compound(String),

    /// Bad UTF-8 / decoding error while reading a string field.
    #[error("invalid string encoding: {0}")]
    Encoding(String),
}

impl Error {
    /// Build a `Corrupt` error with just a message.
    pub fn corrupt(message: impl Into<String>) -> Self {
        Self::Corrupt {
            message: message.into(),
            stream: None,
            path: None,
        }
    }

    /// Build a `Corrupt` error and attach the originating stream name.
    pub fn corrupt_in(message: impl Into<String>, stream: impl Into<String>) -> Self {
        Self::Corrupt {
            message: message.into(),
            stream: Some(stream.into()),
            path: None,
        }
    }

    /// Build an `Unsupported` error with optional record type.
    pub fn unsupported(message: impl Into<String>, record_type: Option<i32>) -> Self {
        Self::Unsupported {
            message: message.into(),
            record_type,
        }
    }

    /// Tag any `Error::Corrupt` with the file path it came from.
    pub fn with_path(mut self, p: impl Into<PathBuf>) -> Self {
        if let Self::Corrupt { ref mut path, .. } = self {
            *path = Some(p.into());
        }
        self
    }
}

/// Crate-wide result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_with_stream_and_path() {
        let err = Error::corrupt_in("bad header", "FileHeader").with_path("/tmp/x.PcbLib");
        let s = err.to_string();
        assert!(s.contains("FileHeader"));
        assert!(s.contains("/tmp/x.PcbLib"));
        assert!(s.contains("bad header"));
    }

    #[test]
    fn io_conversion() {
        let err: Error = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "eof").into();
        assert!(matches!(err, Error::Io(_)));
    }
}
