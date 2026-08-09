<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1.5 common prototype corpus

This directory is research evidence, not a TOS Core grammar, parser, runtime,
IR schema or standard library. It keeps finalists honest by making them process
the same semantic cases.

`cases.json` defines stable IDs and expected semantic outcomes. The illustrative
text in a case is deliberately not accepted `.tos` syntax. A candidate must
document how its prototype represents the case, rather than claim that this
corpus defines a future grammar.

`measure.py` records raw wall-clock samples. A measured command must emit both:

```text
digest=<stable-logical-result-id> overlap=<true|false>
```

`overlap=true` is only valid when the candidate independently records that two
CPU-bound partitions ran at the same time. Timing alone is insufficient.

Reproduce the corpus validation:

```sh
python3 docs/research/stage15/prototypes/common/measure.py --validate-only
python3 -m unittest docs/research/stage15/prototypes/common/test_measure.py
```
