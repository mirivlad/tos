// SPDX-License-Identifier: GPL-3.0-or-later
//! Structured frontend diagnostics for TOS Core V1 (docs/41 section 7).
//!
//! `docs/41` requires every parser, checker, verifier, runtime and resource
//! diagnostic to carry a stable symbolic code, severity, stage, module name and
//! canonical repository path, source-set identity and normalized source content
//! ID, byte span with derived line/UTF-8 column, structured key/value fields and
//! ordered causal diagnostics.
//!
//! This module supplies the part a source-unit-scoped frontend stage can know:
//! code, severity, stage, span, derived positions, fields and causes. Module
//! name, canonical repository path, source-set identity and normalized source
//! content ID are attached by the compilation driver that owns module-to-path
//! mapping (`docs/42`); that layer does not exist yet, so this record does not
//! carry placeholder values for them.

use std::string::{String, ToString};
use std::vec::Vec;

use crate::{SourceUnit, Span};

/// Diagnostic severity as defined by docs/41 section 7.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    pub fn symbol(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        }
    }
}

/// Frontend stage that produced a diagnostic, in the docs/41 precedence order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Stage {
    Lex,
    Parse,
    Type,
    Ownership,
    Effect,
    Resource,
    Ir,
    Runtime,
}

impl Stage {
    pub fn symbol(self) -> &'static str {
        match self {
            Stage::Lex => "lex",
            Stage::Parse => "parse",
            Stage::Type => "type",
            Stage::Ownership => "ownership",
            Stage::Effect => "effect",
            Stage::Resource => "resource",
            Stage::Ir => "IR",
            Stage::Runtime => "runtime",
        }
    }
}

/// One-based line and one-based UTF-8 scalar column derived from a byte offset.
///
/// The column counts Unicode scalar values from the start of the line, not
/// bytes, so a diagnostic on a line containing multi-byte comment or string
/// text still points at the character a reader sees.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    line: usize,
    column: usize,
}

impl Position {
    pub fn line(self) -> usize {
        self.line
    }

    pub fn column(self) -> usize {
        self.column
    }

    /// Derives the position of `byte_offset` within `source`.
    ///
    /// An offset past the end of the source resolves to the position one past
    /// the final scalar value, which is the correct place to report a
    /// diagnostic against the end-of-file token.
    pub fn at(source: &SourceUnit, byte_offset: usize) -> Position {
        let bytes = source.bytes();
        let limit = byte_offset.min(bytes.len());
        let mut line = 1;
        let mut column = 1;
        let mut index = 0;
        while index < limit {
            if bytes[index] == b'\n' {
                line += 1;
                column = 1;
                index += 1;
                continue;
            }
            // Continuation bytes belong to the scalar value that started
            // earlier, so only leading bytes advance the column.
            if bytes[index] & 0b1100_0000 != 0b1000_0000 {
                column += 1;
            }
            index += 1;
        }
        Position { line, column }
    }
}

/// One structured `key=value` field of a diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticField {
    key: &'static str,
    value: String,
}

impl DiagnosticField {
    pub fn key(&self) -> &'static str {
        self.key
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

/// A single frontend diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    code: &'static str,
    severity: Severity,
    stage: Stage,
    span: Span,
    start: Position,
    end: Position,
    fields: Vec<DiagnosticField>,
    causes: Vec<Diagnostic>,
}

impl Diagnostic {
    pub fn new(
        code: &'static str,
        severity: Severity,
        stage: Stage,
        span: Span,
        source: &SourceUnit,
    ) -> Diagnostic {
        Diagnostic {
            code,
            severity,
            stage,
            span,
            start: Position::at(source, span.start()),
            end: Position::at(source, span.end()),
            fields: Vec::new(),
            causes: Vec::new(),
        }
    }

    pub fn with_field(mut self, key: &'static str, value: impl ToString) -> Diagnostic {
        self.fields.push(DiagnosticField {
            key,
            value: value.to_string(),
        });
        self
    }

    pub fn with_cause(mut self, cause: Diagnostic) -> Diagnostic {
        self.causes.push(cause);
        self
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn start(&self) -> Position {
        self.start
    }

    pub fn end(&self) -> Position {
        self.end
    }

    pub fn fields(&self) -> &[DiagnosticField] {
        &self.fields
    }

    /// Ordered causal diagnostics, nearest cause first.
    pub fn causes(&self) -> &[Diagnostic] {
        &self.causes
    }

    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| field.value.as_str())
    }
}
