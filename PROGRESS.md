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
  Project Architect approval заархивированы. **Stage 2 Part B авторизован**:
  ADR-0028 accepted. V1 surface contract фиксирует `[]`
  для data/declaration lists, `{}` только для executable blocks, `()` для
  arguments/grouping и explicit `return` без implicit tail values. Stage 2
  production implementation начинается с reference frontend; Stage 3 не начат.
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
`subset`, `translated` и foreign runtimes; docs/43 озаглавлен «TOS Core V1» и
описывает `tos-ir/v1` как IR именно этого языка; docs/03 говорит про
«versioned frontend contract». Ни один принятый документ не утверждает, что
исполняемый код обязан происходить из семантики TOS Core, и ни один не
запрещает будущей версии IR представлять foreign unsafe semantics. Граница
зафиксирована в module-документации `ownership.rs`, `flow.rs` и `place.rs`:
это semantic state фронтенда, доказательство, а не условие представимости
программы; изоляция процесса, capabilities и verifier — отдельный слой, не
зависящий от этих типов.

## Граница закрытого Stage 1

- Stage 1 — bootable trusted-source foundation, не shell/desktop, не Stage 1.5
  language runtime и не persistent Git implementation: заявлен только G0.
- Recovery selection/rollback остаются Stage 5; external IRQ/APIC policy,
  drivers и general platform support не заявляются Stage 1 deliverables.
- F-23 (dead `CapsError::PayloadOverlap`) и F-24 (`actions/checkout@v4`
  maintenance warning) — явные non-Stage-1 deferred maintenance items.
