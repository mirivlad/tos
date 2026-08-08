# SPDX-License-Identifier: GPL-3.0-or-later
/* x86_64 Stage 1 exception stubs.  Every stub normalizes the CPU frame to
 * vector, error, RIP before calling the Rust fatal handler. */

.macro NOERR vector
.global exception_stub_\vector
exception_stub_\vector:
    push 0
    push \vector
    jmp exception_common
.endm

.macro ERR vector
.global exception_stub_\vector
exception_stub_\vector:
    push \vector
    jmp exception_common
.endm

.global exception_common
exception_common:
    mov r11, rsp
    mov rdi, qword ptr [r11]
    mov rsi, qword ptr [r11 + 8]
    mov rdx, qword ptr [r11 + 16]
    and rsp, -16
    call exception_fatal
1:
    hlt
    jmp 1b

NOERR 0
NOERR 1
NOERR 2
NOERR 3
NOERR 4
NOERR 5
NOERR 6
NOERR 7
ERR   8
NOERR 9
ERR   10
ERR   11
ERR   12
ERR   13
ERR   14
NOERR 15
NOERR 16
ERR   17
NOERR 18
NOERR 19
NOERR 20
ERR   21
NOERR 22
NOERR 23
NOERR 24
NOERR 25
NOERR 26
NOERR 27
NOERR 28
NOERR 29
NOERR 30
NOERR 31

.p2align 3
.global exception_stub_table
exception_stub_table:
.quad exception_stub_0, exception_stub_1, exception_stub_2, exception_stub_3
.quad exception_stub_4, exception_stub_5, exception_stub_6, exception_stub_7
.quad exception_stub_8, exception_stub_9, exception_stub_10, exception_stub_11
.quad exception_stub_12, exception_stub_13, exception_stub_14, exception_stub_15
.quad exception_stub_16, exception_stub_17, exception_stub_18, exception_stub_19
.quad exception_stub_20, exception_stub_21, exception_stub_22, exception_stub_23
.quad exception_stub_24, exception_stub_25, exception_stub_26, exception_stub_27
.quad exception_stub_28, exception_stub_29, exception_stub_30, exception_stub_31
