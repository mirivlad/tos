# SPDX-License-Identifier: GPL-3.0-or-later
/* x86_64 Stage 1 exception stubs.  Every stub normalizes the CPU frame to
 * vector, error, RIP before calling the Rust fatal handler. */

/* Stated, not inherited: these are instructions, and instructions belong in the
 * section the nucleus maps executable. Without this line the stubs land in
 * whatever section the previously concatenated assembly file left current. */
.section .text

/* The flags the nucleus begins with, at every door an interrupt opens.
 *
 * A gate clears IF and TF and nothing else, so RFLAGS on entry to a handler is
 * still the RFLAGS of whatever was interrupted — a process's, when a process
 * was on the processor. Two of its bits are not decoration:
 *
 *   DF reverses `rep movs` and `rep stos`. The System V AMD64 ABI requires DF=0
 *   at every function entry, so every Rust function called from this file — and
 *   the `memcpy` the compiler emits for a struct assignment — is compiled on
 *   that promise. A tick taken inside a process's own `std … cld` window, which
 *   is `memmove` copying overlapping bytes backwards, carried DF=1 into the
 *   scheduler; `*frame = table[next].frame` then wrote the frame's 160 bytes
 *   *downwards* from its start, over the return address the stub had pushed one
 *   instruction earlier, and the `ret` left for wherever those bytes said.
 *   Observed as a page fault at CPL 0 on a page-aligned address in the frame
 *   pool, roughly once in thirty boots of a two-process gate.
 *
 *   NT can be set by `popfq` at CPL 3, is inert there, and makes `iretq` raise
 *   #GP in long mode — so a handler that inherits it cannot return at all.
 *
 * The other door into the nucleus has had this right from the start: IA32_FMASK
 * clears TF, IF, DF, NT and AC on `syscall` (syscall.rs). This is that same
 * rule at this door. It states a known state rather than masking bits out of an
 * inherited one because nothing a handler needs is in RFLAGS: the interrupted
 * flags are in the frame the processor pushed, which is where the handler reads
 * them and where `iretq` puts them back. */
.macro NUCLEUS_FLAGS
    push 0
    popfq
.endm

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
    NUCLEUS_FLAGS
    mov r11, rsp
    mov rdi, qword ptr [r11]
    mov rsi, qword ptr [r11 + 8]
    mov rdx, qword ptr [r11 + 16]
    /* The interrupted CS. Its low two bits are the privilege level the fault
       was taken at, which is the whole of the difference between "the boot is
       over" and "a process is over" (ADR-0049 section 3). */
    mov rcx, qword ptr [r11 + 24]
    /* The interrupted RSP. In long mode the processor pushes SS:RSP on every
       interrupt, same privilege or not, so this is always there — and it is
       what tells a wild jump apart from a stack that ran out, which is a
       distinction a fatal report should not make its reader guess at. */
    mov r8, qword ptr [r11 + 40]
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

/* The two handlers that return.
 *
 * Every stub above ends the boot, so none of them preserves anything. These do
 * the opposite: an interrupt taken at CPL 3 goes back to the process it
 * interrupted, and a process that came back with a different register than it
 * left with would be a process the system corrupted. Callee-saved registers are
 * the C function's problem; these save the rest. */
/* The timer stub saves *everything*, not only what the C ABI clobbers.
 *
 * Counting a tick needs nine registers saved. Choosing a different process to
 * return to needs all fifteen, plus the frame the processor pushed, because
 * what `iretq` reads is then not what the interrupt wrote. The frame this
 * builds is laid out to match `TrapFrame` exactly, and its address is what the
 * handler receives — so the step from "count a tick" to "resume someone else"
 * is a change of what the handler writes, not of what it can reach. */
.global timer_stub
timer_stub:
    NUCLEUS_FLAGS
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx
    push rbx
    push rax
    mov rdi, rsp            /* the frame, as the handler's one argument */
    sub rsp, 8              /* the call wants RSP 16-aligned before it */
    call timer_interrupt
    add rsp, 8
    pop rax
    pop rbx
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    iretq

.global spurious_stub
spurious_stub:
    NUCLEUS_FLAGS
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    sub rsp, 8
    call spurious_interrupt
    add rsp, 8
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    iretq

/* One stub per routed device vector (ADR-0082 §5f step 7).
 *
 * The handler has to know *which* vector arrived, and the architecture does not
 * push one. Reading the local APIC's in-service register back would be asking
 * the controller to re-derive something the gate already knew, so the gate
 * carries it: each stub is identical apart from the index it puts in `edi`.
 *
 * A gate exists for every vector of the range from boot and is never removed,
 * which is what makes a late message on a **retired** vector land somewhere
 * defined instead of in an empty IDT slot.
 */
.macro DEVICE_STUB index
.global device_stub_\index
device_stub_\index:
    NUCLEUS_FLAGS
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    mov edi, \index         /* which of the device vectors this gate is */
    sub rsp, 8              /* the call wants RSP 16-aligned before it */
    call device_interrupt
    add rsp, 8
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    iretq
.endm

DEVICE_STUB 0
DEVICE_STUB 1
DEVICE_STUB 2
DEVICE_STUB 3
DEVICE_STUB 4
DEVICE_STUB 5
DEVICE_STUB 6
DEVICE_STUB 7
DEVICE_STUB 8
DEVICE_STUB 9
DEVICE_STUB 10
DEVICE_STUB 11
DEVICE_STUB 12
DEVICE_STUB 13
DEVICE_STUB 14
DEVICE_STUB 15

/* A table of addresses, not code. */
.section .rodata
.p2align 3
.global device_stub_table
device_stub_table:
.quad device_stub_0, device_stub_1, device_stub_2, device_stub_3
.quad device_stub_4, device_stub_5, device_stub_6, device_stub_7
.quad device_stub_8, device_stub_9, device_stub_10, device_stub_11
.quad device_stub_12, device_stub_13, device_stub_14, device_stub_15

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

.section .text
