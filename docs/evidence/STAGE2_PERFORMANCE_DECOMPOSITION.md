<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Is the implementation good enough to argue about budgets?

Evidence level: **P1** (locally measured, docs/35).
Question asked: not "are the budgets right", but "is this implementation already
good enough that the budgets are the thing left to discuss". The attempt below
was to **refute** that, and it partly succeeded: three more general
inefficiencies were found and fixed after the two that were already known.

## Where the frontend's cost actually is

A total says the frontend is slow. It does not say which stage to look at, and
guessing produced a refuted hypothesis once already. So the stages are timed
separately, on the same 262 114-byte fixture (native medians, microseconds):

```text
              before      after
read          23 268        402      transport validity
parse          6 148      5 936      grammar
check         32 291     32 194      types, ownership, effects, resources
lower         24 499     24 528      tos-ir/v1
verify        53 572     50 921      independent verifier
                        --------
total        ~139 800   ~114 000
```

Two surprises, both worth stating because neither was where attention would
have gone by default:

- **the independent verifier is the largest stage**, larger than parsing and
  lowering together;
- **transport validation was the second largest**, and it was almost entirely
  waste.

## Three general inefficiencies found and fixed

None is a benchmark special case; each is wrong for every input.

**1. The engine cloned every instruction it executed.** `run_block` cloned the
`Instruction` out of the module before evaluating it, with a comment explaining
it avoided a stale borrow. The borrow does not exist: the module outlives the
engine, so a reference into it is not a borrow of `self`. Copying the reference
out makes that visible to the borrow checker and removes a struct copy from the
hottest loop an interpreter has.

```text
engine, native p95   333 743 us  ->  209 128 us   (1.6x)
```

**2. Transport validation normalized the whole source to compare it.**
`SourceReader::read` built a complete NFC copy of the source and compared it
byte for byte. Every ASCII scalar value is NFC-stable — none decomposes, none
has a nonzero combining class, none composes with what precedes it — so an
all-ASCII source unit *is* its own normal form. docs/39 restricts identifiers to
ASCII and admits Unicode only in string data and comments, so this is the
ordinary case, and the test that decides it is a scan with no allocation.

```text
read, native median   23 268 us  ->  402 us   (57x)
```

The conformance corpus's non-ASCII and non-NFC rejection vectors still produce
exactly their recorded codes; the fast path is taken only when it cannot change
an answer.

**3. The verifier formatted a location for every entry it checked.**
`alloc::format!("source map {index}")` and its siblings ran on every source-map
entry, every instruction and every function — building a string to describe a
place where, almost always, nothing was wrong. The locations are now built by a
closure the finding calls, so they cost nothing until a finding exists.

This one is small natively (verify 53.6 ms → 50.9 ms) and is kept anyway,
because the lesson from the previous round is that a per-item allocation costs
far more on the reference platform than a host profile suggests.

## What this did to the ratio — the result that matters

```text
                native p95     reference p95     ratio
engine, before    333 743 us     5 541 378 us     16.6x
engine, after     215 529 us     3 628 441 us     16.8x
```

**A 1.6x engine speedup left the ratio where it was.** That is the empirical
form of the structural argument: a budget written as `reference / native` of the
same implementation measures the *platform*, because an improvement moves the
numerator and the denominator together. It is now shown rather than reasoned.

The frontend behaves differently, as it must — its budget is absolute:

```text
frontend, before   149 048 us native   1 389 773 us reference   9.3x
frontend, after    124 129 us native   1 280 854 us reference  10.3x
budget                                   500 000 us
```

The optimisations moved the reference figure by 8%, and the budget is still
missed by 2.56x. The ratio is stable across every measurement taken (9.3x, 9.8x,
10.3x), which says the platform factor is uniform and no pathology remains — the
remaining cost is the work itself.

## The honest state

- **Engine.** Two rounds of general optimisation, one of them worth 1.6x, and
  the ratio did not move. The claim that this budget is a platform property
  rather than an implementation property now has evidence behind it.
- **Frontend.** Still 2.56x over. `check`, `lower` and `verify` are 114 ms of
  the 124 ms that remain, and none of them has been profiled *inside* yet. It is
  too early to say the budget is unreachable; what can be said is that reaching
  it needs roughly another 2.4x from those three stages.
- Sample counts here are diagnostic, not gate evidence: the frontend reference
  figures are single boots and the engine's are three. The full 21-sample pair
  in `docs/evidence/STAGE2_PERFORMANCE_PAIR_P1.md` is the record, and it must be
  re-taken when the frontend work settles.
