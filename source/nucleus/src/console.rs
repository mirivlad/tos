// SPDX-License-Identifier: GPL-3.0-or-later
//! The human-facing boot console.
//!
//! This is a bounded boot renderer and nothing more. It is not a terminal, a
//! tty, a shell or the Stage 3 console: it has no input, no scrollback, no
//! escape sequences and no cursor addressing a caller can reach. It can clear a
//! screen, draw a header, put down one boot-status row, resolve that row as
//! finished or failed, show a diagnosis, and draw the final screen.
//!
//! **It reports; it does not decide.** Every row it draws is a fact the boot
//! path had already established over serial, which is the normative channel. A
//! row is put down as `[ .. ]` *before* the work starts, so a system that stops
//! is named by the step it stopped in; it becomes `[ OK ]` only once that step
//! has actually returned. Nothing here can change a boot outcome: the console
//! may be absent, and then the boot is identical minus the picture.
//!
//! Everything is bounded by construction. Rows stop at the bottom of the
//! surface rather than wrapping or scrolling, text is truncated to the width it
//! has, and no operation allocates.

use crate::framebuffer::{
    ascii_dimensions, pyro_art_body, text_width, Color, Framebuffer, ACCENT, BACKGROUND, DONE,
    FAILED, GLYPH_HEIGHT, MUTED, TEXT,
};

/// The final screen's message. Two separate lines, and both say only what is
/// true at this point: the Stage 2 runtime finished and the machine stopped.
/// Not "ready", not "welcome" — nothing continues after this.
pub const COMPLETE_LINE: &[u8] = b"Stage 2 runtime complete.";
pub const HALTED_LINE: &[u8] = b"System halted normally.";

const TITLE: &[u8] = b"TOS";
const SUBTITLE: &[u8] = b"Text Operating System";
const STOPPED: &[u8] = b"Boot stopped.";

/// Width of the status field, in characters: `[ OK ]`.
const STATUS_CHARS: usize = 6;

/// What a boot row is reporting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    /// The step has been entered and has not returned.
    Busy,
    /// The step returned successfully.
    Done,
    /// The step ended the boot.
    Failed,
}

impl Status {
    /// The marker text. All three are the same width, so a row changing state
    /// does not move the text beside it.
    pub fn marker(self) -> &'static [u8] {
        match self {
            Status::Busy => b"[ .. ]",
            Status::Done => b"[ OK ]",
            Status::Failed => b"[FAIL]",
        }
    }

    fn color(self) -> Color {
        match self {
            Status::Busy => ACCENT,
            Status::Done => DONE,
            Status::Failed => FAILED,
        }
    }
}

/// A short piece of text assembled without allocating.
///
/// Writing past the capacity truncates rather than panicking or wrapping: a
/// diagnosis too long for the space it has is still worth showing, and the full
/// text is on serial either way.
pub struct Text<const N: usize> {
    bytes: [u8; N],
    length: usize,
}

impl<const N: usize> Default for Text<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Text<N> {
    pub const fn new() -> Self {
        Self {
            bytes: [0; N],
            length: 0,
        }
    }

    pub fn push(&mut self, text: &[u8]) -> &mut Self {
        for &byte in text {
            if self.length == N {
                break;
            }
            self.bytes[self.length] = byte;
            self.length += 1;
        }
        self
    }

    pub fn push_number(&mut self, value: usize) -> &mut Self {
        // 20 digits is the widest u64 in decimal.
        let mut digits = [0u8; 20];
        let mut count = 0;
        let mut rest = value;
        loop {
            digits[count] = b'0' + (rest % 10) as u8;
            count += 1;
            rest /= 10;
            if rest == 0 || count == digits.len() {
                break;
            }
        }
        while count > 0 {
            count -= 1;
            self.push(&[digits[count]]);
        }
        self
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

/// The boot console over one framebuffer.
pub struct BootConsole<'a> {
    fb: Framebuffer<'a>,
    scale: usize,
    /// Vertical distance between row baselines.
    line_height: usize,
    margin_x: usize,
    margin_y: usize,
    /// Where the next row goes.
    next_y: usize,
    /// The row awaiting an outcome, if one is open and was actually drawn.
    open: Option<usize>,
    /// Whether a row is open at all, drawn or not. A row that did not fit still
    /// owns the outcome, so its result is not applied to an earlier row.
    open_row: bool,
}

impl<'a> BootConsole<'a> {
    /// Take the screen and draw the header.
    pub fn new(fb: Framebuffer<'a>) -> Self {
        // Chosen so an ordinary QEMU surface holds every boot step with room to
        // spare: at 800x600 this is scale 2, giving 18-pixel rows and about
        // twenty-five of them below the header.
        let scale = (fb.width() / 400).min(fb.height() / 300).clamp(1, 3);
        let mut console = Self {
            fb,
            scale,
            line_height: (GLYPH_HEIGHT + 2) * scale,
            margin_x: 12 * scale,
            margin_y: 8 * scale,
            next_y: 0,
            open: None,
            open_row: false,
        };
        console.header();
        console
    }

    fn header(&mut self) {
        self.fb.clear(BACKGROUND);
        let title_scale = self.scale * 2;
        let mut y = self.margin_y;
        self.fb
            .draw_text(TITLE, self.margin_x, y, title_scale, ACCENT);
        y += GLYPH_HEIGHT * title_scale + 4 * self.scale;
        self.fb
            .draw_text(SUBTITLE, self.margin_x, y, self.scale, MUTED);
        self.next_y = y + GLYPH_HEIGHT * self.scale + self.line_height;
    }

    /// The last y a row may start at and still fit entirely on the surface.
    fn last_row_y(&self) -> usize {
        self.fb
            .height()
            .saturating_sub(self.margin_y + GLYPH_HEIGHT * self.scale)
    }

    /// Draw one row, or nothing if the surface is full. Returns its y.
    fn row(&mut self, status: Status, label: &[u8], detail: Option<&[u8]>) -> Option<usize> {
        if self.next_y > self.last_row_y() {
            return None;
        }
        let y = self.next_y;
        self.next_y += self.line_height;
        self.fb.draw_text(
            status.marker(),
            self.margin_x,
            y,
            self.scale,
            status.color(),
        );
        let mut x = self.margin_x + text_width(STATUS_CHARS + 1, self.scale);
        self.write_at(&mut x, y, label, TEXT);
        if let Some(detail) = detail {
            x += text_width(1, self.scale);
            self.write_at(&mut x, y, detail, MUTED);
        }
        Some(y)
    }

    /// Draw text at `x`, truncated to what the surface can hold, and advance
    /// `x` past it.
    fn write_at(&mut self, x: &mut usize, y: usize, text: &[u8], color: Color) {
        let room = self
            .fb
            .width()
            .saturating_sub(*x + self.margin_x)
            .checked_div(text_width(1, self.scale))
            .unwrap_or(0);
        let text = &text[..text.len().min(room)];
        self.fb.draw_text(text, *x, y, self.scale, color);
        *x += text_width(text.len(), self.scale);
    }

    /// Repaint the status field of the open row, if there is one on screen.
    fn resolve(&mut self, status: Status) {
        if let Some(y) = self.open.take() {
            self.fb.fill_rect(
                self.margin_x,
                y,
                text_width(STATUS_CHARS, self.scale),
                GLYPH_HEIGHT * self.scale,
                BACKGROUND,
            );
            self.fb.draw_text(
                status.marker(),
                self.margin_x,
                y,
                self.scale,
                status.color(),
            );
        }
        self.open_row = false;
    }

    /// A fact already established when the console was able to draw it.
    pub fn fact(&mut self, label: &[u8], detail: Option<&[u8]>) {
        self.row(Status::Done, label, detail);
    }

    /// Announce a step that is about to run. It stays `[ .. ]` until it
    /// returns, so a stall is named by the step that is stalling.
    pub fn begin(&mut self, label: &[u8], detail: Option<&[u8]>) {
        self.open = self.row(Status::Busy, label, detail);
        self.open_row = true;
    }

    /// The open step returned successfully.
    pub fn succeed(&mut self) {
        self.resolve(Status::Done);
    }

    /// Whether a step is waiting for its outcome.
    pub fn is_busy(&self) -> bool {
        self.open_row
    }

    /// The open step ended the boot. The log above it is kept: the point of the
    /// failure screen is that the operator can see how far the system got.
    pub fn fail(&mut self, code: &[u8], location: &[u8]) {
        self.resolve(Status::Failed);
        self.blank_row();
        self.text_row(code, FAILED);
        if !location.is_empty() {
            self.text_row(location, MUTED);
        }
        self.blank_row();
        self.text_row(STOPPED, TEXT);
    }

    fn blank_row(&mut self) {
        if self.next_y <= self.last_row_y() {
            self.next_y += self.line_height;
        }
    }

    fn text_row(&mut self, text: &[u8], color: Color) {
        if self.next_y > self.last_row_y() {
            return;
        }
        let y = self.next_y;
        self.next_y += self.line_height;
        let mut x = self.margin_x;
        self.write_at(&mut x, y, text, color);
    }

    /// The boot log has done its work: replace it with the final screen.
    ///
    /// Only the successful path reaches this. The mascot is the canonical
    /// artwork the primitives already carry, drawn as large as the surface
    /// allows, and the two lines below it say exactly what happened.
    pub fn final_screen(&mut self) {
        self.fb.clear(BACKGROUND);
        self.open = None;
        self.open_row = false;

        let title_scale = self.scale * 2;
        let title_height = GLYPH_HEIGHT * title_scale;
        let message_scale = self.message_scale();
        let message_height = GLYPH_HEIGHT * message_scale;
        let gap = self.line_height;
        let message_block = message_height * 2 + gap;

        let art = pyro_art_body();
        let (columns, rows) = ascii_dimensions(art);
        let reserved = title_height + message_block + gap * 4 + self.margin_y * 2;
        let cell = self
            .fb
            .width()
            .saturating_sub(self.margin_x * 2)
            .checked_div(columns.max(1))
            .unwrap_or(0)
            .min(
                self.fb
                    .height()
                    .saturating_sub(reserved)
                    .checked_div(rows.max(1))
                    .unwrap_or(0),
            );
        let art_height = rows * cell;

        let block = title_height + gap * 2 + art_height + gap * 2 + message_block;
        let mut y = self.fb.height().saturating_sub(block) / 2;
        y = y.max(self.margin_y);

        self.draw_centered(TITLE, y, title_scale, ACCENT);
        y += title_height + gap * 2;
        if cell > 0 {
            let art_width = columns * cell;
            let x = self.fb.width().saturating_sub(art_width) / 2;
            self.fb.draw_ascii_grid(art, x, y, cell, ACCENT);
            y += art_height;
        }
        y += gap * 2;
        self.draw_centered(COMPLETE_LINE, y, message_scale, TEXT);
        y += message_height + gap;
        self.draw_centered(HALTED_LINE, y, message_scale, TEXT);
    }

    /// The largest scale at which both message lines still fit the width.
    fn message_scale(&self) -> usize {
        let longest = COMPLETE_LINE.len().max(HALTED_LINE.len());
        let room = self.fb.width().saturating_sub(self.margin_x * 2);
        let mut scale = self.scale + 1;
        while scale > 1 && text_width(longest, scale) > room {
            scale -= 1;
        }
        scale
    }

    fn draw_centered(&mut self, text: &[u8], y: usize, scale: usize, color: Color) {
        // The trailing tracking column is not part of the visible text, so it
        // is left out of the measurement that centres it.
        let width = text_width(text.len(), scale).saturating_sub(scale);
        let x = self.fb.width().saturating_sub(width) / 2;
        self.fb.draw_text(text, x, y, scale, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framebuffer::Framebuffer;
    use tos_boot_protocol::{FB_FORMAT_BGRX8, FB_FORMAT_RGBX8};

    const WIDTH: usize = 800;
    const HEIGHT: usize = 600;

    struct Surface {
        bytes: Vec<u8>,
    }

    impl Surface {
        fn new() -> Self {
            Self {
                bytes: vec![0; WIDTH * HEIGHT * 4],
            }
        }

        fn console(&mut self) -> BootConsole<'_> {
            let fb = Framebuffer::new(
                &mut self.bytes,
                WIDTH as u32,
                HEIGHT as u32,
                (WIDTH * 4) as u32,
                FB_FORMAT_RGBX8,
            )
            .expect("surface");
            BootConsole::new(fb)
        }

        fn pixel(&self, x: usize, y: usize) -> (u8, u8, u8) {
            let offset = y * WIDTH * 4 + x * 4;
            (
                self.bytes[offset],
                self.bytes[offset + 1],
                self.bytes[offset + 2],
            )
        }

        /// Whether the surface holds a pixel of this colour anywhere.
        fn has(&self, color: Color) -> bool {
            self.bytes
                .chunks_exact(4)
                .any(|pixel| (pixel[0], pixel[1], pixel[2]) == color)
        }

        fn count(&self, color: Color) -> usize {
            self.bytes
                .chunks_exact(4)
                .filter(|pixel| (pixel[0], pixel[1], pixel[2]) == color)
                .count()
        }
    }

    #[test]
    fn text_truncates_instead_of_overflowing() {
        let mut text = Text::<8>::new();
        text.push(b"abcdefghij");
        assert_eq!(text.as_bytes(), b"abcdefgh");
    }

    #[test]
    fn text_writes_decimal_numbers() {
        let mut text = Text::<32>::new();
        text.push(b"init.tos:").push_number(37).push(b":");
        text.push_number(9);
        assert_eq!(text.as_bytes(), b"init.tos:37:9");
        let mut zero = Text::<4>::new();
        zero.push_number(0);
        assert_eq!(zero.as_bytes(), b"0");
    }

    #[test]
    fn header_is_drawn_on_a_cleared_surface() {
        let mut surface = Surface::new();
        surface.console();
        assert_eq!(surface.pixel(0, 0), BACKGROUND);
        assert!(surface.has(ACCENT), "the wordmark is missing");
        assert!(surface.has(MUTED), "the subtitle is missing");
    }

    /// pending/current -> success. The marker changes and nothing else does.
    #[test]
    fn an_open_row_resolves_to_success() {
        let mut surface = Surface::new();
        {
            let mut console = surface.console();
            console.begin(b"Reading canonical source", None);
            assert!(console.is_busy());
        }
        assert!(surface.has(ACCENT), "a busy marker must be drawn");
        let busy_accent = surface.count(ACCENT);

        let mut surface = Surface::new();
        {
            let mut console = surface.console();
            console.begin(b"Reading canonical source", None);
            console.succeed();
            assert!(!console.is_busy());
        }
        assert!(
            surface.has(DONE),
            "a finished row must show the done marker"
        );
        assert!(
            surface.count(ACCENT) < busy_accent,
            "the busy marker must be repainted, not left behind"
        );
        assert!(!surface.has(FAILED));
    }

    /// pending/current -> failure. The row that failed stays on screen, the
    /// rows above it are untouched, and the diagnosis is added below.
    #[test]
    fn an_open_row_resolves_to_failure_and_keeps_the_log() {
        let mut surface = Surface::new();
        {
            let mut console = surface.console();
            console.fact(b"Boot ABI v1", None);
            console.begin(b"Checking source", None);
            console.fail(b"E1223_REFUTABLE_PATTERN", b"system/boot/init.tos:37:9");
            assert!(!console.is_busy());
        }
        assert!(
            surface.has(DONE),
            "the earlier row must survive the failure"
        );
        assert!(surface.has(FAILED), "the failed row must be marked");
        assert!(surface.has(TEXT), "the diagnosis text is missing");
    }

    #[test]
    fn a_success_marker_is_never_drawn_before_the_step_returns() {
        let mut surface = Surface::new();
        {
            let mut console = surface.console();
            console.begin(b"Verifying tos-ir/v1", None);
        }
        assert!(
            !surface.has(DONE),
            "an unfinished step must not claim success"
        );
    }

    /// Rows stop at the bottom edge. Nothing wraps, nothing scrolls and, most
    /// importantly, nothing is written outside the buffer.
    #[test]
    fn rows_stop_at_the_bottom_of_the_surface() {
        let mut surface = Surface::new();
        {
            let mut console = surface.console();
            for _ in 0..500 {
                console.fact(b"Row", None);
            }
        }
        let last = HEIGHT - 1;
        for x in 0..WIDTH {
            assert_eq!(
                surface.pixel(x, last),
                BACKGROUND,
                "a row was drawn past the last full line"
            );
        }
    }

    /// The renderer must not touch a byte outside the slice it was handed, on
    /// any surface size, including one too small for its own header.
    #[test]
    fn a_tiny_surface_stays_inside_its_buffer() {
        for (width, height) in [(1u32, 1u32), (16, 8), (64, 40), (320, 200)] {
            let pitch = (width as usize) * 4;
            let mut bytes = vec![0u8; pitch * height as usize + 8];
            let guard = bytes.len() - 8;
            bytes[guard..].fill(0xa5);
            {
                let fb = Framebuffer::new(
                    &mut bytes[..guard],
                    width,
                    height,
                    pitch as u32,
                    FB_FORMAT_BGRX8,
                )
                .expect("surface");
                let mut console = BootConsole::new(fb);
                console.fact(b"Boot ABI v1", None);
                console.begin(b"Checking source", Some(b"system/boot/init.tos"));
                console.fail(b"E1223_REFUTABLE_PATTERN", b"system/boot/init.tos:37:9");
                console.final_screen();
            }
            assert_eq!(
                &bytes[guard..],
                &[0xa5; 8],
                "{width}x{height} wrote past the framebuffer"
            );
        }
    }

    #[test]
    fn the_final_screen_replaces_the_log_with_the_mascot_and_its_two_lines() {
        let mut surface = Surface::new();
        {
            let mut console = surface.console();
            console.fact(b"Boot ABI v1", None);
            console.begin(b"Executing boot module", None);
            console.succeed();
            console.final_screen();
        }
        assert!(
            !surface.has(DONE),
            "the boot log must be gone from the final screen"
        );
        assert!(!surface.has(FAILED));
        // The mascot is the accent-coloured mass on this screen, and it must be
        // the dominant object rather than a decoration: the artwork alone
        // covers far more of the surface than the wordmark and text together.
        let art = pyro_art_body();
        let cells = art.iter().filter(|&&byte| byte == b'@').count();
        assert!(
            surface.count(ACCENT) > cells * 16,
            "the mascot is not drawn at a size that dominates the screen"
        );
        assert!(surface.has(TEXT), "the message lines are missing");
    }
}
