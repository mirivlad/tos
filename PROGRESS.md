# TOS — журнал прогресса (рабочий лог)

Рабочий файл владельца для фиксации «что сделано и проверено».
**Ненормативный.** Не заменяет CHANGELOG.md, ADR, отчёты по этапам и
docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md — это рабочий лог, а не документация.

Правила ведения:
- статус пункта меняется только вместе с записью в «Журнал верификации»
  (команда + результат);
- «сделано» = есть реальный прогон, не описание.

## Текущая позиция

- Базовая линия: TOS v0.2.1, коммит `c5b818c` (Establish TOS architecture baseline).
- Этап: Stage 0 завершён (архитектура/право, ADR-005 QEMU x86_64 first,
  ADR-006 Rust no_std). Идёт Stage 1: capsule v1 + boot ABI v1 + загрузчик +
  нуклеус + первый QEMU-прогон.
- Вся работа ведётся в `source/` (решение owner; docs/17-монобренч на корень
  приостановлен до Stage 1 — scope-решение, не изменение контрактов).

## Checklist Stage 1

| # | Пункт | Статус | Доказательство |
|---|-------|--------|----------------|
| 1 | Дерево `source/` по docs/17 | done | каталоги crates/, boot/, nucleus/, host-tools/, tests/, system/, interfaces/ |
| 2 | Спека CAPSULE_FORMAT_V1.md | done | source/interfaces/boot/CAPSULE_FORMAT_V1.md |
| 3 | Спека BOOT_ABI_V1.md | done | source/interfaces/boot/BOOT_ABI_V1.md (§6 serial boot-event log) |
| 4 | crates/tos-hash (SHA-256 no_std) | done | RFC 4231, 7/7 тестов |
| 5 | crates/boot-protocol (BootInfo v1) | done | 6/6 тестов (структура 224 B, map, containment) |
| 6 | crates/capsule: парсер (no_std, тотальный) | done | 11/11 lib-тестов; split flags, reserved 12 B, биекция, boot-canonical cross-check, licence tail |
| 7 | crates/capsule: host-билдер (feature="host") | done | детерминизм: builder == golden vector |
| 8 | crates/tos-serial (16550 COM1, no_std) | done | используется loader + nucleus |
| 9 | boot/uefi-loader (EFI app, рукописные биндинги) | done | PE32+ EFI app x86_64 собран, 0 warnings; release 14848 B |
| 10 | nucleus (freestanding, boot ABI v1, serial, halt-код) | done | raw-binary собран (entry первой, `sub rsp`); release 10520 B |
| 11 | system/boot/init.tos + NOTICES.txt | done | source/system/boot/ |
| 12 | host-tools/capsule (CLI-билдер) | done | регенерация векторов через него |
| 13 | Golden-векторы (13 .bin, коммитимые) | done | source/tests/vectors/capsule-v1/; valid-001 из реального init.tos + licence |
| 14 | tests/integration | done | 8/8 (golden, tamper, determinism, truncation, perf) |
| 15 | tests/fuzz (детерминированный мутационный) | done | FUZZ PASS rounds=300000 |
| 16 | Сборка всех таргетов (host + uefi + none) | done | host: 31/31 тестов; uefi loader: PE32+ EFI app 0 warnings; none nucleus: raw binary (entry first) |
| 17 | host-tools/qemu-test (ESP-образ + OVMF) | done | run.sh: identity gate `--git-commit HEAD`, manifest repo-relative |
| 18 | QEMU-прогоны: success + corrupted capsule | done | exit code **33 (HALT_OK)**; 9 negative-векторов → 67 (CAPSULE_INVALID) |
| 19 | scripts/check-spdx, check-dco | done | оба OK на всех tracked files / всех 11 коммитах |
| 20 | Архитектур-импакт-стейтмент (AGENTS.md §5, Level 2) | done | source/ARCHITECTURE_IMPACT_STATEMENT.md, коммит dc16726 |
| 21 | Stage 1 отчёт + identity record | done | source/interfaces/boot/STAGE1_REPORT.md |
| 22 | Коммит source/ + PROGRESS.md (DCO) | pending | текущие правки (raw OID, QEMU-фиксы) в рабочем дереве |

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

## Открытые вопросы / риски

- Нормативные документы не перестроены после правок интерфейсов (BOOT_ABI_V1.md,
  CAPSULE_FORMAT_V1.md — см. §2 AGENTS.md). Переносится в отдельный коммит
  вместе с `python3 tools/build-specification.py`.
- OVMF: нужен пакет/файл OVMF.fd для QEMU (проверить наличие на хосте).
- Подпись DCO: `mirivlad <mirvtop@yandex.ru>` (совпадает с git config и базовым
  коммитом c5b818c; вопрос про mir@yandex.ru закрыт — в историю не вносим).
