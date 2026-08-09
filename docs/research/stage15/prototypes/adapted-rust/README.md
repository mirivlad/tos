<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Adapted Rust research profile

This is a **non-production** Stage 1.5 experiment. It neither accepts `.tos`
source nor admits Rust, `std`, Cargo, an OS thread API or the host ABI into the
future TOS trusted runtime. It makes concrete the additional contracts an
adapted Rust foundation would have to supply.

The profile uses an opaque, verifier-issued MMIO capability; a scoped-thread
parallel backend; explicit 64-worker/64-task limits; cache identity bound to a
dependency digest; release/acquire publication; and structured cancellation.
It has no ambient filesystem, network or time operation in the modeled source
operations. The host experiment itself uses `std` and `/proc` only to gather
research evidence, which is one of the candidate's recovery/host-dependency
costs rather than a claim about a bootstrap profile.

Two invalid Rust inputs are retained as negative compiler evidence:

- `invalid_capability.rs` attempts to construct a capability with a private
  field and must be rejected (`E0451`);
- `invalid_mutable_share.rs` attempts competing safe mutable borrows across a
  scoped thread and must be rejected (`E0499`).

Reproduce the focused evidence:

```sh
python3 -m unittest docs/research/stage15/prototypes/adapted-rust/test_profile.py
rustc --edition=2024 -D warnings -O \
  docs/research/stage15/prototypes/adapted-rust/profile.rs \
  -o /tmp/tos-stage15-adapted-rust
/tmp/tos-stage15-adapted-rust --mode reference --workers 1
/tmp/tos-stage15-adapted-rust --mode parallel --workers 2
rustc --edition=2024 docs/research/stage15/prototypes/adapted-rust/invalid_capability.rs
rustc --edition=2024 docs/research/stage15/prototypes/adapted-rust/invalid_mutable_share.rs
```

The last two commands must fail. Rust's own Reference currently says its general
memory model is incomplete; this experiment uses documented atomics but cannot
silently turn that upstream limitation into a finished TOS memory specification.
