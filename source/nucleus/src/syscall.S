# SPDX-License-Identifier: GPL-3.0-or-later
/* The one edge across the isolation boundary (SYSTEM_ABI_V1 section 3).

   `syscall` does not switch stacks, does not save registers and does not
   disable anything the FMASK register was not told to disable. Everything the
   contract promises about registers is promised by this stub, so it is written
   out in full rather than left to a compiler that has no reason to know the
   rule.

   On entry the processor has already put the user's RIP in RCX and the user's
   RFLAGS in R11, and left RSP pointing at the user's stack. */

.section .text
.global syscall_entry
syscall_entry:
    /* Step off the caller's stack before touching memory the caller could have
       aimed anywhere. Until this pair of moves the nucleus is running on a
       stack a process chose. */
    mov qword ptr [rip + syscall_user_rsp], rsp
    mov rsp, qword ptr [rip + syscall_kernel_rsp]

    /* What the architecture handed us, and what must go back to `sysretq`. */
    push rcx
    push r11

    /* The six arguments, in the order SYSTEM_ABI_V1 section 3 gives them, laid
       out as an array the dispatcher reads. They are pushed rather than moved
       because five of the six are caller-saved in the C ABI the dispatcher
       follows, and the contract says they come back unchanged. */
    push r9
    push r8
    push r10
    push rdx
    push rsi
    push rdi

    mov rdi, rax                /* the operation selector */
    mov rsi, rsp                /* the six arguments */
    call syscall_dispatch       /* -> status in RAX, value in RDX */

    pop rdi
    pop rsi
    add rsp, 8                  /* the saved RDX stays saved: RDX is a result */
    pop r10
    pop r8
    pop r9

    pop r11
    pop rcx
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
