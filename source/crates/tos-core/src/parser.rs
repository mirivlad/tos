// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic TOS Core V1 syntax parsing.
//!
//! This module owns syntax-tree construction only. Name, type, effect and
//! resource decisions belong to later frontend stages.
//!
//! Parsing is deterministic and recovering, as required by docs/39 section 4: a
//! lexical failure is reported alone, and a syntax failure produces exactly one
//! diagnostic per synchronization region before the parser resumes at that
//! region's boundary. The parser never guesses a missing declaration,
//! capability, type or operator — a region that failed contributes no tree.

use std::boxed::Box;
use std::vec::Vec;

use crate::{Diagnostic, LexError, Lexer, Severity, SourceUnit, Stage, Token, TokenKind};

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
    Assignment,
    Expression,
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
    target: Option<Expression>,
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
    pub fn target(&self) -> Option<&Expression> {
        self.target.as_ref()
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

/// Internal error signal. The parser recovers from these at a synchronization
/// region boundary and reports each one as a [`Diagnostic`]; it is never a
/// public parse result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParseErrorCode {
    ExpectedModuleHeader,
    ExpectedIdentifier,
    ExpectedVersionComponent,
    ExpectedProfile,
    UnexpectedToken,
    ExpectedLiteral,
    ListSeparatorRequired,
}

impl ParseErrorCode {
    /// Stable symbolic diagnostic code from the registry in docs/44 section 7.
    ///
    /// `E1107_UNEXPECTED_TOKEN` is the registered residual of the parse stage:
    /// it is correct only where no more specific parser code applies.
    fn symbol(self) -> &'static str {
        match self {
            ParseErrorCode::ExpectedModuleHeader => "E1100_EXPECTED_MODULE_HEADER",
            ParseErrorCode::ExpectedIdentifier => "E1101_EXPECTED_IDENTIFIER",
            ParseErrorCode::ExpectedVersionComponent => "E1102_EXPECTED_VERSION_COMPONENT",
            ParseErrorCode::ExpectedProfile => "E1103_EXPECTED_PROFILE",
            ParseErrorCode::ExpectedLiteral => "E1104_EXPECTED_LITERAL",
            ParseErrorCode::ListSeparatorRequired => "E1106_LIST_SEPARATOR_REQUIRED",
            ParseErrorCode::UnexpectedToken => "E1107_UNEXPECTED_TOKEN",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ParseError {
    code: ParseErrorCode,
    span: Span,
}

/// Synchronization region in which an error was detected, as defined by
/// docs/39 section 4.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Region {
    Declaration,
    Statement,
    List,
}

impl Region {
    fn symbol(self) -> &'static str {
        match self {
            Region::Declaration => "declaration",
            Region::Statement => "statement",
            Region::List => "list",
        }
    }
}

/// Which token closes the list currently being parsed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ListCloser {
    Kind(TokenKind),
    /// `>` closing a type-argument list. The lexer emits `>` as an operator
    /// token, so it has no dedicated [`TokenKind`].
    Angle,
}

/// Result of one parse: the tree that could be built without guessing, and
/// every diagnostic produced along the way.
///
/// A partial tree may be present alongside errors, because docs/39 requires the
/// parser to recover and keep reporting. It is never a valid module in that
/// state — use [`ParseOutcome::into_accepted`] to obtain a tree only when the
/// source parsed cleanly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseOutcome<T> {
    value: Option<T>,
    diagnostics: Vec<Diagnostic>,
}

impl<T> ParseOutcome<T> {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error)
    }

    /// The tree the parser managed to build, which may be partial when
    /// [`ParseOutcome::has_errors`] is true.
    pub fn value(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// The tree, but only when the source produced no error diagnostic.
    pub fn into_accepted(self) -> Option<T> {
        if self.has_errors() {
            return None;
        }
        self.value
    }

    fn failed(diagnostic: Diagnostic) -> ParseOutcome<T> {
        ParseOutcome {
            value: None,
            diagnostics: vec![diagnostic],
        }
    }
}

pub struct Parser;

impl Parser {
    pub fn parse_header(source: &SourceUnit) -> ParseOutcome<ModuleHeader> {
        Parser::run(source, |cursor| {
            let header = cursor.parse_header();
            cursor.finish_region(header, Region::Declaration)
        })
    }

    pub fn parse_prefix(source: &SourceUnit) -> ParseOutcome<ModulePrefix> {
        Parser::run(source, |cursor| {
            let prefix = cursor.parse_module_prefix()?;
            cursor.expect_end_of_source();
            Some(prefix)
        })
    }

    pub fn parse_outline(source: &SourceUnit) -> ParseOutcome<ModuleOutline> {
        Parser::run(source, |cursor| {
            let outline = cursor.parse_module_outline()?;
            cursor.expect_end_of_source();
            Some(outline)
        })
    }

    pub fn parse_schema(source: &SourceUnit) -> ParseOutcome<Schema> {
        Parser::run(source, |cursor| cursor.parse_schema_body())
    }

    fn run<T>(
        source: &SourceUnit,
        parse: impl FnOnce(&mut TokenCursor) -> Option<T>,
    ) -> ParseOutcome<T> {
        let tokens = match Lexer::lex(source) {
            Ok(tokens) => tokens,
            Err(error) => return ParseOutcome::failed(lexical_diagnostic(error, source)),
        };
        let mut cursor = TokenCursor {
            source,
            tokens,
            index: 0,
            diagnostics: Vec::new(),
        };
        let value = parse(&mut cursor);
        ParseOutcome {
            value,
            diagnostics: cursor.diagnostics,
        }
    }
}

struct TokenCursor<'source> {
    source: &'source SourceUnit,
    tokens: Vec<Token>,
    index: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'source> TokenCursor<'source> {
    fn parse_module_prefix(&mut self) -> Option<ModulePrefix> {
        let header = self.parse_header();
        let header = self.finish_region(header, Region::Declaration)?;
        let mut imports = Vec::new();
        while self.current_text() == "import" {
            let import = self.parse_import();
            if let Some(import) = self.finish_region(import, Region::Declaration) {
                imports.push(import);
            }
        }
        Some(ModulePrefix { header, imports })
    }

    fn parse_module_outline(&mut self) -> Option<ModuleOutline> {
        let prefix = self.parse_module_prefix()?;
        let resource = self.parse_resource_declaration();
        let resource = self.finish_region(resource, Region::Declaration)?;
        Some(ModuleOutline { prefix, resource })
    }

    fn parse_schema_body(&mut self) -> Option<Schema> {
        let outline = self.parse_module_outline();
        let mut records = Vec::new();
        let mut enums = Vec::new();
        let mut extern_functions = Vec::new();
        let mut functions = Vec::new();
        while self.current().kind() != TokenKind::Eof {
            match self.current_text() {
                "record" => {
                    let declaration = self.parse_record_declaration();
                    if let Some(declaration) = self.finish_region(declaration, Region::Declaration)
                    {
                        records.push(declaration);
                    }
                }
                "enum" => {
                    let declaration = self.parse_enum_declaration();
                    if let Some(declaration) = self.finish_region(declaration, Region::Declaration)
                    {
                        enums.push(declaration);
                    }
                }
                "extern" => {
                    let signature = self.parse_extern_function();
                    if let Some(signature) = self.finish_region(signature, Region::Declaration) {
                        extern_functions.push(signature);
                    }
                }
                "fn" => {
                    let declaration = self.parse_function();
                    if let Some(declaration) = self.finish_region(declaration, Region::Declaration)
                    {
                        functions.push(declaration);
                    }
                }
                // docs/39 forbids guessing a missing declaration, so an
                // unrecognized item head is reported and its region skipped.
                _ => {
                    let error = self.error_here(ParseErrorCode::UnexpectedToken);
                    self.report(error, Region::Declaration);
                    self.synchronize(Region::Declaration);
                }
            }
        }
        let outline = outline?;
        Some(Schema {
            outline,
            records,
            enums,
            extern_functions,
            functions,
        })
    }

    fn expect_end_of_source(&mut self) {
        while self.current().kind() != TokenKind::Eof {
            let error = self.error_here(ParseErrorCode::UnexpectedToken);
            self.report(error, Region::Declaration);
            self.synchronize(Region::Declaration);
        }
    }

    /// Reports and synchronizes when a region failed, yielding `None`.
    fn finish_region<T>(&mut self, result: Result<T, ParseError>, region: Region) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.report(error, region);
                self.synchronize(region);
                None
            }
        }
    }

    fn report(&mut self, error: ParseError, region: Region) {
        let diagnostic = Diagnostic::new(
            error.code.symbol(),
            Severity::Error,
            Stage::Parse,
            error.span,
            self.source,
        )
        .with_field("region", region.symbol())
        .with_field("found", self.describe(error.span));
        self.diagnostics.push(diagnostic);
    }

    /// Names the source text a diagnostic points at, for the `found` field.
    fn describe(&self, span: Span) -> &'source str {
        if span.start() >= self.source.bytes().len() {
            return "<end of source>";
        }
        span.text(self.source)
    }

    /// Skips to the end of the failed region, per docs/39 section 4.
    ///
    /// A declaration region ends after the next top-level `;` or `]`. A
    /// statement region ends after the next `;`, or at the closing brace of the
    /// current block, which is left unconsumed for the block loop to see.
    /// Nesting is tracked so that a delimiter inside a nested construct does
    /// not end the outer region early.
    ///
    /// A declaration region also ends at the `}` that closes a top-level
    /// declaration body and returns delimiter nesting to zero (docs/39 section
    /// 4, ADR-0032): a `fn` declaration ends with a block rather than `;` or
    /// `]`, so without that boundary one malformed signature would discard
    /// every later declaration in the source unit.
    fn synchronize(&mut self, region: Region) {
        let mut depth = 0usize;
        loop {
            let kind = self.current().kind();
            match kind {
                TokenKind::Eof => return,
                TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::OpenBrace => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::CloseBracket if region == Region::Declaration && depth == 0 => {
                    self.advance();
                    return;
                }
                TokenKind::CloseBrace if region == Region::Statement && depth == 0 => return,
                TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace => {
                    let closes_declaration_body = region == Region::Declaration
                        && depth == 1
                        && kind == TokenKind::CloseBrace;
                    depth = depth.saturating_sub(1);
                    self.advance();
                    if closes_declaration_body {
                        return;
                    }
                }
                TokenKind::Semicolon => {
                    self.advance();
                    if depth == 0 {
                        return;
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Skips to the next element of a failed list, per docs/39 section 4.
    ///
    /// Returns `true` when a separator was crossed and another element may
    /// follow, and `false` when the list ended at its closer or at end of
    /// source.
    fn synchronize_list(&mut self, closer: ListCloser) -> bool {
        let mut depth = 0usize;
        loop {
            if depth == 0 && self.at_list_closer(closer) {
                return false;
            }
            match self.current().kind() {
                TokenKind::Eof => return false,
                TokenKind::Comma if depth == 0 => {
                    self.advance();
                    return !self.at_list_closer(closer);
                }
                TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::OpenBrace => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                    self.advance();
                }
                _ => {
                    if closer == ListCloser::Angle {
                        match self.current_text() {
                            "<" => depth += 1,
                            ">" => depth = depth.saturating_sub(1),
                            ">>" => depth = depth.saturating_sub(2),
                            _ => {}
                        }
                    }
                    self.advance();
                }
            }
        }
    }

    fn at_list_closer(&self, closer: ListCloser) -> bool {
        match closer {
            ListCloser::Kind(kind) => self.current().kind() == kind,
            // `Option<Option<i32>>` ends both argument lists at one `>>` token.
            ListCloser::Angle => matches!(self.current_text(), ">" | ">>"),
        }
    }

    /// Consumes the `>` that closes a type-argument list.
    ///
    /// The lexer emits `>>` as a single shift operator, so a type argument list
    /// nested directly inside another ends at half a token. This splits the
    /// shift token: the leading `>` closes the inner list and the trailing `>`
    /// stays in the stream for the enclosing one.
    fn expect_close_angle(&mut self) -> Result<Span, ParseError> {
        let token = self.current();
        if token.kind() == TokenKind::Operator {
            match token.text(self.source) {
                ">" => return Ok(Span::from(self.advance())),
                ">>" => {
                    self.tokens[self.index] = Token {
                        kind: TokenKind::Operator,
                        start: token.start() + 1,
                        end: token.end(),
                    };
                    return Ok(Span {
                        start: token.start(),
                        end: token.start() + 1,
                    });
                }
                _ => {}
            }
        }
        Err(self.error_here(ParseErrorCode::UnexpectedToken))
    }

    /// Parses a comma-separated list, recovering at list level so that one
    /// malformed element does not discard the rest of the list.
    ///
    /// A trailing comma is permitted in every V1 list (docs/39 section 5).
    fn parse_comma_list<T>(
        &mut self,
        closer: ListCloser,
        parse_element: fn(&mut Self) -> Result<T, ParseError>,
    ) -> Vec<T> {
        let mut items = Vec::new();
        if self.at_list_closer(closer) {
            return items;
        }
        loop {
            match parse_element(self) {
                Ok(item) => items.push(item),
                Err(error) => {
                    self.report(error, Region::List);
                    if self.synchronize_list(closer) {
                        continue;
                    }
                    break;
                }
            }
            if self.consume_kind(TokenKind::Comma).is_some() {
                if self.at_list_closer(closer) {
                    break;
                }
                continue;
            }
            if !self.at_list_closer(closer) {
                let error = self.error_here(ParseErrorCode::ListSeparatorRequired);
                self.report(error, Region::List);
                if self.synchronize_list(closer) {
                    continue;
                }
                break;
            }
            break;
        }
        items
    }
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
        let limits = self.parse_comma_list(
            ListCloser::Kind(TokenKind::CloseBracket),
            Self::parse_resource_limit,
        );
        let end = self.expect_kind(TokenKind::CloseBracket, ParseErrorCode::UnexpectedToken)?;
        Ok(ResourceDeclaration {
            limits,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_resource_limit(&mut self) -> Result<ResourceLimit, ParseError> {
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::Colon, ParseErrorCode::UnexpectedToken)?;
        let value = self.expect_literal()?;
        Ok(ResourceLimit {
            name,
            value,
            span: Span {
                start: name.start(),
                end: value.end(),
            },
        })
    }

    fn parse_record_declaration(&mut self) -> Result<RecordDeclaration, ParseError> {
        let start = self.expect_word("record", ParseErrorCode::UnexpectedToken)?;
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::OpenBracket, ParseErrorCode::UnexpectedToken)?;
        let fields = self.parse_field_list();
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
        let variants = self.parse_comma_list(
            ListCloser::Kind(TokenKind::CloseBracket),
            Self::parse_enum_variant,
        );
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
            let tuple_types = self.parse_tuple_type_list();
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
            let fields = self.parse_field_list();
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

    fn parse_tuple_type_list(&mut self) -> Vec<TypeSyntax> {
        self.parse_comma_list(ListCloser::Kind(TokenKind::CloseParen), Self::parse_type)
    }

    fn parse_field_list(&mut self) -> Vec<RecordField> {
        self.parse_comma_list(
            ListCloser::Kind(TokenKind::CloseBracket),
            Self::parse_record_field,
        )
    }

    fn parse_record_field(&mut self) -> Result<RecordField, ParseError> {
        if self.current_text() == "pub" {
            self.advance();
        }
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::Colon, ParseErrorCode::UnexpectedToken)?;
        let ty = self.parse_type()?;
        let type_end = ty.span().end();
        Ok(RecordField {
            name,
            ty,
            span: Span {
                start: name.start(),
                end: type_end,
            },
        })
    }

    fn parse_function(&mut self) -> Result<FunctionDeclaration, ParseError> {
        let start = self.expect_word("fn", ParseErrorCode::UnexpectedToken)?;
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::OpenParen, ParseErrorCode::UnexpectedToken)?;
        let parameters = self.parse_parameters();
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
        while !matches!(
            self.current().kind(),
            TokenKind::CloseBrace | TokenKind::Eof
        ) {
            let statement = self.parse_statement();
            if let Some(statement) = self.finish_region(statement, Region::Statement) {
                statements.push(statement);
            }
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
        if self.current_text() != "return" {
            return self.parse_assignment_or_expression_statement();
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
            target: None,
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
            target: None,
            expression,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_assignment_or_expression_statement(&mut self) -> Result<Statement, ParseError> {
        let start = Span::from(self.current());
        let target_or_expression = self.parse_expression()?;
        if self.consume_kind(TokenKind::Equal).is_some() {
            let expression = self.parse_expression()?;
            let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
            return Ok(Statement {
                form: StatementForm::Assignment,
                mutable: false,
                binding: None,
                declared_type: None,
                target: Some(target_or_expression),
                expression: Some(expression),
                span: Span {
                    start: start.start(),
                    end: end.end(),
                },
            });
        }
        let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
        Ok(Statement {
            form: StatementForm::Expression,
            mutable: false,
            binding: None,
            declared_type: None,
            target: None,
            expression: Some(target_or_expression),
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
            let arguments = self.parse_call_arguments();
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

    fn parse_call_arguments(&mut self) -> Vec<Expression> {
        self.parse_comma_list(
            ListCloser::Kind(TokenKind::CloseParen),
            Self::parse_expression,
        )
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
        let parameters = self.parse_parameters();
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

    fn parse_parameters(&mut self) -> Vec<FunctionParameter> {
        self.parse_comma_list(
            ListCloser::Kind(TokenKind::CloseParen),
            Self::parse_parameter,
        )
    }

    fn parse_parameter(&mut self) -> Result<FunctionParameter, ParseError> {
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
        Ok(FunctionParameter {
            name,
            ty,
            borrow_mode,
            span: Span {
                start: start.start(),
                end,
            },
        })
    }

    fn parse_effects(&mut self) -> Result<Vec<Span>, ParseError> {
        if self.current_text() != "uses" {
            return Ok(Vec::new());
        }
        self.advance();
        self.expect_kind(TokenKind::OpenBracket, ParseErrorCode::UnexpectedToken)?;
        let effects = self.parse_comma_list(
            ListCloser::Kind(TokenKind::CloseBracket),
            Self::expect_identifier,
        );
        self.expect_kind(TokenKind::CloseBracket, ParseErrorCode::UnexpectedToken)?;
        Ok(effects)
    }

    fn parse_type(&mut self) -> Result<TypeSyntax, ParseError> {
        if self.consume_kind(TokenKind::OpenParen).is_some() {
            let start = self.tokens[self.index - 1].start();
            let elements = self.parse_tuple_type_list();
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
            let parameters = self.parse_tuple_type_list();
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
            let end = self.expect_close_angle()?;
            return Ok(TypeSyntax::Array {
                element: Box::new(element),
                length,
                span: Span {
                    start,
                    end: end.end(),
                },
            });
        }
        let arguments = self.parse_comma_list(ListCloser::Angle, Self::parse_type);
        let end = self.expect_close_angle()?;
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

/// Builds the single lexical diagnostic that ends a parse before it starts.
///
/// docs/39 section 4 requires the lowest-numbered applicable lexical error to
/// be emitted first; the lexer stops at the first offending byte, so a failed
/// lex yields exactly this diagnostic and no parse diagnostics.
fn lexical_diagnostic(error: LexError, source: &SourceUnit) -> Diagnostic {
    let span = Span {
        start: error.byte_offset(),
        end: error.byte_offset(),
    };
    Diagnostic::new(
        error.code().symbol(),
        Severity::Error,
        Stage::Lex,
        span,
        source,
    )
    .with_field("byte_offset", error.byte_offset())
}
