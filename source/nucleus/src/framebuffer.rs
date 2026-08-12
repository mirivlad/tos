// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded drawing primitives over the loader-declared framebuffer.
//!
//! This is deliberately not a console, a terminal, a GUI or a public ABI. It is
//! the smallest set of operations the human-facing boot presentation needs:
//! fill, glyph text and the `@`-cell grid the canonical mascot artwork is drawn
//! from. Every operation clips to the buffer it was handed, none allocates, and
//! none of them can change a boot decision.
//!
//! What is drawn and when is decided one level up, in [`crate::console`]. The
//! separation is the point: a primitive that knows what a boot step is would
//! have to be taught again by every later screen.

use tos_boot_protocol::{BootInfo, FB_FORMAT_BGRX8, FB_FORMAT_NONE, FB_FORMAT_RGBX8};

pub type Color = (u8, u8, u8);

/// The Stage 1/2 palette. Dark ground, one accent, readable light text; the two
/// status colours exist so a failed step is distinguishable from a finished one
/// at a glance, which a single accent cannot do.
pub const BACKGROUND: Color = (12, 17, 27);
pub const ACCENT: Color = (255, 114, 57);
pub const TEXT: Color = (226, 234, 243);
pub const MUTED: Color = (138, 154, 176);
pub const DONE: Color = (108, 199, 127);
pub const FAILED: Color = (233, 96, 96);

/// Glyph cell geometry, in unscaled pixels.
pub const GLYPH_WIDTH: usize = 5;
pub const GLYPH_HEIGHT: usize = 7;
/// Horizontal advance per character: the glyph plus one column of tracking.
pub const ADVANCE: usize = GLYPH_WIDTH + 1;

/// Width in pixels of `chars` characters drawn at `scale`, tracking included.
pub fn text_width(chars: usize, scale: usize) -> usize {
    chars.saturating_mul(ADVANCE).saturating_mul(scale)
}

// Canonical artwork/data: assets/mascot/tos_ascii-art2.txt is CC-BY-SA-4.0.
// The checked provenance record retains its digest, attribution and licence
// identity. This GPL renderer consumes those exact bytes; it does not relabel
// the artwork or make a blanket licence claim about the nucleus.
const PYRO_ART_WITH_NOTICE: &[u8] = include_bytes!("../../../assets/mascot/tos_ascii-art2.txt");

/// The canonical mascot artwork without its licence-notice first line.
pub fn pyro_art_body() -> &'static [u8] {
    match PYRO_ART_WITH_NOTICE.iter().position(|&byte| byte == b'\n') {
        Some(first_line_end) => &PYRO_ART_WITH_NOTICE[first_line_end + 1..],
        None => &[],
    }
}

/// Columns and rows of an `@`-cell grid, as `draw_ascii_grid` walks it.
pub fn ascii_dimensions(art: &[u8]) -> (usize, usize) {
    let (mut col, mut max_col, mut rows) = (0usize, 0usize, 1usize);
    for &byte in art {
        if byte == b'\n' {
            max_col = max_col.max(col);
            col = 0;
            rows = rows.saturating_add(1);
        } else {
            col = col.saturating_add(1);
        }
    }
    (max_col.max(col), rows)
}

pub struct Framebuffer<'a> {
    bytes: &'a mut [u8],
    width: usize,
    height: usize,
    pitch: usize,
    format: u32,
}

impl<'a> Framebuffer<'a> {
    pub fn new(
        bytes: &'a mut [u8],
        width: u32,
        height: u32,
        pitch: u32,
        format: u32,
    ) -> Option<Self> {
        if width == 0 || height == 0 || !matches!(format, FB_FORMAT_RGBX8 | FB_FORMAT_BGRX8) {
            return None;
        }
        let width = usize::try_from(width).ok()?;
        let height = usize::try_from(height).ok()?;
        let pitch = usize::try_from(pitch).ok()?;
        if pitch < width.checked_mul(4)? || pitch.checked_mul(height)? > bytes.len() {
            return None;
        }
        Some(Self {
            bytes,
            width,
            height,
            pitch,
            format,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    fn put_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let Some(offset) = y
            .checked_mul(self.pitch)
            .and_then(|row| x.checked_mul(4).and_then(|column| row.checked_add(column)))
        else {
            return;
        };
        let Some(pixel) = self.bytes.get_mut(offset..offset + 4) else {
            return;
        };
        let (r, g, b) = color;
        match self.format {
            FB_FORMAT_RGBX8 => pixel.copy_from_slice(&[r, g, b, 0]),
            FB_FORMAT_BGRX8 => pixel.copy_from_slice(&[b, g, r, 0]),
            _ => {}
        }
    }

    pub fn fill_rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: Color) {
        let end_x = x.saturating_add(width).min(self.width);
        let end_y = y.saturating_add(height).min(self.height);
        for py in y.min(self.height)..end_y {
            for px in x.min(self.width)..end_x {
                self.put_pixel(px, py, color);
            }
        }
    }

    /// Paint the whole visible surface. Only the declared geometry is touched:
    /// padding between `width * 4` and `pitch` is left as the firmware left it.
    pub fn clear(&mut self, color: Color) {
        self.fill_rect(0, 0, self.width, self.height, color);
    }

    /// Render `@` cells as foreground rectangles. This generic primitive has no
    /// knowledge of a particular mascot or external artwork.
    pub fn draw_ascii_grid(
        &mut self,
        art: &[u8],
        origin_x: usize,
        origin_y: usize,
        cell: usize,
        foreground: Color,
    ) {
        if cell == 0 {
            return;
        }
        let (mut col, mut row) = (0usize, 0usize);
        for &byte in art {
            match byte {
                b'\n' => {
                    col = 0;
                    row = row.saturating_add(1);
                }
                b'@' => {
                    let x = origin_x.saturating_add(col.saturating_mul(cell));
                    let y = origin_y.saturating_add(row.saturating_mul(cell));
                    self.fill_rect(x, y, cell, cell, foreground);
                    col = col.saturating_add(1);
                }
                _ => col = col.saturating_add(1),
            }
        }
    }

    pub fn draw_text(&mut self, text: &[u8], x: usize, y: usize, scale: usize, color: Color) {
        let mut cursor = x;
        for &byte in text {
            for (row, bits) in glyph(byte).iter().enumerate() {
                for col in 0..GLYPH_WIDTH {
                    if bits & (1 << (GLYPH_WIDTH - 1 - col)) != 0 {
                        self.fill_rect(
                            cursor.saturating_add(col * scale),
                            y.saturating_add(row * scale),
                            scale,
                            scale,
                            color,
                        );
                    }
                }
            }
            cursor = cursor.saturating_add(ADVANCE * scale);
        }
    }
}

/// Borrow the loader-declared framebuffer, if there is one that can be
/// represented safely.
///
/// SAFETY: the caller must already have accepted the boot ABI record and the
/// memory map that describes it — ADR-0022 requires a present framebuffer range
/// to be mapped and reserved, and BOOT_ABI_V1 validation is what establishes
/// that its declared tuple is internally consistent. The returned borrow aliases
/// the framebuffer for its whole lifetime, so the caller must not create a
/// second one. Absent, unsupported or unrepresentable geometry yields `None`
/// and nothing is written.
pub unsafe fn map(bi: &BootInfo) -> Option<Framebuffer<'static>> {
    if bi.framebuffer_format == FB_FORMAT_NONE || bi.framebuffer_phys == 0 {
        return None;
    }
    let required = u64::from(bi.framebuffer_pitch).checked_mul(u64::from(bi.framebuffer_height))?;
    let length = usize::try_from(required).ok()?;
    // SAFETY: the checked pitch x height size above bounds the borrow to the
    // range the loader declared and reserved; no allocation is performed.
    let bytes = core::slice::from_raw_parts_mut(bi.framebuffer_phys as *mut u8, length);
    Framebuffer::new(
        bytes,
        bi.framebuffer_width,
        bi.framebuffer_height,
        bi.framebuffer_pitch,
        bi.framebuffer_format,
    )
}

/// A 5x7 bitmap cell for one byte of printable ASCII.
///
/// Unmapped bytes draw a hollow box rather than nothing: text that silently
/// loses characters would misreport what the system is doing, which is the one
/// thing this screen exists to avoid.
fn glyph(byte: u8) -> [u8; GLYPH_HEIGHT] {
    match byte {
        b' ' => [0; 7],
        b'!' => [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
        b'"' => [
            0b01010, 0b01010, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
        b'#' => [
            0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010,
        ],
        b'$' => [
            0b00100, 0b01111, 0b10100, 0b01110, 0b00101, 0b11110, 0b00100,
        ],
        b'%' => [
            0b11000, 0b11001, 0b00010, 0b00100, 0b01000, 0b10011, 0b00011,
        ],
        b'&' => [
            0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101,
        ],
        b'\'' => [
            0b00100, 0b00100, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
        b'(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
        b')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
        b'*' => [
            0b00000, 0b00100, 0b10101, 0b01110, 0b10101, 0b00100, 0b00000,
        ],
        b'+' => [
            0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000,
        ],
        b',' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00100, 0b01000,
        ],
        b'-' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        b'.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100,
        ],
        b'/' => [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
        ],
        b'0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        b'1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        b'2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        b'3' => [
            0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
        ],
        b'4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        b'5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        b'6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        b'7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        b'8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        b'9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        b':' => [
            0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b01100, 0b00000,
        ],
        b';' => [
            0b00000, 0b01100, 0b01100, 0b00000, 0b00110, 0b00100, 0b01000,
        ],
        b'<' => [
            0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010,
        ],
        b'=' => [
            0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000,
        ],
        b'>' => [
            0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000,
        ],
        b'?' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100,
        ],
        b'@' => [
            0b01110, 0b10001, 0b10111, 0b10101, 0b10111, 0b10000, 0b01110,
        ],
        b'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        b'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        b'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        b'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        b'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        b'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        b'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        b'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        b'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        b'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        b'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        b'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        b'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        b'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        b'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        b'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        b'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        b'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        b'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        b'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        b'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        b'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        b'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        b'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        b'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        b'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        b'[' => [
            0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110,
        ],
        b'\\' => [
            0b10000, 0b01000, 0b01000, 0b00100, 0b00010, 0b00010, 0b00001,
        ],
        b']' => [
            0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110,
        ],
        b'^' => [
            0b00100, 0b01010, 0b10001, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
        b'_' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111,
        ],
        b'`' => [
            0b01000, 0b00100, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
        b'a' => [
            0b00000, 0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111,
        ],
        b'b' => [
            0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        b'c' => [
            0b00000, 0b00000, 0b01111, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        b'd' => [
            0b00001, 0b00001, 0b01111, 0b10001, 0b10001, 0b10001, 0b01111,
        ],
        b'e' => [
            0b00000, 0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110,
        ],
        b'f' => [
            0b00110, 0b01001, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000,
        ],
        b'g' => [
            0b00000, 0b00000, 0b01111, 0b10001, 0b01111, 0b00001, 0b01110,
        ],
        b'h' => [
            0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        b'i' => [
            0b00100, 0b00000, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        b'j' => [
            0b00010, 0b00000, 0b00110, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        b'k' => [
            0b10000, 0b10000, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010,
        ],
        b'l' => [
            0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        b'm' => [
            0b00000, 0b00000, 0b11010, 0b10101, 0b10101, 0b10101, 0b10101,
        ],
        b'n' => [
            0b00000, 0b00000, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        b'o' => [
            0b00000, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        b'p' => [
            0b00000, 0b00000, 0b11110, 0b10001, 0b11110, 0b10000, 0b10000,
        ],
        b'q' => [
            0b00000, 0b00000, 0b01111, 0b10001, 0b01111, 0b00001, 0b00001,
        ],
        b'r' => [
            0b00000, 0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000,
        ],
        b's' => [
            0b00000, 0b00000, 0b01111, 0b10000, 0b01110, 0b00001, 0b11110,
        ],
        b't' => [
            0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b01001, 0b00110,
        ],
        b'u' => [
            0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101,
        ],
        b'v' => [
            0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        b'w' => [
            0b00000, 0b00000, 0b10001, 0b10001, 0b10101, 0b10101, 0b01010,
        ],
        b'x' => [
            0b00000, 0b00000, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001,
        ],
        b'y' => [
            0b00000, 0b00000, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110,
        ],
        b'z' => [
            0b00000, 0b00000, 0b11111, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        b'{' => [
            0b00110, 0b01000, 0b01000, 0b11000, 0b01000, 0b01000, 0b00110,
        ],
        b'|' => [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        b'}' => [
            0b01100, 0b00010, 0b00010, 0b00011, 0b00010, 0b00010, 0b01100,
        ],
        b'~' => [
            0b00000, 0b00000, 0b01000, 0b10101, 0b00010, 0b00000, 0b00000,
        ],
        _ => [
            0b11111, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11111,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgbx8_pixel_order() {
        let mut bytes = [0; 4];
        let mut fb = Framebuffer::new(&mut bytes, 1, 1, 4, FB_FORMAT_RGBX8).unwrap();
        fb.put_pixel(0, 0, (1, 2, 3));
        assert_eq!(bytes, [1, 2, 3, 0]);
    }

    #[test]
    fn bgrx8_pixel_order() {
        let mut bytes = [0; 4];
        let mut fb = Framebuffer::new(&mut bytes, 1, 1, 4, FB_FORMAT_BGRX8).unwrap();
        fb.put_pixel(0, 0, (1, 2, 3));
        assert_eq!(bytes, [3, 2, 1, 0]);
    }

    #[test]
    fn pitch_selects_the_next_scanline() {
        let mut bytes = [0; 24];
        let mut fb = Framebuffer::new(&mut bytes, 2, 2, 12, FB_FORMAT_RGBX8).unwrap();
        fb.put_pixel(1, 1, (9, 8, 7));
        assert_eq!(&bytes[16..20], &[9, 8, 7, 0]);
        assert_eq!(&bytes[8..12], &[0; 4]);
    }

    #[test]
    fn rectangles_clip_to_framebuffer_bounds() {
        let mut bytes = [0; 16];
        let mut fb = Framebuffer::new(&mut bytes, 2, 2, 8, FB_FORMAT_RGBX8).unwrap();
        fb.fill_rect(1, 1, 10, 10, (4, 5, 6));
        assert_eq!(&bytes[12..16], &[4, 5, 6, 0]);
        assert_eq!(&bytes[0..12], &[0; 12]);
    }

    #[test]
    fn absent_or_invalid_framebuffer_is_not_rendered() {
        assert!(Framebuffer::new(&mut [], 0, 0, 0, FB_FORMAT_NONE).is_none());
        assert!(Framebuffer::new(&mut [0; 4], 1, 1, 3, FB_FORMAT_RGBX8).is_none());
    }

    /// A pitch wider than the visible width is the ordinary firmware case, and
    /// the invisible padding is not ours to paint: a clear must leave it alone.
    #[test]
    fn clear_leaves_scanline_padding_untouched() {
        let mut bytes = [0xffu8; 2 * 12];
        let mut fb = Framebuffer::new(&mut bytes, 2, 2, 12, FB_FORMAT_RGBX8).unwrap();
        fb.clear((1, 2, 3));
        assert_eq!(&bytes[0..8], &[1, 2, 3, 0, 1, 2, 3, 0]);
        assert_eq!(&bytes[8..12], &[0xff; 4]);
        assert_eq!(&bytes[20..24], &[0xff; 4]);
    }

    /// Text far off the right and bottom edges must not write a single byte,
    /// which is the property that keeps a long diagnostic from becoming memory
    /// corruption.
    #[test]
    fn text_outside_the_surface_writes_nothing() {
        let mut bytes = [0u8; 8 * 8 * 4];
        let mut fb = Framebuffer::new(&mut bytes, 8, 8, 32, FB_FORMAT_RGBX8).unwrap();
        fb.draw_text(b"BOOT", 4096, 4096, 2, TEXT);
        assert!(bytes.iter().all(|&byte| byte == 0));
    }

    /// Every byte the boot console can be asked to draw must have a cell, so
    /// nothing it prints degrades into the unmapped box.
    #[test]
    fn every_printable_ascii_byte_has_a_glyph() {
        let fallback = glyph(0x7f);
        for byte in 0x20u8..0x7f {
            if byte == b' ' {
                assert_eq!(glyph(byte), [0; GLYPH_HEIGHT], "space must be blank");
                continue;
            }
            assert_ne!(
                glyph(byte),
                [0; GLYPH_HEIGHT],
                "byte {byte:#x} draws nothing"
            );
            assert_ne!(glyph(byte), fallback, "byte {byte:#x} is unmapped");
        }
    }

    /// Glyph rows are 5 bits wide; a stray sixth bit would shift a column into
    /// the neighbouring character.
    #[test]
    fn glyph_rows_stay_inside_the_cell() {
        for byte in 0u8..=255 {
            for row in glyph(byte) {
                assert_eq!(row & !0b11111, 0, "byte {byte:#x} row overflows the cell");
            }
        }
    }

    #[test]
    fn ascii_grid_draws_only_at_cells() {
        let mut bytes = [0; 36];
        let mut fb = Framebuffer::new(&mut bytes, 3, 3, 12, FB_FORMAT_RGBX8).unwrap();
        fb.draw_ascii_grid(b"@ @\n @@", 0, 0, 1, (7, 8, 9));
        assert_eq!(&bytes[0..4], &[7, 8, 9, 0]);
        assert_eq!(&bytes[4..8], &[0; 4]);
        assert_eq!(&bytes[8..12], &[7, 8, 9, 0]);
        assert_eq!(&bytes[16..20], &[7, 8, 9, 0]);
        assert_eq!(&bytes[20..24], &[7, 8, 9, 0]);
    }

    #[test]
    fn pyro_uses_the_canonical_spdx_prefixed_artwork_body() {
        assert!(PYRO_ART_WITH_NOTICE.starts_with(b"# SPDX-License-Identifier: CC-BY-SA-4.0\n"));
        assert!(pyro_art_body().contains(&b'@'));
        assert_ne!(pyro_art_body(), PYRO_ART_WITH_NOTICE);
    }

    #[test]
    fn ascii_dimensions_measure_the_widest_row() {
        assert_eq!(ascii_dimensions(b"@@\n@@@@\n@"), (4, 3));
        let (columns, rows) = ascii_dimensions(pyro_art_body());
        assert!(columns > 40 && rows > 20, "unexpected mascot geometry");
    }
}
