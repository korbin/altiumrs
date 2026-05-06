//! Non-fatal warnings emitted during reading.

use crate::enums::DiagnosticSeverity;

/// A single advisory message produced while reading a file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub stream: Option<String>,
    pub record_index: Option<usize>,
}

impl Diagnostic {
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
            stream: None,
            record_index: None,
        }
    }

    pub fn warning_in(message: impl Into<String>, stream: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
            stream: Some(stream.into()),
            record_index: None,
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Info,
            message: message.into(),
            stream: None,
            record_index: None,
        }
    }

    pub fn with_stream(mut self, stream: impl Into<String>) -> Self {
        self.stream = Some(stream.into());
        self
    }

    pub fn with_record(mut self, index: usize) -> Self {
        self.record_index = Some(index);
        self
    }
}
