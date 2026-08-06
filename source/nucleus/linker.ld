/* SPDX-License-Identifier: GPL-3.0-or-later */
/* TOS nucleus linker script: raw image, entry first, RIP-relative only.
   The loader copies the resulting binary to executable pages and jumps to
   its first byte, which is the `boot_entry` symbol. */

ENTRY(boot_entry)

SECTIONS
{
    . = 0x0;
    .text : {
        KEEP(*(.text.boot_entry))
        *(.text .text.*)
    }
    .rodata : { *(.rodata .rodata.*) }
    .data : { *(.data .data.*) }
    .bss (NOLOAD) : { *(.bss .bss.*) }
}
