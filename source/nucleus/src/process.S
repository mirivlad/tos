# SPDX-License-Identifier: GPL-3.0-or-later
/* Leaving ring 0 for a process, and getting back when the process ends.

   Two directions, and they are not symmetric. Downward, into a process, is
   `process_start`: a frame is a frame, so the first entry into a process and
   the resumption of one the timer stopped are the same instructions over the
   same layout. Upward, out of one, is the capture/resume pair — the whole of
   "the scheduler resumes", a context recorded before a process starts and
   jumped back into when it is over. That pair exists because a process does not
   return: it is left by `iretq` and re-entered by a fault or by the system-call
   edge, so the nucleus has to record where to continue *before* it stops being
   the thing running. */

.section .text

/* rdi = a TrapFrame to continue. Does not return.

   The frame carries all fifteen registers and the five words `iretq` consumes,
   so entering a process for the first time and resuming one the timer
   interrupted are the same instruction sequence over the same layout. The first
   entry is just a frame the launcher wrote rather than one the processor
   pushed — which is why there is no separate "enter a new process" path here
   any more, and why a process cannot tell which of the two happened to it.

   RSP is pointed *at the frame* and the registers are popped out of it, which
   is what the timer stub does at its own end — the frame is read, never
   written, and nothing is pushed onto it. That is only true with interrupts
   masked: an interrupt taken between the two instructions below would push the
   processor's frame over the table this RSP is pointing into. `cli` here rather
   than at the caller because the window belongs to these instructions, and IF
   comes back from the frame's own RFLAGS at `iretq`. */
.global process_start
process_start:
    cli
    /* The stack the system-call edge re-enters the nucleus on: the scheduler's
       own stack, recorded at the last moment it is the live one. */
    mov qword ptr [rip + syscall_kernel_rsp], rsp
    mov rsp, rdi
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
