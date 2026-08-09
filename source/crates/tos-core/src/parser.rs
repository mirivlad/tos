// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic TOS Core V1 syntax parsing.
//!
//! This module owns syntax-tree construction only. Name, type, effect and
//! resource decisions belong to later frontend stages.

use std::boxed::Box;
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
pub enum TypeSyntaxForm {
    Name,
    Constructed,
    Array,
    Tuple,
    Function,
}

#[derive(Debug, Eq, PartialEq)]
pub enum TypeSyntax {
    Name {
        path: Vec<Span>,
        span: Span,
    },
    Constructed {
        name: Span,
        arguments: Vec<TypeSyntax>,
        span: Span,
    },
    Array {
        element: Box<TypeSyntax>,
        length: Span,
        span: Span,
    },
    Tuple {
        elements: Vec<TypeSyntax>,
        span: Span,
    },
    Function {
        parameters: Vec<TypeSyntax>,
        result: Box<TypeSyntax>,
        span: Span,
    },
}

impl TypeSyntax {
    pub fn form(&self) -> TypeSyntaxForm {
        match self {
            Self::Name { .. } => TypeSyntaxForm::Name,
            Self::Constructed { .. } => TypeSyntaxForm::Constructed,
            Self::Array { .. } => TypeSyntaxForm::Array,
            Self::Tuple { .. } => TypeSyntaxForm::Tuple,
            Self::Function { .. } => TypeSyntaxForm::Function,
        }
    }
    pub fn text<'source>(&self, source: &'source SourceUnit) -> &'source str {
        self.span().text(source)
    }
    pub fn span(&self) -> Span {
        match self {
            Self::Name { span, .. }
            | Self::Constructed { span, .. }
            | Self::Array { span, .. }
            | Self::Tuple { span, .. }
            | Self::Function { span, .. } => *span,
        }
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

    pub fn ty(&self) -> &TypeSyntax {
        &self.ty
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BorrowMode {
    Owned,
    Shared,
    Mutable,
}

#[derive(Debug, Eq, PartialEq)]
pub struct FunctionParameter {
    name: Span,
    ty: TypeSyntax,
    borrow_mode: BorrowMode,
    span: Span,
}
impl FunctionParameter {
    pub fn name(&self) -> Span {
        self.name
    }
    pub fn ty(&self) -> &TypeSyntax {
        &self.ty
    }
    pub fn borrow_mode(&self) -> BorrowMode {
        self.borrow_mode
    }
    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct FunctionSignature {
    name: Span,
    parameters: Vec<FunctionParameter>,
    result: TypeSyntax,
    effects: Vec<Span>,
    span: Span,
}
impl FunctionSignature {
    pub fn name(&self) -> Span {
        self.name
    }
    pub fn parameters(&self) -> &[FunctionParameter] {
        &self.parameters
    }
    pub fn result(&self) -> &TypeSyntax {
        &self.result
    }
    pub fn effects(&self) -> &[Span] {
        &self.effects
    }
    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Block {
    statements: Vec<Statement>,
    span: Span,
}
impl Block {
    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }
    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatementForm {
    Let,
    Return,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpressionForm {
    Primary,
    Group,
    Unary,
    Binary,
    Call,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Expression {
    form: ExpressionForm,
    left: Option<Box<Expression>>,
    operator: Option<Span>,
    right: Option<Box<Expression>>,
    inner: Option<Box<Expression>>,
    callee: Option<Box<Expression>>,
    arguments: Vec<Expression>,
    span: Span,
}

impl Expression {
    pub fn form(&self) -> ExpressionForm {
        self.form
    }

    pub fn left(&self) -> Option<&Expression> {
        self.left.as_deref()
    }

    pub fn operator_text<'source>(&self, source: &'source SourceUnit) -> Option<&'source str> {
        self.operator.map(|operator| operator.text(source))
    }

    pub fn right(&self) -> Option<&Expression> {
        self.right.as_deref()
    }

    pub fn inner(&self) -> Option<&Expression> {
        self.inner.as_deref()
    }

    pub fn callee(&self) -> Option<&Expression> {
        self.callee.as_deref()
    }

    pub fn arguments(&self) -> &[Expression] {
        &self.arguments
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Statement {
    form: StatementForm,
    mutable: bool,
    binding: Option<Span>,
    declared_type: Option<TypeSyntax>,
    expression: Option<Expression>,
    span: Span,
}
impl Statement {
    pub fn form(&self) -> StatementForm {
        self.form
    }
    pub fn is_mutable(&self) -> bool {
        self.mutable
    }
    pub fn binding(&self) -> Option<Span> {
        self.binding
    }
    pub fn declared_type(&self) -> Option<&TypeSyntax> {
        self.declared_type.as_ref()
    }
    pub fn expression(&self) -> Option<&Expression> {
        self.expression.as_ref()
    }
    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct FunctionDeclaration {
    signature: FunctionSignature,
    body: Block,
}
impl FunctionDeclaration {
    pub fn signature(&self) -> &FunctionSignature {
        &self.signature
    }
    pub fn body(&self) -> &Block {
        &self.body
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Schema {
    outline: ModuleOutline,
    records: Vec<RecordDeclaration>,
    enums: Vec<EnumDeclaration>,
    extern_functions: Vec<FunctionSignature>,
    functions: Vec<FunctionDeclaration>,
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
    pub fn extern_functions(&self) -> &[FunctionSignature] {
        &self.extern_functions
    }
    pub fn functions(&self) -> &[FunctionDeclaration] {
        &self.functions
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
        let mut extern_functions = Vec::new();
        let mut functions = Vec::new();
        while matches!(parser.current_text(), "record" | "enum" | "extern" | "fn") {
            if parser.current_text() == "record" {
                records.push(parser.parse_record_declaration()?);
            } else if parser.current_text() == "enum" {
                enums.push(parser.parse_enum_declaration()?);
            } else {
                if parser.current_text() == "extern" {
                    extern_functions.push(parser.parse_extern_function()?);
                } else {
                    functions.push(parser.parse_function()?);
                }
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
            extern_functions,
            functions,
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
            types.push(self.parse_type()?);
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
            let ty = self.parse_type()?;
            let type_end = ty.span().end();
            fields.push(RecordField {
                name,
                ty,
                span: Span {
                    start: name.start(),
                    end: type_end,
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

    fn parse_function(&mut self) -> Result<FunctionDeclaration, ParseError> {
        let start = self.expect_word("fn", ParseErrorCode::UnexpectedToken)?;
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::OpenParen, ParseErrorCode::UnexpectedToken)?;
        let parameters = self.parse_parameters()?;
        self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
        self.expect_word("->", ParseErrorCode::UnexpectedToken)?;
        let result = self.parse_type()?;
        let effects = self.parse_effects()?;
        let body = self.parse_block()?;
        let signature = FunctionSignature {
            name,
            parameters,
            result,
            effects,
            span: Span {
                start: start.start(),
                end: body.span.start(),
            },
        };
        Ok(FunctionDeclaration { signature, body })
    }

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let start = self.expect_kind(TokenKind::OpenBrace, ParseErrorCode::UnexpectedToken)?;
        let mut statements = Vec::new();
        while self.current().kind() != TokenKind::CloseBrace {
            statements.push(self.parse_statement()?);
        }
        let end = self.expect_kind(TokenKind::CloseBrace, ParseErrorCode::UnexpectedToken)?;
        Ok(Block {
            statements,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        if self.current_text() == "let" {
            return self.parse_let_statement();
        }
        let start = Span::from(self.current());
        self.expect_word("return", ParseErrorCode::UnexpectedToken)?;
        let expression = if self.current().kind() == TokenKind::Semicolon {
            None
        } else {
            Some(self.parse_expression()?)
        };
        let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
        Ok(Statement {
            form: StatementForm::Return,
            mutable: false,
            binding: None,
            declared_type: None,
            expression,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_let_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect_word("let", ParseErrorCode::UnexpectedToken)?;
        let mutable = if self.current_text() == "mut" {
            self.advance();
            true
        } else {
            false
        };
        let binding = self.expect_identifier()?;
        let declared_type = if self.consume_kind(TokenKind::Colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect_kind(TokenKind::Equal, ParseErrorCode::UnexpectedToken)?;
        let expression = Some(self.parse_expression()?);
        let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
        Ok(Statement {
            form: StatementForm::Let,
            mutable,
            binding: Some(binding),
            declared_type,
            expression,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_binary_level(Self::parse_logical_and, &["||"])
    }

    fn parse_logical_and(&mut self) -> Result<Expression, ParseError> {
        self.parse_binary_level(Self::parse_equality, &["&&"])
    }

    fn parse_equality(&mut self) -> Result<Expression, ParseError> {
        self.parse_binary_level(Self::parse_comparison, &["==", "!="])
    }

    fn parse_comparison(&mut self) -> Result<Expression, ParseError> {
        self.parse_binary_level(Self::parse_bit_or, &["<", "<=", ">", ">="])
    }

    fn parse_bit_or(&mut self) -> Result<Expression, ParseError> {
        self.parse_binary_level(Self::parse_bit_xor, &["|"])
    }

    fn parse_bit_xor(&mut self) -> Result<Expression, ParseError> {
        self.parse_binary_level(Self::parse_bit_and, &["^"])
    }

    fn parse_bit_and(&mut self) -> Result<Expression, ParseError> {
        self.parse_binary_level(Self::parse_shift, &["&"])
    }

    fn parse_shift(&mut self) -> Result<Expression, ParseError> {
        self.parse_binary_level(Self::parse_sum, &["<<", ">>"])
    }

    fn parse_sum(&mut self) -> Result<Expression, ParseError> {
        self.parse_binary_level(Self::parse_product, &["+", "-"])
    }

    fn parse_product(&mut self) -> Result<Expression, ParseError> {
        self.parse_binary_level(Self::parse_unary_expression, &["*", "/", "%"])
    }

    fn parse_binary_level(
        &mut self,
        parse_operand: fn(&mut Self) -> Result<Expression, ParseError>,
        operators: &[&str],
    ) -> Result<Expression, ParseError> {
        let mut left = parse_operand(self)?;
        while operators.contains(&self.current_text()) {
            let operator = Span::from(self.advance());
            let right = parse_operand(self)?;
            left = Expression {
                form: ExpressionForm::Binary,
                span: Span {
                    start: left.span.start(),
                    end: right.span.end(),
                },
                left: Some(Box::new(left)),
                operator: Some(operator),
                right: Some(Box::new(right)),
                inner: None,
                callee: None,
                arguments: Vec::new(),
            };
        }
        Ok(left)
    }

    fn parse_unary_expression(&mut self) -> Result<Expression, ParseError> {
        let operator = if matches!(self.current_text(), "!" | "-" | "~" | "await" | "join") {
            Some(Span::from(self.advance()))
        } else if self.current_text() == "borrow" {
            let start = Span::from(self.advance());
            let end = if self.current_text() == "mut" {
                Span::from(self.advance())
            } else {
                start
            };
            Some(Span {
                start: start.start(),
                end: end.end(),
            })
        } else {
            None
        };
        if let Some(operator) = operator {
            let inner = self.parse_unary_expression()?;
            let end = inner.span.end();
            return Ok(Expression {
                form: ExpressionForm::Unary,
                left: None,
                operator: Some(operator),
                right: None,
                inner: Some(Box::new(inner)),
                callee: None,
                arguments: Vec::new(),
                span: Span {
                    start: operator.start(),
                    end,
                },
            });
        }
        self.parse_postfix_expression()
    }

    fn parse_postfix_expression(&mut self) -> Result<Expression, ParseError> {
        let mut callee = self.parse_primary_expression()?;
        while self.consume_kind(TokenKind::OpenParen).is_some() {
            let arguments = self.parse_call_arguments()?;
            let end = self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
            callee = Expression {
                form: ExpressionForm::Call,
                left: None,
                operator: None,
                right: None,
                inner: None,
                span: Span {
                    start: callee.span.start(),
                    end: end.end(),
                },
                callee: Some(Box::new(callee)),
                arguments,
            };
        }
        Ok(callee)
    }

    fn parse_call_arguments(&mut self) -> Result<Vec<Expression>, ParseError> {
        let mut arguments = Vec::new();
        if self.current().kind() == TokenKind::CloseParen {
            return Ok(arguments);
        }
        loop {
            arguments.push(self.parse_expression()?);
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
        Ok(arguments)
    }

    fn parse_primary_expression(&mut self) -> Result<Expression, ParseError> {
        if self.current().kind() == TokenKind::OpenParen {
            let start = Span::from(self.advance());
            let inner = self.parse_expression()?;
            let end = self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
            return Ok(Expression {
                form: ExpressionForm::Group,
                left: None,
                operator: None,
                right: None,
                inner: Some(Box::new(inner)),
                callee: None,
                arguments: Vec::new(),
                span: Span {
                    start: start.start(),
                    end: end.end(),
                },
            });
        }
        if matches!(
            self.current().kind(),
            TokenKind::Boolean
                | TokenKind::Integer
                | TokenKind::Size
                | TokenKind::Duration
                | TokenKind::String
                | TokenKind::Bytes
                | TokenKind::Identifier
        ) {
            let span = Span::from(self.advance());
            Ok(Expression {
                form: ExpressionForm::Primary,
                left: None,
                operator: None,
                right: None,
                inner: None,
                callee: None,
                arguments: Vec::new(),
                span,
            })
        } else {
            Err(self.error_here(ParseErrorCode::UnexpectedToken))
        }
    }

    fn parse_extern_function(&mut self) -> Result<FunctionSignature, ParseError> {
        let start = self.expect_word("extern", ParseErrorCode::UnexpectedToken)?;
        self.expect_word("fn", ParseErrorCode::UnexpectedToken)?;
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::OpenParen, ParseErrorCode::UnexpectedToken)?;
        let parameters = self.parse_parameters()?;
        self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
        self.expect_word("->", ParseErrorCode::UnexpectedToken)?;
        let result = self.parse_type()?;
        let effects = self.parse_effects()?;
        let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
        Ok(FunctionSignature {
            name,
            parameters,
            result,
            effects,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_parameters(&mut self) -> Result<Vec<FunctionParameter>, ParseError> {
        let mut parameters = Vec::new();
        if self.current().kind() == TokenKind::CloseParen {
            return Ok(parameters);
        }
        loop {
            let start = Span::from(self.current());
            let borrow_mode = if self.current_text() == "borrow" {
                self.advance();
                if self.current_text() == "mut" {
                    self.advance();
                    BorrowMode::Mutable
                } else {
                    BorrowMode::Shared
                }
            } else {
                BorrowMode::Owned
            };
            let name = self.expect_identifier()?;
            self.expect_kind(TokenKind::Colon, ParseErrorCode::UnexpectedToken)?;
            let ty = self.parse_type()?;
            let end = ty.span().end();
            parameters.push(FunctionParameter {
                name,
                ty,
                borrow_mode,
                span: Span {
                    start: start.start(),
                    end,
                },
            });
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
        Ok(parameters)
    }

    fn parse_effects(&mut self) -> Result<Vec<Span>, ParseError> {
        if self.current_text() != "uses" {
            return Ok(Vec::new());
        }
        self.advance();
        self.expect_kind(TokenKind::OpenBracket, ParseErrorCode::UnexpectedToken)?;
        let mut effects = Vec::new();
        if self.current().kind() != TokenKind::CloseBracket {
            loop {
                effects.push(self.expect_identifier()?);
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
        self.expect_kind(TokenKind::CloseBracket, ParseErrorCode::UnexpectedToken)?;
        Ok(effects)
    }

    fn parse_type(&mut self) -> Result<TypeSyntax, ParseError> {
        if self.consume_kind(TokenKind::OpenParen).is_some() {
            let start = self.tokens[self.index - 1].start();
            let elements = self.parse_tuple_type_list()?;
            let end = self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
            return Ok(TypeSyntax::Tuple {
                elements,
                span: Span {
                    start,
                    end: end.end(),
                },
            });
        }
        if self.current_text() == "fn" {
            let start = Span::from(self.advance());
            self.expect_kind(TokenKind::OpenParen, ParseErrorCode::UnexpectedToken)?;
            let parameters = self.parse_tuple_type_list()?;
            self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
            self.expect_word("->", ParseErrorCode::UnexpectedToken)?;
            let result = self.parse_type()?;
            return Ok(TypeSyntax::Function {
                parameters,
                span: Span {
                    start: start.start(),
                    end: result.span().end(),
                },
                result: Box::new(result),
            });
        }
        let path = self.parse_dotted_name()?;
        let start = path[0].start();
        let name = *path.last().expect("dotted type name is nonempty");
        if self.current_text() != "<" {
            return Ok(TypeSyntax::Name {
                path,
                span: Span {
                    start,
                    end: name.end(),
                },
            });
        }
        self.advance();
        if name.text(self.source) == "array" {
            let element = self.parse_type()?;
            self.expect_kind(TokenKind::Comma, ParseErrorCode::ListSeparatorRequired)?;
            let length = self.expect_literal()?;
            let end = self.expect_word(">", ParseErrorCode::UnexpectedToken)?;
            return Ok(TypeSyntax::Array {
                element: Box::new(element),
                length,
                span: Span {
                    start,
                    end: end.end(),
                },
            });
        }
        let mut arguments = Vec::new();
        loop {
            arguments.push(self.parse_type()?);
            if self.consume_kind(TokenKind::Comma).is_some() {
                continue;
            }
            break;
        }
        let end = self.expect_word(">", ParseErrorCode::UnexpectedToken)?;
        Ok(TypeSyntax::Constructed {
            name,
            arguments,
            span: Span {
                start,
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
