<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1.5 language-foundation decision report

1. Stage 1 closed: `9687d8acdef104f02536b7f7881ce4b77a1144d3`.
2. Stage 1.5 baseline: `345fa8c10a3da0715a3c24eb37327ff3277bedc7`.
3. Requirements update: `add6358b9372a5d45b329eedc84ec4bab7cdcabd`.
4–6. A bespoke Core, B TOS-over-Wasm formal core, C adapted Rust and D unchanged Rust/Pony/Go were considered; B and D screening rejects are exact in `screening.md`.
7–9. Finalists are bespoke and adapted Rust; their common 13-case prototypes, stable diagnostics, typed driver state machine, capability denial, fuel, maps, cache identity and two modes are retained under `prototypes/`.
10–13. Raw 3+21 1/2/4-worker samples, measured source/binary/dependency/cold-start evidence, recovery/TCB component inventories and explicit N/A production metrics are in `measurements/TCB_AND_RECOVERY.md`; bootstrap/recovery evidence is bounded serialized mode, not a final runtime claim.
12–13. Future TCB comparison is in the finalist reports: bespoke owns parser/checker/verifier/interpreter/runtime; Rust requires that layer plus containment of its host toolchain/runtime.
14–18. Both retain canonical text, typed IR verifier boundary, ownership/atomic model, unforgeable capabilities and driver operations; exact mappings are in the finalist reports. Class B is rejected because it leaves these semantics TOS-owned, not merely for host thread creation.
19–24. Both prototypes preserve source maps/cache identity and show SMP viability, but only bespoke makes the whole semantic/host ABI boundary TOS-owned without an adapter becoming the real language.
25. Proposed winner: bespoke TOS Core.
26. It passes because canonical source, semantics, IR, verifier, bootstrap and full runtime relationship are one explicit TOS contract.
27–28. Runner-up: adapted Rust; it loses on semantic authority, recovery/TCB and host ABI containment, not speed.
29. Winner risk: ownership/concurrency/diagnostics/resource implementation is real Stage 2 work, explicitly not deferred from the contract.
30. Strongest reject: Wasm Threads has host-created threads; Pony is actor-only; unchanged Rust/Go lack the required boundary.
31–32. ADR-0027 contains the exact proposed docs/05/docs/16 reconciliation: Stage 1.5 fixes the semantic boundary; Stage 2 writes the complete specification. No source migration exists yet.
33. Exact Proposed ADR: `docs/adr/0027-language-foundation-selection.md`.
34. Research commits: `aee1786`, `3e4294b`, `9bd210b`, `f20b200`, `9dc84d7`.
35. Commands: each prototype README plus `python3 -m unittest` and `measure.py` commands.
36. Gate status: decision evidence complete; selection ADR remains Proposed.
37. Residual risk: implementation complexity and future cross-engine conformance.
38. After acceptance, Stage 2 first implements normative semantics, bounded frontend, typed IR verifier and reference interpreter.
39. **Stage 2 production implementation has NOT begun.**
40. **PROJECT ARCHITECT: ACCEPT / REJECT**
