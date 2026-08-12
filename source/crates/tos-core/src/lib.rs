// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded canonical TOS Core V1 source reader (docs/39, ADR-0029).

#![no_std]

extern crate alloc;

// The test harness is a host program by construction, so it keeps `std`.
#[cfg(test)]
extern crate std;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

mod boundary;
mod capability;
mod checker;
mod concurrency;
mod defer;
mod diagnostic;
mod exhaustiveness;
mod flow;
mod guards;
mod lower;
mod metering;
mod modules;
mod mutability;
mod ownership;
mod parser;
mod place;
mod profile;
mod returns;
mod summary;
mod types;
mod typing;

pub use checker::Checker;
pub use diagnostic::{Diagnostic, DiagnosticField, ModuleIdentity, Position, Severity, Stage};
pub use lower::{lower_module, Gap, ModuleContext, FRONTEND_IDENTITY};
pub use modules::{check_module_set, check_module_summaries, check_source_set, ModuleEntry};
pub use parser::{
    Block, BorrowMode, CallArgument, ConstDeclaration, EnumDeclaration, EnumVariant,
    EnumVariantForm, Expression, ExpressionForm, FunctionDeclaration, FunctionParameter,
    FunctionSignature, Import, ImportKind, ModuleHeader, ModuleOutline, ModulePrefix, ParseOutcome,
    Parser, Pattern, PatternForm, Profile, RecordDeclaration, RecordField, ResourceDeclaration,
    ResourceLimit, Schema, Span, Statement, StatementForm, TypeSyntax, TypeSyntaxForm, Visibility,
};
pub use summary::{ImportSummary, Located, ModuleSummary, QualifiedUse};
pub use tos_ir::module_digest;

mod unicode {
    include!(concat!(env!("OUT_DIR"), "/unicode_tables.rs"));
}

pub const MAX_SOURCE_BYTES: usize = 256 * 1024;

/// Diagnostics retained for one module (docs/44 section 2).
///
/// Hostile source can carry an error every few bytes, so the number of
/// diagnostics a module may produce is bounded like every other frontend input.
/// Reaching the bound stops recording, not parsing: recovery still runs to
/// completion so the outcome stays well formed.
pub const MAX_DIAGNOSTICS_PER_MODULE: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceErrorCode {
    SourceTooLarge,
    InvalidUtf8,
    BomForbidden,
    BareCr,
    NotNfc,
    NulForbidden,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceError {
    code: SourceErrorCode,
    byte_offset: usize,
}
impl SourceErrorCode {
    /// Stable symbolic diagnostic code from docs/39 section 1.
    pub fn symbol(self) -> &'static str {
        match self {
            SourceErrorCode::SourceTooLarge => "E1000_SOURCE_LIMIT",
            SourceErrorCode::InvalidUtf8 => "E1001_INVALID_UTF8",
            SourceErrorCode::BomForbidden => "E1002_BOM_FORBIDDEN",
            SourceErrorCode::BareCr => "E1003_BARE_CR",
            SourceErrorCode::NotNfc => "E1004_NOT_NFC",
            SourceErrorCode::NulForbidden => "E1005_NUL_FORBIDDEN",
        }
    }
}

impl SourceError {
    pub fn code(self) -> SourceErrorCode {
        self.code
    }
    pub fn byte_offset(self) -> usize {
        self.byte_offset
    }
}
#[derive(Debug, Eq, PartialEq)]
pub struct SourceUnit {
    bytes: Box<[u8]>,
}
impl SourceUnit {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

pub struct SourceReader;
impl SourceReader {
    pub fn read(input: &[u8]) -> Result<SourceUnit, SourceError> {
        if input.len() > MAX_SOURCE_BYTES {
            return Err(SourceError {
                code: SourceErrorCode::SourceTooLarge,
                byte_offset: MAX_SOURCE_BYTES,
            });
        }
        if let Err(error) = core::str::from_utf8(input) {
            return Err(SourceError {
                code: SourceErrorCode::InvalidUtf8,
                byte_offset: error.valid_up_to(),
            });
        }
        if input.starts_with(&[0xef, 0xbb, 0xbf]) {
            return Err(SourceError {
                code: SourceErrorCode::BomForbidden,
                byte_offset: 0,
            });
        }
        let mut lf = Vec::with_capacity(input.len());
        let mut index = 0;
        while index < input.len() {
            if input[index] == b'\r' {
                if input.get(index + 1) == Some(&b'\n') {
                    lf.push(b'\n');
                    index += 2;
                } else {
                    return Err(SourceError {
                        code: SourceErrorCode::BareCr,
                        byte_offset: index,
                    });
                }
            } else {
                lf.push(input[index]);
                index += 1;
            }
        }
        if let Some(offset) = lf.iter().position(|&byte| byte == 0) {
            return Err(SourceError {
                code: SourceErrorCode::NulForbidden,
                byte_offset: offset,
            });
        }
        let text =
            core::str::from_utf8(&lf).expect("validated input stays UTF-8 after CRLF replacement");
        // Every ASCII scalar value is NFC-stable: none decomposes, none has a
        // nonzero canonical combining class, and none composes with what
        // precedes it. So an all-ASCII source unit *is* its own normal form,
        // and normalizing it would allocate a second copy of the whole source
        // to prove it equal to the first.
        //
        // docs/39 restricts identifiers to ASCII and admits Unicode only inside
        // string data and comments, so this is the ordinary case rather than a
        // special one — and the check that decides it is a scan with no
        // allocation at all.
        if lf.is_ascii() {
            return Ok(SourceUnit {
                bytes: lf.into_boxed_slice(),
            });
        }
        let nfc = nfc(text);
        if nfc.as_bytes() != lf {
            let offset = nfc
                .as_bytes()
                .iter()
                .zip(&lf)
                .position(|(a, b)| a != b)
                .unwrap_or(nfc.len().min(lf.len()));
            return Err(SourceError {
                code: SourceErrorCode::NotNfc,
                byte_offset: offset,
            });
        }
        Ok(SourceUnit {
            bytes: lf.into_boxed_slice(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LexErrorCode {
    TabOutsideLiteral,
    NonAsciiWhitespace,
    InvalidIdentifier,
    UnexpectedCharacter,
    InvalidIntegerLiteral,
    InvalidString,
    InvalidBytes,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LexError {
    code: LexErrorCode,
    byte_offset: usize,
}
impl LexErrorCode {
    /// Stable symbolic diagnostic code from the registry in docs/44 section 7.
    pub fn symbol(self) -> &'static str {
        match self {
            LexErrorCode::TabOutsideLiteral => "E1010_TAB_OUTSIDE_LITERAL",
            LexErrorCode::NonAsciiWhitespace => "E1011_NON_ASCII_WHITESPACE",
            LexErrorCode::InvalidIdentifier => "E1012_INVALID_IDENTIFIER",
            LexErrorCode::UnexpectedCharacter => "E1013_UNEXPECTED_CHARACTER",
            LexErrorCode::InvalidIntegerLiteral => "E1020_INVALID_INTEGER_LITERAL",
            LexErrorCode::InvalidString => "E1030_INVALID_STRING",
            LexErrorCode::InvalidBytes => "E1031_INVALID_BYTES",
        }
    }
}

impl LexError {
    pub fn code(self) -> LexErrorCode {
        self.code
    }
    pub fn byte_offset(self) -> usize {
        self.byte_offset
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Identifier,
    Keyword,
    Boolean,
    Integer,
    Size,
    Duration,
    String,
    Bytes,
    Equal,
    Semicolon,
    Comma,
    Colon,
    Dot,
    Question,
    Arrow,
    FatArrow,
    Operator,
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    OpenBrace,
    CloseBrace,
    Eof,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token {
    kind: TokenKind,
    start: usize,
    end: usize,
}
impl Token {
    pub fn kind(self) -> TokenKind {
        self.kind
    }
    pub fn start(self) -> usize {
        self.start
    }
    pub fn end(self) -> usize {
        self.end
    }

    pub fn text(self, source: &SourceUnit) -> &str {
        core::str::from_utf8(&source.bytes()[self.start..self.end])
            .expect("token spans are UTF-8 boundaries from SourceUnit")
    }
}
pub struct Lexer;
impl Lexer {
    pub fn lex(source: &SourceUnit) -> Result<Vec<Token>, LexError> {
        let bytes = source.bytes();
        let mut tokens = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let start = i;
            let byte = bytes[i];
            if byte == b' ' || byte == b'\n' {
                i += 1;
                continue;
            }
            if byte == b'\t' {
                return Err(LexError {
                    code: LexErrorCode::TabOutsideLiteral,
                    byte_offset: i,
                });
            }
            if byte >= 0x80 {
                let ch = core::str::from_utf8(&bytes[i..])
                    .expect("SourceUnit UTF-8")
                    .chars()
                    .next()
                    .unwrap();
                return Err(LexError {
                    code: if ch.is_whitespace() {
                        LexErrorCode::NonAsciiWhitespace
                    } else {
                        LexErrorCode::InvalidIdentifier
                    },
                    byte_offset: i,
                });
            }
            if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if is_ident_start(byte) {
                i += 1;
                while i < bytes.len() && is_ident_continue(bytes[i]) {
                    i += 1;
                }
                if bytes[start] == b'b' && bytes.get(i) == Some(&b'"') {
                    i = scan_quoted(bytes, i, true)?;
                    tokens.push(Token {
                        kind: TokenKind::Bytes,
                        start,
                        end: i,
                    });
                } else {
                    let word = core::str::from_utf8(&bytes[start..i]).unwrap();
                    let kind = if word == "true" || word == "false" {
                        TokenKind::Boolean
                    } else if is_keyword(word) {
                        TokenKind::Keyword
                    } else {
                        TokenKind::Identifier
                    };
                    tokens.push(Token {
                        kind,
                        start,
                        end: i,
                    });
                }
                continue;
            }
            if byte.is_ascii_digit() {
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let literal = core::str::from_utf8(&bytes[start..i]).unwrap();
                let kind = integer_kind(literal).ok_or(LexError {
                    code: LexErrorCode::InvalidIntegerLiteral,
                    byte_offset: start,
                })?;
                tokens.push(Token {
                    kind,
                    start,
                    end: i,
                });
                continue;
            }
            if byte == b'"' {
                i = scan_quoted(bytes, i, false)?;
                tokens.push(Token {
                    kind: TokenKind::String,
                    start,
                    end: i,
                });
                continue;
            }
            let (kind, width) = match bytes.get(i..i + 2) {
                Some(b"->") => (TokenKind::Arrow, 2),
                Some(b"=>") => (TokenKind::FatArrow, 2),
                Some(b"==" | b"!=" | b"<=" | b">=" | b"&&" | b"||" | b"<<" | b">>") => {
                    (TokenKind::Operator, 2)
                }
                _ => match byte {
                    b'=' => (TokenKind::Equal, 1),
                    b';' => (TokenKind::Semicolon, 1),
                    b',' => (TokenKind::Comma, 1),
                    b':' => (TokenKind::Colon, 1),
                    b'.' => (TokenKind::Dot, 1),
                    b'?' => (TokenKind::Question, 1),
                    b'(' => (TokenKind::OpenParen, 1),
                    b')' => (TokenKind::CloseParen, 1),
                    b'[' => (TokenKind::OpenBracket, 1),
                    b']' => (TokenKind::CloseBracket, 1),
                    b'{' => (TokenKind::OpenBrace, 1),
                    b'}' => (TokenKind::CloseBrace, 1),
                    b'+' | b'-' | b'*' | b'/' | b'%' | b'!' | b'~' | b'<' | b'>' | b'&' | b'|'
                    | b'^' => (TokenKind::Operator, 1),
                    // docs/44 section 7 fixes the split: a non-ASCII scalar
                    // value here could only be attempting an identifier, which
                    // is ASCII-only; anything else begins no lexical form.
                    _ => {
                        let code = if byte >= 0x80 {
                            LexErrorCode::InvalidIdentifier
                        } else {
                            LexErrorCode::UnexpectedCharacter
                        };
                        return Err(LexError {
                            code,
                            byte_offset: i,
                        });
                    }
                },
            };
            i += width;
            tokens.push(Token {
                kind,
                start,
                end: i,
            });
        }
        tokens.push(Token {
            kind: TokenKind::Eof,
            start: bytes.len(),
            end: bytes.len(),
        });
        Ok(tokens)
    }
}
fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}
fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}
fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "as" | "async"
            | "await"
            | "bootstrap"
            | "borrow"
            | "break"
            | "cancel"
            | "capability"
            | "const"
            | "continue"
            | "defer"
            | "else"
            | "enum"
            | "extern"
            | "fn"
            | "for"
            | "full"
            | "if"
            | "import"
            | "in"
            | "join"
            | "let"
            | "loop"
            | "match"
            | "module"
            | "mut"
            | "parallel"
            | "profile"
            | "pub"
            | "record"
            | "resource"
            | "return"
            | "spawn"
            | "unsafe"
            | "uses"
            | "version"
            | "while"
    )
}
fn integer_kind(value: &str) -> Option<TokenKind> {
    let (digits, kind) = if let Some(suffix) = ["KiB", "MiB", "GiB", "B"]
        .into_iter()
        .find(|suffix| value.ends_with(suffix))
    {
        (value.strip_suffix(suffix)?, TokenKind::Size)
    } else if let Some(suffix) = ["min", "ns", "us", "ms", "s", "h"]
        .into_iter()
        .find(|suffix| value.ends_with(suffix))
    {
        (value.strip_suffix(suffix)?, TokenKind::Duration)
    } else if let Some(suffix) = ["u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64"]
        .into_iter()
        .find(|suffix| value.ends_with(suffix))
    {
        (value.strip_suffix(suffix)?, TokenKind::Integer)
    } else {
        (value, TokenKind::Integer)
    };
    let (radix, body) = if let Some(body) = digits.strip_prefix("0x") {
        (16, body)
    } else if let Some(body) = digits.strip_prefix("0b") {
        (2, body)
    } else {
        (10, digits)
    };
    valid_integer_digits(body, radix).then_some(kind)
}

fn valid_integer_digits(body: &str, radix: u32) -> bool {
    !body.is_empty()
        && !body.starts_with('_')
        && !body.ends_with('_')
        && !body.contains("__")
        && body
            .chars()
            .filter(|&character| character != '_')
            .all(|character| character.is_digit(radix))
}

fn scan_quoted(bytes: &[u8], quote_offset: usize, only_ascii: bool) -> Result<usize, LexError> {
    let code = if only_ascii {
        LexErrorCode::InvalidBytes
    } else {
        LexErrorCode::InvalidString
    };
    let mut decoded = Vec::new();
    let mut offset = quote_offset + 1;

    while offset < bytes.len() {
        match bytes[offset] {
            b'"' => {
                if !only_ascii && core::str::from_utf8(&decoded).is_err() {
                    return Err(LexError {
                        code,
                        byte_offset: quote_offset,
                    });
                }
                return Ok(offset + 1);
            }
            b'\n' | b'\r' | 0 => {
                return Err(LexError {
                    code,
                    byte_offset: offset,
                })
            }
            b'\\' => {
                let escape_offset = offset;
                offset += 1;
                let escaped = *bytes.get(offset).ok_or(LexError {
                    code,
                    byte_offset: escape_offset,
                })?;
                match escaped {
                    b'\\' => decoded.push(b'\\'),
                    b'"' => decoded.push(b'"'),
                    b'n' => decoded.push(b'\n'),
                    b'r' => decoded.push(b'\r'),
                    b't' => decoded.push(b'\t'),
                    b'0' => decoded.push(0),
                    b'x' => {
                        let high = *bytes.get(offset + 1).ok_or(LexError {
                            code,
                            byte_offset: escape_offset,
                        })?;
                        let low = *bytes.get(offset + 2).ok_or(LexError {
                            code,
                            byte_offset: escape_offset,
                        })?;
                        let value = hex_byte(high, low).ok_or(LexError {
                            code,
                            byte_offset: escape_offset,
                        })?;
                        decoded.push(value);
                        offset += 2;
                    }
                    b'u' if !only_ascii => {
                        let (scalar, end) =
                            scan_unicode_escape(bytes, offset, escape_offset, code)?;
                        let character = char::from_u32(scalar).ok_or(LexError {
                            code,
                            byte_offset: escape_offset,
                        })?;
                        let mut buffer = [0; 4];
                        decoded.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                        offset = end;
                    }
                    _ => {
                        return Err(LexError {
                            code,
                            byte_offset: escape_offset,
                        })
                    }
                }
                offset += 1;
            }
            byte if only_ascii && !(b' '..=b'~').contains(&byte) => {
                return Err(LexError {
                    code,
                    byte_offset: offset,
                })
            }
            byte => {
                decoded.push(byte);
                offset += 1;
            }
        }
    }

    Err(LexError {
        code,
        byte_offset: quote_offset,
    })
}

fn hex_byte(high: u8, low: u8) -> Option<u8> {
    let high = (high as char).to_digit(16)?;
    let low = (low as char).to_digit(16)?;
    Some((high * 16 + low) as u8)
}

fn scan_unicode_escape(
    bytes: &[u8],
    u_offset: usize,
    escape_offset: usize,
    code: LexErrorCode,
) -> Result<(u32, usize), LexError> {
    if bytes.get(u_offset + 1) != Some(&b'{') {
        return Err(LexError {
            code,
            byte_offset: escape_offset,
        });
    }
    let mut offset = u_offset + 2;
    let digits_start = offset;
    let mut scalar = 0u32;
    while let Some(&byte) = bytes.get(offset) {
        if byte == b'}' {
            if offset == digits_start
                || offset - digits_start > 6
                || (0xd800..=0xdfff).contains(&scalar)
            {
                return Err(LexError {
                    code,
                    byte_offset: escape_offset,
                });
            }
            return Ok((scalar, offset));
        }
        let digit = (byte as char).to_digit(16).ok_or(LexError {
            code,
            byte_offset: escape_offset,
        })?;
        if offset - digits_start == 6 {
            return Err(LexError {
                code,
                byte_offset: escape_offset,
            });
        }
        scalar = scalar * 16 + digit;
        offset += 1;
    }
    Err(LexError {
        code,
        byte_offset: escape_offset,
    })
}

fn find3(table: &[(u32, u32, u32)], key: (u32, u32)) -> Option<u32> {
    table
        .binary_search_by_key(&key, |&(a, b, _)| (a, b))
        .ok()
        .map(|i| table[i].2)
}
fn find2<T: Copy>(table: &[(u32, T)], key: u32) -> Option<T> {
    table
        .binary_search_by_key(&key, |&(a, _)| a)
        .ok()
        .map(|i| table[i].1)
}
fn ccc(cp: u32) -> u8 {
    find2(unicode::CCC, cp).unwrap_or(0)
}
fn decompose(cp: u32, out: &mut Vec<u32>) {
    const S: u32 = 0xac00;
    const L: u32 = 0x1100;
    const V: u32 = 0x1161;
    const T: u32 = 0x11a7;
    const N: u32 = 588;
    const M: u32 = 28;
    if (S..S + 11172).contains(&cp) {
        let x = cp - S;
        out.push(L + x / N);
        out.push(V + (x % N) / M);
        if !x.is_multiple_of(M) {
            out.push(T + x % M);
        }
    } else if let Some((_, a, b)) = unicode::DECOMP
        .binary_search_by_key(&cp, |&(a, _, _)| a)
        .ok()
        .map(|i| unicode::DECOMP[i])
    {
        decompose(a, out);
        if b != 0 {
            decompose(b, out);
        }
    } else {
        out.push(cp);
    }
}
fn compose(a: u32, b: u32) -> Option<u32> {
    const S: u32 = 0xac00;
    const L: u32 = 0x1100;
    const V: u32 = 0x1161;
    const T: u32 = 0x11a7;
    const M: u32 = 28;
    if (L..L + 19).contains(&a) && (V..V + 21).contains(&b) {
        return Some(S + ((a - L) * 21 + (b - V)) * M);
    }
    if (S..S + 11172).contains(&a) && (a - S).is_multiple_of(M) && (T + 1..T + M).contains(&b) {
        return Some(a + b - T);
    }
    find3(unicode::COMPOSE, (a, b))
}
fn nfc(text: &str) -> String {
    let mut d = Vec::new();
    for ch in text.chars() {
        decompose(ch as u32, &mut d);
    }
    let mut ordered = Vec::new();
    for cp in d {
        let c = ccc(cp);
        let start = ordered
            .iter()
            .rposition(|&x| ccc(x) == 0)
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut i = ordered.len();
        while i > start && c != 0 && ccc(ordered[i - 1]) > c {
            i -= 1;
        }
        ordered.insert(i, cp);
    }
    let mut out = Vec::new();
    let mut starter = None;
    let mut last = 0;
    for cp in ordered {
        let c = ccc(cp);
        if let Some(i) = starter {
            if let Some(x) = compose(out[i], cp) {
                if last == 0 || last < c {
                    out[i] = x;
                    continue;
                }
            }
        }
        out.push(cp);
        if c == 0 {
            starter = Some(out.len() - 1);
        }
        last = c;
    }
    out.into_iter().filter_map(char::from_u32).collect()
}

#[cfg(test)]
mod tests {
    use std::{format, vec};

    use super::*;

    #[test]
    fn lexer_emits_identifiers_keywords_literals_and_punctuation() {
        let source = SourceReader::read(b"let value = 42i32; // comment\n").unwrap();
        let tokens = Lexer::lex(&source).expect("valid lexical input");
        assert_eq!(
            tokens.iter().map(|token| token.kind()).collect::<Vec<_>>(),
            vec![
                TokenKind::Keyword,
                TokenKind::Identifier,
                TokenKind::Equal,
                TokenKind::Integer,
                TokenKind::Semicolon,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn lexer_accepts_documented_literal_forms_and_ignores_comments() {
        let source = SourceReader::read(
            b"let number = 0x2a_i32; let size = 64KiB; let delay = 1min; let text = \"\\u{e9} \\xC3\\xA9\"; let raw = b\"abc \\x00\";\n",
        )
        .unwrap();
        let error = Lexer::lex(&source).expect_err("underscore before suffix is invalid");
        assert_eq!(
            (error.code(), error.byte_offset()),
            (LexErrorCode::InvalidIntegerLiteral, 13)
        );

        let source = SourceReader::read(
            b"let number = 0x2ai32; let size = 64KiB; let delay = 1min; let text = \"\\u{e9} \\xC3\\xA9\"; let raw = b\"abc \\x00\";\n",
        )
        .unwrap();
        let kinds = Lexer::lex(&source)
            .expect("all documented literal forms are valid")
            .iter()
            .map(|token| token.kind())
            .collect::<Vec<_>>();
        assert!(kinds.contains(&TokenKind::Integer));
        assert!(kinds.contains(&TokenKind::Size));
        assert!(kinds.contains(&TokenKind::Duration));
        assert!(kinds.contains(&TokenKind::String));
        assert!(kinds.contains(&TokenKind::Bytes));
    }

    #[test]
    fn lexer_reports_documented_lexical_errors_at_the_offending_byte() {
        for (input, code, offset) in [
            (b"let\tx;".as_slice(), LexErrorCode::TabOutsideLiteral, 3),
            (
                "let\u{a0}x;".as_bytes(),
                LexErrorCode::NonAsciiWhitespace,
                3,
            ),
            ("let é;".as_bytes(), LexErrorCode::InvalidIdentifier, 4),
            (
                b"let x = 0b102;".as_slice(),
                LexErrorCode::InvalidIntegerLiteral,
                8,
            ),
            (
                b"let x = \"\\q\";".as_slice(),
                LexErrorCode::InvalidString,
                9,
            ),
            ("let x = b\"é\";".as_bytes(), LexErrorCode::InvalidBytes, 10),
        ] {
            let source = SourceReader::read(input).expect("transport source is valid");
            let error = Lexer::lex(&source).expect_err("case must be rejected lexically");
            assert_eq!(
                (error.code(), error.byte_offset()),
                (code, offset),
                "{input:?}"
            );
        }
    }

    #[test]
    fn lexer_preserves_punctuation_spans_for_the_parser() {
        let source = SourceReader::read(
            b"a->b => c==d != e<=f >= g&&h||i<<j>>k + - * / % ! ~ < > & | ^ ( ) [ ] { } , : . ?;",
        )
        .unwrap();
        let tokens = Lexer::lex(&source).expect("punctuation is lexical input");
        let spellings = tokens
            .iter()
            .map(|token| token.text(&source))
            .collect::<Vec<_>>();
        assert_eq!(
            spellings,
            vec![
                "a", "->", "b", "=>", "c", "==", "d", "!=", "e", "<=", "f", ">=", "g", "&&", "h",
                "||", "i", "<<", "j", ">>", "k", "+", "-", "*", "/", "%", "!", "~", "<", ">", "&",
                "|", "^", "(", ")", "[", "]", "{", "}", ",", ":", ".", "?", ";", ""
            ]
        );
    }

    #[test]
    fn parser_builds_the_declared_module_header_with_source_spans() {
        let source = SourceReader::read(b"module system.boot version 1.0 profile bootstrap;")
            .expect("transport-valid source");
        let header = Parser::parse_header(&source)
            .into_accepted()
            .expect("module header parses");
        assert_eq!(header.name().len(), 2);
        assert_eq!(header.version(), (1, 0));
        assert_eq!(header.profile(), Profile::Bootstrap);
        assert_eq!(
            header.name()[0].text(&source),
            "system",
            "name component keeps its canonical source span"
        );
    }

    #[test]
    fn parser_builds_ordinary_and_capability_import_syntax() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap;\n\
              import system.text;\n\
              import capability system.time.Clock as clock;\n",
        )
        .expect("transport-valid source");
        let prefix = Parser::parse_prefix(&source)
            .into_accepted()
            .expect("module prefix parses");
        assert_eq!(prefix.imports().len(), 2);
        assert_eq!(prefix.imports()[0].kind(), ImportKind::Module);
        assert_eq!(prefix.imports()[0].binding().text(&source), "text");
        assert_eq!(prefix.imports()[1].kind(), ImportKind::Capability);
        assert_eq!(prefix.imports()[1].binding().text(&source), "clock");
    }

    #[test]
    fn parser_builds_a_bracketed_resource_declaration() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap;\n\
              resource [fuel: 1000, stack: 64KiB, imports: 0,]",
        )
        .expect("transport-valid source");
        let outline = Parser::parse_outline(&source)
            .into_accepted()
            .expect("resource declaration parses");
        assert_eq!(outline.resource().limits().len(), 3);
        assert_eq!(outline.resource().limits()[1].name().text(&source), "stack");
        assert_eq!(
            outline.resource().limits()[1].value().text(&source),
            "64KiB"
        );
    }

    #[test]
    fn parser_builds_a_record_with_typed_fields() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [] record Point [x: i32, y: i32,]",
        )
        .expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("record schema parses");
        assert_eq!(schema.records().len(), 1);
        assert_eq!(schema.records()[0].name().text(&source), "Point");
        assert_eq!(schema.records()[0].fields().len(), 2);
        assert_eq!(schema.records()[0].fields()[1].ty().text(&source), "i32");
    }

    #[test]
    fn parser_builds_unit_tuple_and_named_field_enum_variants() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [] \
              enum Message [Empty, Pair(i32, i32), Rgb [red: u8, green: u8,],]",
        )
        .expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("enum schema parses");
        assert_eq!(schema.enums().len(), 1);
        let variants = schema.enums()[0].variants();
        assert_eq!(variants.len(), 3);
        assert_eq!(variants[0].form(), EnumVariantForm::Unit);
        assert_eq!(variants[1].tuple_types().len(), 2);
        assert_eq!(variants[2].fields().len(), 2);
        assert_eq!(variants[2].fields()[0].name().text(&source), "red");
    }

    #[test]
    fn parser_builds_constructed_tuple_and_array_type_syntax() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [] record Forms [pair: (i32, bool), result: Result<i32, Error>, buffer: array<u8, 16>,]",
        )
        .expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("type forms parse");
        let fields = schema.records()[0].fields();
        assert_eq!(fields[0].ty().form(), TypeSyntaxForm::Tuple);
        assert_eq!(fields[1].ty().form(), TypeSyntaxForm::Constructed);
        assert_eq!(fields[2].ty().form(), TypeSyntaxForm::Array);
        assert_eq!(fields[2].ty().text(&source), "array<u8, 16>");
    }

    #[test]
    fn parser_builds_an_extern_function_signature_and_effect_list() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [] extern fn now(borrow clock: Clock) -> duration uses [clock,];",
        )
        .expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("extern declaration parses");
        let function = &schema.extern_functions()[0];
        assert_eq!(function.name().text(&source), "now");
        assert_eq!(function.parameters().len(), 1);
        assert_eq!(function.parameters()[0].borrow_mode(), BorrowMode::Shared);
        assert_eq!(function.effects()[0].text(&source), "clock");
    }

    #[test]
    fn parser_builds_a_function_body_with_an_explicit_return() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [] fn main() -> i32 { return 42i32; }",
        )
        .expect("transport-valid source");
        let module = Parser::parse_schema(&source)
            .into_accepted()
            .expect("function body parses");
        assert_eq!(module.functions().len(), 1);
        assert_eq!(
            module.functions()[0].signature().name().text(&source),
            "main"
        );
        assert_eq!(module.functions()[0].body().statements().len(), 1);
        assert_eq!(
            module.functions()[0].body().statements()[0].form(),
            StatementForm::Return
        );
    }

    #[test]
    fn parser_preserves_binary_operator_precedence_in_return_expression() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [] fn main() -> i32 { return a + b * c; }",
        )
        .expect("transport-valid source");
        let module = Parser::parse_schema(&source)
            .into_accepted()
            .expect("function expression parses");
        let expression = module.functions()[0].body().statements()[0]
            .expression()
            .expect("return value");
        assert_eq!(expression.form(), ExpressionForm::Binary);
        assert_eq!(expression.operator_text(&source), Some("+"));
        assert_eq!(
            expression.right().unwrap().operator_text(&source),
            Some("*")
        );
    }

    #[test]
    fn parser_preserves_parenthesized_expression_grouping() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [] fn main() -> i32 { return (a + b) * c; }",
        )
        .expect("transport-valid source");
        let module = Parser::parse_schema(&source)
            .into_accepted()
            .expect("grouped expression parses");
        let expression = module.functions()[0].body().statements()[0]
            .expression()
            .expect("return value");
        assert_eq!(expression.operator_text(&source), Some("*"));
        let group = expression.left().expect("left operand");
        assert_eq!(group.form(), ExpressionForm::Group);
        assert_eq!(group.inner().unwrap().operator_text(&source), Some("+"));
    }

    #[test]
    fn parser_binds_unary_operators_tighter_than_product_operators() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [] fn main() -> i32 { return -a * !b; }",
        )
        .expect("transport-valid source");
        let module = Parser::parse_schema(&source)
            .into_accepted()
            .expect("unary expression parses");
        let expression = module.functions()[0].body().statements()[0]
            .expression()
            .expect("return value");
        assert_eq!(expression.operator_text(&source), Some("*"));
        assert_eq!(expression.left().unwrap().form(), ExpressionForm::Unary);
        assert_eq!(expression.left().unwrap().operator_text(&source), Some("-"));
        assert_eq!(expression.right().unwrap().form(), ExpressionForm::Unary);
        assert_eq!(
            expression.right().unwrap().operator_text(&source),
            Some("!")
        );
    }

    #[test]
    fn parser_builds_a_single_call_construct_syntax_node() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [] fn main() -> i32 { return add_one(41i32); }",
        )
        .expect("transport-valid source");
        let module = Parser::parse_schema(&source)
            .into_accepted()
            .expect("call expression parses");
        let expression = module.functions()[0].body().statements()[0]
            .expression()
            .expect("return value");
        assert_eq!(expression.form(), ExpressionForm::Call);
        assert_eq!(expression.callee().unwrap().span().text(&source), "add_one");
        assert_eq!(expression.arguments().len(), 1);
        assert_eq!(
            expression.arguments()[0].value().span().text(&source),
            "41i32"
        );
    }

    #[test]
    fn parser_builds_a_typed_mutable_let_binding() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [] fn main() -> i32 { let mut count: i32 = 41i32; return count; }",
        )
        .expect("transport-valid source");
        let module = Parser::parse_schema(&source)
            .into_accepted()
            .expect("let binding parses");
        let binding = &module.functions()[0].body().statements()[0];
        assert_eq!(binding.form(), StatementForm::Let);
        assert!(binding.is_mutable());
        assert_eq!(
            binding.pattern().unwrap().name().unwrap().text(&source),
            "count"
        );
        assert_eq!(binding.declared_type().unwrap().text(&source), "i32");
        assert_eq!(binding.expression().unwrap().span().text(&source), "41i32");
    }

    #[test]
    fn parser_builds_an_assignment_statement() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [] fn main() -> i32 { let mut count: i32 = 41i32; count = count + 1i32; return count; }",
        )
        .expect("transport-valid source");
        let module = Parser::parse_schema(&source)
            .into_accepted()
            .expect("assignment parses");
        let assignment = &module.functions()[0].body().statements()[1];
        assert_eq!(assignment.form(), StatementForm::Assignment);
        assert_eq!(assignment.target().unwrap().span().text(&source), "count");
        assert_eq!(
            assignment.expression().unwrap().operator_text(&source),
            Some("+")
        );
    }

    #[test]
    fn parser_rejects_a_resource_list_without_a_comma() {
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [fuel: 1 stack: 1B]",
        )
        .expect("transport-valid source");
        let outcome = Parser::parse_outline(&source);
        assert!(outcome.has_errors());
        assert!(outcome.into_accepted().is_none());
        let outcome = Parser::parse_outline(&source);
        let diagnostic = &outcome.diagnostics()[0];
        assert_eq!(diagnostic.code(), "E1106_LIST_SEPARATOR_REQUIRED");
        assert_eq!(diagnostic.stage(), Stage::Parse);
        assert_eq!(diagnostic.span().text(&source), "stack");
        assert_eq!(diagnostic.field("region"), Some("list"));
    }

    const PREFIX: &str = "module system.boot version 1.0 profile bootstrap; resource [fuel: 1000] ";

    fn parse(body: &str) -> (SourceUnit, ParseOutcome<Schema>) {
        let text = alloc::format!("{PREFIX}{body}");
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let outcome = Parser::parse_schema(&source);
        (source, outcome)
    }

    #[test]
    fn parser_reports_one_diagnostic_for_each_failed_declaration_region() {
        // A broken function signature and a superseded enum form surround a
        // valid record. Each failed region contributes exactly one diagnostic
        // and no tree, and neither swallows what follows it.
        let (source, outcome) = parse(
            "fn 7() -> i32 { return 1i32; } record Second [value: i32] enum Third {Value} \
             fn main() -> i32 { return 1i32; }",
        );
        assert!(outcome.has_errors());
        let codes: Vec<&str> = outcome
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code())
            .collect();
        assert_eq!(
            codes,
            ["E1101_EXPECTED_IDENTIFIER", "E1107_UNEXPECTED_TOKEN"]
        );
        assert_eq!(outcome.diagnostics()[0].span().text(&source), "7");
        assert_eq!(outcome.diagnostics()[1].span().text(&source), "{");
        for diagnostic in outcome.diagnostics() {
            assert_eq!(diagnostic.field("region"), Some("declaration"));
        }

        // Recovery continued: the intact declarations after each failure are
        // still parsed, and the broken ones are absent rather than guessed.
        let schema = outcome.value().expect("recovery keeps the partial schema");
        assert_eq!(schema.records().len(), 1);
        assert_eq!(schema.records()[0].name().text(&source), "Second");
        assert_eq!(schema.enums().len(), 0);
        assert_eq!(schema.functions().len(), 1);
        assert_eq!(
            schema.functions()[0].signature().name().text(&source),
            "main"
        );
    }

    #[test]
    fn parser_recovers_at_the_next_statement_boundary() {
        let (source, outcome) = parse(
            "fn main() -> i32 { let a: i32 = 1i32; let = 2i32; let c: i32 = 3i32; return c; }",
        );
        assert!(outcome.has_errors());
        assert_eq!(outcome.diagnostics().len(), 1);
        let diagnostic = &outcome.diagnostics()[0];
        assert_eq!(diagnostic.code(), "E1101_EXPECTED_IDENTIFIER");
        assert_eq!(diagnostic.field("region"), Some("statement"));
        assert_eq!(diagnostic.span().text(&source), "=");

        let schema = outcome.value().expect("recovery keeps the partial schema");
        let statements = schema.functions()[0].body().statements();
        assert_eq!(statements.len(), 3);
        assert_eq!(
            statements[0]
                .pattern()
                .unwrap()
                .name()
                .unwrap()
                .text(&source),
            "a"
        );
        assert_eq!(
            statements[1]
                .pattern()
                .unwrap()
                .name()
                .unwrap()
                .text(&source),
            "c"
        );
        assert_eq!(statements[2].form(), StatementForm::Return);
    }

    #[test]
    fn parser_chains_field_and_index_suffixes_left_to_right() {
        let (source, outcome) = parse(
            "record Point [row: i32] \
             fn main(grid: array<Point, 2>) -> i32 { return grid[1i32].row; }",
        );
        let schema = outcome.into_accepted().expect("postfix suffixes parse");
        let statements = schema.functions()[0].body().statements();
        let field = statements[0].expression().unwrap();
        assert_eq!(field.form(), ExpressionForm::Field);
        assert_eq!(field.name().unwrap().text(&source), "row");
        assert_eq!(field.span().text(&source), "grid[1i32].row");

        let index = field.inner().expect("field applies to the indexed value");
        assert_eq!(index.form(), ExpressionForm::Index);
        assert_eq!(index.right().unwrap().span().text(&source), "1i32");
        assert_eq!(
            index.inner().unwrap().span().text(&source),
            "grid",
            "indexing applies to the primary receiver"
        );
    }

    #[test]
    fn parser_applies_a_call_suffix_to_a_field_receiver() {
        let (source, outcome) = parse("fn main() -> i32 { return device.reset(1i32); }");
        let schema = outcome.into_accepted().expect("call on a field parses");
        let call = schema.functions()[0].body().statements()[0]
            .expression()
            .unwrap();
        assert_eq!(call.form(), ExpressionForm::Call);
        assert_eq!(call.arguments().len(), 1);
        let callee = call.callee().expect("callee is the field access");
        assert_eq!(callee.form(), ExpressionForm::Field);
        assert_eq!(callee.name().unwrap().text(&source), "reset");
    }

    #[test]
    fn parser_builds_question_and_cast_suffixes() {
        let (source, outcome) = parse(
            "extern fn load() -> Result<i32, i32>; \
             fn main() -> Result<i64, i32> { return load()? as i64; }",
        );
        let schema = outcome.into_accepted().expect("question and cast parse");
        let cast = schema.functions()[0].body().statements()[0]
            .expression()
            .unwrap();
        assert_eq!(cast.form(), ExpressionForm::Cast);
        assert_eq!(cast.cast_type().unwrap().text(&source), "i64");

        let question = cast.inner().expect("cast applies to the propagated value");
        assert_eq!(question.form(), ExpressionForm::Question);
        assert_eq!(question.span().text(&source), "load()?");
        assert_eq!(
            question.inner().unwrap().form(),
            ExpressionForm::Call,
            "`?` applies to the call result"
        );
    }

    #[test]
    fn a_cast_binds_tighter_than_an_arithmetic_operator() {
        // `as` is a postfix suffix, so it attaches to `left` alone.
        let (source, outcome) =
            parse("fn main(left: i32, right: i64) -> i64 { return left as i64 + right; }");
        let schema = outcome.into_accepted().expect("cast in a sum parses");
        let sum = schema.functions()[0].body().statements()[0]
            .expression()
            .unwrap();
        assert_eq!(sum.form(), ExpressionForm::Binary);
        assert_eq!(sum.operator_text(&source), Some("+"));
        assert_eq!(sum.left().unwrap().form(), ExpressionForm::Cast);
        assert_eq!(sum.left().unwrap().span().text(&source), "left as i64");
        assert_eq!(sum.right().unwrap().span().text(&source), "right");
    }

    #[test]
    fn parser_builds_named_constructor_arguments() {
        let (source, outcome) = parse(
            "record Point [x: i32, y: i32] \
             fn main() -> Point { return Point(x: 1i32, y: 2i32,); }",
        );
        let schema = outcome.into_accepted().expect("named arguments parse");
        let call = schema.functions()[0].body().statements()[0]
            .expression()
            .unwrap();
        assert_eq!(call.form(), ExpressionForm::Call);
        let arguments = call.arguments();
        assert_eq!(arguments.len(), 2, "a trailing comma closes the list");
        assert_eq!(arguments[0].name().unwrap().text(&source), "x");
        assert_eq!(arguments[0].value().span().text(&source), "1i32");
        assert_eq!(arguments[1].name().unwrap().text(&source), "y");
        assert_eq!(arguments[1].span().text(&source), "y: 2i32");
    }

    #[test]
    fn positional_arguments_carry_no_name() {
        let (_, outcome) = parse("fn main() -> i32 { return add(1i32, 2i32); }");
        let schema = outcome.into_accepted().expect("positional arguments parse");
        let call = schema.functions()[0].body().statements()[0]
            .expression()
            .unwrap();
        assert!(call
            .arguments()
            .iter()
            .all(|argument| argument.name().is_none()));
    }

    #[test]
    fn an_argument_list_may_not_mix_positional_and_named_forms() {
        // docs/39 section 5 gives call_arguments one form per list.
        let (source, outcome) = parse("fn main() -> i32 { return add(1i32, y: 2i32); }");
        assert!(outcome.has_errors());
        let diagnostic = outcome
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.span().text(&source) == "y")
            .expect("the disagreeing argument is reported");
        assert_eq!(diagnostic.code(), "E1107_UNEXPECTED_TOKEN");

        let (source, outcome) = parse("fn main() -> i32 { return add(x: 1i32, 2i32); }");
        assert!(outcome.has_errors());
        assert!(outcome
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.span().text(&source) == "2i32"));
    }

    #[test]
    fn named_constructor_fields_require_commas() {
        // Conformance case R012.
        let source = SourceReader::read(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../docs/language/conformance/v1/reject/record-field-separator.tos"
        )))
        .expect("vector is transport-valid");
        let outcome = Parser::parse_schema(&source);
        assert!(outcome.has_errors());
        let diagnostic = &outcome.diagnostics()[0];
        assert_eq!(diagnostic.code(), "E1106_LIST_SEPARATOR_REQUIRED");
        assert_eq!(diagnostic.span().text(&source), "second");
    }

    #[test]
    fn parser_records_visibility_and_the_async_marker() {
        let (source, outcome) = parse(
            "pub record Point [x: i32] enum Hidden [Value] \
             pub const LIMIT: i32 = 8i32; \
             pub async fn run() -> i32 { return 1i32; } \
             fn helper() -> i32 { return 2i32; }",
        );
        let schema = outcome.into_accepted().expect("item visibility parses");
        assert_eq!(schema.records()[0].visibility(), Visibility::Public);
        assert_eq!(schema.enums()[0].visibility(), Visibility::Private);

        let limit = &schema.consts()[0];
        assert_eq!(limit.visibility(), Visibility::Public);
        assert_eq!(limit.name().text(&source), "LIMIT");
        assert_eq!(limit.ty().text(&source), "i32");
        assert_eq!(limit.value().span().text(&source), "8i32");

        let run = schema.functions()[0].signature();
        assert_eq!(run.visibility(), Visibility::Public);
        assert!(run.is_async());
        assert!(run.span().text(&source).starts_with("async fn run"));

        let helper = schema.functions()[1].signature();
        assert_eq!(helper.visibility(), Visibility::Private);
        assert!(!helper.is_async());
    }

    #[test]
    fn parser_builds_tuple_and_array_literals() {
        let (source, outcome) = parse(
            "fn main() -> i32 { let pair: (i32, i32) = (1i32, 2i32,); \
             let values: array<i32, 2> = [3i32, 4i32]; let empty: array<i32, 0> = []; return 0i32; }",
        );
        let schema = outcome
            .into_accepted()
            .expect("tuple and array literals parse");
        let statements = schema.functions()[0].body().statements();

        let tuple = statements[0].expression().unwrap();
        assert_eq!(tuple.form(), ExpressionForm::Tuple);
        assert_eq!(tuple.elements().len(), 2);
        assert_eq!(tuple.elements()[1].span().text(&source), "2i32");

        let array = statements[1].expression().unwrap();
        assert_eq!(array.form(), ExpressionForm::Array);
        assert_eq!(array.elements().len(), 2);

        let empty = statements[2].expression().unwrap();
        assert_eq!(empty.form(), ExpressionForm::Array);
        assert!(empty.elements().is_empty());
    }

    #[test]
    fn a_parenthesized_expression_stays_a_group() {
        let (source, outcome) = parse("fn main() -> i32 { return (1i32 + 2i32) * 3i32; }");
        let schema = outcome.into_accepted().expect("grouping parses");
        let product = schema.functions()[0].body().statements()[0]
            .expression()
            .unwrap();
        let group = product.left().unwrap();
        assert_eq!(group.form(), ExpressionForm::Group);
        assert_eq!(group.inner().unwrap().span().text(&source), "1i32 + 2i32");
    }

    #[test]
    fn a_one_element_tuple_is_not_a_v1_form() {
        // docs/39 section 5 requires at least two tuple elements, so `(a,)` is
        // neither a tuple nor a group.
        let (_, outcome) = parse("fn main() -> i32 { let pair: i32 = (1i32,); return 0i32; }");
        assert!(outcome.has_errors());
        assert!(outcome.into_accepted().is_none());
    }

    #[test]
    fn parser_builds_closures_and_spawned_blocks() {
        let (source, outcome) = parse(
            "fn main() -> i32 { let step: fn (i32) -> i32 = fn (value: i32) { return value; }; \
             let task: Task<i32> = spawn async { return 1i32; }; return 0i32; }",
        );
        let schema = outcome.into_accepted().expect("closure and spawn parse");
        let statements = schema.functions()[0].body().statements();

        let closure = statements[0].expression().unwrap();
        assert_eq!(closure.form(), ExpressionForm::Closure);
        assert_eq!(closure.parameters().len(), 1);
        assert_eq!(closure.parameters()[0].name().text(&source), "value");
        assert_eq!(closure.body().unwrap().statements().len(), 1);

        let spawn = statements[1].expression().unwrap();
        assert_eq!(spawn.form(), ExpressionForm::Spawn);
        assert_eq!(spawn.operator_text(&source), Some("async"));
        assert_eq!(spawn.body().unwrap().statements().len(), 1);
    }

    #[test]
    fn spawn_requires_an_explicit_mode() {
        let (_, outcome) =
            parse("fn main() -> i32 { let task: i32 = spawn { return 1i32; }; return 0i32; }");
        assert!(outcome.has_errors());
        assert!(outcome.into_accepted().is_none());
    }

    #[test]
    fn parser_builds_every_let_pattern_form() {
        let (source, outcome) = parse(
            "fn main() -> i32 { let (first, second): (i32, i32) = (1i32, 2i32); \
             let Some(value): Option<i32> = first; let _ : i32 = second; return value; }",
        );
        let schema = outcome.into_accepted().expect("patterns parse");
        let statements = schema.functions()[0].body().statements();

        let tuple = statements[0].pattern().unwrap();
        assert_eq!(tuple.form(), PatternForm::Tuple);
        assert_eq!(tuple.elements().len(), 2);
        assert_eq!(tuple.elements()[0].form(), PatternForm::Name);
        assert_eq!(tuple.elements()[1].name().unwrap().text(&source), "second");

        let destructure = statements[1].pattern().unwrap();
        assert_eq!(destructure.form(), PatternForm::Destructure);
        assert_eq!(destructure.name().unwrap().text(&source), "Some");
        assert_eq!(
            destructure.elements()[0].name().unwrap().text(&source),
            "value"
        );

        let wildcard = statements[2].pattern().unwrap();
        assert_eq!(wildcard.form(), PatternForm::Wildcard);
        assert!(wildcard.name().is_none());
    }

    #[test]
    fn parser_builds_if_else_chains() {
        let (source, outcome) = parse(
            "fn main(value: i32) -> i32 { if (value == 0i32) { return 1i32; } \
             else if (value == 1i32) { return 2i32; } else { return 3i32; } }",
        );
        let schema = outcome.into_accepted().expect("if/else chain parses");
        let first = &schema.functions()[0].body().statements()[0];
        assert_eq!(first.form(), StatementForm::If);
        assert_eq!(
            first.expression().unwrap().span().text(&source),
            "value == 0i32"
        );
        assert_eq!(first.body().unwrap().statements().len(), 1);
        assert!(first.else_body().is_none());

        let second = first.else_if().expect("else if continues the chain");
        assert_eq!(second.form(), StatementForm::If);
        assert_eq!(second.else_body().unwrap().statements().len(), 1);
    }

    #[test]
    fn parser_builds_match_branches_as_blocks() {
        let (source, outcome) = parse(
            "enum State [Ready, Stopped] \
             fn main(state: State) -> i32 { match (state) { Ready => { return 1i32; } \
             _ => { return 0i32; } } }",
        );
        let schema = outcome.into_accepted().expect("match statement parses");
        let statement = &schema.functions()[0].body().statements()[0];
        assert_eq!(statement.form(), StatementForm::Match);
        let branches = statement.branches();
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[0].pattern().name().unwrap().text(&source), "Ready");
        assert_eq!(branches[1].pattern().form(), PatternForm::Wildcard);
        assert_eq!(branches[1].body().statements().len(), 1);
    }

    #[test]
    fn parser_builds_loop_forms_and_jumps() {
        let (source, outcome) = parse(
            "fn main(limit: i32, items: array<i32, 2>) -> i32 { \
             while (limit > 0i32) { break; } \
             for item in (items) { continue; } \
             loop { break; } return 0i32; }",
        );
        let schema = outcome.into_accepted().expect("loop forms parse");
        let statements = schema.functions()[0].body().statements();

        assert_eq!(statements[0].form(), StatementForm::While);
        assert_eq!(
            statements[0].body().unwrap().statements()[0].form(),
            StatementForm::Break
        );

        assert_eq!(statements[1].form(), StatementForm::For);
        assert_eq!(
            statements[1]
                .pattern()
                .unwrap()
                .name()
                .unwrap()
                .text(&source),
            "item"
        );
        assert_eq!(
            statements[1].body().unwrap().statements()[0].form(),
            StatementForm::Continue
        );

        assert_eq!(statements[2].form(), StatementForm::Loop);
    }

    #[test]
    fn an_unparenthesized_control_head_is_rejected() {
        // Conformance cases R009 through R011.
        for vector in [
            "if-identifier-control-head",
            "while-identifier-control-head",
            "match-identifier-control-head",
        ] {
            let path = alloc::format!(
                "{}/../../../docs/language/conformance/v1/reject/{vector}.tos",
                env!("CARGO_MANIFEST_DIR")
            );
            let bytes = std::fs::read(&path).expect("vector is readable");
            let source = SourceReader::read(&bytes).expect("vector is transport-valid");
            let outcome = Parser::parse_schema(&source);
            assert!(outcome.has_errors(), "{vector}");
            assert_eq!(
                outcome.diagnostics()[0].code(),
                "E1105_CONTROL_HEAD_PARENS_REQUIRED",
                "{vector}"
            );
        }
    }

    #[test]
    fn parser_builds_scope_and_cancellation_statements() {
        let (source, outcome) = parse(
            "fn main(task: Task<i32>) -> i32 { parallel { return 1i32; } \
             defer { return 2i32; } unsafe { return 3i32; } cancel task; return 0i32; }",
        );
        let schema = outcome.into_accepted().expect("scope statements parse");
        let statements = schema.functions()[0].body().statements();
        assert_eq!(statements[0].form(), StatementForm::Parallel);
        assert_eq!(statements[1].form(), StatementForm::Defer);
        assert_eq!(statements[2].form(), StatementForm::Unsafe);
        for statement in &statements[0..3] {
            assert_eq!(statement.body().unwrap().statements().len(), 1);
        }

        let cancel = &statements[3];
        assert_eq!(cancel.form(), StatementForm::Cancel);
        assert_eq!(cancel.expression().unwrap().span().text(&source), "task");
    }

    #[test]
    fn only_a_place_may_be_assigned_to() {
        let (source, outcome) = parse(
            "record Point [x: i32] \
             fn main(grid: array<Point, 2>) -> i32 { grid[0i32].x = 1i32; return 0i32; }",
        );
        let schema = outcome.into_accepted().expect("a place assignment parses");
        let assignment = &schema.functions()[0].body().statements()[0];
        assert_eq!(assignment.form(), StatementForm::Assignment);
        assert_eq!(
            assignment.target().unwrap().span().text(&source),
            "grid[0i32].x"
        );

        for target in ["read()", "(value)", "1i32"] {
            let body = alloc::format!("fn main() -> i32 {{ {target} = 1i32; return 0i32; }}");
            let (_, outcome) = parse(&body);
            assert!(outcome.has_errors(), "{target} is not a place");
        }
    }

    fn check(body: &str) -> (SourceUnit, Vec<Diagnostic>) {
        let text = alloc::format!("{PREFIX}{body}");
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        (source, diagnostics)
    }

    #[test]
    fn checker_requires_every_resource_key() {
        // PREFIX declares only `fuel`, so the other nine are missing.
        let (_, diagnostics) = check("fn main() -> i32 { return 0i32; }");
        let missing: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1700_RESOURCE_DECLARATION_REQUIRED")
            .filter_map(|d| d.field("key"))
            .collect();
        assert_eq!(
            missing,
            // Reported in the order docs/41 section 6 lists the keys.
            [
                "stack",
                "allocation",
                "tasks",
                "workers",
                "sync",
                "shared",
                "cleanup",
                "recursion",
                "imports"
            ]
        );
        assert!(diagnostics.iter().all(|d| d.stage() == Stage::Resource));
    }

    #[test]
    fn checker_reports_unknown_and_repeated_resource_keys() {
        let text = "module system.boot version 1.0 profile bootstrap; \
             resource [fuel: 1000, fuel: 2000, budget: 1, stack: 4, allocation: 4KiB, tasks: 1, \
             workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
             fn main() -> i32 { return 0i32; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        let codes: Vec<(&str, Option<&str>)> = diagnostics
            .iter()
            .map(|d| (d.code(), d.field("key")))
            .collect();
        assert_eq!(
            codes,
            [
                ("E1703_DUPLICATE_RESOURCE_DECLARATION", Some("fuel")),
                ("E1704_UNKNOWN_RESOURCE_LIMIT", Some("budget")),
                // `stack` takes a size, and 4 is an integer.
                ("E1704_UNKNOWN_RESOURCE_LIMIT", Some("stack")),
            ]
        );
        assert_eq!(diagnostics[2].field("expected"), Some("size"));
    }

    #[test]
    fn checker_reports_a_duplicate_declared_field() {
        let (source, diagnostics) = check("record Point [x: i32, x: i32]");
        let duplicate = diagnostics
            .iter()
            .find(|d| d.code() == "E1205_DUPLICATE_RECORD_FIELD")
            .expect("duplicate field is reported");
        assert_eq!(duplicate.stage(), Stage::Type);
        assert_eq!(duplicate.field("field"), Some("x"));
        assert_eq!(duplicate.span().text(&source), "x");
        assert!(duplicate.field("first_declared_at").is_some());
    }

    #[test]
    fn checker_reports_a_duplicate_constructor_argument_at_any_depth() {
        let (_, diagnostics) = check(
            "record Point [x: i32, y: i32] \
             fn main() -> i32 { if (true) { let p: Point = Point(x: 1i32, x: 2i32, y: 3i32); } \
             return 0i32; }",
        );
        let duplicate = diagnostics
            .iter()
            .find(|d| d.code() == "E1205_DUPLICATE_RECORD_FIELD")
            .expect("nested duplicate argument is reported");
        assert_eq!(duplicate.field("field"), Some("x"));
    }

    #[test]
    fn checker_resolves_names_through_their_scopes() {
        let (_, diagnostics) = check(
            "enum Signal [Low, High] record Point [x: i32] \
             fn helper() -> i32 { return 1i32; } \
             fn main(input: Signal) -> i32 { let p: Point = Point(x: helper()); \
             match (input) { Low => { return p.x; } other => { return 0i32; } } }",
        );
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code() != "E1202_UNKNOWN_VALUE_NAME"),
            "declared names, parameters, bindings and variants all resolve"
        );
    }

    #[test]
    fn checker_reports_an_unbound_value_name() {
        let (source, diagnostics) = check("fn main() -> i32 { return missing; }");
        let unknown = diagnostics
            .iter()
            .find(|d| d.code() == "E1202_UNKNOWN_VALUE_NAME")
            .expect("an unbound name is reported");
        assert_eq!(unknown.stage(), Stage::Type);
        assert_eq!(unknown.field("name"), Some("missing"));
        assert_eq!(unknown.span().text(&source), "missing");
    }

    #[test]
    fn a_let_initializer_cannot_see_its_own_binding() {
        let (_, diagnostics) = check("fn main() -> i32 { let value: i32 = value; return 0i32; }");
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.field("name") == Some("value"))
                .count(),
            1
        );
    }

    #[test]
    fn a_binding_does_not_escape_its_block() {
        let (_, diagnostics) =
            check("fn main() -> i32 { if (true) { let inner: i32 = 1i32; } return inner; }");
        assert!(diagnostics
            .iter()
            .any(|d| d.code() == "E1202_UNKNOWN_VALUE_NAME" && d.field("name") == Some("inner")));
    }

    #[test]
    fn field_names_are_not_resolved_as_values() {
        // `.total` and the `x:` label name fields, not values in scope.
        let (_, diagnostics) = check(
            "record Point [x: i32] fn main(p: Point) -> i32 { let q: Point = Point(x: p.x); return q.x; }",
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1202_UNKNOWN_VALUE_NAME"));
    }

    #[test]
    fn a_qualified_pattern_path_is_never_a_binding() {
        // ADR-0033 section 5: a path names a constructor, and an unknown one
        // is an error rather than a catch-all.
        let (source, diagnostics) = check(
            "enum Signal [Low, High] \
             fn main(signal: Signal) -> i32 { match (signal) { Signal.Low => { return 1i32; } \
             Signal.Middle => { return 2i32; } } }",
        );
        let unknown = diagnostics
            .iter()
            .find(|d| d.code() == "E1202_UNKNOWN_VALUE_NAME")
            .expect("an unknown qualified variant is reported");
        assert_eq!(unknown.field("name"), Some("Middle"));
        assert_eq!(unknown.field("enum"), Some("Signal"));
        assert_eq!(unknown.span().text(&source), "Signal.Middle");
    }

    #[test]
    fn a_known_qualified_pattern_path_checks_clean() {
        let (_, diagnostics) = check(
            "enum Signal [Low, High] \
             fn main(signal: Signal) -> i32 { match (signal) { Signal.Low => { return 1i32; } \
             Signal.High => { return 2i32; } } }",
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1202_UNKNOWN_VALUE_NAME"));
    }

    #[test]
    fn parser_records_the_whole_pattern_path() {
        let (source, outcome) = parse(
            "enum Signal [Low] \
             fn main(signal: Signal) -> i32 { match (signal) { upstream.Signal.Low => { return 1i32; } } }",
        );
        let schema = outcome.into_accepted().expect("a qualified path parses");
        let branch = &schema.functions()[0].body().statements()[0].branches()[0];
        let pattern = branch.pattern();
        assert!(pattern.is_qualified());
        let path: Vec<&str> = pattern.path().iter().map(|s| s.text(&source)).collect();
        assert_eq!(path, ["upstream", "Signal", "Low"]);
        assert_eq!(pattern.name().unwrap().text(&source), "Low");
    }

    #[test]
    fn a_non_unit_function_must_return_on_every_path() {
        let (source, diagnostics) = check(
            "fn ok(ready: bool) -> i32 { if (ready) { return 1i32; } else { return 2i32; } } \
             fn bad(ready: bool) -> i32 { if (ready) { return 1i32; } } \
             fn unit_is_fine(ready: bool) -> unit { if (ready) { return; } }",
        );
        let missing: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1221_MISSING_RETURN")
            .map(|d| d.span().text(&source))
            .collect();
        assert_eq!(missing, ["bad"]);
        assert_eq!(
            diagnostics
                .iter()
                .find(|d| d.code() == "E1221_MISSING_RETURN")
                .unwrap()
                .field("scope"),
            Some("function")
        );
    }

    #[test]
    fn a_diverging_loop_and_a_returning_match_complete_the_paths() {
        let (_, diagnostics) = check(
            "enum Signal [Low, High] \
             fn spin() -> i32 { loop { } } \
             fn choose(signal: Signal) -> i32 { match (signal) { Low => { return 1i32; } \
             High => { return 2i32; } } }",
        );
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code() != "E1221_MISSING_RETURN"),
            "a loop without break and an all-returning match both diverge"
        );
    }

    #[test]
    fn a_loop_with_a_break_still_needs_a_return() {
        let (source, diagnostics) = check("fn spin() -> i32 { loop { break; } }");
        let missing = diagnostics
            .iter()
            .find(|d| d.code() == "E1221_MISSING_RETURN")
            .expect("a loop that can break falls through");
        assert_eq!(missing.span().text(&source), "spin");
    }

    #[test]
    fn a_closure_may_not_mix_a_value_return_with_a_fallthrough() {
        let (_, diagnostics) = check(
            "fn main(ready: bool) -> unit { \
             let step: fn (bool) -> i32 = fn (flag: bool) { if (flag) { return 1i32; } }; }",
        );
        let missing = diagnostics
            .iter()
            .find(|d| d.code() == "E1221_MISSING_RETURN")
            .expect("the closure body falls through past a value return");
        assert_eq!(missing.field("scope"), Some("closure"));
    }

    #[test]
    fn a_return_inside_a_closure_does_not_satisfy_its_enclosing_function() {
        let (source, diagnostics) =
            check("fn main() -> i32 { let step: fn () -> i32 = fn () { return 1i32; }; }");
        let missing: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1221_MISSING_RETURN")
            .map(|d| d.span().text(&source))
            .collect();
        assert_eq!(
            missing,
            ["main"],
            "each return scope is analysed on its own"
        );
    }

    #[test]
    fn bootstrap_rejects_the_first_full_profile_feature() {
        let (source, diagnostics) =
            check("fn main() -> unit { defer { } } async fn later() -> unit { }");
        let profile: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1702_PROFILE_NOT_SUPPORTED")
            .collect();
        assert_eq!(profile.len(), 1, "docs/42 asks for the first feature only");
        assert_eq!(profile[0].field("feature"), Some("defer"));
        assert_eq!(profile[0].stage(), Stage::Resource);
        assert_eq!(profile[0].span().text(&source), "defer { }");
    }

    #[test]
    fn bootstrap_rejects_each_full_only_construct() {
        let cases = [
            ("fn main() -> unit { unsafe { } }", "unsafe"),
            ("extern fn outside() -> unit;", "extern"),
            ("async fn later() -> unit { }", "async fn"),
            (
                "fn main() -> unit { let step: fn () -> unit = fn () { }; }",
                "closure",
            ),
            (
                "fn main() -> unit { let task: Task<i32> = spawn async { return 1i32; }; }",
                "spawn async",
            ),
            (
                "fn main(task: Task<i32>) -> unit { let value: i32 = await task; }",
                "await",
            ),
        ];
        for (body, feature) in cases {
            let (_, diagnostics) = check(body);
            let found = diagnostics
                .iter()
                .find(|d| d.code() == "E1702_PROFILE_NOT_SUPPORTED")
                .unwrap_or_else(|| std::panic!("{feature} must be rejected under bootstrap"));
            assert_eq!(found.field("feature"), Some(feature));
        }
    }

    #[test]
    fn bootstrap_admits_its_own_concurrency_forms() {
        // `parallel`, `spawn parallel` and `cancel` have defined serialized
        // Bootstrap semantics.
        let (_, diagnostics) = check(
            "fn main() -> unit { parallel { } let task: Task<i32> = spawn parallel { return 1i32; }; cancel task; }",
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1702_PROFILE_NOT_SUPPORTED"));
    }

    #[test]
    fn a_full_profile_module_admits_full_constructs() {
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] async fn later() -> unit { }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1702_PROFILE_NOT_SUPPORTED"));
    }

    #[test]
    fn assignment_requires_a_mutable_binding() {
        let (source, diagnostics) = check(
            "record Point [x: i32] \
             fn main(fixed: i32) -> unit { let mut count: i32 = 1i32; count = 2i32; \
             let total: i32 = 3i32; total = 4i32; fixed = 5i32; }",
        );
        let immutable: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1201_ASSIGN_TO_IMMUTABLE")
            .filter_map(|d| d.field("binding"))
            .collect();
        assert_eq!(immutable, ["total", "fixed"]);
        assert_eq!(
            diagnostics
                .iter()
                .find(|d| d.code() == "E1201_ASSIGN_TO_IMMUTABLE")
                .unwrap()
                .span()
                .text(&source),
            "total"
        );
    }

    #[test]
    fn mutability_follows_the_root_of_a_place() {
        let (_, diagnostics) = check(
            "record Point [x: i32] \
             fn main() -> unit { let mut movable: Point = Point(x: 1i32); movable.x = 2i32; \
             let fixed: Point = Point(x: 3i32); fixed.x = 4i32; }",
        );
        let immutable: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1201_ASSIGN_TO_IMMUTABLE")
            .filter_map(|d| d.field("binding"))
            .collect();
        assert_eq!(immutable, ["fixed"]);
    }

    #[test]
    fn only_an_exclusive_borrow_parameter_is_assignable() {
        let (_, diagnostics) = check(
            "fn takes_mut(borrow mut slot: i32) -> unit { slot = 1i32; } \
             fn takes_shared(borrow slot: i32) -> unit { slot = 1i32; }",
        );
        let immutable: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1201_ASSIGN_TO_IMMUTABLE")
            .filter_map(|d| d.field("binding"))
            .collect();
        assert_eq!(immutable, ["slot"], "only the shared borrow is reported");
    }

    #[test]
    fn an_unbound_assignment_target_is_reported_once() {
        let (_, diagnostics) = check("fn main() -> unit { missing = 1i32; }");
        assert_eq!(
            diagnostics
                .iter()
                .filter(
                    |d| d.field("name") == Some("missing") || d.field("binding") == Some("missing")
                )
                .count(),
            1,
            "an unbound name is E1202 only"
        );
    }

    #[test]
    fn an_extern_item_has_no_accepted_ffi_interface() {
        let (source, diagnostics) = check("extern fn outside(value: i32) -> i32;");
        let ffi = diagnostics
            .iter()
            .find(|d| d.code() == "E1801_FFI_NOT_AVAILABLE")
            .expect("extern is reserved but unavailable in V1");
        assert_eq!(ffi.stage(), Stage::Effect);
        assert_eq!(ffi.field("item"), Some("outside"));
        assert!(ffi.span().text(&source).starts_with("extern fn outside"));
    }

    #[test]
    fn an_unsafe_block_requires_a_leading_safety_rationale() {
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] \
             fn documented() -> unit { unsafe {\n    // SAFETY: the caller holds the device grant.\n    } } \
             fn bare() -> unit { unsafe { } }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        let missing: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1802_UNSAFE_RATIONALE_REQUIRED")
            .collect();
        assert_eq!(missing.len(), 1, "only the undocumented block is reported");
        assert_eq!(missing[0].stage(), Stage::Effect);
        assert_eq!(
            missing[0].field("expected"),
            Some("leading SAFETY: line comment")
        );
    }

    #[test]
    fn a_trailing_safety_comment_is_not_a_rationale() {
        // docs/40 section 7 requires the comment to lead the block.
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] \
             fn late() -> unit { unsafe {\n    let value: i32 = 1i32;\n    // SAFETY: too late.\n    } }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert!(diagnostics
            .iter()
            .any(|d| d.code() == "E1802_UNSAFE_RATIONALE_REQUIRED"));
    }

    fn check_header(version: &str) -> Vec<Diagnostic> {
        let text = alloc::format!(
            "module system.boot version {version} profile bootstrap; resource [fuel: 1000] \
             fn main() -> unit {{ }}"
        );
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        Checker::check(&source, &schema)
            .into_iter()
            .filter(|d| d.code().starts_with("E16"))
            .collect()
    }

    #[test]
    fn the_declared_language_version_must_be_exactly_one_zero() {
        assert!(check_header("1.0").is_empty());

        let major = check_header("2.0");
        assert_eq!(major.len(), 1);
        assert_eq!(major[0].code(), "E1601_UNSUPPORTED_LANGUAGE_VERSION");
        assert_eq!(major[0].field("declared"), Some("2"));
        assert_eq!(major[0].stage(), Stage::Type);

        let minor = check_header("1.3");
        assert_eq!(minor.len(), 1);
        assert_eq!(minor[0].code(), "E1602_UNSUPPORTED_LANGUAGE_MINOR");
        assert_eq!(minor[0].field("declared"), Some("3"));
        assert_eq!(minor[0].field("supported"), Some("0"));
    }

    #[test]
    fn an_unsupported_major_hides_the_minor_finding() {
        // One header cannot be wrong in two ways at once.
        let both = check_header("2.7");
        assert_eq!(both.len(), 1);
        assert_eq!(both[0].code(), "E1601_UNSUPPORTED_LANGUAGE_VERSION");
    }

    #[test]
    fn a_named_constructor_names_every_declared_field_once() {
        let (source, diagnostics) = check(
            "record Point [x: i32, y: i32] \
             fn main() -> unit { let partial: Point = Point(x: 1i32); \
             let stray: Point = Point(x: 1i32, y: 2i32, z: 3i32); }",
        );
        let missing = diagnostics
            .iter()
            .find(|d| d.code() == "E1206_MISSING_RECORD_FIELD")
            .expect("an omitted field is reported");
        assert_eq!(missing.field("field"), Some("y"));
        assert_eq!(missing.field("constructor"), Some("Point"));
        assert_eq!(missing.stage(), Stage::Type);

        let unknown = diagnostics
            .iter()
            .find(|d| d.code() == "E1207_UNKNOWN_RECORD_FIELD")
            .expect("an unknown field is reported");
        assert_eq!(unknown.field("field"), Some("z"));
        assert_eq!(unknown.span().text(&source), "z");
    }

    #[test]
    fn a_named_field_enum_variant_uses_the_same_rule() {
        let (_, diagnostics) = check(
            "enum Colour [Rgb [red: u8, green: u8, blue: u8]] \
             fn main() -> unit { let partial: Colour = Rgb(red: 1u8, green: 2u8); }",
        );
        let missing: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1206_MISSING_RECORD_FIELD")
            .filter_map(|d| d.field("field"))
            .collect();
        assert_eq!(missing, ["blue"]);
    }

    #[test]
    fn a_complete_constructor_checks_clean() {
        let (_, diagnostics) = check(
            "record Point [x: i32, y: i32] \
             fn main() -> unit { let whole: Point = Point(y: 2i32, x: 1i32); }",
        );
        assert!(
            diagnostics.iter().all(|d| !d.code().starts_with("E120")),
            "field order does not matter"
        );
    }

    #[test]
    fn an_ordinary_call_is_not_checked_against_fields() {
        // Only a callee naming a local constructor has a declared field list.
        let (_, diagnostics) = check(
            "record Point [x: i32, y: i32] \
             fn helper(value: i32) -> i32 { return value; } \
             fn main() -> unit { let value: i32 = helper(1i32); }",
        );
        assert!(diagnostics.iter().all(|d| !d.code().starts_with("E120")));
    }

    fn check_full(body: &str) -> (SourceUnit, Vec<Diagnostic>) {
        let text = alloc::format!(
            "module system.boot version 1.0 profile full; resource [fuel: 1000] {body}"
        );
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        (source, diagnostics)
    }

    #[test]
    fn a_defer_body_may_not_divert_control_or_start_work() {
        let (_, diagnostics) = check_full(
            "fn main(task: Task<i32>) -> unit { defer {\n    // SAFETY: unused here.\n    return; } \
             defer { let value: i32 = join task; } \
             defer { let started: Task<i32> = spawn async { return 1i32; }; } }",
        );
        let operations: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1225_INVALID_DEFER")
            .filter_map(|d| d.field("operation"))
            .collect();
        assert_eq!(operations, ["return", "join", "spawn"]);
        assert!(diagnostics
            .iter()
            .filter(|d| d.code() == "E1225_INVALID_DEFER")
            .all(|d| d.stage() == Stage::Type));
    }

    #[test]
    fn a_break_inside_a_loop_of_the_defer_body_is_allowed() {
        // The break targets that loop, not the cleanup block.
        let (_, diagnostics) = check_full("fn main() -> unit { defer { loop { break; } } }");
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1225_INVALID_DEFER"));

        let (_, bare) = check_full("fn main() -> unit { defer { break; } }");
        assert_eq!(
            bare.iter()
                .filter(|d| d.code() == "E1225_INVALID_DEFER")
                .filter_map(|d| d.field("operation"))
                .collect::<Vec<_>>(),
            ["break"]
        );
    }

    #[test]
    fn a_closure_inside_a_defer_body_keeps_its_own_return_scope() {
        let (_, diagnostics) = check_full(
            "fn main() -> unit { defer { let step: fn () -> i32 = fn () { return 1i32; }; } }",
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1225_INVALID_DEFER"));
    }

    fn module_source(name: &str, imports: &str) -> SourceUnit {
        let text = alloc::format!(
            "module {name} version 1.0 profile bootstrap; {imports} resource [fuel: 1000] \
             fn main() -> unit {{ }}"
        );
        SourceReader::read(text.as_bytes()).expect("transport-valid source")
    }

    fn module_schema(source: &SourceUnit) -> Schema {
        Parser::parse_schema(source)
            .into_accepted()
            .expect("module must parse")
    }

    #[test]
    fn a_module_path_must_match_its_declared_name() {
        let source = module_source("system.boot.init", "");
        let schema = module_schema(&source);
        let matched =
            check_module_set(&[ModuleEntry::new("system/boot/init.tos", &source, &schema)]);
        assert!(matched.is_empty());

        let mismatched = check_module_set(&[ModuleEntry::new("system/init.tos", &source, &schema)]);
        assert_eq!(mismatched.len(), 1);
        assert_eq!(mismatched[0].code(), "E1603_MODULE_PATH_MISMATCH");
        assert_eq!(
            mismatched[0].field("expected"),
            Some("system/boot/init.tos")
        );
        assert_eq!(mismatched[0].stage(), Stage::Type);
    }

    #[test]
    fn an_import_must_name_a_module_of_the_source_set() {
        let source = module_source("app.main", "import app.missing;");
        let schema = module_schema(&source);
        let diagnostics = check_module_set(&[ModuleEntry::new("app/main.tos", &source, &schema)]);
        let missing = diagnostics
            .iter()
            .find(|d| d.code() == "E1604_IMPORT_NOT_FOUND")
            .expect("an unresolvable import is reported");
        assert_eq!(missing.field("import"), Some("app.missing"));
        assert_eq!(missing.field("importer"), Some("app.main"));
    }

    #[test]
    fn a_module_name_declared_twice_makes_its_import_ambiguous() {
        // docs/42 section 1: resolution reads only the declared source set. A
        // set holding the same name twice offers two candidates and nothing to
        // choose between them.
        let importer = module_source("app.main", "import app.shared;");
        let one = module_source("app.shared", "");
        let other = module_source("app.shared", "");
        let importer_schema = module_schema(&importer);
        let one_schema = module_schema(&one);
        let other_schema = module_schema(&other);
        let diagnostics = check_module_set(&[
            ModuleEntry::new("app/main.tos", &importer, &importer_schema),
            ModuleEntry::new("app/shared.tos", &one, &one_schema),
            ModuleEntry::new("app/shared.tos", &other, &other_schema),
        ]);
        let ambiguous = diagnostics
            .iter()
            .find(|d| d.code() == "E1605_AMBIGUOUS_IMPORT")
            .expect("two candidates cannot resolve deterministically");
        assert_eq!(ambiguous.field("import"), Some("app.shared"));
        assert_eq!(ambiguous.field("candidates"), Some("2"));
        assert_eq!(
            codes(&diagnostics, "E1604_IMPORT_NOT_FOUND"),
            0,
            "an ambiguous import is not also a missing one"
        );
    }

    #[test]
    fn an_earlier_declared_root_shadows_a_later_one() {
        // ADR-0038: the order settles roots, so layering a private root over a
        // shared one resolves rather than colliding.
        let importer = module_source("app.main", "import app.shared;");
        let private = module_source("app.shared", "");
        let shared = module_source("app.shared", "");
        let importer_schema = module_schema(&importer);
        let private_schema = module_schema(&private);
        let shared_schema = module_schema(&shared);
        let diagnostics = check_module_set(&[
            ModuleEntry::in_root(0, "app/main.tos", &importer, &importer_schema),
            ModuleEntry::in_root(0, "app/shared.tos", &private, &private_schema),
            ModuleEntry::in_root(1, "app/shared.tos", &shared, &shared_schema),
        ]);
        assert_eq!(codes(&diagnostics, "E1605_AMBIGUOUS_IMPORT"), 0);
        assert_eq!(codes(&diagnostics, "E1604_IMPORT_NOT_FOUND"), 0);
    }

    #[test]
    fn two_declared_dependencies_offering_one_name_collide() {
        // Nothing orders dependency source sets against each other, so the
        // root order cannot decide this one.
        let importer = module_source("app.main", "import app.shared;");
        let one = module_source("app.shared", "");
        let other = module_source("app.shared", "");
        let importer_schema = module_schema(&importer);
        let one_schema = module_schema(&one);
        let other_schema = module_schema(&other);
        let diagnostics = check_module_set(&[
            ModuleEntry::in_root(0, "app/main.tos", &importer, &importer_schema),
            ModuleEntry::from_dependency("vendor.a", 1, "app/shared.tos", &one, &one_schema),
            ModuleEntry::from_dependency("vendor.b", 2, "app/shared.tos", &other, &other_schema),
        ]);
        let ambiguous = diagnostics
            .iter()
            .find(|d| d.code() == "E1605_AMBIGUOUS_IMPORT")
            .expect("nothing orders two declared dependencies");
        assert_eq!(ambiguous.field("collision"), Some("dependency"));
        assert_eq!(ambiguous.field("collided"), Some("vendor.a, vendor.b"));
    }

    #[test]
    fn one_root_declaring_a_name_twice_collides() {
        let importer = module_source("app.main", "import app.shared;");
        let one = module_source("app.shared", "");
        let other = module_source("app.shared", "");
        let importer_schema = module_schema(&importer);
        let one_schema = module_schema(&one);
        let other_schema = module_schema(&other);
        let diagnostics = check_module_set(&[
            ModuleEntry::in_root(0, "app/main.tos", &importer, &importer_schema),
            ModuleEntry::in_root(1, "app/shared.tos", &one, &one_schema),
            ModuleEntry::in_root(1, "app/shared.tos", &other, &other_schema),
        ]);
        let ambiguous = diagnostics
            .iter()
            .find(|d| d.code() == "E1605_AMBIGUOUS_IMPORT")
            .expect("one root cannot declare a name twice");
        assert_eq!(ambiguous.field("collision"), Some("root"));
        assert_eq!(ambiguous.field("candidates"), Some("2"));
    }

    #[test]
    fn a_capability_import_is_not_resolved_against_the_source_set() {
        // docs/42 section 4: a capability names an interface contract, not a
        // module of this set.
        let source = module_source("app.main", "import capability system.time.Clock as clock;");
        let schema = module_schema(&source);
        let diagnostics = check_module_set(&[ModuleEntry::new("app/main.tos", &source, &schema)]);
        assert_eq!(codes(&diagnostics, "E1604_IMPORT_NOT_FOUND"), 0);
        assert_eq!(codes(&diagnostics, "E1605_AMBIGUOUS_IMPORT"), 0);
    }

    #[test]
    fn a_loop_with_no_bound_and_no_fuel_is_unmetered() {
        // docs/41 section 6: a loop must have a statically proven finite bound
        // or consume fuel. `fuel: 0` leaves nothing for a back edge to consume.
        let text = "module system.boot version 1.0 profile bootstrap; resource [fuel: 0] \
             pub fn main() -> unit { loop { } }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        let unmetered = diagnostics
            .iter()
            .find(|d| d.code() == "E1701_UNMETERED_LOOP")
            .expect("an unbounded loop with no fuel is unmetered");
        assert_eq!(unmetered.stage(), Stage::Resource);
        assert_eq!(unmetered.field("form"), Some("loop"));
    }

    #[test]
    fn fuel_meters_a_loop_that_has_no_static_bound() {
        let text = "module system.boot version 1.0 profile bootstrap; resource [fuel: 1000] \
             pub fn main() -> unit { loop { } }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert_eq!(codes(&diagnostics, "E1701_UNMETERED_LOOP"), 0);
    }

    #[test]
    fn a_for_loop_is_bounded_by_the_sequence_it_iterates() {
        let text = "module system.boot version 1.0 profile bootstrap; resource [fuel: 0] \
             pub fn main() -> unit { let values: array<i32, 2> = [0, 0]; \
             for value in (values) { } }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert_eq!(
            codes(&diagnostics, "E1701_UNMETERED_LOOP"),
            0,
            "the iteration count is the sequence length, which is finite"
        );
    }

    #[test]
    fn an_import_cycle_is_reported_once_with_its_ordered_path() {
        let first = module_source("app.first", "import app.second;");
        let second = module_source("app.second", "import app.first;");
        let first_schema = module_schema(&first);
        let second_schema = module_schema(&second);
        let diagnostics = check_module_set(&[
            ModuleEntry::new("app/first.tos", &first, &first_schema),
            ModuleEntry::new("app/second.tos", &second, &second_schema),
        ]);
        let cycles: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1606_IMPORT_CYCLE")
            .collect();
        assert_eq!(cycles.len(), 1, "one cycle is one finding");
        assert_eq!(
            cycles[0].field("cycle"),
            Some("app.first -> app.second -> app.first")
        );
        assert_eq!(cycles[0].field("members"), Some("2"));
    }

    #[test]
    fn an_acyclic_import_graph_resolves_clean() {
        let leaf = module_source("app.leaf", "");
        let mid = module_source("app.mid", "import app.leaf;");
        let root = module_source("app.root", "import app.mid; import app.leaf;");
        let leaf_schema = module_schema(&leaf);
        let mid_schema = module_schema(&mid);
        let root_schema = module_schema(&root);
        let diagnostics = check_module_set(&[
            ModuleEntry::new("app/leaf.tos", &leaf, &leaf_schema),
            ModuleEntry::new("app/mid.tos", &mid, &mid_schema),
            ModuleEntry::new("app/root.tos", &root, &root_schema),
        ]);
        assert!(
            diagnostics.is_empty(),
            "a shared dependency is not a cycle: {:?}",
            diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_resolved_diagnostic_carries_its_module_identity() {
        // docs/41 section 7 requires module name, canonical path and normalized
        // source content ID on every diagnostic.
        let source = module_source("app.main", "import app.missing;");
        let schema = module_schema(&source);
        let entry = ModuleEntry::new("app/main.tos", &source, &schema);
        let diagnostics = check_source_set(&[entry]);
        assert!(!diagnostics.is_empty());
        for diagnostic in &diagnostics {
            let identity = diagnostic
                .module()
                .expect("a resolved diagnostic names its module");
            assert_eq!(identity.name(), "app.main");
            assert_eq!(identity.path(), "app/main.tos");
            assert!(identity.content_id().starts_with("sha256:"));
            assert_eq!(identity.content_id().len(), "sha256:".len() + 64);
            assert!(identity.source_set().is_none(), "the driver supplies it");
        }
    }

    #[test]
    fn the_content_id_names_the_normalized_bytes() {
        // CRLF is normalized before the source unit exists, so the transport
        // form does not change the identity.
        let lf =
            SourceReader::read(b"module a.b version 1.0 profile bootstrap;\nresource [fuel: 1]\n")
                .expect("transport-valid source");
        let crlf = SourceReader::read(
            b"module a.b version 1.0 profile bootstrap;\r\nresource [fuel: 1]\r\n",
        )
        .expect("transport-valid source");
        let lf_schema = module_schema(&lf);
        let crlf_schema = module_schema(&crlf);
        assert_eq!(
            ModuleEntry::new("a/b.tos", &lf, &lf_schema)
                .identity()
                .content_id(),
            ModuleEntry::new("a/b.tos", &crlf, &crlf_schema)
                .identity()
                .content_id()
        );
    }

    #[test]
    fn a_diagnostic_without_a_resolver_carries_no_identity() {
        let (_, diagnostics) = check("fn main() -> i32 { return missing; }");
        assert!(
            diagnostics.iter().all(|d| d.module().is_none()),
            "a source unit alone cannot supply a path, so none is invented"
        );
    }

    #[test]
    fn retained_diagnostics_are_bounded_per_module() {
        // docs/44 section 2 bounds diagnostics like every other frontend input:
        // hostile source must not be able to make the list grow without limit.
        let mut text = String::from("module system.boot version 1.0 profile bootstrap; ");
        text.push_str("resource [fuel: 1000] ");
        for _ in 0..(MAX_DIAGNOSTICS_PER_MODULE * 2) {
            text.push_str("enum Broken {Value} ");
        }
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let outcome = Parser::parse_schema(&source);
        assert_eq!(outcome.diagnostics().len(), MAX_DIAGNOSTICS_PER_MODULE);
        assert!(outcome.is_truncated());
        assert!(outcome.has_errors());
        // The retained ones are the earliest, so the first problem is kept.
        assert_eq!(outcome.diagnostics()[0].span().text(&source), "{");
    }

    #[test]
    fn a_clean_parse_is_not_marked_truncated() {
        let (_, outcome) = parse("fn main() -> i32 { return 1i32; }");
        assert!(!outcome.is_truncated());
        assert!(outcome.diagnostics().is_empty());
    }

    #[test]
    fn an_unresolved_type_name_is_reported() {
        let (source, diagnostics) =
            check("fn main() -> i32 { let value: Missing = 1i32; return 0i32; }");
        let unknown = diagnostics
            .iter()
            .find(|d| d.code() == "E1203_UNKNOWN_TYPE_NAME")
            .expect("an unresolved type name is rejected");
        assert_eq!(unknown.stage(), Stage::Type);
        assert_eq!(unknown.field("type"), Some("Missing"));
        assert_eq!(unknown.span().text(&source), "Missing");
    }

    #[test]
    fn every_v1_type_form_resolves() {
        let (_, diagnostics) = check(
            "record Point [x: i32] enum Signal [Low] \
             fn main(a: Option<i32>, b: Result<i32, Signal>, c: array<Point, 2>, \
             d: (i32, bool), e: fn (i32) -> unit, f: AtomicU64, g: slice<u8>) -> unit { }",
        );
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code() != "E1203_UNKNOWN_TYPE_NAME"),
            "primitives, predeclared, local and constructed forms all resolve"
        );
    }

    #[test]
    fn a_constructor_arity_mismatch_names_both_arities() {
        let (_, diagnostics) = check("fn main(a: Option<i32, bool>, b: Result<i32>) -> unit { }");
        let arity: Vec<(&str, &str, &str)> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1204_TYPE_ARGUMENT_ARITY")
            .map(|d| {
                (
                    d.field("constructor").unwrap_or_default(),
                    d.field("expected_arity").unwrap_or_default(),
                    d.field("actual_arity").unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(arity, [("Option", "1", "2"), ("Result", "2", "1")]);
    }

    #[test]
    fn an_unresolved_name_precedes_an_arity_finding() {
        // ADR-0034 section 3: the arity of a type that does not exist is not a
        // fact, so one mistake does not become two diagnostics.
        let (_, diagnostics) = check("fn main(a: Missing<i32, bool>) -> unit { }");
        let codes: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code().starts_with("E120"))
            .map(|d| d.code())
            .collect();
        assert_eq!(codes, ["E1203_UNKNOWN_TYPE_NAME"]);
    }

    #[test]
    fn a_wrongly_applied_constructor_does_not_cascade() {
        // The bad argument inside is not reported: the constructed type it
        // would belong to does not exist.
        let (_, diagnostics) = check("fn main(a: Option<Missing, bool>) -> unit { }");
        let codes: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code().starts_with("E120"))
            .map(|d| d.code())
            .collect();
        assert_eq!(codes, ["E1204_TYPE_ARGUMENT_ARITY"]);
    }

    #[test]
    fn a_qualified_type_resolves_against_the_module_its_binding_names() {
        let upstream = module_source("app.upstream", "");
        let text = "module app.client version 1.0 profile bootstrap; import app.upstream as up; \
             resource [fuel: 1000] fn main(value: up.Missing) -> unit { }";
        let client = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let upstream_schema = module_schema(&upstream);
        let client_schema = module_schema(&client);
        let diagnostics = check_source_set(&[
            ModuleEntry::new("app/client.tos", &client, &client_schema),
            ModuleEntry::new("app/upstream.tos", &upstream, &upstream_schema),
        ]);
        let unknown = diagnostics
            .iter()
            .find(|d| d.code() == "E1203_UNKNOWN_TYPE_NAME")
            .expect("the import resolves, so the missing type is a type-name error");
        assert_eq!(unknown.field("type"), Some("up.Missing"));
        assert_eq!(unknown.field("module"), Some("app.upstream"));
    }

    #[test]
    fn a_match_over_an_enum_must_cover_every_variant() {
        let (_, diagnostics) = check(
            "enum Signal [Low, High, Mute] \
             fn main(signal: Signal) -> i32 { match (signal) { Low => { return 1i32; } } }",
        );
        let missing = diagnostics
            .iter()
            .find(|d| d.code() == "E1220_NONEXHAUSTIVE_MATCH")
            .expect("a missing case is reported");
        assert_eq!(missing.stage(), Stage::Type);
        assert_eq!(missing.field("subject"), Some("Signal"));
        assert_eq!(missing.field("missing"), Some("High, Mute"));
        assert_eq!(missing.field("missing_count"), Some("2"));
    }

    #[test]
    fn a_wildcard_or_binding_arm_is_exhaustive() {
        // ADR-0033: a bare name that is not a variant of the expected type
        // binds, and a binding matches every value.
        for arm in ["_", "other"] {
            let body = alloc::format!(
                "enum Signal [Low, High] \
                 fn main(signal: Signal) -> i32 {{ match (signal) {{ Low => {{ return 1i32; }} \
                 {arm} => {{ return 0i32; }} }} }}"
            );
            let (_, diagnostics) = check(&body);
            assert!(
                diagnostics
                    .iter()
                    .all(|d| d.code() != "E1220_NONEXHAUSTIVE_MATCH"),
                "{arm} covers the rest"
            );
        }
    }

    #[test]
    fn qualified_and_payload_arms_count_as_coverage() {
        let (_, diagnostics) = check(
            "enum Reading [Empty, Sample(i32)] \
             fn main(reading: Reading) -> i32 { match (reading) { Reading.Empty => { return 0i32; } \
             Sample(amount) => { return amount; } } }",
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1220_NONEXHAUSTIVE_MATCH"));
    }

    #[test]
    fn predeclared_sums_are_exhaustive_over_their_own_variants() {
        let (_, incomplete) = check(
            "fn main(value: Option<i32>) -> i32 { match (value) { Some(inner) => { return inner; } } }",
        );
        assert_eq!(
            incomplete
                .iter()
                .find(|d| d.code() == "E1220_NONEXHAUSTIVE_MATCH")
                .and_then(|d| d.field("missing")),
            Some("None")
        );

        let (_, complete) = check(
            "fn main(value: Result<i32, i32>) -> i32 { match (value) { Ok(inner) => { return inner; } \
             Err(problem) => { return problem; } } }",
        );
        assert!(complete
            .iter()
            .all(|d| d.code() != "E1220_NONEXHAUSTIVE_MATCH"));
    }

    #[test]
    fn a_scrutinee_without_a_stated_type_is_not_analysed() {
        // Reporting here would need inference this slice does not do, and a
        // guess could invent a missing case.
        let (_, diagnostics) = check(
            "enum Signal [Low, High] fn pick() -> Signal { return Low; } \
             fn main() -> i32 { let chosen = pick(); match (chosen) { Low => { return 1i32; } } }",
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1220_NONEXHAUSTIVE_MATCH"));
    }

    #[test]
    fn a_returned_value_must_have_the_declared_result_type() {
        let (_, diagnostics) = check(
            "fn ok() -> i32 { return 1i32; } \
             fn wrong(ready: bool) -> i32 { return ready; }",
        );
        let mismatch: Vec<(&str, &str)> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1222_RETURN_TYPE_MISMATCH")
            .map(|d| {
                (
                    d.field("expected").unwrap_or_default(),
                    d.field("actual").unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(mismatch, [("i32", "bool")]);
    }

    #[test]
    fn an_unsuffixed_literal_takes_the_declared_result_type() {
        // docs/40 section 3 contextually types it, so no width disagrees.
        let (_, diagnostics) =
            check("fn wide() -> i64 { return 1; } fn narrow() -> u8 { return 2; }");
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1222_RETURN_TYPE_MISMATCH"));
    }

    #[test]
    fn a_valueless_return_is_a_mismatch_in_a_non_unit_function() {
        let (_, diagnostics) = check("fn takes() -> i32 { return; }");
        let mismatch = diagnostics
            .iter()
            .find(|d| d.code() == "E1222_RETURN_TYPE_MISMATCH")
            .expect("`return;` does not produce an i32");
        assert_eq!(mismatch.field("expected"), Some("i32"));
        assert_eq!(mismatch.field("actual"), Some("unit"));

        let (_, unit) = check("fn nothing() -> unit { return; }");
        assert!(unit
            .iter()
            .all(|d| d.code() != "E1222_RETURN_TYPE_MISMATCH"));
    }

    #[test]
    fn types_flow_through_calls_fields_and_propagation() {
        let (_, diagnostics) = check(
            "record Point [x: i32, y: bool] \
             fn make() -> Point { return Point(x: 1i32, y: true); } \
             fn read() -> Result<i32, i32> { return Ok(1i32); } \
             fn field_is_bool() -> i32 { return make().y; } \
             fn question_unwraps() -> i32 { return read()?; }",
        );
        let mismatch: Vec<(&str, &str)> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1222_RETURN_TYPE_MISMATCH")
            .map(|d| {
                (
                    d.field("expected").unwrap_or_default(),
                    d.field("actual").unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            mismatch,
            [("i32", "bool")],
            "the field access disagrees; `?` unwrapping the Ok payload does not"
        );
    }

    #[test]
    fn a_binding_carries_its_annotation_or_its_initializer_type() {
        let (_, diagnostics) = check(
            "fn annotated() -> i32 { let flag: bool = true; return flag; } \
             fn inferred() -> i32 { let flag = true; return flag; }",
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code() == "E1222_RETURN_TYPE_MISMATCH")
                .count(),
            2,
            "both the annotated and the inferred binding are known to be bool"
        );
    }

    #[test]
    fn consuming_a_task_yields_its_outcome_type() {
        // docs/41: `join Task<T>` produces `TaskResult<T>`, which is not `T`.
        let (_, diagnostics) =
            check("fn main(task: Task<i32>) -> i32 { let outcome = join task; return outcome; }");
        let mismatch = diagnostics
            .iter()
            .find(|d| d.code() == "E1222_RETURN_TYPE_MISMATCH")
            .expect("TaskResult<i32> is not i32");
        assert_eq!(mismatch.field("expected"), Some("i32"));
        assert_eq!(mismatch.field("actual"), Some("TaskResult<i32>"));
    }

    #[test]
    fn an_undetermined_type_reports_nothing() {
        // A type from another module has a known identity but no shape here,
        // and a guess would invent a mismatch.
        let text = "module app.client version 1.0 profile bootstrap; import app.upstream as up; \
             resource [fuel: 1000] fn main(value: up.Reading) -> i32 { return value; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code() != "E1222_RETURN_TYPE_MISMATCH"),
            "a type from another module has no shape here"
        );
    }

    #[test]
    fn as_permits_only_signedness_preserving_widening() {
        let (_, diagnostics) = check(
            "fn widen(value: i32) -> i64 { return value as i64; } \
             fn widen_unsigned(value: u8) -> u32 { return value as u32; }",
        );
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code() != "E1212_INVALID_AS_CONVERSION"),
            "widening within one signedness is the permitted form"
        );
    }

    #[test]
    fn as_rejects_narrowing_and_sign_change() {
        let cases = [
            (
                "fn narrow(value: i32) -> i8 { return value as i8; }",
                "i32",
                "i8",
            ),
            (
                "fn resign(value: i32) -> u32 { return value as u32; }",
                "i32",
                "u32",
            ),
            (
                "fn same(value: i32) -> i32 { return value as i32; }",
                "i32",
                "i32",
            ),
            (
                "fn from_bool(flag: bool) -> i32 { return flag as i32; }",
                "bool",
                "i32",
            ),
        ];
        for (body, from, to) in cases {
            let (_, diagnostics) = check(body);
            let invalid = diagnostics
                .iter()
                .find(|d| d.code() == "E1212_INVALID_AS_CONVERSION")
                .unwrap_or_else(|| std::panic!("{from} as {to} must be rejected"));
            assert_eq!(invalid.field("from"), Some(from));
            assert_eq!(invalid.field("to"), Some(to));
            assert_eq!(invalid.stage(), Stage::Type);
        }
    }

    #[test]
    fn an_opaque_handle_cast_is_not_a_conversion_error() {
        // docs/40 section 3 routes these elsewhere; no code names the
        // condition for the non-capability handles.
        let (_, diagnostics) = check("fn main(task: Task<i32>) -> i32 { return task as i32; }");
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1212_INVALID_AS_CONVERSION"));
    }

    #[test]
    fn checked_narrowing_uses_the_conversion_functions() {
        // `to_u8` is the source form for a narrowing conversion, and it
        // produces a Result rather than a cast.
        let (_, diagnostics) =
            check("fn main(value: i32) -> Result<u8, ConversionError> { return to_u8(value); }");
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code() != "E1222_RETURN_TYPE_MISMATCH"
                    && d.code() != "E1212_INVALID_AS_CONVERSION"),
            "a checked conversion is a call, not a cast"
        );
    }

    #[test]
    fn integer_types_must_agree_when_assigned_or_passed() {
        let (_, diagnostics) = check(
            "fn takes(value: i32) -> i32 { return value; } \
             fn main(wide: i64) -> i32 { let mut slot: i32 = 1i32; slot = wide; \
             return takes(wide); }",
        );
        let mismatches: Vec<(&str, &str, &str)> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1210_INTEGER_TYPE_MISMATCH")
            .map(|d| {
                (
                    d.field("expected").unwrap_or_default(),
                    d.field("actual").unwrap_or_default(),
                    d.field("position").unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            mismatches,
            [("i32", "i64", "assignment"), ("i32", "i64", "argument")]
        );
    }

    #[test]
    fn an_unsuffixed_literal_takes_the_required_integer_type() {
        let (_, diagnostics) = check(
            "fn takes(value: u64) -> u64 { return value; } \
             fn main() -> u64 { let mut slot: u8 = 1; slot = 2; return takes(3); }",
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1210_INTEGER_TYPE_MISMATCH"));
    }

    #[test]
    fn size_is_its_own_integer_type() {
        let (_, diagnostics) = check(
            "fn main(count: size) -> i32 { let mut slot: i32 = 1i32; slot = count; return 0i32; }",
        );
        let mismatch = diagnostics
            .iter()
            .find(|d| d.code() == "E1210_INTEGER_TYPE_MISMATCH")
            .expect("size does not silently become i32");
        assert_eq!(mismatch.field("actual"), Some("size"));
    }

    #[test]
    fn a_disagreement_between_other_kinds_has_no_allocated_code() {
        // docs/40 section 3 names E1210 for integer types only; reporting a
        // bool-to-i32 assignment would need a code no document allocates.
        let (_, diagnostics) = check(
            "fn main(flag: bool) -> i32 { let mut slot: i32 = 1i32; slot = flag; return 0i32; }",
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1210_INTEGER_TYPE_MISMATCH"));
    }

    #[test]
    fn an_index_has_exact_type_size() {
        let (_, diagnostics) = check(
            "fn main(values: array<i32, 2>, position: i32, offset: size) -> i32 { \
             let a: i32 = values[0]; let b: i32 = values[offset]; \
             let c: i32 = values[position]; return a; }",
        );
        let mismatches: Vec<(&str, &str)> = diagnostics
            .iter()
            .filter(|d| d.code() == "E1211_INDEX_TYPE_MISMATCH")
            .map(|d| {
                (
                    d.field("expected").unwrap_or_default(),
                    d.field("actual").unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            mismatches,
            [("size", "i32")],
            "a literal and a size index are both accepted"
        );
    }

    #[test]
    fn indexing_yields_the_element_type() {
        let (_, diagnostics) =
            check("fn main(values: array<bool, 2>) -> i32 { return values[0]; }");
        let mismatch = diagnostics
            .iter()
            .find(|d| d.code() == "E1222_RETURN_TYPE_MISMATCH")
            .expect("the element type flows out of the index");
        assert_eq!(mismatch.field("actual"), Some("bool"));
    }

    #[test]
    fn a_public_signature_may_not_name_a_private_type() {
        let (source, diagnostics) = check(
            "record Hidden [value: i32] \
             pub fn leak(hidden: Hidden) -> i32 { return hidden.value; }",
        );
        let private = diagnostics
            .iter()
            .find(|d| d.code() == "E1607_PRIVATE_PUBLIC_TYPE")
            .expect("an importing module could not name Hidden");
        assert_eq!(private.stage(), Stage::Type);
        assert_eq!(private.field("type"), Some("Hidden"));
        assert_eq!(private.field("exported_by"), Some("leak"));
        assert_eq!(private.span().text(&source), "Hidden");
    }

    #[test]
    fn an_exported_wrapper_does_not_hide_a_private_type() {
        // docs/42 section 1 covers the transitive surface: a consumer cannot
        // construct Wrapper without naming Hidden.
        let (_, diagnostics) = check(
            "record Hidden [value: i32] pub record Wrapper [inner: Hidden] \
             pub fn get(seed: i32) -> Wrapper { return Wrapper(inner: Hidden(value: seed)); }",
        );
        let private = diagnostics
            .iter()
            .find(|d| d.code() == "E1607_PRIVATE_PUBLIC_TYPE")
            .expect("the private type is still in the public surface");
        assert_eq!(private.field("type"), Some("Hidden"));
        assert_eq!(private.field("exported_by"), Some("get"));
    }

    #[test]
    fn private_types_stay_legal_outside_a_public_surface() {
        let (_, diagnostics) = check(
            "record Scratch [tally: i32] pub record Reading [sample: i32] \
             fn helper(scratch: Scratch) -> Scratch { return scratch; } \
             pub fn observe(reading: Reading) -> Reading { \
             let working: Scratch = Scratch(tally: reading.sample); \
             return Reading(sample: working.tally); }",
        );
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code() != "E1607_PRIVATE_PUBLIC_TYPE"),
            "a body and a private function are implementation details"
        );
    }

    #[test]
    fn an_imported_type_is_reachable_in_a_public_signature() {
        let text = "module app.client version 1.0 profile bootstrap; import app.upstream as up; \
             resource [fuel: 1000] pub fn relay(reading: up.Reading) -> up.Reading { return reading; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1607_PRIVATE_PUBLIC_TYPE"));
    }

    #[test]
    fn a_recursive_exported_type_terminates() {
        let (_, diagnostics) = check(
            "pub record Node [next: Option<Node>] pub fn head(node: Node) -> Node { return node; }",
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1607_PRIVATE_PUBLIC_TYPE"));
    }

    #[test]
    fn an_affine_value_may_be_used_once() {
        let (source, diagnostics) = check(
            "pub record Message [payload: bytes] \
             fn take(message: Message) -> unit { } \
             pub fn main() -> unit { let message = Message(payload: b\"hi\"); \
             take(message); take(message); }",
        );
        let moved = diagnostics
            .iter()
            .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
            .expect("the second call uses a moved value");
        assert_eq!(moved.stage(), Stage::Ownership);
        assert_eq!(moved.field("place"), Some("message"));
        assert_eq!(moved.span().text(&source), "message");
        assert!(moved.field("moved_at").is_some());
    }

    #[test]
    fn a_copy_value_survives_repeated_use() {
        // docs/40 section 5 fixes the Copy set; a user record is never in it.
        let (_, diagnostics) = check(
            "fn take(value: i32) -> unit { } \
             pub fn main() -> unit { let count: i32 = 1i32; take(count); take(count); \
             let pair: (i32, bool) = (1i32, true); let a = pair; let b = pair; }",
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code() != "E1301_USE_AFTER_MOVE"));
    }

    #[test]
    fn a_field_read_after_a_move_is_a_use() {
        let (_, diagnostics) = check(
            "pub record Message [payload: bytes] \
             pub fn main(message: Message) -> size { let copied = message; \
             return message.payload[0B]; }",
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code() == "E1301_USE_AFTER_MOVE")
                .count(),
            1
        );
    }

    #[test]
    fn a_borrow_does_not_move() {
        let (_, diagnostics) = check(
            "pub record Message [payload: bytes] \
             fn inspect(borrow message: Message) -> unit { } \
             pub fn main(message: Message) -> unit { inspect(borrow message); \
             inspect(borrow message); }",
        );
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code() != "E1301_USE_AFTER_MOVE"),
            "a borrowed argument leaves ownership with the caller"
        );
    }

    #[test]
    fn an_aggregate_literal_takes_ownership_of_its_members() {
        let (_, diagnostics) = check(
            "pub record Message [payload: bytes] \
             pub fn main(message: Message) -> unit { let pair = (message, 1i32); \
             let again = message; }",
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code() == "E1301_USE_AFTER_MOVE")
                .count(),
            1,
            "the tuple literal moved the record"
        );
    }

    fn moves(diagnostics: &[Diagnostic]) -> usize {
        diagnostics
            .iter()
            .filter(|d| d.code() == "E1301_USE_AFTER_MOVE")
            .count()
    }

    const AFFINE: &str =
        "pub record Message [payload: bytes] fn take(message: Message) -> unit { } ";

    #[test]
    fn a_move_in_each_alternative_branch_is_correct() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(ready: bool, message: Message) -> unit {{ \
             if (ready) {{ take(message); }} else {{ take(message); }} }}"
        ));
        assert_eq!(
            moves(&diagnostics),
            0,
            "each arm starts from the entry state"
        );
    }

    #[test]
    fn a_move_on_one_path_blocks_a_later_use() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(ready: bool, message: Message) -> unit {{ \
             if (ready) {{ take(message); }} take(message); }}"
        ));
        assert_eq!(moves(&diagnostics), 1);
        let reported = diagnostics
            .iter()
            .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
            .unwrap();
        assert_eq!(reported.field("certainty"), Some("on some paths"));
    }

    #[test]
    fn no_move_in_either_branch_leaves_the_value_available() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} fn peek(borrow message: Message) -> unit {{ }} \
             pub fn main(ready: bool, message: Message) -> unit {{ \
             if (ready) {{ peek(borrow message); }} else {{ }} take(message); }}"
        ));
        assert_eq!(moves(&diagnostics), 0);
    }

    #[test]
    fn a_diverging_branch_does_not_contribute_its_moves() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(ready: bool, message: Message) -> unit {{ \
             if (ready) {{ take(message); return; }} take(message); }}"
        ));
        assert_eq!(
            moves(&diagnostics),
            0,
            "the branch that moved cannot reach the later use"
        );
    }

    #[test]
    fn nested_control_flow_joins_correctly() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} enum Mode [Fast, Slow] \
             pub fn main(mode: Mode, ready: bool, message: Message) -> unit {{ \
             match (mode) {{ Fast => {{ if (ready) {{ take(message); }} else {{ take(message); }} }} \
             Slow => {{ take(message); }} }} }}"
        ));
        assert_eq!(moves(&diagnostics), 0, "every path moves it exactly once");
    }

    #[test]
    fn a_match_arm_starts_from_the_entry_state() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} enum Mode [Fast, Slow] \
             pub fn main(mode: Mode, message: Message) -> unit {{ \
             match (mode) {{ Fast => {{ take(message); }} Slow => {{ take(message); }} }} \
             take(message); }}"
        ));
        assert_eq!(moves(&diagnostics), 1, "only the use after the join fails");
    }

    #[test]
    fn grouping_does_not_change_consuming_semantics() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(message: Message) -> unit {{ take((message)); take(message); }}"
        ));
        assert_eq!(moves(&diagnostics), 1, "parentheses name the same place");
    }

    #[test]
    fn a_match_subject_binds_by_move() {
        // docs/40 section 5: patterns bind by move unless the subject is an
        // immutable Copy value.
        let (_, diagnostics) = check(
            "enum Mode [Fast, Slow] fn use_mode(mode: Mode) -> unit { } \
             pub fn main(mode: Mode) -> unit { match (mode) { Fast => { } Slow => { } } \
             use_mode(mode); }",
        );
        assert_eq!(moves(&diagnostics), 1);
    }

    #[test]
    fn a_copy_subject_survives_a_match() {
        let (_, diagnostics) = check(
            "fn use_flag(flag: bool) -> unit { } \
             pub fn main(flag: bool) -> unit { match (flag) { _ => { } } use_flag(flag); }",
        );
        assert_eq!(moves(&diagnostics), 0);
    }

    #[test]
    fn a_move_inside_a_loop_is_seen_by_the_next_iteration() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(ready: bool, message: Message) -> unit {{ \
             while (ready) {{ take(message); }} }}"
        ));
        assert_eq!(moves(&diagnostics), 1, "the second iteration reuses it");
    }

    #[test]
    fn shadowing_does_not_disturb_the_outer_binding() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(message: Message) -> unit {{ \
             if (true) {{ let message = Message(payload: b\"inner\"); take(message); }} \
             take(message); }}"
        ));
        assert_eq!(
            moves(&diagnostics),
            0,
            "the inner binding is a different one"
        );
    }

    fn codes(diagnostics: &[Diagnostic], code: &str) -> usize {
        diagnostics.iter().filter(|d| d.code() == code).count()
    }

    const COUNTER: &str = "pub record Counter [value: i32, other: i32] \
         fn read(borrow counter: Counter) -> i32 { return counter.value; } \
         fn write(borrow mut counter: Counter) -> unit { counter.value = 1i32; } ";

    #[test]
    fn repeated_immutable_borrows_are_compatible() {
        let (_, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main(counter: Counter) -> i32 {{ \
             let first = borrow counter; let second = borrow counter; return read(borrow counter); }}"
        ));
        assert_eq!(codes(&diagnostics, "E1302_CONFLICTING_BORROW"), 0);
    }

    #[test]
    fn an_immutable_and_a_mutable_borrow_conflict() {
        let (source, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> i32 {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             let view = borrow counter; write(borrow mut counter); return 0i32; }}"
        ));
        let conflict = diagnostics
            .iter()
            .find(|d| d.code() == "E1302_CONFLICTING_BORROW")
            .expect("a mutable borrow cannot join a shared one");
        assert_eq!(conflict.stage(), Stage::Ownership);
        assert_eq!(conflict.field("borrow"), Some("borrow mut"));
        assert_eq!(conflict.field("conflicts_with"), Some("borrow"));
        assert_eq!(conflict.span().text(&source), "borrow mut counter");
    }

    #[test]
    fn a_mutable_borrow_is_exclusive() {
        let (_, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> unit {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             let first = borrow mut counter; write(borrow mut counter); }}"
        ));
        assert_eq!(codes(&diagnostics, "E1302_CONFLICTING_BORROW"), 1);
    }

    #[test]
    fn a_borrow_ends_with_its_region() {
        // A temporary borrow lives for its statement, so the next one is free.
        let (_, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> unit {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             write(borrow mut counter); write(borrow mut counter); read(borrow counter); }}"
        ));
        assert_eq!(codes(&diagnostics, "E1302_CONFLICTING_BORROW"), 0);
    }

    #[test]
    fn a_branch_local_borrow_does_not_reach_its_sibling() {
        let (_, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main(ready: bool) -> unit {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             if (ready) {{ let view = borrow mut counter; }} \
             else {{ let second = borrow mut counter; }} }}"
        ));
        assert_eq!(codes(&diagnostics, "E1302_CONFLICTING_BORROW"), 0);
    }

    #[test]
    fn borrows_of_unrelated_fields_do_not_conflict() {
        let (_, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> unit {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             let one = borrow mut counter.value; let two = borrow mut counter.other; }}"
        ));
        assert_eq!(
            codes(&diagnostics, "E1302_CONFLICTING_BORROW"),
            0,
            "a field borrow locks the containing path, not siblings"
        );
    }

    #[test]
    fn a_field_borrow_locks_its_containing_path() {
        let (_, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> unit {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             let inner = borrow mut counter.value; let whole = borrow counter; }}"
        ));
        assert_eq!(codes(&diagnostics, "E1302_CONFLICTING_BORROW"), 1);
    }

    #[test]
    fn constant_indices_are_disjoint_and_dynamic_ones_overlap() {
        let (_, disjoint) = check(
            "pub fn main() -> unit { let mut values: array<i32, 4> = [0, 0, 0, 0]; \
             let one = borrow mut values[0]; let two = borrow mut values[1]; }",
        );
        assert_eq!(codes(&disjoint, "E1302_CONFLICTING_BORROW"), 0);

        let (_, overlapping) = check(
            "pub fn main(at: size) -> unit { let mut values: array<i32, 4> = [0, 0, 0, 0]; \
             let one = borrow mut values[at]; let two = borrow mut values[1]; }",
        );
        assert_eq!(
            codes(&overlapping, "E1302_CONFLICTING_BORROW"),
            1,
            "an unknown index may hit any element"
        );
    }

    #[test]
    fn a_write_under_an_immutable_borrow_is_rejected() {
        let (source, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> unit {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             let view = borrow counter; counter.value = 1i32; }}"
        ));
        let mutated = diagnostics
            .iter()
            .find(|d| d.code() == "E1303_MUTATE_WHILE_BORROWED")
            .expect("an immutable borrow forbids mutation");
        assert_eq!(mutated.stage(), Stage::Ownership);
        assert_eq!(mutated.field("place"), Some("counter.value"));
        assert_eq!(mutated.span().text(&source), "counter.value");
    }

    #[test]
    fn a_write_to_an_unborrowed_field_is_allowed() {
        let (_, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> unit {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             let view = borrow counter.value; counter.other = 1i32; }}"
        ));
        assert_eq!(codes(&diagnostics, "E1303_MUTATE_WHILE_BORROWED"), 0);
    }

    #[test]
    fn a_write_with_no_live_borrow_is_allowed() {
        let (_, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> unit {{ let mut counter = Counter(value: 0i32, other: 0i32); counter.value = 1i32; }}"
        ));
        assert_eq!(codes(&diagnostics, "E1303_MUTATE_WHILE_BORROWED"), 0);
    }

    #[test]
    fn a_task_may_not_capture_a_mutable_binding_by_alias() {
        // docs/40 section 6: writing through a capture needs a mutable alias,
        // which is not Transferable.
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] \
             record Counter [value: i32] \
             pub fn main() -> unit { let mut counter = Counter(value: 0i32); \
             let first: Task<unit> = spawn parallel { counter.value = 1i32; }; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        let invalid = diagnostics
            .iter()
            .find(|d| d.code() == "E1304_INVALID_TASK_CAPTURE")
            .expect("an alias capture is not a transfer");
        assert_eq!(invalid.stage(), Stage::Ownership);
        assert_eq!(invalid.field("capture"), Some("counter"));
        assert_eq!(invalid.field("reason"), Some("mutable binding by alias"));
    }

    #[test]
    fn an_invalid_capture_does_not_also_move_the_value() {
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] \
             record Counter [value: i32] \
             pub fn main() -> unit { let mut counter = Counter(value: 0i32); \
             let first: Task<unit> = spawn parallel { counter.value = 1i32; }; \
             let second: Task<unit> = spawn parallel { counter.value = 2i32; }; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert_eq!(codes(&diagnostics, "E1304_INVALID_TASK_CAPTURE"), 2);
        assert_eq!(
            codes(&diagnostics, "E1301_USE_AFTER_MOVE"),
            0,
            "a rejected capture transfers nothing, so no move follows it"
        );
    }

    #[test]
    fn a_task_may_capture_a_copy_value() {
        let (_, diagnostics) = check(
            "pub fn main(count: i32) -> i32 { \
             let worker: Task<i32> = spawn parallel { return count; }; return count; }",
        );
        assert_eq!(codes(&diagnostics, "E1304_INVALID_TASK_CAPTURE"), 0);
        assert_eq!(codes(&diagnostics, "E1301_USE_AFTER_MOVE"), 0);
    }

    #[test]
    fn a_task_capture_of_an_affine_value_moves_it() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(message: Message) -> unit {{ \
             let worker: Task<Message> = spawn parallel {{ return message; }}; \
             take(message); }}"
        ));
        assert_eq!(
            moves(&diagnostics),
            1,
            "the task took sole ownership across the boundary"
        );
    }

    #[test]
    fn a_closure_may_not_capture_a_borrow() {
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] \
             record Counter [value: i32] \
             pub fn main(borrow counter: Counter) -> unit { \
             let read_it: fn () -> i32 = fn () { return counter.value; }; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        let invalid = diagnostics
            .iter()
            .find(|d| d.code() == "E1305_INVALID_CLOSURE_CAPTURE")
            .expect("a borrow does not reach into a closure");
        assert_eq!(invalid.field("capture"), Some("counter"));
        assert_eq!(invalid.field("reason"), Some("borrow"));
    }

    #[test]
    fn a_closure_captures_copy_by_copy_and_affine_by_move() {
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] \
             record Message [payload: bytes] fn take(message: Message) -> unit { } \
             pub fn main(count: i32, message: Message) -> i32 { \
             let by_copy: fn () -> i32 = fn () { return count; }; \
             let by_move: fn () -> Message = fn () { return message; }; \
             take(message); return count; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert_eq!(codes(&diagnostics, "E1305_INVALID_CLOSURE_CAPTURE"), 0);
        assert_eq!(
            codes(&diagnostics, "E1301_USE_AFTER_MOVE"),
            1,
            "the Copy capture leaves count usable; the affine one moved message"
        );
    }

    #[test]
    fn a_closure_parameter_is_not_a_capture() {
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] \
             pub fn main() -> unit { \
             let step: fn (i32) -> i32 = fn (value: i32) { return value; }; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert_eq!(codes(&diagnostics, "E1305_INVALID_CLOSURE_CAPTURE"), 0);
    }

    const PAIR: &str = "pub record Pair [left: bytes, right: bytes] \
         fn take_bytes(value: bytes) -> unit { } fn take_pair(pair: Pair) -> unit { } ";

    #[test]
    fn a_field_may_be_partially_moved() {
        let (_, diagnostics) = check(&alloc::format!(
            "{PAIR} pub fn main(pair: Pair) -> unit {{ take_bytes(pair.left); \
             take_bytes(pair.right); }}"
        ));
        assert_eq!(
            moves(&diagnostics),
            0,
            "moving one field leaves the untouched one movable"
        );
    }

    #[test]
    fn a_moved_field_may_not_be_moved_twice() {
        let (_, diagnostics) = check(&alloc::format!(
            "{PAIR} pub fn main(pair: Pair) -> unit {{ take_bytes(pair.left); \
             take_bytes(pair.left); }}"
        ));
        assert_eq!(moves(&diagnostics), 1);
    }

    #[test]
    fn a_partially_moved_aggregate_may_not_be_used_whole() {
        // docs/40 section 5 allows the remainder to be used only to move or
        // drop its untouched fields.
        let (_, diagnostics) = check(&alloc::format!(
            "{PAIR} pub fn main(pair: Pair) -> unit {{ take_bytes(pair.left); \
             take_pair(pair); }}"
        ));
        assert_eq!(moves(&diagnostics), 1);
        let reported = diagnostics
            .iter()
            .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
            .unwrap();
        assert_eq!(reported.field("place"), Some("pair"));
        assert_eq!(reported.field("moved"), Some("pair.left"));
    }

    #[test]
    fn a_whole_move_blocks_a_later_field_use() {
        let (_, diagnostics) = check(&alloc::format!(
            "{PAIR} pub fn main(pair: Pair) -> unit {{ take_pair(pair); \
             take_bytes(pair.left); }}"
        ));
        assert_eq!(moves(&diagnostics), 1);
    }

    #[test]
    fn writing_a_place_restores_it() {
        let (_, diagnostics) = check(&alloc::format!(
            "{PAIR} pub fn main() -> unit {{ let mut pair = Pair(left: b\"a\", right: b\"b\"); \
             take_bytes(pair.left); pair.left = b\"c\"; take_bytes(pair.left); }}"
        ));
        assert_eq!(
            moves(&diagnostics),
            0,
            "an assignment gives the place a value again"
        );
    }

    #[test]
    fn a_break_carries_its_state_to_the_loop_exit() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(ready: bool, message: Message) -> unit {{ \
             loop {{ take(message); break; }} take(message); }}"
        ));
        assert_eq!(moves(&diagnostics), 1, "the break carried the move out");
    }

    #[test]
    fn a_continue_feeds_the_back_edge() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(ready: bool, message: Message) -> unit {{ \
             while (ready) {{ take(message); continue; }} }}"
        ));
        assert_eq!(
            moves(&diagnostics),
            1,
            "the next iteration sees the move the continue carried"
        );
    }

    #[test]
    fn a_conditional_break_carries_maybe_moved_state() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(ready: bool, message: Message) -> unit {{ \
             while (ready) {{ if (ready) {{ take(message); break; }} }} take(message); }}"
        ));
        assert_eq!(moves(&diagnostics), 1);
        assert_eq!(
            diagnostics
                .iter()
                .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
                .unwrap()
                .field("certainty"),
            Some("on some paths")
        );
    }

    #[test]
    fn a_bare_loop_has_no_zero_iteration_exit() {
        // Only a `break` leaves a bare loop, so code after an unbroken one is
        // unreachable and cannot see the entry state.
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(message: Message) -> unit {{ \
             loop {{ take(message); }} }}"
        ));
        assert_eq!(moves(&diagnostics), 1, "the back edge reuses the value");

        let (_, unreachable) = check(&alloc::format!(
            "{AFFINE} pub fn main(message: Message) -> unit {{ \
             loop {{ }} take(message); }}"
        ));
        assert_eq!(moves(&unreachable), 0);
    }

    #[test]
    fn a_while_head_may_fail_so_entry_reaches_the_exit() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(ready: bool, message: Message) -> unit {{ \
             while (ready) {{ return; }} take(message); }}"
        ));
        assert_eq!(moves(&diagnostics), 0, "zero iterations is a real path");
    }

    #[test]
    fn break_and_continue_bind_to_their_own_loop() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} pub fn main(ready: bool, message: Message) -> unit {{ \
             while (ready) {{ loop {{ break; }} }} take(message); }}"
        ));
        assert_eq!(
            moves(&diagnostics),
            0,
            "the inner break leaves the inner loop only"
        );
    }

    #[test]
    fn an_assignment_evaluates_its_index_before_the_right_side() {
        // docs/40 section 4 fixes the order, so the index consumes first and
        // the right side is the use that fails.
        let (source, diagnostics) = check(&alloc::format!(
            "{AFFINE} fn position(message: Message) -> size {{ return 0B; }} \
             fn width(message: Message) -> i32 {{ return 1i32; }} \
             pub fn main(message: Message) -> unit {{ \
             let mut values: array<i32, 2> = [0, 0]; \
             values[position(message)] = width(message); }}"
        ));
        assert_eq!(moves(&diagnostics), 1);
        let reported = diagnostics
            .iter()
            .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
            .unwrap();
        assert_eq!(
            reported.span().text(&source),
            "message",
            "the right side is what uses the moved value"
        );
        assert!(reported.span().start() > source.bytes().len() - 40);
    }

    #[test]
    fn a_short_circuit_right_side_is_a_conditional_path() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} fn ready_of(message: Message) -> bool {{ return true; }} \
             pub fn main(flag: bool, message: Message) -> unit {{ \
             let both = flag && ready_of(message); take(message); }}"
        ));
        assert_eq!(moves(&diagnostics), 1);
        assert_eq!(
            diagnostics
                .iter()
                .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
                .unwrap()
                .field("certainty"),
            Some("on some paths"),
            "the right side runs only when the left is true"
        );
    }

    #[test]
    fn closure_capture_analysis_is_lexically_scoped() {
        // The `then` arm shadows `count`; the `else` arm captures the outer one.
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] \
             record Counter [value: i32] \
             pub fn main(ready: bool, borrow counter: Counter) -> unit { \
             let hidden: fn () -> i32 = fn () { \
             if (ready) { let counter = 1i32; return counter; } \
             else { return counter.value; } }; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert_eq!(
            codes(&diagnostics, "E1305_INVALID_CLOSURE_CAPTURE"),
            1,
            "the shadow in one arm must not hide the capture in the other"
        );
    }

    #[test]
    fn a_match_arm_binding_does_not_hide_a_capture_in_another_arm() {
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] \
             record Counter [value: i32] enum Mode [Fast, Slow] \
             pub fn main(mode: Mode, borrow counter: Counter) -> unit { \
             let peek: fn () -> i32 = fn () { \
             match (mode) { Fast => { return 0i32; } Slow => { return counter.value; } } }; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert_eq!(codes(&diagnostics, "E1305_INVALID_CLOSURE_CAPTURE"), 1);
    }

    #[test]
    fn a_sequential_declaration_still_applies_after_its_let() {
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] \
             record Counter [value: i32] \
             pub fn main(borrow counter: Counter) -> unit { \
             let peek: fn () -> i32 = fn () { let counter = 1i32; return counter; }; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert_eq!(
            codes(&diagnostics, "E1305_INVALID_CLOSURE_CAPTURE"),
            0,
            "the local declaration covers the later use"
        );
    }

    #[test]
    fn a_nested_closure_capture_reaches_the_outer_scope() {
        let text = "module system.boot version 1.0 profile full; resource [fuel: 1000] \
             record Counter [value: i32] \
             pub fn main(borrow counter: Counter) -> unit { \
             let outer: fn () -> unit = fn () { \
             let inner: fn () -> i32 = fn () { return counter.value; }; }; }";
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        assert!(
            codes(&diagnostics, "E1305_INVALID_CLOSURE_CAPTURE") >= 1,
            "a borrow reached through a nested closure is still a capture"
        );
    }

    #[test]
    fn a_synchronization_object_is_not_a_lock_guard() {
        // docs/41 separates the object from the guard a lock operation yields;
        // only the guard may not transfer.
        let (_, diagnostics) = check(
            "pub fn main(lock: Mutex<i32>) -> unit { \
             let worker: Task<unit> = spawn parallel { let held = lock; }; }",
        );
        assert_eq!(codes(&diagnostics, "E1304_INVALID_TASK_CAPTURE"), 0);
    }

    // ADR-0035 makes `E1302_CONFLICTING_BORROW` the whole exclusivity
    // violation. The five rows of its matrix are checked one by one.

    #[test]
    fn an_owner_read_under_a_mutable_borrow_is_rejected() {
        let (source, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> i32 {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             let held = borrow mut counter; return counter.value; }}"
        ));
        let conflict = diagnostics
            .iter()
            .find(|d| d.code() == "E1302_CONFLICTING_BORROW")
            .expect("an owner read goes around the exclusive borrow");
        assert_eq!(conflict.stage(), Stage::Ownership);
        assert_eq!(conflict.field("operation"), Some("read"));
        assert_eq!(conflict.field("place"), Some("counter.value"));
        assert_eq!(conflict.field("conflicts_with"), Some("borrow mut"));
        assert_eq!(conflict.span().text(&source), "counter.value");
    }

    #[test]
    fn an_owner_write_under_a_mutable_borrow_is_rejected() {
        let (_, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> unit {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             let held = borrow mut counter; counter.value = 1i32; }}"
        ));
        let conflict = diagnostics
            .iter()
            .find(|d| d.code() == "E1302_CONFLICTING_BORROW")
            .expect("an owner write goes around the exclusive borrow");
        assert_eq!(conflict.field("operation"), Some("write"));
        assert_eq!(
            codes(&diagnostics, "E1303_MUTATE_WHILE_BORROWED"),
            0,
            "E1303 stays the shared-borrow case"
        );
    }

    #[test]
    fn a_move_under_a_live_borrow_is_rejected() {
        let (_, diagnostics) = check(&alloc::format!(
            "{AFFINE} fn peek(borrow message: Message) -> unit {{ }} \
             pub fn main(message: Message) -> unit {{ \
             let view = borrow message; take(message); }}"
        ));
        let conflict = diagnostics
            .iter()
            .find(|d| d.code() == "E1302_CONFLICTING_BORROW")
            .expect("a move invalidates what a live borrow named");
        assert_eq!(conflict.field("operation"), Some("move"));
        assert_eq!(conflict.field("conflicts_with"), Some("borrow"));
        assert_eq!(
            moves(&diagnostics),
            0,
            "the rejected move leaves the value in place"
        );
    }

    #[test]
    fn using_the_borrow_binding_itself_stays_legal() {
        // ADR-0035: an operation through the correct borrow is not an owner
        // alias. Reading through a shared borrow and writing through a mutable
        // one are exactly as legal as before.
        let (_, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> i32 {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             let held = borrow mut counter; write(borrow mut held); return read(borrow held); }}"
        ));
        assert_eq!(codes(&diagnostics, "E1302_CONFLICTING_BORROW"), 0);
        assert_eq!(codes(&diagnostics, "E1303_MUTATE_WHILE_BORROWED"), 0);
    }

    #[test]
    fn an_owner_read_under_a_shared_borrow_is_allowed() {
        let (_, diagnostics) = check(&alloc::format!(
            "{COUNTER} pub fn main() -> i32 {{ let mut counter = Counter(value: 0i32, other: 0i32); \
             let view = borrow counter; return counter.value; }}"
        ));
        assert_eq!(
            codes(&diagnostics, "E1302_CONFLICTING_BORROW"),
            0,
            "any number of immutable borrows may coexist with a read"
        );
    }

    // ADR-0035 defer ownership semantics. `defer` needs the Full profile.

    const CLEANUP: &str = "module system.boot version 1.0 profile full; \
         resource [fuel: 1000] record Message [payload: bytes] \
         fn take(message: Message) -> unit { } \
         fn peek(borrow message: Message) -> unit { } ";

    fn check_cleanup(body: &str) -> (SourceUnit, Vec<Diagnostic>) {
        let text = alloc::format!("{CLEANUP}{body}");
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        (source, diagnostics)
    }

    /// Where the nth `defer` keyword starts, for asserting which body reported.
    fn defer_at(source: &SourceUnit, nth: usize) -> usize {
        let text = core::str::from_utf8(source.bytes()).expect("source is UTF-8");
        text.match_indices("defer").nth(nth).expect("that defer").0
    }

    #[test]
    fn registering_a_cleanup_leaves_the_resource_usable() {
        let (_, diagnostics) = check_cleanup(
            "pub fn main(message: Message) -> unit { \
             defer { take(message); } peek(borrow message); peek(borrow message); }",
        );
        let ownership: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.stage() == Stage::Ownership)
            .map(|d| d.code())
            .collect();
        assert!(
            ownership.is_empty(),
            "registration reads, borrows and moves nothing: {ownership:?}"
        );
    }

    #[test]
    fn a_move_before_a_deferred_consuming_use_is_reported_in_the_cleanup() {
        let (source, diagnostics) = check_cleanup(
            "pub fn main(message: Message) -> unit { take(message); defer { take(message); } }",
        );
        assert_eq!(moves(&diagnostics), 1);
        let reported = diagnostics
            .iter()
            .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
            .unwrap();
        assert!(
            reported.span().start() > defer_at(&source, 0),
            "the cleanup is what uses the moved value"
        );
    }

    #[test]
    fn a_return_path_runs_the_registered_cleanup() {
        // Only the returning path has already moved the value, so the cleanup
        // is rejected there and nowhere else.
        let (source, diagnostics) = check_cleanup(
            "pub fn main(ready: bool, message: Message) -> unit { \
             defer { take(message); } if (ready) { take(message); return; } }",
        );
        assert_eq!(moves(&diagnostics), 1);
        assert!(
            diagnostics
                .iter()
                .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
                .unwrap()
                .span()
                .start()
                > defer_at(&source, 0)
        );
    }

    #[test]
    fn a_break_runs_the_cleanups_of_the_block_it_leaves() {
        let (source, diagnostics) = check_cleanup(
            "pub fn main(message: Message) -> unit { \
             loop { defer { take(message); } break; } take(message); }",
        );
        assert_eq!(moves(&diagnostics), 1);
        assert!(
            diagnostics
                .iter()
                .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
                .unwrap()
                .span()
                .start()
                > defer_at(&source, 0),
            "the break ran the loop body's cleanup, so the later use fails"
        );
    }

    #[test]
    fn a_break_does_not_run_the_cleanups_of_blocks_it_stays_inside() {
        let (source, diagnostics) = check_cleanup(
            "pub fn main(ready: bool, message: Message) -> unit { \
             defer { take(message); } while (ready) { break; } take(message); }",
        );
        assert_eq!(
            moves(&diagnostics),
            1,
            "only the cleanup itself may fail, not the use before it"
        );
        assert!(
            diagnostics
                .iter()
                .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
                .unwrap()
                .span()
                .start()
                > defer_at(&source, 0),
            "the break left no enclosing block, so the outer cleanup did not run"
        );
    }

    #[test]
    fn nested_cleanups_run_in_reverse_registration_order() {
        let (source, diagnostics) = check_cleanup(
            "pub fn main(message: Message) -> unit { \
             defer { take(message); } defer { take(message); } }",
        );
        assert_eq!(moves(&diagnostics), 1);
        let reported = diagnostics
            .iter()
            .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
            .unwrap();
        assert!(
            reported.span().start() < defer_at(&source, 1),
            "the later registration ran first, so the earlier one found nothing"
        );
    }

    #[test]
    fn one_cleanup_effect_is_visible_to_the_next() {
        let (source, diagnostics) = check_cleanup(
            "pub fn main(message: Message) -> unit { \
             defer { peek(borrow message); } defer { take(message); } }",
        );
        assert_eq!(moves(&diagnostics), 1);
        assert!(
            diagnostics
                .iter()
                .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
                .unwrap()
                .span()
                .start()
                < defer_at(&source, 1),
            "the consuming cleanup ran first and the borrowing one saw it"
        );
    }

    #[test]
    fn shadowing_after_registration_does_not_rebind_the_cleanup() {
        // Binding identity is fixed lexically where the cleanup registers, so
        // the body still names the moved parameter, not the later local.
        let (_, diagnostics) = check_cleanup(
            "pub fn main(message: Message) -> unit { take(message); \
             defer { take(message); } \
             let message = Message(payload: b\"other\"); peek(borrow message); }",
        );
        assert_eq!(
            moves(&diagnostics),
            1,
            "a late-bound cleanup would have found the fresh local instead"
        );
    }

    #[test]
    fn a_cleanup_registered_on_one_path_runs_on_that_path_only() {
        let (_, diagnostics) = check_cleanup(
            "pub fn main(ready: bool, message: Message) -> unit { \
             if (ready) { defer { take(message); } } take(message); }",
        );
        assert_eq!(moves(&diagnostics), 1);
        assert_eq!(
            diagnostics
                .iter()
                .find(|d| d.code() == "E1301_USE_AFTER_MOVE")
                .unwrap()
                .field("certainty"),
            Some("on some paths"),
            "the other path never registered the cleanup"
        );
    }

    #[test]
    fn a_cleanup_after_an_exit_never_registers() {
        let (_, diagnostics) = check_cleanup(
            "pub fn main(message: Message) -> unit { \
             take(message); return; defer { take(message); } }",
        );
        assert_eq!(
            moves(&diagnostics),
            0,
            "the return left before the registration was reached"
        );
    }

    // docs/41 sections 2 and 5: task scopes and atomic order legality.

    #[test]
    fn a_task_scope_may_not_be_left_with_an_unconsumed_child() {
        let (_, diagnostics) = check(
            "pub fn main() -> unit { parallel { let child = spawn parallel { return 1i32; }; } }",
        );
        let unjoined = diagnostics
            .iter()
            .find(|d| d.code() == "E1401_UNJOINED_TASK")
            .expect("every child is consumed before scope exit");
        assert_eq!(unjoined.field("task"), Some("child"));
    }

    #[test]
    fn a_joined_child_discharges_the_obligation() {
        let (_, diagnostics) = check(
            "pub fn main() -> unit { parallel { let child = spawn parallel { return 1i32; }; \
             let outcome = join child; } }",
        );
        assert_eq!(codes(&diagnostics, "E1401_UNJOINED_TASK"), 0);
    }

    #[test]
    fn cancel_alone_does_not_discharge_the_obligation() {
        // docs/41 section 2 is explicit: cancel consumes no ownership.
        let (_, diagnostics) = check(
            "pub fn main() -> unit { parallel { let child = spawn parallel { return 1i32; }; \
             cancel child; } }",
        );
        assert_eq!(codes(&diagnostics, "E1401_UNJOINED_TASK"), 1);
    }

    #[test]
    fn an_unbound_child_handle_can_never_be_consumed() {
        let (_, diagnostics) =
            check("pub fn main() -> unit { parallel { spawn parallel { return 1i32; }; } }");
        let unjoined = diagnostics
            .iter()
            .find(|d| d.code() == "E1401_UNJOINED_TASK")
            .expect("a discarded handle is unconsumable");
        assert_eq!(
            unjoined.field("reason"),
            Some("the child handle is never bound")
        );
    }

    #[test]
    fn each_task_scope_owns_its_own_children() {
        // The inner scope joins; the outer one does not, and only the outer
        // child is reported.
        let (_, diagnostics) = check(
            "pub fn main() -> unit { let outer = spawn parallel { return 1i32; }; \
             parallel { let inner = spawn parallel { return 2i32; }; let done = join inner; } }",
        );
        assert_eq!(codes(&diagnostics, "E1401_UNJOINED_TASK"), 1);
        assert_eq!(
            diagnostics
                .iter()
                .find(|d| d.code() == "E1401_UNJOINED_TASK")
                .unwrap()
                .field("task"),
            Some("outer")
        );
    }

    #[test]
    fn a_load_rejects_a_release_order() {
        let (_, diagnostics) =
            check("pub fn read_it(borrow state: AtomicU32) -> u32 { return state.load(Release); }");
        let invalid = diagnostics
            .iter()
            .find(|d| d.code() == "E1410_INVALID_ATOMIC_ORDER")
            .expect("a load never releases");
        assert_eq!(invalid.field("operation"), Some("load"));
        assert_eq!(invalid.field("order"), Some("Release"));
    }

    #[test]
    fn a_store_rejects_an_acquire_order() {
        let (_, diagnostics) = check(
            "pub fn write_it(borrow state: AtomicU32) -> unit { state.store(1u32, Acquire); }",
        );
        assert_eq!(codes(&diagnostics, "E1410_INVALID_ATOMIC_ORDER"), 1);
    }

    #[test]
    fn every_order_is_legal_for_a_read_modify_write() {
        let (_, diagnostics) = check(
            "pub fn bump(borrow state: AtomicU32) -> u32 { return state.fetch_add(1u32, AcqRel); }",
        );
        assert_eq!(codes(&diagnostics, "E1410_INVALID_ATOMIC_ORDER"), 0);
    }

    #[test]
    fn a_compare_exchange_failure_order_may_not_be_stronger_than_success() {
        let (_, diagnostics) = check(
            "pub fn swap_it(borrow state: AtomicU32) -> Result<u32, u32> { \
             return state.compare_exchange(0u32, 1u32, Acquire, SeqCst); }",
        );
        let invalid = diagnostics
            .iter()
            .find(|d| d.code() == "E1410_INVALID_ATOMIC_ORDER")
            .expect("a failure order may not exceed the success order");
        assert_eq!(invalid.field("position"), Some("failure"));
        assert_eq!(invalid.field("success_order"), Some("Acquire"));
    }

    #[test]
    fn a_compare_exchange_failure_order_may_not_release() {
        let (_, diagnostics) = check(
            "pub fn swap_it(borrow state: AtomicU32) -> Result<u32, u32> { \
             return state.compare_exchange(0u32, 1u32, SeqCst, Release); }",
        );
        assert_eq!(codes(&diagnostics, "E1410_INVALID_ATOMIC_ORDER"), 1);
    }

    #[test]
    fn a_legal_compare_exchange_pair_is_accepted() {
        let (_, diagnostics) = check(
            "pub fn swap_it(borrow state: AtomicU32) -> Result<u32, u32> { \
             return state.compare_exchange(0u32, 1u32, AcqRel, Acquire); }",
        );
        assert_eq!(codes(&diagnostics, "E1410_INVALID_ATOMIC_ORDER"), 0);
    }

    // docs/40 section 3 and docs/42 section 4: declared authority.

    const AUTHORITY: &str = "module system.boot version 1.0 profile bootstrap; \
         import capability system.time.Clock as clock; resource [fuel: 1000] ";

    fn check_authority(body: &str) -> (SourceUnit, Vec<Diagnostic>) {
        let text = alloc::format!("{AUTHORITY}{body}");
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("checker input must parse");
        let diagnostics = Checker::check(&source, &schema);
        (source, diagnostics)
    }

    #[test]
    fn a_capability_operation_needs_the_declared_effect() {
        let (_, diagnostics) =
            check_authority("pub fn sample() -> duration { return clock.now(); }");
        let missing = diagnostics
            .iter()
            .find(|d| d.code() == "E1501_UNDECLARED_CAPABILITY_EFFECT")
            .expect("authority is never ambient");
        assert_eq!(missing.stage(), Stage::Effect);
        assert_eq!(missing.field("capability"), Some("clock"));
        assert_eq!(missing.field("interface"), Some("system.time.Clock"));
    }

    #[test]
    fn a_declared_effect_admits_the_operation() {
        let (_, diagnostics) =
            check_authority("pub fn sample() -> duration uses [clock] { return clock.now(); }");
        assert_eq!(codes(&diagnostics, "E1501_UNDECLARED_CAPABILITY_EFFECT"), 0);
    }

    #[test]
    fn a_caller_must_declare_every_effect_its_callee_requires() {
        let (_, diagnostics) = check_authority(
            "fn sample() -> duration uses [clock] { return clock.now(); } \
             pub fn main() -> duration { return sample(); }",
        );
        let missing = diagnostics
            .iter()
            .find(|d| d.code() == "E1501_UNDECLARED_CAPABILITY_EFFECT")
            .expect("a call cannot launder authority");
        assert_eq!(missing.field("required_by"), Some("sample"));
        assert_eq!(missing.field("capability"), Some("clock"));
    }

    #[test]
    fn a_declared_caller_effect_covers_its_callee() {
        let (_, diagnostics) = check_authority(
            "fn sample() -> duration uses [clock] { return clock.now(); } \
             pub fn main() -> duration uses [clock] { return sample(); }",
        );
        assert_eq!(codes(&diagnostics, "E1501_UNDECLARED_CAPABILITY_EFFECT"), 0);
    }

    #[test]
    fn a_capability_cannot_be_cast_into_existence() {
        let (_, diagnostics) = check_authority(
            "pub fn main() -> unit { let forged: system.time.Clock = 1u64 as system.time.Clock; }",
        );
        let forged = diagnostics
            .iter()
            .find(|d| d.code() == "E1502_FORGED_CAPABILITY")
            .expect("a scalar cannot become authority");
        assert_eq!(forged.stage(), Stage::Effect);
        assert_eq!(forged.field("interface"), Some("system.time.Clock"));
        assert_eq!(forged.field("operation"), Some("cast"));
        assert_eq!(
            codes(&diagnostics, "E1212_INVALID_AS_CONVERSION"),
            0,
            "docs/40 section 3 routes this away from the generic conversion error"
        );
    }

    #[test]
    fn a_capability_cannot_be_constructed() {
        let (_, diagnostics) =
            check_authority("pub fn main() -> unit { let forged = system.time.Clock(); }");
        let forged = diagnostics
            .iter()
            .find(|d| d.code() == "E1502_FORGED_CAPABILITY")
            .expect("a capability is nonconstructible");
        assert_eq!(forged.field("operation"), Some("construct"));
    }

    #[test]
    fn an_unimported_path_is_not_a_capability_forgery() {
        let (_, diagnostics) =
            check_authority("pub fn main() -> unit { let value = 1u64 as u64; }");
        assert_eq!(codes(&diagnostics, "E1502_FORGED_CAPABILITY"), 0);
    }

    #[test]
    fn a_type_argument_list_nested_directly_inside_another_parses() {
        // The lexer emits `>>` as one shift operator, so both argument lists
        // close at a single token.
        let (source, outcome) =
            parse("fn main() -> Result<Option<i32>, Option<i32>> { return 1i32; }");
        assert!(
            !outcome.has_errors(),
            "unexpected diagnostics: {:?}",
            outcome
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code())
                .collect::<Vec<_>>()
        );
        let schema = outcome
            .into_accepted()
            .expect("nested type arguments parse");
        let result = schema.functions()[0].signature().result();
        assert_eq!(result.form(), TypeSyntaxForm::Constructed);
        assert_eq!(result.text(&source), "Result<Option<i32>, Option<i32>>");
    }

    #[test]
    fn parser_recovers_at_the_next_list_separator() {
        // The malformed middle field is dropped; the elements on both sides of
        // it survive because synchronization stops at the next comma.
        let (source, outcome) = parse("record Point [x: i32, 7: i32, z: i32]");
        assert!(outcome.has_errors());
        assert_eq!(outcome.diagnostics().len(), 1);
        let diagnostic = &outcome.diagnostics()[0];
        assert_eq!(diagnostic.code(), "E1101_EXPECTED_IDENTIFIER");
        assert_eq!(diagnostic.field("region"), Some("list"));
        assert_eq!(diagnostic.span().text(&source), "7");

        let schema = outcome.value().expect("recovery keeps the partial schema");
        let fields = schema.records()[0].fields();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name().text(&source), "x");
        assert_eq!(fields[1].name().text(&source), "z");
    }

    #[test]
    fn a_lexical_failure_is_reported_alone() {
        // docs/39 section 4 orders the lowest applicable lexical error first;
        // the source below is also syntactically broken, and none of that is
        // reported while the bytes cannot be tokenized.
        let source = SourceReader::read(
            b"module system.boot version 1.0 profile bootstrap; resource [fuel: 1] record @ [",
        )
        .expect("transport-valid source");
        let outcome = Parser::parse_schema(&source);
        assert_eq!(outcome.diagnostics().len(), 1);
        let diagnostic = &outcome.diagnostics()[0];
        assert_eq!(diagnostic.code(), "E1013_UNEXPECTED_CHARACTER");
        assert_eq!(diagnostic.stage(), Stage::Lex);
        assert_eq!(diagnostic.field("byte_offset"), Some("76"));
        assert!(outcome.into_accepted().is_none());
    }

    #[test]
    fn a_character_that_begins_no_lexical_form_is_distinguished_from_an_identifier_violation() {
        // docs/44 section 7 fixes this precedence: non-ASCII takes the
        // identifier code, everything else takes the unexpected-character code.
        let source = SourceReader::read("fn main() { let ключ: i32 = 1i32; }".as_bytes())
            .expect("transport-valid source");
        let error = Lexer::lex(&source).expect_err("a non-ASCII identifier is invalid");
        assert_eq!(error.code().symbol(), "E1012_INVALID_IDENTIFIER");
        assert_eq!(error.byte_offset(), 16);

        for (text, offset) in [("let @ = 1i32;", 4), ("let $x = 1i32;", 4)] {
            let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
            let error = Lexer::lex(&source).expect_err("the character begins no lexical form");
            assert_eq!(error.code().symbol(), "E1013_UNEXPECTED_CHARACTER");
            assert_eq!(error.byte_offset(), offset);
        }
    }

    #[test]
    fn unexpected_character_conformance_vectors_match_their_recorded_spans() {
        // Conformance cases R029 and R030, including the recorded byte offset
        // and derived line/column.
        let vectors: [(&str, &[u8], usize, usize, usize); 2] = [
            (
                "unexpected-character-at",
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../docs/language/conformance/v1/reject/unexpected-character-at.tos"
                )),
                287,
                7,
                12,
            ),
            (
                "unexpected-character-dollar",
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../docs/language/conformance/v1/reject/unexpected-character-dollar.tos"
                )),
                288,
                7,
                9,
            ),
        ];
        for (name, bytes, offset, line, column) in vectors {
            let source = SourceReader::read(bytes).expect("vector is transport-valid");
            let outcome = Parser::parse_schema(&source);
            assert_eq!(outcome.diagnostics().len(), 1, "{name}");
            let diagnostic = &outcome.diagnostics()[0];
            assert_eq!(diagnostic.code(), "E1013_UNEXPECTED_CHARACTER", "{name}");
            assert_eq!(diagnostic.stage(), Stage::Lex, "{name}");
            assert_eq!(diagnostic.span().start(), offset, "{name}");
            assert_eq!(diagnostic.start().line(), line, "{name}");
            assert_eq!(diagnostic.start().column(), column, "{name}");
            assert!(outcome.into_accepted().is_none(), "{name}");
        }
    }

    #[test]
    fn a_declaration_after_a_damaged_function_is_still_parsed() {
        // ADR-0032 section 3: a `fn` declaration ends with a block, so without
        // the closing-brace boundary this source would report one diagnostic
        // and lose every later declaration.
        let (source, outcome) = parse(
            "fn () -> i32 { return 1i32; } record Kept [value: i32] \
             fn healthy() -> i32 { return 2i32; }",
        );
        assert!(outcome.has_errors());
        assert_eq!(outcome.diagnostics().len(), 1);
        assert_eq!(
            outcome.diagnostics()[0].field("region"),
            Some("declaration")
        );

        let schema = outcome.value().expect("recovery keeps the partial schema");
        assert_eq!(schema.records().len(), 1);
        assert_eq!(schema.records()[0].name().text(&source), "Kept");
        assert_eq!(schema.functions().len(), 1);
        assert_eq!(
            schema.functions()[0].signature().name().text(&source),
            "healthy"
        );
    }

    #[test]
    fn diagnostic_positions_use_lines_and_utf8_columns() {
        // The comment holds two-byte scalar values, so a byte-counted column
        // would drift on the following line.
        let source = SourceReader::read(
            "module system.boot version 1.0 profile bootstrap;\n// ключ\nresource [\n".as_bytes(),
        )
        .expect("transport-valid source");
        let outcome = Parser::parse_outline(&source);
        assert!(outcome.has_errors());
        let diagnostic = &outcome.diagnostics()[0];
        assert_eq!(diagnostic.start().line(), 4);
        assert_eq!(diagnostic.start().column(), 1);

        // Byte 54 opens the second Cyrillic scalar value of the comment. A
        // byte-counted column would report 6 for it.
        let mid_line = Position::at(&source, 54);
        assert_eq!(mid_line.line(), 2);
        assert_eq!(mid_line.column(), 5);
    }

    #[test]
    fn accepted_output_is_withheld_whenever_a_diagnostic_is_an_error() {
        let (_, outcome) = parse("record Point [x: i32, 7: i32]");
        assert!(outcome.value().is_some());
        assert!(outcome.into_accepted().is_none());
    }

    #[test]
    fn superseded_brace_syntax_is_rejected_by_the_conformance_vectors() {
        // Conformance cases R020, R021 and R022: `[]` introduces resource,
        // enum and record declaration lists, and the pre-acceptance brace forms
        // must not parse.
        let vectors: [(&str, &[u8]); 3] = [
            (
                "old-resource-braces",
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../docs/language/conformance/v1/reject/old-resource-braces.tos"
                )),
            ),
            (
                "old-enum-braces",
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../docs/language/conformance/v1/reject/old-enum-braces.tos"
                )),
            ),
            (
                "old-record-braces",
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../docs/language/conformance/v1/reject/old-record-braces.tos"
                )),
            ),
        ];
        for (name, bytes) in vectors {
            let source = SourceReader::read(bytes).expect("vector is transport-valid");
            let outcome = Parser::parse_schema(&source);
            assert!(outcome.has_errors(), "{name} must not parse");
            assert!(outcome.into_accepted().is_none(), "{name} must not parse");
        }
    }

    #[test]
    fn source_reader_accepts_ascii_without_changing_bytes() {
        let b = b"module system.test version 1.0 profile bootstrap;\n";
        assert_eq!(SourceReader::read(b).unwrap().bytes(), b);
    }
    #[test]
    fn source_reader_rejects_utf8_before_normalization() {
        let e = SourceReader::read(b"\xff").unwrap_err();
        assert_eq!(
            (e.code(), e.byte_offset()),
            (SourceErrorCode::InvalidUtf8, 0)
        );
    }
    #[test]
    fn source_reader_normalizes_crlf_transport_before_identity_bytes() {
        assert_eq!(
            SourceReader::read(b"a\r\nb\r\n").unwrap().bytes(),
            b"a\nb\n"
        );
    }

    #[test]
    fn source_reader_enforces_the_documented_byte_bound_before_normalization() {
        let input = vec![b'a'; MAX_SOURCE_BYTES + 1];
        let error = SourceReader::read(&input).expect_err("source over the bound must fail");
        assert_eq!(error.code(), SourceErrorCode::SourceTooLarge);
        assert_eq!(error.byte_offset(), MAX_SOURCE_BYTES);
    }
    #[test]
    fn source_reader_reports_documented_source_errors() {
        for (i, c, o) in [
            (
                b"\xef\xbb\xbfmodule".as_slice(),
                SourceErrorCode::BomForbidden,
                0,
            ),
            (b"a\rb".as_slice(), SourceErrorCode::BareCr, 1),
            ("// e\u{301}\n".as_bytes(), SourceErrorCode::NotNfc, 3),
            (b"a\0b".as_slice(), SourceErrorCode::NulForbidden, 1),
        ] {
            let e = SourceReader::read(i).unwrap_err();
            assert_eq!((e.code(), e.byte_offset()), (c, o));
        }
    }
    #[test]
    fn unicode_nfc_is_checked_in_comments_and_strings() {
        assert!(SourceReader::read("// é\nlet x = \"Ḋ\";\n".as_bytes()).is_ok());
        assert_eq!(
            SourceReader::read("// D\u{307}\u{323}\n".as_bytes())
                .unwrap_err()
                .code(),
            SourceErrorCode::NotNfc
        );
    }
    #[test]
    fn ucd_normalization_cases_accept_nfc_and_reject_non_nfc() {
        for line in include_str!("../unicode/ucd-17.0.0/NormalizationTest.txt")
            .lines()
            .filter(|line| !line.starts_with('#') && line.contains(';'))
        {
            let fields: Vec<_> = line
                .split('#')
                .next()
                .unwrap()
                .split(';')
                .map(str::trim)
                .collect();
            if fields.len() < 3 {
                continue;
            }
            let decode = |field: &str| -> String {
                field
                    .split_whitespace()
                    .filter_map(|cp| u32::from_str_radix(cp, 16).ok().and_then(char::from_u32))
                    .collect()
            };
            let (c1, c2, c3) = (decode(fields[0]), decode(fields[1]), decode(fields[2]));
            if [c1.as_str(), c2.as_str(), c3.as_str()]
                .iter()
                .any(|s| s.contains(['\0', '\r', '\n']))
            {
                continue;
            }
            assert!(
                SourceReader::read(format!("// {c2}\n").as_bytes()).is_ok(),
                "NFC {fields:?}"
            );
            for candidate in [c1, c3] {
                if candidate != c2 {
                    assert_eq!(
                        SourceReader::read(format!("// {candidate}\n").as_bytes())
                            .unwrap_err()
                            .code(),
                        SourceErrorCode::NotNfc,
                        "non-NFC {fields:?}"
                    );
                }
            }
        }
    }
}
