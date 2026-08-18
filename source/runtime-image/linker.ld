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

    /* The image's own header, first, and emitted by the linker rather than by
       the program: the values it carries are section boundaries, and the only
       thing that knows those is the thing that placed them. A nucleus reading a
       raw image has no symbol table, so this is how the image says which of its
       bytes are text, which are data, and how much memory it needs beyond what
       the file carries. */
    .tos.header : {
        QUAD(0x534f54494d473100)                     /* "SOTIMG1\0" */
        QUAD(runtime_entry - __tos_runtime_start)           /* entry offset */
        QUAD(__tos_runtime_text_end - __tos_runtime_start)  /* text bytes */
        QUAD(__tos_runtime_load_end - __tos_runtime_start)  /* file bytes */
        QUAD(__tos_runtime_end - __tos_runtime_start)       /* memory bytes */
    }

    .text : {
        KEEP(*(.text.runtime_entry))
        *(.text .text.*)
    }
    .rodata : { *(.rodata .rodata.*) }
    /* Page-aligned, because a permission is a property of a page: text sharing
       a page with writable data would have to be one or the other. */
    . = ALIGN(4096);
    __tos_runtime_text_end = .;
    .data : { *(.data .data.*) }
    .got : { *(.got .got.*) }
    .got.plt : { *(.got.plt .got.plt.*) }
    . = ALIGN(4096);
    __tos_runtime_load_end = .;
    .bss (NOLOAD) : { *(.bss .bss.*) }
    . = ALIGN(4096);
    __tos_runtime_end = .;
}
