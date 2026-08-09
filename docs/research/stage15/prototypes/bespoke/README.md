<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Bespoke TOS Core research model

`model.rs` is a **non-production** executable semantic model. It does not
accept `.tos` source, establish a grammar, define the final IR schema or provide
a Stage 2 runtime. Its only purpose is to make the bespoke candidate falsifiable
against the common Stage 1.5 corpus.

The model defines a small typed-IR vocabulary, a verifier-owned MMIO token with
a private constructor, fixed task/worker limits, explicit source-span retention,
and structured parallel work. It uses Rust host threads only as a research
backend for a model that represents its own task/resource contracts; Rust is
not selected by compiling this file.

The reference mode forces one worker. The parallel mode runs 64 fixed CPU-bound
partitions in scoped threads. It records `max_active` and the Linux
`/proc/thread-self/stat` processor field for CPU-bound workers; it reports
`overlap=true` only if at least two workers were concurrently active and
observed on at least two CPUs.

Reproduce the focused evidence:

```sh
python3 -m unittest docs/research/stage15/prototypes/bespoke/test_model.py
rustc --edition=2024 -O docs/research/stage15/prototypes/bespoke/model.rs \
  -o /tmp/tos-stage15-bespoke-model
/tmp/tos-stage15-bespoke-model --mode reference --workers 1
/tmp/tos-stage15-bespoke-model --mode parallel --workers 2
```

The Linux `/proc` CPU observation is measurement evidence, not a required TOS
runtime API. The selection report records this host-specific limitation.
