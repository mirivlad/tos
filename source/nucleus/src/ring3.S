# SPDX-License-Identifier: GPL-3.0-or-later
/* Reaching CPL 3, and the payloads that prove what happens there.

   Everything below `ring3_enter` is *test-only material*: it is assembled into
   the nucleus image only when a test feature selects this module, and it exists
   because the promises of SYSTEM_ABI_V1 section 3 are promises about a
   processor, and the only witness a processor accepts is itself. A Rust
   function calling another Rust function proves nothing about what `syscall`
   preserves.

   The payloads are position-independent by construction — registers, immediates
   and `syscall`, no absolute address and no data reference — because they are
   copied into a frame and executed at an address they were not linked for. */

.section .text

/* rdi = entry point, rsi = top of the user stack, rdx = code selector,
   rcx = data selector. The two selectors are passed in rather than written
   here so that the GDT stays the single place that decides them.
   Does not return: the payload it lands in ends by faulting. */
.global ring3_enter
ring3_enter:
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
    iretq

/* Fills every register SYSTEM_ABI_V1 section 3 says survives a call, makes two
   calls, and checks. Ends in `ud2` when everything held and in `hlt` when it
   did not — two faults a reader of the boot log cannot confuse, and `hlt` from
   CPL 3 is a privileged instruction, so the failure path also cannot succeed by
   accident. */
.global ring3_payload_start
ring3_payload_start:
    movabs rbx, 0x1111111111111111
    movabs rbp, 0x2222222222222222
    movabs r12, 0x3333333333333333
    movabs r13, 0x4444444444444444
    movabs r14, 0x5555555555555555
    movabs r15, 0x6666666666666666
    movabs rdi, 0x7777777777777777
    movabs rsi, 0x8888888888888888
    movabs r10, 0x9999999999999999
    movabs r8,  0xaaaaaaaaaaaaaaaa
    movabs r9,  0xbbbbbbbbbbbbbbbb

    movabs rax, 0x7fffffff  /* an operation number that is not assigned */
    syscall
    cmp rax, -7             /* E_NOT_SUPPORTED, and still runnable */
    jne 9f

    movabs rax, 0x1111111111111111
    cmp rbx, rax
    jne 9f
    movabs rax, 0x2222222222222222
    cmp rbp, rax
    jne 9f
    movabs rax, 0x3333333333333333
    cmp r12, rax
    jne 9f
    movabs rax, 0x4444444444444444
    cmp r13, rax
    jne 9f
    movabs rax, 0x5555555555555555
    cmp r14, rax
    jne 9f
    movabs rax, 0x6666666666666666
    cmp r15, rax
    jne 9f
    movabs rax, 0x7777777777777777
    cmp rdi, rax
    jne 9f
    movabs rax, 0x8888888888888888
    cmp rsi, rax
    jne 9f
    movabs rax, 0x9999999999999999
    cmp r10, rax
    jne 9f
    movabs rax, 0xaaaaaaaaaaaaaaaa
    cmp r8, rax
    jne 9f
    movabs rax, 0xbbbbbbbbbbbbbbbb
    cmp r9, rax
    jne 9f

    mov rax, 10             /* context_yield: self only, and legal */
    syscall
    cmp rax, 0
    jne 9f

    ud2                     /* every promise held */
9:
    hlt                     /* one did not */
.global ring3_payload_end
ring3_payload_end:

/* A process executing a privileged instruction. Nothing else: the whole
   payload is the instruction whose refusal is the evidence. */
.global ring3_privileged_start
ring3_privileged_start:
    hlt
.global ring3_privileged_end
ring3_privileged_end:

/* A process reaching for the nucleus's own memory. The address is the one the
   nucleus is linked at, which every boot log names, and the page there is
   present and supervisor-only — so what refuses this write is the mapping, not
   a check anyone wrote. */
.global ring3_nucleus_start
ring3_nucleus_start:
    movabs rax, 0x2000000
    mov qword ptr [rax], 1
    hlt
.global ring3_nucleus_end
ring3_nucleus_end:
