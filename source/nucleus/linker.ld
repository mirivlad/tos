/* SPDX-License-Identifier: GPL-3.0-or-later */
/* TOS nucleus linker script: raw image, entry first, STATIC addresses.
   The nucleus is linked at NUCLEUS_BASE (0x1_000_000). The UEFI loader
   allocates exactly that physical address (ALLOCATE_ADDRESS) and copies
   the image there, then jumps to its first byte (`boot_entry`).
   Static relocation keeps cross-crate calls direct (no GOT).

   .got/.got.plt must be listed explicitly: they are orphan sections and
   LLD would otherwise not emit them into --oformat=binary, leaving the
   memcmp/memcpy/memset indirection slots pointing at garbage (observed in
   QEMU as #GP with RIP=0 after TOS.NUCLEUS.ENTRY). */

ENTRY(boot_entry)

SECTIONS
{
    . = 0x2000000;
    .text : {
        KEEP(*(.text.boot_entry))
        *(.text .text.*)
    }
    .rodata : { *(.rodata .rodata.*) }
    .data : { *(.data .data.*) }
    .got : { *(.got .got.*) }
    .got.plt : { *(.got.plt .got.plt.*) }
    .bss (NOLOAD) : { *(.bss .bss.*) }
}
