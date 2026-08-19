# SPDX-License-Identifier: GPL-3.0-or-later
/* The one edge across the isolation boundary (SYSTEM_ABI_V1 section 3).

   `syscall` does not switch stacks, does not save registers and does not
   disable anything the FMASK register was not told to disable. Everything the
   contract promises about registers is promised by this stub, so it is written
   out in full rather than left to a compiler that has no reason to know the
   rule.

   On entry the processor has already put the user's RIP in RCX and the user's
   RFLAGS in R11, and left RSP pointing at the user's stack.

   **The stub builds a `TrapFrame`, the same one the timer stub builds.** Until
   blocking existed it saved only the six arguments, because the only thing that
   ever happened next was a return through `sysretq`. A call that blocks does not
   return next: it is set down and picked up later, possibly after other
   processes have run, and what picks it up is the scheduler — which knows how to
   enter a context and nothing about how that context came to be one. So the two
   ways into the nucleus now leave the same thing behind, and a suspended system
   call is indistinguishable from a preempted process, which is what makes one
   mechanism able to resume either.

   The five words the processor pushes on an interrupt are synthesized here: RIP
   and RFLAGS are what the architecture already put in RCX and R11, and RSP is
   the stack the caller was on. The two selectors are left zero and filled in by
   the nucleus, which is where their values are declared — the alternative is a
   second copy of the GDT's layout in assembly, drifting. */

.section .text
.global syscall_entry
syscall_entry:
    /* Step off the caller's stack before touching memory the caller could have
       aimed anywhere. Until this pair of moves the nucleus is running on a
       stack a process chose. */
    mov qword ptr [rip + syscall_user_rsp], rsp
    mov rsp, qword ptr [rip + syscall_kernel_rsp]

    /* The five words an interrupt would have pushed, in that order. */
    push 0                                      /* SS, filled by the nucleus */
    push qword ptr [rip + syscall_user_rsp]     /* RSP */
    push r11                                    /* RFLAGS, per the architecture */
    push 0                                      /* CS, filled by the nucleus */
    push rcx                                    /* RIP, per the architecture */

    /* All fifteen registers, in `TrapFrame` order. Five of the six argument
       registers are caller-saved in the C ABI the dispatcher follows and the
       contract says they come back unchanged, so saving them is not an
       optimization; saving the other nine is what makes the frame resumable. */
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

    mov rdi, rax                /* the operation selector */
    mov rsi, rsp                /* the frame, which is also the arguments */
    call syscall_dispatch       /* writes the answer into the frame */

    /* RCX and R11 come back as the architecture left them — the return address
       and the caller's flags — because that is what the process had in them
       when it made the call. */
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
    mov rsp, qword ptr [rip + syscall_user_rsp]
    sysretq

.section .data
.align 8
.global syscall_kernel_rsp
syscall_kernel_rsp:
    .quad 0
syscall_user_rsp:
    .quad 0

/* Leave `.text` current. Assembly blocks are concatenated into one translation
   unit, so a file that ends in another section decides where the *next* file's
   instructions land — and `.data` is mapped no-execute, which turns that into a
   fault at the first instruction of a stub written years apart from here. Every
   file below states its own section too; this is the second lock on the same
   door. */
.section .text
