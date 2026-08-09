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

#[derive(Debug, Eq, PartialEq)]
pub struct ResourceLimit {
    name: Span,
    value: Span,
    span: Span,
}

impl ResourceLimit {
    pub fn name(&self) -> Span {
        self.name
    }

    pub fn value(&self) -> Span {
        self.value
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ResourceDeclaration {
    limits: Vec<ResourceLimit>,
    span: Span,
}

impl ResourceDeclaration {
    pub fn limits(&self) -> &[ResourceLimit] {
        &self.limits
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ModuleOutline {
    prefix: ModulePrefix,
    resource: ResourceDeclaration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeSyntax {
    span: Span,
}

impl TypeSyntax {
    pub fn text(self, source: &SourceUnit) -> &str {
        self.span.text(source)
    }

    pub fn span(self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct RecordField {
    name: Span,
    ty: TypeSyntax,
    span: Span,
}

impl RecordField {
    pub fn name(&self) -> Span {
        self.name
    }

    pub fn ty(&self) -> TypeSyntax {
        self.ty
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct RecordDeclaration {
    name: Span,
    fields: Vec<RecordField>,
    span: Span,
}

impl RecordDeclaration {
    pub fn name(&self) -> Span {
        self.name
    }

    pub fn fields(&self) -> &[RecordField] {
        &self.fields
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnumVariantForm {
    Unit,
    Tuple,
    NamedFields,
}

#[derive(Debug, Eq, PartialEq)]
pub struct EnumVariant {
    name: Span,
    form: EnumVariantForm,
    tuple_types: Vec<TypeSyntax>,
    fields: Vec<RecordField>,
    span: Span,
}

impl EnumVariant {
    pub fn name(&self) -> Span {
        self.name
    }

    pub fn form(&self) -> EnumVariantForm {
        self.form
    }

    pub fn tuple_types(&self) -> &[TypeSyntax] {
        &self.tuple_types
    }

    pub fn fields(&self) -> &[RecordField] {
        &self.fields
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct EnumDeclaration {
    name: Span,
    variants: Vec<EnumVariant>,
    span: Span,
}

impl EnumDeclaration {
    pub fn name(&self) -> Span {
        self.name
    }

    pub fn variants(&self) -> &[EnumVariant] {
        &self.variants
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Schema {
    outline: ModuleOutline,
    records: Vec<RecordDeclaration>,
    enums: Vec<EnumDeclaration>,
}

impl Schema {
    pub fn outline(&self) -> &ModuleOutline {
        &self.outline
    }

    pub fn records(&self) -> &[RecordDeclaration] {
        &self.records
    }

    pub fn enums(&self) -> &[EnumDeclaration] {
        &self.enums
    }
}

impl ModuleOutline {
    pub fn prefix(&self) -> &ModulePrefix {
        &self.prefix
    }

    pub fn resource(&self) -> &ResourceDeclaration {
        &self.resource
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
    ExpectedLiteral,
    ListSeparatorRequired,
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

    pub fn parse_outline(source: &SourceUnit) -> Result<ModuleOutline, ParseError> {
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
        let resource = parser.parse_resource_declaration()?;
        parser.expect_kind(TokenKind::Eof, ParseErrorCode::UnexpectedToken)?;
        Ok(ModuleOutline {
            prefix: ModulePrefix { header, imports },
            resource,
        })
    }

    pub fn parse_schema(source: &SourceUnit) -> Result<Schema, ParseError> {
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
        let resource = parser.parse_resource_declaration()?;
        let mut records = Vec::new();
        let mut enums = Vec::new();
        while matches!(parser.current_text(), "record" | "enum") {
            if parser.current_text() == "record" {
                records.push(parser.parse_record_declaration()?);
            } else {
                enums.push(parser.parse_enum_declaration()?);
            }
        }
        parser.expect_kind(TokenKind::Eof, ParseErrorCode::UnexpectedToken)?;
        Ok(Schema {
            outline: ModuleOutline {
                prefix: ModulePrefix { header, imports },
                resource,
            },
            records,
            enums,
        })
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

    fn parse_resource_declaration(&mut self) -> Result<ResourceDeclaration, ParseError> {
        let start = self.expect_word("resource", ParseErrorCode::UnexpectedToken)?;
        self.expect_kind(TokenKind::OpenBracket, ParseErrorCode::UnexpectedToken)?;
        let mut limits = Vec::new();
        if self.current().kind() != TokenKind::CloseBracket {
            loop {
                let name = self.expect_identifier()?;
                self.expect_kind(TokenKind::Colon, ParseErrorCode::UnexpectedToken)?;
                let value = self.expect_literal()?;
                limits.push(ResourceLimit {
                    name,
                    value,
                    span: Span {
                        start: name.start(),
                        end: value.end(),
                    },
                });
                if self.consume_kind(TokenKind::Comma).is_some() {
                    if self.current().kind() == TokenKind::CloseBracket {
                        break;
                    }
                    continue;
                }
                if self.current().kind() != TokenKind::CloseBracket {
                    return Err(self.error_here(ParseErrorCode::ListSeparatorRequired));
                }
                break;
            }
        }
        let end = self.expect_kind(TokenKind::CloseBracket, ParseErrorCode::UnexpectedToken)?;
        Ok(ResourceDeclaration {
            limits,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_record_declaration(&mut self) -> Result<RecordDeclaration, ParseError> {
        let start = self.expect_word("record", ParseErrorCode::UnexpectedToken)?;
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::OpenBracket, ParseErrorCode::UnexpectedToken)?;
        let fields = self.parse_field_list()?;
        let end = self.expect_kind(TokenKind::CloseBracket, ParseErrorCode::UnexpectedToken)?;
        Ok(RecordDeclaration {
            name,
            fields,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_enum_declaration(&mut self) -> Result<EnumDeclaration, ParseError> {
        let start = self.expect_word("enum", ParseErrorCode::UnexpectedToken)?;
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::OpenBracket, ParseErrorCode::UnexpectedToken)?;
        let mut variants = Vec::new();
        if self.current().kind() != TokenKind::CloseBracket {
            loop {
                variants.push(self.parse_enum_variant()?);
                if self.consume_kind(TokenKind::Comma).is_some() {
                    if self.current().kind() == TokenKind::CloseBracket {
                        break;
                    }
                    continue;
                }
                if self.current().kind() != TokenKind::CloseBracket {
                    return Err(self.error_here(ParseErrorCode::ListSeparatorRequired));
                }
                break;
            }
        }
        let end = self.expect_kind(TokenKind::CloseBracket, ParseErrorCode::UnexpectedToken)?;
        Ok(EnumDeclaration {
            name,
            variants,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_enum_variant(&mut self) -> Result<EnumVariant, ParseError> {
        let name = self.expect_identifier()?;
        if self.consume_kind(TokenKind::OpenParen).is_some() {
            let tuple_types = self.parse_tuple_type_list()?;
            let end = self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
            return Ok(EnumVariant {
                name,
                form: EnumVariantForm::Tuple,
                tuple_types,
                fields: Vec::new(),
                span: Span {
                    start: name.start(),
                    end: end.end(),
                },
            });
        }
        if self.consume_kind(TokenKind::OpenBracket).is_some() {
            let fields = self.parse_field_list()?;
            let end = self.expect_kind(TokenKind::CloseBracket, ParseErrorCode::UnexpectedToken)?;
            return Ok(EnumVariant {
                name,
                form: EnumVariantForm::NamedFields,
                tuple_types: Vec::new(),
                fields,
                span: Span {
                    start: name.start(),
                    end: end.end(),
                },
            });
        }
        Ok(EnumVariant {
            name,
            form: EnumVariantForm::Unit,
            tuple_types: Vec::new(),
            fields: Vec::new(),
            span: name,
        })
    }

    fn parse_tuple_type_list(&mut self) -> Result<Vec<TypeSyntax>, ParseError> {
        let mut types = Vec::new();
        if self.current().kind() == TokenKind::CloseParen {
            return Ok(types);
        }
        loop {
            types.push(self.parse_simple_type()?);
            if self.consume_kind(TokenKind::Comma).is_some() {
                if self.current().kind() == TokenKind::CloseParen {
                    break;
                }
                continue;
            }
            if self.current().kind() != TokenKind::CloseParen {
                return Err(self.error_here(ParseErrorCode::ListSeparatorRequired));
            }
            break;
        }
        Ok(types)
    }

    fn parse_field_list(&mut self) -> Result<Vec<RecordField>, ParseError> {
        let mut fields = Vec::new();
        if self.current().kind() == TokenKind::CloseBracket {
            return Ok(fields);
        }
        loop {
            if self.current_text() == "pub" {
                self.advance();
            }
            let name = self.expect_identifier()?;
            self.expect_kind(TokenKind::Colon, ParseErrorCode::UnexpectedToken)?;
            let ty = self.parse_simple_type()?;
            fields.push(RecordField {
                name,
                ty,
                span: Span {
                    start: name.start(),
                    end: ty.span().end(),
                },
            });
            if self.consume_kind(TokenKind::Comma).is_some() {
                if self.current().kind() == TokenKind::CloseBracket {
                    break;
                }
                continue;
            }
            if self.current().kind() != TokenKind::CloseBracket {
                return Err(self.error_here(ParseErrorCode::ListSeparatorRequired));
            }
            break;
        }
        Ok(fields)
    }

    fn parse_simple_type(&mut self) -> Result<TypeSyntax, ParseError> {
        if self.current().kind() == TokenKind::Identifier {
            Ok(TypeSyntax {
                span: Span::from(self.advance()),
            })
        } else {
            Err(self.error_here(ParseErrorCode::ExpectedIdentifier))
        }
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

    fn expect_literal(&mut self) -> Result<Span, ParseError> {
        if matches!(
            self.current().kind(),
            TokenKind::Boolean
                | TokenKind::Integer
                | TokenKind::Size
                | TokenKind::Duration
                | TokenKind::String
                | TokenKind::Bytes
        ) {
            Ok(Span::from(self.advance()))
        } else {
            Err(self.error_here(ParseErrorCode::ExpectedLiteral))
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
