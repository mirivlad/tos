// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded canonical TOS Core V1 source reader (docs/39, ADR-0029).

use std::boxed::Box;
use std::string::String;
use std::vec::Vec;

mod diagnostic;
mod parser;

pub use diagnostic::{Diagnostic, DiagnosticField, Position, Severity, Stage};
pub use parser::{
    Block, BorrowMode, CallArgument, EnumDeclaration, EnumVariant, EnumVariantForm, Expression,
    ExpressionForm, FunctionDeclaration, FunctionParameter, FunctionSignature, Import, ImportKind,
    ModuleHeader, ModuleOutline, ModulePrefix, ParseOutcome, Parser, Profile, RecordDeclaration,
    RecordField, ResourceDeclaration, ResourceLimit, Schema, Span, Statement, StatementForm,
    TypeSyntax, TypeSyntaxForm,
};

mod unicode {
    include!(concat!(env!("OUT_DIR"), "/unicode_tables.rs"));
}

pub const MAX_SOURCE_BYTES: usize = 256 * 1024;

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
        assert_eq!(binding.binding().unwrap().text(&source), "count");
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
        let text = std::format!("{PREFIX}{body}");
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
        assert_eq!(statements[0].binding().unwrap().text(&source), "a");
        assert_eq!(statements[1].binding().unwrap().text(&source), "c");
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
