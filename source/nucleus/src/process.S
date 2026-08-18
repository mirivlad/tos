# SPDX-License-Identifier: GPL-3.0-or-later
/* Leaving ring 0 for a process, and getting back when the process ends.

   The pair below is the whole of "the nucleus resumes": a context captured
   before a process starts, and a jump back into it when the process is over.
   It exists because a process does not return — it is left by `iretq` and
   re-entered by a fault or by the system-call edge — so the nucleus has to
   record where to continue *before* it stops being the thing running. */

.section .text

/* rdi = entry point, rsi = top of the user stack, rdx = code selector,
   rcx = data selector, r8 = the launch record's address in the process's own
   space. The selectors are passed in rather than written here so that the GDT
   stays the single place that decides them, and the record is put into RDI at
   the last instruction before the jump — a Rust call between here and there
   would use RDI for its own first argument and the process would be entered
   with a pointer to whatever that was. Does not return. */
.global process_enter
process_enter:
    /* The stack the edge re-enters the nucleus on. Everything pushed below
       this point belongs to a frame that `iretq` abandons. */
    mov qword ptr [rip + syscall_kernel_rsp], rsp
    push rcx                /* SS */
    push rsi                /* RSP */
    push 0x2                /* RFLAGS: interrupts stay masked — nothing routes
                               an interrupt yet, and IF=1 here would send the
                               first stray one to a vector with no gate */
    push rdx                /* CS */
    push rdi                /* RIP */
    mov rdi, r8             /* the process's one argument */
    iretq

/* rdi = where to record the caller's context. Returns 0 to the caller that
   recorded it and whatever `process_resume` was given to the caller that is
   resumed — the one distinction the nucleus needs to tell "a process is about
   to start" from "a process has ended". */
.global process_capture
process_capture:
    mov qword ptr [rdi + 0], rbx
    mov qword ptr [rdi + 8], rbp
    mov qword ptr [rdi + 16], r12
    mov qword ptr [rdi + 24], r13
    mov qword ptr [rdi + 32], r14
    mov qword ptr [rdi + 40], r15
    mov qword ptr [rdi + 48], rsp
    mov rax, qword ptr [rsp]        /* the return address `ret` will consume */
    mov qword ptr [rdi + 56], rax
    xor eax, eax
    ret

/* rdi = a context recorded by `process_capture`, rsi = what its caller sees
   returned. Does not return to *its* caller: it continues the other one. */
.global process_resume
process_resume:
    mov rbx, qword ptr [rdi + 0]
    mov rbp, qword ptr [rdi + 8]
    mov r12, qword ptr [rdi + 16]
    mov r13, qword ptr [rdi + 24]
    mov r14, qword ptr [rdi + 32]
    mov r15, qword ptr [rdi + 40]
    mov rsp, qword ptr [rdi + 48]
    mov rax, qword ptr [rdi + 56]
    mov qword ptr [rsp], rax        /* the return address, put back as found */
    mov rax, rsi
    ret

.section .text
