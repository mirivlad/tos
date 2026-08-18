/* SPDX-License-Identifier: GPL-3.0-or-later */
/* The ring-3 runtime image: a raw image whose first byte is its entry.

   Linked at RUNTIME_BASE (0x1000_0000) in the *process's* address space. The
   nucleus maps the image there and enters at its first byte, so the address is
   part of the launch boundary and moving it is a versioned change. */

ENTRY(runtime_entry)

SECTIONS
{
    . = 0x10000000;
    __tos_runtime_start = .;
    .text : {
        KEEP(*(.text.runtime_entry))
        *(.text .text.*)
    }
    .rodata : { *(.rodata .rodata.*) }
    .data : { *(.data .data.*) }
    .got : { *(.got .got.*) }
    .got.plt : { *(.got.plt .got.plt.*) }
    __tos_runtime_load_end = .;
    .bss (NOLOAD) : { *(.bss .bss.*) }
    . = ALIGN(4096);
    __tos_runtime_end = .;
}
