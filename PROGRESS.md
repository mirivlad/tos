<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS — журнал прогресса (рабочий лог)

Рабочий файл владельца для фиксации «что сделано и проверено».
**Ненормативный.** Не заменяет CHANGELOG.md, ADR, отчёты по этапам и
docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md — это рабочий лог, а не документация.

Правила ведения:
- статус пункта меняется только вместе с записью в «Журнал верификации»
  (команда + результат);
- «сделано» = есть реальный прогон, не описание.

## Текущая позиция

- Базовая линия: TOS v0.2.1. Исторический bootstrap-коммит `c5b818c` остаётся
  началом Stage 1; актуальное closure evidence зафиксировано отдельными
  commit-addressed records в `source/legal/`.
- Этап: Stage 0 завершён. **Stage 1 формально закрыт**: capsule v1, Boot ABI
  v1, UEFI loader, nucleus, source identity, fail-closed evidence, P2
  performance evidence и Project Architect approval заархивированы. **Stage
  1.5 формально закрыт**: ADR-0027 accepted, evidence/TCB/recovery analysis и
  Project Architect approval заархивированы. **Stage 2 формально закрыт**
  (2026-08-12, candidate `e38785cb828dea67c86ecb0bc0873a607d5d3bca`): canonical
  TOS Core V1 source исполняется через production reader/parser/checker,
  детерминированный `tos-ir/v1` lowerer, независимый verifier и bounded
  reference engine на реальном freestanding boot path; approval заархивирован в
  `source/legal/publication-records/`. Acknowledged и не являются blocker'ами:
  evidence level P1, differential testing `N/A` при одном движке, Proposed
  ADR-0044. V1 surface contract фиксирует `[]` для data/declaration lists, `{}`
  только для executable blocks, `()` для arguments/grouping и explicit `return`
  без implicit tail values. **Stage 3 production implementation не начат и не
  авторизован.**
- **Stage 2 remains CLOSED.** Post-Stage-2 interstage work: **Human Boot
  Observability / Boot Console** — human-facing журнал реальной загрузки поверх
  уже существующих boot/runtime событий. Не стадия, не новый архитектурный
  контракт, не Stage 3: boot console ненормативен, serial `TOS.*` / `TOS.RUN.*`
  остаются нормативным каналом, framebuffer — best-effort и никогда не влияет на
  boot outcome. Финальный экран означает успешное завершение Stage 2 runtime и
  штатный halt, а не готовую интерактивную систему.
- Вся работа ведётся в `source/` (решение owner; docs/17-монобренч на корень
  приостановлен до Stage 1 — scope-решение, не изменение контрактов).

## Checklist Stage 1

| # | Пункт | Статус | Доказательство |
|---|-------|--------|----------------|
| 1 | Дерево `source/` по docs/17 | done | каталоги crates/, boot/, nucleus/, host-tools/, tests/, system/, interfaces/ |
| 2 | Спека CAPSULE_FORMAT_V1.md | done | source/interfaces/boot/CAPSULE_FORMAT_V1.md |
| 3 | Спека BOOT_ABI_V1.md | done | source/interfaces/boot/BOOT_ABI_V1.md (§6 serial boot-event log) |
| 4 | crates/tos-hash (SHA-256 no_std) | done | SHA KAT/streaming, 7/7 host-тестов |
| 5 | crates/boot-protocol (BootInfo v1) | done | 24 host-теста: 224-B layout, map/containment, framebuffer tuple/geometry |
| 6 | crates/capsule: parser (no_std, total) | done | 24 host-теста: flags/reserved, canonical tables, SHA-1 padding, detached identity, limits/precedence |
| 7 | crates/capsule: host-билдер (feature="host") | done | детерминизм: builder == golden vector |
| 8 | crates/tos-serial (16550 COM1, no_std) | done | используется loader + nucleus |
| 9 | boot/uefi-loader (EFI app, рукописные биндинги) | done | PE32+ EFI app x86_64 собран, 0 warnings; release 14848 B |
| 10 | nucleus (freestanding, boot ABI v1, serial, halt-код) | done | raw-binary собран (entry первой, `sub rsp`); release 10520 B |
| 11 | system/boot/init.tos + NOTICES.txt | done | source/system/boot/ |
| 12 | host-tools/capsule (CLI-билдер) | done | регенерация векторов через него |
| 13 | Capsule-v1 fixtures/provenance | done | 14 `.bin`: 1 accept + 13 declared fail-closed; each binary checked by ADR-0019 provenance |
| 14 | tests/integration | done | 19 host integration tests: vectors, tamper, deterministic detached identity, framebuffer renderer |
| 15 | tests/fuzz (детерминированный мутационный) | done | `FUZZ PASS rounds=200000` in full preflight/Source CI |
| 16 | Сборка всех таргетов (host + UEFI + none) | done | format, host tests/clippy; UEFI loader PE32+; freestanding nucleus raw binary |
| 17 | host-tools/qemu-test (ESP-образ + OVMF) | done | run.sh: identity gate `--git-commit HEAD`, manifest repo-relative |
| 18 | QEMU-прогоны: success + corruption + exceptions | done | normal **33**; 13 capsule negatives/mismatch **67**; #UD/#GP **73** (`TOS.EXCEPTION`) |
| 19 | SPDX/DCO + provenance | done | SPDX checks every tracked vector provenance; every reachable commit carries a DCO sign-off |
| 20 | Архитектур-импакт-стейтмент (AGENTS.md §5, Level 2) | done | source/ARCHITECTURE_IMPACT_STATEMENT.md, коммит dc16726 |
| 21 | Immutable Stage 1 report + identity record | done | `source/legal/release-manifests/f220603…-stage1-report.md` (G0/R0, P2 artifact, actual scope) |
| 22 | Formal closure + DCO | done | immutable Project Architect approval record; current reachable history passes DCO |
| 23 | ADR-0026 P2 performance conformance | done | CI raw native/TCG full+crypto 3+21 series; 101,203,198 B / 2,007 hashes; TCG ratio ≤ 1.30 |

## Журнал верификации (append-only)

### 2026-08-06 — host-тесты, фаззинг, векторы

- `cargo test` (workspace, host): boot-protocol 6/6, capsule lib 10/10,
  tos-hash 7/7, integration 8/8. 0 failed.
- `cargo run --release -p tos-tests-fuzz 300000` → `FUZZ PASS rounds=300000`.
- Векторы перегенерированы: 7 .bin в tests/vectors/capsule-v1/;
  valid-001.bin sha256 = `0a4a0d8f7f3c738b866f1cec22ec58a25b43602f533ba6345d348d5ac06a30c2`.
  Ожидаемые отклонения парсера на invalid-векторах совпали
  (MissingBootCanonical, TraversalInPath, DuplicatePath).
- perf-smoke (debug): 1000 файлов / 16 МиБ распарсено за 2.87 s.
  Контракт 250 ms p95 меряется на release/QEMU, не на debug-сборке.
- Найден и исправлен баг в tos-hash: полноблочный цикл `update` сжимал буфер
  без копирования данных (`copy_from_slice` отсутствовал) — падали
  streaming_matches_single_shot и rfc4231_million_a; после фикса RFC 4231
  полностью зелёный, векторы перегенерированы.

### 2026-08-06 — коммиты Stage 1 (DCO)

- Рабочее дерево `source/` + PROGRESS.md закоммичены поверх c5b818c:
  `8435698` спеки, `42f52bc` crates, `2edc9b7` boot stack+tests,
  `1bc8c16` журнал, `3226077` workspace config, `dc16726` impact statement.
- Подпись DCO: `mirivlad <mirvtop@yandex.ru>`. Рабочее дерево чистое.

### 2026-08-06 — сборка целевых таргетов

- `rustup target add x86_64-unknown-uefi x86_64-unknown-none` (для тулчейна
  1.97.1 из source/; повтор из корня не помог — клал в stable).
- Loader `--target x86_64-unknown-uefi`: полная отладка вызовов через
  `(*ptr).field` (C-стиль `ptr->field` не компилится), `in("dx")`/`in("al")`
  вместо `mov dx,{port}` (sub-register). PE32+ EFI x86-64, 0 warnings.
- Nucleus `--target x86_64-unknown-none`: добавлен `build.rs` (`-T linker.ld`,
  `--oformat=binary`), `#[link_section=".text.boot_entry"]`; raw-образ, точка
  входа первой (`sub rsp` пролог). release 10520 B.
- `source/.cargo/config.toml`: `relocation-model=static` для x86_64-unknown-none.

### 2026-08-06 — ABI-корректность: флаги, reserved, биекция, identity gate

- Капсула v1 ужесточена по CAPSULE_FORMAT_V1.md §4/§9: раздельные наборы флагов
  для path (bit0) и file (bits 0–1), 12 Б reserved в file entry обязаны быть
  нулями, path table — биекция на `[0, file_count)` (DuplicateFileIndex /
  UnreferencedFile), boot-canonical cross-check (path→file), licence tail —
  точный хвост капсулы (offset+length == EOF, UTF-8).
- Capsule lib: 11/11 тестов; integration 8/8 (golden из реального
  `source/system/boot/init.tos` + NOTICES как licence, determinism,
  tamper/truncation по всем байтам).
- Векторы: valid-001 пересобран из реального init.tos + NOTICES; добавлены
  6 invalid-векторов (file-reserved, path-flag, dup-file-index,
  unreferenced-file, bootcanon-mismatch, licence-tail). Итог: 13 .bin.
- Identity gate (пункт 9): `tos-capsule-tool --git-commit HEAD` резолвит
  commit oid, проверяет `cat-file -e`, верифицирует каждый src-файл по
  `git cat-file blob HEAD:path` и пишет meta JSON
  (commit, sha256(commit-oid), repo_path+content_sha256, capsule_sha256).
  Проверено: tampered init.tos → отказ (exit 2); deterministic rebuild дал
  тот же capsule_sha256.
- `qemu-test/run.sh`: капсула по умолчанию строится с `--git-commit HEAD`
  и repo-relative manifest (identity gate в хватке), invalid-векторы можно
  подкладывать вторым аргументом.

### 2026-08-06 — QEMU: полный успех + negative-сценарии

- **Первый полный QEMU-прогон Stage 1** (OVMF 4M, q35, 256 MiB, TCG):
  `bash host-tools/qemu-test/run.sh /tmp/qemu-success7` → **exit 33 (HALT_OK)**.
  Serial boot-event log целиком:
  `TOS.BOOT.ENTRY → TOS boot loader → TOS.CAPSULE.OK files=1 → TOS.BOOT.HANDOFF →
  TOS.NUCLEUS.ENTRY → TOS.CAPSULE.OK files=1 → TOS.BOOTTEXT.PATH /system/boot/init.tos →
  TOS.BOOTTEXT.LINE → TOS.BOOTTEXT.DIGEST a3c82b57… → TOS.IDENTITY source_kind=git
  source_digest=f59a14d5… capsule_digest=5c839516… → TOS.HALT ok=0x10`.
  Identity gate подтверждён на реальном железе: `source_kind=git`,
  `source_digest` = raw OID коммита f59a14d (не SHA-256-обёртка).
- **9 negative-сценариев** (invalid-векторы из tests/vectors/capsule-v1/ как
  CAPSULE_FILE): каждый → **exit 67 (CAPSULE_INVALID)** с точным
  `TOS.BOOT.FAILC capsule_err=…`: UnsupportedIdentityKind, TotalLengthMismatch,
  MissingBootCanonical, BootCanonicalFlagMismatch, LicenceTailMismatch,
  TraversalInPath, DuplicatePath, UnreferencedFile, BadPathFlags. Фейлы —
  в loader'е до handoff, nucleus не достигается.
- **Найденные и устранённые баги** (подробно в STAGE1_REPORT.md §5):
  1. handoff inline-asm: LLVM клал entry в RDI, `mov rdi,{bi}` затирал его →
     call на BootInfo вместо nucleus; исправлено фиксированными регистрами
     (`in("rdi")` bi, `in("rax")` entry, `call *%rax`);
  2. nucleus при PIC линковался PIE-подобно: LLD оставлял .got с
     R_X86_64_RELATIVE, не применявшимися в `--oformat=binary` → #GP RIP=0;
     фикс: `relocation-model=static` + фиксированный адрес NUCLEUS_BASE +
     `-no-pie` в nucleus/build.rs + явные `.got/.got.plt` в linker.ld.

### 2026-08-06 — identity raw OID (ADR-0016)

- Решение: capsule identity несёт **raw commit OID** (kind=git, alg=1/SHA-1,
  len=0x14), а не SHA-256(oid). Оформлено как ADR-0016
  (docs/adr/0016-capsule-git-raw-oid-identity.md).
- Проверено e2e: `/tmp/tos_git.bin` на offset 96 = `01 01 14 00 f5 9a 14 d5…`
  (kind=1, alg=1, len=0x14, raw OID f59a14d…); QEMU `TOS.IDENTITY` печатает
  тот же raw OID.

### 2026-08-09 — Stage 1 closure

- `./scripts/preflight.sh --full` на archive-record baseline → **30/30 PASS**.
- GitHub Actions evidence: Documentation integrity, Provenance, Source CI и
  QEMU/P2 прошли на `b84dbb9`.  P2 artifact и exact source evidence описаны в
  immutable Stage 1 report; final approval находится в publication record.

### 2026-08-09 — Stage 2 Part A: proposed semantic/IR contract

- Подготовлен единый Proposed TOS Core V1 contract (docs/39–44) и Proposed
  ADR-0028: нормализованный `.tos` source model/grammar, nominal type/effect,
  affine ownership/regions, structured async/parallelism, TOS-owned atomics,
  resources, modules/capabilities, typed IR и independent verifier. Это не
  production implementation и ожидает единственный Project Architect
  checkpoint.
- Добавлены non-normative programmer guide/tutorial, 23 canonical GPL `.tos`
  examples/conformance inputs и status matrix; каждый пример явно marked
  proposed/not implemented.
- `python3 tools/build-specification.py --check` → PASS;
  `python3 tools/build-release-manifest.py --check` → PASS (211 files);
  `sh scripts/check-spdx.sh` → PASS (279 classified, 13 exempt);
  `./scripts/preflight.sh --full` → **30/30 PASS**.

### 2026-08-09 — Stage 2 Part A: checkpoint-correction resubmission

- Локальное RED evidence: новый `scripts/check-stage2-language-contract.py`
  обнаружил отсутствующие tuple/slice grammar forms, незафиксированную
  границу control-head/record-init, `nil`, противоречивую lifecycle task и
  отсутствие required vectors.
- ADR-0028 и docs/39–44 синхронизированы без изменения foundation: tuple и
  `slice<T>` выражены в EBNF; `Semaphore` и прочие fixed-arity runtime types
  больше не принимают type arguments; control heads parenthesized; record
  fields comma-separated; `nil` не является V1 value; `cancel` остаётся
  request, а `join`/`await` consume `Task<T>` into `TaskResult<T>`.
- Добавлены C006–C008 и R009–R015, canonical examples/guide/tutorial/status
  matrix обновлены; новый mechanical cross-contract gate включён в preflight.
- Финальная верификация для этого состояния: `./scripts/preflight.sh --full`
  → **31/31 PASS**; Stage 2 production implementation по-прежнему не начат.

### 2026-08-09 — Stage 2 Part A: second narrow contract resubmission

- Не переоткрывая закрытые находки предыдущего review, contract получает один
  value model для `if`/`match`, единый Call form для функций/constructors,
  выражаемый `to_*` checked-conversion contract и automatic structural Copy
  rule для V1 aggregates.
- Добавлены C009–C012 и R016–R017; mechanical gate проверяет эти формы,
  canonical tail examples и отсутствие competing `enum_init` parse.
- ADR-0028 остаётся Proposed; Part B production implementation и Stage 3 не
  начаты. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-09 — Stage 2 Part A: final syntax simplification (pending verification)

- Проектный surface contract ADR-0028 синхронизирован с readability decision:
  `[]` описывает lists/declarations, `{}` выполняет statements, `()` содержит
  parameters/arguments/grouping, list members используют `,`, simple actions
  завершаются `;`, а normal value требует explicit `return`.
- `if`/`match` вновь statement-only; match branches — executable blocks без
  comma separator. Record construction — `Point(x: ..., y: ...)`; parser всё
  ещё строит единый Call/Construct family без semantic backtracking. User
  records/enums affine; только primitive roots, tuples и arrays имеют stated
  Copy rule.
- Обновлены proposed guide/tutorial/examples/conformance и expanded mechanical
  gate. `./scripts/preflight.sh --full` → **31/31 PASS**; production
  implementation и Stage 3 не начаты.

### 2026-08-09 — Stage 2 Part A: final contract-consistency correction (pending verification)

- ADR-0028 остаётся Proposed. Убрано последнее противоречие про executable
  blocks: plain `{ ... }` не expression. Closure использует `fn (...) { ... }`,
  array type — `array<T, N>`, а named-field enum variants constructible через
  тот же named Call/Construct form, что records.
- Return scope теперь явно определяется для function/closure/spawn body;
  ordinary nested blocks его не создают. Добавлены conformance vectors и RED→GREEN
  expectations в mechanical gate. `./scripts/preflight.sh --full` → **31/31
  PASS**; Part B и Stage 3 не начаты.

### 2026-08-09 — Stage 2 Part A accepted / Part B authorized (pending verification)

- Project Architect accepted ADR-0028 at reviewed baseline `327fe5f…`.
  Docs/39–44 становятся accepted Tier 2 contract; guide/tutorial сохраняют
  отдельный implementation status. `?` редакционно reconciled к nearest
  return scope for function/closure/spawn body.
- После документальных gates начинается Stage 2 Part B production reference
  implementation. `./scripts/preflight.sh --full` → **31/31 PASS**; Stage 3
  по-прежнему не авторизован.

### 2026-08-09 — Stage 2 Unicode normalization contract clarification

- Project Architect accepted ADR-0029: TOS Core 1.0 canonical source is NFC
  specifically under Unicode/UCD 17.0.0 and UAX #15 Revision 57. This fixes
  source-identity, cache, IR and verifier behavior independently from host
  Unicode/locale tables; identifiers remain ASCII-only.
- The first frontend must retain reproducible UCD-data provenance, hashes and
  generator identity and pass the Unicode conformance cases before claiming
  lexer completion. This clarification admits no runtime Unicode dependency;
  Stage 3 remains unauthorized.

### 2026-08-09 — Stage 2 Part B source reader and lexer

- The production `tos-core` crate now has a bounded SourceReader and lexer.
  The source reader enforces the 256 KiB input ceiling, UTF-8/BOM/CRLF/NUL
  precedence and UCD 17.0.0 NFC with locally generated tables; the lexer
  produces bounded spanned tokens and documents lexical error offsets for
  whitespace, identifier, integer, string and byte-literal input. Parser,
  checker, IR, verifier and interpreter remain unimplemented. Stage 3 remains
  unauthorized.

### 2026-08-10 — Accepted vendor-material boundary and `/vendor`

- Project Architect accepted ADR-0030: external CPU microcode, GPU/Wi-Fi/device
  firmware and comparable vendor-produced bytes are vendor-controlled opaque
  material. They live in the new root namespace `/vendor`, are never canonical
  TOS source, and MUST NOT replace a component the architecture requires to be
  textual. `/system` declares a requirement by vendor/identity/version/hash/
  placement/policy; the opaque bytes stay in `/vendor`. The owner MUST be able
  to see where the boundary runs.
- I-01 is not amended and no Level 4 identity amendment is required: vendor
  opaque material is not a TOS executable component and does not become
  canonical source. The ADR states that scope explicitly instead of leaving it
  to be inferred.
- Tier 2 synchronized: docs/03 (namespace list, trust zones), docs/09
  (`/vendor` class), docs/11 (a driver stays textual even when it loads vendor
  firmware), docs/17, docs/20, docs/27 (opaque material is not third-party
  textual source), docs/34 (trust boundary 13, subsystem threats, non-goal).
- No implementation is authorized by this decision; `/vendor` is required only
  from the stage that first needs physical-hardware firmware.

### 2026-08-10 — Accepted runtime system source hierarchy

- Project Architect accepted ADR-0031 and docs/45 as Tier 2: classification of
  every runtime path (canonical source / source overlay / configuration /
  mutable state / derived cache / ephemeral / capability namespace / external
  material), the direct unrenamed mapping from repository `source/system/` to
  runtime `/system`, thirteen entries inside `/system`, the `/work`↔`/system`
  relationship and where `/vendor` dependencies are declared.
- The gap was real rather than cosmetic: docs/04, docs/07 and docs/14 already
  use `/system/...` paths as facts, docs/13 requires a lock manifest without
  saying where it lives, and I-16 requires a reported source path that nothing
  made resolvable.
- Manifests stay inside module source, following docs/11; no parallel manifest
  tree is introduced. Lock manifests are classified as canonical source, not
  derived cache, because they cannot be regenerated identically later.
- docs/17 now records that its layout tree is written relative to the
  implementation root, which the implemented repository nests under `source/`.

### 2026-08-10 — Engineering-ownership motivation restored

- README "Core thesis", docs/00 purpose and a new docs/01 section put the
  owner's engineering ownership first: open an installed component as source,
  understand it, change it, check the change, keep running that version.
  Provenance, reproducibility, rollback and auditability are stated as
  consequences of the model rather than its purpose.
- docs/00 replaces "every non-firmware component" with the precise boundary
  from ADR-0030, and README states plainly that TOS does not pretend vendor
  microcode and device firmware are open.
- README status corrected: Stage 2 Part A is accepted, Part B is under way with
  the source reader and lexer complete and the parser in progress.

### 2026-08-10 — Accepted parser diagnostics and recovery clarification

- Project Architect accepted ADR-0032, closing three gaps the first production
  parser exposed in the accepted TOS Core V1 contract.
- **Registry conflict.** docs/41 §7 asserted that a full diagnostic registry is
  in docs/44; docs/44 had none, and docs/39 allocated only E1105/E1106 while
  eight conformance rejects said merely "parse error". docs/44 §7 is now the
  authoritative registry for every source-reader, lexer and parser code, with
  stage and exact condition. E1100–E1104 and E1107 are ratified after checking
  that each has one unambiguous meaning and no overlap; E1105/E1106 keep their
  numbers. E1107_UNEXPECTED_TOKEN is the registered residual of the parse stage.
- Later-stage families (E12xx–E18xx, V20xx) stay with docs/40–43 and MUST be
  folded into the registry by the stage that implements them; their stage
  labels are not guessed in advance.
- **Declaration recovery.** docs/39 §4 now ends a declaration region at the `}`
  closing a top-level declaration body, because a `fn` declaration ends with a
  block and neither `;` nor `]` terminates it. No further heuristic is admitted.
- **E1013_UNEXPECTED_CHARACTER** allocated for a valid UTF-8 character that
  begins no lexical form. Precedence is mechanical: non-ASCII outside a literal
  or comment stays E1012_INVALID_IDENTIFIER, everything else is E1013.
  Conformance vectors R029/R030 fix span and precedence for `@` and `$`.
- EXPECTATIONS.md no longer contains "parse error": every reject names a stable
  code. `scripts/check-stage2-language-contract.py` now fails when a cited code
  is unregistered, a registry entry lacks stage or condition, docs/39 names an
  unregistered frontend code, or a "parse error" cell reappears.

### 2026-08-10 — Stage 2 Part B: parser diagnostic model and recovery

- Public parser API moved from single-error `Result<_, ParseError>` to
  `ParseOutcome<T>`: structured diagnostics plus the partial tree. No second
  parsing path exists — `ParseError` is an internal signal and `ParseErrorCode`
  is no longer exported. `into_accepted()` yields a tree only when no error
  diagnostic was produced.
- New `diagnostic.rs` implements docs/41 §7: stable symbolic code, severity,
  stage, byte span, derived line/UTF-8 column, structured key/value fields and
  ordered causal diagnostics. Module name, canonical repository path,
  source-set identity and normalized source content ID are deliberately absent:
  their owner is the compilation driver of docs/42, which does not exist yet,
  and the record carries no placeholder values.
- All three docs/39 §4 synchronization regions implemented — declaration
  (next top-level `;`/`]`, or the `}` closing a declaration body per ADR-0032),
  statement (next `;`, or the block's closing brace left unconsumed) and list
  (next `,` or the list closer, including `>` for type-argument lists). One
  diagnostic per region; nothing missing is guessed.
- A lexical failure is reported alone, with no parse diagnostics, as the docs/39
  ordering requires. Lexer split for E1013 per ADR-0032.
- Tests 26 → 37: recovery at all three regions, a valid declaration surviving a
  damaged function, lexical isolation, E1012/E1013 precedence, R029/R030 span
  and column, line/UTF-8 columns, withheld accepted output, and R020–R022 read
  directly from the conformance corpus.
- Fixed a type-parsing defect from cf4224f: `Option<Option<i32>>` did not parse
  because the lexer emits `>>` as one shift token while `parse_type` expected
  the word `>`. Added `expect_close_angle`, which splits the token.

### 2026-08-10 — Stage 2 Part B: TOS Core grammar complete

- Парсер покрывает всю EBNF docs/39 §5: postfix chain (`.field`, `[index]`,
  `?`, `as`), named call arguments, tuple/array/closure/spawn primaries,
  паттерны всех четырёх форм, пятнадцать statement-форм (if/else, match, while,
  for, loop, break, continue, parallel, cancel, defer, unsafe плюс ранее
  реализованные let/return/assignment/expression) и полный item-набор с
  `pub`, `const` и `async fn`.
- Assignment ограничен `place` (имя плюс field/index суффиксы), как требует
  грамматика; для этого форма `Primary` разделена на `Literal` и `Name`.
- Новый conformance-гейт `crates/tos-core/tests/conformance.rs` привязывает
  парсер прямо к корпусу: canonical examples и `accept/` обязаны парситься без
  единой диагностики; `reject/` с frontend-кодом обязан выдать именно его;
  `reject/` с кодом поздней стадии обязан пройти парсер, иначе вектор не может
  дойти до стадии, которую заявляет.
- Гейт нашёл три дефекта корпуса: `async.tos`/`capability.tos`/`parallel.tos`
  использовали зарезервированные слова как сегмент имени модуля (переименованы
  вместе с файлами); `unchecked-conversion.tos` не мог дойти до
  `E1212_INVALID_AS_CONVERSION`, потому что сам не парсился; R024 ожидал
  `E1107` там, где точный код — `E1101_EXPECTED_IDENTIFIER`.
- Fuzz расширен на TOS Core: source reader и парсер прогоняются по мутациям и
  случайным байтам; проверяется тотальность, завершаемость recovery и то, что
  исход всегда либо чистое дерево, либо хотя бы одна диагностика.
  200 000 раундов PASS.
- Исправлено ложное срабатывание `scripts/check-unsafe-safety.py`: `unsafe {`
  внутри строкового литерала с образцом TOS-исходника читался как Rust-блок.
  Проверка по-прежнему ловит настоящие unsafe-блоки без SAFETY.
- Тестов 37 → 60 (58 unit + 2 conformance). `./scripts/preflight.sh --full`
  → **31/31 PASS**.
- Шаг 2 из docs/44 §6 закрыт. Шаги 3–10 (checker, ownership, IR, verifier,
  интерпретатор, source maps, corpus/perf evidence, `init.tos`) не начаты.

### 2026-08-10 — Stage 2 Part B: first checker slice

- Новый `checker.rs` — начало шага 3 docs/44 §6. Реализованы проверки, которым
  достаточно собственных объявлений модуля: resource envelope по docs/41 §6
  (`E1700_RESOURCE_DECLARATION_REQUIRED` для каждого из десяти обязательных
  ключей, `E1703_DUPLICATE_RESOURCE_DECLARATION`, `E1704_UNKNOWN_RESOURCE_LIMIT`
  для незнакомого ключа или неверного класса литерала) и exact-once именованных
  полей (`E1205_DUPLICATE_RECORD_FIELD`) — как в объявлении record/enum-варианта,
  так и в named-конструкторе на любой глубине выражения.
- Семейства E12xx и E17xx частично внесены в реестр docs/44 §7 со stage и
  условием, как требует правило включения. Механический гейт проверяет stage
  каждого реализованного кода.
- Conformance-гейт расширен: вектор с кодом поздней стадии, которая уже
  реализована, обязан быть отклонён чекером именно этим кодом. R014 и R025
  (`duplicate-record-field`, `duplicate-record-constructor-field`) теперь
  связаны с реализацией.
- Name resolution, типы, эффекты и ownership не реализованы и ничего не
  сообщают — проверка, которую нельзя выполнить, молчит, а не угадывает.
- Тестов 60 → 64. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: value-name resolution

- Чекер резолвит каждое имя в value-позиции: predeclared values, predeclared
  functions и atomic orders; module scope (импорты, records, варианты enum как
  unqualified-конструкторы, consts, extern fn, fn — собирается до обхода тел,
  поэтому порядок объявления и рекурсия работают); параметры функции; биндинги
  `let`, `for`, ветвей `match` и параметров замыканий с блочной областью
  видимости. Нерезолвящееся имя — `E1202_UNKNOWN_VALUE_NAME`.
- Имена полей после `.` и метки именованных аргументов не резолвятся как
  значения — это поля, их проверит типовой срез.
- Инициализатор `let` не видит собственный биндинг; биндинг не покидает блок.
- Проверено эмпирически: по всем canonical examples и `accept/`-векторам ноль
  нерезолвленных имён. R015 (`nil-absence`) связан с реализацией.
- `E1202_UNKNOWN_VALUE_NAME` внесён в реестр docs/44 §7 со stage `type`.
- Тестов 64 → 69. `./scripts/preflight.sh --full` → **31/31 PASS**.

**Требуется решение Project Architect (Level 2):** правило разрешения bare
pattern name. docs/39 §2 объявляет нешадоwable только predeclared value names,
из чего следует, что любой другой идентификатор в паттерне связывает; но
принятый корпус (`explicit-control-return.tos`, `copy-aggregates.tos`)
сопоставляет пользовательские варианты enum по краткому имени. Текущий срез
диагностически нейтрален к обоим прочтениям — множество резолвящихся имён
совпадает. Следующий срез (типы и exhaustiveness `match`) без этого правила
реализован быть не может.

### 2026-08-10 — Accepted pattern name resolution (ADR-0033)

- Project Architect принял ADR-0033. Bare identifier в паттерне разрешается от
  **ожидаемого типа**: если ожидаемый тип — enum и имя точно совпадает с
  вариантом, это constructor pattern; иначе — новый binding. Правило
  номинальное: регистр букв не значит ничего, и существующий lexical binding с
  тем же именем решение не меняет. Два enum могут иметь одноимённые варианты —
  различает тип subject. `Some`/`None`/`Ok`/`Err`/`Completed`/`Cancelled`
  остаются non-shadowable конструкторами.
- Закрыт конфликт docs/39 ↔ docs/40: docs/40 §2 требовал квалифицированное имя
  для импортированного варианта, а `pattern` в грамматике его не выражал.
  Добавлена форма `pattern_path = pattern_name ( "." identifier )*` на
  существующей точечной пунктуации, без `::`. Одиночный identifier остаётся
  ровно одной синтаксической альтернативой — парсер не решает, конструктор это
  или binding. Путь с точкой всегда конструктор и никогда не binding.
- Синхронизированы docs/39 (грамматика + pattern resolution boundary), docs/40
  (точная семантика), docs/42 (qualified/import resolution), docs/44 (класс
  conformance-векторов), реестр `E1202`.
- Корпус: C017–C019 и R031–R032 покрывают все десять требуемых случаев —
  локальный краткий unit-вариант, binding при отсутствии варианта, два enum с
  общим именем варианта, payload-деструктуризация, явно квалифицированный
  локальный вариант, квалифицированный импортированный, неизвестный
  квалифицированный, exhaustive match по кратким именам, wildcard/binding
  exhaustiveness и независимость от регистра.
- Реализовано сейчас: парсер строит полный путь; чекер валидирует
  квалифицированный путь по локально объявленным enum'ам (`E1202`). Разрешение
  краткого имени от ожидаемого типа и exhaustiveness требуют типов и придут с
  типовым срезом; текущая аппроксимация диагностически нейтральна.
- Тестов 69 → 72. `./scripts/preflight.sh --full` → **31/31 PASS**.

**Отдельная граница, решение не принималось:** должны ли паттерны в `let` и
`for` быть irrefutable и что сообщается для refutable в этих позициях.
ADR-0033 явно оставляет вопрос открытым; реализация его не выводит.

### 2026-08-10 — Stage 2 Part B: return completeness

- Новый `returns.rs` реализует правило docs/40 §5: каждый достижимый путь
  нормального завершения функции с не-`unit` возвращаемым типом обязан
  выполнить явный `return`; достижение конца такой функции —
  `E1221_MISSING_RETURN`. Тело closure и `spawn` подчиняется тому же правилу
  против выведенного результата: смешение возврата значения с достижимым
  проваливанием — та же ошибка.
- Анализ — чистая достижимость, типы не нужны: `return`/`break`/`continue` не
  завершаются нормально; `if` — только если обе ветви завершаются; `loop`
  завершается нормально лишь при наличии `break` своего уровня; `while`/`for`
  завершаются всегда; `parallel`/`unsafe` прозрачны.
- Каждая return scope анализируется отдельно: `return` внутри closure или
  spawned body относится к ней, поэтому вложенные scope не засчитываются
  внешней функции и наоборот.
- Exhaustiveness `match` (`E1220`) относится к типовому срезу. Здесь match
  считается завершающимся, если хотя бы одна ветвь завершается нормально —
  такое допущение может пропустить ошибку, но никогда не выдумывает её.
- R019 (`missing-nonunit-return`) связан с реализацией; `E1221_MISSING_RETURN`
  внесён в реестр docs/44 §7 со stage `type`.
- Тестов 72 → 77. `./scripts/preflight.sh --full` → **31/31 PASS**.

**Следующая граница, требующая решения (Level 2):** разрешение типовых
выражений. docs/40 §1 задаёт правила (имя типа разрешается через объявленный
import-граф; `Result<T,E>` принимает два аргумента), но диагностических кодов
для «имя типа ни к чему не разрешается» и «неверная арность конструктора типа»
не выделено. По ADR-0032 выделение новых кодов в реестре — versioned language
decision, а не выбор реализации.

### 2026-08-10 — Stage 2 Part B: Bootstrap profile enforcement

- Новый `profile.rs` реализует docs/42 §3: `profile bootstrap` — строгое
  исполнимое подмножество `profile full`, и Full-модуль не может быть молча
  принят Bootstrap-фронтендом. Запрещены `async fn`, `spawn async`, `await`,
  closures, `defer`, `unsafe`, `extern`, а также `workers` больше 1.
- Сообщается ровно одна диагностика `E1702_PROFILE_NOT_SUPPORTED` — с первой
  запрещённой возможностью в порядке исходника, как требует контракт, плюс
  поля `feature` и `profile`.
- `parallel`, `spawn parallel`, `join` и `cancel` остаются разрешёнными: у них
  определённая сериализованная Bootstrap-семантика по docs/41.
- Проверка чисто синтаксическая, типы не нужны. R006 (`full-profile-async`)
  связан с реализацией; `E1702_PROFILE_NOT_SUPPORTED` внесён в реестр
  docs/44 §7 со stage `resource`.
- Тестов 77 → 81. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: assignment mutability

- Новый `mutability.rs` реализует docs/40 §2: присваивание требует mutable
  binding, присваивание в неизменяемое место — `E1201_ASSIGN_TO_IMMUTABLE`.
- Мутабельность определяется корнем place: `movable.x = ...` разрешено при
  `let mut movable`, `fixed.x = ...` — нет. Парсер уже гарантирует, что цель
  присваивания — place, поэтому корень всегда `Name`.
- Параметр присваиваем только при `borrow mut`; owned и `borrow` параметры
  неизменяемы. Биндинги `for` и ветвей `match` неизменяемы — грамматика не даёт
  им `mut`.
- Несвязанное имя в цели присваивания сообщается только как `E1202`: одна
  ошибка не удваивается.
- Отслеживание активных borrow относится к ownership-срезу; здесь сообщается
  только то, что делают определённым сами формы объявления.
- `E1201_ASSIGN_TO_IMMUTABLE` внесён в реестр docs/44 §7 со stage `type`.
- Тестов 81 → 85. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: unsafe and FFI boundary

- Новый `boundary.rs` реализует docs/42 §5 и docs/40 §7. V1 резервирует
  синтаксис `extern` и `unsafe`, чтобы граница была видна с первой реализации,
  но не допускает никакого внешнего calling contract.
- Любой `extern`-item отвергается как `E1801_FFI_NOT_AVAILABLE`: принятой
  FFI-схемы интерфейса в V1 нет, и docs/42 прямо запрещает включать её флагом
  сборки, наличием host-библиотеки или unsafe-блоком.
- Блок `unsafe { ... }` обязан открываться строчным комментарием, начинающимся
  с `SAFETY:`; отсутствие — `E1802_UNSAFE_RATIONALE_REQUIRED`. Комментарий
  должен вести блок, поэтому `SAFETY:` после первого оператора не засчитывается.
  Лексер отбрасывает комментарии, так что обоснование читается из текста
  самого блока.
- Семейство E18xx внесено в реестр docs/44 §7 со stage `effect`: обе проверки
  говорят о том, что язык допускает на внешней/unsafe границе, и обе выражены
  через контракты интерфейсов и capability.
- Тестов 85 → 88. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: declared language version

- Чекер проверяет объявленную версию исходного языка по docs/42 §1: для V1 она
  обязана быть ровно `1.0`. Другой major — `E1601_UNSUPPORTED_LANGUAGE_VERSION`,
  неизвестный minor — `E1602_UNSUPPORTED_LANGUAGE_MINOR`. Это версия языка, а
  не номер релиза модуля, поэтому модуль не может выбрать себе диалект.
- Неподдерживаемый major скрывает находку по minor: один заголовок не бывает
  неверен двумя способами сразу.
- Семейство E16xx частично внесено в реестр docs/44 §7 со stage `type`.
  E1603–E1607 требуют канонического пути и разрешения импортов между модулями
  и придут со срезом, который этим владеет.
- Тестов 88 → 90. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: named constructor fields

- Чекер сверяет именованный список аргументов с конструктором, который он
  называет: неизвестное поле — `E1207_UNKNOWN_RECORD_FIELD`, пропущенное —
  `E1206_MISSING_RECORD_FIELD` (дубликат `E1205` был уже). Порядок полей
  значения не имеет — правило exact-once, а не позиционное.
- Работает и для record, и для named-field вариантов enum: docs/39 §5 даёт им
  одну форму конструирования. Имя, объявленное более чем одним конструктором,
  пропускается — выбор между ними требует типов.
- Это номинальное разрешение, а не типизация: проверка включается, только когда
  callee — простое имя одного локального конструктора. Обычный вызов функции
  против списка полей не проверяется.
- R026 (`missing-record-constructor-field`) связан с реализацией; `E1206` и
  `E1207` внесены в реестр docs/44 §7 со stage `type`.
- Тестов 90 → 94. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: defer body restrictions

- Новый `defer.rs` реализует docs/40 §5: тело `defer` не может выполнять
  `return`, `break`, `continue`, `await`, `join`, порождать работу или
  захватывать новый ресурс — `E1225_INVALID_DEFER`. Cleanup-блок, способный сам
  отвести управление, сделал бы порядок выполнения defer неанализируемым.
- Шесть из семи запретов видны в дереве и проверяются. «Захват нового ресурса» —
  типизированное свойство вызываемой операции; здесь по нему ничего не
  сообщается, вместо угадывания, какие вызовы аллоцируют.
- Границы областей соблюдены: `break` внутри цикла, объявленного в самом теле
  defer, относится к этому циклу и разрешён; тело closure — собственная return
  scope, поэтому `return` в ней не отводит управление из cleanup-блока; тело
  `spawn` не просматривается, потому что запрещён сам spawn.
- `E1225_INVALID_DEFER` внесён в реестр docs/44 §7 со stage `type`.
- Тестов 94 → 97. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: module resolution over a source set

- Новый `modules.rs` и публичный API `ModuleEntry` / `check_module_set`
  реализуют docs/42 §1 в части, которой нужен не один модуль: имя `a.b.c`
  отображается в канонический путь `a/b/c.tos`, несовпадение —
  `E1603_MODULE_PATH_MISMATCH`; импорт, не называющий ни один модуль набора, —
  `E1604_IMPORT_NOT_FOUND`; цикл в графе импортов — `E1606_IMPORT_CYCLE` с
  упорядоченным путём цикла в поле.
- Резолвер читает только переданный source set: ни текущего каталога, ни
  файловой системы, ни сети, ни часов, ни окружения; импорт не инициирует fetch.
- Один цикл — одна находка, независимо от того, со скольких участников в него
  входят. Обход стартует по именам модулей в лексическом порядке и следует
  импортам в порядке исходника, поэтому путь цикла воспроизводим.
- Общая зависимость двух модулей циклом не считается.
- `E1605_AMBIGUOUS_IMPORT` возникает, когда имя разрешается более чем под одним
  объявленным module root. Список корней — конфигурация compilation driver,
  которую этот API пока не принимает, поэтому условие не сообщается, а не
  аппроксимируется по единственному корню.
- Тестов 97 → 101. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: module identity on diagnostics

- Закрыт задокументированный пробел в диагностической записи: docs/41 §7
  требует на каждой диагностике имя модуля, канонический repository path,
  normalized source content ID и source-set identity. Раньше их некому было
  проставить; теперь резолвер набора модулей знает путь.
- `ModuleIdentity` несёт имя, путь, content ID и опциональный source-set.
  Content ID — SHA-256 нормализованных байтов, то есть именует ровно тот текст,
  который принял фронтенд, а не транспортную форму: LF и CRLF дают один ID.
- `ModuleEntry::check` и `check_source_set` проставляют identity на каждую
  диагностику — и per-module, и межмодульную. Диагностика, полученная без
  резолвера, identity не несёт: одна source unit не может назвать путь, и
  placeholder не выдумывается.
- Source-set identity остаётся входом compilation driver (выбранный system
  commit или принятая detached identity), а не выводимой величиной.
- `tos-core` получил зависимость на первопартийный `tos-hash`: content identity
  требует хеша.
- Тестов 101 → 104. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: bounded diagnostics per module

- Реализован обязательный предел docs/44 §2: `MAX_DIAGNOSTICS_PER_MODULE = 256`.
  Враждебный исходник может нести ошибку каждые несколько байт, поэтому число
  диагностик ограничено так же, как любой другой вход фронтенда.
- Достижение предела останавливает запись, а не разбор: recovery доходит до
  конца, исход остаётся корректным, `has_errors` сохраняется.
- Удерживаются самые ранние диагностики, поэтому усечённый список всё равно
  начинается с первой проблемы в исходнике. `ParseOutcome::is_truncated`
  сообщает о факте усечения, вместо молчаливой потери.
- Тестов 104 → 106. `./scripts/preflight.sh --full` → **31/31 PASS**;
  fuzz 100 000 раундов PASS.

### 2026-08-10 — Accepted type-name and arity diagnostics (ADR-0034)

- Project Architect принял ADR-0034: выделены `E1203_UNKNOWN_TYPE_NAME` и
  `E1204_TYPE_ARGUMENT_ARITY`, оба stage `type`.
- Убрана двусмысленность docs/40: фраза «using another arity is a parse/type
  error» заменена одним нормативным ответом — число type arguments является
  статико-типовым свойством, парсер строит constructed-type node для любого
  известного V1-конструктора с `<...>`, а checker сравнивает фактическое число
  с фиксированной арностью. docs/39 получил соответствующую grammar boundary.
- User generics не вводятся: `<...>` допустим только после имени, которое язык
  уже определяет как параметризованный конструктор. `array<T, N>` остаётся в
  своей форме — его второй аргумент константа, а не тип.
- Precedence зафиксирована: неразрешённое имя → `E1203`; разрешённый
  конструктор с неверным числом аргументов → `E1204`; типы аргументов
  проверяются только после корректной арности. Одна ошибка не порождает
  каскад из несуществующего constructed type.
- Квалифицированное имя разрешает сначала module/import часть: биндинг, не
  являющийся импортом, — `E1203`; импорт, который сам не разрешается, — один
  `E1604` от модульного среза, без удвоения; существующий импорт, чей модуль не
  объявляет имя, — `E1203` от межмодульного среза с полем `module`.
- Корпус: R033–R037 покрывают все пять требуемых случаев.
- **Гейт нашёл собственную дыру**: accept-векторы проверялись только на разбор,
  но не чекером, из-за чего инвертированное условие в резолвере не было
  замечено. Гейт исправлен — canonical source обязан пройти и разбор, и все
  реализованные проверки; он сразу поймал ошибку на
  `accept/pattern-qualified-import.tos`.
- Тестов 106 → 112. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: match exhaustiveness

- Новый `exhaustiveness.rs` реализует docs/40 §5: `match` по enum, `Option`,
  `Result` или `TaskResult` обязан покрывать все варианты; непокрытый —
  `E1220_NONEXHAUSTIVE_MATCH` с полями subject, missing и missing_count.
- Правило покрытия арок взято из принятого ADR-0033: `_` исчерпывает, и краткое
  имя, не являющееся вариантом ожидаемого типа, тоже — оно связывает, а
  связывание совпадает с любым значением. Квалифицированный путь и
  payload-деструктуризация засчитываются как покрытие своего варианта.
- Ожидаемый тип берётся там, где исходник его заявляет: объявленный тип
  параметра или `let` с аннотацией. Scrutinee без заявленного типа не
  анализируется — вывод типов сюда не входит, а догадка могла бы выдумать
  недостающий случай.
- R032 (`pattern-nonexhaustive-variants`) связан с реализацией;
  `E1220_NONEXHAUSTIVE_MATCH` внесён в реестр docs/44 §7 со stage `type`.
- Тестов 112 → 117. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: expression typing and return agreement

- Новый `typing.rs` даёт выражениям тип и проверяет им одно правило docs/40 §5:
  каждый `return` в функции обязан нести объявленный результат, а `return;` в
  не-`unit` функции — та же ошибка `E1222_RETURN_TYPE_MISMATCH` с полями
  expected/actual.
- Типизация сознательно частичная. Выражение, тип которого объявления не
  определяют, получает `Unknown`, а `Unknown` согласуется со всем — поэтому
  неопределённый тип никогда не порождает диагностику, пока вывод неполон.
- Неsuffixed целочисленный литерал контекстно типизируется по docs/40 §3:
  согласуется с любым точным целым типом и с `size`. Проверка диапазона в этот
  срез не входит.
- Выводятся: литералы всех классов, имена (параметры, `let` с аннотацией и без,
  module consts, unit-варианты enum), вызовы объявленных функций и
  конструкторов, фиксированные `to_*` (`Result<D, ConversionError>`), доступ к
  полю record, индексация массива, tuple/array-литералы, группировка, `as`,
  бинарные и унарные операторы, `?` (снимает `Result` до payload) и
  `await`/`join` (`Task<T>` → `TaskResult<T>`).
- Тип из другого модуля имеет известную идентичность, но не форму — `Unknown`.
- Новый вектор R038 (`return-type-mismatch`) связан с реализацией;
  `E1222_RETURN_TYPE_MISMATCH` внесён в реестр docs/44 §7 со stage `type`.
- Тестов 117 → 124. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: as-conversion legality

- Реализовано правило docs/40 §3: `as` допустима только для целочисленного
  расширения с сохранением знаковости. Любая другая конверсия —
  `E1212_INVALID_AS_CONVERSION` с полями from/to. Narrowing, смена знака и
  конверсия в тот же тип отвергаются; проверенное сужение выражается вызовом
  `to_*`, возвращающим `Result<D, ConversionError>`.
- Каст непрозрачного handle (`Task`, `Region`, `Mutex`, `Channel`, атомики,
  функции и т. п.) docs/40 §3 сознательно направляет не сюда. Для capability
  выделен `E1502_FORGED_CAPABILITY`; для остальных непрозрачных типов документ
  говорит «the corresponding nonconstructible-type error», но такого кода ни
  один документ не выделяет. Поэтому по ним не сообщается ничего, вместо
  заимствования кода с другим смыслом.
- R016 (`unchecked-conversion`) связан с реализацией;
  `E1212_INVALID_AS_CONVERSION` внесён в реестр docs/44 §7 со stage `type`.
- Тестов 124 → 128. `./scripts/preflight.sh --full` → **31/31 PASS**.

**Ожидает решения (выделение кода):** «nonconstructible-type error» для каста
непрозрачных типов, кроме capability. Условие описано в docs/40 §3 словами, но
кода не имеет; по ADR-0032 выделение — versioned language decision.

### 2026-08-10 — Stage 2 Part B: integer type agreement

- Реализовано правило docs/40 §3: присваивание или передача значений разных
  целочисленных типов — `E1210_INTEGER_TYPE_MISMATCH` с полями expected,
  actual и position (`assignment` либо `argument`). Неявных числовых конверсий
  нет; расширение выражается `as`, проверенное сужение — вызовом `to_*`.
- Неsuffixed литерал принимает требуемый целый тип и мисматча не даёт.
- `size` — собственный тип этого семейства: он не становится `i32` молча.
- Позиционные аргументы сверяются по порядку параметров; именованный список
  принадлежит конструктору и проверяется по именам полей, а не по позиции.
- Расхождение между другими родами типов (например `bool` в `i32`) кода не
  имеет: docs/40 §3 выделяет `E1210` только для целочисленных. Сообщается
  только то, что контракт называет.
- Новый вектор R039 (`integer-type-mismatch`) связан с реализацией;
  `E1210_INTEGER_TYPE_MISMATCH` внесён в реестр docs/44 §7 со stage `type`.
- Тестов 128 → 132. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: index type

- Реализовано правило docs/40 §3: индекс массива, slice и region имеет точный
  тип `size`; целочисленный литерал контекстно типизируется как `size`. Любой
  другой тип индекса — `E1211_INDEX_TYPE_MISMATCH` с полями expected/actual.
- Индексация даёт тип элемента: `array<T, N>` и `slice<T>` → `T`, что
  подхватывается остальной типизацией.
- Новый вектор R040 (`index-type-mismatch`) связан с реализацией;
  `E1211_INDEX_TYPE_MISMATCH` внесён в реестр docs/44 §7 со stage `type`.
- Тестов 132 → 134. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Найден конфликт: docs/42 §1 ↔ принятый conformance-корпус

При реализации `E1607_PRIVATE_PUBLIC_TYPE` обнаружено систематическое
противоречие двух принятых источников. Реализация отката, решение не
принималось.

- **docs/42 §1** (Tier 2): «A public function's parameter/return types and
  effect capabilities must be exported/reachable; an otherwise private ABI type
  is `E1607_PRIVATE_PUBLIC_TYPE`.» То есть тип, объявленный без `pub`, не может
  появляться в сигнатуре `pub fn`.
- **Принятый корпус** (docs/44 §1 называет его accepted conformance contract
  evidence): 15 из 27 файлов делают ровно это. `first.tos` объявляет
  `enum FirstError` без `pub` и возвращает `Result<i32, FirstError>` из
  `pub fn main`. То же в data.tos, ownership.tos, results.tos,
  counter-service.tos, capabilities.tos, parallel-work.tos,
  bootstrap-parallel.tos, call-and-constructor.tos, control-heads.tos,
  explicit-control-return.tos, named-enum-variant.tos,
  named-record-constructor.tos, pattern-bindings.tos,
  pattern-local-variants.tos.
- Форма `pub record` / `pub enum` разрешена грамматикой docs/39 §5, но в корпусе
  не встречается **ни разу**. Единственный межмодульный пример (C005,
  `math.tos` → `modules.tos`) экспортирует только функцию с примитивной
  сигнатурой, поэтому ни один тип в корпусе границу модуля не пересекает.

Прогон реализованной по букве docs/42 проверки даёт `E1607` на 12 accept-файлах
и меняет первичный код у двух reject-векторов. Это не единичная опечатка, а
расхождение контракта и его же принятых доказательств.

**Два возможных разрешения, выбор за Project Architect:**

1. Прав контракт: корпус приводится в соответствие — типы в публичных
   сигнатурах получают `pub`. Меняются 15 файлов; правило остаётся как
   написано, и приватный тип в публичном интерфейсе действительно отвергается.
2. Правило уже написано, но шире задуманного: например, оно относится только к
   типу, фактически пересекающему границу импорта, а модуль-приватный тип в
   публичной сигнатуре допустим, потому что V1 не даёт отдельного ABI-обещания.
   Тогда правится формулировка docs/42 §1, а корпус остаётся.

До решения `E1607_PRIVATE_PUBLIC_TYPE` не реализуется, и семейство E16xx в
реестре docs/44 §7 остаётся без него.

### 2026-08-10 — Разрешён конфликт docs/42 §1: вариант 1

- Project Architect выбрал вариант 1: правило docs/42 §1 сохраняется, корпус
  приводится ему в соответствие. `pub` означает публичный **source-level**
  интерфейс, поэтому импортирующий модуль обязан суметь назвать и разрешить
  типы публичной сигнатуры. Отсутствие обещания стабильного binary ABI —
  отдельное утверждение и visibility rule не ослабляет.
- docs/42 §1 уточнён: правило покрывает **транзитивную публичную type surface**,
  а не только внешнее имя. Публично необходимая поверхность экспортированного
  record — типы его полей, enum — типы payload вариантов, потому что потребитель
  не может ни сконструировать, ни сматчить их, не назвав. `pub record Wrapper
  [value: PrivateType]` не обходит правило. Тип, используемый только в теле
  функции или только модуль-приватным item, — деталь реализации и в поверхность
  не входит.
- Корпус: экспортировано 27 типов в 23 файлах — только те, что действительно
  входят в публичную поверхность, без механической расстановки `pub`. Первичный
  код каждого reject-вектора сохранён.
- Добавлены пять векторов для самого правила: C020 (экспортированный тип в
  публичной сигнатуре, приватные — только в теле и в приватной функции), C021
  (импортированный экспортированный тип через **настоящую границу двух
  модулей**), C022 (приватные типы у приватной функции и внутри тела публичной),
  R041 (`pub fn` прямо называет приватный тип), R042 (приватный тип транзитивно
  через экспортированную обёртку).
- Реализован `E1607_PRIVATE_PUBLIC_TYPE` с транзитивным обходом и множеством
  посещённых, поэтому рекурсивный экспортированный тип завершает обход.
  Импортированный тип достижим в своём модуле и не проверяется здесь.
- `E1607_PRIVATE_PUBLIC_TYPE` внесён в реестр docs/44 §7 со stage `type`;
  добавлен класс векторов `visibility`.
- Тестов 134 → 139. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: affine ownership, move and use after move

- Новый `ownership.rs` реализует docs/40 §5: безопасное не-`Copy` значение
  аффинно, у него один владелец, и оно перемещается при присваивании, передаче
  во владеющий параметр, возврате и помещении в агрегат. Использование после
  перемещения — `E1301_USE_AFTER_MOVE` со stage `ownership` и полями binding и
  moved_at.
- Copy-множество фиксировано контрактом и вычисляется тем же выводом типов, что
  и остальная типизация: `Type::is_copy` — единственный источник истины.
  Числовые типы, `size`, `duration`, `bool`, `unit` и `Shared<T>` — Copy; tuple
  Copy ровно когда все элементы Copy, array — когда элемент Copy;
  пользовательские record/enum, `Option`/`Result`/`TaskResult`, строки, байты и
  все дескрипторы — аффинны. Неопределённый тип считается Copy, поэтому
  неизвестность никогда не даёт move-диагностику.
- `borrow` читает, не забирая владения, поэтому заимствованный аргумент не
  делает следующее использование ошибкой.
- Срез отслеживает целые простые биндинги. Путь вроде `message.payload`
  считается использованием `message`, поэтому перемещение с последующим чтением
  поля ловится; частичное перемещение одного поля пока не моделируется и ничего
  не сообщает.
- R001 (`use-after-move`) и R017 (`noncopy-aggregate`) связаны с реализацией;
  `E1301_USE_AFTER_MOVE` внесён в реестр docs/44 §7 со stage `ownership`.
- Тестов 139 → 144. `./scripts/preflight.sh --full` → **31/31 PASS**.

### 2026-08-10 — Stage 2 Part B: ownership, borrows and captures

- `ownership.rs` переписан как структурный dataflow поверх `flow.rs` и
  `place.rs`. Каждая альтернатива выполняется из одного и того же входного
  состояния и результаты объединяются, поэтому move в одной ветви не протекает
  в соседнюю, а move по любому достижимому пути блокирует последующее
  использование. Недостижимый путь (после `return`/`break`/`continue`) в join
  не вносит ничего. Тело цикла анализируется дважды: move'ы только
  накапливаются, поэтому это неподвижная точка, а первый проход молчит — так
  диагностики детерминированы и не дублируются.
- Состояние выражено над **местами**, как и правила docs/40 §5: record можно
  переместить частично, borrow поля запирает содержащий путь, но не соседние
  поля, индексы перекрываются, кроме неравных константных. Биндинги ключуются
  по вхождению объявления, поэтому shadowing никогда не сливает два разных.
- Реализованы `E1302_CONFLICTING_BORROW`, `E1303_MUTATE_WHILE_BORROWED`,
  `E1304_INVALID_TASK_CAPTURE`, `E1305_INVALID_CLOSURE_CAPTURE`; `E1301`
  доведён до корректной работы на control flow, grouping и match-субъекте
  (patterns bind by move), плюс partial moves.
- Регион borrow: временный живёт до конца своего statement, связанный именем —
  до конца блока, который его скоупит. Захват анализируется по настоящему
  множеству свободных имён тела, а не по всем видимым биндингам. Невалидный
  захват не является переносом, поэтому владение остаётся у внешнего
  владельца и ложный `E1301` за ним не следует.
- Корпус: R002→`E1302`, R004→`E1304` подключены; добавлены C023, C024, R043,
  R044. Все пять ownership-кодов внесены в реестр docs/44 §7 со stage
  `ownership`.
- Тестов 144 → 178. `./scripts/preflight.sh --full` → **31/31 PASS**.

**Архитектурная граница (проверено, blocker'а нет).** Ownership — свойство
frontend'а TOS Core, а не всей ОС. Проверено, что принятые документы это
допускают: docs/07 прямо предусматривает уровни совместимости `compatible`,
`subset`, `translated` и foreign runtimes; docs/06 определяет TOS IR как
versioned representation, разделяемое поддерживаемыми фронтендами, а docs/43
фиксирует схему `tos-ir/v1` — вместе с её affine/Copy-верификацией — именно за
TOS Core V1. Ни один принятый документ не утверждает, что исполняемый код
обязан происходить из семантики TOS Core.

Оба предусмотренных пути остаются открытыми, и ни один не объявляется
обязательным: будущая versioned схема или профиль IR, способные нести семантику
другого фронтенда, и foreign runtime integration по docs/07 там, где это
уместнее. Соответственно `tos-ir/v1` не становится универсальным IR для
небезопасного языка, а ownership не становится обязательным условием общего IR.
Граница зафиксирована в module-документации `ownership.rs`, `flow.rs` и
`place.rs`: это semantic state фронтенда, доказательство, а не условие
представимости программы; изоляция процесса, capabilities и verifier —
отдельный слой, не зависящий от этих типов.

### 2026-08-10 — Stage 2 Part B: ownership hardening

Исправленное:

1. **Structured loop flow.** `reachable: bool` заменён явными каналами выхода
   `Flow { normal, breaks, continues, returns }`. `break` несёт своё состояние
   в выход цикла, `continue` — в back edge, `return` — из return scope; раньше
   все три просто теряли состояние. `break`/`continue` потребляются своим
   циклом и наружу не выходят, поэтому во вложенных циклах они относятся к
   правильному. Для `while`/`for` вход сам является возможным выходом
   (ноль итераций); для голого `loop` — нет, выход только через `break`.
2. **Loop fixed point.** Вместо предположения «двух проходов всегда хватает»
   тело итерируется до стабильности состояния входа по конечной монотонной
   решётке (`State::same_facts`), с ограничением числа раундов только как
   защитой от ошибки в решётке. Итерации молчат, отчитывается финальный проход.
3. **Порядок вычисления присваивания** приведён к docs/40 §4: сначала base и
   index места слева направо, затем правая часть, затем запись. Введено
   `evaluate_place_address`, которое вычисляет base/index, но не читает старое
   значение цели.
4. **Short-circuit `&&` и `||`.** Правая часть анализируется как условный путь
   и объединяется с путём, где она не выполнялась — это даёт корректную
   `certainty`.
5. **Лексические области в анализе свободных имён.** Объявления дочернего
   блока, одной ветви `if` или одной arm `match` больше не загрязняют соседнюю
   ветвь и не переживают конструкцию: на каждой границе создаётся дочернее
   окружение. `let` действует после себя внутри своего блока; паттерн `for`
   скоупится телом.
6. **`Mutex<T>`/`RwLock<T>` больше не считаются lock guard.** docs/41 отличает
   синхронизационный объект от аффинного guard, выдаваемого операцией
   захвата, — запрещён именно guard. `Region<T>`/`DmaRegion<T>` тоже больше не
   объявляются mutable по одному конструктору типа: docs/40 §6 делает
   shareability и mutability фактом capability contract, которого у чекера
   нет. Обе классификации удалены, чтобы не изобретать E1304.
7. **`defer` больше не обходится как немедленно исполняемый блок** (см. ниже).
8. **C023 исправлен**: теперь вектор действительно захватывает owned affine
   значение и переносит sole ownership; добавлен R045 на последующее внешнее
   использование.

Тестов 178 → 191. `./scripts/preflight.sh --full` → **31/31 PASS**.

---

### 2026-08-11 — Принято ADR-0035: ownership `defer` и класс конфликта E1302

Project Architect закрыл решения A и B предыдущего hardening-среза. ADR-0035
принят, docs/40 §4–5 и реестр docs/44 §7 приведены к нему.

**`defer` — отложенный лексический cleanup, не closure.** Регистрация не
читает, не берёт в borrow и не перемещает значения; лексические имена тела
связываются с binding identities точки регистрации, поэтому позднее shadowing
не меняет смысл тела. На каждом реально достигнутом exit path сначала
вычисляется вызвавшее выход действие, затем зарегистрированные на этом пути
cleanup'ы исполняются в обратном порядке, и состояние одного является входом
следующего; только после этого bindings покидают scope. Тело проверяется
против состояния конкретного exit path, поэтому ресурс остаётся пригодным к
использованию между регистрацией и выходом, а cleanup, невыполнимый на
достигающем его пути, отвергается именно там.

Реализовано поверх существующего `Flow`, без второго механизма unwinding:
`walk_block` накапливает зарегистрированные тела и разматывает их в точке, где
канал покидает блок, — поэтому `return` из вложенного блока проходит cleanup
каждого покидаемого лексического блока, а `break`/`continue` — только тех,
которые действительно покидаются. Одно тело анализируется на каждом пути,
который его исполняет; повторная находка по той же позиции сообщается один раз.

**`E1302_CONFLICTING_BORROW` — весь класс нарушения эксклюзивности.** Новых
кодов не выделено. Принятая матрица:

```text
shared borrow  + owner write   -> E1303
mutable borrow + owner read    -> E1302
mutable borrow + owner write   -> E1302
any borrow     + owner move    -> E1302
incompatible borrow pair       -> E1302
```

Поле `operation` называет операцию. Действия через сам корректный borrow
binding не являются owner alias и остаются допустимыми по его kind; отвергнутое
перемещение не записывается, поэтому за одной ошибкой не следует каскад
`E1301`.

Evidence: C025 (`accept/defer-cleanup-order.tos`), R046–R048 (три строки
матрицы), R049 (`reject/defer-move-then-cleanup.tos`); 15 unit-тестов на
каждую строку матрицы и на каждый exit path defer.

### 2026-08-11 — Аудит diagnostic registry и закрытие frontend frontier

Механическая сверка `docs/44 §7` ↔ production implementation ↔ conformance
evidence. Реестр вырос с 51 до 59 кодов; каждый зарегистрированный код имеет
явный статус.

**Реализовано в этом проходе** (ранее в реестре отсутствовали и в production
path не существовали): `E1401_UNJOINED_TASK`, `E1410_INVALID_ATOMIC_ORDER`,
`E1501_UNDECLARED_CAPABILITY_EFFECT`, `E1502_FORGED_CAPABILITY`,
`E1605_AMBIGUOUS_IMPORT`, `E1701_UNMETERED_LOOP`.

**Найденные попутно дефекты.** Тип-резолвер не знал, что
`import capability system.time.Clock as clock` делает `system.time.Clock`
достижимым импортированным типом, поэтому подделка capability сообщалась как
`E1203_UNKNOWN_TYPE_NAME` и более специфичное правило не срабатывало. Резолвер
источникового набора разрешал capability-импорты против набора модулей, из-за
чего модуль с capability-импортом получал ложный `E1604_IMPORT_NOT_FOUND`.

**Статусы, отличные от «implemented + tested + corpus-bound»:**

- `E1605_AMBIGUOUS_IMPORT` — implemented + unit-tested, corpus vector
  невыразим: условие относится к source set из двух модулей с одинаковым
  именем, а файловый корпус даёт каждому вектору отдельный канонический путь.
- `E1708_UNBOUNDED_CLEANUP` — deliberately unreachable under V1: в грамматике
  docs/39 §4 нет формы объявления drop-контракта, поэтому ни один V1-модуль не
  может создать это условие. Код зарегистрирован для контракта, который такую
  форму введёт.
- `E1225_INVALID_DEFER` — шесть из семи форм проверяются синтаксически;
  «acquire a new resource» не проверяется, потому что V1 не даёт способа
  узнать, какие операции захватывают ресурс: нет ни аннотации, ни типового
  признака. Это не «ещё не сделано», а отсутствие представимости в V1.
- Лексические и парсерные коды `E1011`, `E1020`, `E1030`, `E1031`, `E1100`,
  `E1102`, `E1103`, `E1104` — implemented + unit-tested; корпусные векторы для
  них не добавлялись, потому что каждое условие уже закрыто unit-тестом с
  точным байтовым смещением, а корпус фиксирует семантические границы.

**Незакрытая граница контракта.** docs/40 §3 требует «the corresponding
nonconstructible-type error» для `as` с region, DMA region, task, объектом
синхронизации, функцией и замыканием. Ни один принятый документ такого кода не
называет, поэтому по этим случаям не сообщается ничего. Это остаётся closure
blocker, не решаемый реализацией.

### 2026-08-11 — Stage 2 §6 пункт 5: схема `tos-ir/v1` и детерминированный lowerer

Новый crate `source/crates/tos-ir` содержит **только** семантическую схему
docs/43: header, таблицы типов/констант, импорты и экспортируемые сигнатуры,
функции с блоками, SSA-значениями и терминаторами, source map. В нём нет
frontend, checker, verifier и engine, поэтому будущий независимый verifier
получает декларативную таблицу, не завися от frontend'а — именно та
структурная независимость, которой требует docs/43 §5. Ни одно значение в
схеме не несёт флага «проверено», callback'а или токена успеха, который
verifier мог бы принять вместо собственного обхода.

`Module::is_copy` пересчитывает правило docs/40 по графу типов, а не доверяет
аннотации frontend'а, как прямо требует docs/43 §2. Обход ограничен по глубине,
поэтому подделанная циклическая таблица не приводит к расхождению.

Байтовое представление не замораживается: docs/43 §1 запрещает это делать до
появления production cache. Определён только `module_digest` — канонический
digest по логическим секциям в порядке docs/43 §2, с длиной-префиксом на каждой
переменной позиции, чтобы два разных модуля не могли дать одинаковый поток
байт сдвигом границы. Receipt verifier'а будет привязываться к нему.

`tos-core/src/lower.rs` детерминированно опускает проверенный модуль в
`tos-ir/v1`: типы и константы интернируются в порядке первого использования по
фиксированному обходу объявлений, функции идут в порядке источника, каждая
инструкция несёт индекс source map. Ничего не читается из часов, окружения,
файловой системы или порядка обхода хеш-таблицы.

**Граница покрытия зафиксирована в коде, а не в прозе.** Конструкция вне
реализованного подмножества даёт именованный `Gap` со спаном, а не приблизительный
модуль: приблизительное опускание дало бы IR с семантикой, которой в источнике
нет, и verifier этого не обнаружил бы, потому что такой IR внутренне
непротиворечив. Сейчас опускаются 28 из 38 принятых векторов; вне подмножества
остаются `async fn`, `spawn`, closure, `defer`, `for`, `cancel`, `unsafe` и
привязка имён в arm'ах `match` с payload.

Gate `tests/lowering.rs` проверяет на всём корпусе: детерминизм (двукратное
опускание даёт равные таблицы и равный digest), наличие source map у каждой
функции, блока и инструкции, попадание каждого терминатора в существующий блок,
попадание каждого операнда в таблицу значений или констант, и изменение digest
при изменении модуля.

### 2026-08-11 — Stage 2 §6 пункт 6: независимый verifier `tos-ir/v1`

Новый crate `source/crates/tos-verifier` зависит **только** от `tos-ir` и
`tos-hash`. Он не видит AST, не получает флаг успеха checker'а, не вызывает
callback frontend'а и не принимает никакого поля модуля вместо собственного
обхода — структурная независимость docs/43 §5. Резолюция и capability-контракт
приходят как declared snapshot, а не обнаруживаются: verifier не смотрит ни в
текущий каталог, ни в сеть, ни в окружение.

Порядок проверки — тот, что фиксирует docs/43 §5: лимиты и счётчики таблиц →
схема и версии → идентичность источника → канонический порядок таблиц →
типы/импорты/capability-интерфейсы → CFG и типизированные операнды →
ownership/профиль/ресурсы → задачи, атомики и unsafe → source maps. Проверка
останавливается на первой primary-находке: последующая, читающая таблицу,
которую предыдущая уже отвергла, сообщала бы следствие, а не дефект.

Реализованы семейства `V2001`, `V2002`, `V2003`, `V2004`, `V2010`, `V2011`,
`V2012`, `V2013`, `V2020`, `V2022`, `V2023`, `V2030`, `V2032`, `V2033`,
`V2040`. Результат — либо `VerifiedModule` receipt, привязанный к digest'у
именно того модуля, который verifier обошёл, либо одна детерминированная
находка. Frontend не может пометить кэш проверенным: receipt выдаёт только
verifier.

**Verifier сразу нашёл два реальных дефекта lowerer'а**, которые ни один тест
frontend'а поймать не мог, потому что IR был внутренне непротиворечив: source
map выдавался в порядке первого использования, а не в каноническом порядке
docs/43 §2 (по source unit, затем по байтовому диапазону), и таблица функций
шла в порядке источника, а не по полному имени. Оба порядка теперь
канонизируются перестановкой с переотображением всех ссылок, а verifier
проверяет оба.

Negative-evidence: 19 тестов в `tests/integration/tests/pipeline.rs`. Каждый
берёт настоящий опущенный модуль и меняет ровно одну вещь — подделанная схема,
подделанная идентичность источника, нарушенный порядок таблиц, ссылка на тип
вне таблицы, цель перехода вне функции, операнд вне таблицы значений, capability
с типом скаляра, capability вне объявленного контракта, двойное перемещение,
Bootstrap с workers > 1, Bootstrap с await, непотреблённый child, недопустимый
atomic order, заявленный unsafe-интерфейс, source-map запись с чужой
идентичностью, таблица сверх опубликованного потолка. Плюс: валидный
frontend-IR проходит verifier по существу, а не по происхождению, и receipt не
совпадает с digest'ом модуля, который verifier не проверял.

### 2026-08-11 — Stage 2 §6 пункт 7: bounded Bootstrap reference interpreter

Новый crate `source/crates/tos-engine` исполняет **только** проверенный IR:
`run` принимает `VerifiedModule` receipt и сверяет его с digest'ом переданного
модуля. Receipt другого модуля — не receipt этого; пути, исполняющего IR,
который verifier не видел, не существует.

Корректность не опирается на host: ни Rust panic/unwinding, ни host exceptions,
ни ambient filesystem/network, ни libc, ни host threads. Целочисленная
арифметика проверяется по объявленной ширине типа TOS, а не по ширине типа
хоста — `i32 * i32` переполняется потому, что программа сказала `i32`, хотя
хост уместил бы результат. Каждое нарушение динамического предусловия — trap со
стабильным кодом и индексом source-map записи, поэтому runtime-ошибка называет
породивший её текст.

Реализовано: fuel-учёт (каждая исполненная операция и каждое back edge тратят
единицу объявленного бюджета), предел рекурсии из envelope, checked
арифметика с trap'ами на overflow/деление на ноль/недопустимый сдвиг, явный
control flow, вызовы и возвраты, агрегаты и варианты, `Result`/`Option` и `?`,
предопределённые `to_*` (как `Result`, а не молчаливое усечение) и
`wrapping_*`, детерминированное исполнение и учёт ресурсов.

18 сквозных тестов в `tests/integration/tests/execution.rs` проходят весь
production path без единого сокращения: SourceReader → Parser → Checker →
Lowerer → tos-ir/v1 → независимый Verifier → Reference Interpreter. Проверяются
как результаты (константы, порядок вычислений, ветвления, циклы, записи,
именованные конструкторы, вызовы, `match`, checked-конверсии), так и границы
(overflow по объявленной ширине, деление на ноль, недопустимый сдвиг,
исчерпание fuel в бесконечном цикле, предел рекурсии), а также отказ движка
исполнять модуль по чужому receipt'у и детерминизм повторного запуска.

Операции, которые этот движок ещё не исполняет — spawn/join/await/cancel,
атомики, capability-операции, resource-операции и cleanup — дают trap
`RUNTIME_OPERATION_NOT_IMPLEMENTED`, а не тихо неверный результат. Ни одна из
них пока и не опускается lowerer'ом, поэтому расхождения между слоями нет.

### 2026-08-11 — `/system/boot/init.tos` не является исходником TOS Core

docs/44 §6 пункт 10 требует, чтобы реальный `/system/boot/init.tos` проходил
обычный production path. Сегодня он не может — и это доказано тестом
`tests/integration/tests/init_boot.rs`, а не заявлено прозой.

Файл транспортно валиден (UTF-8, NFC, без BOM и одиночных CR), поэтому
SourceReader его принимает. Парсер отвергает его первым же не-модульным
символом: `E1013_UNEXPECTED_CHARACTER`. Байты — это Markdown с SPDX-заголовком в
XML-комментарии, а `.tos`-модуль обязан открываться
`module <name> version <v> profile <p>;` (docs/39 §3).

Это не дефект реализации. Файл сам себя описывает как **illustrative** boot text
капсулы Stage 1: nucleus читает его как текст, чтобы убедиться, что канонический
boot-файл резолвится, и вывести первую логическую строку в serial; ADR-0015
гейтит парсер на Stage 1.5. Превращение его в модуль TOS Core меняет boot text
капсулы Stage 1, который читает nucleus и на который завязаны QEMU-гейты, —
это архитектурное решение, а не инженерный шаг. Подделывать исполнение нельзя,
поэтому пункт 10 остаётся открытым до решения Project Architect.

Тест зафиксирован так, что он **упадёт**, когда файл станет модулем TOS Core, —
это напоминание обновить Stage 2 gate, а не молчаливое разрешение.

### 2026-08-11 — Разрешённое однократное переписывание истории (DCO)

Project Architect разрешил ровно одно переписывание опубликованной истории и
только ради одного: коммит `80bfcc1` ушёл в `origin/main` без trailer
`Signed-off-by`, которого требует docs/23, из-за чего `check-dco.sh` и весь
`preflight --full` падали на каждом прогоне и починить это без rewrite было
нельзя.

Trailer добавлен в `80bfcc1`; больше ничего. Девять затронутых коммитов
переиграны по порядку на неизменный `390c08a`, поэтому **все tree-хеши
побайтово совпадают с исходными** — это и есть доказательство, что вместе с SHA
не переехало никакое содержимое. Mapping старых SHA → новых и проверка
равенства деревьев зафиксированы в `PROVENANCE_HISTORY_REWRITE.md`.

Разрешение покрывает только эту починку. Резервная ветка
`dco-backup-pre-rewrite` оставлена локально до подтверждения.

### 2026-08-11 — Полное lowering принятого V1 surface

Все 38 принятых векторов теперь опускаются в `tos-ir/v1`; именованных `Gap` не
осталось. Добавлены: привязка payload'ов в arm'ах `match`, `defer`, `spawn`,
`join`/`await`, `cancel`, closures и вызовы через них, `async fn`, `for` и
`unsafe`.

**Вложенное тело — отдельная функция со своим return scope** (docs/43 §3), а не
встроенные блоки. То, что тело берёт из объемлющей области, становится явным
упорядоченным набором captures: ничто не попадает во вложенное тело через
ambient scope. Имена синтетические и детерминированные (`#closure@<offset>`), с
`#` вне пространства идентификаторов, поэтому столкнуться с объявленной
функцией они не могут.

**`defer`.** Регистрация — маркер `RegisterCleanup`; исполнение — `RunCleanups`
в точке каждого реально покидаемого выхода, со списком тел в обратном порядке
регистрации. Каждый вызов несёт операнды, которые читаются **там, где cleanup
выполняется**: именно это делает регистрацию не берущей владения, как требует
ADR-0035. Captures cleanup'а передаются как mutable borrow объемлющих
биндингов, поэтому то, что оставил один cleanup, видит следующий и сам scope.

**`async fn -> T` даёт `Task<T>`** (docs/40 §4): объявление опускается в одну
инструкцию — spawn ребёнка, несущего работу, и возврат его handle.

**Bootstrap сериализует `parallel`** (docs/43 §7), но сериализует **отложенно**:
ребёнок исполняется на join, а не на spawn. Это сохраняет смысл `cancel` —
отменённый до join ребёнок не стартует, и `Cancelled` остаётся исходом, который
docs/41 §2 допускает.

**Borrow-параметры исполняются copy-in/copy-out.** Правило эксклюзивности языка
гарантирует, что во время вызова живого альянса нет, поэтому копирование туда и
обратно наблюдаемо эквивалентно ссылке и не требует машинерии алиасов. Это
попутно исправило существовавшую ошибку: запись через `borrow mut` параметр
раньше терялась.

**Verifier снова нашёл два дефекта**, невидимых frontend-тестам: индексы тел
cleanup'ов не переотображались при канонической сортировке функций, а правило
task-scope считало потреблением только `join`/`await`. Теперь обязательство
путешествует с handle: `spawn` создаёт его, move или read места передаёт тому,
что инструкция производит, а join, await, возврат или передача дальше —
погашает. `cancel` по-прежнему не погашает.

Добавлено `PlaceStep::DynamicIndex`: анализ алиасов трактует его как любой
элемент, исполнение читает значение, которое он называет. `size`-арифметика
проверяется по ширине reference ABI, названной в движке явно, а не унаследованной
от хоста.

Тестов 360, `preflight --full` 31/31.

### 2026-08-11 — Stage 2 §6 пункт 8: identity plane и cache admission

Новый crate `source/crates/tos-cache` владеет идентичностью производных
объектов по docs/43 §6. Ключ содержит всё, изменение чего меняет смысл объекта:
content ID и **упорядоченное** dependency closure, source set, каноническое имя
и путь модуля, frontend, версию языка и feature revision, Unicode baseline,
схему IR и ревизию source map, verifier, backend и target ABI, политику
оптимизации/безопасности, digest resource envelope и digest capability-контракта.
Тест меняет каждое из 17 полей по одному и требует, чтобы ключ сдвинулся:
поле, которое ему не принадлежит, позволило бы переиспользовать устаревший
объект после изменения, изменившего его смысл.

Lookup **fail-closed**: объект, лежащий не под своим ключом (подстановка),
receipt другого модуля и отсутствующий source-map digest — все отвергаются, ни
один не откатывается на «похожий» источник или host-артефакт. Admission требует
receipt, поэтому кэш не может стать способом обойти verifier.

**Байтовый формат не вводится.** docs/43 §1 запрещает замораживать persisted
representation до появления bounded versioned format contract по docs/18,
поэтому crate определяет идентичность и правила допуска, но не хранение.

`RunningIdentity` замыкает цепочку docs/37 Stage 2: canonical source →
normalized identity → frontend identity → typed IR identity → verifier receipt →
engine identity → cache key. Тест проверяет каждое звено и затем действительно
исполняет то, что этой идентичностью названо.

Удаление всех объектов проверено отдельно: после `clear()` регенерация из того
же канонического источника даёт тот же ключ, тот же receipt и тот же результат —
кэш стоит работы, но не функциональности.

Тестов 368.

### 2026-08-11 — Stage 2 performance: P1, а не сфабрикованный PASS

Новый harness `source/tests/performance-core` измеряет ровно те две метрики,
которые docs/35 назначает bootstrap-профилю, плюс поведение при отказе по
квоте. Дискретизация docs/35: 3 прогрева, 21 выборка, сохраняются median/p95/p99
и сырые значения.

Результат на этой машине (P1, locally measured):

- parse + check + lower + verify модуля 256 KiB — p95 **148.5 ms** при бюджете
  500 ms;
- one-million-operation integer/control-flow benchmark — p95 **331.8 ms**;
- отказ по квоте для входа **того же размера** — p95 58.3 ms, отношение к
  принятому входу **0.398** при бюджете 2.0.

Отношение считается против сопоставимого входа: docs/35 ограничивает отказ
бюджетом *принятого* входа, поэтому отклоняемая фикстура — тот же модуль 256 KiB
с одним лишним ключом ресурса, а не крошечный файл. Первая версия сравнивала с
крошечным и давала бессмысленный 0.000.

Для benchmark'а отношение **не заявляется**: docs/35 формулирует бюджет
относительно «host reference interpreter time under the same semantic
implementation», второй такой реализации ещё нет, поэтому сохранено абсолютное
число, а сравнение будет сделано, когда она появится.

Уровень P1 снимает обе метрики с P0, чего docs/35 не допускает для метрик самой
стадии, но **не закрывает gate**: нужен объявленный reference platform, а эта
машина им не является. Утверждать бюджет отсюда было бы фабрикацией PASS.
Свидетельство сохранено в `docs/evidence/STAGE2_PERFORMANCE_P1.md` вместе с
окружением, toolchain и точной командой воспроизведения.

### 2026-08-11 — ADR-0038 принят и реализован; 0036/0037/0039 пересмотрены

**ADR-0038 — ACCEPT.** Правило реализовано: объявленные module roots ищутся по
порядку, кандидат в самом раннем root разрешает имя — это и есть layering
приватного root поверх общего. Порядок решает только roots. `E1605` покрывает
ровно то, о чём порядок молчит: одно и то же имя дважды внутри одного root, и
имя, предлагаемое несколькими достижимыми объявленными dependency source sets,
которые ничем друг относительно друга не упорядочены. Диагностика называет
столкнувшиеся идентичности. Три случая — driver-level: это свойства объявленной
раскладки source set, а не одного файла, поэтому они записаны в EXPECTATIONS как
D001–D003 и привязаны unit-тестами.

**ADR-0036 — revision 2.** Закрыты три пробела, названные Architect'ом.
Выделен `E1402_INVALID_GUARD_LIFETIME` (family E14xx) с полем `operation`,
покрывающий held_across_await, returned, aggregate, channel, task_boundary и
lock_outlived. Precedence задана явно: guard через границу task/closure — это
`E1402` с `operation=task_boundary`, а **не** E1304/E1305, поэтому двойной
диагностики нет. Определено отношение времён жизни: объект синхронизации обязан
пережить каждый выданный им guard; локальный move guard'а между биндингами lock
не освобождает — обязательство освобождения переезжает вместе с владением, а
освобождает bounded drop конечного владельца. Записано, что checker и
`V2031_SYNC` доказывают одно и то же правило независимо.

**ADR-0037 — revision 2.** Transfer/share модель исправлена. `Region<T>`:
non-Copy, immutable, Shareable, Transferable — владение переезжает ровно в одну
task, а совместное использование пишется явно через `share(region)` →
`Shared<Region<T>>`, а не получается неявным копированием affine handle.
`Region<mut T>`: non-Copy, mutable, не Shareable, не Transferable. **Оба
DMA-варианта консервативны**: не Shareable и не Transferable. Причина
зафиксирована в тексте: `Shared<T>` — это `Copy`, поэтому shareable
`DmaRegion<T>` транзитивно давал бы копии handle в нескольких tasks и обходил бы
ровно то правило, ради которого писался.

**ADR-0039 — revision 2.** `TaskResult<T>` удалён из nonconstructible: docs/39
§2 даёт `Completed` и `Cancelled` как predeclared constructors в expression
position, значит это обычное affine значение результата, которое источник и
должен строить; запрещено изготовление `Task<T>`, а не результата join'а.
`Shared<T>` добавлен: он возникает только через typed `share` contract. Guard'ы
названы как входящие в множество **после** принятия ADR-0036, а не предположены.

### 2026-08-11 — Performance: ADR-0040 и пара native/reference

**ADR-0040 (Proposed)** фиксирует Stage 2 reference platform как тот же
q35/qemu64/1-vCPU/256-MiB/TCG профиль, который уже обязателен для Stage 1, —
одна платформа на две стадии, уже загейченная, детерминированная и достаточно
медленная, чтобы уложившийся в бюджет результат был уложившимся везде. Он же
фиксирует чтение docs/35, данное Architect'ом: «host reference interpreter time
under the same semantic implementation» — это **native-host** прогон того же
`tos-engine` на том же commit, а не вторая реализация семантики. Метрика —
пара измерений и отношение reference/native.

Harness принимает `--profile native|reference` как **объявленный** аргумент и
записывает то, что ему сказали. Он никогда не заключает, что машина, на которой
он запущен, и есть reference platform: выбор платформы после просмотра числа —
это ровно то, что ADR-0040 предотвращает. `--baseline` передаёт native p95 в
reference-прогон, поэтому частное не показывается без измерения, из которого оно
получено.

Взята native половина пары (`docs/evidence/STAGE2_PERFORMANCE_P1.md`). Reference
половина **не взята**: для неё harness должен исполняться внутри профиля, и это
остаток работы. Gate открыт, бюджет из native-записи не утверждается.

### 2026-08-11 — Differential testing: N/A для текущего набора движков

docs/44 §3 требует cross-engine differential testing «for every supported
engine», §7 — чтобы каждый движок проходил одни и те же векторы. Поддерживаемый
движок один, поэтому требование выполнено вакуумно; это буквальное чтение, а не
послабление. Оно становится обязательным в момент появления второго
поддерживаемого движка. Второй движок ради знаменателя не создаётся.

### 2026-08-11 — STAGE2_GATE_EVIDENCE: consistency audit

Документ переписан так, что описывает **одно состояние на одном HEAD**. Прежняя
версия несла утверждения предыдущего прохода — про lowered subset, отсутствующий
cache, Full-конструкции как named gaps и старое число тестов — рядом с
отметками, что те же пункты уже resolved. Историческим статусам место в
PROGRESS, а не внутри candidate record; теперь их там нет.

### 2026-08-11 — ADR-0036 и ADR-0040 приняты; 0037/0039 доработаны

**ADR-0036 — ACCEPT.** Статус проставлен, реализация (guard-типы, lifetime
relation, `E1402`, правила `V2031_SYNC`, синхронизация docs/39–44 и conformance)
ещё не выполнена.

**ADR-0040 — ACCEPT после двух уточнений.** Убрано утверждение «уложился на TCG
— уложился везде»: запись доказывает conformance на объявленной платформе, а не
производительность на любом железе или эмуляторе; ценность фиксированной
платформы — сопоставимость прогонов, а не экстраполяция. Добавлен §1a: reference
measurement обязан исполнять **реальный** Stage 2 runtime/recovery path; запуск
`tos-engine` внутри произвольного Linux-гостя этот gate не закрывает, потому что
libc и host OS становятся зависимостью измеряемого пути — ровно та зависимость,
которую docs/44 исключает. Native host остаётся только baseline сравнения.

**ADR-0037 — revision 3.** `share` добавлен нормативно как predeclared operation:
type rule `share(T) -> Shared<T>` только для транзитивно immutable и Shareable
`T`; вызов потребляет affine аргумент, исходное имя после него moved-from;
операция verifier-visible (собственная IR-операция, не opaque helper) и
учитывается против лимита `shared`.

**Здесь ADR упирается в границу и останавливается.** Для `share(dma)` /
`share(mutable)` нет подходящего кода: в реестре **вообще нет** кода о
несоответствии типа аргумента — `E1210` про целочисленное согласие, `E1211` про
индекс, `E1212` про `as`, `E1222` про return. Заимствовать любой из них под
условие, которого он не описывает, запрещено, поэтому предложены варианты:
узкий `E1214_INVALID_SHARE`; общий `E1215_ARGUMENT_TYPE_MISMATCH`
(рекомендуется — закрывает дыру шире, чем `share`: сегодня и обычный вызов с
неверно типизированным аргументом кода не имеет); и явно отклонённое расширение
`E1210`. Выделение кода — versioned decision, поэтому ADR-0037 остаётся
Proposed.

**ADR-0039 — revision 3.** Убраны constructor-call и aggregate-literal формы.
Проверено на реальном frontend: `Event()`, `Task(1i32)` и `Mutex(1i32)` дают
`E1202_UNKNOWN_VALUE_NAME` — predeclared type не является value name, — поэтому
обещать для них `E1213` значило бы расширять грамматику ради диагностики на
форме, которая и так отвергается. Осталось ровно то, что V1 умеет выразить: `as`
с nonconstructible типом в операнде или в цели.

### 2026-08-11 — Обязательный audit: runtime independence

Проведён фактический audit (`docs/evidence/STAGE2_RUNTIME_INDEPENDENCE_AUDIT.md`),
без слепого переписывания.

**Найдено.** Production-код пяти Stage 2 crate'ов использует из `std` **только**
то, что лежит в `alloc`/`core`: `Vec`, `String`/`ToString`, `Box`, `BTreeMap`,
`BTreeSet`, `format!`, `vec!`, `mem::take/replace`. Ни одного обращения к `fs`,
`io`, `env`, `net`, `thread`, `time`, `process`, `sync`. Единственные
`std::fs`/`std::panic` — внутри `#[cfg(test)]` модуля `tos-core` (строки 1554,
1919, 2877; модуль начинается на 735). Это не случайность: source reader
принимает байты, резолюция читает только объявленный source set, verifier —
объявленный snapshot, движок детерминирован, кэш определяет идентичность без
хранения.

`ldd` показывает зависимость **host-бинаря** от libc/libgcc/ld-linux — это
свойство линковки под host target, а не кода; и оно реально: исполнение через
такой бинарь есть host execution и Stage 2 gate не закрывает. Рядом:
`x86_64-unknown-none` уже собирается и загейчен (nucleus — не динамический
исполняемый файл).

**Единственный настоящий пробел — allocator.** `alloc` требует
`#[global_allocator]`; nucleus его не имеет и `alloc` не использует; ни один
принятый документ механизма не называет. Это архитектурная граница, а не деталь
портирования: docs/41 §6 делает `allocation` учитываемым лимитом, владелец
памяти в Stage 2 до Stage 3 не определён, а интерфейс nucleus↔runtime для
гранта памяти не существует.

**STOP на границе.** Представлены три варианта: A — bounded arena, выдаваемая
nucleus'ом через существующий BootInfo-handoff, с `#[global_allocator]` поверх
неё (рекомендуется: не вводит нового понятия, делает аллокацию подотчётной,
ограниченная работа); B — отказ от allocator и переход на fixed-capacity
хранилища по потолкам docs/44 (переписывает почти всё и делает worst-case
постоянным); C — отложить до Stage 3 (не закрывает контракт; записан для
полноты, не рекомендуется). Выбор — за Project Architect; ADR по варианту A не
писался, чтобы не предвосхищать решение.

После решения конверсия механическая: `#![no_std]` + `extern crate alloc`,
`std::` → `alloc::`/`core::`, `#[panic_handler]` как halt (движок и так не
опирается на panic — каждый динамический отказ это `Trap`). Нового скрытого ABI
не появляется: ни libc, ни WASI, ни Linux personality, ни C ABI; единственный
новый интерфейс — грант памяти варианта A.

### 2026-08-11 — Runtime accounting: allocation, cleanup, workers

Реализовано с семантикой **fail-before-effect**: резервирование проверяется до
того, как происходит оплачиваемое им действие.

- **allocation** списывается там, где значение действительно строится
  (`Op::Aggregate`, `Op::Variant`), *до* конструирования: значение, которое не
  помещается в бюджет, не создаётся вовсе, поэтому trap — это отказ, а не отчёт
  постфактум. Стоимость — свойство формы значения (`CELL_BYTES` на ячейку плюс
  заголовок), а не представления на хосте, поэтому одна и та же программа на
  одном входе учитывает одинаковые байты на любом движке, принявшем это правило.
  Кадр освобождает списанное при возврате, поэтому ограниченная программа
  остаётся ограниченной сколько бы раз ни вызывала.
- **cleanup** списывается на `RegisterCleanup` (именно регистрации считает
  docs/41 §6) и освобождается там, где cleanup'ы выполняются.
- **workers** резервируется один контекст исполнения до первой инструкции;
  Bootstrap сериализуется, поэтому ровно один.

**`sync` и `shared` не метрируются и не заявлены.** Операции, которые их
потребляют — взятие блокировки и `share` — не существуют до реализации ADR-0036
и принятия ADR-0037. Заявлять метрику реализованной на том основании, что
envelope виден verifier'у, нельзя.

Отдельная находка: `cleanup: 0` не доходит до движка — verifier отвергает модуль
статически (`V2022_RESOURCE`), потому что число cleanup'ов на выходе известно
статически. Это более сильное место для отказа, а runtime-списание остаётся
позади него для того, чего статическая граница не видит. `workers: 0` тоже
недостижим из валидного источника: frontend отвергает такой envelope, поэтому
резервирование наблюдается через accounting, а не через trap.

Тестов 377.

### 2026-08-11 — E1213, E1215 и ADR-0041

**ADR-0039 принят и реализован.** `E1213_NONCONSTRUCTIBLE_TYPE` закрывает
последнее молчаливое принятие в type slice: приведение целого к `Task<i32>`
раньше проходило проверку типов. Precedence: capability → `E1502`, любой другой
нонконструируемый handle с любой стороны `as` → `E1213`, и только обычное
преобразование между value-типами доходит до `E1212`. Грамматика не расширялась:
`Event()` и `Task(...)` остаются `E1202`, потому что predeclared type не является
value name. `TaskResult<T>` из множества убран, `Shared<T>` добавлен, `AtomicU64`
включён нормально, а не через частный случай. Векторы R061–R063.

**ADR-0037: принята Option 2 — `E1215_ARGUMENT_TYPE_MISMATCH`** и реализована.
Это residual для разрешённого вызова: специализированные коды сохраняют свои
условия, а `E1215` покрывает то, чего не описывает ни один из них. Не catch-all:
неразрешённый callee — это resolution finding, у него precedence; при
неопределённом типе с любой стороны не сообщается ничего, потому что это была бы
догадка, а не расхождение.

**Реализация вскрыла дыру крупнее самого кода.** Typing-срез обходил только
`return`, присваивание и `let`. Expression statement, головы `if`/`while`,
subject `match` и последовательность `for` не типизировались вообще — то есть ни
один аргумент в этих позициях ничем не проверялся. Теперь типизируются:
используется значение или нет, на расхождение операндов это не влияет. Вектор
R064 намеренно ставит вызов в statement position, чтобы привязать именно тот
путь, который был непроверен.

**ADR-0041 принят** — контракт `RuntimeMemoryGrantV1`. Nucleus выдаёт Stage 2
runtime одну ограниченную область, и она — единственный heap backing store;
runtime не обнаруживает память сам. `BootInfo v1` не трогается: это
loader→nucleus контракт, на котором закрылся Stage 1, и расширять его ради
потребителя, которого тогда не существовало, значит менять устоявшийся контракт.
Зафиксировано различие двух лимитов: implementation heap capacity — это
`length` гранта, а `resource [allocation: ...]` — семантический бюджет самой
программы, и ни один отказ нельзя выдавать за другой. Зафиксирована дисциплина
отказа: исчерпание arena на валидном входе внутри опубликованных лимитов не может
быть обычным panic — либо fallible allocation, либо доказанная верхняя граница;
`alloc_error_handler` это assertion, а не реакция на валидный вход. Bump
allocator, необратимо текущий между операциями, не принимается без доказанного
lifetime-контракта: runtime, который нужно перезапускать ради возврата памяти, не
является recovery-оракулом.

### 2026-08-11 — Stage 2 production runtime переведён на `no_std + alloc`

Пять production crate'ов — `tos-core`, `tos-ir`, `tos-verifier`, `tos-engine`,
`tos-cache` — объявлены `#![no_std]` с `extern crate alloc` и собираются под
`x86_64-unknown-none`. Forked «no_std версии» не создавалось: одна и та же
реализация собирается и на хосте для тестов, и freestanding для реального
runtime.

**Ключевое наблюдение, изменившее порядок работ.** Библиотечный crate с
`#![no_std] + extern crate alloc` компилируется под freestanding target **без
global allocator** — allocator нужен только при линковке бинаря. Значит
конверсия не зависела от реализации ADR-0041 и была не в конце критического
пути, а в его начале. Прошлый план ставил её после аллокатора; это было
неверно, и порядок изменён.

Тестовый модуль остаётся host-программой по построению: `#[cfg(test)] extern
crate std;` и явный `use std::{format, vec};` внутри него. Семантика тестов не
менялась — те же 377 тестов проходят.

Попутно вскрылось, что в `no_std` нет автоматического prelude `alloc`, поэтому
несколько голых `vec![` в парсере молча опирались на std prelude; они
квалифицированы.

**Два новых механических гейта** (preflight теперь 33):

- `check-freestanding-runtime.py` — production-код пяти crate'ов не называет ни
  одной host-возможности (`fs`, `io`, `env`, `net`, `thread`, `time`,
  `process`, `sync`, `os`, `libc`, `extern "C"`), и каждый lib.rs объявляет
  `#![no_std]`. Второе обязательно: без него первое — лишь соглашение об
  именовании, потому что `std` всё равно был бы слинкован. Тестовые модули
  исключаются явно — host-харнесс по построению.
- freestanding build пяти crate'ов под `x86_64-unknown-none`. Именно сборка, а
  не проверка исходников, доказывает, что весь dependency closure свободен от
  `std`.

Это переводит утверждение runtime-independence из «доказано аудитом на бумаге» в
«проверяется на каждом прогоне». Остаётся линковка freestanding **бинаря** — там
и понадобится allocator по ADR-0041.

### 2026-08-11 — ADR-0041: `RuntimeMemoryGrantV1` и bounded heap с реальным reclaim

Новый crate `source/crates/tos-runtime` — единственный компонент Stage 2 с
`unsafe`; остальные четыре остаются `#![forbid(unsafe_code)]`.

**Grant — объявленный вход, а не находка.** `BoundedHeap::ungranted()` отвергает
любую аллокацию: runtime без гранта не имеет памяти, и притворяться иначе значило
бы завести тот самый ambient allocator, который ADR-0041 запрещает. Валидация
гранта — версия, ненулевой base, степень двойки в alignment, выравненность base,
переполнение `base + length`, минимальный размер — происходит до того, как
тронут хоть один байт.

**Форма выбрана осознанно: first-fit free list с boundary tags и немедленным
слиянием обоих соседей**, а не bump. ADR-0041 отказывает bump-аллокатору,
текущему между обычными операциями, и это правильно: runtime, который нужно
перезапускать ради возврата памяти, не является recovery-оракулом. Footer у
каждого блока существует именно для слияния с *предыдущим* соседом без поиска.

Доказано тестами, а не заявлено:

- освобождённая память действительно возвращается — половина арены выделяется,
  освобождается и выделяется снова;
- три освобождённых соседа сливаются в один блок, и вся арена снова выделяется
  одним куском;
- **тысяча циклов allocate/free с чередующимся порядком освобождения возвращает
  арену к её исходной раскладке блоков**, а не просто к нулевому in_use — это и
  есть свойство, отличающее долговечный аллокатор от протекающего;
- исчерпание отвечает отказом: живые аллокации остаются целы и освобождаемы;
- запрос с выравниванием больше grain отвергается, а не обслуживается неверно.

**Честная граница по infallible-путям.** `try_allocate` возвращает `None`, но
`GlobalAlloc` по контракту обязан вернуть null, который `alloc` превращает в
`handle_alloc_error`. Поэтому дисциплина — вторая из двух, допущенных ADR-0041:
арена размеряется выше **измеренной** границы, и `high_water()` существует
именно для того, чтобы эту границу измерять, а не предполагать. Само измерение
на реальном 256 KiB модуле — следующий шаг.

`GlobalHeap` использует raw cell вместо lock, и это записано на самом типе:
reference runtime однопоточен по построению, поэтому lock защищал бы от
вызывающего, которого не существует. Это первое, что придётся изменить, если
Full engine когда-нибудь поведёт этот аллокатор из нескольких контекстов.

Unsafe-инвентарь дополнен разделом Stage 2: девять деклараций, два общих
обязательства (обещание гранта и однопоточность), всё остальное проверяется
внутри. Тестов 386, preflight 33/33.

### 2026-08-11 — Allocator proof audit: три реальных дефекта исправлены

Review нашёл настоящие ошибки, а не стилистику. Все три подтверждены тестами,
которые сначала падали.

**1. Accounting портил учёт живых аллокаций.** Когда остаток блока слишком мал,
чтобы стать отдельным блоком, весь блок оставался занятым, но `in_use`
увеличивался только на `wanted`, а при освобождении вычитался `tag.size`.
Разница списывалась с *другой* живой аллокации, и арена выглядела пустее, чем
была. Исправлено: `occupy` возвращает размер, который блок реально удержал, и
учёт ведётся **целыми блоками вместе с тегами** — `committed`. Освобождение
возвращает ровно то, что взяла заявка.

**2. `high_water` не был верхней границей footprint'а арены.** Он считал только
выданный payload, то есть исключал теги каждого блока, округление до grain,
неотделённый остаток и дыры между живыми блоками. Размерять арену по нему было
неверно. Заменён на **`peak_extent`** — наибольший адрес, до которого арена
когда-либо доходила, минус base. Арена такого размера обслужила бы ту же
последовательность идентично, потому что выбор first-fit не зависит от того,
сколько региона лежит за ним. Метрика намеренно **не убывает** при
освобождении: граница, которая уменьшалась бы, занижала бы требование
повторного идентичного прогона, а граница обязана ошибаться вверх.

**3. Alignment: комментарий обещал over-allocation, код отказывал.** Для
`GlobalAlloc` нельзя предполагать, что dependency closure никогда не запросит
выравнивание сильнее grain — достаточно одного `repr(align(64))` типа.
Реализовано по-настоящему: каждая аллокация несёт фиксированный префикс, в
последнем слове которого лежит расстояние назад до заголовка блока. Один путь
`deallocate` обслуживает и обычную аллокацию, и ту, чей payload сдвинут ради
сильного выравнивания: указатель сам знает, где его блок, ничего не выводится из
адреса. Проверено на 16, 32, 64, 256 и 4096.

Цена префикса не спрятана: она попадает в `peak_extent`, то есть в ту самую
величину, по которой размеряется арена. Тестов 388.

### 2026-08-11 — Измерена граница implementation arena

`source/tests/arena-bound` прогоняет **весь** production path — SourceReader,
Parser, Checker, Lowerer, независимый Verifier, reference engine — над
каноническим модулем, заполняющим потолок docs/44 в 256 KiB, с bounded heap из
`tos_runtime`, установленным глобальным аллокатором.

```text
peak extent   54 408 096 байт = 51.89 MiB
committed     идентичен до и после измеряемого прогона
blocks        6 всего, 2 свободных
result        Int(I32, 3) — проверяется, а не отбрасывается
```

Это та самая измеренная граница, которой требует вторая дисциплина ADR-0041.
`peak_extent` уже включает теги каждого блока, округление до grain,
per-allocation префикс, неотделённые остатки и дыры под фронтиром — то есть это
граница, а не сумма запрошенных payload'ов.

**Прогон конвейера через кучу — заодно сильнейший тест самой кучи.** Сотни тысяч
нерегулярных пар allocate/free нагружают split, coalesce и переиспользование
далеко за пределами unit-теста, и любая порча проявилась бы неверным ответом, а
не прошедшим assert'ом. Ответ верный, блоков после прогона шесть, committed
вернулся к исходному — конвейер отдал ровно то, что взял, и арена не
раздробилась.

Граница закрывает случай **одного модуля на опубликованном потолке**. Source set
из нескольких модулей (docs/44 допускает closure до 256) требует собственного
измерения; число оставлено в evidence, а не зашито константой, чтобы размер
гранта оставался объявленным решением, а не магическим значением.

### 2026-08-12 — Post-Stage-2 interstage: Human Boot Observability / Boot Console

**Stage 2 остаётся CLOSED. Stage 3 не начат.** Это не стадия и не новая
системная функциональность: это межстадийный UX-slice поверх уже существующего
Stage 2 boot path. Ни один accepted contract не изменён — TOS Core V1,
`tos-ir/v1`, verifier contract, ownership/resource semantics, Boot ABI v1,
`RuntimeMemoryGrantV1`, cache/provenance identities, Stage 2 closure evidence и
performance budgets остались как были. Level 1 по docs/21: реализация
существующего контракта без изменения наблюдаемой семантики.

**Что заменено.** Статический `render_stage1_status()` (`TRUSTED BOOT
FOUNDATION / CAPSULE VERIFIED / SOURCE GIT / BOOT ABI V1 / STAGE 1`) больше не
существует. Он описывал состояние, которого система уже не в том месте
достигает, и был единственным human-facing screen'ом. Вместо него — три
раздельных уровня: `nucleus/src/framebuffer.rs` (bounded primitives + полный
printable-ASCII 5×7 glyph set), `nucleus/src/console.rs` (`BootConsole` —
только boot observability: clear, header, строка статуса, current operation,
success/failure, финальный экран) и `nucleus/src/boot_report.rs` (потребитель
pipeline-событий). Терминала, ввода, scrollback, ANSI и tty здесь нет и не
предполагается.

**Framebuffer как consumer, а не как второй канал.** Один факт — два
потребителя: нормативный serial event и best-effort картинка. Для стадий
конвейера это существующий `Trace`: `SerialTrace` заменён на `BootTrace`,
который сначала безусловно пишет `TOS.RUN.STAGE`, а затем — если консоль есть —
отдаёт то же событие в `ConsoleReporter`. `Trace::entering` по контракту
вызывается *до* стадии, поэтому строка `[ .. ]` появляется до работы: зависшая
стадия называет себя сама. Диагностика на экране читается из структурного
`Run`, а не парсится обратно из отрендеренного serial-текста.

**Точка доверия к framebuffer'у.** Консоль создаётся только после того, как
приняты (1) boot ABI record по сырым байтам, включая framebuffer tuple, адрес,
геометрию, byte pitch и поддерживаемый формат, и (2) memory map целиком. Ни
одна проверка не ослаблена ради более раннего вывода. Два уже доказанных факта
рисуются ретроспективно (`Boot ABI v1`, `Memory map validated`), всё остальное —
live.

**Успех и отказ.** После полного прохода reader → parser → checker →
resolution → lowering → verifier → engine экран очищается и показывает
существующий canonical Pyro (`assets/mascot/tos_ascii-art2.txt`, тот же
`include_bytes!`, та же provenance-запись, gate `check-embedded-artwork-
provenance` зелёный) и две строки: `Stage 2 runtime complete.` /
`System halted normally.` Это ровно то, что произошло; `TOS ready`,
`Welcome`, `starting shell` не пишутся, потому что это была бы неправда. При
отказе экран **не** очищается: успешные шаги остаются, отказавший шаг
становится `[FAIL]`, ниже — код и location, `Boot stopped.`, и Pyro не
появляется.

**Проверено.**

- `cargo test` — 38 host-тестов в integration lib (было 22): rendering safety
  (RGBX/BGRX, clipping, invalid framebuffer = no-op, запись за пределы буфера на
  1×1…320×200 с guard-байтами), state transitions (current → success,
  current → failure), «`[ OK ]` не рисуется до возврата шага», stage ordering
  против настоящего `tos_pipeline::execute`, final screen (лог заменён,
  canonical Pyro присутствует и доминирует, обе строки на месте), failure
  (Pyro нет, отказавший шаг виден, код/позиция взяты из структурного
  диагностика).
- QEMU: `run.sh --expect 33` PASS, `stage2-runtime.sh` PASS,
  `boot-module-failure.sh` PASS (exit 75, stage=check) — result codes и serial
  contract не изменились.
- Новый gate `host-tools/qemu-test/no-framebuffer.sh` (+ `--no-framebuffer` в
  `run.sh`, `-vga none`): два прогона на одном профиле, с адаптером и без;
  23 события `TOS.*` совпали полностью, кроме полей самой платформы в
  `TOS.BOOT.HANDOFF`. Это и есть доказательство того, что UI ничего не решает.
- Ручная визуальная проверка реального QEMU-кадра (screendump через monitor):
  success — Pyro во весь экран и две строки; failure — журнал сохранён,
  `[FAIL] Checking source`, `E1222_RETURN_TYPE_MISMATCH`,
  `system/boot/init.tos:30:12`, `Boot stopped.`

**Стоимость на boot path измерена, а не оценена словами.** Один и тот же
release-бинарь, один и тот же профиль q35/qemu64/TCG, окно
`TOS.BOOT.ENTRY` → `TOS.BOOTTEXT.PATH`, 5 выборок: с framebuffer'ом
80.8 / 91.4 / 81.2 / 84.0 / 89.4 ms, без него (`--no-framebuffer`)
68.6 / 74.7 / 72.0 / 69.6 / 69.0 ms. Консоль стоит ≈13 ms, и почти всё это —
одна очистка экрана 1280×800. На 16-МиБ workload'е ADR-0026 та же величина
теряется в шуме: 7 выборок до изменения 2632…2863 ms (медиана 2727), после —
2657…2770 ms (медиана 2702). Принятая метрика ADR-0026 — отношение
full/crypto p95, а не абсолют; Stage 2 budgets (ADR-0043/0045) меряются
host-side на reference path, где консоли нет вовсе.

**Найденный (не внесённый этим slice'ом) дефект.**
`host-tools/qemu-test/stage1-performance-conformance.sh` уже не проходит на
чистом `7cb4b04`: fixture `tests/performance/stage1_capsule_workload.py`
кладёт в capsule boot-текст `# Stage 1 performance fixture canonical boot
text`, который не является модулем TOS Core, поэтому Stage 2 boot path
корректно останавливается на `stage=parse` и выдаёт 75 вместо ожидаемых
харнессом 33. Проверено stash'ем рабочего дерева и прогоном того же capsule на
нетронутом HEAD. Этот gate не входит в `preflight.sh`; он требует отдельного
решения (fixture с настоящим TOS Core модулем либо явное `--expect 75`), и
намеренно не чинится здесь: это Stage 1/Stage 2 performance evidence, а не UX.

**Известная граница.** Boot console ненормативна: нормативны serial-события.
Финальный экран означает успешное завершение Stage 2 runtime и штатный halt, а
не готовую интерактивную ОС. `[ OK ]` не появляется раньше, чем факт
установлен; при переполнении экрана строки просто перестают рисоваться (никакого
скроллинга и никакой записи за границы), а полная диагностика в любом случае
остаётся на serial.

### 2026-08-12 — Stage 3 Phase 0: контракты предложены, реализация не начата

**Stage 2 остаётся CLOSED. Stage 3 production implementation не начат и не
авторизован.** Опубликован Proposed-набор контрактов, без которого Stage 3
нельзя начинать: `docs/superpowers/plans/2026-08-12-stage3-phase0-contracts.md`,
ADR-0048…0051, четыре interface-контракта в `docs/superpowers/specs/` и
предложенный текст угроз `docs/evidence/STAGE3_THREAT_ENTRIES.md`. Ни один
принятый контракт не изменён; Proposed-документы намеренно **не** добавлены в
`docs/SPECIFICATION_SOURCES.txt` — консолидированный вид несёт только принятую
авторитетность.

**ADR-0048 (Level 3) — где исполняется TOS Core.** Решение: CPL 3, собственное
адресное пространство, по экземпляру runtime на процесс, единственный край —
`SYSTEM_ABI_V1`. Альтернатива «всё в ring 0, изоляция через verifier» отклонена
не по вкусу: вытеснение изнутри интерпретатора потребовало бы контракта
yield/resume, то есть переоткрытия принятой семантики docs/40–41 и модели учёта,
против которой измерен бюджет ADR-0043. Следствия, которые нельзя обнаружить
потом, зафиксированы прямо в ADR: verifier перестаёт быть механизмом изоляции;
движок становится per-process derived artifact со своей identity; fuel перестаёт
быть механизмом справедливости; identity обязана называть, **кто** утверждает
каждое поле.

**ADR-0049** расширяет baseline ADR-0023 до прерываний и вытеснения: PIC
маскируется, только local APIC timer, вектора 0–31 сохраняют смысл, отказ в
ring 0 по-прежнему завершает загрузку, а отказ в ring 3 убивает один процесс.
**ADR-0050** сохраняет свойство ADR-0041 («runtime без гранта не имеет памяти»)
и добавляет множественность: frame allocator в nucleus, `owner`+`generation` в
гранте, очистка кадров перед повторной выдачей.

**ADR-0051 — найдено противоречие между принятыми документами, а не выбран
удобный вариант.** docs/11 показывает `manifest driver { … }` внутри TOS-кода,
docs/45 объявляет это нормативным образцом, а принятая грамматика V1 (docs/39,
ADR-0028) допускает шесть форм item'а — `resource`, `record`, `enum`, `const`,
`fn`, `extern` — и `manifest` среди них нет: **показанный в docs/11 исходник ни
одна принятая грамматика не разбирает.** Предложенное решение делит манифест по
авторитету: что модуль запрашивает — уже принятый V1-исходник и уже лежит в
проверенном IR (`CapabilityImport`, `resource_envelope`, imports, exports,
effects); что модуль **предоставляет** — это capability-запрос, где номинальный
тип capability и есть публикуемый интерфейс, потому что docs/37 прямо называет
провалом «textual manifest grants itself authority»; чем и как его
**супервизируют** — канонический текст супервизора в `/system/policy/`. Ни
изменения языка, ни изменения `tos-ir/v1` не требуется.

**Измерено, а не предположено.** Ранний вариант «манифест как record-значный
`pub const`, читаемый из IR» отклонён после проверки на реальном конвейере:
`pub const` записи с именованными аргументами сегодня разбирается, проходит
проверку типов, lowering, verifier и исполняется, но чтение его из функции
отказывает на lowering с `construct=unbound place`, а `Constant` в `tos-ir/v1`
скалярный и именованной таблицы модульных констант нет. Заставить это работать
означало бы менять закрытый Stage 2 контракт. Сам пропуск реален — принятая
форма объявления молча исчезает — и внесён в Phase 1 как реализация уже
принятого контракта, но ни один контракт Stage 3 на нём не стоит.

**Бюджет определён до измерения.** `IPC_V1` §8 фиксирует знаменатель
относительного бюджета docs/35 (p99 ≤ 8× in-process function call): это вызов
экспортированной функции TOS Core с одним 64-байтным параметром, тем же билдом
движка, в том же процессе, на платформе ADR-0040, с той же дисциплиной выборок.
Выбранный после первого измерения бенчмарк — это подгонка, а не доказательство.

**Дальше.** Phase 1 (multi-module source sets) не начинается, пока ADR-0048…0051
не подписаны. Правки принятых docs/11, docs/45, docs/34 и docs/35 намеренно
отложены до подписи: принятый документ не должен описывать непринятую границу.

### 2026-08-12 — ADR-0048…0051 приняты; контракты опубликованы

Project Architect подписал все четыре решения (2026-08-12). Оформлено принятие:

- ADR-0048…0051 переведены в `Accepted (Project Architect-approved)` с строкой
  approval;
- четыре контракта перенесены из `docs/superpowers/specs/` в
  `source/interfaces/system/` и внесены в `docs/SPECIFICATION_SOURCES.txt`;
  `check-interface-contract-authority` видит теперь 8 принятых контрактов вместо
  четырёх. Гейт при этом поймал реальную ошибку: он требует, чтобы строка
  статуса была **отдельной строкой целиком**, а не началом абзаца;
- отложенные правки внесены: docs/34 получил раздел X3.1…X3.10 (детализация
  trust boundary 7 с честными уровнями E1/E2/E3), docs/35 — определение
  знаменателя относительного бюджета IPC со ссылкой на `IPC_V1` §8, docs/11 —
  пример манифеста, переписанный в принятой V1-форме, docs/45 — разделение
  «что компонент объявляет о себе» и «как его супервизируют»;
- `docs/evidence/STAGE3_THREAT_ENTRIES.md` удалён после слияния: две копии
  одного нормативного текста расходятся.

Заодно исправлено накопившееся: `SPECIFICATION_SOURCES.txt` отставал и не
включал принятые ADR-0042, 0043, 0045, 0046, 0047 — они добавлены. ADR-0044
остаётся вне списка, потому что он всё ещё `Proposed`. Консолидированная
спецификация теперь собирается из 117 источников вместо 104.

`./scripts/preflight.sh` — PASS, 24 гейта.

### 2026-08-12 — Stage 3 Phase 1 начата: Task 0 закрыт, найдена вторая граница

План: `docs/superpowers/plans/2026-08-12-stage3-phase1-module-sets.md`. Перед
планированием измерено фактическое состояние многомодульности на `7b0847d`, а не
предположено:

- импорт модуля лоуверится в `tos_ir::Import` с пустым `module_content_id`;
- cross-module вызов лоуверится в `CallTarget::Imported` с типом `unit` —
  «single-module lowering knows the callee's name, not its signature»;
- движок на любом `Imported` вызове выдаёт trap `RUNTIME_UNRESOLVED_IMPORT`;
- конвейер передаёт verifier'у пустой `ResolutionSnapshot::default()`;
- `dependency_digest` — digest пустого списка.

То есть представление в `tos-ir/v1` для многомодульности **есть**, а шага,
который его связывает, нет. Это Level 1 — реализация принятого контракта.

**Task 0 сделан и упёрся в границу.** Модульный `const` — одна из шести принятых
форм item'а (docs/39), и docs/42 §1 дважды называет константы частью
межмодульной поверхности. При этом docs/40 нигде не определяет, чем константу
можно инициализировать и когда она вычисляется, а `tos-ir/v1` не может ни
назвать модульную константу, ни импортировать её: `Constant` — скалярный пул,
именованной таблицы нет. Сейчас объявление принимается и молча выбрасывается.

Сделано в рамках контракта: отказ теперь называет конструкцию —
`construct=module-level const` вместо `unbound place` / `unresolved value name`,
которые описывают внутреннюю структуру лоуверинга и отправляют читателя искать
опечатку во вполне корректном исходнике. Два теста: чтение константы даёт
именованный gap на стадии lower; объявленная и непрочитанная константа
по-прежнему не мешает модулю исполниться.

**Дальше не пошёл: это решение, а не багфикс.** Любое полное закрытие требует
либо сузить две принятые фразы docs/42, либо расширить `tos-ir/v1` — закрытый
Stage 2 контракт. Оформлено как **ADR-0052 (Proposed)** с тремя вариантами и
рекомендацией A (константы, инициализируемые литералами и конструкторами, с
подстановкой в точке использования: без фазы инициализации модуля, без
изменения IR). Phase 1 намеренно ограничена **импортом функций**, чтобы не
зависеть от ответа.

### Требуют решения Project Architect

**C. Contract gaps оформлены как ADR-0036…0039 (Proposed).** Четыре границы,
на которых реализация останавливалась, теперь описаны как узкие versioned
language decisions с точным текстом решения, а не угаданы в коде. Ни один из
них не реализуется до подписи Project Architect: в статусе стоит
`Proposed`, строка approval пустая, и подделывать её нельзя.

- **ADR-0036 — representation guard'ов.** docs/41 §4 выдаёт lock affine guard,
  но V1 type surface его не называет, поэтому checker не мог установить, что
  значение *является* guard'ом иначе как угадав по конструктору объекта, что
  ADR-0035 §3 запрещает. Предложены `MutexGuard<T>`, `ReadGuard<T>`,
  `WriteGuard<T>`, операции `lock`/`read`/`write` и правило, что освобождение —
  это bounded `drop` guard'а, а не операция `unlock`, оставляющая имя после
  освобождения.
- **ADR-0037 — Transferable для Region/DmaRegion.** Права региона живут в
  гранте, а V1 не даёт способа его записать. Предложено внести режим в тип
  (`Region<T>` против `Region<mut T>`) и зафиксировать таблицу
  Copy/shareable/mutable/Transferable; DMA-регион не Transferable ни в одном
  режиме.
- **ADR-0038 — precedence module roots и точное условие `E1605`.** Две фразы
  docs/42 §1 противоречат друг другу: упорядоченный список корней делает
  неоднозначность невозможной, но код для неё выделен. Предложено читать
  порядок как порядок поиска (первый корень выигрывает — это и есть layering),
  а `E1605` — как коллизию между несколькими *достижимыми объявленными*
  зависимостями, которую порядок не должен замалчивать.
- **ADR-0039 — `E1213_NONCONSTRUCTIBLE_TYPE`.** docs/40 §3 требует
  «corresponding nonconstructible-type error» для семи opaque-типов, но его не
  называет, поэтому приведение целого к `Task<i32>` сейчас принимается молча.
  Предложен код, список нонконструируемых типов и precedence
  `E1502` → `E1213` → `E1212`.

## Граница закрытого Stage 1

- Stage 1 — bootable trusted-source foundation, не shell/desktop, не Stage 1.5
  language runtime и не persistent Git implementation: заявлен только G0.
- Recovery selection/rollback остаются Stage 5; external IRQ/APIC policy,
  drivers и general platform support не заявляются Stage 1 deliverables.
- F-23 (dead `CapsError::PayloadOverlap`) и F-24 (`actions/checkout@v4`
  maintenance warning) — явные non-Stage-1 deferred maintenance items.
