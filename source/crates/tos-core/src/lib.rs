// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded canonical TOS Core V1 source reader (docs/39, ADR-0029).

use std::boxed::Box;
use std::string::String;
use std::vec::Vec;

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
    InvalidIntegerLiteral,
    InvalidString,
    InvalidBytes,
    UnexpectedByte,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LexError {
    code: LexErrorCode,
    byte_offset: usize,
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
                    _ => {
                        return Err(LexError {
                            code: LexErrorCode::UnexpectedByte,
                            byte_offset: i,
                        })
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
