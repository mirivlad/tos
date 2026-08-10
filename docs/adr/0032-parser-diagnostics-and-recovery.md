<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0032: TOS Core V1 parser diagnostics and recovery clarification

- Status: Accepted (Project Architect-approved)
- Date: 2026-08-10
- Decision level: 3 — clarifies the accepted TOS Core V1 contract by resolving a
  normative conflict, allocating stable diagnostic codes that conformance
  evidence will depend on, and amending a recovery rule
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-10

## Context

The first production parser exposed three gaps in the accepted TOS Core V1
contract. Each one was blocking in the same way: an implementation had to choose
behavior that conformance evidence would later be measured against, with no
normative text to choose from.

**1. The diagnostic registry does not exist.**
`docs/41` section 7 requires every diagnostic to carry a stable symbolic code and
states that "a full registry and conformance expectations are in docs/44".
`docs/44` contains conformance expectations but no registry. `docs/39` allocates
exactly two parser codes — `E1105_CONTROL_HEAD_PARENS_REQUIRED` and
`E1106_LIST_SEPARATOR_REQUIRED` — and describes the other syntax rejects
(R018, R020–R024, R027, R028) only as "parse error". This is a conflict between
two Tier 2 documents under `docs/38`, not a missing detail: `docs/41` asserts the
existence of authority that `docs/44` does not carry.

**2. Declaration-level recovery discards the rest of the source unit.**
`docs/39` section 4 ends a declaration synchronization region at "the next
top-level `;` or `]`". Neither terminates a `fn` declaration, whose body is a
brace block. Following the rule literally, one malformed function signature
causes every later declaration in the file to be skipped, so the parser reports
one diagnostic where it could report several and produces an emptier tree than
the source supports.

**3. A character that begins no lexical form has no code.**
`docs/39` section 2 allocates `E1012_INVALID_IDENTIFIER` for a non-ASCII scalar
value used where an identifier is formed. A valid UTF-8 ASCII character that
begins no lexical form at all — `@`, `$`, `#`, `` ` ``, `'`, `\` outside a
literal or comment — has no allocated code. Reporting it as
`E1012_INVALID_IDENTIFIER` would overload an identifier-specific code with an
unrelated condition and make the two indistinguishable to conformance tooling.

## Decision

### 1. `docs/44` becomes the authoritative diagnostic code registry

`docs/44` gains a registry section listing, for every frontend diagnostic code
reachable by the source reader, lexer and parser: the stable symbolic code, its
stage, and the exact condition that raises it. `docs/41` section 7's reference to
docs/44 is now satisfied rather than aspirational.

Codes remain allocated by the document that owns the rule. `docs/39` continues to
define lexical and grammatical conditions; the registry records them in one
enumerable place and adds the parser codes that had no home.

### 2. Parser codes E1100–E1104 and E1107 are ratified

The following codes are allocated. `E1105` and `E1106` keep their existing
numbers and meanings and are not renumbered.

| Code | Condition |
|---|---|
| `E1100_EXPECTED_MODULE_HEADER` | a required module-header keyword (`module`, `version`) is absent at its position |
| `E1101_EXPECTED_IDENTIFIER` | an identifier is required at this position and the token present is not one |
| `E1102_EXPECTED_VERSION_COMPONENT` | a module-header version component is not a decimal integer representable as `u32` |
| `E1103_EXPECTED_PROFILE` | the module-header profile is neither `bootstrap` nor `full` |
| `E1104_EXPECTED_LITERAL` | a literal is required at this position and the token present is not one |
| `E1107_UNEXPECTED_TOKEN` | the token cannot begin or continue the construct being parsed and no more specific code applies |

Each has one unambiguous meaning and none overlaps an existing code.
`E1101_EXPECTED_IDENTIFIER` is syntactic — a well-formed token of the wrong class
where an identifier is required — and is distinct from the lexical
`E1012_INVALID_IDENTIFIER`, which fires when bytes cannot form an identifier at
all. `E1104_EXPECTED_LITERAL` is likewise distinct from the lexical
`E1020_INVALID_INTEGER_LITERAL`.

`E1107_UNEXPECTED_TOKEN` is the defined residual of the parse stage. It is
correct only where no other parser code applies; a more specific code always
wins. It is not a licence to leave conditions unclassified: a recurring
`E1107` case that has a distinct meaning is a reason to allocate a code, not to
keep using the residual.

### 3. Declaration recovery may end at a closing brace

`docs/39` section 4 is amended: declaration-level recovery ends a synchronization
region at the next top-level `;` or `]`, **or** at the `}` that closes a
top-level declaration body and returns delimiter nesting to zero.

The purpose is bounded: one error in a declaration or signature must not cost the
remainder of the source unit merely because a function declaration ends with a
block rather than `;` or `]`. The additional boundary never skips past a
boundary the original rule names — it can only end a region earlier.

No further recovery heuristic is admitted. The parser still must not guess a
missing declaration, capability, type or operator, and still emits exactly one
diagnostic per synchronization region.

### 4. `E1013_UNEXPECTED_CHARACTER` is allocated

`E1013_UNEXPECTED_CHARACTER` applies to a valid UTF-8 source character outside a
literal or comment that, at its position, neither begins nor continues any
admissible lexical form.

Precedence against `E1012_INVALID_IDENTIFIER` is fixed and mechanical: a
non-ASCII scalar value outside a literal or comment is
`E1012_INVALID_IDENTIFIER`, because identifiers are the only construct that
non-ASCII text could be attempting to form and `docs/39` section 2 already
assigns that condition. Every other such character — necessarily ASCII — is
`E1013_UNEXPECTED_CHARACTER`. Both report the first byte of the offending
character.

`E1012_INVALID_IDENTIFIER` therefore remains exactly what its contract says: an
identifier-related violation.

### 5. Registry drift is mechanically prevented

`scripts/check-stage2-language-contract.py` gains checks that fail when:

- a diagnostic code cited by `docs/language/conformance/v1/EXPECTATIONS.md` is
  absent from the `docs/44` registry;
- a registry entry lacks a stage or condition;
- an `E10xx`/`E11xx` code named in `docs/39` is absent from the registry;
- a conformance expectation still says "parse error" instead of a code.

`EXPECTATIONS.md` replaces every remaining "parse error" cell with the exact
expected stable code.

## Architecture impact statement

- **Change level:** 3.
- **Invariants affected:** none amended. I-09 is served — diagnostic codes are a
  versioned boundary and now have a single enumerable definition. I-15 is served
  by making "parse error" a precise claim instead of a category.
- **Canonical representation after the change:** unchanged.
- **Trusted-base impact:** none. No dependency enters the loader or nucleus.
- **Source-to-runtime impact:** diagnostics gain stable identity, so a rejected
  source unit can be tied to an exact documented condition rather than to
  implementation wording.
- **Recovery and rollback impact:** none at the system level. Parser recovery in
  section 3 concerns source-unit diagnostics only.
- **Stage identity gate:** no stage gate is claimed or closed. Stage 2 Part B
  remains in progress and Stage 3 remains unauthorized.
- **Threat-model impact:** none. Recovery still terminates: every
  synchronization step consumes at least one token or reaches end of source, so
  hostile input cannot induce non-termination. Bounded parsing under S2 is
  unchanged.
- **Performance contract:** none applicable.
- **Compatibility profile:** TOS Core 1.0. Ratifying codes fixes them for V1; a
  later code change is a versioned language decision.
- **New dependencies:** none.
- **Licence and patent impact:** none.
- **Tests that enforce the decision:** parser unit tests asserting each ratified
  code and all three synchronization regions; a conformance-negative test in
  which a valid top-level declaration follows a damaged function and is still
  parsed; lexical vectors for `@` and `$` fixing span and precedence; and the
  mechanical registry/expectations gate in section 5.

## Consequences

Conformance evidence can name an exact code for every rejected source, and the
implementation stops carrying provisional semantics. The residual `E1107` keeps
the registry honest about what is not yet classified rather than hiding it.

The cost is that six parser codes are now fixed for TOS Core 1.0 and can only be
changed through a versioned language decision. That is the intended trade: codes
that conformance depends on must not drift.

## Alternatives considered

**Leave the codes provisional until more of the parser exists.** Rejected: the
conformance corpus already exists and would have to assert something. Provisional
codes in accepted expectations are the drift this project's documentation
hierarchy is built to prevent.

**Put the registry in `docs/39`.** Rejected: `docs/41` already names `docs/44` as
its location, and `docs/39` owns syntax rather than diagnostics across all
stages. Moving the reference instead of satisfying it would leave `docs/41`
inaccurate.

**Report `@` and `$` as `E1012_INVALID_IDENTIFIER`.** Rejected: it makes an
identifier diagnostic mean two unrelated things and prevents tooling from
distinguishing a Unicode identifier attempt from a stray symbol.

**Extend declaration recovery with further heuristics**, such as resuming at any
token that could begin a declaration. Rejected as unbounded guessing: it would
let the parser invent a declaration boundary the grammar does not define.
