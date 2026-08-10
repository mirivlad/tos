// SPDX-License-Identifier: GPL-3.0-or-later
//! Structured frontend diagnostics for TOS Core V1 (docs/41 section 7).
//!
//! `docs/41` requires every parser, checker, verifier, runtime and resource
//! diagnostic to carry a stable symbolic code, severity, stage, module name and
//! canonical repository path, source-set identity and normalized source content
//! ID, byte span with derived line/UTF-8 column, structured key/value fields and
//! ordered causal diagnostics.
//!
//! A stage produces the part it can know on its own: code, severity, stage,
//! span, derived positions, fields and causes. The module identity — name,
//! canonical repository path, normalized source content ID and source-set
//! identity — is attached by the layer that resolves modules over a source set
//! (`crate::modules`), because only that layer knows a module's path. A
//! diagnostic produced without it carries no identity rather than a placeholder.

use std::boxed::Box;
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

/// The identity of the module a diagnostic belongs to (docs/41 section 7,
/// docs/42 section 6).
///
/// The source-set identity is the selected system commit or accepted detached
/// source-set identity. It is an input to the resolver rather than something
/// derivable from one source unit, so it is present only when the caller
/// supplied it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleIdentity {
    name: String,
    path: String,
    content_id: String,
    source_set: Option<String>,
}

impl ModuleIdentity {
    pub fn new(name: String, path: String, content_id: String) -> ModuleIdentity {
        ModuleIdentity {
            name,
            path,
            content_id,
            source_set: None,
        }
    }

    pub fn with_source_set(mut self, source_set: impl ToString) -> ModuleIdentity {
        self.source_set = Some(source_set.to_string());
        self
    }

    /// The declared module name, dot-separated.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The canonical repository path, never a host path (docs/42 section 6).
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The content identity of the normalized source bytes.
    pub fn content_id(&self) -> &str {
        &self.content_id
    }

    pub fn source_set(&self) -> Option<&str> {
        self.source_set.as_deref()
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
    module: Option<Box<ModuleIdentity>>,
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
            module: None,
            fields: Vec::new(),
            causes: Vec::new(),
        }
    }

    /// Attaches the identity of the module this diagnostic belongs to.
    pub fn with_module(mut self, module: ModuleIdentity) -> Diagnostic {
        self.module = Some(Box::new(module));
        self
    }

    /// The module identity, when the resolver attached one.
    pub fn module(&self) -> Option<&ModuleIdentity> {
        self.module.as_deref()
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
