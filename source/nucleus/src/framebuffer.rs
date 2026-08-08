// SPDX-License-Identifier: GPL-3.0-or-later
//! Best-effort Stage 1 framebuffer diagnostic.
//!
//! This is deliberately not a console, GUI or public ABI. It draws only after
//! the normal boot decision has succeeded and silently does nothing if a
//! framebuffer is absent or cannot be represented safely.

use tos_boot_protocol::{BootInfo, FB_FORMAT_BGRX8, FB_FORMAT_NONE, FB_FORMAT_RGBX8};

type Color = (u8, u8, u8);

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

    fn fill_rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: Color) {
        let end_x = x.saturating_add(width).min(self.width);
        let end_y = y.saturating_add(height).min(self.height);
        for py in y.min(self.height)..end_y {
            for px in x.min(self.width)..end_x {
                self.put_pixel(px, py, color);
            }
        }
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

    fn draw_text(&mut self, text: &[u8], x: usize, y: usize, scale: usize, color: Color) {
        let mut cursor = x;
        for &byte in text {
            for (row, bits) in glyph(byte).iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
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
            cursor = cursor.saturating_add(6 * scale);
        }
    }

    fn stage1_status(&mut self) {
        const BACKGROUND: Color = (12, 17, 27);
        const PANEL: Color = (25, 39, 61);
        const ACCENT: Color = (255, 114, 57);
        const TEXT: Color = (226, 234, 243);

        self.fill_rect(0, 0, self.width, self.height, BACKGROUND);
        let scale = (self.width / 180).min(self.height / 100).clamp(1, 3);
        let margin = 8usize.saturating_mul(scale);
        self.fill_rect(
            margin,
            margin,
            self.width.saturating_sub(margin.saturating_mul(2)),
            self.height.saturating_sub(margin.saturating_mul(2)),
            PANEL,
        );
        let x = margin.saturating_add(8 * scale);
        let mut y = margin.saturating_add(8 * scale);
        self.draw_text(b"TOS", x, y, scale.saturating_mul(2), ACCENT);
        // A neutral synthetic status marker exercises the generic grid path.
        // It is not Pyro or any other separately licensed artwork.
        self.draw_ascii_grid(
            b"@@@\n@ @\n@@@",
            self.width.saturating_sub(16 * scale),
            margin.saturating_add(8 * scale),
            scale,
            ACCENT,
        );
        y = y.saturating_add(20 * scale);
        self.draw_text(b"TRUSTED BOOT FOUNDATION", x, y, scale, TEXT);
        y = y.saturating_add(14 * scale);
        self.draw_text(b"CAPSULE VERIFIED", x, y, scale, TEXT);
        y = y.saturating_add(10 * scale);
        self.draw_text(b"SOURCE GIT", x, y, scale, TEXT);
        y = y.saturating_add(10 * scale);
        self.draw_text(b"BOOT ABI V1", x, y, scale, TEXT);
        y = y.saturating_add(10 * scale);
        self.draw_text(b"STAGE 1", x, y, scale, TEXT);
    }
}

/// Draw the final human-facing Stage 1 status without changing boot outcome.
///
/// # Safety
///
/// The caller supplies only a validated BootInfo from the loader's verified
/// handoff. ADR-0022 requires the present framebuffer range to be mapped and
/// reserved; this function uses its checked pitch × height size and performs no
/// allocation. Invalid/absent values return without writing.
#[cfg_attr(test, allow(dead_code))]
pub unsafe fn render_stage1_status(bi: &BootInfo) {
    if bi.framebuffer_format == FB_FORMAT_NONE || bi.framebuffer_phys == 0 {
        return;
    }
    let Some(required) =
        u64::from(bi.framebuffer_pitch).checked_mul(u64::from(bi.framebuffer_height))
    else {
        return;
    };
    let Ok(length) = usize::try_from(required) else {
        return;
    };
    let bytes = core::slice::from_raw_parts_mut(bi.framebuffer_phys as *mut u8, length);
    if let Some(mut framebuffer) = Framebuffer::new(
        bytes,
        bi.framebuffer_width,
        bi.framebuffer_height,
        bi.framebuffer_pitch,
        bi.framebuffer_format,
    ) {
        framebuffer.stage1_status();
    }
}

fn glyph(byte: u8) -> [u8; 7] {
    match byte {
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
        b'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        b'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
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
        b'1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        _ => [0; 7],
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
    fn status_panel_uses_bounded_renderer() {
        let mut bytes = [0; 160 * 100 * 4];
        let mut fb = Framebuffer::new(&mut bytes, 160, 100, 640, FB_FORMAT_RGBX8).unwrap();
        fb.stage1_status();
        assert!(bytes.iter().any(|&byte| byte != 0));
    }
}
