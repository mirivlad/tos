<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Finalist C — adapted restricted Rust

## Boundary evaluated

An adapted profile would need to make UTF-8 Rust-subset source canonical, ban
ambient `std` authority and ordinary `unsafe`, add TOS capability/resource
contracts, lower into typed TOS IR and run an independent TOS verifier. Rust
would therefore be a front-end semantic substrate, not the operating-system ABI
or a licence to expose rustc/LLVM/C/libc/host threads at runtime.

## Common evidence

`prototypes/adapted-rust/profile.rs` runs the same 13 cases and same worker
workload. It has a private verifier-issued capability, 64 task/worker bound,
cache identity, atomic Release/Acquire publication and scoped cancellation.
`invalid_capability.rs` fails with E0451; `invalid_mutable_share.rs` fails with
E0499, proving two required safe negative cases through the actual compiler.
The 2/4-worker raw records have `overlap=true` and the same common digest.

## Fit and decisive loss

Rust ownership, `Send`/`Sync`, scoped threads and atomics are technically
strong evidence, not convenience credit. But the required adaptation recreates
the TOS source semantics, capability model, quota policy, typed IR and verifier
boundary outside unchanged Rust. Rust's Reference calls its general memory
model incomplete; inheriting its C++20-style atomics without a separate TOS
contract leaves a material semantic dependency. A recovery implementation also
risks architectural capture by rustc/LLVM, Rust runtime conventions and C/host
ABI unless it contains a separate frontend/verifier—again duplicating the
foundation the candidate was meant to avoid. These are decisive TCB/recovery
and semantic-authority costs, not benchmark or ecosystem penalties.
