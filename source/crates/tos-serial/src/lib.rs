// SPDX-License-Identifier: GPL-3.0-or-later
//! Polled 16550 UART on COM1 (I/O ports 0x3F8..0x3FF), 115200 8N1.
//! Drives the serial boot-event log (`interfaces/boot/BOOT_ABI_V1.md` §6).
//! Shared by the UEFI loader and the nucleus.

#![no_std]

use core::arch::asm;

const COM1: u16 = 0x3F8;
const THR: u16 = COM1; // +0 transmit holding
const LSR: u16 = COM1 + 5; // line status
const LSR_THR_EMPTY: u8 = 0x20;

#[inline]
fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
fn inb(port: u16) -> u8 {
    let v: u8;
    unsafe {
        asm!("in al, dx", out("al") v, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    v
}

/// Initialise COM1 to 115200 8N1.
pub fn init() {
    outb(COM1 + 1, 0x00); // disable interrupts
    outb(COM1 + 3, 0x80); // DLAB on
    outb(COM1, 0x01); // +0 divisor low: DLL aliases THR while DLAB is set
    outb(COM1 + 1, 0x00); // divisor high
    outb(COM1 + 3, 0x03); // 8N1, DLAB off
    outb(COM1 + 2, 0xC7); // FIFO enable, clear, 14-byte threshold
    outb(COM1 + 4, 0x0B); // IRQs enabled, RTS/DSR set
}

/// Transmit one byte, polling until the THR is empty.
pub fn putc(c: u8) {
    while inb(LSR) & LSR_THR_EMPTY == 0 {}
    outb(THR, c);
}

/// Transmit a byte string.
pub fn puts(s: &[u8]) {
    for &b in s {
        putc(b);
    }
}

/// Transmit a hex digit.
pub fn put_hex_nibble(v: u8) {
    let c = if v < 10 { b'0' + v } else { b'a' + v - 10 };
    putc(c);
}

/// Transmit a u32 as decimal digits.
pub fn put_u32_decimal(v: u32) {
    let mut buf = [0u8; 10];
    let mut n = v;
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    puts(&buf[i..]);
}

/// Transmit a u64 as 16 lowercase hex digits.
pub fn put_hex64(v: u64) {
    for i in (0..16).rev() {
        put_hex_nibble(((v >> (i * 4)) & 0xf) as u8);
    }
}

/// Transmit a 32-byte digest as 64 hex digits.
pub fn put_hex32(d: &[u8; 32]) {
    for b in d {
        put_hex_nibble(b >> 4);
        put_hex_nibble(b & 0xf);
    }
}
