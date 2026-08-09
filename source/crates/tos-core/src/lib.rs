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
