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

use alloc::boxed::Box;
use alloc::vec::Vec;

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

/// Item visibility. `pub` exports an item from its module; the absence of the
/// marker keeps it module-private (docs/39 section 5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Visibility {
    Private,
    Public,
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
        /// `mut` written inside the type argument (ADR-0037).
        ///
        /// The grammar admits it for `Region` and `DmaRegion` only, and it is
        /// not a general mutability qualifier: a region's rights are part of
        /// its type because the four facts of ADR-0037 section 2 differ between
        /// the granted modes, and a type that did not carry the mode would make
        /// them unknowable at the point they matter.
        mutable: bool,
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
    visibility: Visibility,
    name: Span,
    fields: Vec<RecordField>,
    span: Span,
}

impl RecordDeclaration {
    pub fn visibility(&self) -> Visibility {
        self.visibility
    }

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
    visibility: Visibility,
    name: Span,
    variants: Vec<EnumVariant>,
    span: Span,
}

impl EnumDeclaration {
    pub fn visibility(&self) -> Visibility {
        self.visibility
    }

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
    visibility: Visibility,
    is_async: bool,
    name: Span,
    parameters: Vec<FunctionParameter>,
    result: TypeSyntax,
    effects: Vec<Span>,
    span: Span,
}
impl FunctionSignature {
    pub fn visibility(&self) -> Visibility {
        self.visibility
    }
    /// Whether the declaration carries the `async` marker.
    pub fn is_async(&self) -> bool {
        self.is_async
    }
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

/// Binding pattern form (docs/39 section 5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternForm {
    /// `_`
    Wildcard,
    /// `name`, including a predeclared value such as `None`
    Name,
    /// `name(p, q)`, matching a tuple variant
    Destructure,
    /// `(p, q)`, matching a tuple value
    Tuple,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Pattern {
    form: PatternForm,
    path: Vec<Span>,
    elements: Vec<Pattern>,
    span: Span,
}

impl Pattern {
    pub fn form(&self) -> PatternForm {
        self.form
    }

    /// The path of a `Name` or `Destructure` pattern.
    ///
    /// A single segment may be a constructor or a binding; a path with more
    /// than one segment is always a constructor path (ADR-0033). Which one a
    /// single segment is depends on the pattern's expected type and is decided
    /// by the checker, not here.
    pub fn path(&self) -> &[Span] {
        &self.path
    }

    /// The last segment of the path, which names the constructor or binding.
    pub fn name(&self) -> Option<Span> {
        self.path.last().copied()
    }

    /// Whether the pattern was written as a qualified constructor path.
    pub fn is_qualified(&self) -> bool {
        self.path.len() > 1
    }

    /// The sub-patterns of a `Destructure` or `Tuple` pattern.
    pub fn elements(&self) -> &[Pattern] {
        &self.elements
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
    If,
    Match,
    While,
    For,
    Loop,
    Break,
    Continue,
    Parallel,
    Cancel,
    Defer,
    Unsafe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpressionForm {
    /// A literal value.
    Literal,
    /// An identifier, including a predeclared value or function name.
    Name,
    Group,
    Unary,
    Binary,
    Call,
    Field,
    Index,
    Question,
    Cast,
    Tuple,
    Array,
    Closure,
    Spawn,
}

/// One argument of a call or constructor.
///
/// docs/39 section 5 gives calls a single Call/Construct form whose arguments
/// are either all positional or all named; `name` is `Some` exactly for the
/// named form.
#[derive(Debug, Eq, PartialEq)]
pub struct CallArgument {
    name: Option<Span>,
    value: Expression,
    span: Span,
}

impl CallArgument {
    pub fn name(&self) -> Option<Span> {
        self.name
    }

    pub fn value(&self) -> &Expression {
        &self.value
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Expression {
    form: ExpressionForm,
    left: Option<Box<Expression>>,
    operator: Option<Span>,
    right: Option<Box<Expression>>,
    inner: Option<Box<Expression>>,
    callee: Option<Box<Expression>>,
    arguments: Vec<CallArgument>,
    elements: Vec<Expression>,
    parameters: Vec<FunctionParameter>,
    body: Option<Box<Block>>,
    name: Option<Span>,
    cast_type: Option<TypeSyntax>,
    span: Span,
}

/// Takes an expression tree apart without recursing along an operator chain.
///
/// A left-associative chain is as deep as it is long, and so is a run of prefix
/// operators. The compiler's generated drop glue follows `left` and `inner`
/// recursively, so freeing a conforming 256 KiB source unit could overflow the
/// stack even after every *walk* over it had been made iterative — a crash on
/// the way out rather than on the way in, and just as far from the structured
/// rejection docs/44 section 2 requires.
///
/// Only the boxed single children are unwound here. `arguments`, `elements` and
/// `body` are dropped by the ordinary glue, because those nest only where the
/// source nests and the delimiter-nesting limit already bounds them.
impl Drop for Expression {
    fn drop(&mut self) {
        let mut pending: Vec<Box<Expression>> = Vec::new();
        let detach = |expression: &mut Expression, pending: &mut Vec<Box<Expression>>| {
            for slot in [
                &mut expression.left,
                &mut expression.right,
                &mut expression.inner,
                &mut expression.callee,
            ] {
                if let Some(child) = slot.take() {
                    pending.push(child);
                }
            }
        };
        detach(self, &mut pending);
        while let Some(mut node) = pending.pop() {
            detach(&mut node, &mut pending);
            // `node` goes out of scope with its boxed children already taken,
            // so its own drop reaches no further.
        }
    }
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

    pub fn arguments(&self) -> &[CallArgument] {
        &self.arguments
    }

    /// The field name of a `Field` expression.
    pub fn name(&self) -> Option<Span> {
        self.name
    }

    /// The target type of a `Cast` expression.
    pub fn cast_type(&self) -> Option<&TypeSyntax> {
        self.cast_type.as_ref()
    }

    /// The members of a `Tuple` or `Array` expression.
    pub fn elements(&self) -> &[Expression] {
        &self.elements
    }

    /// The parameters of a `Closure` expression.
    pub fn parameters(&self) -> &[FunctionParameter] {
        &self.parameters
    }

    /// The executable block of a `Closure` or `Spawn` expression.
    pub fn body(&self) -> Option<&Block> {
        self.body.as_deref()
    }

    /// An expression node carrying only its form and span, for builders that
    /// fill in the parts their form actually uses.
    fn node(form: ExpressionForm, span: Span) -> Expression {
        Expression {
            form,
            left: None,
            operator: None,
            right: None,
            inner: None,
            callee: None,
            arguments: Vec::new(),
            elements: Vec::new(),
            parameters: Vec::new(),
            body: None,
            name: None,
            cast_type: None,
            span,
        }
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct MatchBranch {
    pattern: Pattern,
    body: Block,
    span: Span,
}

impl MatchBranch {
    pub fn pattern(&self) -> &Pattern {
        &self.pattern
    }

    pub fn body(&self) -> &Block {
        &self.body
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Statement {
    form: StatementForm,
    mutable: bool,
    pattern: Option<Pattern>,
    declared_type: Option<TypeSyntax>,
    target: Option<Expression>,
    expression: Option<Expression>,
    body: Option<Block>,
    else_body: Option<Block>,
    else_if: Option<Box<Statement>>,
    branches: Vec<MatchBranch>,
    span: Span,
}
impl Statement {
    pub fn form(&self) -> StatementForm {
        self.form
    }

    /// The executable block of a compound statement: the taken branch of `if`,
    /// or the body of `while`, `for` and `loop`.
    pub fn body(&self) -> Option<&Block> {
        self.body.as_ref()
    }

    /// The `else` block of an `if` statement, when the alternative is a block.
    pub fn else_body(&self) -> Option<&Block> {
        self.else_body.as_ref()
    }

    /// The `else if` continuation of an `if` statement.
    pub fn else_if(&self) -> Option<&Statement> {
        self.else_if.as_deref()
    }

    /// The branches of a `match` statement.
    pub fn branches(&self) -> &[MatchBranch] {
        &self.branches
    }

    /// A statement node carrying only its form and span.
    fn node(form: StatementForm, span: Span) -> Statement {
        Statement {
            form,
            mutable: false,
            pattern: None,
            declared_type: None,
            target: None,
            expression: None,
            body: None,
            else_body: None,
            else_if: None,
            branches: Vec::new(),
            span,
        }
    }
    pub fn is_mutable(&self) -> bool {
        self.mutable
    }
    /// The bound pattern of a `Let` statement.
    pub fn pattern(&self) -> Option<&Pattern> {
        self.pattern.as_ref()
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
pub struct ConstDeclaration {
    visibility: Visibility,
    name: Span,
    ty: TypeSyntax,
    value: Expression,
    span: Span,
}

impl ConstDeclaration {
    pub fn visibility(&self) -> Visibility {
        self.visibility
    }

    pub fn name(&self) -> Span {
        self.name
    }

    pub fn ty(&self) -> &TypeSyntax {
        &self.ty
    }

    pub fn value(&self) -> &Expression {
        &self.value
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
    consts: Vec<ConstDeclaration>,
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
    pub fn consts(&self) -> &[ConstDeclaration] {
        &self.consts
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
    ControlHeadParensRequired,
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
            ParseErrorCode::ControlHeadParensRequired => "E1105_CONTROL_HEAD_PARENS_REQUIRED",
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
    truncated: bool,
}

impl<T> ParseOutcome<T> {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether the retention bound of docs/44 section 2 dropped diagnostics.
    ///
    /// The reported ones are the earliest, so a truncated list still starts at
    /// the first problem in the source.
    pub fn is_truncated(&self) -> bool {
        self.truncated
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
            diagnostics: alloc::vec![diagnostic],
            truncated: false,
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
            truncated: false,
        };
        let value = parse(&mut cursor);
        ParseOutcome {
            value,
            diagnostics: cursor.diagnostics,
            truncated: cursor.truncated,
        }
    }
}

struct TokenCursor<'source> {
    source: &'source SourceUnit,
    tokens: Vec<Token>,
    index: usize,
    diagnostics: Vec<Diagnostic>,
    truncated: bool,
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
        let mut consts = Vec::new();
        let mut extern_functions = Vec::new();
        let mut functions = Vec::new();
        while self.current().kind() != TokenKind::Eof {
            // `visibility? item`: the marker is consumed before the item head
            // is known, so every item form receives it.
            let visibility = if self.current_text() == "pub" {
                self.advance();
                Visibility::Public
            } else {
                Visibility::Private
            };
            match self.current_text() {
                "record" => {
                    let declaration = self.parse_record_declaration(visibility);
                    if let Some(declaration) = self.finish_region(declaration, Region::Declaration)
                    {
                        records.push(declaration);
                    }
                }
                "enum" => {
                    let declaration = self.parse_enum_declaration(visibility);
                    if let Some(declaration) = self.finish_region(declaration, Region::Declaration)
                    {
                        enums.push(declaration);
                    }
                }
                "const" => {
                    let declaration = self.parse_const_declaration(visibility);
                    if let Some(declaration) = self.finish_region(declaration, Region::Declaration)
                    {
                        consts.push(declaration);
                    }
                }
                "extern" => {
                    let signature = self.parse_extern_function(visibility);
                    if let Some(signature) = self.finish_region(signature, Region::Declaration) {
                        extern_functions.push(signature);
                    }
                }
                "fn" | "async" => {
                    let declaration = self.parse_function(visibility);
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
            consts,
            extern_functions,
            functions,
        })
    }

    fn parse_const_declaration(
        &mut self,
        visibility: Visibility,
    ) -> Result<ConstDeclaration, ParseError> {
        let start = self.expect_word("const", ParseErrorCode::UnexpectedToken)?;
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::Colon, ParseErrorCode::UnexpectedToken)?;
        let ty = self.parse_type()?;
        self.expect_kind(TokenKind::Equal, ParseErrorCode::UnexpectedToken)?;
        let value = self.parse_expression()?;
        let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
        Ok(ConstDeclaration {
            visibility,
            name,
            ty,
            value,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
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
        if self.diagnostics.len() >= crate::MAX_DIAGNOSTICS_PER_MODULE {
            // Recovery keeps running so the parse still terminates cleanly.
            self.truncated = true;
            return;
        }
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
        let mut name = alloc::vec![self.expect_identifier()?];
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

    fn parse_record_declaration(
        &mut self,
        visibility: Visibility,
    ) -> Result<RecordDeclaration, ParseError> {
        let start = self.expect_word("record", ParseErrorCode::UnexpectedToken)?;
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::OpenBracket, ParseErrorCode::UnexpectedToken)?;
        let fields = self.parse_field_list();
        let end = self.expect_kind(TokenKind::CloseBracket, ParseErrorCode::UnexpectedToken)?;
        Ok(RecordDeclaration {
            visibility,
            name,
            fields,
            span: Span {
                start: start.start(),
                end: end.end(),
            },
        })
    }

    fn parse_enum_declaration(
        &mut self,
        visibility: Visibility,
    ) -> Result<EnumDeclaration, ParseError> {
        let start = self.expect_word("enum", ParseErrorCode::UnexpectedToken)?;
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::OpenBracket, ParseErrorCode::UnexpectedToken)?;
        let variants = self.parse_comma_list(
            ListCloser::Kind(TokenKind::CloseBracket),
            Self::parse_enum_variant,
        );
        let end = self.expect_kind(TokenKind::CloseBracket, ParseErrorCode::UnexpectedToken)?;
        Ok(EnumDeclaration {
            visibility,
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

    fn parse_function(
        &mut self,
        visibility: Visibility,
    ) -> Result<FunctionDeclaration, ParseError> {
        let async_marker = if self.current_text() == "async" {
            Some(Span::from(self.advance()))
        } else {
            None
        };
        let fn_word = self.expect_word("fn", ParseErrorCode::UnexpectedToken)?;
        let start = async_marker.unwrap_or(fn_word);
        let name = self.expect_identifier()?;
        self.expect_kind(TokenKind::OpenParen, ParseErrorCode::UnexpectedToken)?;
        let parameters = self.parse_parameters();
        self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
        self.expect_word("->", ParseErrorCode::UnexpectedToken)?;
        let result = self.parse_type()?;
        let effects = self.parse_effects()?;
        let body = self.parse_block()?;
        let signature = FunctionSignature {
            visibility,
            is_async: async_marker.is_some(),
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

    /// Consumes the parenthesized head of a control statement.
    ///
    /// docs/39 section 5 requires the parentheses so that the head has an
    /// explicit boundary and cannot be confused with a record construction;
    /// their absence is `E1105_CONTROL_HEAD_PARENS_REQUIRED`.
    fn parse_control_head(&mut self) -> Result<Expression, ParseError> {
        if self.current().kind() != TokenKind::OpenParen {
            return Err(self.error_here(ParseErrorCode::ControlHeadParensRequired));
        }
        self.advance();
        let head = self.parse_expression()?;
        self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
        Ok(head)
    }

    fn parse_if_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect_word("if", ParseErrorCode::UnexpectedToken)?;
        let head = self.parse_control_head()?;
        let body = self.parse_block()?;
        let mut end = body.span.end();
        let mut else_body = None;
        let mut else_if = None;
        if self.current_text() == "else" {
            self.advance();
            if self.current_text() == "if" {
                let nested = self.parse_if_statement()?;
                end = nested.span.end();
                else_if = Some(Box::new(nested));
            } else {
                let block = self.parse_block()?;
                end = block.span.end();
                else_body = Some(block);
            }
        }
        Ok(Statement {
            expression: Some(head),
            body: Some(body),
            else_body,
            else_if,
            ..Statement::node(
                StatementForm::If,
                Span {
                    start: start.start(),
                    end,
                },
            )
        })
    }

    fn parse_match_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect_word("match", ParseErrorCode::UnexpectedToken)?;
        let head = self.parse_control_head()?;
        self.expect_kind(TokenKind::OpenBrace, ParseErrorCode::UnexpectedToken)?;
        let mut branches = Vec::new();
        while !matches!(
            self.current().kind(),
            TokenKind::CloseBrace | TokenKind::Eof
        ) {
            let branch = self.parse_match_branch();
            if let Some(branch) = self.finish_region(branch, Region::Statement) {
                branches.push(branch);
            }
        }
        let end = self.expect_kind(TokenKind::CloseBrace, ParseErrorCode::UnexpectedToken)?;
        Ok(Statement {
            expression: Some(head),
            branches,
            ..Statement::node(
                StatementForm::Match,
                Span {
                    start: start.start(),
                    end: end.end(),
                },
            )
        })
    }

    /// Parses `pattern "=>" block`.
    ///
    /// Branches are executable blocks and are not comma-separated, so a comma
    /// after a branch is reported rather than accepted (conformance case R024).
    fn parse_match_branch(&mut self) -> Result<MatchBranch, ParseError> {
        let pattern = self.parse_pattern()?;
        self.expect_kind(TokenKind::FatArrow, ParseErrorCode::UnexpectedToken)?;
        let body = self.parse_block()?;
        Ok(MatchBranch {
            span: Span {
                start: pattern.span.start(),
                end: body.span.end(),
            },
            pattern,
            body,
        })
    }

    fn parse_while_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect_word("while", ParseErrorCode::UnexpectedToken)?;
        let head = self.parse_control_head()?;
        let body = self.parse_block()?;
        Ok(Statement {
            expression: Some(head),
            span: Span {
                start: start.start(),
                end: body.span.end(),
            },
            body: Some(body),
            ..Statement::node(StatementForm::While, start)
        })
    }

    fn parse_for_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect_word("for", ParseErrorCode::UnexpectedToken)?;
        let pattern = self.parse_pattern()?;
        self.expect_word("in", ParseErrorCode::UnexpectedToken)?;
        let head = self.parse_control_head()?;
        let body = self.parse_block()?;
        Ok(Statement {
            pattern: Some(pattern),
            expression: Some(head),
            span: Span {
                start: start.start(),
                end: body.span.end(),
            },
            body: Some(body),
            ..Statement::node(StatementForm::For, start)
        })
    }

    fn parse_loop_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect_word("loop", ParseErrorCode::UnexpectedToken)?;
        let body = self.parse_block()?;
        Ok(Statement {
            span: Span {
                start: start.start(),
                end: body.span.end(),
            },
            body: Some(body),
            ..Statement::node(StatementForm::Loop, start)
        })
    }

    /// Parses `break ;` and `continue ;`, which carry no value in V1.
    fn parse_jump_statement(
        &mut self,
        form: StatementForm,
        word: &str,
    ) -> Result<Statement, ParseError> {
        let start = self.expect_word(word, ParseErrorCode::UnexpectedToken)?;
        let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
        Ok(Statement::node(
            form,
            Span {
                start: start.start(),
                end: end.end(),
            },
        ))
    }

    /// Parses `parallel`, `defer` and `unsafe`, which each introduce a scope
    /// around one executable block.
    fn parse_scope_statement(
        &mut self,
        form: StatementForm,
        word: &str,
    ) -> Result<Statement, ParseError> {
        let start = self.expect_word(word, ParseErrorCode::UnexpectedToken)?;
        let body = self.parse_block()?;
        Ok(Statement {
            span: Span {
                start: start.start(),
                end: body.span.end(),
            },
            body: Some(body),
            ..Statement::node(form, start)
        })
    }

    /// Parses `cancel expression ;`.
    ///
    /// Cancellation is a request: docs/41 keeps it distinct from the consuming
    /// `join`, so this statement does not discharge the task it names.
    fn parse_cancel_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect_word("cancel", ParseErrorCode::UnexpectedToken)?;
        let expression = self.parse_expression()?;
        let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
        Ok(Statement {
            expression: Some(expression),
            ..Statement::node(
                StatementForm::Cancel,
                Span {
                    start: start.start(),
                    end: end.end(),
                },
            )
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match self.current_text() {
            "let" => return self.parse_let_statement(),
            "if" => return self.parse_if_statement(),
            "match" => return self.parse_match_statement(),
            "while" => return self.parse_while_statement(),
            "for" => return self.parse_for_statement(),
            "loop" => return self.parse_loop_statement(),
            "break" => return self.parse_jump_statement(StatementForm::Break, "break"),
            "continue" => return self.parse_jump_statement(StatementForm::Continue, "continue"),
            "parallel" => return self.parse_scope_statement(StatementForm::Parallel, "parallel"),
            "defer" => return self.parse_scope_statement(StatementForm::Defer, "defer"),
            "unsafe" => return self.parse_scope_statement(StatementForm::Unsafe, "unsafe"),
            "cancel" => return self.parse_cancel_statement(),
            _ => {}
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
            expression,
            ..Statement::node(
                StatementForm::Return,
                Span {
                    start: start.start(),
                    end: end.end(),
                },
            )
        })
    }

    /// Parses `"_" | pattern_name | pattern_name "(" pattern_list? ")" |
    /// "(" pattern_list ")"` (docs/39 section 5).
    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        if self.current().kind() == TokenKind::OpenParen {
            let start = Span::from(self.advance());
            let elements =
                self.parse_comma_list(ListCloser::Kind(TokenKind::CloseParen), Self::parse_pattern);
            let end = self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
            return Ok(Pattern {
                form: PatternForm::Tuple,
                path: Vec::new(),
                elements,
                span: Span {
                    start: start.start(),
                    end: end.end(),
                },
            });
        }
        let first = self.expect_identifier()?;
        if first.text(self.source) == "_" {
            return Ok(Pattern {
                form: PatternForm::Wildcard,
                path: Vec::new(),
                elements: Vec::new(),
                span: first,
            });
        }
        let path = self.parse_pattern_path(first)?;
        let start = path[0].start();
        let last = *path.last().expect("a pattern path is nonempty");
        if self.current().kind() != TokenKind::OpenParen {
            return Ok(Pattern {
                form: PatternForm::Name,
                path,
                elements: Vec::new(),
                span: Span {
                    start,
                    end: last.end(),
                },
            });
        }
        self.advance();
        let elements =
            self.parse_comma_list(ListCloser::Kind(TokenKind::CloseParen), Self::parse_pattern);
        let end = self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
        Ok(Pattern {
            form: PatternForm::Destructure,
            path,
            elements,
            span: Span {
                start,
                end: end.end(),
            },
        })
    }

    /// Continues a pattern path after its first segment.
    ///
    /// `pattern_path = pattern_name ( "." identifier )*` stays deterministic:
    /// no other production may follow a pattern name with a dot.
    fn parse_pattern_path(&mut self, first: Span) -> Result<Vec<Span>, ParseError> {
        let mut path = alloc::vec![first];
        while self.consume_kind(TokenKind::Dot).is_some() {
            path.push(self.expect_identifier()?);
        }
        Ok(path)
    }

    fn parse_let_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect_word("let", ParseErrorCode::UnexpectedToken)?;
        let mutable = if self.current_text() == "mut" {
            self.advance();
            true
        } else {
            false
        };
        let pattern = self.parse_pattern()?;
        let declared_type = if self.consume_kind(TokenKind::Colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect_kind(TokenKind::Equal, ParseErrorCode::UnexpectedToken)?;
        let expression = Some(self.parse_expression()?);
        let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
        Ok(Statement {
            mutable,
            pattern: Some(pattern),
            declared_type,
            expression,
            ..Statement::node(
                StatementForm::Let,
                Span {
                    start: start.start(),
                    end: end.end(),
                },
            )
        })
    }

    fn parse_assignment_or_expression_statement(&mut self) -> Result<Statement, ParseError> {
        let start = Span::from(self.current());
        let target_or_expression = self.parse_expression()?;
        if self.consume_kind(TokenKind::Equal).is_some() {
            // docs/39 section 5: `assignment = place "=" expression`, and a
            // place is a name followed by field and index suffixes only.
            if !is_place(&target_or_expression) {
                return Err(ParseError {
                    code: ParseErrorCode::UnexpectedToken,
                    span: target_or_expression.span,
                });
            }
            let expression = self.parse_expression()?;
            let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
            return Ok(Statement {
                target: Some(target_or_expression),
                expression: Some(expression),
                ..Statement::node(
                    StatementForm::Assignment,
                    Span {
                        start: start.start(),
                        end: end.end(),
                    },
                )
            });
        }
        let end = self.expect_kind(TokenKind::Semicolon, ParseErrorCode::UnexpectedToken)?;
        Ok(Statement {
            expression: Some(target_or_expression),
            ..Statement::node(
                StatementForm::Expression,
                Span {
                    start: start.start(),
                    end: end.end(),
                },
            )
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
                elements: Vec::new(),
                parameters: Vec::new(),
                body: None,
                name: None,
                cast_type: None,
            };
        }
        Ok(left)
    }

    /// Parses a run of prefix operators and the operand under it.
    ///
    /// The run is collected first and the nodes are built from the operand
    /// outwards. A run of prefix operators nests nothing that the
    /// delimiter-nesting limit bounds, so it must not cost a stack frame each:
    /// `!!!!…b` inside a conforming 256 KiB unit would otherwise be as deep as
    /// it is long.
    fn parse_unary_expression(&mut self) -> Result<Expression, ParseError> {
        let mut operators: Vec<Span> = Vec::new();
        loop {
            let operator = if matches!(self.current_text(), "!" | "-" | "~" | "await" | "join") {
                Span::from(self.advance())
            } else if self.current_text() == "borrow" {
                let start = Span::from(self.advance());
                let end = if self.current_text() == "mut" {
                    Span::from(self.advance())
                } else {
                    start
                };
                Span {
                    start: start.start(),
                    end: end.end(),
                }
            } else {
                break;
            };
            operators.push(operator);
        }
        let mut operand = self.parse_postfix_expression()?;
        for operator in operators.into_iter().rev() {
            let end = operand.span.end();
            let mut node = Expression::node(
                ExpressionForm::Unary,
                Span {
                    start: operator.start(),
                    end,
                },
            );
            node.operator = Some(operator);
            node.inner = Some(Box::new(operand));
            operand = node;
        }
        Ok(operand)
    }

    /// Parses `primary ( call_suffix | index | field | question | cast )*`
    /// (docs/39 section 5).
    ///
    /// Suffixes chain left to right, so `a.b[0i32].c` nests as
    /// `Field(Index(Field(a, b), 0i32), c)`.
    fn parse_postfix_expression(&mut self) -> Result<Expression, ParseError> {
        let mut operand = self.parse_primary_expression()?;
        loop {
            if self.consume_kind(TokenKind::OpenParen).is_some() {
                let arguments = self.parse_call_arguments();
                let end =
                    self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
                operand = Expression {
                    form: ExpressionForm::Call,
                    left: None,
                    operator: None,
                    right: None,
                    inner: None,
                    elements: Vec::new(),
                    parameters: Vec::new(),
                    body: None,
                    name: None,
                    cast_type: None,
                    span: Span {
                        start: operand.span.start(),
                        end: end.end(),
                    },
                    callee: Some(Box::new(operand)),
                    arguments,
                };
                continue;
            }
            if self.consume_kind(TokenKind::OpenBracket).is_some() {
                let index = self.parse_expression()?;
                let end =
                    self.expect_kind(TokenKind::CloseBracket, ParseErrorCode::UnexpectedToken)?;
                operand = Expression {
                    form: ExpressionForm::Index,
                    left: None,
                    operator: None,
                    right: Some(Box::new(index)),
                    elements: Vec::new(),
                    parameters: Vec::new(),
                    body: None,
                    name: None,
                    cast_type: None,
                    callee: None,
                    arguments: Vec::new(),
                    span: Span {
                        start: operand.span.start(),
                        end: end.end(),
                    },
                    inner: Some(Box::new(operand)),
                };
                continue;
            }
            if let Some(question) = self.consume_kind(TokenKind::Question) {
                operand = Expression {
                    form: ExpressionForm::Question,
                    left: None,
                    operator: None,
                    right: None,
                    callee: None,
                    arguments: Vec::new(),
                    elements: Vec::new(),
                    parameters: Vec::new(),
                    body: None,
                    name: None,
                    cast_type: None,
                    span: Span {
                        start: operand.span.start(),
                        end: question.end(),
                    },
                    inner: Some(Box::new(operand)),
                };
                continue;
            }
            if self.current_text() == "as" {
                self.advance();
                let target = self.parse_type()?;
                operand = Expression {
                    form: ExpressionForm::Cast,
                    left: None,
                    operator: None,
                    right: None,
                    callee: None,
                    arguments: Vec::new(),
                    elements: Vec::new(),
                    parameters: Vec::new(),
                    body: None,
                    name: None,
                    span: Span {
                        start: operand.span.start(),
                        end: target.span().end(),
                    },
                    cast_type: Some(target),
                    inner: Some(Box::new(operand)),
                };
                continue;
            }
            if self.consume_kind(TokenKind::Dot).is_some() {
                let name = self.expect_identifier()?;
                operand = Expression {
                    form: ExpressionForm::Field,
                    left: None,
                    operator: None,
                    right: None,
                    callee: None,
                    arguments: Vec::new(),
                    elements: Vec::new(),
                    parameters: Vec::new(),
                    body: None,
                    name: Some(name),
                    cast_type: None,
                    span: Span {
                        start: operand.span.start(),
                        end: name.end(),
                    },
                    inner: Some(Box::new(operand)),
                };
                continue;
            }
            return Ok(operand);
        }
    }

    /// Parses a call argument list and enforces that it is entirely positional
    /// or entirely named (docs/39 section 5).
    ///
    /// The two forms are told apart without backtracking: `identifier :` can
    /// only begin a named argument, because `:` is not an expression operator.
    fn parse_call_arguments(&mut self) -> Vec<CallArgument> {
        let arguments = self.parse_comma_list(
            ListCloser::Kind(TokenKind::CloseParen),
            Self::parse_call_argument,
        );
        let Some(first) = arguments.first() else {
            return arguments;
        };
        let named = first.name.is_some();
        for argument in arguments.iter().skip(1) {
            if argument.name.is_some() == named {
                continue;
            }
            // Report at the argument that disagrees with the form the list
            // opened with, not at the list as a whole.
            let error = ParseError {
                code: ParseErrorCode::UnexpectedToken,
                span: argument.name.unwrap_or(argument.span),
            };
            self.report(error, Region::List);
            break;
        }
        arguments
    }

    fn parse_call_argument(&mut self) -> Result<CallArgument, ParseError> {
        if self.current().kind() == TokenKind::Identifier && self.peek(1).kind() == TokenKind::Colon
        {
            let name = self.expect_identifier()?;
            self.expect_kind(TokenKind::Colon, ParseErrorCode::UnexpectedToken)?;
            let value = self.parse_expression()?;
            return Ok(CallArgument {
                name: Some(name),
                span: Span {
                    start: name.start(),
                    end: value.span.end(),
                },
                value,
            });
        }
        let value = self.parse_expression()?;
        Ok(CallArgument {
            name: None,
            span: value.span,
            value,
        })
    }

    /// Parses `"(" expression ")"` or a tuple, which are told apart by the
    /// comma after the first element (docs/39 section 5).
    ///
    /// A V1 tuple has at least two elements, so `(a,)` is rejected rather than
    /// silently read as a one-element tuple or as a group.
    fn parse_group_or_tuple(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect_kind(TokenKind::OpenParen, ParseErrorCode::UnexpectedToken)?;
        let first = self.parse_expression()?;
        if self.current().kind() != TokenKind::Comma {
            let end = self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
            // Built by assignment rather than by functional update: an
            // `Expression` owns an iterative destructor now, and a type that
            // implements `Drop` cannot be moved out of.
            let mut group = Expression::node(
                ExpressionForm::Group,
                Span {
                    start: start.start(),
                    end: end.end(),
                },
            );
            group.inner = Some(Box::new(first));
            return Ok(group);
        }
        self.advance();
        let mut elements = alloc::vec![first];
        elements.extend(self.parse_comma_list(
            ListCloser::Kind(TokenKind::CloseParen),
            Self::parse_expression,
        ));
        if elements.len() < 2 {
            return Err(self.error_here(ParseErrorCode::UnexpectedToken));
        }
        let end = self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
        let mut tuple = Expression::node(
            ExpressionForm::Tuple,
            Span {
                start: start.start(),
                end: end.end(),
            },
        );
        tuple.elements = elements;
        Ok(tuple)
    }

    fn parse_array(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect_kind(TokenKind::OpenBracket, ParseErrorCode::UnexpectedToken)?;
        let elements = self.parse_comma_list(
            ListCloser::Kind(TokenKind::CloseBracket),
            Self::parse_expression,
        );
        let end = self.expect_kind(TokenKind::CloseBracket, ParseErrorCode::UnexpectedToken)?;
        let mut array = Expression::node(
            ExpressionForm::Array,
            Span {
                start: start.start(),
                end: end.end(),
            },
        );
        array.elements = elements;
        Ok(array)
    }

    fn parse_closure(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect_word("fn", ParseErrorCode::UnexpectedToken)?;
        self.expect_kind(TokenKind::OpenParen, ParseErrorCode::UnexpectedToken)?;
        let parameters = self.parse_parameters();
        self.expect_kind(TokenKind::CloseParen, ParseErrorCode::UnexpectedToken)?;
        let body = self.parse_block()?;
        let mut closure = Expression::node(ExpressionForm::Closure, start);
        closure.span = Span {
            start: start.start(),
            end: body.span.end(),
        };
        closure.parameters = parameters;
        closure.body = Some(Box::new(body));
        Ok(closure)
    }

    /// Parses `"spawn" ( "async" | "parallel" ) block`.
    ///
    /// The mode word is required: docs/39 gives no bare `spawn`, and the parser
    /// does not supply a default for a missing one.
    fn parse_spawn(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect_word("spawn", ParseErrorCode::UnexpectedToken)?;
        let mode = match self.current_text() {
            "async" | "parallel" => Span::from(self.advance()),
            _ => return Err(self.error_here(ParseErrorCode::UnexpectedToken)),
        };
        let body = self.parse_block()?;
        let mut spawn = Expression::node(ExpressionForm::Spawn, start);
        spawn.operator = Some(mode);
        spawn.span = Span {
            start: start.start(),
            end: body.span.end(),
        };
        spawn.body = Some(Box::new(body));
        Ok(spawn)
    }

    fn parse_primary_expression(&mut self) -> Result<Expression, ParseError> {
        if self.current().kind() == TokenKind::OpenParen {
            return self.parse_group_or_tuple();
        }
        if self.current().kind() == TokenKind::OpenBracket {
            return self.parse_array();
        }
        if self.current_text() == "fn" {
            return self.parse_closure();
        }
        if self.current_text() == "spawn" {
            return self.parse_spawn();
        }
        let form = match self.current().kind() {
            TokenKind::Identifier => Some(ExpressionForm::Name),
            TokenKind::Boolean
            | TokenKind::Integer
            | TokenKind::Size
            | TokenKind::Duration
            | TokenKind::String
            | TokenKind::Bytes => Some(ExpressionForm::Literal),
            _ => None,
        };
        if let Some(form) = form {
            let span = Span::from(self.advance());
            Ok(Expression {
                form,
                left: None,
                operator: None,
                right: None,
                inner: None,
                callee: None,
                arguments: Vec::new(),
                elements: Vec::new(),
                parameters: Vec::new(),
                body: None,
                name: None,
                cast_type: None,
                span,
            })
        } else {
            Err(self.error_here(ParseErrorCode::UnexpectedToken))
        }
    }

    fn parse_extern_function(
        &mut self,
        visibility: Visibility,
    ) -> Result<FunctionSignature, ParseError> {
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
            visibility,
            is_async: false,
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
        // `mut` is grammatical only here, and only for the two region
        // constructors. Anywhere else it is not a type qualifier at all, and
        // the parse fails where it stands rather than inventing a rule.
        let mutable = matches!(name.text(self.source), "Region" | "DmaRegion")
            && self.current_text() == "mut"
            && {
                self.advance();
                true
            };
        let arguments = self.parse_comma_list(ListCloser::Angle, Self::parse_type);
        let end = self.expect_close_angle()?;
        Ok(TypeSyntax::Constructed {
            name,
            arguments,
            mutable,
            span: Span {
                start,
                end: end.end(),
            },
        })
    }

    fn parse_dotted_name(&mut self) -> Result<Vec<Span>, ParseError> {
        let mut name = alloc::vec![self.expect_identifier()?];
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

    /// The token `offset` positions ahead, saturating at end of source.
    fn peek(&self, offset: usize) -> Token {
        let index = (self.index + offset).min(self.tokens.len() - 1);
        self.tokens[index]
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

/// Whether an expression is a `place` — the only thing an assignment may
/// target (docs/39 section 5).
///
/// A place is a name reached through field and index suffixes. A call result,
/// literal, cast or grouped expression is not assignable, and accepting one
/// here would let the parser admit source the grammar does not.
fn is_place(expression: &Expression) -> bool {
    match expression.form {
        ExpressionForm::Name => true,
        ExpressionForm::Field | ExpressionForm::Index => {
            expression.inner.as_deref().is_some_and(is_place)
        }
        _ => false,
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
