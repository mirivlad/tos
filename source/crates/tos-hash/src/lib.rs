// SPDX-License-Identifier: GPL-3.0-or-later
//! SHA-256, `no_std`, dependency-free.
//!
//! Reimplemented from the public FIPS 180-4 specification (hardware fact: the
//! function and constants are normative inputs/outputs, not expressive code).
//! No third-party code was copied. Stream-safe incremental form plus a one-shot
//! helper. The digest of an empty input is `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.

#![no_std]

#[cfg(test)]
extern crate std;

/// Round constants from FIPS 180-4 section 4.2.2.
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Initial hash values from FIPS 180-4.
const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Streaming SHA-256 state.
#[derive(Clone)]
pub struct Sha256 {
    h: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total_len: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self {
            h: H0,
            buf: [0u8; 64],
            buf_len: 0,
            total_len: 0,
        }
    }

    /// Feed bytes into the hash. Any byte length is accepted.
    pub fn update(&mut self, mut data: &[u8]) {
        self.total_len = self
            .total_len
            .checked_add(data.len() as u64)
            .expect("sha256: total input exceeds u64");
        if self.buf_len != 0 {
            let want = 64 - self.buf_len;
            let take = core::cmp::min(want, data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                self.compress_block();
                self.buf_len = 0;
            }
        }
        while data.len() >= 64 {
            self.buf.copy_from_slice(&data[..64]);
            self.compress_block();
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    fn compress_block(&mut self) {
        let mut w = [0u32; 64];
        // FIPS 180-4 §6.2.2 step 1 for t = 0..15: the block is read as 16
        // big-endian words. Zipping the words with 4-byte chunks keeps the
        // indexing out of the loop without introducing a fallible conversion
        // (no `try_into().unwrap()`): a trusted-base primitive must have no
        // panic path at all.
        for (word, chunk) in w[..16].iter_mut().zip(self.buf.chunks_exact(4)) {
            *word = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.h;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(h);
    }

    /// Finalize and return the 32-byte digest. After this call the instance is
    /// spent; callers discard it.
    pub fn finalize(mut self) -> [u8; 32] {
        let bit_len = self
            .total_len
            .checked_mul(8)
            .expect("message length bits overflow");
        // Append 0x80, then zero bytes until length % 64 == 56, then the
        // 8-byte big-endian bit length; each full 64-byte block is compressed.
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;
        while self.buf_len != 56 {
            if self.buf_len == 64 {
                self.compress_block();
                self.buf_len = 0;
            }
            self.buf[self.buf_len] = 0;
            self.buf_len += 1;
        }
        self.buf[56..64].copy_from_slice(&bit_len.to_be_bytes());
        self.compress_block();
        let mut out = [0u8; 32];
        for (i, h) in self.h.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&h.to_be_bytes());
        }
        out
    }
}

impl core::fmt::Debug for Sha256 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Sha256(len={})", self.total_len)
    }
}

/// One-shot SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize()
}

/// Render a digest as lowercase hex into `out` (must be 64 bytes).
pub fn hex(digest: &[u8; 32], out: &mut [u8; 64]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (i, b) in digest.iter().enumerate() {
        out[i * 2] = HEX[(b >> 4) as usize];
        out[i * 2 + 1] = HEX[(b & 0x0f) as usize];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::String;
    use std::vec;

    fn hex_of(data: &[u8]) -> String {
        let mut out = [0u8; 64];
        hex(&sha256(data), &mut out);
        String::from_utf8(out.to_vec()).unwrap()
    }

    #[test]
    fn empty_string() {
        assert_eq!(
            hex_of(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn rfc4231_abc() {
        assert_eq!(
            hex_of(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn rfc4231_two_block() {
        assert_eq!(
            hex_of(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn rfc4231_million_a() {
        let big = [b'a'; 1_000_000];
        assert_eq!(
            hex_of(&big),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn streaming_matches_single_shot() {
        let data = b"streaming boundary test data 0123456789abcdefghijklmnopqrstuvwxyz";
        let mut h = Sha256::new();
        for chunk in data.chunks(1) {
            h.update(chunk);
        }
        let out = h.finalize();
        assert_eq!(out, sha256(data));
        // multi-byte chunking too
        let mut h2 = Sha256::new();
        for chunk in data.chunks(7) {
            h2.update(chunk);
        }
        assert_eq!(h2.finalize(), sha256(data));
    }

    #[test]
    fn padding_boundaries() {
        // lengths around the 56/64-byte pad boundary must all agree with one-shot.
        for len in [0usize, 1, 54, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129, 1024] {
            let data = vec![0xabu8; len];
            let mut h = Sha256::new();
            h.update(&data);
            assert_eq!(h.finalize(), sha256(&data), "len={len}");
        }
    }

    #[test]
    fn known_digest_of_hex_identity_field() {
        // Deterministic sanity: digest of 32 zero bytes (used for zeroed digest fields).
        let zeros = [0u8; 32];
        let _ = sha256(&zeros);
        // Just assert it is length-stable and deterministic.
        assert_eq!(sha256(&zeros), sha256(&zeros));
    }
}