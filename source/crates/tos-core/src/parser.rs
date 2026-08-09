// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic TOS Core V1 syntax parsing.
//!
//! This module owns syntax-tree construction only. Name, type, effect and
//! resource decisions belong to later frontend stages.

use std::vec::Vec;

use crate::{LexError, Lexer, SourceUnit, Token, TokenKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    start: usize,
    end: usize,
}

impl Span {
    pub fn start(self) -> usize {
        self.start
    }

    pub fn end(self) -> usize {
        self.end
    }

    pub fn text(self, source: &SourceUnit) -> &str {
        core::str::from_utf8(&source.bytes()[self.start..self.end])
            .expect("parser spans are lexer token boundaries")
    }
}

impl From<Token> for Span {
    fn from(token: Token) -> Self {
        Self {
            start: token.start(),
            end: token.end(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Bootstrap,
    Full,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ModuleHeader {
    name: Vec<Span>,
    version: (u32, u32),
    profile: Profile,
    span: Span,
}

impl ModuleHeader {
    pub fn name(&self) -> &[Span] {
        &self.name
    }

    pub fn version(&self) -> (u32, u32) {
        self.version
    }

    pub fn profile(&self) -> Profile {
        self.profile
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImportKind {
    Module,
    Capability,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Import {
    kind: ImportKind,
    path: Vec<Span>,
    binding: Span,
    span: Span,
}

impl Import {
    pub fn kind(&self) -> ImportKind {
        self.kind
    }

    pub fn path(&self) -> &[Span] {
        &self.path
    }

    pub fn binding(&self) -> Span {
        self.binding
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ModulePrefix {
    header: ModuleHeader,
    imports: Vec<Import>,
}

impl ModulePrefix {
    pub fn header(&self) -> &ModuleHeader {
        &self.header
    }

    pub fn imports(&self) -> &[Import] {
        &self.imports
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseErrorCode {
    Lexical,
    ExpectedModuleHeader,
    ExpectedIdentifier,
    ExpectedVersionComponent,
    ExpectedProfile,
    UnexpectedToken,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseError {
    code: ParseErrorCode,
    span: Span,
}

impl ParseError {
    pub fn code(self) -> ParseErrorCode {
        self.code
    }

    pub fn span(self) -> Span {
        self.span
    }
}

pub struct Parser;

impl Parser {
    pub fn parse_header(source: &SourceUnit) -> Result<ModuleHeader, ParseError> {
        let tokens = Lexer::lex(source).map_err(lexical_error)?;
        let mut parser = TokenCursor {
            source,
            tokens,
            index: 0,
        };
        let header = parser.parse_header()?;
        parser.expect_kind(TokenKind::Eof, ParseErrorCode::UnexpectedToken)?;
        Ok(header)
    }

    pub fn parse_prefix(source: &SourceUnit) -> Result<ModulePrefix, ParseError> {
        let tokens = Lexer::lex(source).map_err(lexical_error)?;
        let mut parser = TokenCursor {
            source,
            tokens,
            index: 0,
        };
        let header = parser.parse_header()?;
        let mut imports = Vec::new();
        while parser.current_text() == "import" {
            imports.push(parser.parse_import()?);
        }
        parser.expect_kind(TokenKind::Eof, ParseErrorCode::UnexpectedToken)?;
        Ok(ModulePrefix { header, imports })
    }
}

struct TokenCursor<'source> {
    source: &'source SourceUnit,
    tokens: Vec<Token>,
    index: usize,
}

impl<'source> TokenCursor<'source> {
    fn parse_header(&mut self) -> Result<ModuleHeader, ParseError> {
        let start = self.expect_word("module", ParseErrorCode::ExpectedModuleHeader)?;
        let mut name = vec![self.expect_identifier()?];
        while self.consume_kind(TokenKind::Dot).is_some() {
            name.push(self.expect_identifier()?);
        }
        self.expect_word("version", ParseErrorCode::ExpectedModuleHeader)?;
        let major = self.expect_version_component()?;
        self.expect_kind(TokenKind::Dot, ParseErrorCode::ExpectedVersionComponent)?;
        let minor = self.expect_version_component()?;
        self.expect_word("profile", ParseErrorCode::ExpectedProfile)?;
        let profile = match self.current_text() {
            "bootstrap" => {
                self.advance();
                Profile::Bootstrap
            }
            "full" => {
                self.advance();
                Profile::Full
            }
            _ => return Err(self.error_here(ParseErrorCode::ExpectedProfile)),
        };
        let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
        Ok(ModuleHeader {
            name,
            version: (major, minor),
            profile,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_import(&mut self) -> Result<Import, ParseError> {
        let start = self.expect_word("import", ParseErrorCode::UnexpectedToken)?;
        let kind = if self.current_text() == "capability" {
            self.advance();
            ImportKind::Capability
        } else {
            ImportKind::Module
        };
        let path = self.parse_dotted_name()?;
        let binding = if self.current_text() == "as" {
            self.advance();
            self.expect_identifier()?
        } else if kind == ImportKind::Module {
            *path.last().expect("dotted name has one component")
        } else {
            return Err(self.error_here(ParseErrorCode::ExpectedIdentifier));
        };
        let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
        Ok(Import {
            kind,
            path,
            binding,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_dotted_name(&mut self) -> Result<Vec<Span>, ParseError> {
        let mut name = vec![self.expect_identifier()?];
        while self.consume_kind(TokenKind::Dot).is_some() {
            name.push(self.expect_identifier()?);
        }
        Ok(name)
    }

    fn expect_identifier(&mut self) -> Result<Span, ParseError> {
        if self.current().kind() == TokenKind::Identifier {
            Ok(Span::from(self.advance()))
        } else {
            Err(self.error_here(ParseErrorCode::ExpectedIdentifier))
        }
    }

    fn expect_version_component(&mut self) -> Result<u32, ParseError> {
        if self.current().kind() != TokenKind::Integer {
            return Err(self.error_here(ParseErrorCode::ExpectedVersionComponent));
        }
        let token = self.advance();
        token.text(self.source).parse().map_err(|_| ParseError {
            code: ParseErrorCode::ExpectedVersionComponent,
            span: Span::from(token),
        })
    }

    fn expect_word(&mut self, word: &str, code: ParseErrorCode) -> Result<Span, ParseError> {
        if self.current_text() == word {
            Ok(Span::from(self.advance()))
        } else {
            Err(self.error_here(code))
        }
    }

    fn expect_kind(&mut self, kind: TokenKind, code: ParseErrorCode) -> Result<Span, ParseError> {
        if self.current().kind() == kind {
            Ok(Span::from(self.advance()))
        } else {
            Err(self.error_here(code))
        }
    }

    fn consume_kind(&mut self, kind: TokenKind) -> Option<Span> {
        (self.current().kind() == kind).then(|| Span::from(self.advance()))
    }

    fn current(&self) -> Token {
        self.tokens[self.index]
    }

    fn current_text(&self) -> &str {
        self.current().text(self.source)
    }

    fn advance(&mut self) -> Token {
        let token = self.current();
        self.index += 1;
        token
    }

    fn error_here(&self, code: ParseErrorCode) -> ParseError {
        ParseError {
            code,
            span: Span::from(self.current()),
        }
    }
}

fn lexical_error(error: LexError) -> ParseError {
    ParseError {
        code: ParseErrorCode::Lexical,
        span: Span {
            start: error.byte_offset(),
            end: error.byte_offset(),
        },
    }
}
