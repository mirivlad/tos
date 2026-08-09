<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Accepted TOS Core V1 conformance corpus

This tree contains backend-neutral canonical `.tos` inputs for accepted TOS
Core V1. The inputs are GPL-3.0-or-later reusable implementation/conformance
sources; the corpus documentation is CC-BY-SA-4.0. They become mechanically
executed only after the accepted Stage 2 frontend/verifier exists. Until then,
their expected accepted/rejected status and stable primary diagnostics are
specified in `EXPECTATIONS.md`, not inferred from a host language.

`accept/` contains inputs a conforming V1 implementation must accept for the
declared profile. `reject/` contains sources whose named primary diagnostic is
part of the contract. The corpus deliberately includes semantic and
IR-generation expectations without treating current Stage 1.5 prototypes as
the grammar.
