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

## Граница закрытого Stage 1

- Stage 1 — bootable trusted-source foundation, не shell/desktop, не Stage 1.5
  language runtime и не persistent Git implementation: заявлен только G0.
- Recovery selection/rollback остаются Stage 5; external IRQ/APIC policy,
  drivers и general platform support не заявляются Stage 1 deliverables.
- F-23 (dead `CapsError::PayloadOverlap`) и F-24 (`actions/checkout@v4`
  maintenance warning) — явные non-Stage-1 deferred maintenance items.
