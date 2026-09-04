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
  без implicit tail values.
- **Stage 3 формально закрыт** (2026-09-03, коммит свидетельства `77970cb`;
  approval в `source/legal/publication-records/`). Основание —
  `docs/evidence/STAGE3_CLOSURE_AUDIT.md`: 60 обязательств, 56 CLOSED, 0
  блокирующих, 4 вынесены за Stage 3 принятым решением; P2 observer
  qualification и P2 абсолютная задержка IPC `p99 = 39.147 мкс` при бюджете
  `<= 200 мкс`. Текст ниже описывает Stage 3 как он строился.
- **Stage 4A — половина построена и зелёная, вторая половина упёрлась во второй
  STOP** (2026-09-04). ADR-0079 **Accepted** (Vladimir Tomashevskiy,
  2026-09-03). Построено и закрыто гейтом *QEMU textual PCI function claim*:
  platform root `platform.pci.Bus` (минтится launcher'ом, scope в записи),
  вывод `PciFunction` через операцию 24 с эксклюзивной привязкой и generation,
  механизм в ring 0 на CAM (`0xCF8`/`0xCFC`, из CPL 3 недостижим),
  `PLATFORM_INTERFACE_V1`, операции `SYSTEM_ABI_V1` 24–26, §2.1 — общее правило
  для hardware mechanism primitive. Канонический текстовый модуль держит root,
  забирает две реальные функции машины (00:04.0 и 00:05.0) и получает три
  разных отказа, которые сам решить не может: `TOS.RUN.COMPLETED value=i64:15`.
  **Не достигнуто:** чтение configuration space из текста. Операции есть и
  нуклеус их выполняет, но модуль не может их *объявить*:
  `SYSTEM_INTERFACE_V1` §4.1 требует, чтобы `uses` называл `import capability`
  модуля, а импорт `platform.pci.FunctionConfig` некому ответить на старте —
  единственный законный производитель это операция 24, которая выполняется
  позже. Это ровно тот вопрос, который ADR-0078 §6 оставил открытым.
  Поверхность решения — `docs/evidence/STAGE4A_HARDWARE_BOUNDARY.md` §7.
  Побочная находка: `LAUNCH_VERSION` и `BUNDLE_LAUNCH_VERSION` — одно
  пространство дискриминатора; версия 5 конфликтовала, поэтому `LAUNCH_VERSION`
  теперь 6.
- **Stage 4B формально закрыт** (2026-09-04, коммит свидетельства `ec03210`;
  approval в `source/legal/publication-records/`). Ruling Архитектора называет
  базис явно и столь же явно перечисляет, чего approval **не** даёт: ни IRQ, ни
  DMA, ни Virtqueue, ни block-I/O, ни reset. Ни один гейт Stage 1–4A не
  ослаблен; ни один порог не менялся ради closure. ADR-0081 принят: модель
  доступа к региону решена один раз для обычной памяти, DMA и device-памяти,
  с явно раздельными видами. Реализовано: индексный доступ `r[i]` для четырёх режимов Region/
  DmaRegion (это существующая семантика V1, а не новый API), TOS Core 1.2 с
  запечатанными типами `MmioRegion`/`MmioRegionMut` и восемью операциями
  доступа с явной шириной и порядком байт, собственные IR-операции
  `MmioRead`/`MmioWrite` (обычное чтение можно устранить или переупорядочить,
  доступ к устройству — нельзя), измерение BAR один раз при claim под
  эксклюзивностью Stage 4A, device-память как потомок assignment'а, и
  отображение страницы BAR как UC (`PCD|PWT`) без записи в PTE для read-only.
  Канонический текст прочитал реальный VirtIO common config:
  `num_queues=1 device_status=0x00 config_generation=0 queue0_size=256
  features=0x0101`. Восемь отказов отображения исполняются (255), доступ за
  границу окна отклоняется до обращения к устройству, без устройства проба
  сообщает отказ. Гейт механически проверяет, что слова VirtIO нет в ring 0.
  ADR-0075/0076 **не** расширены на device-память. Две находки governance
  остались в записи, а не были прощены: ADR-0081 §0 (реализация опередила
  approval) и §3a evidence-документа (красный QEMU workflow был настоящим
  независимым отказом и починен по причине). Закрытие переносит без изменений
  два обязательства: половину правила liveness `SYSTEM_ABI_V1` §6 — обязательное
  условие до первого routed device interrupt — и identity gate Stage 4, который
  не заявлен.
- **Stage 4B: языковой STOP** (2026-09-04), снятый ADR-0081 того же дня:
  TOS Core не умел читать через регион вообще. Построено и закрыто гейтом *QEMU textual VirtIO capability
  discovery*: канонический TOS Core 1.1 модуль обходит capability-list реального
  устройства через `pci_config_read`, находит все четыре современные VirtIO PCI
  структуры (common/notify/ISR/device, все в BAR 4, common cfg offset 0x0
  length 0x1000), читает у каждой BAR/offset/length и отказывается от
  некорректных цепочек с явной границей обхода. Нуклеус не получил ни строки
  кода; гейт механически проверяет, что слово VirtIO не встречается в ring 0.
  **STOP:** TOS Core не умеет читать через регион вообще — в `docs/39` §2 нет
  предопределённых `read`/`write`/`slice`, индексация региона не типизируется
  (`typing.rs::index_type`), `SYSTEM_INTERFACE_V1` §8 говорит, что ни одна
  операция не возвращает регион, а `docs/40` описывает контракты доступа,
  которых нет в грамматике. Плюс нигде не определена volatile-семантика.
  Это блокирует и Stage 4D (очередь VirtIO живёт в `DmaRegion`), поэтому выбор
  ABI-операций для MMIO вопрос не снимает. Поверхность решения —
  `docs/evidence/STAGE4B_MMIO_BOUNDARY.md` §6.
- **Stage 4A формально закрыт** (2026-09-04, коммит свидетельства `2655aaa`;
  approval в `source/legal/publication-records/`): канонический текст читает
  реальное PCI configuration space устройства под capability
  (vendor 0x1AF4, device 0x1042, class 0x01, subclass 0x00, cap ptr 0x98 — этих
  значений нет в тексте модуля, а без устройства та же проба даёт 0xFFFF),
  восемь из девяти негативов исполняются, подделанный скаляр отклоняет
  независимый verifier, ADR-0079 и ADR-0080 приняты. Approval закрывает **только**
  границу авторитета и чтение configuration space: ни BAR/MMIO, ни device-память,
  ни IRQ, ни DMA, ни IOMMU, ни reset, ни очереди, ни block-I/O, ни persistent
  storage, ни repository handoff. Дефект liveness `SYSTEM_ABI_V1` §6 переходит
  через закрытие без изменений и остаётся обязательным условием до первого
  routed device interrupt; identity gate Stage 4 не заявлен.
- **Stage 4A: первый архитектурный STOP** (2026-09-03), снятый решением
  Архитектора того же дня.
  Раунд — hardware boundary и PCI discovery; цель была узкой: канонический
  текстовый процесс читает configuration space реального `virtio-blk-pci` под
  capability. Железо не тронуто, ни один контракт не изменён, ни один gate
  Stage 1–3 не ослаблен. Основание — `docs/evidence/STAGE4A_HARDWARE_BOUNDARY.md`:
  у capability, называющей PCI-функцию, нет законного origin
  (`CAPABILITY_V1` §2 в редакции ADR-0075 §5 + ADR-0055), а два доступных
  механизма доступа к config space отвергаются **разными** принятыми
  клаузулами — ABI-операция противоречит букве `SYSTEM_ABI_V1` §2, а
  отображение ECAM противоречит операции 17, ADR-0075 §6 и ADR-0076. Поверхность
  решения — `docs/adr/0079-hardware-authority-origin.md` (**Proposed**), D1…D5.
  Рекомендация: D1-A + D2-M + D3-P3. Отдельная находка, не входящая в STOP:
  `nucleus/src/process.rs:1091` реализует только первую половину правила
  liveness из `SYSTEM_ABI_V1` §6 — это надо закрыть до первого routed device
  interrupt, а не после.
- **Как это было по ходу работы: Stage 3, в работе, не закрыт.** Реализация ведётся против
  принятых Stage 3 контрактов (`IPC_V1`, `CAPABILITY_V1`, `SYSTEM_ABI_V1`,
  `PROCESS_IDENTITY_V1`, ADR-0048…ADR-0073). На реальном freestanding пути
  работают процесс с `RuntimeMemoryGrantV1 = 54 MiB`, IPC, process
  create/exit/reclaim и оба latency-бюджета (P2). Host-side reference: путь
  build → `TOSBUNDLE/v1` → admission (ADR-0073), bounded residency (ADR-0071),
  compact image (ADR-0070). Формального closure approval для Stage 3 нет и он
  здесь не заявляется. Записи ниже, начиная с 2026-08-12, — журнал этой
  работы.
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
рекомендацией A. Phase 1 намеренно ограничена **импортом функций**, чтобы не
зависеть от ответа.

**ADR-0052 пересмотрен после вопроса Project Architect** — «рекомендация выходит
из лёгкости реализации или из того, что язык приятнее в использовании?».
Вопрос был по делу: первая редакция аргументировала A в том числе тем, что так
не придётся трогать `tos-ir/v1`. Это слабое основание для языкового решения.
Перепроверено с языковой стороны — рекомендация та же, но по другой причине и с
двумя изменёнными условиями. V1 **уже** решил, чем является константа:
`array<T, N>` требует «compile-time `size` constant» (docs/40), а
`const_primary` допускает `identifier`, то есть имя константы. Язык не может
одновременно считать `CAPACITY` compile-time значением в размере массива и
runtime-объектом во всех остальных местах. Отсюда: инициализатор — это
`const_expression` самого V1 плюс литералы и конструкторы, а не «только
литералы» (иначе теряется `const WINDOW: size = PAGE * 4`, законный и частый
паттерн); межмодульный импорт констант **не сужается**, потому что compile-time
константе не нужно представление в IR, чтобы пересечь границу модуля —
подстановку делает тот же source-set шаг, а зависимость уже покрыта
`dependency_digest`. Вариант B отклонён по архитектуре, а не по трудоёмкости:
фаза инициализации модуля исполняла бы код вне объявленного resource envelope и
вне receipt'а верификатора — двух свойств, на которых закрыт Stage 2.

Следующий вопрос — «то есть остался один вариант?» — вскрыл, что предыдущая
правка переусердствовала. Грамматика размеров массива доказывает, что
compile-time обязана быть **какая-то** константа, а не каждая: язык вправе нести
две формы (как `const`/`static` в Rust) либо одну runtime-форму с более узким
правилом для позиции размера массива. Поэтому B переписан в сильной редакции, а
не в виде чучела; C помечен как не-вариант, а не как наполнитель списка «для
баланса»; A получил условие, которого не хватало: **инициализатор не расширяется
позже**. Иначе будущая версия сможет разрешить вызовы в инициализаторе и тихо
изменить момент вычисления уже написанного исходника — текст тот же, смысл
другой. Живых вариантов два: A и B; выбор между ними — про то, готов ли TOS
исполнять код вне собственного учёта, а не про трудоёмкость.

**ADR-0052 принят (вариант A, 2026-08-12) и реализован.** Константа — значение
времени компиляции: инициализатор это константное выражение, использование —
подставленное значение. Следствия зафиксированы нормативно в docs/40 §2, код
`E1224_NONCONSTANT_INITIALIZER` внесён в таблицу docs/44. `tos-ir/v1` не
тронут: compile-time константа потребляется при lowering, как тип, поэтому ни
именованной таблицы констант, ни импорта констант в IR не появилось — и обещание
docs/42 §1 («imports … constants») выполняется подстановкой, без сужения.

Реализовано: новый slice чекера `constants` (11-й) отказывает по E1224 с полем
`reason`, различая `call`, `field-access`, `index`, `conversion`, `error-edge`,
`closure`, `spawn`; lowering подставляет инициализатор в точке использования — и
в позиции значения, и в позиции места, где константа материализуется в слот,
чтобы из неё можно было спроецировать поле. Цикл констант отказывается, а не
уходит в рекурсию: стек подстановки в lowerer'е делает проход тотальным даже
если его вызвали в обход чекера.

Шесть тестов в `crates/tos-pipeline/tests/constants.rs`: скалярная константа
исполняется (`i32:7`); константа от константы — `PAGE * 4i32` даёт `i32:16384`,
ровно тот паттерн, ради которого A взят в редакции `const_expression`, а не
«только литералы»; агрегатная константа читается по полю (`LIMITS.depth` →
`i32:8`); инициализатор с вызовом отказывается на стадии check с
`reason=call`; самоссылающаяся константа не исполняется; непрочитанная
константа ничего не ломает.

Ограничение зафиксировано честно: проекция, индексация и приведение запрещены
**внутри инициализатора** — не потому, что исполняются, а потому, что у них нет
своих compile-time правил; чтение поля константы в обычном коде это не
затрагивает. Межмодульная подстановка появится вместе с source-set шагом
(Task 2).

### 2026-08-13 — Phase 1 Task 1: source set доходит до конвейера

`execute_set(SetRequest { source_set, units, entry_path, entry })` принимает
набор модулей; `execute` теперь — его частный случай на один unit, поэтому все
существующие вызовы, включая boot path, не изменились. Это не заявление, а
тест: одиночный путь и набор из одного модуля дают одинаковые `module_digest` и
`content_id`.

Порядок стадий сохранён. Read читает каждый unit, Parse разбирает каждый, Check
прогоняет per-module проверки **с идентичностью модуля на каждом диагностике** —
без неё строка и колонка в наборе ничего не значат, — а Resolve отдаёт набор в
`check_module_set`. Оказалось, что фронтенд к многомодульности готов лучше, чем
конвейер: `E1604_IMPORT_NOT_FOUND`, `E1606_IMPORT_CYCLE` и расхождение
имя/путь уже реализованы, их нужно было только начать вызывать.

**dependency_digest перестал быть digest'ом пустого списка.** Замыкание
считается обходом в ширину от entry по импортам в порядке исходника, то есть
детерминированно; digest берётся по достижимым зависимостям, entry исключён.
Два набора, различающиеся только зависимостью, дают разные module digest — это
и есть точная инвалидация кэша. Недостижимый модуль не меняет ничего.

**Одна ошибка проектирования поймана контрактом, а не тестом.** Сначала я сделал
«entry-модуля нет в наборе» вариантом `Run` и отрендерил как
`TOS.RUN.REFUSED stage=resolve reason=…`. Принятый `RUNTIME_OBSERVABILITY_V1` §4
требует для `stage=resolve` поле `count=` и предшествующие `TOS.RUN.DIAGNOSTIC`,
так что событие было бы неконформным. Причина глубже формы: это вообще не отказ
чьего-то исходника, а ошибка запроса. Теперь `execute_set` возвращает
`Result<Run, SetError>`, проверка идёт **до** объявления первой стадии, и тест
требует, чтобы при ней не было объявлено ни одной стадии.

`path=` на `TOS.RUN.REFUSED stage=read` — добавленное поле по правилу расширения
§2 контракта (обязательные поля не тронуты) и задокументировано в нём: в наборе
смещение без имени unit'а не называет ничего.

Десять тестов в `crates/tos-pipeline/tests/module_sets.rs`. Cargo test по
воркспейсу: 527 пройдено, 0 упало.

### 2026-08-13 — Phase 1 Task 2: идентичность зависимости перестала быть заглушкой

Две вещи, которые lowering одного модуля знать не может и не должен выдумывать:
идентичность модуля, в который разрешился импорт, и тип вызова через границу.

`lower_module_in_set(source, schema, context, &[ResolvedImport])` — новая точка
входа; `lower_module` стал её частным случаем с пустым списком, поэтому все
существующие вызовы (тесты, performance-харнессы) не изменились.

**`Import.module_content_id`** связывается с content id модуля, который
действительно разрешился. Если зависимость не передана, поле остаётся **пустым**,
а не правдоподобным: верификатор должен уметь отличить «не разрешено» от
«разрешено вот в это». Тест закрепляет обе стороны.

**Тип cross-module вызова** — объявленный результат вызываемого, а не `unit`.
Это была не косметика: фронтенд сообщал верификатору неправду о том, что
возвращает программа. Типы переносятся между таблицами модулей рекурсивным
re-intern'ом, номинальная идентичность при этом сохраняется, потому что
`TypeDef::Nominal` несёт content id объявившего модуля — тест на `record Pair`
проверяет именно это. Вызов имени, которого зависимость не экспортирует, теперь
именованный lowering gap, а не `unit`, принятый на веру.

Замыкание лоуверится **зависимостями вперёд**, детерминированным
depth-first post-order (Task 1 использовал BFS; для lowering нужен порядок, в
котором модуль идёт после всего, что импортирует). Каждый модуль набора получает
свой собственный dependency digest — по своему замыканию, а не по замыканию
entry.

Наблюдаемый эффект: набор из двух модулей с настоящим вызовом через границу
проходит read → parse → check → resolve → lower → **verify** и упирается только
в движок: `RUNTIME_UNRESOLVED_IMPORT`. Верификатор принял IR, то есть типы
сошлись. Это ровно то состояние, которое перевернёт Task 4.

Тесты: 4 на уровне IR в `crates/tos-core/tests/module_sets.rs` (identity,
отсутствие выдумки, тип вызова, номинальная идентичность) и 3 добавленных в
`crates/tos-pipeline/tests/module_sets.rs`. Cargo test по воркспейсу: 534
пройдено, 0 упало.

### 2026-08-13 — Phase 1 Task 3: verifier видит набор, который судит

До этого конвейер передавал верификатору `ResolutionSnapshot::default()` —
пустой. Верификатор проверял только entry-модуль, а зависимости не видел вовсе.

Теперь снимок строится из **фактически залоуверенных** модулей, а не из
запроса: снимок, собранный из того, что попросил вызывающий, позволил бы
верификатору подтвердить его же собственное предположение. Верифицируется
**каждый** модуль замыкания: зависимость, чей IR верификатор не видел,
исполнялась бы по receipt'у своего вызывающего, а receipt — утверждение об
одном модуле.

`ResolutionSnapshot` получил поле `exports` — имена функций, которые
предоставляет каждый модуль. Не сигнатуры: сравнение сигнатур означает
сравнение типов между таблицами двух модулей, это отдельный и больший вопрос, и
заявить, что он решён, было бы хуже, чем не решать его.

Две новые проверки верификатора:

- импорт, заявляющий `module_content_id`, с которым снимок не согласен,
  отклоняется (`V2012_IMPORT`). Фронтенд сообщает, во что разрешился импорт;
  снимок сообщает, что набор реально предоставляет; расхождение — заявленное
  разрешение, которого не было, а верификатор существует ровно для того, чтобы
  слово фронтенда не было последним;
- вызов имени, которого разрешённый модуль не экспортирует, отклоняется —
  раньше это ловил только lowering, то есть тот же фронтенд.

Пустой снимок по-прежнему оставляет резолюцию без суждения: молчание — не
принятие утверждения, которое нечем проверить.

Пять тестов в `tests/integration/tests/pipeline.rs`, все на **вручную подделанном
IR** — верификатор, который видит только то, что произвёл фронтенд, от фронтенда
не независим. Cargo test: 540 пройдено, 0 упало.

### 2026-08-13 — Phase 1 Task 4: движок держит замыкание; вызов через границу работает

`run_set(&[Verified { module, receipt }], entry, name, arguments)` — движок
разрешает `CallTarget::Imported` **по набору и только по нему**: он не грузит,
не ищет и не выдумывает модуль. `run` стал его частным случаем на один модуль.

Результат: `math.double(21i32)` из boot-модуля возвращает `i32:42`, пройдя весь
reference path через настоящую границу модулей.

**Дисциплина receipt'ов сохранена и усилена.** Все receipt'ы проверяются **до
того, как что-либо исполнится**, а не в момент, когда до модуля дойдёт вызов:
иначе программа выбирала бы, какие модули проверяются, выбирая ветку. Тест на
это отдельный — модуль, который никогда не вызывается, с подделанным receipt'ом
отклоняет весь прогон.

**Правильного имени недостаточно.** Набор, содержащий другую ревизию модуля под
тем же именем, отклоняется: вызывающий был залоуверен и верифицирован против
конкретной идентичности, и исполнение против другой означало бы выполнение кода,
с которым его никогда не проверяли.

**Один прогон — один бюджет.** docs/41 §6 допускает вызов только когда
объявленный контракт вызываемого укладывается в envelope вызывающего, поэтому
прогоном управляет envelope entry-модуля, а пересечение границы не способ
получить второй бюджет. Тест это фиксирует: fuel limit — entry'шный, работа
вызываемого списывается на него, глубина растёт как у обычного вызова.

**Найдена и закрыта ловушка с диагностикой.** `Trap.source` — индекс в карте
исходников *того* модуля, который упал. Trap, ушедший за границу и разрешённый
по карте вызывающего, назвал бы существующую строку в чужом файле — это хуже,
чем не назвать ничего. Теперь trap несёт собственную разрешённую запись
(в `Box`, чтобы редкое поле не утяжеляло каждый `Result` движка), а конвейер
считает позицию по тому source unit'у, который названа записью. Тест: деление на
ноль внутри зависимости локализуется в `system/lib/math.tos`.

Тесты: 15 в `crates/tos-pipeline/tests/module_sets.rs` (включая перевёрнутый
`a_cross_module_call_runs_and_returns_the_callee_answer`, который до Task 4
фиксировал границу) и 5 новых в `tests/integration/tests/execution.rs`. Cargo
test: 546 пройдено, 0 упало.

### 2026-08-13 — Phase 1 Task 5: набор модулей загружается на реальном boot path

Nucleus собирает набор из **всех** `.tos`-файлов капсулы, а не только из
канонического boot-текста. `/system/version` и NOTICES модулями не считаются:
предложить их фронтенду значило бы попросить разобрать файл, который никогда не
заявлял себя модулем. Количество ограничено константой ядра
(`MAX_BOOT_MODULES = 64`), а не числом из капсулы — ядро не размеряет массив по
значению, которое выбрал кто-то другой; капсула сверх лимита отклоняется, а не
усекается молча.

**Доказательство на настоящем железном пути.** Новый gate
`host-tools/qemu-test/module-set.sh` собирает detached-капсулу из фикстуры
`tests/vectors/module-set/` — boot-модуль импортирует `system.lib.arith` и
возвращает `arith.double(21i32)`. В QEMU, на обычных артефактах, прошивке и
профиле машины: `modules=2`, все семь стадий в порядке, `value=i32:42`,
`depth=2`, exit 33. Ни один модуль капсулы не вычисляет 42 сам по себе — значит
ответ и есть доказательство, что вызов пересёк границу. Gate добавлен в
`preflight --full`.

Serial-словарь не изменился: `modules=` дописано к `TOS.RUN.BEGIN` по
собственному правилу расширения контракта и там же задокументировано — `path`
называет entry, а прогон, разрешивший набор, исполнил больше.

Канонический boot path не затронут: `value=i32:240`, теперь с `modules=1`.

**Найдено по ходу.** Первая версия строила вектор units **до** того, как куча
приняла грант, — и загрузка упала в `TOS.PANIC`. Это ADR-0041 работает как
задумано: runtime без гранта не имеет памяти. Исправление — собирать набор после
adoption, там же, где происходят все остальные аллокации прогона.

### 2026-08-13 — Phase 1 Task 6: граница арены для замыкания измерена отдельным числом

`cargo run --release -p tos-arena-bound -- --closure` меряет **один прогон по
целому замыканию**: все модули прочитаны, проверены, разрешены, залоуверены,
верифицированы и исполнены вместе, причём entry вызывает каждую зависимость —
чтобы они были достигнуты, а не просто присутствовали.

| Замыкание | Peak extent |
|---|---|
| 2 модуля | 41 312 B (0.04 MiB) |
| 4 модуля | 83 552 B (0.08 MiB) |
| 8 модулей | 152 880 B (0.15 MiB) |
| 16 модулей | 309 280 B (0.29 MiB) |
| 32 модуля | 620 752 B (0.59 MiB) |

Наклон ≈ 19.3 KiB на модуль, ряд линеен по всему диапазону: замыкание не стоит
сверхлинейно от числа модулей.

**Меряется в отдельном процессе, и это не удобство.** Frontier арены никогда не
опускается. Замыкание, измеренное *после* прогона с 256-КиБ модулем, отчиталось
бы его высшей отметкой и назвало её своей; измеренное *до* — оставило бы свои
освобождённые блоки под опубликованной одномодульной границей. Два числа, каждое
из которых должно быть своим, требуют двух арен, а арена принимается однажды.
Существующие цифры не тронуты.

**Что не измерено — сказано там же, где число.** Это много *маленьких* модулей,
а не замыкание из модулей потолочного размера: один 256-КиБ модуль стоит
50.47 MiB сам по себе, и 32 таких — другое измерение, которое не проводилось.
Число модулей и размер модуля здесь меряются по одному, их произведение не
заявляется. 32 — там, где ряд останавливается, а линейный наклон не является
доказательством того, что дальше.

**Phase 1 закрыта.** Все шесть задач выполнены.

### 2026-08-17 — Stage 3 Phase 2 начата: Task 1, ядро владеет физическими кадрами

План: `docs/superpowers/plans/2026-08-17-stage3-phase2-first-process.md`. Перед
планированием измерено фактическое состояние границы изоляции на `82644ec` — по
коду, а не по памяти о нём: ядро работает на страничных таблицах прошивки (`CR3`
не пишется нигде в `source/`), GDT несёт пять дескрипторов без единого
пользовательского, `TSS.rsp0` — ноль, `syscall` не заведён (`EFER.SCE`,
`IA32_STAR`, `IA32_LSTAR`, `IA32_FMASK` не программируются), любое исключение
фатально, аллокатора кадров нет, а движок исполняется в CPL 0 на стеке ядра.
То есть «граница ядро/runtime» Stage 2 — граница **полномочий**, ровно как и
написано в шапке `runtime.rs`. Phase 2 делает её границей железа.

**Task 1 сделан: аллокатор кадров — ADR-0050 §1.** Новый крейт `tos-frames`
принимает свободные пролёты валидированной карты минус всё занятое и с этого
момента является единственным в системе, кто решает, какая физическая память
занята. Вычитание не ослаблено: образ ядра вместе с `.bss`, капсула, handoff-
запись, конвертированная карта, фреймбуфер и текущий стек вычитаются так же, как
их вычитала деривация Stage 2 — память, которую процесс мог бы перезаписать, не
защищена тем, что чья-то бухгалтерия верна.

**Два выхода, и это не одно и то же.** `allocate_frame` выдаёт по одному кадру
4 КиБ — из этого будут строиться адресное пространство, таблица страниц и
per-process грант. `carve` берёт физически непрерывный прогон и существует для
тех немногих структур, которые обязаны быть непрерывными, потому что их ещё
никто не отображает; на загрузке так делается грант кучи. Carve **никогда** не
удовлетворяется из освобождённых кадров: чтобы это пообещать, пул должен был бы
дефрагментировать, а обещание, которого никто не реализует, хуже отсутствующего.

**Очистка — там, где её честно заявлять.** Освобождённый кадр очищается при
освобождении (ADR-0050 §3) и ещё раз перед выдачей, поэтому кадр чист и когда
пришёл из списка, и когда пул отдаёт его впервые: кадр, несущий то, что оставила
прошивка, — канал раскрытия без владельца. Carve не очищается и говорит об этом:
его единственный вызывающий на загрузке берёт память, которой не видел ни один
процесс, а очистка 96 МиБ на каждой загрузке купила бы только секунды. При
возврате прогон уходит через путь освобождения и очищается там — то есть ровно в
тот момент, когда его мог бы увидеть другой владелец.

**Найдено по ходу.** Два занятых пролёта, заканчивающихся по одному адресу,
предлагают кусок после себя **дважды**. Деривация Stage 2 этого не замечала,
потому что брала максимум; пул, принявший такой кусок дважды, выдал бы одну и ту
же физическую память двум владельцам — единственная ошибка учёта, которую этот
аллокатор не переживает. Дубликат отвергается по совпадению начала, и на это
есть тест.

**Грант остался V1.** ADR-0050 §2 прямо оставляет V1 в силе для ядра, выдающего
память одному runtime без процессной подложки, поэтому `owner` и `generation`
появятся вместе с процессом, которому они нужны, а не раньше. Свойство ADR-0041
не тронуто: `GlobalHeap` по-прежнему отказывает во всякой аллокации, пока грант
не принят.

Тесты: 16 в `crates/tos-frames/tests/frames.rs`, все на **настоящей** памяти —
пул admit'ится над живым выровненным хостовым выделением, поэтому проверяются те
самые записи указателей, которые аллокатор действительно делает (связи списка
внутри свободных кадров и очистка), а не его собственная арифметика. Свойства
гранта, которые доказывались в `tos-runtime/tests/region.rs`, переехали туда, где
теперь принимается решение; `region::derive`/`largest_free` удалены, а не
оставлены вторым способом получить грант. Cargo test: 554 пройдено, 0 упало.
`./scripts/preflight.sh` — PASS, 24 гейта. QEMU: канонический boot path даёт
`value=i32:240`, module-set — `i32:42`, оба на обычных артефактах.

### 2026-08-17 — Phase 2 Task 2: ядро взяло собственное адресное пространство

До этого коммита ядро работало на страничных таблицах, которые оставила UEFI:
identity map, который оно никогда не писало, не проверяло и не могло описать.
Пока в системе ровно один контекст исполнения, это терпимо; на ADR-0048 —
перестаёт быть терпимым, потому что именно таблицы страниц, а не верификатор,
удерживают один процесс от памяти другого. Границу, которой ядро не владеет, оно
не может и обеспечивать.

`paging.rs` строит четырёхуровневое дерево из кадров пула и по валидированной
карте — ни одна прошивочная таблица при этом не читается — и загружает `CR3`.
Отображение identity: ядро слинковано по физическому адресу и туда же положено
загрузчиком, поэтому ядро, переехавшее собственным текстом из-под себя, обязано
было бы быть перемещаемым, чтобы пережить эту инструкцию. Ring 3 — отдельный
вопрос и отдельное пространство.

**Чего в этом пространстве намеренно нет.**

- **Физической страницы ноль.** Разыменование нуля обязано падать, а падать оно
  может только если страницы нет. Стоит это одной таблицы.
- **Отображения, одновременно писуемого и исполняемого.** Образ разрезан по
  границам секций: текст — read-only + executable, всё остальное — writable +
  NX. Границы `__tos_text_end` и `__tos_rodata_end` в linker script выровнены на
  страницу, потому что право — свойство страницы: текст, деливший страницу с
  данными, был бы либо тем, либо другим, и оба ответа неверны. `CR0.WP`
  выставлен: без него запись из ring 0 игнорирует read-only, и разрез был бы
  декоративным.
- **Пользовательского отображения.** Ни одна запись не несёт `U/S`.
- **Несуществующей памяти.** Отображается только то, что описала карта, плюс
  объявленный загрузчиком фреймбуфер — он же единственный, кто получает
  uncacheable-режим, потому что кэшированная запись в память устройства это
  запись, чьё прибытие никем не обещано.

**Доказательство — отказ железа, а не чтение таблицы обратно.** Два новых
негативных гейта на изолированных тестовых сборках нуклеуса:
`exception-injection.sh paging` — чтение нулевой страницы даёт
`vector=14 error=0x0 cr2=0x0`; `exception-injection.sh readonly-text` — запись в
собственный текст даёт `vector=14 error=0x3 cr2=0x2000000`, то есть
protection violation, а не отсутствие страницы. Второй гейт проверяет
*конъюнкцию* двух вещей — read-only отображения и `CR0.WP`; при отсутствии любой
из них запись прошла бы молча, а это ровно тот отказ, которого дамп таблиц не
видит. Оба гейта внесены в `preflight --full`.

**Измерено, а не оценено:** на профиле ADR-0040 (256 МиБ) дерево стоит **22
кадра** (88 КиБ) при пуле в 59 051 кадр. Постоянного события об этом на serial
нет намеренно: `BOOT_ABI_V1` допускает между своими идентификаторами только
идентификаторы принятого versioned-контракта, а контракта про память ядра пока
нет; отчёт появится вместе с работой по identity процесса (`PROCESS_IDENTITY_V1`
§6), а не в виде нового namespace, придуманного по дороге. Число выше — разовое
измерение на этом коммите, и названо именно так.

Что не изменилось: `value=i32:240` на каноническом пути и `i32:42` на
module-set, консоль по-прежнему рисуется (её запись во фреймбуфер происходит уже
*после* переключения `CR3` — не упади она, гейта бы и не потребовалось),
загрузка без фреймбуфера по-прежнему проходит. `./scripts/preflight.sh --full` —
PASS, 39 гейтов.

Заодно исправлено: `MANIFEST.txt`/`SHA256SUMS` не были перегенерированы в
коммите Task 1 — план фазы правился уже после прогона гейта. Перегенерированы
здесь.

### 2026-08-17 — Phase 2 Task 3: ring 3 существует, и его край — единственный

`SYSTEM_ABI_V1` §3 обещает вещи про процессор — что сохраняется при вызове, что
разрушается, куда попадает управление. Свидетель такого обещания только один:
сам процессор. Поэтому задача закончена не тем, что код написан, а тем, что
полезная нагрузка **исполнилась в CPL 3**, сделала два вызова и сверила
регистры.

**Что теперь стоит на каждой загрузке.** GDT получил пользовательские
дескрипторы, и их порядок не свободен: `sysret` берёт `SS` из
`IA32_STAR[63:48] + 8`, а `CS` — из `+ 16`, так что данные обязаны лежать прямо
перед кодом. Это зафиксировано `const _: () = assert!(...)`, чтобы перемещение
дескриптора ломало сборку, а не давало загрузку, возвращающуюся в ring 3 через
не тот дескриптор. `TSS.rsp0` указывает на отдельный стек ядра: процессор читает
его при каждой смене привилегий, и TSS с нулём в `rsp0` превратил бы первый же
отказ процесса в тройную ошибку, которая никому ничего не сообщает. `EFER.SCE`,
`IA32_STAR`, `IA32_LSTAR`, `IA32_FMASK` запрограммированы; `int 0x80` входом не
является — механизм один, значит и путь аудита один.

**Точка входа написана на ассемблере целиком.** Всё, что контракт обещает про
регистры, обещает этот stub: он уходит со стека, который выбрал процесс, **до**
первого обращения к памяти, кладёт шесть аргументов в том порядке, в каком их
называет §3, и возвращает нетронутыми пять из шести — `rdx` не восстанавливается,
потому что `rdx` это результат.

**Диспетчер отказывает точно, а не правдоподобно.** Неназначенный номер —
`E_NOT_SUPPORTED`, и вызвавший остаётся исполнимым (§7 запрещает убивать за
вопрос). Любая операция, которой нужен capability, — `E_NO_CAPABILITY`, и это не
заглушка, а истина: таблицы capability ещё нет, значит держателя handle нет, и
§8.1 просит именно этого отказа. `context_yield` возвращает OK: при одном
исполнимом контексте отдать остаток кванта — вернуться в него же.
`time_monotonic` — единственная операция V1, которой в этом ядре нет: тик
принадлежит таймеру ADR-0049, таймера ещё нет, а выдуманное число хуже отказа.
Пропуск назван в коде и здесь, а не оставлен молчанием.

**Контракту дописана нумерация, которой он сам требует.** §7 говорит «operation
numbers are assigned once and never reused», но ни номеров, ни значений статусов
в тексте не было — правилу про числа, не называющему чисел, невозможно
соответствовать. В §4 внесены значения (`OK` = 0, далее −1…−7), в §5 — номера
1…11, и записано, что **0 не назначается никогда**: регистр, который никто не
писал, содержит ноль, и придать нулю смысл значит превратить забытый селектор в
успешный вызов. Ни одна операция, право или гарантия не изменились, поэтому это
по-прежнему версия 1 — но это правка принятого контракта, и она вынесена сюда
отдельно, чтобы Project Architect мог её увидеть, а не обнаружить.

**Доказательство.** Два гейта на изолированных сборках:
`exception-injection.sh ring3` — нагрузка заполняет каждый регистр, который §3
называет сохраняемым, вызывает неназначенную операцию, проверяет статус и
неизменность всех регистров, вызывает `context_yield`, и только после этого
исполняет `ud2`. Vector 6 по адресу внутри пользовательской страницы кода — это и
есть всё утверждение целиком: чтобы дойти до той инструкции, должна была пройти
каждая проверка, а чтобы дойти до неё **там**, нужен был CPL 3.
`exception-injection.sh ring3-privileged` — `hlt` в CPL 3 даёт #GP: отказ
процессора, а не проверку ядра. Оба в `preflight --full` (41 гейт, PASS).

**Найдена ошибка, которую нашла именно Task 2.** Первый прогон повис. `-d int`
показал: нагрузка **прошла** (v=06, cpl=3, IP=0x40000144 — то есть все сверки
регистров сошлись), а упал обработчик исключения: он оказался по адресу внутри
`.data`, помеченной NX. Причина — ассемблерные блоки склеиваются в одну единицу
трансляции, и новый `syscall.S` заканчивался директивой `.section .data`, из-за
чего код `exception.S`, не называвший своей секции, лёг в данные. До Task 2 это
прошло бы незамеченным: данные были исполняемыми. Исправлено с двух сторон —
каждый файл называет свою секцию сам, и `syscall.S` возвращает `.text` текущей
секцией; таблица адресов stub'ов переехала в `.rodata`, где ей и место.

**Найдена вторая, того же рода.** Промежуточные записи таблиц страниц писались
без `U/S`, хотя комментарий рядом утверждал обратное: архитектура берёт
*пересечение* прав по пути, поэтому пользовательская страница под супервизорным
путём — отображённая, present и недостижимая. Теперь путь расширяется под лист,
а лист остаётся единственным местом, где записано настоящее право: лист без
`U/S` супервизорный, каким бы разрешительным ни был путь над ним.

### 2026-08-17 — Phase 2 Task 4 остановлен на границе: ADR-0053 и ADR-0054

Task 4 (рантайм как per-process артефакт) не начат: он упирается в решение,
которое реализация не вправе принять, выбрав удобный путь молча. Обе границы
измерены по коду и контрактам, а не предположены.

**Граница 1 — чем доставляется ring-3 образ рантайма (ADR-0053, Proposed).**
ADR-0048 говорит: «the capsule must carry the runtime image». Формат капсулы
бинарь не запрещает — `CAPSULE_FORMAT_V1` §9 требует UTF-8 только от имён путей
и блока лицензионного уведомления. Запрещает всё, что стоит **над** форматом:
каждая запись манифеста проверяется как точные байты закоммиченного git-blob'а
(`verify_committed`), обязана нести inline `SPDX-License-Identifier`
(`spdx_expression`), записывается в sidecar как *source material* с
repo-путём, а единственный флаг файла — boot-canonical, то есть сказать «это
производное, а не исходник» капсула не умеет. Продукт сборки не является ничем
из перечисленного, а закоммитить его значило бы противоречить собственной
идентичности проекта. Три варианта — капсула учится различать классы файлов (A),
загрузчик передаёт образ рядом через расширение Boot ABI (B), образ едет секцией
внутри артефакта ядра (C) — с рекомендацией **A** и с прямо названной ценой
каждого. Отмечено и то, что C делает доверенную базу *меньше* в том смысле,
который важен: ядро перестаёт линковать `tos-pipeline` и исполнять парсер,
чекер, лоуверинг, верификатор и интерпретатор в CPL 0.

**Граница 2 — как процесс сообщает, что закончил (ADR-0054, Proposed).**
`SYSTEM_ABI_V1` §5 не содержит self-exit: процесс завершается либо
`process_terminate` (требует process-authority *на этот процесс*, которого сам
процесс не держит), либо отказом железа. Третьего пути нет ни в одном из
четырёх принятых контрактов — проверено по всем. Это блокирует ровно первый
процесс: `init` возвращает `i32:240`, и после переезда за границу изоляции этому
значению нечем вернуться, а загрузке нечем отличить «закончил» от «завис».
Варианты: self-only операция `process_exit` (A), завершение как IPC-событие
супервизору (B), «первый процесс не заканчивается» (C). Рекомендация — **A**:
B прав про сервисы и неправ про bootstrap (у первого процесса супервизора нет
по построению), а C платит за отсрочку той самой сравнимостью результата Stage 2
через переезд границы, которую эта фаза обязана сохранить.

Ни один ADR не реализуется до подписи: статус `Proposed`, строка approval
пустая. Задачи 1–3 закрыты и от обоих решений не зависят; механизм запуска
процесса (адресное пространство, грант, вход в CPL 3) тоже не зависит — зависит
то, **что** запускается и **как** оно отчитывается.

### 2026-08-17 — Phase 2 Tasks 4–6: первый процесс исполняется в CPL 3

**ADR-0053 (вариант B) и ADR-0054 (вариант A) приняты Project Architect'ом.**
Образ рантайма доставляется загрузчиком рядом с капсулой; завершение процесса —
self-only операция `process_exit` (номер 12).

**`/system/boot/init.tos` больше не функция, которую вызывает ядро.** Он
прочитан, проверен, разрешён, залоуверен, верифицирован и исполнен образом
рантайма внутри процесса: собственное адресное пространство, CPL 3, собственный
грант, собственный стек — и возвращает **тот же `i32:240`**, что публиковала
доказательная база Stage 2. Ответ не изменился, когда изменилось место его
вычисления; иначе про переезд нельзя было бы сказать, что это переезд.

**Ядро — 78 КиБ. Было 934.** Парсер, чекер, лоуверинг, верификатор и
интерпретатор больше не линкуются в ring 0 вообще, а вместе с ними ушёл и
глобальный аллокатор ядра: ему нечего стало выделять. Stage-2-путь удалён, а не
оставлен рядом — ADR-0048 запрещает путь, на котором TOS Core исполняется в
CPL 0.

**Boot ABI перешёл на minor 1.** Три поля — `runtime_phys`, `runtime_length`,
`runtime_digest` — либо все заполнены, либо все нулевые: запись, объявляющая
образ без длины, вынуждала бы выбирать, какой половине верить. Отсутствие образа
законно и означает, что процесс не запускается и об этом сказано. Расширение
fail-closed в обе стороны по правилу, которое minor 0 уже нёс.

**Образ несёт заголовок, который эмитит линкер.** У сырого образа нет таблицы
символов, поэтому границы секций сообщает он сам: entry, text, file, memory.
Текст отображается read-only + executable из копии загрузчика; данные и `.bss` —
свежие кадры, куда копируется то, что несёт файл, а остальное уже ноль, потому
что кадр из пула такой. Два процесса, разделяющие один образ, не должны делить
одну записываемую страницу.

**Процесс не может обратиться к последовательному порту**, поэтому события
`TOS.RUN.*` пишутся рантаймом в report-регион и релеятся ядром при каждом входе
в край. Свойство Stage 2 сохранено: строка, написанная до вызова, оказывается в
логе раньше, чем вызов вернётся, — значит зависшую стадию по-прежнему называет
последнее событие.

**Четыре дефекта, найденные исполнением, — каждый такого рода, какой находит
только железо.**

1. Хвост с путями в launch-записи считался от **ёмкости** таблицы юнитов, а не
   от числа юнитов: первый путь начинался за 16 байт до конца кадра и уходил за
   него — в таблицу страниц. Отказ назвал отсутствующее отображение, за два
   уровня от записи, которая его стёрла. Теперь граница проверяется по записи,
   таблице и всем путям до первого байта.
2. Адрес записи клался в `rdi`, после чего вызов Rust забирал `rdi` под свой
   первый аргумент, и процесс входил с указателем на точку входа. Теперь он
   ставится последней инструкцией перед `iretq`.
3. Образ отображался read-only целиком, поэтому рантайм падал на собственных
   данных, а `.bss` не отображался вовсе. Отсюда заголовок образа.
4. Капсула отображалась с первого байта, который не выровнен на кадр, поэтому
   каждый юнит внутри был смещён на то, насколько глубоко в кадр она начиналась,
   и конвейер отказал по NUL на девятом байте. Теперь она отображается с кадра,
   в котором начинается, а перекос добавляется каждому юниту.

**Отказ в процессе больше не конец загрузки** (ADR-0049 §3). Два гейта в
`process-isolation.sh` этим и заканчиваются не на отказе: полезная нагрузка в
CPL 3 сверяет каждый регистр, который §3 объявляет сохраняемым, получает
`E_NOT_SUPPORTED` на неназначенную операцию, вызывает `context_yield` — и только
потом исполняет `ud2` (или `hlt` во втором сценарии, что даёт #GP от
процессора). Гейт требует **exit 33**: система не просто пережила отказ, она
после него довела до конца собственную работу и дала `value=i32:240`.

Правки контрактов, вынужденные переездом: `TOS.BOOTMODULE.FAIL` теперь допускает
`stage=process` — ядро больше не исполняет стадий, и утверждать, какая стадия
отказала, значило бы повторять чужое заявление; какая именно, говорит
`TOS.RUN.REFUSED` самого рантайма.

**Процесс не достаёт до памяти ядра, и это тоже проверено железом.** Третий
сценарий `process-isolation.sh nucleus`: полезная нагрузка пишет по адресу, по
которому слинковано ядро и который называет каждый лог загрузки. Отказ —
`vector=14 error=0x7 cr2=0x2000000 cpl=3`: present + write + user, то есть
отказало отображение, а не проверка, которую кто-то написал; верификатор этой
нагрузки вообще не видел. Это доказательство, которого ADR-0048 требует прямо.

**Память умершего процесса возвращается в пул, и это измерено.** Что процесс
держал, читается из его собственных таблиц страниц, а не запоминается отдельно:
одна запись о том, чем он владел, и это та, которой пользовался процессор.
Страница снимается **до** освобождения кадра — кадр в пуле, к которому ещё ведёт
отображение, это кадр, до которого доберутся двое. На каноническом пути
возвращается **25 107 кадров** (грант 24 576 + стек 512 + report 16 + запись 1 +
данные образа 2), после чего пулу доступно 58 983 из 59 051. Гейт
`stage2-runtime.sh` требует, чтобы вернувшихся кадров было не меньше, чем
занимает грант. Не возвращаются пока таблицы страниц мёртвого пространства
(~46 кадров на процесс) — освободить внутреннюю таблицу значит доказать, что под
ней ничего не отображено, а в том же дереве живут отображения самого ядра; это
названо здесь, а не оставлено на потом находкой.

`./scripts/preflight.sh --full` — PASS, 43 гейта. Cargo test: 441.

### 2026-08-19 — Phase 3 Task 1–2: время идёт, и процесс прерывается

План: `docs/superpowers/plans/2026-08-19-stage3-phase3-time-and-preemption.md`.
ADR-0049 принят давно; здесь он реализован в той части, которая не требует
второго процесса.

**Прерывания включаются один раз** — после того как подложка процессов
существует, и до входа в первый процесс, ровно там, где их ставит ADR-0049.
Legacy 8259 маскируется целиком, а не перепрограммируется: контроллеру, через
который никто ничего не маршрутизирует, нечего настраивать. Local APIC включён,
его страница отображена uncacheable в **каждом** пространстве, которое строит
ядро: прерывание, взятое в CPL 3, исполняет обработчик ядра без смены `CR3`, а
обработчик подтверждает прерывание записью в регистр устройства.

**IDT теперь на 256 записей, и это решение, а не побочный эффект.** Заявлены два
вектора выше 31 — таймер и spurious; все остальные **отсутствуют**, поэтому
прерывание на незаявленном векторе — отказ, чего и требует ADR-0049 §2. Прежняя
32-элементная таблица давала тот же ответ случайно.

**Первый обработчик в этой системе, который возвращается.** Все прежние stub'ы
заканчивают загрузку и потому ничего не сохраняют; этот делает обратное:
процесс, вернувшийся с другим регистром, — процесс, который система испортила.
Он не выделяет памяти, не берёт блокировок и делает ровно два действия:
инкремент и подтверждение.

**`time_monotonic` перестал быть единственной неисполненной операцией V1.** Он
возвращает тик, который считает прерывания и ничего больше: Stage 3 не заявляет
ни настенного времени, ни доверенного источника времени.

**Найдено исполнением: процесс входил с замаскированными прерываниями.**
`process_enter` клал в RFLAGS `0x2` — верно, пока никто не маршрутизировал
прерывания, и неверно ровно с того момента, как таймер включился. Симптом был
точным: тик стоял 200 000 чтений подряд, при том что до запуска процесса он
успевал дойти до 310. Теперь `0x202`, и процесс исполняется прерываемым — то
есть вытесняемым.

**Измерено изнутри процесса, потому что только он видит оба конца.** Рантайм
читает тик до прогона и после, и на каноническом пути тот успевает вырасти с 300
до 371: за один прогон процесс был прерван **семьдесят один раз** и каждый раз
возобновлён, а результат остался `value=i32:240`. Гейт `stage2-runtime.sh` теперь
требует, чтобы тик к началу процесса был ненулевым и вырос к концу.

**И отдельно — заявленное в Task 2 целиком.** «Тик сдвинулся между двумя моими
вызовами» и «меня прервали, пока я исполнял собственные инструкции» — разные
утверждения, и первое не доказывает второго. Рантайм крутит цикл на 20 млн
итераций **без единого системного вызова** и читает тик до и после: 355 → 395,
то есть сорок прерываний взято прямо посреди его собственного кода, и он
продолжил исполняться. Это и проверяет гейт.

Квант — 100 000 отсчётов при делителе 16, не откалиброван ни по чему и не выдаётся
за длительность. ADR-0049 оставляет конкретный источник и режим реализации;
что он фиксирует — что таймер один и что он для вытеснения и учёта времени.

`./scripts/preflight.sh --full` — PASS, 43 гейта.

### 2026-08-19 — Phase 3 Task 3 начата: обработчик видит прерванный контекст

Считать тик — девять сохранённых регистров. Вернуться в **другой** процесс —
все пятнадцать плюс кадр, который положил процессор, потому что то, что прочтёт
`iretq`, тогда уже не то, что записало прерывание. Stub таймера теперь строит
именно такой кадр, его разметка совпадает с `TrapFrame`, а адрес кадра —
единственный аргумент обработчика. Шаг от «посчитать тик» к «возобновить
другого» становится изменением того, **что обработчик пишет**, а не того, до
чего он может дотянуться.

Поле уже несёт работу: по `CS` прерванного кадра ядро различает, шло ли время в
процессе или в самом ядре, и сообщает это в `TOS.RUN.PROCESS_EXIT` полем
`ticks=`. На каноническом пути из ~393 тиков процессу принадлежат **108**. Это
число ядра, а не процесса: процесс не может наблюдать, сколько его не было на
процессоре, и заявленное им было бы догадкой.

Что ещё не сделано в Task 3, и это следующая по-настоящему новая работа:
процессов по-прежнему один. `process::launch` доводит его до конца и только
потом возвращается, так что планировщика, таблицы процессов и переключения
контекстов пока нет — есть место, куда они встанут.

`./scripts/preflight.sh --full` — PASS, 43 гейта.

### 2026-08-19 — Phase 3 Task 3 закрыта: два процесса делят процессор

Переключение контекста оказалось ровно тем, чем его описал план: две копии и
запись `CR3`. Кадр прерванного процесса уходит в его слот, кадр следующего
приходит на его место, `CR3` следует за ним — и `iretq` в конце stub'а
возвращается в **другого**, не зная, что передумал. Вход в процесс впервые —
та же операция над кадром, который написал launcher, а не положил процессор;
отдельного «войти в новый процесс» больше нет, и процесс не может отличить одно
от другого.

Появилась таблица процессов: четыре слота, каждый со своим адресным
пространством, кадром, областью отчёта и учётом времени. `launch` разделился на
`create` (построить, не входя) и `schedule` (отдать процессор каждому, пока
живые есть). Это и есть вся разница между «один процесс» и «несколько»:
построение и вход перестали быть одной операцией.

Доказательство взято с двух сторон, и обе стороны нужны.

- **Со стороны ядра.** `TOS.RUN.PROCESS_EXIT` теперь несёт `process=`,
  `ticks=`, `quanta=` и пару `first_tick=`/`last_tick=`. В прогоне с двумя
  процессами: `[595,763]` и `[594,766]` — интервалы **перекрываются**, чего два
  процесса, отработавшие один после другого, дать не могут. `quanta=86` и `87`:
  процессор отбирали и возвращали восемьдесят с лишним раз каждому, и это не то,
  что процесс может устроить себе сам.
- **Со стороны процессов, не спрашивая, кто из них говорит.** Каждый runtime
  обрамляет двумя чтениями тика цикл, который **не делает ни одного системного
  вызова**. Скобки `[676,764]` и `[675,766]` перекрываются — кто бы какую строку
  ни написал, оба продвигались на одном и том же отрезке тиков, и никто ничему
  не уступал.

Оба вернули `i32:240`. Канонический путь не сдвинулся: там один процесс,
`quanta=1`, переключения не происходит вовсе — round-robin с одним готовым
контекстом пропускает переключение, а не выполняет его на себя.

**Почему второй процесс за feature-флагом.** Планировщик, таблица и
переключение — production-код, на каждом booted. А вот *сколько процессов у
загрузки* — не решение этого ядра: ни один принятый контракт не говорит, что у
канонического boot их два, и ядро, решившее так само, владело бы service policy,
чего ADR-0048 §2 ему не даёт. Поэтому второй процесс живёт за
`test-two-processes` — ровно там же, где живут ring-3 экскурсии: механизм
настоящий, политика не выдумана. Когда контракт скажет, из чего состоит набор
процессов загрузки, launcher прочтёт его вместо флага, и ниже launcher'а не
изменится ничего.

Побочно закрылась вторая половина evidence ADR-0049 §3. Ring-3 экскурсия раньше
доходила до своего фолта **до** того, как строился первый процесс. Теперь она
попадает в ту же таблицу и входит через тот же планировщик, так что фолт
случается при живом соседе — и гейты проверяют, что упавший процесс и процесс,
доделавший работу, разные.

Новый гейт: `host-tools/qemu-test/scheduler.sh`. `TOS.RUN.PROCESS_BEGIN`,
`_EXIT`, `_FAULT` и `_RECLAIMED` получили `process=`; `RUNTIME_OBSERVABILITY_V1`
обновлён и там же сказано прямо, что события §3–§6 при нескольких процессах
чередуются и `process=` не несут — атрибутировать их из транспорта нельзя, и
контракт этого не требует, потому что каждое утверждение, которому нужен
владелец, — событие ядра.

`./scripts/preflight.sh --full` — PASS, 44 гейта.

### 2026-08-19 — Phase 4 остановлена на границе: ADR-0055…0057

Фаза 4 — capability handles и типизированный IPC — упирается в пробел, который
обнаруживается на первой же строке реализации и который не решается выбором
реализации.

**`SYSTEM_ABI_V1` §5 назначает двенадцать операций. Девять требуют capability,
три — self-only. Ни одна не производит capability.** `endpoint_send` требует
endpoint-хендл, а endpoint никто не создаёт; `region_share` требует
region-хендл, а грант приходит в процесс базой и длиной, что не хендл и прав не
несёт; `process_create` требует process-authority capability, которую тоже никто
не производит — то есть операция, которой supervisor наделял бы ребёнка, сама
недостижима ни для кого. Launch record хендлов не несёт вовсе.

Буквально: конформная Stage 3 система не может держать ни одной capability
никогда и не может выполнить ничего, кроме трёх self-only операций. Ровно в этом
состоянии код и находится — и сейчас это неотличимо от корректного.

Это не ошибка одного документа. Это вопрос, на который каждый из трёх принятых
документов рассчитывает, что отвечает другой, — случай, который AGENTS.md §2
требует сообщать, а не разрешать удобным чтением.

Рядом нашлись ещё два пробела того же рода, оба блокирующие и оба узкие:
ни один контракт не говорит, **в каком аргументе** лежит capability, которую
требует операция (§3 перечисляет шесть регистров и ни одной раскладки), и
`IPC_V1` §3 объявляет три границы сообщения «fixed maximum, declared by this
contract version», **не называя ни одного из трёх чисел**.

Оформлено как три Proposed ADR с вариантами, ценами и рекомендацией:

- **ADR-0055 — откуда у процесса первая capability.** A: наделяет launcher,
  наделение едет в launch record (`LAUNCH_VERSION` 2), а `process_create`
  наделяет ребёнка аттенюацией того, что держит родитель, — рекурсия
  заканчивается на boot-процессе, и эскалация через порождение невозможна тем же
  движением. B: self-only операция создания объекта — необходима позже, но
  bootstrap не решает: два процесса, каждый со своим endpoint, всё равно друг
  друга не достают. C: грант становится region-capability — меняет ADR-0041 и
  ADR-0050 и отвечает только про регионы. Рекомендация: **A**.
- **ADR-0056 — где хендл и что отвечает пустая таблица.** A: capability — первый
  аргумент, порядок отказа «границы → поколение → тип → права», то есть
  out-of-range даёт `E_BAD_HANDLE`, а пустая таблица отказывает так каждому
  индексу. Сейчас ядро отвечает `E_NO_CAPABILITY` по номеру операции, не глядя
  на аргумент, — что соответствует §8.1 и **не** соответствует §8.2.
  Рекомендация: **A**.
- **ADR-0057 — три числа `IPC_V1`.** A: 256 байт inline, 4 capability,
  2 региона. B: вдвое больше по каждой оси. C: страница — перестаёт быть «small
  enough to copy without allocation» ровно в том смысле, ради которого
  ADR-0049 §5 существует. Рекомендация: **A**.

Что **не** сделано намеренно: таблицу capability можно написать сегодня против
наделения, которого нет, и две операции, которые только потребляют хендл, —
отказывать всему. Оба компилируются, оба проходят гейт, написанный под их
поведение, и ни одно не является доказательством чего-либо: без способа получить
хендл «отказывает каждому хендлу» — это поведение, которое у ядра уже есть,
достигнутое бо́льшим количеством кода. Это форма, которую AGENTS.md §4 называет
disguised throwaway, и расстояние от неё до настоящей вещи — ровно одна подпись.

План фазы: `docs/superpowers/plans/2026-08-19-stage3-phase4-capabilities-and-ipc.md`.

Побочное наблюдение, не блокирующее: ADR-0044 (Proposed) отсутствует в
`docs/SPECIFICATION_SOURCES.txt`, тогда как ADR-0036…0039 (тоже Proposed) в нём
есть. Новые 0055…0057 добавлены по преобладающему прецеденту; расхождение по
0044 не трогалось, потому что оно не про эту работу.

### 2026-08-19 — ADR-0055…0057 приняты (вариант A) и реализованы

Три решения подписаны Project Architect, все по варианту A, и реализованы в тот
же день. Власть в системе перестала быть описанием.

**Таблица capability.** Пер-процессная, в памяти ядра, куда у процесса нет
отображения: шестнадцать статически зарезервированных слотов, в каждом объект,
права, scope и поколение. Хендл — это индекс **и** поколение, потому что
`CAPABILITY_V1` §2 формулирует валидность как «index in range **and** generation
matching», а голому индексу не с чем сравниваться. Поколения начинаются с
единицы, так что хендл из одних нулей — значение регистра, который никто не
писал, — не именует ничего нигде.

**Порядок отказа (ADR-0056).** Диспетчер разрешает первый аргумент до того, как
узнаёт, что за операция: границы → поколение → тип → права, первый провал решает
статус. Поэтому индекс вне таблицы — `E_BAD_HANDLE`, всё после — 
`E_NO_CAPABILITY`, а пустая таблица отказывает `E_BAD_HANDLE` на каждом индексе.
Процесс, который ничего не держит, ничего и не именует.

**Наделение (ADR-0055).** Ядро пишет таблицу процесса **до того, как процесс
войдёт**, из того, что решил launcher, и описывает выданное обратно в launch
record (`LAUNCH_VERSION` 2). Процесс может сузить таблицу (`capability_release`)
или уточнить (`capability_attenuate`); расширить — нечем. Решение launcher'а
лежит на логе как решение: `TOS.RUN.PROCESS_ENDOWED process= capabilities=
policy=launcher-constant asserted_by=launcher`, и оно выпускается даже когда
выдано ноль.

**У канонической загрузки наделение пустое, и это политика, а не её
отсутствие.** `system.boot.init` не запрашивает ни одной capability, а правило —
не выдавать ничего, о чём модуль не просил. `stage2-runtime.sh` это проверяет:
процесс, не держащий ничего, — то, что делает каждую последующую выдачу
атрибутируемой.

**IPC (ADR-0057: 256 байт, 4 capability, 2 региона).** Endpoint с ограниченной
очередью, которую никогда не растят ради сообщения. Полезная нагрузка **не едет
в вызове**: `SYSTEM_ABI_V1` §3 не допускает указателя, по которому ядро ходит, а
шесть регистров не несут 256 байт — поэтому у каждого процесса есть слот
сообщения, который отображает launcher, и ядро читает и пишет его через
собственную identity-карту, как область отчёта с самого первого процесса.

Доказательства (`capabilities.sh`), все — ответы ядра на вопросы процесса:

| Что | Ответ |
|---|---|
| индекс за таблицей | `-2` (`E_BAD_HANDLE`) |
| шестнадцать индексов внутри, с угаданным поколением | `-1` ×16, `guessed=0` |
| 28 байт между двумя процессами | `bytes=28 text=authority-crossed-a-boundary` |
| на байт больше границы | `-3`, отказ, не усечение |
| `receive` хендлом с правом `send` | `-1` — права раздельны (`IPC_V1` §2) |
| аттенюация с запросом **всех** прав | `status=0`, и полученный хендл всё равно отказывает в чужой половине |
| освобождённый хендл, названный снова | `-1` — устарел по поколению |

Оба процесса при этом вернули `i32:240`.

Найден и исправлен настоящий дефект по дороге: runtime клал второй аргумент
вызова в `rdx`, тогда как §3 задаёт порядок `rdi, rsi, rdx, r10, r8, r9` — ядро
читало как длину то, что случайно лежало в `rsi`, и отправка отказывала
`E_BAD_ARGUMENT`. `rdx` — регистр третьего аргумента на входе и *значения* на
выходе.

Что **не** сделано и названо в плане как несделанное: передача linear
capability и регионов (`IPC_V1` §5, §6), `endpoint_call`/`endpoint_reply`,
блокирующий приём с путём отмены, и confused deputy (`CAPABILITY_V1` §7.6) —
последний требует передачи, и docs/37 называет его явно как тест, который тихо
проваливается в системах, проходящих остальные пять. Сообщение сегодня не может
*назвать* ни одной capability, что сильнее отказа, но это другое утверждение.

`./scripts/preflight.sh --full` — PASS, 45 гейтов.

### 2026-08-19 — Порядок фазы 4 изменён: сначала полномочие над процессом

Блокировка отложена на инкремент по решению владельца. Причина не в объёме:
`SYSTEM_ABI_V1` §6 требует у блокирующей операции путь отмены, а пока никто не
может держать полномочие над процессом, отменять некому — блокировка приехала бы
вокруг пути отказа без единого доказательства, то есть ровно «unimplemented
failure path» из AGENTS.md §4. Цена — `endpoint_call` и измеримый round-trip ждут
ещё инкремент; это вся цена, и это не аргумент.

**Два структурных исправления, которые надо было сделать давно.**

Пул кадров был локальной переменной `boot_entry` и приезжал в планировщик по
`&mut`. Это не то, что ADR-0050 называет владением памятью машины, и с края он
был недостижим вовсе. Теперь пул — состояние ядра, и с ним одно правило:
**`&mut Frames` никогда не переживает инструкцию, покидающую ядро**. Планировщик
раньше держал его через `iretq`, и первый же системный вызов вошедшего процесса
дал бы второй `&mut` на ту же память.

Описание того, что эта загрузка умеет запускать — образ, набор исходников,
карта, идентичность гранта, — тоже стало состоянием ядра. Супервизор выбирает
модуль и полномочие; из чего сделана система, он не поставляет, и процесс,
способный это переопределить, был бы процессом, способным запустить то, чего
загрузка не принимала.

**Права процесса выведены, а не придуманы.** `CAPABILITY_V1` §3 требует «a finite
set from the object type's declared rights», а прав процесса не объявляет никто.
Единственный тип, чьи права **объявлены**, показывает правило: `IPC_V1` §2 даёт
endpoint'у `send`, `receive`, `call` — ровно три операции §5, которые именуют
endpoint. Значит **права объекта — это операции, которые его именуют**, а над
процессом их две: `create` и `terminate`.

**Полномочие именует процесс, а не класс.** §3 допускает объект и запрещает «all
of them», поэтому capability со смыслом «может создавать что угодно» не бывает:
процесс, который вправе создавать, держит полномочие над тем процессом, под
которым создаётся ребёнок, — над собой. Это единственная capability, которую
может выдать только launcher, потому что она именует процесс, не существующий до
момента выдачи. Цепочка на этом и заканчивается.

Операции 8 и 9 реализованы. Вызывающий получает полномочие над созданным,
несущее ровно те права, что несло использованное; ребёнок наделяется ничем — это
правило launcher'а на уровень ниже.

**Найден и исправлен дефект собственной работы:** capability, именующая процесс,
пережила бы этот процесс и указала на его преемника в том же слоте. Это та же
устарелость, от которой поколение защищает хендл, уровнем ниже, и `CAPABILITY_V1`
§3 уже это формулирует — время жизни capability ограничено её объектом. У слота
теперь своё поколение, а проверка живёт в `resolve`, один раз: операция, которая
обязана помнить спросить, однажды забудет.

Доказательство (`supervisor.sh`): процесс создаёт процесс, завершает его, и ядро
записывает **кто** завершил — третий способ окончания, единственный, который есть
решение другой стороны. Тот же хендл после этого отказывает (`-1`), индекс модуля
вне набора отказан, а не подрезан (`-3`).

**Измерено попутно, и это ещё не дефект, но станет им:** `process_create` строит
адресное пространство внутри одного системного вызова с замаскированными
прерываниями, и это достаточно долго, чтобы к возврату таймерное прерывание
всегда было отложено — ребёнок всегда получает один квант раньше следующего
вызова создателя. Ни один принятый контракт не нарушен (ADR-0049 §5 — про
interrupt context), но системный вызов, стоимость которого O(памяти машины), —
это задержка, ждущая контракта, который её измерит. Записано, чтобы нашлось
намеренно.

`./scripts/preflight.sh --full` — PASS, 46 гейтов.

### 2026-08-19 — ADR-0058 и ADR-0059 приняты и реализованы

**Блокировка (ADR-0059, вариант D).** Заблокированный контекст не готов к
исполнению и просыпается **отвеченным**: операция, удовлетворяющая ожидание,
сама его и выполняет, так что никто не просыпается, чтобы спросить снова. Две
копии inline-нагрузки — ровно бюджет docs/35. Блокировка по умолчанию, бит 0
регистра флагов просит не ждать.

Край системного вызова теперь строит **тот же `TrapFrame`, что и stub таймера**.
Раньше он сохранял шесть аргументов, потому что дальше всегда был возврат;
приостановленный вызов кладут и поднимают позже, а поднимает его планировщик,
который умеет входить в контекст и ничего не знает о том, как контекст им стал.
Теперь оба пути в ядро оставляют одно и то же, и приостановленный системный
вызов неотличим от вытесненного процесса.

Условие завершения планировщика изменилось с «нет готовых» на «нет готовых **и
нет ждущих**»: с блокировкой первое сообщало бы о навсегда остановившейся системе
как об успешной загрузке. Правило живости записано **обеими половинами** — «нет
готовых *и ничто маршрутизированное не может это изменить*», — чтобы первое
устройственное прерывание Stage 4 обязано было вернуться к нему намеренно, а не
тихо его обесценило.

Терминатор ливлока считает **доставки, а не ходы**. Написал сначала через ходы —
гейт поймал: отменённый контекст немедленно становится готовым и берёт ход, ровно
ради этого его и отменяют, так что счётчик, сбрасываемый по «кто-то побежал»,
сбрасывается каждый раз и до двух не доходит.

Доказательство — система, построенная так, чтобы по-настоящему встать
(`blocking.sh`): процесс держит право receive на endpoint, куда **некому**
отправить — других процессов нет, а операции, создающей endpoint, не существует.
Неблокирующая форма отвечает `-4`, блокирующая ждёт, правило срабатывает и
отменяет, процесс наблюдает `-5` и спрашивает ещё раз (что может только
возобновлённый), второе срабатывание не находит ни одной доставки. **Загрузка
падает с `RESULT_BOOT_MODULE_FAILED`** — в этом весь смысл правила.

**Объёмные аргументы (ADR-0058, вариант A).** Слот сообщения стал тем, чем его
называет решение, — областью аргументов вызова; запись запуска — версии 3.
Хендлы сообщения лежат по смещению, которое фиксирует `IPC_V1`, счётчик едет в
регистре, так что ядро знает границы читаемого до чтения.

**В очередь кладётся объект, а не хендл отправителя.** Хендл — имя в одной
таблице и в другой не значит ничего, а отправитель может освободить его или
кончиться до доставки. Отправка разрешает то, что ей дали, отказывает всему
сообщению, если что-то не разрешилось, и очередь несёт объекты; имена получателя
делаются, когда сообщение доходит. Поэтому же неудавшаяся отправка не передаёт
ничего: момента, в котором существует частичная передача, просто нет.

Доказательство — контраст шириной в одну строку: собственный хендл получателя
получает `-1` на `endpoint_send`, а хендл, приехавший с сообщением, выполняет тот
же вызов на том же endpoint со статусом `0`. Разница между двумя статусами — это
делегирование и больше ничего.

Отправка — **делегирование**, отправитель сохраняет своё. Линейный случай
`CAPABILITY_V1` §4 относится к capability, которые интерфейс объявил линейными, а
ни один тип объекта Stage 3 таким не объявлен — это утверждение о том, что
существует, а не послабление правила.

Не сделано и названо в плане: передача регионов и §9.6; наделение ребёнка при
`process_create` и имя модуля путём вместо порядкового номера — раскладка обоих в
той же области, что и остальное; `endpoint_call`/`endpoint_reply`, которым теперь
нужна только single-use reply capability из `IPC_V1` §4.

`./scripts/preflight.sh --full` — PASS, 47 гейтов.

### 2026-08-19 — Фаза 4: пункты 1–3 закрыты, и найдена настоящая граница Stage 3

**`endpoint_call`/`endpoint_reply`.** Право ответить — объект, а не флаг: его
никому не выдают, ядро делает его в момент вызова, оно приезжает с запросом в
**последнем** слоте таблицы передачи (чтобы получателю не надо было знать,
сколько capability отправил вызывающий), и тратится тем, что им воспользовались.
Одноразовость — свойство счётчика, а не флага, который кто-то обязан не забыть
сбросить: capability именует вызов, и всё, что вызов заканчивает — ответ, отмена,
смерть вызывающего, — счётчик двигает. Поэтому требование `IPC_V1` §9.5 (у
отменённого вызывающего reply-capability аннулируется, а не течёт) — не второй
путь, который можно забыть, а тот же самый, с другой стороны.

**Наделение ребёнка и имя модуля путём.** Порядковый номер помещался в регистр —
это всё его достоинство; он именует позицию в списке, который никто не
публиковал, и две загрузки с разными капсулами дали бы один номер разным модулям.
Каждая запись наделения именует capability, которую держит **родитель**, и права,
которые он хочет дать ребёнку; ребёнок получает пересечение — расширение не
отвергается, оно невыразимо. Запись, которая не разрешилась, отменяет **всю**
постройку: ребёнок, наделённый наполовину, держал бы власть, которую никто не
решал ему давать.

**Confused deputy** (`CAPABILITY_V1` §7.6) — тест, который docs/37 называет
проваливающимся тихо там, где проходят остальные пять. Пара построена так, чтобы
вопрос вообще имел смысл: депутат держит `send` **и** `receive`, то есть его
власть настоящая, а клиент — только `call`.

| Запрос | Что делает депутат |
|---|---|
| именует объект **числом**, без capability | отказывает: число — не хендл, а воспользоваться им значило бы действовать своей властью по указанию постороннего |
| несёт capability, которую клиент действительно держит | действует **ею** и получает `-1`; строкой позже делает ту же операцию своим хендлом и получает `0` |

Два статуса, одна операция, один процесс, соседние строки: **власть привязана не
к действующему, а к тому, что действующему дали для этой работы.**

### Настоящая граница, найденная при сверке

`docs/37` спрашивает: «исполняют ли текстовые процессы настоящие contract'ы
capability/IPC — или работают декоративными скриптами вокруг привилегированных
двоичных сервисов?» Всё, что построено в фазе 4, приводит в действие **Rust-образ
runtime** — привилегированный двоичный. Текстовый модуль считает число.

`import capability` разбирается и опускается в IR. А вызвать по ней нечем:
`extern fn` грамматика резервирует и отвергает как `E1801_FFI_NOT_AVAILABLE`
«until a later accepted FFI contract supplies an interface identifier and
capability rule». Языковая половина контракта существует объявлением, половина
вызова — не существует.

Оформлено как **ADR-0060 (Proposed)**, решение уровня 3. Вариант A: дать схему
интерфейса, которой `extern` уже ждёт, — грамматика не меняется, тип-система не
меняется, `E1801` перестаёт быть безусловным. Отвергнуты: методы на
capability-типах (меняют принятую семантику и **прячут** границу, которую docs/42
§5 требует делать видимой), встроенные интринсики (вшивают ABI Stage 3 в язык
навсегда), импорт как вызываемое значение (расщепляет один грант на имя за
операцию).

Три вещи вынесены в само решение, а не в схему: схема — это **класс** документов,
а не документ (драйвер Stage 4 обязан быть вторым её экземпляром, а не особым
случаем); **порядок эффектов детерминирован и доказывается верификатором, а
значения — нет**; и блокирующий `extern`-вызов — это заблокированный процесс, то
есть движок обязан уметь выйти и быть вновь введённым на границе вызова.

`./scripts/preflight.sh --full` — PASS, 49 гейтов.

### 2026-08-19 — ADR-0060 принят: `extern` перестал быть безусловным отказом

Первый артефакт, которого решение требовало, написан:
`source/interfaces/system/SYSTEM_INTERFACE_V1.md` — **первая принятая схема
интерфейса**, по чек-листу docs/42 §5. Она не FFI: запреты §5 не тронуты, никакой
C ABI, libc или динамический загрузчик не появились. Она определяет ровно то, чего
системе не хватало, — как модуль вызывает операцию по capability, которую ему
выдали.

Нового синтаксиса ноль. Три формы, все уже в TOS Core V1:

```tos
import capability system.ipc.Endpoint as endpoint;
extern fn endpoint_send(cap: system.ipc.Endpoint, length: u64) -> i64 uses [endpoint];
```

`import capability` просит власть и связывает имя; `uses [имя]` называет **эту
просьбу**, так что интерфейс — это тип импортированной capability, и до операции
нельзя дотянуться, не запросив власть, которой она принадлежит; первый параметр —
сама capability. Это ровно фраза docs/42 §2, записанная механизмом.

`E1801_FFI_NOT_AVAILABLE` перестал быть безусловным и стал тем, чем docs/44 его
всегда называл: **элемент `extern`, не именующий принятой схемы**. Диагностика
несёт `reason=`, и каждая из четырёх причин проверена тестом, ломающим ровно одну
вещь: имя операции, число значений, тип результата, отсутствие импорта.

Схема объявляет только операции, которые уже существуют, уже достижимы через
`SYSTEM_ABI_V1` и уже доказаны гейтами. `process_create` **намеренно
отсутствует**: он берёт имя модуля и наделение, которые живут в области
аргументов, а у TOS Core V1 нет указателей и схема их не вводит — интерфейс,
объявивший бы его, объявлял бы операцию, аргументы которой ни один модуль не может
подать. Он появится, когда будет типизированный способ сказать, что он берёт.

Гейт `check-interface-schema` держит документ и таблицу фронтенда вместе: два
утверждения одного факта расходятся, и сравниваются именно операции.

Что осталось до того, как текстовый модуль действительно позовёт систему:
опускание `extern`-вызова в IR, доказательство эффекта верификатором, выход
движка на границе вызова и возврат в неё, и обработчик в runtime-образе. Схема и
фронтенд — первая треть; она закончена и проверяема сама по себе.

`./scripts/preflight.sh --full` — PASS, 50 гейтов.

### 2026-08-19 — Вторая треть ADR-0060: артефакт называет интерфейс

**Опускание.** Вызов `extern`-операции опускается в инструкцию с
`unsafe_interface = Some(путь интерфейса)` — это и есть «accepted interface ID»,
которого docs/43 §3 требует от extern-операции, и слот под него в IR уже был,
пустой, с комментарием «Set on an operation reaching an accepted external
interface». Контракт IR не менялся: он этого и ждал.

**Исправлено попутно расхождение, которое было и раньше.**
`Signature.effects` документирует себя как «by interface path», а опускание
клало туда **имя связки** из `uses [...]`. Имя связки значит что-то только внутри
модуля, который его написал; читатель артефакта не может узнать, на что оно
ссылалось. Теперь там путь интерфейса — то, чем поле себя и называет. Эффект, не
именующий импорта, сохраняет написанный текст: модуль, объявивший эффект, которого
не просил, продолжает говорить об этом в артефакте, а не теряет это.

**Верификатор** перестал отвергать всякий интерфейс и начал доказывать две вещи,
которые артефакт обязан сказать о себе сам: модуль этот интерфейс **запрашивал**,
и функция, делающая вызов, **объявила** его эффектом. Схему верификатор не носит —
верификатор, знающий, какие интерфейсы существуют, был бы вторым местом, где они
объявлены.

Доказательства (`tests/integration/tests/interface_schema.rs`) ломают **артефакт**,
а не исходник: верификатор, видящий только то, что порождает этот фронтенд, ничего
не доказывает про фронтенд, написанный кем-то другим. Убрали импорт — отказ
«reached but never imported»; убрали эффект у функции — «reached without being
declared».

**Найдено при написании теста, и это важнее самой работы:** `docs/42` §3 запрещает
`extern` в профиле **bootstrap**. Значит модуль, дотягивающийся до системы, — это
`profile full` по решению документа, а не по удобству. И значит **канонический
boot-текст, который bootstrap, не сможет позвать ни одной операции никогда.**
Супервизор будет Full-модулем, а Full-путь движка — то, чего Stage 2 не строил.
Записано сейчас, чтобы это не обнаружилось при написании супервизора.

`./scripts/preflight.sh --full` — PASS, 50 гейтов.

### 2026-08-19 — Открытый дефект: перемежающийся фатальный сбой в CI

CI один раз упал на гейте capabilities, перезапуск прошёл. Это **не** «мигнуло и
ладно»: отказ настоящий, доказательство сохранено, и он записан здесь открытым,
а не закрыт зелёной галочкой.

```
TOS.EXCEPTION vector=14 error=0x0000000000000011 rip=0x000000000bb75000 cr2=0x000000000bb75000
```

Что известно: ошибка `0x11` — это present + instruction fetch, то есть **ядро
выбирало инструкцию из страницы, помеченной no-execute**. `rip == cr2`, адрес
выровнен по странице и лежит внутри пула кадров. Сбой пришёлся на стадию `check`,
когда оба процесса уже работали. Двоичный образ ядра в этом коммите **не менялся**
— менялся только runtime-образ (фронтенд и верификатор), то есть изменилось
время, а не код ядра. Локально не воспроизвелось ни разу за 13 прогонов.

Что сделано сразу: `TOS.EXCEPTION` дополнен полями `cs=` и `rsp=` после четырёх
фиксированных (по правилу расширения Boot ABI v1). И это немедленно окупилось —
**дефект воспроизвёлся локально** на гейте deputy, примерно **1 раз из 20**, и
теперь с уликами:

```
TOS.EXCEPTION vector=14 error=0x11 rip=0x08023000 cr2=0x08023000 cs=0x08 rsp=0x02013738
```

- `cs=0x08` — селектор **ядра**. Исполнялось ядро, а не процесс; это не фолт
  CPL 3, который процессные пути должны были поймать.
- `rsp=0x02013738` — внутри **образа ядра** (база 0x02000000, размер ~97 КиБ),
  то есть на одном из двух статических стеков прерываний, а не на стеке
  загрузчика (0x0dcc____), на котором работает планировщик. Значит это путь,
  на который попадают из CPL 3, — прерывание или фолт.
- Оба локальных срабатывания дали **побайтово одинаковые** `rip` и `rsp`. Это
  не блуждающий переход по случайному мусору: на данной сборке цель одна и та
  же, достигаемая редким стечением обстоятельств.
- Адрес выборки инструкции выровнен по странице и лежит в пуле кадров, страница
  present и no-execute.

Что это **исключает**: случайную порчу (совпадали бы разные адреса) и
переполнение стека (rsp далеко от границы). Что остаётся правдоподобным: `ret`
или `iretq` по значению, которое на этом пути кладётся на стек и оказывается
физическим адресом из пула. Но это по-прежнему гипотеза, а не вывод.

Что **не** сделано и почему: причина не найдена, и чинить догадкой хуже, чем
оставить открытым. Символов у плоского образа ядра нет, так что следующий шаг —
собрать ядро с таблицей символов и разрешить `rip` и содержимое стека на
`rsp`, а не гадать по числам.

**Гейты с двумя процессами перемежающе нестабильны, пока это не исправлено.**
Это сказано здесь прямо, потому что зелёный прогон после перезапуска — не
доказательство исправности.

*(Закрыто в следующей записи. Гипотеза «`ret` по значению из пула» оказалась
верной по следствию и неверной по причине: стек портил не сам путь, а флаг.)*

### 2026-08-19 — Дефект закрыт: ядро начинало работу с флагами процесса

Причина найдена, доказана прогоном и закрыта детерминированным гейтом.

**Как найдена.** QEMU умеет протоколировать каждое прерывание вместе с полным
состоянием процессора (`-d int`). Воспроизведение в цикле поверх готового ESP
гейта deputy дало отказ на 34-м прогоне — и вместе с ним две соседние записи,
которых не хватало:

```
840: v=20 ... cpl=3 ... RFL=00000602 [D------]     <- тик в процессе, DF=1
841: v=0e e=0011 cpl=0 IP=0008:0000000008023000    RFL=00000446 [D--Z-P-]
     RAX=RBX=0000000002013770  RDI=00000000020136d0  R12=000000000200cc70
```

`R12` — это адрес `memcpy` в образе ядра; `RAX/RBX` — адрес `TrapFrame`,
который построил стаб таймера; `RDI` — ровно на 160 байт **ниже** него, а
`TrapFrame` занимает ровно 160 байт. То есть `*frame = table[next].frame`
выполнился `rep movs` **назад**: вместо того чтобы записать кадр, он записал
160 байт под ним — поверх адреса возврата, который стаб положил инструкцией
раньше. `ret` ушёл туда, куда сказали эти байты.

**Причина.** Шлюз прерывания сбрасывает `IF`, `TF`, `NT`, `RF` и `VM` — и не
трогает `DF`. Обработчик, вошедший из CPL 3, начинает работу с `RFLAGS`
процесса. А обработчики ядра написаны на Rust, то есть скомпилированы под
обещание System V AMD64, что `DF` сброшен, — включая `memcpy`, который
компилятор подставляет на присваивание структуры. Процесс держит `DF` десяток
инструкций внутри собственного `memmove` (`std … cld`, копирование
перекрывающихся байтов назад); тик, попавший в это окно, вносил флаг в
планировщик. Отсюда и «раз в тридцать загрузок», и побайтово одинаковый адрес:
цель не случайная, случайно только попадание в окно.

**Вторая дверь в ядро всё это время была права.** `IA32_FMASK` сбрасывает на
`syscall` ровно `TF`, `IF`, `DF`, `NT` и `AC` — то самое правило, которого не
было у двери прерываний. Исправление формулирует его один раз (`NUCLEUS_FLAGS`
в `exception.S`) и ставит в начало всех трёх стабов, которые зовут Rust:
`exception_common`, `timer_stub`, `spurious_stub`. Не маска поверх
унаследованного, а известное состояние: обработчику из `RFLAGS` не нужно
ничего — прерванные флаги лежат в кадре, который положил процессор, оттуда их
читает обработчик и туда же их возвращает `iretq`.

**Гейт — `host-tools/qemu-test/direction-flag.sh`.** Удача убрана с обеих
сторон: процесс держит `DF` не десяток инструкций, а сотни миллионов (один
asm-блок без единого обращения к памяти, так что сам процесс от флага не
страдает и отказ может быть только в ядре), а ядро запускается с двумя
процессами, потому что при одном `preempt` возвращается **до** копирования
кадров и враждебному флагу нечего портить. Доказательство того, что вопрос
вообще был задан, даёт сам процесс: два чтения тика вокруг окна, без единого
системного вызова внутри, — тик сдвинулся, значит прерывания брались, пока
`DF` был поднят.

Гейт проверен в обе стороны, и это здесь главное:

- с исправлением — PASS, 2 процесса, у каждого 1174 кванта, `held_begin=806
  held_end=2956`;
- без исправления (одна строка убрана из `exception.S`) — отказ **побайтово
  тот же**, что был записан открытым: `vector=14 error=0x11 rip=0x08023000
  cr2=0x08023000 cs=0x08 rsp=0x02013738`.

**Тупиковая попытка, записанная, чтобы её не повторили.** Сначала гейт был
построен на `NT`: процесс может его поднять, а `iretq` в long mode с поднятым
`NT` даёт `#GP`, и держать его можно всю жизнь процесса, в отличие от `DF`.
Гейт прошёл **и на неисправленном ядре** — потому что шлюз прерывания `NT`
сбрасывает сам, и проверять там было нечего. Признак, по которому это видно
заранее, один: список сбрасываемых шлюзом флагов, а не правдоподобность
рассуждения. Свидетелем должен быть тот флаг, который отказал.

`./scripts/preflight.sh --full` — PASS, 51 гейт.

Проверено после исправления: 60 подряд загрузок гейта deputy — чисто (раньше
отказ примерно 1 из 30).

### 2026-08-20 — ADR-0060, треть третья: движок выходит на границе вызова

Схема интерфейсов была принята и доказана как **артефакт** (модуль называет
интерфейс, верификатор это проверяет). Здесь — что происходит, когда такой
артефакт исполняется.

**Порт, а не путь.** У движка появился `System` — одна операция `reach`, которой
он отдаёт путь интерфейса, имя операции и аргументы и получает значение. Движок
не выполняет ни одной операции и не знает, что они значат. Это же и есть
требование ADR-0060 §3 «движок должен уметь выходить на границе вызова и быть
там же продолженным»: выход — это `reach`, продолжение — его возврат. Ничего не
разматывается и не восстанавливается из сохранённой позиции — блокирующая
операция останавливает **процесс**, а кадр движка просто не исполняется, пока
это длится; переносит приостановленный прогон через границу то, что
приостанавливает хост (в образе — trap frame процесса, ADR-0059).

**`Value::Capability(Handle)`.** Полномочие как непрозрачное число, которое
движок только переносит: не сравнивает, не печатает, не преобразует и не
производит. `Debug` у `Handle` написан руками и печатает `capability`, рендерер
значений — тоже: `docs/42` §2 держит конкретное представление handle вне
провенанса и логов, а диагностика — не исключение из этого.

**Система — аргумент прогона, а не окружение.** `run_set` принимает её явно, и
вызывающий, которому нечего предложить, говорит это, передавая `Unreachable` —
не заглушку, возвращающую ноль, а отказ `RUNTIME_INTERFACE_UNREACHABLE`. Ноль
был бы неотличим от успешной операции.

Доказательства (`tests/integration/tests/interface_reach.rs`, 5 тестов):

- движок выходит ровно один раз, за тем интерфейсом и той операцией, которые
  названы в тексте, и несёт первым аргументом ту самую capability (ADR-0056);
- возвращённое значение — хоста, а не движка;
- **порядок эффектов детерминирован, значения — нет**: два прогона одного модуля
  на одном входе делают одни и те же вызовы в одном порядке и получают разные
  ответы; `fuel_used` совпадает;
- **вызов оплачен до того, как сделан** (`SYSTEM_INTERFACE_V1` §6): бюджет
  подбирается прогоном, а не угадывается, и при бюджете на единицу меньше
  достаточного хоста не спрашивают вовсе;
- handle не попадает ни в рендерер значений, ни в `Debug`.

**Чего здесь нет и почему.** Образ рантайма передаёт прогону `Unreachable`. Не
из осторожности: **не решено, как endowment процесса связывается с параметрами
entry-функции модуля.** `SYSTEM_INTERFACE_V1` §10.3 называет `CapabilityDenied`
при старте, но ни один принятый документ не фиксирует, что с чем сопоставляется —
а без capability модуль не может назвать ни одной операции (все операции схемы
берут её первым аргументом). Реализовывать отображение операций на
`SYSTEM_ABI_V1` сейчас значило бы положить в загрузочный путь код, который
нечем исполнить и нечем проверить. Это решение уровня 2, и оно вынесено ниже.

`./scripts/preflight.sh --full` — PASS, 51 гейт.

### 2026-08-20 — ADR-0061 (Proposed): чем нарисована стрелочка import → grant

Написан как Proposed; решение за Project Architect. Разбор дал три факта,
**два из которых опровергли первую редакцию рекомендации этого же ADR** — это
записано здесь именно потому, что первая редакция была написана из общих
соображений, а не из документов.

1. **Два импорта одного интерфейса законны.** Проверено прогоном: `as input` и
   `as output`, оба `system.ipc.Endpoint` — checker чист, verifier `Ok`. Значит
   путь интерфейса **не уникальный ключ**, и «сопоставлять по типу объекта» —
   это проверка, а не стрелочка.
2. **`binding` уже в артефакте и уже в дайджесте.** `tos-ir/v1` несёт
   `CapabilityImport { interface, binding, ty }`, и `digest.rs` пишет в дайджест
   модуля обе строки. Имя импорта — часть идентичности модуля, покрытая receipt
   и cache identity, а не локальное удобство фронтенда.
3. **`PROCESS_IDENTITY_V1` §7.3 требует ключа**: «A denied capability appears as
   a difference between the requested and granted sets, and the process's
   `CapabilityDenied` startup failure names it». Разность множеств не вычислима
   без идентичности элементов, а позиция становится идентичностью только если
   launch record несёт и отказы тоже.

Отсюда: три **оси**, а не три варианта — ключ (чем назван запрос), проверка (что
грант годится по виду объекта) и решатель (константа launcher'а сейчас,
`/system/policy/` потом). Рекомендация: **B** для поверхности (связывает
`import capability`, а не параметры `main`), **binding** как ключ, вид объекта
как проверка под любым ключом, соответствие едет в launch record явно.

**И одна хорошая новость по цене.** `Op::Capability { import, right, operands }`
**уже есть в `tos-ir/v1`**, и верификатор уже проверяет `import` по границе
`capability_imports.len()`. Операндная форма, которой требует B, лежит внутри
закрытой Stage 2 схемы — ADR-0028 трогать не придётся.

**Принят (Project Architect, 2026-08-20): surface B, ключ — binding.**
Идентичность запроса — `(module identity, binding)`. Проверка соответствия
интерфейса виду объекта. Решатель — константа launcher'а сейчас,
`/system/policy/` потом; ключ от этого не меняется. Транспорт — launch record
несёт binding явно, а не полагается на положение записи в массиве.

**Поправка Project Architect, внесённая в текст ADR:** §7.3 не вынуждает
математически выбрать binding — он вынуждает иметь **стабильную идентичность
элемента**. Её можно было бы дать и explicit ID, и позицией с дополнительной
механикой. Binding выигрывает не по логической необходимости, а потому что уже
существует, уникален внутри модуля и уже входит в digest: выбрать что-то ещё
значит построить вторую идентичность рядом с уже несущей нагрузку. Первая
редакция рекомендации формулировала это сильнее, чем следует, и исправлена.

### 2026-08-20 — `IPC_V1` §2: у endpoint'а один получатель, и теперь это проверяется

Дефект найден аудитом §9 против того, что реально проверяется. `IPC_V1` §2
говорит «An endpoint has exactly one receive-rights holder at a time», §9.4
требует этого как conformance evidence — и **не проверялось ничем**. Launcher мог
выдать двум процессам `receive` на один endpoint, и оба брали бы сообщения, а
кому досталось конкретное — решал бы порядок вызова.

Правило живёт в `capability::grant`, потому что это единственная дверь, через
которую запись в таблицу вообще появляется. Проверять на `endpoint_receive`
означало бы проверять после того, как правило уже нарушено: право держат оба, и
отказ второму *вызову* сделал бы исход зависящим от порядка вызовов — ровно та
недетерминированность, ради которой правило существует.

**Holder читается как процесс, а не как capability.** Недетерминированным второй
holder делает то, *в какой контекст* доставлено сообщение; два handle внутри
одного контекста такого вопроса не создают. Значит процесс может аттенюировать
своё receive и держать оба результата, а второй процесс — не может держать ни
одного.

`grant` теперь возвращает причину вместо `Option`: полная таблица — это граница,
которую выбрало ядро, а второй получатель — правило, которое выбрал `IPC_V1`, и
путать их значит отправить читателя лога искать не там. Отказ константе
launcher'а виден на записи — `TOS.RUN.ENDOWMENT_REFUSED` с причиной.

Гейт `second-receiver.sh` грузит константу launcher'а, которая **неправа
намеренно**: просит ровно то, что §2 запрещает. Иначе вопрос не задать — гейт над
правильной константой доказывал бы, что launcher корректен, а не что правило
работает. Проверяется: отказ называет правило, а не полную таблицу; первый
процесс право **получил** (иначе `grant`, отказывающий всем, прошёл бы гейт);
второй стартовал, не держа ничего; и ожидание выжившего получателя затем отменено
правилом живучести — потому что единственный, кто мог бы ему послать, теперь не
держит ничего.

`./scripts/preflight.sh --full` — PASS, 52 гейта.

### 2026-08-20 — ADR-0061 реализован: модуль на TOS Core зовёт систему

**Ответ на вопрос идентичности Stage 3 перестал быть «нет».** `docs/37`
спрашивает: «do textual processes exercise real capability/IPC contracts rather
than running as decorative scripts around privileged binary services?» Всё, что
построила Phase 4, проверялось Rust-образом рантайма — привилегированным
бинарником, — а текстовый модуль считал число. Теперь загрузочный текст сам
просит capability и выполняет операции.

**Документы.** `SYSTEM_INTERFACE_V1` §4 получил вид объекта для каждого
интерфейса — стык, у которого до сих пор у `CAPABILITY_V1` §3 был один конец, а
у схемы другой. §3 показывает форму вызова: `endpoint_send(endpoint, 8u64)`,
`main()` без параметров.

**Launch record v4.** Каждый грант несёт **binding, на который он отвечает**. Не
позицию: позиция различила бы два импорта одного интерфейса, но перестановка
двух строк `import capability` молча поменяла бы полномочия местами, а политику
можно было бы писать только против числа. `process_create` несёт то же от
родителя к ребёнку; права ребёнка над собой едут в регистре, имя — в отдельном
слоте области аргументов, потому что права это значение, а имя нет.

**Lowering.** Первый аргумент операции не опускается вовсе — это *имя* импорта,
и оно становится полем инструкции. `Op::Capability` в `tos-ir/v1` уже был
объявлен ровно для этого («an operation on a declared imported capability»), так
что **закрытая схема Stage 2 не тронута**. Следствие сильнее, чем правило
`docs/42` §2 о представлении handle: capability не может утечь из места, которого
она не занимает — в артефакте нет ни одного операнда, который её держит.

**Верификатор** получил третью проверку к двум прежним: интерфейс, названный
инструкцией, и интерфейс импорта, под которым она выполняется, должны совпадать.
Артефакт, где они расходятся, прошёл бы всё остальное — индекс в границах,
интерфейс импортирован, функция его объявила — и выполнял бы операцию на
полномочии не того типа.

**Движок** спрашивает хост про каждый запрос **до первой инструкции**, по
интерфейсу и по имени, и отказывает всему прогону `CapabilityDenied` с именем
binding'а. Это стартовый отказ `docs/42` §2 и «never reaches the call»
`SYSTEM_INTERFACE_V1` §10.3, а не сюрприз на месте вызова после уже сделанной
работы. Capability стала свойством прогона, а не модуля: один и тот же артефакт
под разными грантами достаёт разные объекты, не меняясь ни на байт.

**Образ рантайма** реализует `System`: отвечает на запросы из launch record по
имени, проверяет вид объекта против §4 и выполняет операции вызовами
`SYSTEM_ABI_V1`. Таблица номеров живёт именно здесь, а не во фронтенде —
фронтенд, знающий системный ABI, был бы вторым местом, где тот объявлен, а
`docs/42` §5 версионирует их раздельно. Гейт схемы теперь держит вместе **три**
пары: операции, виды объектов и номера ABI; каждая проверена внесением
расхождения.

**Доказательство — `module-operation.sh`.** Загрузочный текст `profile full`
(иначе нельзя: `docs/42` §3 запрещает `extern` в bootstrap) просит один
capability, получает `send` и не получает `receive`, и делает две операции:

```
TOS.RUN.REQUEST binding=endpoint interface=system.ipc.Endpoint object=1 wanted=1
TOS.RUN.INTERFACE operation=endpoint_send status=0
TOS.RUN.INTERFACE operation=endpoint_receive status=-1
TOS.RUN.COMPLETED value=i64:1
```

Доказательство — **разница между двумя ответами**, а не любой из них. Модуль,
подделавший бы любой, не мог бы получить два разных статуса; и возвращаемое
значение составлено так, что ни один статус в одиночку его не даёт
(`0 - (-1) = 1`; отказать обоим или разрешить оба даёт 0). Плюс проверяется
порядок: `send` дошёл до системы раньше `receive`, как написано в модуле.

Восемь свойств движка — в `interface_reach.rs`, и одно из них то самое, ради
которого выбран ключ: модуль с двумя импортами одного интерфейса достаёт два
разных объекта, а перестановка двух строк `import capability` не меняет, какое
имя получает какой объект.

`./scripts/preflight.sh --full` — PASS, 53 гейта.

### 2026-08-20 — `IPC_V1` §9.1 и §9.3: три границы одной таблицы отвечали по-разному

Продолжение аудита §9 против того, что реально проверяется. Найдено расхождение
внутри одного контракта: **три границы `IPC_V1` §3 отвечали двумя разными
статусами**. Payload сверх 256 байт — `E_BAD_ARGUMENT`; пятая capability в
сообщении — `E_LIMIT`.

По букве `SYSTEM_ABI_V1` §4 подходили оба: `E_LIMIT` — «a declared bound would be
exceeded», `E_BAD_ARGUMENT` — «an argument was outside its declared domain».
Решает не буква, а **что из них полезно**: `E_LIMIT` — это ответ полной очереди
(§9.2), то есть «попробуй позже», а три числа §3 суть константы контракта,
известные вызывающему до вызова, то есть «этот вызов не сработает никогда».
Отвечать на них одинаково значит слить два разных факта — ровно то, что §4
запрещает делать с парой `E_NO_CAPABILITY`/`E_BAD_HANDLE`, и по той же причине.
Исправлено в сторону `E_BAD_ARGUMENT`, у `endpoint_send` и `endpoint_call`.

Доказательства добавлены на существующую строку `TOS.RUN.IPC.SENT`:

```
oversize=-3   payload на байт длиннее максимума        (§9.1)
overcount=-3  на одну capability больше четырёх        (§9.1)
unheld=-2     передача handle, которого процесс не держит (§9.3)
```

**Ожидание по `unheld` было моим, и ядро оказалось правее.** Я ждал
`E_NO_CAPABILITY`; пришло `E_BAD_HANDLE`, потому что индекс `0xdeadbeef` лежит вне
таблицы, а это «ты не назвал ничего», а не «у тебя нет прав» — первый шаг порядка
отказа ADR-0056. Записано так, как ответила система.

**Что из §9 остаётся незакрытым:** §9.6 (линейная передача региона) — регионы не
объект этой стадии, и **режим передачи региона в сообщении не определён ни одним
принятым документом**: `IPC_V1` §5 делает линейность свойством, которое
«объявляет интерфейс», а `SYSTEM_INTERFACE_V1` §8 прямо не объявляет ни одной
операции с регионом. §9.7 (бюджеты §8) упирается в отдельный вопрос ниже.

`./scripts/preflight.sh --full` — PASS, 53 гейта.

### 2026-08-20 — ADR-0061, пункты 3 и 4 списка доказательств: два отказа при старте

Список доказательств ADR-0061 требует не только удачного случая. Пункты 3 и 4 —
запрос, на который никто не ответил, и запрос, на который ответили **объектом не
того вида**. Оба доказаны на загрузочном пути, той же капсулой; отличается только
константа launcher'а, и это делает три загрузки одним экспериментом, а не тремя.

- **Никто не ответил.** Production-ядро: его константа не выдаёт ничего, потому
  что `system.boot.init` обычно ничего не просит. То есть это обычный launcher,
  встретивший модуль, который просит, — без всякой тестовой сборки.
- **Ответили не тем видом.** `test-wrong-kind` выдаёт полномочие над **процессом**
  под именем, под которым модуль просит **endpoint**. Имя совпало, вид нет — тот
  единственный случай, ради которого в `SYSTEM_INTERFACE_V1` §4 появилась колонка
  вида объекта. Отказ при старте, а не на первом вызове.

Оба дают `TOS.RUN.REFUSED stage=execute reason=capability-denied binding=endpoint
interface=system.ipc.Endpoint` и загрузку `RESULT_BOOT_MODULE_FAILED` — модуль,
который не может стартовать, не даёт успешной загрузки, и это правильный исход,
а не терпимый.

**Рендерер отказов исправлен по дороге.** `reason=` — это поле, а значение с
пробелами это два поля для любого читателя, который делит по ним. Раньше
`entry-arity expected=… actual=…` превращалось в `reason=entry-arity_expected=…`.
Теперь причина — один токен, а её части идут отдельными полями и ищутся по имени.

**Дефект гейта, найденный собственной аварией.** Я собрал `test-wrong-kind` без
отдельного `CARGO_TARGET_DIR` и затёр production-ядро. Обе «разные» загрузки
выдали побайтово одинаковые логи — и гейт это принял, потому что проверял только
строку отказа, которая у них общая. **Две загрузки с одинаковым логом — это одно
доказательство, а не два.** Теперь каждая проверяет ещё и *насколько далеко дошёл
запрос*: у первой нет ни одной строки `TOS.RUN.REQUEST` (никто не ответил), у
второй ровно одна, `object=3 wanted=1` (ответили, но не тем). Это записано в
самом гейте вместе с причиной, по которой там появилось.

`./scripts/preflight.sh --full` — PASS, 53 гейта.

### 2026-08-20 — Второй интерфейс схемы: модуль завершает собственный процесс

`system.process.Control` — вторая половина `SYSTEM_INTERFACE_V1` §4, и она не
повторение первой по двум причинам. Её capability именует **процесс**, так что
это единственная загрузка, где стартовая проверка вида объекта работает против
второго вида, а не против того, который случайно оказался первым в таблице. И
`process_terminate` — единственная операция схемы, чей эффект виден без того,
чтобы модуль о нём сообщал: процесс, который она заканчивает, — тот самый, что её
позвал.

**Один текст модуля, две константы launcher'а, отличающиеся одним битом.** Грант в
обоих случаях — настоящее полномочие над этим процессом; меняется только наличие
`terminate`. Значит два исхода не объяснимы ни модулем, ни капсулой, ни образом
рантайма, ни кодом ядра — только маской прав:

```
без terminate:  TOS.RUN.INTERFACE operation=process_terminate status=-1
                TOS.RUN.COMPLETED value=i64:-1
c  terminate:   TOS.RUN.PROCESS_TERMINATED process=0 by=0
                (строки TOS.RUN.INTERFACE нет вовсе)
```

**Отсутствие строки статуса — более острая половина доказательства.** Хост пишет
`TOS.RUN.INTERFACE` *после* возврата из вызова, значит её отсутствие говорит, что
вызов не вернулся: процесс был закончен внутри него. В отказной загрузке та же
строка присутствует, от того же хоста, на той же операции.

**Найдено при написании и стоит того, чтобы записать.** Право `create` — одно из
двух прав объекта-процесса, поэтому образ рантайма, держащий такой грант, уходит
в свою супервизорную ветку и создаёт ребёнка над тем же загрузочным текстом,
не наделив его ничем. Ребёнок просит `control`, ему никто не отвечает, и он
отказывается при старте с именем binding'а. Это не помеха: это преднамеренный
случай `module-operation.sh`, пришедший сюда сам собой, — и загрузка **без** него
означала бы, что грант, на котором стоит вся первая половина, не был настоящим
полномочием над процессом. Поэтому он проверяется, а не отфильтровывается.

Ещё одна мелочь того же рода: комментарий у последней проверки утверждал прямо
обратное тому, что она проверяет. Исправлен по логу, а не по памяти.

`./scripts/preflight.sh --full` — PASS, 54 гейта.

### 2026-08-20 — ADR-0062 (Proposed): аргументы, которые не влезают в регистр

Из двух оставшихся заблокированных направлений взято то, что разблокирует
больше. `SYSTEM_INTERFACE_V1` §4 сам называет причину, по которой
`process_create` отсутствует: он берёт имя модуля и endowment, а «TOS Core V1 has
no way to write into that region, because it has no pointers and this schema
admits none». Все операции схемы берут capability и максимум один `u64` — не по
совпадению, а потому что это самое большое, что схема умеет описать.

**Заблокировано этим — остаток Stage 3.** Супервизор, читающий `/system/policy/`
(ADR-0051 §3), — это `.tos`-модуль, который зовёт `process_create` с путём
модуля и endowment'ом. На вопрос идентичности docs/37 уже отвечено для операций,
влезающих в регистр; супервизор — то место, где на него отвечают для собственной
структуры системы.

Что документы **уже** решили и что менять нельзя: модуль никогда не держит
указатель (`docs/39`, §3 схемы); ядро никогда не ходит по адресу, выбранному
процессом (`SYSTEM_ABI_V1` §3); байтам уже отведено место — область аргументов и
фиксированные смещения ADR-0058; и хост уже стоит между модулем и ABI —
он уже достаёт handle из значения и кладёт в `rdi`. Маршалинг — то же действие
над более длинным аргументом.

Чего не решено: может ли принятая схема объявить параметр **не скалярного** типа,
и если да, что происходит между значением модуля и чтением системы.

Рекомендация: **A сейчас** (`text`, маршалит хост) с **границей как частью
решения** — схема, объявляющая параметр переменной длины, объявляет его максимум,
и значение сверх него отказывается до вызова, тем же статусом, что и слишком
длинный inline-payload. Иначе «насколько длинным может быть аргумент модуля»
отвечает тот хост, который его запустил. Потом — **узкая форма B** (несколько
скалярных аргументов, которые хост складывает в раскладку, зафиксированную ABI),
потому что она достаёт `process_create`, не кладя в схему ни типа, ни раскладки:
схема, объявляющая тип записи, объявляла бы то, что объявляет и язык, а два
объявления одной формы расходятся.

Не C (регион) первым: он тянет за собой вопросы регионов, на которых уже стоит
`IPC_V1` §9.6, то есть требует ответить на контракт региона раньше, чем появится
сервис, которому регион нужен. Не D (супервизор остаётся на Rust): это условие
провала docs/37, переписанное как план.

Статус Proposed. Против него ничего не реализовано.

### 2026-08-20 — `IPC_V1` §9.7, счётная половина: цена посчитана, а не оценена

§9.7 требует, чтобы пересечения границы и копии были **посчитаны**, а не
оценены. Считать может только ядро: процесс не видит, сколько раз он пересёк
границу, а копия внутри ядра снаружи невидима вовсе.

Счётчики стоят **рядом с тем, что считают** — по инкременту у каждого
`copy_nonoverlapping` полезной нагрузки, — так что копию, которую забыли учесть,
пришлось бы написать мимо счётчика, стоящего вплотную. Отчёт:

```
TOS.RUN.IPC.COST messages=4 payload_copies=6 ipc_in=12 other_in=98 returns=106 resumptions=4
```

Шесть чисел и **никакой арифметики**: отношения, которые ограничивает §8,
вычисляет тот, кто читает. Ядро, сообщающее «2 копии на сообщение», сообщало бы
своё мнение о делении, а не то, что посчитало.

**Закрыто:** копии. §8 даёт две на inline-сообщение; измерено 6 на 4, и гейт
request-reply это проверяет. Отношение выходит *ниже* границы, а не на ней,
потому что ответ стоит одну копию, а не две: он идёт из области отвечающего прямо
в область уже ждущего его вызывающего, очередь ему не нужна. Загрузка, давшая бы
ровно две на сообщение, означала бы, что ответ поставили в очередь.

**Посчитано, но ещё не отнесено к обмену:** пересечения. §8 ограничивает их «на
один request/reply», а итог загрузки — не это: `time_monotonic` в цикле ожидания
пересекает ту же границу и не принадлежит никакому обмену. Поэтому счётчик
разделён (`ipc_in` — только четыре операции обмена; `returns` — вызовы,
вернувшиеся через грань, чего заблокированный не делает; `resumptions` —
контексты, вошедшие через планировщик, то есть обратное направление для тех же).
Вытеснения нет ни в одном: тик возвращается через свой стаб — ровно то
исключение, которое §8 и называет. **Чего не хватает — привязки счётчика к одному
обмену**, и это следующий шаг, а не закрытый пункт.

**Не сделано и почему.** Временну́ю половину §8 (p99, 200 µs, «не более 8×
внутрипроцессного вызова») не измеряю: это упирается в конфликт контрактов,
вынесенный ниже, а не в отсутствие кода.

`./scripts/preflight.sh --full` — PASS, 54 гейта.

### 2026-08-20 — Попытка привязать пересечения к одному обмену: инструмент отвергнут

Записано как неудача, потому что результат был бы опубликован как измерение.

Сделано было так: загрузка, в которой IPC — ровно один request/reply и больше
ничего (образ рантайма под отдельным флагом делал один `endpoint_call` на одной
стороне и один `endpoint_receive` + один `endpoint_reply` на другой), плюс
счётчик исходящих пересечений, относимых к операции: возврат через грань считался
там же, где вход, а возврат через планировщик — по флагу на слоте, поставленному
там, где ожидание кончилось.

Первый прогон дал ровно то, чего ждёшь от такой схемы:

```
exchanges=1  ipc_in=3  ipc_out=3   → шесть пересечений на один request/reply
```

§8 разрешает четыре. И причина видна из самих счётчиков: сервер делает `receive`
и `reply` **двумя** операциями — два входа и два выхода, — плюс вызов клиента.
Система, укладывающаяся в четыре, отвечала бы и снова вставала в ожидание одной
операцией, а такой в этом ABI нет.

**Но число не воспроизвелось.** Четыре прогона подряд: `ipc_out` то 2, то 3, и —
что решает дело — **два прогона с одинаковым профилем пересечений
(`returns=77 resumptions=4`) дали разные `ipc_out`.** Значит расходится не
расписание, а моя привязка: она иногда теряет выход операции. Число, которое то
пять, то шесть, — это оценка в одежде счётчика, ровно то, что §9.7 запрещает.

**Поэтому инструмент удалён, а не оставлен с оговоркой.** Убраны: гейт
`exchange-cost.sh`, флаг образа рантайма `test-exchange-cost`, счётчик
`ipc_out` и флаг слота, его питавший. Оставлено то, что считается верно:
`messages`, `payload_copies` (граница §8 по копиям закрыта и проверяется),
`ipc_in`, `other_in`, `returns`, `resumptions`, `exchanges`.

Гипотеза о шести пересечениях **остаётся гипотезой** и записана здесь как таковая:
она согласуется с устройством ABI и с первым прогоном, и её не подтверждает
инструмент, который сам себе противоречит. Правильный следующий шаг — не чинить
привязку наугад, а сначала объяснить расхождение двух прогонов с одинаковым
профилем.

### 2026-08-20 — Расхождение объяснено: у возврата операции **две** двери

Причина найдена чтением, а не подбором. Вызов, который заблокировался, не
возвращается через грань: его кладут и поднимают позже — и поднять его могут
**две разные вещи**: планировщик, входящий в контекст, или тик таймера,
переключающийся на него (`preempt` меняет кадры и уходит через `iretq`). Какая
именно — зависит только от того, заблокировался ли будивший процесс сразу или
успел дожить до тика. Поэтому `returns` и `resumptions` совпадали, а привязка
расходилась: она смотрела на одну дверь.

Тик — это вытеснение, и §8 его исключает. Возврат операции — не вытеснение; он
лишь воспользовался той же дверью. Счёт теперь берётся у **обеих**, и у
инструмента появился инвариант, который раньше нарушался:

```
ipc_in=12 … ipc_out=12     ipc_in=13 … ipc_out=13    (шесть прогонов подряд)
```

**Каждая вошедшая операция вернулась ровно один раз.** Разброс 12/13 — это
настоящая разница трафика (опрос, находящий или не находящий сообщение), а не шум
инструмента. Инвариант проверяется гейтом request-reply, и там же написано,
почему он там: потому что однажды он был ложным.

**И тогда шесть перестаёт быть гипотезой.** Сбалансированный счётчик говорит, что
операция IPC стоит ровно два пересечения, по одному в каждую сторону. Обмен — это
три операции (`endpoint_call`, `endpoint_receive`, `endpoint_reply`), то есть
шесть пересечений, при разрешённых §8 четырёх. Уложиться в четыре может только
операция, которая отвечает и снова встаёт в ожидание одной парой пересечений;
в `SYSTEM_ABI_V1` такой нет. **Это дополнение к ABI, а не настройка чего-либо
здесь**, и оно вынесено ниже как расхождение с контрактом, а не молча пропущено.

`./scripts/preflight.sh --full` — PASS, 54 гейта.

### 2026-08-20 — ADR-0062 принят по документам: модуль запускает процесс

Выбран вариант A — и **это чтение, а не изобретение**, что и записано в самом ADR.
`SYSTEM_INTERFACE_V1` §3 уже говорит: «The remaining parameters are **values**.
No parameter is a pointer, because TOS Core V1 has none.» `string` — значение
TOS Core V1, а не указатель. То, что первые пять операций брали только `u64`, —
факт про эти пять операций, а не правило. Неопределённой была **граница**, и её
решает `SYSTEM_ABI_V1` §3 («never pointers the nucleus dereferences without
bounds») вместе с `MAX_MODULE_PATH` как собственным примером длины, заданной
контрактом, а не вызывающим.

**Что добавлено.** §4.1 схемы: какие типы параметра допустимы (`u64`, `string`) и
правило границы — параметр переменной длины объявляет максимум, значение сверх
него отказывается **до вызова**, тем же `E_BAD_ARGUMENT`, что и слишком длинный
inline-payload. И `process_create` в §4 — **без endowment**: endowment это список,
списка §4.1 не допускает, а ребёнок, наделённый ничем, — законченная операция, а
не половина. `SYSTEM_ABI_V1` §5 и так берёт счётчик endowment'а регистром, где
ноль — законное значение.

**Доказательство — `process-launch.sh`.** Модуль на TOS Core создаёт процесс:

```
TOS.RUN.INTERFACE operation=process_create status=0    ← имя, которое капсула несёт
TOS.RUN.INTERFACE operation=process_create status=-3   ← имя, которого не несёт
TOS.RUN.COMPLETED value=i64:3                          ← 0 − (−3)
TOS.RUN.PROCESS_EXIT process=1 …                       ← ребёнок, который действительно был
```

Одна операция, одна capability, один авторитет — различается только значение.
Возвращаемое число составлено так, что ни один ответ в одиночку его не даёт.
И ребёнок не статус, а процесс: он запустился, попросил `control`, ему никто не
ответил, и он отказался при старте — ровно так выглядит снаружи ребёнок,
наделённый ничем.

Гейт схемы теперь держит вместе **четыре** пары: операции, виды объектов, номера
ABI и объявленные границы параметров; последняя проверена внесением расхождения.

**Две ошибки, найденные исполнением, а не чтением.** `module` — ключевое слово
грамматики, так что очевидное имя параметра не парсится; и тип называется
`string`, а не `text`, — я написал `text` по аналогии с `Value::Text` движка.
Обе исправлены и в документе тоже, где пример был бы невыполним.

`./scripts/preflight.sh --full` — PASS, 55 гейтов.

### 2026-08-20 — Супервизор на TOS Core: то, что запускает сервисы, — это текст

`IPC_V1` §9.6 закрыт чтением: §5 делает режим передачи региона свойством
объявления интерфейса, `SYSTEM_INTERFACE_V1` §8 не объявляет ни одной операции с
регионом, значит регион не возникает и передавать нечего. Это решение документов,
а не пробел реализации, и теперь оно **проверяется** (`interface_schema.rs`:
ни один принятый интерфейс не именует регион и не берёт его параметром).

Дальше — супервизор. Два модуля, оба канонический текст, оба в одной капсуле:
`/system/policy/services.tos` говорит **что** запускать, `/system/boot/init.tos`
говорит **как**. ADR-0051 §3 помещает политику в `/system/policy/` как «canonical
source keyed by module name … canonical text like any other component; not a
binary configuration database» — модуль TOS Core и есть самое буквальное чтение
этого. Разделены намеренно: супервизор, несущий собственную нагрузку, — это
компонент, выдающий сам себе задание, то есть форма отказа по docs/37.

```
TOS.RUN.INTERFACE operation=process_create status=0    ← имя, которое капсула несёт
TOS.RUN.INTERFACE operation=process_create status=-3   ← имя, которого не несёт
TOS.RUN.COMPLETED value=i64:1
```

**Доказательство — число, которого нет ни в одном из двух модулей.** Два вызова
означают, что `policy.count()` вернул 2, а это знает только политика; два разных
ответа означают, что имена пришли откуда-то, а не из супервизора; и `1` не
написано нигде.

**Два настоящих дефекта, найденных этой работой — оба невидимые до неё.**

1. **Ядро: путь модуля съезжал на байт за модуль.** `path_offset` считал шагом
   `path.len()` — с ведущим слэшем, — а резервируется и пишется `relative(path)`,
   на байт короче. Последний путь вылезал за отведённое и попадал в таблицу
   endowment'а. Невидимо, пока у загрузки нет **обоих**: нескольких модулей и
   непустого endowment'а. У `module-set.sh` есть первое, endowment пуст —
   перекрывать нечем; поэтому гейт с двумя модулями этого не видел, а первый
   супервизор увидел сразу: `path=system/policy/services.to` вместо `...tos`.

2. **Фронтенд: индексация массива переменной не исполнялась.** Lowering клал
   `PlaceStep::Index(None)` для любого неконстантного индекса — шаг без позиции
   вообще, — и движок мог только отказать: «an index step reached execution
   without a value». `DynamicIndex` в `tos-ir/v1` есть с тех пор, как написан
   lowering `for`; не хватало не механизма, а его применения к обычному
   `a[i]`. Найдено написанием супервизора: политика — это массив имён, а любая
   настоящая политика — это массив имён. Регрессия закрыта host-тестом, включая
   выход за границу (`RUNTIME_INDEX_OUT_OF_RANGE`).

`./scripts/preflight.sh --full` — PASS, 56 гейтов.

### 2026-08-20 — Дрейф от ADR-0054 устранён; ADR-0063 (Proposed) с доказательством

**Дрейф оказался двойным, и второй половины никто не называл.** ADR-0054 в своей
таблице последствий требует двух вещей: «`SYSTEM_ABI_V1` gains operation 12
(minor version); **`PROCESS_IDENTITY_V1` gains an exit record**». Не было ни
того, ни другого.

- §5 ABI обрывалась на 11, хотя §3 того же документа **уже** называет три
  self-only операции, включая `process_exit`. То есть текст и таблица одного
  контракта расходились между собой.
- §3 `PROCESS_IDENTITY_V1` не имела записи о завершении вовсе.

Оба перенесены как есть, без нового решения. Запись о завершении — **три поля, а
не одно**, потому что ADR-0054 говорит: «the nucleus asserts *that* the process
exited and *when*, the process claims *with what*, and the two are never merged».
Одно поле `status` позволило бы прочесть заявление процесса о себе как вывод
системы — ровно то смешение, ради предотвращения которого весь этот контракт и
существует.

Закрыто гейтом `check-abi-operations.sh`: номера контракта и константы обеих
реализаций (диспетчер ядра и образ рантайма) сверяются, назначение обязано быть
`1..n` ровно по одному разу, а диспетчер обязан называть **каждую** назначенную
операцию — иначе номер, потраченный контрактом, отвечал бы `E_NOT_SUPPORTED`,
который §7 резервирует за операциями *более поздней* версии. Проверено внесением
расхождения.

**ADR-0063 (Proposed): операция, требующая двух capability.**

Сначала — то, что было велено проверить до предложения: **достижим ли предел 4
существующими контрактами. Нет, и это выводится.**

Четыре пересечения вынуждены: запрос существует у клиента на CPL 3 и должен
дойти до ядра (вход клиента); должен дойти до сервера на CPL 3 (выход сервера);
ответ вычислен сервером на CPL 3 и должен дойти до ядра (вход сервера); должен
дойти до клиента (выход клиента). Четыре разных события, по два на процесс, по
одному в каждую сторону. Значит **не меньше** четырёх, а `docs/35` разрешает
ровно четыре — **граница плотная**, запаса нет нигде.

Измерено: этот обмен стоит **шесть**. Лишние два опознаны точно — *возврат*
`endpoint_reply` и *вход* следующего `endpoint_receive`: между ними не
происходит ничего, сервер выходит, ответив, и тут же входит ждать.

Пять способов убрать их без новой операции разобраны и отвергнуты. Существенный —
флаг на `endpoint_reply` «и потом жди»: механизм есть (§6 даёт флаговый регистр),
но ждать надо на **endpoint**, а у `endpoint_reply` в `rdi` только reply. Вывести
endpoint из reply значит изготовить полномочие над другим объектом — то, чего
`CAPABILITY_V1` §2 не допускает. Значит вторая capability нужна **всё равно**;
флаг экономит номер, а не расширение.

**Половина уже решена документом.** `SYSTEM_ABI_V1` §3: «Should an operation ever
require two capabilities, this contract assigns their positions in §5 order when
that operation is added.» ABI это предвидел. Не решена **схемная** половина: у
`SYSTEM_INTERFACE_V1` §2 интерфейс имеет один тип capability, а §3 говорит, что
первый параметр — capability, а остальные значения.

И вместе с ней вскрылось второе: `docs/42` §2, который цитирует сама §3 схемы,
требует совпадения «the capability type, **requested operation/right**, …» —
а схема **никогда не объявляла право**. С одной capability это была скрытая дыра;
с двумя это разница между «ответь здесь и жди там» и «ответь здесь и жди там,
куда указывает второй handle».

Рекомендация: **S-A + B** — параметры-capability с собственным интерфейсом и
правом у каждого, и операция 13 `endpoint_reply_receive`. Номер 13 свободен:
проверено, что «тринадцатая операция» в ADR-0055 и ADR-0061 — это **отвергнутые**
варианты, а не назначения.

Реализации нет. Статус Proposed, решение за Project Architect.

### 2026-08-20 — ADR-0063 принят и реализован; операция 13 существует в артефактах

Принят вариант S-A + B. Схема объявляет для каждой операции, какое **право**
должна нести каждая её capability, — это `docs/42` §2 («the capability type,
requested operation/right, …») прибывшее туда, где контракт и так его обещал.
Объявлено для всех операций, а не только для новой: правило, сформулированное
для одной, было бы правилом об одной.

Операция 13 `endpoint_reply_receive` берёт две capability — reply, который она
тратит, и endpoint, на котором она затем ждёт, — в позициях, которые
`SYSTEM_ABI_V1` §3 назначил заранее. Обе разрешаются **до того, как использована
любая**: наполовину выполненная операция оставила бы вызывающего отвеченным, а
сервер не ждущим, то есть ровно то состояние, ради невозможности которого
операция и существует.

`Op::Capability` несёт остальные импорты индексами импортов, а не операндами:
capability в артефакте по-прежнему нет нигде, инструкция называет **какой
запрос** каждая из них отвечает, а это имя написал модуль. Дайджест их
покрывает — иначе два модуля с разной властью имели бы одну идентичность.

**Дыра, найденная написанием доказательств.** Передать endpoint туда, где
положен reply, **принималось**: extern-объявление сверялось со схемой, а
аргументы-capability в точке вызова — ни с чем, потому что у capability-binding'а
как значения нет типа, который checker выводит. Операция с двумя capability
приняла бы их в любом порядке. Checker теперь сверяет каждый аргумент-capability
с интерфейсом, которого требует эта позиция; verifier отказывает артефакту,
называющему один импорт дважды, и проверяет границы для каждого названного
импорта, а не для первого.

### 2026-08-20 — Обмен стоит четыре пересечения: измерено, а не заявлено

Реализация операции 13 была написана, но **список доказательств ADR-0063
(пункты 2–8) не построен**. Построение показало, что операция ни разу не
исполнялась: в ней было три дефекта, и первый делал её неработоспособной всегда.

1. **Длина ответа читалась не из того регистра.** `reply()` брал её из `rsi` —
   там её держит операция 4, — а §5 строка 13 кладёт длину операции 13 в `rdx`,
   потому что `rsi` занят **второй capability**. То есть операция 13 передавала
   сообщение, длиной которого был handle endpoint'а: `E_BAD_ARGUMENT` при любом
   вызове. Не «редкий случай» — полная неработоспособность, невидимая, пока
   операцию никто не выполнил.
2. **Флаги читались не из того регистра.** `receive()` брал их тоже из `rsi`.
   Для операции 13 это handle endpoint'а, а `NON_BLOCKING` — нулевой бит: handle
   нечётен примерно в половине случаев, и ждущая половина операции иногда не
   ждала бы. Оба места теперь получают регистр от вызывающего, потому что
   регистр — свойство операции, а не функции, которая её обслуживает.
3. **Счётчик не считал новую операцию.** `is_ipc()` перечислял четыре операции,
   и 13-й среди них не было. Инструмент, которым меряется §8, не видел операцию,
   ради выполнения §8 и добавленную: загрузка, уложившаяся в четыре пересечения,
   измерилась бы как стоящая **меньше**, чем стоит, — просто потому, что
   половина её работы не считалась.

И четвёртый, в аудит-записи: операция, в которой отменено ожидание, **выводилась
из того, чего ждали**. Сообщения на endpoint'е с ADR-0063 ждут две операции, так
что `TOS.RUN.BLOCK_CANCELLED` сказал бы `operation=2` о процессе, который
операцию 2 не вызывал. Номер операции теперь несёт слот (`blocked_in`), а не
догадка по виду ожидания.

**Мера — разность, а не итог.** Цикл сервера входят один раз и покидают один
раз, и эти два пересечения не принадлежат никакому обмену: загрузка с одним
обменом стоит 6, с тремя — 14. Решать, *какие именно* два были прологом, — это
решение, то есть оценка. Поэтому каждая форма сервера мерится **дважды**, на
одном обмене и на трёх, и берётся разность: постоянная часть сокращается сама,
без чьего-либо мнения.

Четыре загрузки, две формы сервера, один и тот же клиент. Под этими константами
IPC загрузки — это **сам обмен и ничего больше**: ни опроса, ни проб, ни
делегирования (иначе измерялась бы разность, а разность — оценка в одежде
счётчика, что уже было записано выше как отвергнутый инструмент).

```
две операции (endpoint_reply + endpoint_receive):   8 → 20   наклон 6
одна операция (endpoint_reply_receive):             6 → 14   наклон 4
```

**Четыре — это граница `docs/35`, и она достигнута.** Разность наклонов ровно
**2** — те самые два пересечения, которые ADR-0063 опознал поимённо: возврат
`endpoint_reply` и вход следующего `endpoint_receive`, между которыми не
происходит ничего.

**Счётчик проверяется с другой стороны границы.** Оба процесса считают свои
собственные операции и сообщают их; гейт держит сумму против `ipc_in` ядра.
Счётчик, который нельзя сверить снаружи, — это ядро, рассказывающее о себе.
Плюс инвариант `ipc_in == ipc_out` во всех пяти загрузках.

**Отказы — пятая загрузка.** Каждый спрашивается у **живой** reply-capability,
поэтому «не доставлено ничего» не утверждается здесь, а доказывается тем, что
случается дальше: тот же reply тратится после отказа и работает, и клиент
получает оба своих ответа.

```
swapped=-1  no_reply=-2  no_endpoint=-2  no_right=-1  spent=-1  sending=-1
```

Честно о пункте 2 списка: ADR просит «четыре отказа, различимые по статусу». Они
различимы **по двум** статусам, и это чтение контракта, а не пробел. Порядок
проверки — индекс, поколение, тип, права — назначен так, что всё после индекса
отвечает `E_NO_CAPABILITY` намеренно; различает отказы не число, а **какая из
двух capability отказана**, и это видно по тому, что после отказа уцелело.

Пункты доказательств ADR-0063: 1 — уже был (`check-abi-operations.sh`);
2, 3, 4, 5, 6, 7 — `exchange-cost.sh`; 8 — копии считаны там же (`≤ 2` на
сообщение), отсутствие аллокации структурно: у ядра нет аллокатора.

**Расхождение 0b закрыто.** Оно было записано как расхождение с контрактом —
шесть пересечений при разрешённых четырёх — и теперь это не расхождение, а
измерение.

`./scripts/preflight.sh --full` — PASS, 58 гейтов.

### 2026-08-21 — Расхождение ADR-0036 ↔ ADR-0039 rev 3: три ответа на один вопрос

Найдено при проверке статусов, а не при работе над функциональностью. Ничего не
исправлено: `docs/38` запрещает разрешать конфликт выбором реализации, а здесь
конфликтуют два **принятых** Tier 1 текста. Вынесено как **ADR-0064 (Proposed)**.

**Сначала факт, потом чтение.** Что реальный frontend делает сегодня
(`00664f8`; крейты фронтенда побайтово те же, что в `850f1b3`):

```
Event()            → E1213_NONCONSTRUCTIBLE_TYPE  type=Event       operation=construct
Task(1i32)         → E1213_NONCONSTRUCTIBLE_TYPE  type=Task        operation=construct
Mutex(1i32)        → E1213_NONCONSTRUCTIBLE_TYPE  type=Mutex       operation=construct
MutexGuard(0i32)   → E1213_NONCONSTRUCTIBLE_TYPE  type=MutexGuard  operation=construct
Event              → E1213_NONCONSTRUCTIBLE_TYPE  type=Event       operation=construct
Nowhere(0i32)      → E1202_UNKNOWN_VALUE_NAME     name=Nowhere
1i32 as Task<i32>  → E1213_NONCONSTRUCTIBLE_TYPE  operation=as
```

Третий факт не назван ни одним документом: **голое имя `Event` в позиции
значения тоже получает `operation=construct`**, хотя ничего ни к чему не
применялось. Поле неверно по существу, а не по коду.

**Что чему противоречит — точными цитатами.**

- ADR-0036 §1 (Accepted): «Writing one as a constructor is the
  nonconstructible-type error of ADR-0039»; §7 требует негативный вектор
  «applying a constructor to a guard type».
- ADR-0039 **revision 3** (Accepted): список операций `E1213` — только две формы
  `as`, «that is the whole list», а `Event()`, `Task(1i32)`, `Mutex(1i32)` —
  «the frontend already reports each as `E1202_UNKNOWN_VALUE_NAME`. Verified
  against the reference frontend, not assumed»; §4: вектор для `Event()`
  **намеренно отсутствует**.
- `docs/44`, реестр диагностик (Tier 2), строка `E1213`: «A predeclared type in
  value position is `E1202`, not this (ADR-0039)».
- Вектор R070 ожидает `E1213` для `MutexGuard(0i32)`.

Итого: реестр `docs/44` говорит одно, а реализация и вектор R070 — другое, и оба
ADR приняты. Совпадение реализации с вектором тут ничего не спасает: вектор был
написан под §1 ADR-0036, а реализация — под ту же строку, и оба разошлись с
ADR-0039 rev 3 и с реестром, который под неё написан.

**Как это произошло — по коммитам.** `b3832fe` принял ADR-0036 *как написан* и
**тем же коммитом** сузил ADR-0039 до revision 3, убрав ровно те формы, на
которые ссылается §1 ADR-0036: ссылка повисла в тот же миг, и никто этого не
назвал. `98533e9` добавил R070 по §7 ADR-0036. `b16cc6c` изменил
`Resolver::resolve`, и его собственное сообщение это фиксирует: «exposed a gap in
ADR-0039's landing … It is `E1213` with `operation=construct`». Это не пробел
в реализации решения — это **предложение revision 3, отменённое в коде**, причём
факт, на котором revision 3 была принята («verified against the reference
frontend»), после этого перестал быть верным.

**Иерархия сама не решает.** Оба ADR — Tier 1, одна дата, одна подпись, один
коммит, ни один не supersede'ит другой. `docs/38` про такой случай говорит не
«кто главнее», а «silent contradiction is invalid» и требует остановиться на
границе. `docs/44` — Tier 2 и согласован с ADR-0039, но Tier 2 не решает, какому
из двух Tier 1 ему соответствовать.

Одна асимметрия сужает чтение, не решая: строка decision level самого ADR-0036
говорит, что он добавляет «one diagnostic code», и это `E1402`. Значит ADR-0036
не назначает `E1213` и не расширяет его операции — §1 **ссылается** на ADR-0039.
При таком чтении противоречия нет вовсе, а есть повисшая ссылка. Чтение
доступное и связное — и `docs/38` всё равно не разрешает агенту принять его в
одиночку.

**Почему гейты этого не увидели — структурная причина.** Связь «форма → код»
в проекте есть и работает: это конформанс-вектор, и драйвер корпуса держит
фронтенд у каждого записанного кода. Чего гейты не могут — заметить форму, у
которой вектора **нет**: `check-stage2-language-contract.py` связывает код со
стадией и проверяет, что цитируемые коды зарегистрированы, поэтому фраза реестра,
называющая код для формы, за которой никто не написал вектор, — это проза,
которую не читает ни один гейт. ADR-0039 §4 исключил этот вектор намеренно,
рассудив, что ответ и так решён; в результате единственная фраза, хранившая
ответ, осталась непроверяемой, и «R070 ожидает `E1213`» и «реестр говорит
`E1202` для этой формы» проходят рядом друг с другом. И главное: тест, который поймал бы это, **назван самим
ADR-0039** в его architecture impact statement — «checker unit tests … for a
predeclared type in value position still being `E1202`» — и никогда не был
написан. Поэтому `b16cc6c` смог изменить ответ, ничего не покраснив: удалять было
нечего.

**Варианты и рекомендация — в ADR-0064.** Коротко: (A) чтение-делегирование,
форма — `E1202`, откат ветки резолвера, R070 исправляется на `E1202`, ADR-0036
получает revision 4 с исправленной ссылкой; (B) ADR-0039 revision 4 возвращает
форму в `E1213` — и у этого варианта есть факт, которого у revision 3 не было:
её обоснование («пришлось бы расширять грамматику») **ложно для реализации на
стадии резолвинга**, что доказывает работающий frontend; (C) отдельный код
`E1214`. Рекомендую **A** — по верности принятому, а не по вкусу: revision 3
позднее и уже́, принята на фактическом основании, и `docs/44` написан под неё.

Кода не менял. `./scripts/preflight.sh --full` — PASS, 58 гейтов.

### 2026-08-21 — ADR-0064 принят (вариант B): решает позиция, а не написание

Project Architect выбрал **B** с уточнением границы, и уточнение — часть решения,
а не примечание к нему. Нормативно зафиксировано в ADR-0039 **revision 4**:

| форма | код | `operation` |
|---|---|---|
| тип применён к аргументам: `Event()`, `Task(…)`, `MutexGuard(…)` | `E1213` | `construct` |
| то же имя, написанное отдельно: `Event` | `E1202` | *(нет)* |
| `as` с нонконструируемым типом, любая сторона | `E1213` | `as` |
| capability в любой из этих форм | `E1502` | — |

**Основание выбора, записанное в revision 4:** факт, на котором стояла revision 3
(«пришлось бы расширять грамматику»), **опровергнут работающим фронтендом** —
`docs/39` §5 даёт вызову и конструированию одну форму, её callee уже обычное имя,
и находка рождается на стадии резолвинга, до всякого типа. Плюс симметрия правила
`docs/40` §3: opaque runtime handle нельзя изготовить из данных — ни авторитет,
ни замок.

**Резолвер переписан по контексту, а не по имени.** Введена `Position`
(`Callee` / `Value`), и `resolve` принимает её от того, кто знает форму
выражения. Общего special-case имени нет и быть не должно: именно правило «по
написанию» превращало любое упоминание типа в конструирование, и голое `Event`
получало `operation=construct` — поле, ложное о тексте, который перед ним. Это
записано в самом ADR-0039 rev 4 как запрет реализации, а не как пожелание.

**ADR-0036 не правился и не нуждается в правке:** при варианте B его §1
(«writing one as a constructor is the nonconstructible-type error of ADR-0039») и
§7 верны как написаны. Повисшая ссылка закрылась с другого конца. `docs/44`
поправлен ровно на границу: строка `E1213` теперь называет обе операции, обе
стороны границы и precedence capability.

**Дыра в доказательствах закрыта — шесть пунктов.** Unit-тесты: применение типа к
аргументам (`Event`, `Task`, `Mutex`, `Channel`) → `E1213`/`construct`, **ровно
одна** диагностика на попытку; guard-тип отдельно (`MutexGuard(0i32)`, это §7
ADR-0036); голое имя в трёх позициях (`let`, другой `let`, аргумент вызова) →
`E1202` и **ни одной** диагностики с `operation=construct`; обе стороны `as` →
`E1213`/`as` и ноль `E1212`; `system.time.Clock()` → `E1502` и ноль `E1213`
(precedence §2 ADR-0039).

Вектора — **по одному с каждой стороны границы**, и это то, что делает дрейф
невозвратимым: R070 (`forged-guard.tos`, конструкторная форма, `E1213`) и **R081**
(`predeclared-type-in-value-position.tos`, голое имя, `E1202`). Фронтенд,
вернувшийся к правилу по написанию, роняет R081; фронтенд, потерявший
конструкторную форму, роняет R070. Ни одна половина больше не двигается молча.

Чего **не** построено и названо, чтобы не считалось построенным: линт, читающий
*условие* из реестра и держащий корпус по нему. Это обобщение за пределы одной
строки и отдельное решение о том, какая часть прозы реестра машиночитаема.

`./scripts/preflight.sh --full` — PASS, 58 гейтов.

### 2026-08-21 — Пункт E закрыт: манифест источников полон, и это теперь гейт

Compliance repair существующего контракта, не новое решение: `docs/38`
§Release check уже требует присутствия каждого Accepted ADR в
`docs/SPECIFICATION_SOURCES.txt`.

**Сравнение сделано механически по всему `docs/adr/*.md`, а не по подозреваемым
номерам,** и статус брался из собственной строки `- Status:` каждого файла.
64 файла: 63 Accepted, 1 Proposed (ADR-0044). Отсутствовали ровно четыре —
0061, 0062, 0063, 0064 — и добавлены в естественном порядке после 0060.
Проверено также обратное направление: записей, ссылающихся на несуществующий
файл, нет; дубликатов нет; не-Accepted документов в манифесте нет. ADR-0044
не добавлен: `docs/38` требует присутствия только Accepted и о других статусах
не говорит ничего, поэтому и гейт не требует. Публикация Proposed внутри сборки
полномочий ему всё равно не дала бы — `docs/38` исключает это дважды («listing a
path … does not by itself grant Tier 2 authority» и Tier 5 «never independent
authority»); полномочие берётся из строки статуса самого документа, оттуда же,
откуда его читает гейт.

**Почему дерево было зелёным — и это две разные причины, а не одна.**

1. `tools/build-specification.py --check` доказывает
   `generated output == output(listed inputs)`, но **не** доказывает
   `listed inputs == all required inputs`. Воспроизводимость — утверждение о
   списке; полнота — утверждение о самом списке. Недостающий вход отсутствовал
   по обе стороны сравнения, поэтому сравнение сходилось.
2. **Проверка полноты существовала — но только в CI**, встроенным python'ом в
   `.github/workflows/documentation-integrity.yml`, и никогда не входила в
   `preflight`. То есть локальный набор гейтов и набор CI разошлись: «PASS,
   58 гейтов» было правдой, пока `main` на GitHub падал. Проверено, а не
   предположено: шаг «Validate source manifest and accepted ADR coverage»
   падает **на каждом push начиная с коммита принятия ADR-0061**
   (2026-08-19 16:55 UTC), последний прогон — на коммите `613c34e`.

**Гейт добавлен, и он один на двоих.** `scripts/check-specification-manifest.py`
вызывается и preflight'ом, и CI; встроенная копия из workflow удалена — проверка,
существующая в двух реализациях, это проверка, которая разъезжается, и здесь она
уже разъехалась. Заодно исправлен дефект самой копии: она считала ADR принятым,
если слово «Accepted» встречается **где угодно** в строке статуса, а это другой
вопрос, чем «каков его статус».

Гейт проверяет четыре вещи и **не знает ни одного номера ADR**: каждый Accepted
ADR присутствует; ровно один раз; каждая запись манифеста указывает на
существующий файл; статус читается из файла, поэтому ADR-0065 и далее
подхватываются без правки кода. Не-Accepted не становятся обязательными — и не
запрещаются, потому что ни один принятый документ этого не требует; граница
названа в самом скрипте, чтобы не выглядела недосмотром.

Проверено внесением расхождения — по одному на каждое условие: удалённая запись
принятого ADR, дубликат записи, запись без файла, Proposed, ставший Accepted, и
ADR без строки статуса. Все пять красные, PASS только на исправленном дереве.

`./scripts/preflight.sh --full` — PASS, 59 гейтов.

### 2026-08-21 — Аудит CI ↔ preflight: что доказывается, а не чем запускается

Сверены все шаги четырёх workflow с `scripts/preflight.sh` — по **утверждению**,
а не по строке команды. Итог: 34 шага CI, из них 5 — подготовка среды и выгрузка
артефактов (утверждений не несут), 29 — проверки.

**Дублированных реализаций одного правила больше нет ни одной.** Единственная,
что была, — встроенная python-проверка полноты манифеста в
`documentation-integrity.yml` — удалена накануне. Остальные шаги CI вызывают те
же скрипты или те же cargo-команды, что preflight.

**CI-проверок, которых нет в preflight, ровно одна:**

| проверка | классификация |
|---|---|
| `stage1-performance-conformance.sh --evidence-status P2` (ADR-0026) | **настоящее расхождение**: скрипт есть локально, в preflight не вызывается. Это единственный случай, где `preflight --full` может быть зелёным, а CI на том же коммите — красным |

**Исправлено сразу, потому что следует из того, что CI и так о себе заявляет:**

| проверка | было | стало |
|---|---|---|
| `Workspace tests` (source-ci) | `cargo test` — а UEFI-загрузчик не входит в workspace default members, то есть его unit-тесты не запускались вовсе | `cargo test` + `cargo test -p tos-uefi-loader`, как в preflight, который это уже нашёл однажды |

**Preflight-only, и это вопрос не мой, а архитектурный** (ниже, пункт F). CI не
запускает ни одной из детерминированных проверок целостности репозитория, кроме
трёх документационных: authority интерфейсных контрактов, схема интерфейсов,
номера операций ABI, контракт boot-событий, exception foundation, provenance
встроенной графики, лаунчер `run-tos`, интерактивный режим QEMU, захват событий,
timed harness, Stage 1 workload и native-harness, консистентность языкового
контракта Stage 2, freestanding-исходники и их сборка — плюс QEMU-гейты
stage2-runtime, no-framebuffer, module-set, boot-module-failure,
capsule-size-limit, четыре инъекции исключений и три гейта изоляции процессов.

**Гейты, чьи собственные регресс-тесты не запускает никто.** Найдены механически
(нет вызывающих ни в preflight, ни в workflow, ни в других скриптах):
`scripts/tests/check-capsule-format-alignment.sh`,
`check-capsule-vector-provenance.sh`, `check-spdx-assembly.sh`,
`check-spdx-assets.sh`, `check-spdx-json.sh`,
`qemu-bootinfo-identity-mismatch.sh`. Плюс `scripts/check-generated-spec.sh` —
однострочная обёртка над тем, что preflight и CI вызывают напрямую; вторая дверь
к той же проверке, не вторая реализация.

Отдельно: `scripts/tests/check-unsafe-safety.sh` — **не** пробел паритета. Это
регресс-тест самого чекера на фикстуре; про репозиторий CI и preflight
утверждают одно и то же (`check-unsafe-safety.py`), а локально дополнительно
проверяется, что чекер всё ещё ловит недокументированный `unsafe`. Разное
утверждение, а не разный охват.

`./scripts/preflight.sh --full` — PASS, 59 гейтов.

### 2026-08-21 — ADR-0065 принят (A′): один инвентарь, CI называет профиль

**Пункт F закрыт решением, а не правкой.** Зафиксировано, что означает зелёное:
`preflight --full` — канонический локальный набор обязательных репозиторных
гейтов; job'ы репозиторной конформности на одном SHA обязаны покрывать каждый
гейт этого набора; CI может добавлять environment-специфичное сверх него.
Зелёный CI на коммите значит **не меньше**, чем зелёный `--full` на том же
дереве. Обратное не заявляется.

**Механизм — тот, при котором второму списку негде появиться.** Инвентарь один,
в `scripts/preflight.sh`, четыре поля на гейт: профиль, локальный scope, ярлык,
функция. `--list` печатает его и **ничего не исполняет** — это единственный
источник состава. **Workflow называет профиль и никогда не гейт**: пропустить
гейт нельзя, потому что YAML о гейтах не знает; единственное, что там может
сломаться, — незапущенный профиль, и это одна проверка вместо шестидесяти девяти.

```
docs        default   10   текст: спека, манифесты, контракты, реестр, паритет
provenance  default    3   полная история: DCO, SPDX, provenance графики
source      default    9   тулчейн Rust
source      full-only  1   фаззинг
selftest    default   13   фикстуры: гейты, проверяющие гейты
qemu        full-only 33   прошивка и эмулятор, включая ADR-0026
```

Профиль — это **класс среды**, а не тема: поэтому `capsule_provenance` (собирает
capsule-tool) в `source`, а не в `provenance`. Scope — локальная кадансировка и
на обязанности CI не влияет: `--profile X` гоняет весь X независимо от scope.

**environment — машиночитаемая метка YAML, не комментарий:**
`env: { GATE_PARITY: environment }`. Комментарий не входит в документ, который
видит парсер; метка, которую парсер не видит, значит то, что решит следующий
читатель. Парсер разбирает workflow структурно (PyYAML), комментарии на смысл не
влияют.

**Разделено то, что смешивалось.** `check-unsafe-safety`: запуск чекера над
доверенной базой — утверждение о репозитории, профиль `source`; его фикстурный
тест — утверждение о чекере, профиль `selftest`. Туда же шесть найденных сирот и
шесть self-тестов харнесса, которые раньше числились обычными гейтами
(`run-tos`, интерактивный режим, timed harness, захват событий, Stage 1 workload
и native-harness) — все они гоняют фикстуры и подложные OVMF, а не систему.

**Названное изменение охвата обычного `preflight`** (требование отдельно
называть такие вещи): бытовой прогон получает профиль `selftest` — шесть гейтов,
которых не запускал никто, и один, который был спрятан внутри другого. Больше
ничего не изменилось: класс гейтов default-scope тот же, что до миграции.
`--full` дополнительно получил ADR-0026 conformance (7m12s локально, P1;
`qemu_p95_ratio=1.180` при границе 1.30) и QEMU-self-test BootInfo.

**Инъекции — критерии 1–3 на реальном дереве, а не только на фикстуре:**

```
изъят шаг `--profile qemu`      → profile 'qemu' (33 gate(s)) is run by no workflow job
объявлен gate в новом профиле   → profile 'newprofile' (1 gate(s)) is run by no workflow job
env-шаг с меткой                → PASS
он же без метки                 → step … is neither a profile nor declared `env: GATE_PARITY: environment`
```

Плюс собственный регресс-тест паритета (`selftest`): пять отказов и два
принятия, включая отказ принять **комментарий** вместо YAML-метки.

**Удалено:** `scripts/check-generated-spec.sh` — трёхстрочный alias над
`build-specification.py --check`, вызывающих ноль; ссылка на него в
`docs/17` (иллюстрация дерева `scripts/`) заменена на `preflight.sh`, чтобы
описание оставалось правдой.

`./scripts/preflight.sh --full` — PASS, 69 гейтов.

**Паритет сработал в первом же прогоне — и нашёл три гейта, которые ломались бы
в CI, если бы CI их когда-нибудь запускал.** Все три зависели от `ripgrep`,
которого на раннере нет:

```
docs:     nucleus exception foundation           exit 127
selftest: capsule format alignment self-test     exit 127
qemu:     QEMU BootInfo identity mismatch        FAIL: loader feature is missing …
```

Третий — худший вид отказа: `rg` использовался внутри условия (`if rg -q …`),
поэтому **отсутствие инструмента прочиталось как несоблюдённое утверждение о
репозитории**, а не как отсутствие инструмента. Гейт сообщал о дефекте, которого
нет.

Исправлено не установкой ripgrep в CI, а его удалением из гейтов: три скрипта
переведены на POSIX `grep`, которому неоткуда пропасть. Установка инструмента
оставила бы необъявленную зависимость и, главное, оставила бы ловушку «нет
инструмента = ложное утверждение».

Переписанное утверждение может стать пустым, поэтому каждое проверено внесением
дефекта: изменённая константа в `exception.rs`, добавленное «misaligned» в
правило 16 `CAPSULE_FORMAT_V1`, изменённая строка feature в манифесте
загрузчика — все три **detected**.

### 2026-08-23 — ADR-0066 принят: внешний прибор, а не часы внутри системы

Прежнее чтение «Stage 3 не имеет доверенных часов, значит временные бюджеты
переносятся» отменено Project Architect. Оно смешивало две разные границы:
**какое время система предоставляет процессу** и **чем измеряют саму систему**.

ADR-0066 сохраняет production-модель ADR-0049 без расширения: monotonic tick для
preemption и bounded timeout accounting, без пересчёта в duration, wall clock и
`system.time.Clock`. Обе временные границы `docs/35` остаются Stage 3 gates и
меряются внешним observer на профиле ADR-0040. Один observer обязан мерить floor,
фиксированный TOS Core denominator и 64-byte IPC; 3 warm-up + 21 individual
sample, raw values, median и nearest-rank p99, ничего не вычитается.

Measurement-only COM1 path остаётся test-only: IOPL 0, TSS bitmap разрешает
только `0x3f8..=0x3ff`, production nucleus/runtime сравниваются по байтам.
Pairing теперь отказывает на duplicate, overlap, close без open, mismatched или
незакрытую пару, reversal и non-positive interval; это проверяется отдельным
self-test гейтом.

Текущий QEMU log trace — **P1 diagnostic, не conformance observer**. Он доказал
протокол и точную semantic boundary внутреннего вызова, но floor и call
пересекаются. Reference Debian QEMU не несёт binary `simple` backend. Поэтому
IPC timing не начинался, `8x` не объявлен и следующий шаг — отдельное versioned
решение о воспроизводимом low-overhead QEMU observer после фактической проверки,
а не смена denominator, batch average или subtraction.

Сохраняемый прогон на чистом `d4f788a4017e15d87963c7338abe3c3285e5d616`
лежит в `docs/evidence/STAGE3_MEASUREMENT_CHANNEL_P1.md` вместе с двумя raw JSON.
В нём диапазоны разделились, но floor median всё ещё равен 45,5% call median,
а call p99 имеет 111,103 µs outlier. Это не отменяет отрицательный итог:
observer остаётся comparable с denominator, прежние серии пересекались, и
выбирать удачный прогон вместо воспроизводимого прибора запрещает ADR-0066.

### 2026-08-23 — QEMU simple observer: реализация кандидата, P2 ещё не заявлен

Для выбранного в design низконакладного пути реализован независимый strict
decoder QEMU simple trace v4. Он сохраняет timestamps как integer nanoseconds,
отказывает на unknown/truncated/duplicate mapping и любой `dropped event`, а
`serial_write` принимает только с точным 16-byte payload. Text log остаётся
поддержан для сохранённого P1 diagnostic, но не маскируется под simple backend.

Добавлен `source/host-tools/qemu-test/build-simple-observer.sh`: он принимает
заранее полученный upstream QEMU 10.0.11 archive только с фиксированным SHA-256,
запрещает Meson downloads, использует vendored wheels, отключает ненужный для
x86_64 `libfdt` и выпускает self-contained bundle. Manifest хеширует launcher,
реальный QEMU engine и три реально читаемых ROM input; observer перепроверяет
эти хеши до boot. Ни QEMU, ни ROM не входят в production TOS или репозиторий.

Exploratory серии уже различают floor и fixed call по median (типично около
5–9 µs против 14–15 µs), но один прогон имел floor outlier 60,134 µs. Это не
P2 evidence и не повод фильтровать sample: сначала код и build recipe должны
быть закоммичены, затем с чистого SHA снимается повторяемая серия. IPC timing
до такой квалификации observer по-прежнему не начинается.

Чистая P1 серия с commit
`e1d2b1e6518c146d2c457fc741fbf8052dbebbe5` теперь сохранена в
`docs/evidence/STAGE3_SIMPLE_OBSERVER_P1.md` и двух raw JSON. Floor:
median 3,532 µs, p99/max 9,990 µs; exact call: median 13,760 µs, p99/max
24,732 µs, min 12,661 µs. Полные диапазоны разделены gap 2,671 µs, поэтому
simple observer локально разрешает immutable denominator. Статус остаётся P1:
следующий обязательный шаг — versioned CI gate и retained P2 artifact, не IPC
claim по локальному удачному прогону.

### 2026-08-23 — профиль observer уточнён после повторяемости, удачный run не выбран

После добавления clean gate прежний upstream simple observer не подтвердил
устойчивость: первый повтор и ещё пять независимых чистых повторов дали overlap
floor/call. Перевод timestamp всего simple backend на
`CLOCK_THREAD_CPUTIME_ID` исключил host descheduling, а ограничение trace через
QMP убрало boot events, но 4 из 10 exploratory runs всё равно пересекались.
Причина осталась в асимметрии самой границы: интервал включал transport и trace
работу после `OPEN`, но не эквивалентную работу после `CLOSE`.

Project Architect утвердил узкое Level 2 уточнение ADR-0066. Пинованный QEMU
10.0.11 теперь меняется только по exact before/after SHA-256 в
`hw/char/serial.c` и `hw/char/trace-events`: один TCG vCPU thread фиксирует
physical `CLOCK_THREAD_CPUTIME_ID` после обработки `OPEN` и до обработки
`CLOSE`, затем пишет оба raw timestamp одной simple-trace парой. UART работает
как прежде; marker transport, trace construction и host descheduling не входят
в interval, и никакая величина не вычитается. Event включается QMP только между
`READY` и последним `CLOSE`.

Floor и immutable denominator измеряются без timer preemption; будущий IPC
numerator обязан сохранить preemption active. Это консервативно уменьшает
denominator и делает `8x` строже. Build-time guard запрещает совмещать
no-preemption профиль с two-process/call-reply numerator.

Критерий resolution теперь причинно парный и задан до чтения значений: после
одинаковых трёх warm-up каждый `call[i]` обязан быть строго больше
`floor[i]`, все 21 individual tail остаются в raw series и в p99. В десяти
независимых локальных прогонах получено 210/210 resolved pairs без retry,
selection, filtering или subtraction. Девять прогонов имели также disjoint
global ranges; в одном unrelated floor tail пересёк чужой call minimum, но его
собственная call pair осталась больше. Это exploratory подтверждение кандидата,
не P2: следующий шаг — commit, clean gate и retained CI artifact.

Observer builder и внедряемый им код выделены в узкую MIT-категорию
`LICENSE.md`: GPL-3.0-or-later TOS code нельзя смешивать с GPL-2.0-only QEMU,
тогда как MIT совместим и с общим QEMU executable, и с MIT `serial.c`.
Production-лицензирование TOS и остальные host tools не изменены.

### 2026-08-23 — парность observer исправлена после независимого review

Независимый review отверг позиционное сопоставление floor и call из разных
boot: одинаковый индекс не связывает TCG phase, подготовку, cache/frequency
state или drift, поэтому прежние 210/210 остаются только diagnostic и не могут
квалифицировать observer. До commit и clean evidence метод исправлен, а не
защищён удобной интерпретацией.

Теперь один `test-measurement-call` process один раз подготавливает module,
verified receipt, argument и engine boundary. Каждый из 3 warm-up и 21 retained
blocks содержит два соседних запроса с общим 4-bit sequence и разным work bit;
порядок заранее чередуется `floor/call`, затем `call/floor`. Trace decoder и
live protocol обязаны увидеть точный план всех 48 complete tags. Duplicate
complete pair больше не может заменить пропуск. Один build manifest связывает
SHA-256 nucleus/runtime-image с exact Cargo features; `preemption=inactive`
выводится только из `test-measurement-no-preemption`, а прежний caller label
удалён.

Project Architect принял Level 2 уточнение qualification до нового clean P1/P2:
по всем 21 raw differences `call-floor` применяется one-sided exact sign test,
неположительные разности остаются в серии и считаются против observer. Порог
`p <= 0.000111` требует минимум 19/21 positive pairs (`232 / 2^21`, примерно
0,000111). Это не фильтр и не замена p99: обе raw серии, все tails и их
nearest-rank p99 сохраняются, ничего не вычитается.

Первый исправленный exploratory boot дал 20/21 positive pairs: единственные
`floor=12,535 µs` и `call=10,858 µs` сохранены как неположительная разность.
Пять следующих независимых boot дали 21/21 каждый; итого 125/126 raw pairs,
без retry selection. Floor p99 по этим пяти повторениям находился между 5,674
и 9,939 µs. Это подтверждает пригодность кандидата для fresh clean gate, но
dirty exploratory данные сами не являются P1/P2 и не разрешают IPC timing.

### 2026-08-23 — symmetric observer прошёл fresh clean P1

Observer implementation и accepted ADR-0066 amendment зафиксированы DCO commit
`626876b64d0692443a6bac3aa3ebeb15c7b7d09d`. После этого на действительно clean
tree выполнен `stage3-observer-conformance.sh --evidence-status P1`. Gate
получил 21/21 positive adjacent pairs, exact sign `p=2^-21`; floor
median/p99 `2,996/5,570 µs`, immutable denominator median/p99
`9,623/29,896 µs`, minimum paired gap `4,947 µs`. Все 42 retained intervals
остались raw, ничего не вычиталось и не фильтровалось.

Raw report и fail-closed qualification сохранены в
`docs/evidence/stage3-observer-paired-p1.json` и
`docs/evidence/stage3-observer-qualification-p1.json`; читаемый boundary report
— `docs/evidence/STAGE3_SYMMETRIC_OBSERVER_P1.md`. Это локальная квалификация
прибора и разрешение перейти к IPC numerator, а не результат IPC budgets. P2
остаётся за GitHub Actions gate и его retained artifact.

### 2026-08-23 — реальный IPC numerator реализован, exploratory tail пока красный

Добавлен не benchmark substitute, а measurement-only запуск существующего
двухпроцессного endpoint path. Неизмеряемый 64-byte priming exchange оставляет
server внутри atomic `endpoint_reply_receive`; затем каждый host request
обрамляет `OPEN/CLOSE` ровно один реальный 64-byte `endpoint_call` и его 64-byte
reply. Server снова блокируется до следующего `OPEN`; после последнего reply он
остаётся в wait до client `STOP`/exit и получает `E_CANCELLED` уже вне interval.
Nucleus собран с `test-call-reply,test-measurement-port`, timer preemption active
и manifest-bound; runtime image — с единственным `test-measurement-ipc`.

Каждый exploratory boot подтвердил `24/24` measured answers плюс один prime,
`50` messages, `75` payload copies (не более двух на message), `25` exchanges и
balanced `51/51` IPC operation crossings. После исправления последнего reply,
который первоначально оставлял server runnable и дал ложный 98-ms shutdown tail
внутри последней пары, получены семь независимых серий: шесть с p99 от `53,303`
до `106,690 µs`, одна с p99 `213,193 µs`. Последняя честно нарушает абсолютные
`200 µs`; она не отфильтрована и не заменена удачным прогоном. Текущий код
готовит fail-closed combined gate, но clean P1 после commit должен выполняться
ровно один раз: его результат, зелёный или красный, и будет retained evidence.

### 2026-08-23 — clean P1 IPC latency прошёл оба бюджета

После commit `d759ad49e44a76791fb780b0a1a35e6ba86d32ac` combined gate был
выполнен ровно один раз на чистом tree. Observer снова квалифицирован: `21/21`
положительных adjacent pairs, sign `p=4.76837158203125e-07`, denominator p99
`23.513 µs`. Реальный 64-byte request/reply с active preemption дал median
`59.965 µs`, p99 `100.761 µs`, то есть `4.285331518734317x` denominator при
лимите `8x` и ниже абсолютных `200 µs`. Guest/nucleus подтвердили один prime,
24 warm-up/retained exchanges, `50` messages, `75` copies, `25` exchanges и
`51/51` IPC crossings. Ничего не вычиталось и не фильтровалось.

Raw denominator, numerator и оба fail-closed qualification records сохранены в
`docs/evidence/STAGE3_IPC_LATENCY_P1.md`. Это P1, не P2 и не закрытие Stage 3:
дальше нужны CI evidence на pushed SHA, restart identity, E3 adversarial suite и
versioned Stage 3 identity/trusted-base report.

Первый QEMU workflow на evidence commit остановился до toolchain и измерений:
`build-simple-observer.sh` сохранил относительный output path, вошёл в свой
build-directory и потому искал `source/configure` относительно нового cwd.
Output теперь канонизируется через `realpath -m` до создания временного дерева.
Точная workflow-форма с относительным `source/target/...` локально построила
observer до конца с прежним launcher SHA-256 `39474e...`; observer patch,
платформа и performance contracts не изменялись. Нужен новый CI run на commit с
этим исправлением; красный run `32644604992` остаётся историческим результатом.

### 2026-08-24 — оба Stage 3 бюджета латентности получили P2

Исправленный workflow прошёл целиком на pushed commit
`78447b31c62cd24a1549f5c2ac7833cdd6fe153b`: run `32644830444`, job
`97207387908`, `qemu` profile, 15 мин 17 с, зелёный. Оба Stage 3 gate внутри
одной job выдали `evidence_status: P2` — статус, который сам скрипт
отказывается печатать вне GitHub Actions.

Observer квалифицирован дважды независимо, каждый раз из своего boot: 21/21
положительных adjacent pairs, sign `p=4.76837158203125e-07`. Floor median/p99
`1,834/2,264 µs` и `1,944/2,274 µs`; denominator `6,492/7,484 µs` и
`6,342/7,254 µs`. Реальный 64-байтный request/reply с active preemption дал
median `38,071 µs`, p99 `51,546 µs` — `7,105872622001654x` от denominator p99
при лимите `8x` (`58,032 µs`) и вдвое ниже абсолютных `200 µs`. Guest и nucleus
снова подтвердили один prime, 24 measured exchange, 25 served, `50` messages,
`75` copies, `25` exchanges, `51/51` crossings. Ничего не вычиталось и не
фильтровалось.

Два факта из сравнения с локальным P1 записаны прямо, а не подразумеваются.
Первый: все четыре измеренных артефакта (nucleus и runtime image для
denominator и numerator) побайтно совпали с локальными при других хосте,
компиляторе и toolchain — различие чисел целиком средовое. Launcher observer'а
тот же `39474e…`, engine другой (`2189eb13…` против `90f836fd…`), потому что
он компилируется на месте из того же pinned архива; digests обоих patched
файлов, `configure`, `SOURCE_DATE_EPOCH` и wheels совпадают.

Второй: относительный бюджет в CI строже, а не мягче. Локально было
`4,285x`, в CI `7,106x` — при том что абсолютная латентность в CI лучше
(`51,546` против `100,761 µs`). Тихий runner сжал denominator (`7,254` против
`23,513 µs`) сильнее, чем numerator. То есть шумный хост раздувает denominator
и льстит отношению; настоящий запас — `11,2%`, и наблюдать нужно именно
CI-число. По ADR-0066 §6 красный результат на нём решается работой над IPC
path, а не сменой инструмента, знаменателя или арифметики.

Записи сохранены как `docs/evidence/STAGE3_SYMMETRIC_OBSERVER_P2.md` и
`docs/evidence/STAGE3_IPC_LATENCY_P2.md` с шестью licensed JSON. Raw серии
observer'а связаны с вердиктом по digest самим gate (`measurement_report_sha256`
совпадает с retained-файлом); IPC qualification свои два raw report называет
только путём — это записанный пробел gate, а не заявленное свойство.

Закрыта количественная половина Stage 3 IPC performance contract. Stage 3 не
закрыт: остаются restart identity/audit, оставшееся E3 adversarial coverage и
versioned Stage 3 identity/trusted-base report.

### 2026-08-24 — IPC-вердикт связывает свои входы по digest

Пробел, названный в P2-записи, закрыт в самом gate. `qualify-ipc.py` получал
четыре входа и сохранял их только путями: пока существует каталог прогона,
такую запись можно проверить, после — нет. Теперь каждый прочитанный вход
хешируется, и `reports_sha256` едет внутри retained record, поэтому retained
qualification и retained raw series доказуемо одни и те же байты.

Digest записывается по мере чтения, а не одним блоком в конце: если запись
признана невалидной на четвёртом входе, три прочитанных всё равно названы
своими байтами, а непрочитанный не получает выдуманного digest. Прежняя
проверка `measurement_report_sha256` observer-квалификации против denominator не
изменилась. Два новых теста в `test-qualify-ipc.py` требуют полного набора
digest на зелёном прогоне и ровно трёх — когда serial log отсутствует; 11/11
проходят.

Retained P2-запись не переписана: она была снята прежней версией gate и говорит
об этом прямо.

### 2026-08-24 — тот же артефакт, следующий прогон, красный: `8x` не закрыт

Через один прогон после зелёного P2 gate стал красным на commit
`2a7ca2033ce7e1d55f50c03cf6ce1ad6b1096dcf` (run `32734827424`) — на коммите,
который ничего, кроме документов и retained evidence, не менял. Denominator p99
`5,519 µs`, numerator p99 `44,406 µs`, отношение `8,046022830222865x` при
лимите `8x`: промах на `0,254 µs`. Абсолютные `200 µs` выполнены с запасом
вчетверо и не спасают — §5 требует, чтобы один и тот же неповторённый p99
удовлетворял обеим границам.

Все четыре измеренных артефакта побайтно те же, что в зелёном прогоне и в
локальном P1 (`db2d100f…`, `9de69c16…`, `3cd72f37…`, `d8401ad6…`), observer тот
же, платформа та же. Значит, различие — только прогон:

| Run | Commit | Denominator p99 | Numerator p99 | Ratio | Verdict |
|---|---|---:|---:|---:|---|
| `32644830444` | `78447b3` | 7,254 µs | 51,546 µs | 7,106x | зелёный |
| `32734827424` | `2a7ca20` | 5,519 µs | 44,406 µs | 8,046x | красный |

**Вывод, который приходится сделать против собственной вчерашней записи:** один
зелёный прогон относительный бюджет не закрыл. Распределение отношения лежит
верхом на `8x`, и сторона решается раннером, а не системой. Формулировка
«закрыта количественная половина Stage 3 IPC performance contract» из записи
выше и из P2-документа исправлена, измеренные числа — нет.

Где именно промах, видно в сырых данных: 20 из 21 сэмплов numerator лежат между
`29,083` и `32,488 µs`, а двадцать первый — `44,406 µs`, и он и есть p99 по
nearest rank. Против того же denominator второй по величине сэмпл дал бы
`5,887x`; медианное отношение — `6,43x`. То есть весь дефицит — один
preemption tail. Убирать его нельзя: §5 держит его в серии намеренно, numerator
идёт с активным preemption, а denominator по §3 — без него и хвостов иметь не
может. На 99-м процентиле сравниваются распределение с таймерными хвостами и
распределение, у которого их нет по построению.

Красный прогон сохранён как `docs/evidence/STAGE3_IPC_LATENCY_P2_RED.md` с
четырьмя licensed JSON. Gate остаётся fail-closed, `qemu` profile на main
красный, и по §6 это не разрешает трогать ни часы, ни workload, ни denominator,
ни бюджет внутри этого результата. Дальше — либо работа над самим IPC path
(хвост, а не средний путь), либо отдельное решение Project Architect по
контракту; второе — уровень 2 и в этом журнале не принимается.

### 2026-08-24 — хвост измерен: это одно timer interrupt, а не путь IPC

Диагностика в одноразовом worktree, репозиторное дерево не трогалось. Прибор —
тот же pinned observer (launcher `39474e…`, engine `90f836fd…`), собранный
штатным `build-simple-observer.sh` из архива `22e410fe…`. Conformance-защиты
сняты только в worktree; патч сохранён отдельным артефактом с digest
`77e8918e…`. Ни одна серия не может стать evidence: `qualify-observer`
отвергает active-preemption denominator, `qualify-ipc` — inactive-preemption
numerator.

Счётчик тиков читается до `OPEN` и после `CLOSE`, между маркерами не
добавлено ничего. Результаты по четырём чередующимся матрицам:

- при штатном quantum тик попадает в 2,8 % и 4,4 % интервалов, и максимум
  прогона оказывается тикнутым в 6/12 и 10/15 случаев; при выключенном таймере
  — 0/315; при quantum ÷10 — 46,4 % и **12/12**. Частота геометрическая
  (`интервал / период`), выведенный период тика — `1,17`/`1,84` мс против
  `0,11` мс на ÷10;
- это не планировщик: в однопроцессной нагрузке, где `preempt()` физически не
  может переключить контекст, тикнутый интервал стоит `+56,89 µs` против
  `+43,60 µs` при двух процессах. Скан таблицы, копии кадра и перезагрузка CR3
  из стоимости исключены;
- около микросекунды из хвоста — обработчик с записью EOI (цена записи в
  устройство берётся из retained CI floor: `0,69–0,97 µs`). Остальное —
  доставка прерывания самим QEMU, и это вычтено, а не измерено: на машине нет
  `perf`, `perf_event_paranoid=3`;
- выключение preemption IPC не ускоряет: `median(D−C) = +7,23 µs`, 9/12. Почему
  — не объяснено ничем измеренным, и это записано как открытый вопрос, потому
  что от него зависит любой вариант с no-preemption numerator.

Оба прогона CI объясняются этим количественно: один сэмпл в стороне на
`+13,475` и `+13,961 µs`, остальные двадцать плотные, артефакты побайтно те же.

Запись — `docs/evidence/STAGE3_PREEMPTION_TAIL_P1.md` плюс raw JSON, из
которого каждое число пересчитывается. Там же зафиксирован дефект собственной
метаданной: harness в matrix 4 сообщал `quantum_count=10000` для обеих сборок,
потому что патч заставил его брать минимум из двух констант; поле оставлено как
есть, эффективное значение выведено из features, а различие сборок доказывают
сами измерения. `check-spdx.sh` расширен на `.patch` — гейт сам это потребовал.

Green и red P2 не изменялись и не заменялись.

### 2026-08-25 — один бинарник, три режима: замедление не сборочное

Последний диагностический эксперимент по предусловию ADR-0068. Один nucleus
(`0f4811c1…`) и один runtime image (`3c670daf…`) грузятся в трёх режимах,
выбираемых на boot из капсульного юнита `/system/diag/timer-mode` — до
появления процессов и задолго до `READY`. Слова режимов выровнены по длине,
поэтому три капсулы различаются содержимым, но не размером (3256 байт каждая),
и layout капсулы как объяснение исключён. Измеряемый путь во всех трёх режимах —
одни и те же машинные инструкции.

Режимы: ACTIVE (`apic::start()`), MASKED (`apic::start_masked()` — все те же
записи плюс бит маски LVT, APIC включён, счётчик идёт, `IF` установлен,
прерывания не доставляются), NO-TIMER (таймер не запускается). Пятнадцать
чередующихся итераций каждого. Контрактные счётчики во всех режимах одинаковы:
`50/75/25/51/51`, клиент `24/24/0`. Тиков: 14/315 в ACTIVE, **0/315** в MASKED
и NO-TIMER.

Парно, против бестиковых сэмплов ACTIVE:

- `MASKED − ACTIVE(clean)` = **`+18,45 µs`**, 14/15;
- `NO-TIMER − MASKED` = `−7,43 µs`, 5/15 — неразличимо;
- `NO-TIMER − ACTIVE(clean)` = **`+16,62 µs`**, 12/15.

Ответ на поставленный вопрос: **да, систематическое замедление inactive IPC
сохраняется при побайтно одинаковых артефактах.** Причина — сама доставка
прерываний, а не программирование APIC: таймер, который считает и
перезагружается, но ничего не доставляет, ведёт себя как отсутствующий. То есть
периодические прерывания ускоряют даже те интервалы, в которые не попадают,
примерно на ту же величину, которую добавляют, когда попадают. Почему — ничем
измеренным не объяснено; это свойство исполнения под этим эмулятором, а не
разница в выполняемой работе.

Для ADR-0068 это довод **против** основного кандидата, и он записан в ADR как
таковой: no-preemption numerator убрал бы хвост `~14 µs` и добавил артефакт
`~17 µs`, причём denominator такого эффекта на своей нагрузке не показывал —
стороны не раздувались бы вместе. Величина эффекта на reference runner
неизвестна: всё измерено на одной машине. Предусловие поэтому не снято, а
уточнено: эффект доставки нужно охарактеризовать на reference platform, и не
диагностическим патчем.

ADR-0068 остаётся Proposed. `docs/35`, `IPC_V1` и гейт не менялись. Сырые серии
— matrix 5 в `stage3-preemption-tail-diagnostic-p1.json`, вместе с hash'ами
единого артефакта, трёх капсул и режимом, о котором отчитался сам гость.
Диагностический патч перевыпущен (`f8a67a4f…`) и по-прежнему хранится отдельно
как явно диагностический.

### 2026-08-25 — ADR-0068 переработан в решение; исправлен неполный retained патч

ADR-0068 переписан из «под каким профилем мерить» в решение «какой из двух
бюджетов латентности является conformance». Предложено: относительный `8x`
уходит из pass/fail Stage 3 без замены другим коэффициентом и без подгонки
denominator; причина — ни один из трёх профилей на ADR-0040 TCG не даёт
отношения, которое честно читается как intrinsic IPC overhead (асимметричный
решается попаданием тика, active/active вознаграждает шум в компараторе,
inactive/inactive закрыт matrix 5). Fixed-call benchmark и само отношение
остаются observational/regression и ничего не закрывают и не блокируют.
Абсолютный `p99 <= 200 µs` остаётся conformance для настоящего 64-байтного IPC
с активным preemption и с хвостами. Серия — 300 удержанных сэмплов после 3
warm-up; observer qualification с её 21 парой и sign-test не трогается. Quantum
и APIC divider — qualifier-bound идентичность профиля. Счётные бюджеты `IPC_V1`
§8 без изменений. Green и red P2 остаются историческими записями прежней
структуры, красный остаётся красным.

**Отдельно — дефект в моей же retained evidence, найденный при проверке
воспроизводимости и исправленный, а не замолчанный.** Коммит `c917ea6` сохранил
неполную копию диагностического патча: экспорт выполнялся из каталога worktree
по относительному пути, поэтому обновилась копия внутри worktree, а файл в
репозитории остался прежним — при том что документ и JSON уже называли новый
digest. Значит, matrix 5 по закоммиченному артефакту не воспроизводилась. Сейчас
сохранён полный патч (`f8a67a4f…`, содержит `apic::start_masked` и выбор режима
на boot), и он проверен на чистом checkout коммита
`6f2837b76275c4ea5ab8f4d0491294d74543c687`: применяется без конфликтов, 6 файлов,
198 вставок. Ни одно измерение не изменилось — неверен был артефакт для их
повторения. Проверка «патч применяется к записанному коммиту» теперь сделана
явно, а не подразумевается.

`docs/35`, `IPC_V1` и гейт по-прежнему не менялись — ADR ждёт утверждения.

### 2026-08-25 — ADR-0068 принят и реализован одним изменением

Перед принятием исправлена математика §5: наличие хотя бы одного тикнутого
сэмпла ничего не говорит о ранге 297. Ранг 297 из 300 — четвёртый сверху,
поэтому p99 оказывается хвостовым значением при `N >= 4`, а не при `N >= 1`.
При `p=3 %` это `98,01 %`, во всём измеренном диапазоне `2,8–4,4 %` —
`96,94…99,93 %` против `47,8 %` на серии из 21. Размер серии не менялся.

Реализация внесена атомарно: `docs/35`, `IPC_V1` §8, ADR-0066 §3/§5 и гейт в
одном коммите, чтобы ни один документ не описывал старую композицию, пока
другой описывает новую.

- относительный `8x` убран из pass/fail; `qualify-ipc.py` считает отношение и
  кладёт его в `observational` с явным `is_a_budget: false`;
- `p99` считается по nearest rank, а не как максимум: при 300 это 297-й
  порядковый статистик. Прежняя реализация брала `max`, что при 21 совпадало,
  а при 300 давало бы ранг 300 под именем p99;
- серия — 300 сэмплов после 3 warm-up; счётчики нагрузки выведены формулами из
  длины серии, а не переписаны константами;
- quantum и APIC divider связаны квалификатором: запись, снятая на другом
  quantum или другом делителе, отвергается. Divider читается из имени
  константы, потому что кодировка регистра `0b0011` сама по себе несравнима;
- в ADR-0066 §3 оставлено прежнее утверждение о «консервативности» с пометкой,
  почему оно было неверным, — запись того, что ADR утверждал, не переписана.

Два ограничения, вскрытые первым же настоящим прогоном на 300, и оба —
настоящие, а не в обвязке. Первое: `--samples` было ограничено 31, потому что
прежняя схема допускала один тег на сэмпл без wrap; ограничение снято, а
сторожем стал предварительно объявленный точный план тегов. Второе: измеряемый
сервер был запрограммирован ровно на 25 обменов — размер под серию 3+21. На 26-м
обмене клиент вызвал `endpoint_call`, отвечать стало некому, и правило живости
завершило загрузку. **Гейт при этом отработал fail-closed:** прогон упал, а не
отчитался короткой серией. Бound сервера теперь выводится как
`1 + warmups + samples`.

Проверено тестами: план тегов на 303 блоках с восемнадцатью wrap, дубликат
через wrap на чужой позиции, перестановка двух блоков — все отвергаются;
серия неверной длины, чужой quantum и чужой divider отвергаются; отношение
`10x` больше не делает прогон красным. Observer qualification с её 21 парой и
sign-test не менялась. 36/36 preflight, 62 self-теста измерительной обвязки.

Настоящий локальный прогон на 300: `303` обмена, `608` сообщений, `912` копий,
`609/609` пересечений — ровно то, что предсказывают формулы.

### 2026-08-25 — новый P2: один бюджет, 300 сэмплов, зелёный

CI run `32754751228` на pushed commit `615dcacbbd145e48f778353f0e64a858aee7fc3c`
прошёл целиком. `p99 = 39,459 µs` при лимите `200 µs`, медиана `21,362 µs`,
максимум `58,338 µs`. p99 — это ранг 297 из 300, и над ним стоят ровно три
сэмпла: `40,781`, `55,203`, `58,338 µs`.

Предсказание ADR-0068 проверилось на этих данных: десять сэмплов из 300 лежат
как минимум на `10 µs` выше медианы — доля `3,33 %`, ровно тот диапазон
`2,8–4,4 %`, что измерен в диагностике. При такой доле `N >= 4` выполняется с
вероятностью `98 %`, здесь было десять, и p99 оказался хвостовым значением, как
и задумано. На серии из 21 то же распределение давало хвост в отчёте меньше чем
в половине прогонов.

Счётчики нагрузки масштабировались как выведено: `303` измеренных обмена плюс
prime, `304` обмена, `608` сообщений, `912` копий, `609/609` пересечений.
Quantum `100000` и divider `16` связаны квалификатором. Observer
переквалифицирован на своих неизменных 21 паре и дал `20/21` — при пороге 19 это
годно, и записано как есть, а не округлено вверх.

Отношение сохранено как `7,176973444889051` внутри блока `observational` с
`is_a_budget: false`. Стоит сказать прямо: под прежней структурой `7,18x` — это
снова near-miss того же рода, что дал зелёный `7,11x` и красный `8,05x` на
побайтно одинаковых артефактах. Выживший бюджет тем и отличается, что не зависит
от того, по какую сторону миллисекунды упало прерывание.

Четыре retained JSON связаны с вердиктом по digest самим гейтом через
`reports_sha256` — тот пробел, который был закрыт после первой P2-записи.
Локально на грязном хосте тот же гейт дал `p99 = 102,637 µs` — тоже внутри
бюджета. Запись — `docs/evidence/STAGE3_IPC_LATENCY_P2_300.md`.

Прежние green и red P2 не тронуты и помечены как записи прежней структуры.
На этом performance work останавливается; открытым остаётся ADR-0067.

### 2026-08-25 — ADR-0067 реализован: операции 14 и 15, tombstone в слоте

Ядро получило то, чего в нём не было и что ADR прямо назвал недостающим:
boot-уникальный instance id (не индекс слота и не handle), родительскую связь и
хранимое утверждение супервизора о restart generation с флагом присутствия.
При завершении процесса запись остаётся в его же слоте, слот с несобранной
записью не переиспользуется, `Waiting::ChildOf` даёт ожидание на отношении, а не
на объекте, и запись выдаётся самой ранней по boot-монотонному порядку
завершения.

Операция 8 не тронута: её дети получают id и родителя, но generation у них
отсутствует, а не равен нулю. Операция 15 несёт `r8` и кладёт instance id в
`CREATE_INSTANCE_ID`, потому что `rdx` уже занят capability handle. Операция 14
пишет `WaitChildRecord` в argument region вызывающего по фиксированному
смещению.

Гейт `lifecycle.sh` (профиль qemu) проверяет на одной загрузке: два ребёнка,
завершившиеся до единственного ожидания — обе записи целы и в порядке
завершения, третье ожидание даёт `E_WOULD_BLOCK`; instance id из create равен
id в записи и не равен handle; generation 7 и 9 повторены дословно; устаревший
handle после сбора даёт `E_NO_CAPABILITY`; capability без `wait_child` —
`E_NO_CAPABILITY`; ребёнок операции 8, закончившийся сам, даёт kind=exited,
self-reported status присутствует, generation отсутствует; неудовлетворимое
ожидание отменяется как `E_CANCELLED`.

Два факта, найденные реализацией и записанные, а не обойдённые.

Первый: `context_yield` (операция 10) возвращает `OK` и не перепланирует, а
завершённый процесс убирается только циклом планировщика — то есть когда
текущий контекст блокируется или заканчивается. Поэтому супервизор, ожидающий
блокирующе, ретайрит своих детей сам, а ребёнок, заканчивающийся сам, доходит
до цикла без чужой помощи. Нагрузка теста построена на этом, а не на
предположении.

Второй: тест 2 из ADR (исчерпание слотов) на этой платформе недостижим. Грант
берёт наибольший непрерывный участок, участки дробятся, и четвёртый ребёнок —
последний, который влезает: `E_LIMIT` приходит от памяти при 33 тысячах
свободных фреймов, а не от таблицы процессов. Гейт этого не изображает. Вместо
этого ядро теперь называет причину отказа в логе — `reason=no-slot uncollected=N`
против `reason=no-grant available=N` — потому что статус ABI один, а чинятся эти
две причины по-разному. Тесты 9/9a/9b (смерть супервизора и делегированный
ожидающий) в этой загрузке тоже не выполнены: им нужна вторая загрузка с
делегированием права, и это следующий шаг, а не заявленный результат.

`check-abi-operations.sh` расширен: он читал только приватные `const`, и
операция, экспортированная модулям ядра, была бы для него невидимой — гейт,
обходимый ключевым словом, не гейт.

36/36 preflight, паритет с CI — 73 гейта в 5 профилях.

### 2026-08-25 — оставшиеся тесты ADR-0067: делегированный наблюдатель и сирота

Гейт `lifecycle.sh` получил вторую загрузку — та же архитектура, но три живых
процесса: супервизор, бездетный средний родитель и наблюдатель, которому право
`wait_child` делегировано **над чужим** process object. Роль каждый узнаёт по
имени привязки своей capability (ADR-0061), а не по индексу слота: роль,
прочитанная из привязки, кем-то выдана, роль, прочитанная из индекса, — догадка.

Наблюдатель блокируется на отношении, которого не создавал, родителя завершает
супервизор, и дальше всё делает ядро: ожидание отвечает `E_CANCELLED` (§9a), а
запись о завершении наблюдателя, которую супервизор не собрал, освобождается при
его собственном конце (§10). Обе вещи теперь в логе как утверждения ядра:
`TOS.RUN.NOTICE_RELEASED child=… parent=… reason=parent-ended`.

По дороге выяснилось и записано: если родитель имеет собственного ребёнка,
делегированный наблюдатель законно **получает** его запись — делегирование
работает, но тогда это тест на делегирование, а не на отмену. Поэтому родитель
в сценарии бездетный: множество, на котором ждут, должно быть пустым, иначе
проверяется не то.

Ещё одно наблюдение, стоившее отладки: `TOS.RUN.NOTICE_RELEASED` пропало из
ядра при одной из промежуточных правок и было восстановлено — молчаливое
освобождение записи выглядит в логе ровно как её отсутствие, и отличить их
нельзя. Строка есть, и гейт требует, чтобы она была именно nucleus-asserted и
называла причину.

Из десяти conformance-тестов ADR закрыты девять. Не закрыт второй — исчерпание
слотов: на этой платформе грант упирается раньше таблицы процессов, что
зафиксировано записью выше и различается в логе (`reason=no-slot` против
`reason=no-grant`). Гейт этого не изображает.

### 2026-08-25 — ADR-0067 дочищен: право оказалось consumptive, и доставка это не учитывала

Три правки по указанию Project Architect, и одна из них вскрыла настоящий
дефект.

`retire()` больше не держит `&mut` на слот через вызовы, которые сами именуют
таблицу: идентичность читается коротким заимствованием, `ending_order` и
`ended_tick` пишутся своим, а `reclaim` и адресное пространство **изымаются**
из слота перед возвратом памяти, а не читаются из него по ссылке. Тест 6a в ADR
переписан: ребёнок операции 8 сообщает generation как **отсутствующую**, а не
как ноль — ровно то, что уже делает реализация.

Главное: добавлен тест на делегированного сборщика, который **уже заблокирован**
в `process_wait_child`, когда у наблюдаемого живого родителя заканчивается
ребёнок. На прежней реализации он падает — я это проверил, отложив исправление:
строки `WATCHER status=` не появляется вовсе, сборщик остаётся заблокированным
рядом с записью, на которую имеет право. Доставка смотрела только на самого
родителя. Теперь она ищет любого заблокированного на этом отношении: сам факт
блокировки и есть авторизация, потому что capability проверялась в момент
вызова.

Из этого следует свойство, которое надо было назвать вслух, и оно записано в
§2, §6 и threat model: **`RIGHT_WAIT_CHILD` — это consumptive collection
authority, а не broadcast observation.** Tombstone собирается ровно один раз, и
делегирование права отдаёт возможность собирать события **вместо**
непосредственного родителя — то есть супервизор, делегировавший его и
продолжающий полагаться на эти события, отдал то, от чего зависит. При
конкуренции нескольких авторизованных сборщиков выбор детерминирован правилом,
а не порядком прибытия: наименьший process instance id среди уже
заблокированных. Порядок прибытия система не публикует, а порядок таблицы —
свойство реализации.

Гейт `lifecycle.sh` теперь делает три загрузки; третья проверяет и сам факт
доставки, и то, что сборщик был заблокирован **до** появления ребёнка — иначе
это был бы опрос, а не доставка.

### 2026-08-25 — `context_yield` наконец уступает квант

Независимый дефект Stage 3, найденный при реализации ADR-0067 и исправленный
отдельно. `SYSTEM_ABI_V1` §5 говорит про операцию 10 «gives up the rest of the
quantum», а реализация отвечала `Answer::status(OK)` и возвращалась — то есть не
уступала ничего: вызывающий сохранял процессор, и процесс, уступивший ход, чтобы
дать поработать соседу, соседу поработать не давал.

Теперь вызов кладётся так же, как блокирующий, с двумя отличиями: ответ
пишется сразу, потому что разбудить этот контекст с ответом будет некому, и
состояние остаётся `Runnable`, потому что уступка хода — не ожидание.

Заодно исправлено то, что делало уступку бессмысленной даже с правильной
семантикой: цикл планировщика выбирал `first_runnable()`, то есть всегда
наименьший индекс, и вернул бы процессор тому же процессу. Пока в цикл попадали
только блокировка и завершение, это было незаметно — тот, кого возобновляли,
только что перестал быть runnable. Теперь поиск идёт round-robin от текущего
слота, как это давно делает `preempt`, и как ADR-0049 §4 и говорит.

`resumes_an_operation` для уступки **не** ставится, и это не мелочь: флаг
существует, чтобы планировщик и таймер считали выход *IPC*-операции дверью,
отличной от edge (ADR-0063). Первая версия его ставила, и гейт `exchange-cost`
немедленно поймал это — «4 IPC-операции вошли и 36 вышли». Уступка кванта не
IPC-операция, и её пересечения в числе, которым `IPC_V1` §8 ограничивает обмен,
делать нечего.

Гейты `scheduler`, `blocking`, `request-reply`, `exchange-cost`, `supervisor` и
`lifecycle` зелёные; тестовая нагрузка ADR-0067 теперь пользуется настоящей
уступкой вместо ожидания таймера.

### 2026-08-25 — грант собирается из фреймов, и четыре слота наконец используемы

**Сначала доказательство, потом код.** Ни один принятый контракт не требует,
чтобы арена процесса была физически непрерывной. ADR-0041 §1 обещает «one
bounded region» с base/length/alignment, но процесс получает `grant_base =
GRANT`, то есть **виртуальный** адрес `0x3000_0000`; ADR-0050 говорит про
per-process гранты из пула и ничего не говорит про непрерывность. А `tos-frames`
прямо описывает нужную модель в собственной документации: `allocate_frame` —
«то, из чего строятся адресное пространство, таблица страниц **или
per-process grant**», а `carve` — «для немногих структур, которые обязаны быть
непрерывными, потому что их ещё никто не отображает», и это про Stage 2 до
пейджинга. Реализация же строила гранты Stage 3 через `carve`.

Отсюда цена: каждый carve забирал наибольший непрерывный участок и оставлял
следующему меньший — 15707, 8512, 4244, 2708 фреймов, — так что четвёртый
процесс не запускался при десятках тысяч свободных фреймов. Теперь грант
отображается `map_fresh` на тот же виртуальный `GRANT` и освобождается
`release_mapped` из таблиц страниц, как остальные отображения процесса.
`Reclaim` хранит длину, а не физический `Span`. Что процесс знает о своём
гранте, не изменилось ни на бит.

**Политика размера выведена из Stage 2 evidence, а не подобрана.**
`STAGE2_ARENA_BOUND.md` измерил высшую точку арены референсного пути: **52,01
МиБ** в худшем случае (set-wide resolution по производным сводкам, 256 модулей
предельного размера), причём `peak_extent` по построению завышает. `MAX_GRANT`
= 96 МиБ в своём же комментарии назван «cap, not a target», `MIN_GRANT` = 8 МиБ
— пол, ниже которого путь не запустится. Между ними политики не было, и размер
фактически был производной от «сколько осталось» — то есть прогон мог удаться
первым и провалиться четвёртым, сообщая факт о планировании как факт о
программе.

Введён `RUNTIME_GRANT = 54 МиБ`: измеренная граница, округлённая вверх. Это же
делает объявленные четыре слота используемыми — на референсной платформе 256
МиБ, до пула доходит около 230, и четыре процесса такого размера помещаются со
своими стеками, записями и таблицами страниц. При `MAX_GRANT` не стартовал бы и
второй.

**Результат:** каждый процесс освобождает одинаковые 14356 фреймов вместо
уполовинивающейся последовательности, и тест 2 из ADR-0067 впервые достигнут по
-настоящему: `filled=3 full=-6 collected=0 after_one=0 full_again=-6` при
`TOS.RUN.PROCESS_REFUSED reason=no-slot uncollected=3`. Один сбор освобождает
ровно один слот. ADR-0067 закрыт 10/10 end-to-end.

Локальный qemu-профиль: 32 гейта прошли, два (ADR-0066 observer и Stage 3 IPC)
не запускались — pinned observer удалён при чистке диска и собирается в CI.

**Требует ратификации:** `RUNTIME_GRANT` — свойство reference platform, и хотя
число выведено из принятой evidence и из семантики уже существующих констант,
зафиксировать per-process размер на ADR-0040 платформе стоит решением уровня 2,
а не комментарием в коде.

### 2026-08-25 — что арена стоит на самом деле, и чего реализация обещать не может

Исправлен комментарий к `RUNTIME_GRANT`: `52,01 МиБ` из `STAGE2_ARENA_BOUND` —
это `committed` из `resolution_over_summaries`, живое состояние в момент, а не
`peak_extent` полного пайплайна. Грант обязан покрывать **frontier**:
аллокатор не может выдать адрес выше выданной области, сколько бы живых байт в
ней ни было в этот миг. Правильный однопроцессный ориентир — `50,33 МиБ` из
`one_module_at_the_ceiling`, и 54 МиБ есть именно он с округлением. Константа
помечена как provisional candidate.

**Прямое измерение обещанного предела.** Гарнитура арены получила режим
`--ceiling`: полный production `execute_set` над замыканием модулей предельного
размера, с проверкой ответа и — главное — с исключением корпуса исходников. В
TOS юниты это байты капсулы вне гранта, поэтому корпус строится первым, frontier
снимается до запуска, и отчитывается то, что прогон потребовал **сверх** него.

| модулей | сверх корпуса |
|---:|---:|
| 2 | 109,40 МиБ |
| 4 | 160,07 МиБ |
| 8 | 261,25 МиБ |
| 16 | 460,00 МиБ |

Наклон линейный: **25,03 МиБ на модуль** при базе **59,94 МиБ**.

**Реализация обещает полный предел docs/44 и выполнить его не может.** Меньшего
declared conformance profile нет: `Limits::default()` — это принятый V1 ceiling
с `modules: 256`, а `MAX_SOURCE_BYTES` — 256 КиБ. По измеренному наклону это
`≈ 6,3 ГиБ` арены на процесс, примерно в двадцать пять раз больше всей
референсной платформы. Разрыв не между грантом и обещанием, а между обещанием и
машиной.

Меньший cap я **не выбирал**: подбирать предел так, чтобы он поместился в грант,
— это выбирать conformance profile по счёту за память. ADR-0069 (Proposed)
фиксирует механизм (виртуально непрерывный грант из обычных фреймов), правило
(размер — фиксированное свойство профиля, не функция остатка), статус 54 МиБ как
кандидата и потолок `MAX_GRANT` как потолка, — и отдельным разделом называет
этот разрыв как требующий решения.

Что помещается и что упирается: пул после ядра — 58 839 фреймов (~229,8 МиБ),
процесс стоит 14 356 (56,08 МиБ, из них 2,08 — стек, отчёт, аргументы, launch
record и таблицы страниц), четверо — 224,3 МиБ с запасом ~5,5 МиБ. Первым
упирается память через таблицу процессов: выше ~55,37 МиБ гранта четвёртый
процесс не стартует, и отказ меняется с `reason=no-slot` на `reason=no-grant`.
Размер таблицы и размер гранта — одно решение с двумя именами.

Evidence: `docs/evidence/STAGE3_PROCESS_GRANT.md`.

### 2026-08-25 — наклон был удержанием реализации, а не ценой контракта

Project Architect указал на то, чего я не проверил: `STAGE2_ARENA_BOUND.md` уже
фиксирует принятую memory architecture — резолюция по сводкам, фазовая обработка
по одному модулю, `summarize()` возвращает owned значение именно чтобы дерево
разбора можно было сразу выбросить, а `check_module_summaries()` существует
именно чтобы резолюция дерева не требовала. `execute_set` этого не делал.

**Диагностика по границам фаз** (тот же ceiling-fixture, 8 модулей по 256 КиБ,
корпус исходников исключён):

| удерживаемый объект | на модуль |
|---|---:|
| `SourceUnit` | 0,22 МиБ |
| **дерево разбора** | **13,99 МиБ** |
| сводка | 0,19 МиБ |
| **lowered IR** | **15,13 МиБ** |

Цифра деревьев совпала с уже опубликованной в Stage 2 (14 040 464 B), сводка
меньше в 74 раза.

**Восстановлена фазовая дисциплина.** Разбор, проверка и сводка — по одному
модулю, дерево выбрасывается в конце своего хода; резолюция — только по
сводкам; дерево строится заново для понижения того модуля, которому оно
принадлежит. Второй проход — это повторный запуск того же детерминированного
парсера по тем же нормализованным байтам, а не более дешёвая замена: ни одна
граница frontend или verifier не обойдена, скрытого unchecked пути нет. Трасса
стадий исправлена, чтобы не называть `Check` до первого разбора.

| модулей | удерживающий путь | фазовый путь |
|---:|---:|---:|
| 2 | 109,40 МиБ | 92,83 МиБ |
| 4 | 160,07 МиБ | 118,16 МиБ |
| 8 | 261,25 МиБ | 168,76 МиБ |
| 16 | 460,00 МиБ | 268,15 МиБ |

`25,03 МиБ` на модуль стали **`12,52`** при базе `68,09`.

**Остановка по указанию.** Остаток наклона — это lowered IR: `run_set` получает
весь набор сразу, поэтому каждый `Module` жив до конца прогона. Линейный
компонент остался, и переделывать движок я не стал — приношу данные.

`~6,3 ГиБ` теперь фигурируют только как экстраполяция удерживавшего пути и явно
не как необходимая цена V1; фазовый путь экстраполируется в `~3,2 ГиБ`, что тоже
экстраполяция реализации, а не требование контракта. Lower conformance profile
не вводится. §6 ADR-0069 переписан, формулировка про `MAX_PROCESSES` заменена на
«jointly constrained by the ADR-0040 memory budget», граница ~55 МиБ помечена
приблизительной, поскольку накладные расходы на таблицы страниц сами зависят от
размера гранта.

### 2026-08-25 — из 15 МиБ живого модуля смысла около 6,5

Диагностика остаточного наклона. Для каждого lowered `tos_ir::Module` на том же
ceiling-fixture измерены live-дельта арены, длина `canonical_stream` и таблицы.
`canonical_stream` использован **только** как оценка плотности семантики: docs/43
намеренно не фиксировал on-disk encoding, и ничего такого здесь не предлагается.

| | live `Module` | canonical stream | отношение |
|---|---:|---:|---:|
| предельный модуль-зависимость | 12,10 МиБ | 5,26 МиБ | **2,3x** |
| entry (больше функций) | 19,18 МиБ | 8,41 МиБ | **2,3x** |
| замыкание из 8, всего | 104,03 МиБ | 45,24 МиБ | **2,3x** |

Отношение одинаково на 2, 4 и 8 модулях — значит это свойство представления, а
не фикстуры.

Таблицы одного предельного модуля: 2 268 типов, 2 268 экспортов, 2 268 функций,
2 268 блоков, 6 801 инструкция, 9 068 SSA-значений, 1 константа и **11 338
записей source-map**.

Повторение живёт именно там. Каждая `SourceMapEntry` владеет шестью `String`, из
которых пять именуют **модуль**, а не операцию: `1 666 686 B` текста при **147
различных байтах**, то есть одна и та же идентичность выписана 11 338 раз. У
entry — 3 198 450 B при 150 различных. Это 13–16 % живого модуля.

**Ответ на поставленный вопрос:** из ~15 МиБ живого модуля примерно 6,5 МиБ —
канонический семантический поток, примерно 8,5 МиБ — накладные расходы
представления: запас ёмкости `Vec`, выравнивание узлов, заголовки `String` и
повторённый текст идентичности.

Поскольку канонический поток существенно меньше, по указанию остановился и
подготовил **ADR-0070 (Proposed)**: compact immutable verified execution image
как **производный кэш** — источник остаётся каноническим, образ digest-bound,
удаляемый и полностью восстановимый из источника, verifier boundary не
обходится (либо повторная верификация при загрузке, либо проверяемая привязка к
выданной расписке; третьего — «доверять кэшу, потому что он наш» — нет).
Числа выигрыша в ADR не заявлены: их место — измерение той же фикстуры с
образом, до принятия, а не после.

Движок и `run_set` не тронуты. ADR-0069 остаётся Proposed, `54 MiB` provisional.

### 2026-08-25 — ADR-0070 переработан: кодировка до верификатора, а не после

Четыре указания Project Architect, и первое меняет саму цепочку доверия.
Post-verifier transform убран: компактная форма — **недоверенный** bounded/
versioned encoding `tos-ir/v1`, созданный **до** верификатора; верификатор
проверяет именно это представление и выдаёт расписку, связанную с semantic
module digest и точной artifact identity; движок исполняет проверенный образ.
Выбор «сделать образ после верификации, а потом либо перепроверить, либо
доверять привязке» не отвечён, а удалён: кэш, которому доверяют потому, что он
свой, — это ровно тот отказ, ради предотвращения которого решение и существует.

Persisted form обязан выполнить docs/43 §1 целиком: magic, версия кодировки
независимая от семантической, явные границы длин и таблиц до любой аллокации,
канонические правила, fail-closed на неизвестные поля и версии, artifact digest
отдельно от semantic, и негативные тесты парсера как условие принятия.

Компактность и residency больше не альтернативы. Восемь канонических потоков
предельных модулей — уже `45,24 МиБ`; экстраполяция нынешней плотности на 256
модулей — порядка `1,35 ГиБ`. Это не граница будущей кодировки, но достаточно,
чтобы «сделать модули меньше и держать все» не считалось ответом на вопрос о
гранте. ADR-0070 решает плотность **одного** проверенного образа и не обещает
резидентности замыкания. Streaming/lazy execution перенесены из альтернатив в
явное follow-up требование: отдельный ADR определяет bounded verified-module
residency, где поставщик — явный аргумент движка, ограниченный точным
разрешённым замыканием и идентичностями, без ambient filesystem, network и
module search.

Связь с ADR-0044 зафиксирована. Тот ADR предлагает digest scheme v2 —
канонические varint и module-level identity вместо повторения — и прямо ждал
операционного повода: «receipt or cache persistence, boot-time pressure, many
modules». Три из четырёх наступили и измерены. Две независимые кодировки не
вводятся: либо образ есть представление v2 внутри рамки, либо верификатор
образа независимо восстанавливает тот же versioned semantic digest. Интернирование
идентичности source-map — часть общего представления: логически каждая запись
по-прежнему несёт поля docs/43, физически пять из шести ссылаются на одну запись
идентичности модуля.

В ADR-0069 исправлена альтернатива про «в двадцать пять раз»: после §6 это была
экстраполяция удерживавшего пути, а не утверждение о цене V1. Остаётся более
узкое и достаточное: грант ограничен общемашинным бюджетом, делимым с тремя
другими процессами. `54 MiB` остаётся provisional до evidence по compact image и
bounded residency. Отдельно записано для будущего residency ADR: память гранта и
физическая резидентность образов считаются **раздельно**, но обе тратятся из
одного бюджета ADR-0040 — вынос IR из арены сам по себе не экономия физической
памяти, а перенос статьи расхода.

### 2026-08-26 — Прототип компактного образа: 33x на артефакте, 28,32 МиБ на проверке

Пункт 4 указаний по ADR-0070 выполнен: собран **измерительный** encoder/parser/
verifier path (`source/tests/image-prototype/`), production engine не тронут.
Контейнер — явно экспериментальный `TOSIMGx0`, версия `0`; движок его не
исполняет никогда. Container/security surface реализован полностью: magic,
версия кодировки независимая от семантической, канонические varint, длины
секций и таблиц, границы **до** аллокации, artifact digest, fail-closed на
неизвестную версию и неизвестный тег. Payload покрывает ровно ту поверхность,
которую задействует ceiling fixture; всё остальное отказывает с обеих сторон —
энкодер не пишет то, что не умеет round-trip'ить, парсер не читает тег, которого
не знает. Покрытие перечислено поимённо в evidence: 63 поддержанных tagged
variant и 33 отказывающих.

Один предельный модуль (`262 116 B` источника):

| | Байт | Против образа |
|---|---:|---:|
| живой `tos_ir::Module` | 12 864 160 | **33,13x** |
| нынешний canonical stream | 5 561 951 | **14,32x** |
| **образ** | **388 329** | 1x |

Главный инвариант выполнен: semantic module digest после `encode` →
verifier-owned parse совпадает с digest исходного модуля побитно
(`sha256:83954abd…`), и расписка привязана именно к нему. Artifact digest —
отдельный (`sha256:0352926a…`).

Интернирование идентичности source-map измерено впервые: `11 338` записей несут
**одну** различимую идентичность; `1 869 322 B` при записи в каждой записи
против `89 269 B` таблицей и ссылками — экономия `1,70 МиБ`, `20,9x` на секции.
Логически каждая запись по-прежнему несёт семь полей docs/43. Остаток секции —
`7,87 B` на запись, это спаны, а не повторение.

Время: encode `39,35 мс`, decode `16,95 мс`, verify `67,06 мс`. Парсинг —
примерно четверть стоимости последующей проверки.

**Главный вывод — не лестный.** Артефакт уменьшился в 33 раза, память для его
проверки — нет: пик `28,32 МиБ`, из которых `12,19 МиБ` — материализованный
`Module`, остальное — рабочий набор самого верификатора. Компактный образ есть
утверждение о хранении и передаче; **величина, которую обязан ограничить
residency-ADR, — это пик проверки, а не размер артефакта.** Ровно то раздельное
счетоводство, что записано в ADR-0069 §7.

Негативы: 19 случаев, каждый отказан с названной причиной — wrong magic,
unknown version, truncated frame/body, oversized, trailing bytes, wrong digest
(и по payload, и по самому digest), repeated/unsorted string table,
non-canonical varint, varint past 128 bits, count past limit, count past
remaining bytes, out-of-range string ref, bad UTF-8, unknown `TypeDef` tag.
Payload-негативы запечатаны **корректным** digest: атакующий владеет байтами,
значит владеет и digest, поэтому за него стоит только парсер. Тотальность:
`4 096` пересеянных однобайтовых мутаций (3 018 отказано, 1 078 разобрались в
какой-то модуль — это работа верификатора, не парсера) и `2 055` префиксов
(все отказаны), ноль паник при `panic = "abort"`.

Evidence: `docs/evidence/STAGE3_COMPACT_IMAGE_P1.md`. В §6 ADR-0070 внесено
явное правило о допустимом частичном покрытии, обязательной записи покрытия в
evidence, fail-closed на неподдержанные теги, экспериментальном `v0`, полном
container surface, принадлежности парсера verifier path и главном инварианте
digest equality; там же записано, что такой прототип — evidence для решения о
плотности и архитектуре, но **не** production format и **не** закрывает
completeness requirements docs/43 §1. Статус ADR-0044 не менялся: канонические
varint и module-level identity использованы как экспериментальный кандидат в
v2. ADR-0069 и ADR-0070 остаются Proposed, `54 MiB` — provisional. Engine
integration и полная поверхность IR не начаты.

### 2026-08-26 — ADR-0070 Accepted, ADR-0071 спроектирован и измерен

**ADR-0070 принят** после трёх правок и с новым §7. Формулировка «~6,5 МиБ —
семантическая нагрузка» изъята: canonical stream сам был представлением со
своей ценой, и назвав разницу overhead'ом, мы сделали из него пол, которым он
не был. Осталось измеренное: тот же модуль, тождественный по semantic digest
после round-trip, — `12 864 160 B` живым, `5 561 951 B` потоком, `388 329 B`
образом; theoretical minimum не заявлен. §3 развёл версионирование: образ **не
обязан** быть байт-в-байт потоком digest scheme v2; verifier-owned parser
восстанавливает `tos-ir/v1` и **сам** вычисляет versioned semantic digest из
восстановленного модуля, а receipt связывает его с exact artifact digest.
Образ, несущий собственную идентичность полем, просил бы себе поверить. §4
решает **encoded byte density**, и обе величины записаны раздельно:
`388 329 B` кодировки, `29 697 360 B` verifier working-set peak. **§7 —
implementation gate:** принятие не разрешает production engine integration;
она ждёт формата, покрывающего 100 % `tos-ir/v1` и полностью закрывающего
docs/43 §1. `TOSIMGx0` не кандидат на продвижение.

**ADR-0071 (Proposed)** — bounded verified-module residency: последовательная
верификация точного замыкания на запуске; fixed-size `VerifiedModuleRecord`
отдельно от bounded `VerifiedClosureManifest`; provider с opaque
`ClosureModuleId`, который чеканит только манифест; отсутствие ambient
filesystem/network/search; reload — это **байтовая идентичность** против
доверенной записи (полная верификация один раз на запуске, immutable snapshot,
хеш и разбор одного объекта, парсер всё равно тотален); continuation именует
module/function/block identity; границы по числу и по **module-derived state**;
раздельные счета грант/машина; отказ как единственное поведение; и receipt из
того же кэша — не receipt.

**Evidence собран** (`docs/evidence/STAGE3_MODULE_RESIDENCY_P1.md`), измерительный
harness — `source/tests/residency/`.

Launch плоский по размеру замыкания: наибольший модуль в полёте `19,21 МиБ` при
2 модулях и `19,20 МиБ` при 16. Арена после запуска 16-модульного замыкания —
`6 319 088 B` committed при магазине `6 305 109 B`: остаётся `13 979 B`, то есть
не удерживается ничего. Рост фронтира `52,39 → 60,11 МиБ` (+14,7 % на +700 %
модулей) — это раскладка аллокатора, а не удержание; удерживающий путь стоил бы
около `307 МиБ`.

`VerifiedModuleRecord` — **592 B фиксированного размера без кучи**. Манифест
16-модульного замыкания — 15 связей, `768 B`. На потолке 256: записи `148,0 КиБ`,
манифест `11,3 КиБ`, вместе `0,16 МиБ` против живого замыкания порядка `3,1 ГиБ`.
Строительные леса (`1,53 МиБ` export-таблиц) освобождаются при построении
манифеста.

Резидентность, 8 модулей — таблица, ради которой §7 и написан:

| Bound | Образ | Decoded | Итого | Загрузок | Вытеснений |
|---:|---:|---:|---:|---:|---:|
| 1 | 0,49 МиБ | **19,33 МиБ** | 19,82 МиБ | 15 | 14 |
| 2 | 0,86 МиБ | 31,17 МиБ | 32,03 МиБ | 8 | 6 |
| 4 | 1,60 МиБ | 54,81 МиБ | 56,42 МиБ | 8 | 4 |

Коэффициент **39** между образом и декодированным состоянием при bound=1: байтовая
граница, считанная по образам, назвала бы это «полмегабайта резидентно» при
двадцати. **Два — наименьшая нетрешащая граница** (рабочее множество вызова —
вызывающий плюс вызываемый); четыре не даёт ничего и стоит `24 МиБ`. Отдельно
для ADR-0069: `19–20 МиБ` на резидентный модуль — это bound=2 в пределах
provisional `54 MiB` и bound=4 уже нет (`56,42 МиБ`).

Adversarial A↔B при bound=1: 16 вызовов дали 33 загрузки, 32 вытеснения (16 — из
приостановленного модуля), 16 перезагрузок, `14,26 МиБ` хешировано, `534 мс`,
ответ верный. Eviction under suspension доказан исполнением: вызывающий вытеснен
во время приостановки, перезагружен, проверен по artifact digest и **возвращён в
него**.

Негативы: missing, stale, substituted, truncated, shifting-provider и forged
record — все отказаны с указанием модуля и проверки. Widening теста не имеет,
потому что не имеет представления: `ClosureModuleId` чеканит только доверенный
манифест, поле приватно, конструктора нет.

ADR-0071 остаётся **Proposed** до чтения evidence Архитектором; `54 MiB` —
provisional; production integration закрыта gate'ом ADR-0070 §7.

### 2026-08-26 — Два незакрытых bound ADR-0071: launch peak и manifest

**1. Launch frontier разложен по владельцам.** Прошлое объяснение («раскладка
аллокатора», committed-after-launch как доказательство) было неверным. Рост
`+7,72 МиБ` от 2 к 16 модулям делится ровно надвое: export lookup tables
(`+4,06 МиБ`) и closure-wide resolution snapshot (`+3,66 МиБ`). Сумма — 
`8 106 030 B` против измеренных `8 093 054 B`, расхождение `0,16 %`. Ничего от
аллокатора.

Обе величины сфазированы. Export-таблицы убраны двумя изменениями: верификация
идёт в **обратном порядке зависимостей** (вызывающие раньше вызываемых — порядок
образов топологический, значит к моменту, когда до модуля доходит очередь, все
ссылки на него уже висят), и pending links стали **фиксированного размера** —
модуль и экспорт названы 128-битным усечением sha-256, сравниваемым с тем же
digest'ом таблицы вызываемого, пока тот материализован. Ни одна строка не
переживает свой модуль. Declared resolution нарезана **по модулям**: верификатор
и так смотрит только на импортируемые модули, а форма, в которой запуск получает
резолюцию, — вопрос формата входа, а не изменение верификатора
(`tos_verifier::verify` не тронут).

После этого: накопленное живое состояние по замыканию `4,85 МиБ → 5 616 B`, а
**фронтир от освобождения первого модуля до последнего не двигается вообще**.

| Замыкание | Над магазином | Накоплено | Δ фронтира по релизам |
|---:|---:|---:|---:|
| 2 | 52,10 МиБ | 208 B | **0** |
| 4 | 52,63 МиБ | 240 B | **0** |
| 8 | 53,69 МиБ | 2 928 B | **0** |
| 16 | 55,76 МиБ | 5 616 B | **0** |

Форма стала правильной: `one-module scratch + widest import surface + bounded
closure metadata`. Но **`54 MiB` не держит запуск** — и причина уже не
накопление, а то, что верификация одного предельного модуля стоит около
`52 МиБ`, из которых **`31,75 МиБ` — рабочий набор самого верификатора**
(плоский при любом размере замыкания) поверх `20 МиБ` декодированного модуля.
Атаковать надо рабочий набор верификации, а не грант. Остаточный closure-scaled
член — **import surface самого широкого модуля**, живущий только на время его
верификации; в фикстуре entry импортирует всё, поэтому он и растёт (`0,8 → 4,4
МиБ`).

**2. Настоящая верхняя граница манифеста выведена — и она не проходит.** docs/44
§2 ограничивает «IR tables/blocks/instructions» **declared resource envelope**,
поля которого `u128` и объявляются самим модулем, — конечной границы с IR-стороны
нет. Границу даёт исходник: каждый call site надо написать. Измерено: плотнейшая
упаковка, принимаемая фронтендом в одном соответствующем `256 KiB` модуле, —
**32 256 cross-module call sites**.

| | |
|---|---:|
| ссылок в одном модуле | 32 256 |
| × потолок замыкания 256 | **8 257 536** |
| × `size_of::<Link>()` = 48 B | **378 МиБ** |
| бюджет ADR-0040 | **256 МиБ** |

Соответствующее замыкание может потребовать манифест больше всей машины.
Уплотнение не спасает: полностью `u32`-ссылка — 24 байта и всё ещё `189 МиБ`.
**Вынесено как Level-2 вопрос и намеренно не решено:** docs/44 §2 разрешает
меньший cap только в объявленном conformance profile, а выбирать профиль по
счёту за память — решение, а не измерение.

**Дефект, найденный по дороге.** Первая попытка упаковала 32 738 вызовов в одно
выражение — внутри всех опубликованных лимитов (меньше 256 KiB, вложенность
разделителей единица) — и **уронила процесс переполнением стека** в
`Parser::parse_schema`. Порог на этом хосте: цепочка ~14 000 слагаемых
разбирается, 15 000 — нет. docs/44 §2 говорит, что лимиты «prevent
attacker-controlled recursion»; падение — не отказ. Harness обходит это,
раскладывая вызовы по функциям (`8,4` байта на вызов против теоретических `9`),
дефект записан в evidence и не исправлен здесь.

Остальные пять ворот ADR-0071 закрыты и не перепроверялись. ADR-0071 остаётся
**Proposed**, ADR-0070 — Accepted, ADR-0069 и `54 MiB` — provisional, причём
`54 MiB` теперь считается **не прошедшим** launch evidence.

### 2026-08-26 — Frontend больше не тратит стек на длину плоской цепочки

Correctness-фикс нарушения Accepted docs/44 §2: лимиты обязаны предотвращать
attacker-controlled recursion, а соответствующий 256 KiB модуль ронял процесс.

**Парсер оказался почти невиновен.** Бинарные уровни он и так разбирает циклом;
падали *обходы* дерева. Левоассоциативная цепочка `a + b + c + d` — это
`(((a+b)+c)+d)`, то есть глубина равна длине, и каждый обход, спускавшийся в
`left`, тратил кадр на операнд. Виновных нашлось четырнадцать: typing,
ownership, mutability, guards, names, capability, boundary, defer, metering,
profile, concurrency (три обхода), exhaustiveness, constants, returns и
lowering.

Отдельно — **префиксные прогоны**: `!!!!…b` стоит байт на оператор, то есть
четверть миллиона в предельном модуле, и вот здесь рекурсировал уже сам парсер
(`parse_unary_expression` вызывал себя на каждый оператор).

И **деструктор**: `Expression` держит `Option<Box<Expression>>`, так что
сгенерированная drop-glue шла по левому хребту рекурсивно. Освобождение
соответствующего дерева падало даже после того, как все обходы стали
итеративными — падение на выходе вместо падения на входе, одинаково далёкое от
structured rejection.

Введён `crates/tos-core/src/walk.rs`: итеративный обход с явным worklist
(порядок посещения побайтово тот же — LIFO, дети кладутся в обратном порядке,
тело блока откладывается), и `binary_chain`/`prefix_chain` для тех обходов,
которые не смотрят, а *считают* — typing, lowering, constants. Рекурсия
осталась только там, где синтаксис действительно вложен: блоки, группы, тела
замыканий — всё это уже ограничено delimiter nesting = 256. Никакого нового
expression-length cap не введено.

**Digest не изменился.** В lowering спаны цепочки интернируются заранее,
снаружи внутрь — в том порядке, в каком их интернировала рекурсия, потому что
порядок вставки в source map входит в module digest. Проверено: предельный
модуль image-prototype даёт побитово тот же `sha256:83954abd…` и тот же образ
`388 329 B`.

Регрессии: цепочка на 15 000 операндов; самое длинное плоское выражение,
влезающее в 256 KiB, и первое, которое не влезает (structured `SourceTooLarge`,
не падение); прогон из 100 000 префиксных операторов. Preflight 36/36, включая
fuzz.

### 2026-08-26 — Import-edge манифест закрывает bound; верификатор оказался одной строкой

**Усечение убрано.** 128-битный обрезок sha-256 в launch-пути заменён: pending
edge несёт **content ID, который объявляет сам import** — точную идентичность,
уже записанную фронтендом в IR, — и разрешается 32-байтовым равенством с
записью вызываемого. Не имя, не хеш имени, не усечение. Неверное совпадение не
маловероятно, а невозможно.

**Манифест переведён на import-edge топологию.** Он хранит принадлежность к
замыканию и отображение каждого объявленного import-слота вызывающего в точный
opaque `ClosureModuleId`. Какую *функцию* достигает вызов — восстанавливается
внутри уже зафиксированного модуля, когда тот резидентен; таблица
`(export -> function index)` — это resident module-derived state по §7, входит в
байтовую границу и вытесняется вместе с модулем. Это не ambient search: модуль
уже выбран, provider расширить его не может.

| | |
|---|---:|
| import-объявлений влезает в соответствующий модуль | 10 520 (по исходнику) |
| потолок reference profile | 256 |
| **связывает потолок замыкания** | **255** |
| × 256 модулей | **65 280 рёбер** |
| хранение: один `ClosureModuleId` на слот + смещения | **523 268 B (0,50 МиБ)** |

**0,2 % бюджета машины** против `378 МиБ` у формы, которую это заменило — в 126
раз меньше. Level-2 вопрос закрыт сменой формы, а не меньшим cap'ом: ни один
conformance profile не понижен, ни один потолок не тронут.

**Разбор верификатора по `VERIFY_STEPS` — и результат неожиданный.** Девять
шагов проверки из девяти не выделяют ничего измеримого (изолированно, каждый в
своём процессе). Весь «рабочий набор верификатора» — это одна строка:
`tos_ir::module_digest` собирает канонический поток целиком в `Vec` и потом
хеширует. Поток — `5,30 МиБ`, фронтир — `15,75 МиБ` (растущий `Vec`
переаллоцируется). Значит **пулить нечего и bounded reusable scratch arena не
нужна**: лекарство — хешировать поток инкрементально, не материализуя его. Это
не семантическое срезание угла: digest побайтово тот же, все проверки остаются,
исчезает буфер, а не обход. Здесь не реализовано — это правка digest-пути
`tos-ir`, от которого зависит идентичность каждого модуля, и она заслуживает
отдельного коммита со сравнением digest до и после.

**Максимальная import surface одного модуля выведена.** Плотнейший
соответствующий экспортёр — 6 745 экспортов; его запись в declared resolution —
`644 896 B` (измерено). Модуль может импортировать максимум 255 других, значит
**`164 448 480 B` = `156,83 МиБ`** — почти втрое больше provisional гранта и
61 % всей машины. Из них `7,93 МиБ` — текст имён экспортов, остальное —
представление `BTreeMap<String, BTreeSet<String>>`, несущее его с коэффициентом
**20x**. Та же форма, что и `2,3x` у живого IR в ADR-0070. Но контракт есть
контракт: `tos_verifier::verify` принимает declared resolution с именами
экспортов каждого импортируемого модуля, и harness верификатор не меняет —
значит лекарство здесь **решение**, а не измерение.

Итог: у `54 MiB` теперь два ответчика, а не один — буфер digest'а (§10) и
широчайшая import surface (§11), и ни один из них не лечится увеличением гранта.
ADR-0071 и ADR-0069 остаются Proposed, `54 MiB` — не прошедшим launch evidence.

### 2026-08-26 — Этапы A–D: манифест, digest, declared resolution, и где launch упирается

**A. Манифест — только членство замыкания.** Прежняя import-edge форма давала
`0,50 МиБ` и помещалась, но аргумент про 255 слотов на модуль опирался на
`resource imports` из docs/41, который ограничивает transitive module
dependencies, а не число `import`-объявлений. Манифест теперь хранит одну
**точную resolved-module identity** на члена замыкания — пару «объявленное имя +
content identity», которую и проверяет `V2012_IMPORT`. Import-слоты вызывающего
и export-индекс вызываемого разрешаются при резидентной загрузке и живут как
resident module-derived state §7. Измерено на потолке 256: `Member` 136 B,
манифест `34 856 B`, записи `151 552 B` — **182 КиБ постоянного состояния
запуска**, ограниченного только потолком замыкания. Поиск — 134,8 нс.

**B. Канонический поток больше не материализуется.** Один обход пишет в
абстрактный sink: `Bytes` для `canonical_stream()`, `Digest` для
`module_digest()`. Порядок вызовов `tag/number/signed/text/blob/count/flag` не
тронут, второго кодировщика нет, тест сверяет хеш диагностического потока с
потоковым digest. Та же правка — в `tos_verifier::source_map_digest`. Фронтир
верификатора над декодированным модулем: **15,75 МиБ → 0**; пик decode+verify
**28,32 → 12,57 МиБ**. И это не медленнее: `module_digest` 50,75 → 45,34 мс.
Идентичность неизменна — `sha256:83954abd…`, образ побайтово тот же.

**C. Declared resolution упакован.** docs/43 требует полный snapshot, но не
`BTreeMap<String, BTreeSet<String>>`. Имена лежат подряд, всё остальное —
спаны: сортированная таблица модулей, непрерывный сортированный диапазон
экспортов на модуль, сортированные capability-спаны. Ничего не выброшено — это
не «экспорты, которые вызывающий реально использует». Отдельным флагом и тестом
сохранено различие, которое раньше нёс отсутствующий ключ карты: **модуль без
объявленной export surface и модуль с пустой — разные факты**. Широчайшая
import surface: **156,83 → 25,84 МиБ**, накладные расходы представления над
текстом имён 20x → 3x.

**D. Launch перемерен и проверен жёстким лимитом.** Образы вынесены на хост-
аллокатор — это учёт §8, а не удобство: в TOS образ отображается, а не берётся
из гранта. Сборка `--features grant` даёт арену ровно `54 MiB`.

| Замыкание | Grant frontier | Крупнейший модуль | Магазин |
|---:|---:|---:|---:|
| 2 | 19,79 МиБ | 19,21 МиБ | 0,86 МиБ |
| 16 | 20,69 МиБ | 19,20 МиБ | 6,01 МиБ |
| 64 | 23,98 МиБ | 19,13 МиБ | 23,54 МиБ |
| **256** | **37,16 МиБ** | 18,84 МиБ | 92,88 МиБ |

**Все они проходят внутри жёсткой арены 54 MiB**, на опубликованном потолке
замыкания, без пониженного conformance profile и без адаптивного поведения.

**Стоп-сигнал.** Один допустимый случай не помещается: модуль, импортирующий 255
модулей с максимальной export surface (6 745 экспортов каждый). Declared
resolution такого модуля — **38 584 176 B (36,79 МиБ) живых**, рядом с
декодированным модулем на 12,00 МиБ; пик **58 804 784 B (56,08 МиБ)**, и сборка
с жёстким лимитом падает на аллокации. Владелец назван точно: это срез declared
resolution одного модуля — не манифест (34 856 B), не записи (151 552 B), не
верификатор (ноль над декодированным модулем после B) и не образы (вне гранта).
Грант не поднимался.

### 2026-08-26 — D2/E/F: 54 MiB держит худший случай, и формат образа готов

**D2 — узкое место снято без изменения семантики.** Declared resolution
худшего случая занимал `36,79 МиБ`, фронтир `56,08 МиБ`, жёсткий лимит падал.
Изменено только представление полного export surface: имена экспортов вынесены
из общей таблицы строк в свою, и экспорт стоит **один `u32`** — имена лежат
подряд, значит имя тянется от своего смещения до следующего, и длина не
хранится вовсе. Пара `{start, length}` — восемь байт, `size_of` это
подтверждает, а сужение второго поля не помогло бы: выравнивание добьёт обратно.
Убрать поле — то, чего сужение сделать не может. Измерено: **4,00 B метаданных
на экспорт**, без `repr(packed)`, `unsafe` и невыровненных трюков. Плюс: `build`
принимает уже отсортированную surface как есть (хранимая резолюция приходит
такой), и читатель, знающий размеры, резервирует точно.

Факты не тронуты: полная export surface каждого объявленного модуля, различие
`surface absent`/`present but empty`, точное байтовое равенство, сортированный
поиск, те же вердикты `V2012_IMPORT`/`V2013_CAPABILITY` — на каждое есть тест.
Имя экспорта длиннее 128-байтового потолка идентификатора остаётся точным,
потому что длина не хранится: это записано тестом, а не комментарием.

**Проверено жёстким лимитом, а не оценкой.** Арена ровно `RUNTIME_GRANT`:

| Под жёсткой ареной 54 MiB | Grant frontier |
|---|---:|
| замыкание 2 | 19,68 МиБ |
| 16 | 20,10 МиБ |
| 64 | 21,57 МиБ |
| **256 — опубликованный потолок** | **27,60 МиБ** |
| **256 + худшая declared resolution** | **42,42 МиБ** |

Широчайшая import surface: `156,83 → 16,03 МиБ`, накладные над текстом имён
`20x → 2x`. Build `0,22 мс`, поиск `297,9 нс` по 6 745 отсортированным именам.

**E — ADR-0071 и ADR-0069 помечены ready for acceptance**, статусы не менялись.
`54 MiB` больше не «provisional-failed», а **measured candidate pending architect
acceptance**. Устаревшие верхние summaries в evidence и обоих ADR исправлены.

**F — production-формат образа.** Новый крейт `crates/tos-image`: magic
`TOSIMAGE`, encoding version 1, schema version 1, `no_std`, `forbid(unsafe_code)`,
входит в freestanding-сборку. Покрытие **100 %** `tos-ir/v1`, посчитанное, а не
объявленное: фикстура называет каждый tagged variant — 36 `TypeDef`, 25 `Op`,
6 `Terminator`, 7 `Constant` и все закрытые семейства — и тест сверяет эти
количества с фикстурой, так что вариант, добавленный в `tos-ir` и забытый здесь,
падает тестом раньше, чем доходит до формата, который не умеет его записать.

docs/43 §1 закрыт целиком: magic, версия кодировки независимая от семантической
схемы, версия схемы, канонические varint, явные границы секций и таблиц до
аллокации, каноническая упорядоченность таблиц строк и идентичностей,
fail-closed на неизвестную версию и тег, artifact digest отдельно от
семантического, парсер тотален над произвольными байтами.

Доказано: round-trip всех вариантов с неизменным semantic digest;
encode→parse→verify даёт **ту же расписку**, что и прямая верификация, на
корпусе модулей, реально понижённых фронтендом; воспроизводимые байты и
повторное кодирование разобранного модуля как неподвижная точка; удаляемый и
регенерируемый кэш; отказ на каждом собственном префиксе; пересеянные мутации в
тестах крейта и в fuzz-гейте репозитория при `panic = "abort"`; негативы рамки и
полезной нагрузки, каждый со своей причиной.

`TOSIMGx0` не продвинут: остаётся в `tests/image-prototype` как историческая
фикстура, на которой снято evidence §6 ADR-0070 и ADR-0071 — измерение, у
которого подменили фикстуру, невоспроизводимо.

**Production engine integration не начата.** Гейт ADR-0070 §7 по формату закрыт,
но интеграция зависит ещё и от принятия ADR-0071 и ADR-0069 — они Level-2 и
Proposed.

### 2026-08-31 — ADR-0075 и ADR-0076: у памяти появляется один счёт

**Найдены два счётчика над одной памятью.** `process.rs` финансировал грант
процесса прямо из пула: `grant_bytes` спрашивал `frames.available()`, влезает ли
`RUNTIME_GRANT`, и вырезал его. ADR-0075 §2b ставил корневую `MemoryAuthority`
над «пулом после фиксированных резервов нуклеуса». Это одни и те же кадры.
Построенное так дерево сообщало бы `remaining = N` для кадров, которые
`process_create` уже забрал, и наоборот — отказ на бумаге при выданной памяти.

**ADR-0075 Accepted** после четырёх правок, потребованных Архитектором:
attenuation по диапазону кадров отвергнута (refinement ничего не резервирует и
не создаёт объектов), введена reservation-модель учёта без двойного дебета,
G4 закрыт семантически (mapping — производная авторитета), и зафиксировано
корневое происхождение из boot endowment.

**ADR-0076 Accepted:** один пул, одно дерево, четыре правила. Каждый динамически
выделенный пользовательский байт заряжается ровно одной authority; грант — это
funded allocation, а не потребление способности; kernel-only накладные расходы
живут вне дерева, только если ограничены и зарезервированы до его появления;
второго счётчика свободной памяти нет. Операции 8 и 15 признаны подлежащими
ретайру, а не тихому финансированию из root: операция, тратящая root без
предъявленной `MemoryAuthority`, — это и есть скрытый второй счётчик.

**Реализован примитив.** `nucleus/src/region.rs`: две таблицы и правило между
ними. Authority — конечный бюджет с `allocated + reserved + free == budget` в
каждом узле; `RegionObject` — память с идентичностью, режимом и счётом того, что
до неё ещё дотягивается. Ничего не отображает, не имеет syscall'а: это машина
состояний, против которой пишутся операции, доказанная до того, как хоть одна
из них появилась.

### 2026-09-01 — чек вместо суммы, и невозможный размер — не предел

**Возврат гранта умел создавать бюджет.** `refund_grant(authority, charged)`
позволял вызывающему назвать сумму, не проверял поколение и использовал
`saturating_sub`. Ошибка жизненного цикла, вернувшая грант дважды, тихо
кредитовала authority памятью, которой машина не получала. Это хуже течи: течь
видна в учёте, а придуманный бюджет раздаёт занятые кадры.

Заряд теперь производит `GrantCharge` — не `Copy`, не `Clone`, потребляемый
возвратом. Позже выяснилось, что и этого мало: один authority, финансирующий два
процесса по 54 MiB, держит 108; первый возвращается, остаётся 54, и подделанный
дубликат первого чека находит ровно то, что заявляет, — байты второго процесса,
пока тот работает. Введён ограниченный ledger: чек именует конкретный
outstanding charge, а не сумму.

**Разведены `E_BAD_ARGUMENT` и `E_LIMIT`.** Переполнение при округлении не могло
быть удовлетворено никаким бюджетом; нехватка — факт про сегодня. Вызывающий,
которому на невозможный размер ответили «предел», повторяет запрос вечно.

### 2026-09-01 — страничные таблицы уходят в резерв, и пул остаётся дереву

**Таблицы брались из пула по кадру за раз, на каждом `map_page`.** Корень,
эндаументленный «над тем, что осталось», обещал бы кадры, которые пейджер
продолжал тратить за его спиной. Резерв теперь вычитается из пула до эндаумента,
а пейджер получает его **вместо** пула: `paging` больше нигде не принимает
`&mut Frames`, и «страничная таблица не может быть построена из обещанной
памяти» стало свойством сигнатур.

**Граница выводится, а не измеряется**, и два раза оказывалась неверной.
Ограничение identity-карты по старшему описанному адресу требовало 34 230 кадров
— 133 MiB, потому что framebuffer этой машины лежит у вершины 32-битного
диапазона. Счёт одной таблицы на 2 MiB описанной памяти требовал 5 670, потому
что пейджер кроет основную массу 2-мегабайтными листьями, которым таблица не
нужна вовсе. Повторение того, что пейджер делает на самом деле, по тем же
чанкам, требует 615 кадров, из которых собственное пространство нуклеуса
занимает 23.

**Резерв сделал несущей задокументированную течь.** Таблицы мёртвого адресного
пространства не возвращались никогда — «около пятидесяти кадров на процесс»,
названо в комментарии и оставлено. Против пула в 58 тысяч кадров это невидимо;
против ограниченного резерва это разница между границей на число одновременных
пространств и границей на число пространств за загрузку.

### 2026-09-01 — создание процесса становится транзакцией и узнаёт свою цену

**`create` открывал свою стоимость по мере траты.** Построить пространство,
прочитать header, отобразить данные, спросить пул про грант, измерить record.
Зарядить такое `MemoryAuthority` нельзя: к моменту появления числа память уже
ушла. Теперь всё решаемое решается до первого кадра, а `DynamicCharge` называет
весь pool-backed footprint построчно: writable data 8 192, арена 56 623 104,
стек 2 097 152, report 65 536, arguments 4 096, record 4 096 — итого 58 802 176
байт, ровно те 14 356 кадров, которые reclamation всегда возвращала.

**Отказ больше ничего не оставляет.** Пользовательские кадры вычитываются
обратно из страничных таблиц, launch record снимается и освобождается одним
carve'ом, таблицы уходят в резерв. Отдельно исправлен кадр, который размотка не
могла увидеть: между `allocate_frame` и `map_page` он уже вышел из пула и ещё не
стал листом. Девять способов отказа гоняются детерминированно на реальной
машине, из них `grant-table` — ровно в этот миг.

**Root эндаументится над тем, что у пула осталось**, без второго вычитания
резерва. `grant_bytes` перестал спрашивать пул: это был второй счётчик в одну
строку.

### 2026-09-01 — способности учатся жизненному циклу объекта

**Endowment мог оставить процесс полу-наделённым.** `endow` гранил по очереди и
останавливался на первом отказе — и выполнялся **после** `admit`, так что
лаунчер, чей третий грант отказан, получал опубликованный runnable-процесс с
первыми двумя. ADR-0055 называет такого ребёнка недействительным. Выбран
preflight с последующим infallible commit: всё отказывающее счётно заранее, а
откат, который никогда не выполняется, ничего не доказывает.

**Очередь сообщений — держатель.** Между коммитом отправки и выдачей получателю
только она способна произвести держателя. Ссылка берётся до постановки в
очередь и отпускается после того, как получатель стал держателем, — ноль
недостижим.

**Решение B по владению `MemoryAuthority`:** несколько имён, один бюджет. Узел
считает живые ссылки; отпускание одного алиаса не возвращает ничего, а последнее
— возвращает неизрасходованный остаток вверх по линии финансирования. Root —
якорь загрузки: `retain`/`release_name`/`revoke` над ним отказывают, ring 3
никогда его не именует.

### 2026-09-01 — операции 16 и 17: резервирование и первый настоящий регион

**Операция 16 — не операция 5 с суммой.** Пятая уточняет права и возвращает ещё
одно имя того же бюджета; шестнадцатая создаёт новый узел учёта и опускает
остаток родителя. Доказано из ring 3: после того как алиас зарезервировал
остаток, оригиналу отказано в тех же байтах — два имени, один бюджет.

**Fixed-lane aperture.** Один lane на слот региона, `1 << 39` за ветвь PML4.
Аллокатора нет, дыр нет — и потому нет второй дефицитности: резерв, который
гарантировало дерево, не может быть отказан из-за отсутствия подходящего
виртуального промежутка.

**Граница резерва нашла дубликат.** С lane'ами формула давала 1 477 кадров, и
четвёртый процесс переставал создаваться — на 17 кадров. Причина оказалась не в
lane'ах: ведущий член `identity` резервировал собственное пространство нуклеуса
второй раз, после того как оно стало строиться из пула до появления резерва.
Снятие дубликата вернуло 25 кадров. Ни один потолок не двигался.

**Операция 17.** `RegionBackingSpace` — метаданные в форме таблиц, никогда не в
`CR3`: идентичность региона есть то, из каких кадров он состоит, и эта запись
переживает любое отображение. Транзакция не даёт региону стать недостижимым:
он рождается с construction-ссылкой нуклеуса, которая отпускается только после
того, как способность вручена. Пятнадцать способов отказа — девять для создания
процесса, шесть для региона — сравнивают семь счётчиков до и после.

Из ring 3: некратный размер возвращается округлённым, база в своём lane'е,
регион нулевой (значит, аллокатор не обошли), записываются оба конца, после
освобождения тот же lane переиспользуется, а старый handle не резолвится. Все
кадры возвращаются к эндаументу root'а, все таблицы — к базе резерва.

**Физическое раньше бухгалтерии.** Байты свободны не когда ушла последняя
ссылка, а когда кадры вернулись в пул. Между этими моментами стоит non-Copy
`Reclaim`, который нельзя закрыть дважды и нельзя закрыть за чужой регион.

### 2026-09-02 — три состояния региона, две потребляющие операции и один транспорт

**Регион перестал быть парой счётчиков.** `RegionObject` держал
`writable_mappings` и `readable_mappings` — два числа, которых достаточно, пока
регион принадлежит одному процессу, и которые не описывают разделяемый регион
вовсе. Три вопроса, на которые счётчик не отвечает: отображён ли регион `R` в
процессе `P`, в каком режиме, и уходит ли окно вместе с этим handle'ом. Последний
и решает корректность: процесс может держать несколько `SharedRegion`-имён при
ровно одном физическом окне, и счётчик, растущий вместе с именами, никогда не
скажет, когда окно должно уйти. Теперь регион помнит **какие** адресные
пространства его отображают — битовым множеством по слотам процессов, — а
агрегаты выводятся popcount'ом, так что расходиться им не с чем.

**Аффинность — свойство объекта, а не прав на handle.** Неизменяемый аффинный
регион и разделяемый несут одинаковое отсутствие `write`; правило, читающее
аффинность из маски прав, считало бы их одним и тем же и позволило бы
attenuation'у, снявшему бит, превратить одно в другое. Поэтому в таблице
способностей два внутренних варианта — `Object::Region` и `Object::SharedRegion`,
— оба представляющиеся процессу как `OBJECT_REGION`, а на вопрос «можно ли второе
имя» отвечает сам регион.

**Операции 18 и 7 потребляют, и обе сохраняют слот.** Заморозка и разделение
возвращают **другой** handle на тот же регион: выдать новую способность и
освободить старую значило бы взять вторую ссылку, которую аффинный регион
отказывает, и второй слот таблицы, которого может не быть, — с окном между ними,
где регион назван дважды. Запись на месте с продвижением поколения не требует ни
того, ни другого, и счёт ссылок региона всё время равен единице.

Заморозка судит **весь** lane до того, как сдвинется первый бит: ветвь на месте,
ровно столько страниц, сколько зафиксировано, каждый лист называет тот кадр, что
называет backing-индекс, present, user, NX, currently writable, и ничего за
длиной. Ниже этой черты — один бит на лист, тот же адрес, те же кадры, один
перезагруженный `CR3`. Ни кадра, ни таблицы, ни байта учёта.

**Приём сообщения стал транзакцией.** Раньше приём снимал сообщение с очереди, а
затем гранил — и грант, которому не хватило слота, писал нулевой handle туда, где
должен быть авторитет. Это частичная доставка со статусом успеха, и она перестаёт
быть терпимой в тот момент, когда поехать может регион: снятое и признанное
неприемлемым сообщение — это сообщение, которого нет ни у кого и которое нельзя
повторить. Теперь `peek → preflight → commit → copy → pop → отпустить transit`.
Preflight спрашивает всё, что может отказать, в одном месте — слоты под
способности **и** под регионы, правило одного получателя ровно в той же форме,
в какой его проверяет `grant`, сумму имён каждой authority, наличие адресного
пространства, состояние каждого региона, свободный lane под каждое окно и резерв,
которого оно стоит. Исправлено и для обычных способностей, не только для
регионов.

**Отказавшая отправка оставляет отправителю всё.** Место в очереди спрашивается
**до** того, как что-либо потреблено. Линейный регион, отнятый у отправителя и
затем оказавшийся без адресата, не принадлежит никому, и откат тут нельзя
гарантировать: восстановление окна требует таблиц из резерва и может отказать
само. Отправитель, заблокированный на месте в очереди, не отдал ничего, и когда
место появляется, выполняется **та же** транзакция заново — с аргументами,
которые всё ещё лежат в приостановленном кадре.

**Preflight строже своего commit'а — это дефект, и он был найден.** Первая
версия спрашивала ещё и о том, *годен ли* объект, — чего `grant` не спрашивает:
время жизни способности ограничено её объектом, и место, где это проверяется, —
`resolve`, один раз, когда держатель пытается через неё действовать. Случай не
умозрительный: `endpoint_call` вручает получателю право ответить **до** того,
как вызывающий заблокируется, так что в момент доставки reply-способность именует
вызов, который ещё не начал ждать. Каждая загрузка с request/reply вставала в
дедлок, пока лишний вопрос не убрали. Правило теперь записано там, где живёт:
спрашивать ровно то, что спросит commit, запись за записью, и ничего сверх.

**ABI: `r10` — способности, `r8` — регионы**, для операций 1 и 3. `r10` и был
счётчиком способностей; `r8` на них не был назначен, поэтому нуклеус,
прочитавший его у старого вызывающего, читал бы регистр, которого никто не писал
— тот же довод, что сделал `r8` допустимым на операции 15. Ни `endpoint_reply`,
ни `endpoint_reply_receive` не расширены: ни один принятый документ этого не
требует, а придумать им preflight, границу и семантику отказа значило бы принять
решение вместо того, чтобы его нести.

**Из ring 3.** Два процесса и один endpoint: worker с дочерней authority и
`send` на два endpoint'а — один peer вычерпывает, на втором не получает никто, —
и peer с одним лишь `receive`. Изменяемый регион отказан целиком, и отправитель
по-прежнему в него пишет. Три региона вместо двух — `E_BAD_ARGUMENT`. Полная
очередь отказывает, окно цело, и **тот же** handle затем уходит успешно. После
успешной отправки handle не резолвится, а следующее чтение того адреса —
страничная ошибка на CPL 3. Peer намеренно забивает собственную таблицу: первое,
о чём он просит очередь, — сообщение, которого ему нельзя дать, и ответ
`E_LIMIT` при сохранённом сообщении; один освобождённый слот спустя приходит оно
же. Байты совпадают на обоих концах заряженной длины, регионы приходят в разные
lane'ы, а блокирующий приём, отменённый правилом живости ADR-0059, — это способ
процесса узнать, что он остался один: всё, что читается после него, читается
после смерти отправителя.

И два выделенных процесса, каждый из которых умирает намеренно: один пишет
инструкцию в регион и прыгает в него — present, user, instruction fetch, — другой
освобождает единственный handle и читает — not present, user. Ошибка здесь и есть
доказательство, а ошибка в процессе, делавшем что-то ещё, была бы ошибкой,
которую некому приписать.

Резерв не сдвинулся: `1452` всего, `1451` база, `1` постоянный корень
backing-индекса. Заморозка не тратит таблиц, разделение не двигает отображений,
а приёмник спрашивает резерв о цене lane'а до того, как на него согласиться.

### 2026-09-02 — процесс оплачен предъявленной authority, или не создан вовсе

**Найдено ambient-финансирование в самом ядре создания.** `process::create`
самостоятельно доставал `memory::root()` и заряжал его. Каждый процесс, который
система когда-либо построила, тратил учётный якорь загрузки, и вызывающему не
нужно было держать ничего, чтобы это произошло. ADR-0076 §4 называет это прямо:
операция, тратящая root без предъявленной `MemoryAuthority`, и есть скрытый
второй счётчик — а операциями такого рода были обе, 8 и 15.

Ядро создания теперь получает узел финансирования аргументом и не имеет способа
его добыть: ни одна строка ниже не называет `memory::root`, и нигде нет
`funding.unwrap_or(root)`. Якорь может передать ровно один именованный путь —
`create_bootstrap`, которому загрузка вручает уже выбранный узел, — так что все
его употребления находятся поиском одного идентификатора. Структурный инвариант:
ни один достижимый из ring 3 путь создания не выделяет ни одного
пользовательского кадра без явного аргумента-`MemoryAuthority`.

**Арена — тоже аргумент, и это policy-число, а не доля свободного.** `r9`
проверяется против принятых границ `RuntimeMemoryGrant` и округляется до целых
кадров; заряжается округлённое. Нет `min(requested, available)`, нет «что
осталось», нет процента от родителя — размер, выбранный из пула, это тот самый
второй счётчик: измеренный в один момент и потраченный в другой. Заряд — весь
след процесса, а не одна арена: данные, арена, стек, отчётный регион, регион
аргументов и launch-запись.

**Операция 19 заменяет 8 и 15 сразу, и поэтому restart-поколение ушло из
регистра.** Восьмая не утверждала поколения, пятнадцатая утверждала — значит,
заменяющая обе обязана различать «отсутствует» и «присутствует и равно нулю».
Регистр этого не несёт, поэтому `CreateFundedRecord` несёт поколение и флаг, с
единственной канонической кодировкой: флаг снят — поле обязано быть нулём, и
всякий другой бит флагов обязан быть нулём. Два байтовых узора с одним смыслом —
это контракт со вторым, незадокументированным написанием.

**Финансирование живёт в учёте, а не в способности.** Доказано из ring 3 одной
строкой: создание ставит заряд и **не** потребляет способность; второй ребёнок
не влезает, потому что байты первого потрачены, а не обещаны; смерть первого
возвращает ровно тот заряд; а создатель, отпустивший своё последнее имя узла,
пока оплаченный им ребёнок ещё работает, не освобождает ничего — до тех пор,
пока тот ребёнок не кончится, и байты не уйдут вверх по линии финансирования
мимо узла, который никто не именует. Ничего не наследуется: ребёнок не получает
имени узла, оплатившего его, если создатель не назвал эту способность в
эндаументе как любую другую.

**Восьмая и пятнадцатая отказывают навсегда** — `E_NOT_SUPPORTED`, до чтения
своих аргументов, номера остаются занятыми навсегда. Вместе с ними ушёл
`process_create` из `SYSTEM_INTERFACE_V1`: он связывался с операцией 8, а
операция 19 в этой схеме невыразима — две способности, явный грант, явный
эндаумент и *способность* в результате там, где всякий объявленный результат
`i64`. Схема, рекламирующая операцию, которую ABI отвергает, хуже схемы, честно
говорящей, что пока этого не несёт. Модуль, объявляющий изъятую операцию, теперь
отвергается на проверке границы — до первой инструкции, с причиной, называющей
именно то, что не так.

**Операция 20: процесс из бандла, и граница доверия, которую ядро не переходит.**
Три способности, третья — обязательно *разделяемая* область: цель получает
собственное окно, а создатель сохраняет своё, то есть два держателя, и `share`
как раз и есть операция, делающая область способной быть в двух местах. Ни пути
модуля, ни ординала, ни entry: бандл объявляет свой вход сам, а входа от
вызывающего была бы вторая истина о том, какая это программа.

Ring 0 не читает ни байта артефакта — только факты о способности и жизненном
цикле. Launch v5 — **вторая форма записи**, а не второе прочтение одной:
дискриминатор читается первым, и запись, прочитанная не в той форме, это не
запись с неверными значениями, а набор указателей в то, что случайно там лежало.

Из ring 3: супервизор строит настоящий `TOSBUNDLE/v1` над собственным замыканием
загрузки в область, замораживает, разделяет — и создаёт из одной способности над
одним backing'ом **две** цели подряд. Ни пересборки, ни повторной заморозки, ни
копии; окно самого супервизора после обеих по-прежнему читается. Каждая цель
разбирает артефакт сама, тотальным парсером, и верифицирует каждый образ прежде
первой инструкции. А испорченный бандл — тот же артефакт с одним перевёрнутым
байтом магии — создаёт процесс **успешно**, и уже этот процесс отказывает себе:
успешное создание и неуспешное допущение суть два исхода двух компонентов.

**Запас на четыре обычных процесса стал нулём**, и это стоит сказать, а не
пропустить: четырём нужно 57 424 кадра, и root держит 57 424. Восстановил его не
подпиливание, а вынос: **каждая evidence-нагрузка ушла за фичу**, и образ,
который запускает канонический boot, больше не несёт зондов, до которых на нём
никто не может дотянуться — `system.boot.init` наделён ничем. Код, который boot
несёт и не может достичь, — это память, за которую платит каждый процесс.

### 2026-09-03 — операция возвращает полномочие, и план запуска стал объектом

**Сначала — узкое доказательство, которое Архитектор потребовал провести до
всякой реализации.** Путь `extern operation -> Result<nominal capability, i64>
-> lower -> TOSIMAGE -> независимая проверка -> исполнение движком` зелёный, и
ему не понадобилось ни нового варианта `tos-ir/v1`, ни нового конструктора типов
TOS Core. Диагноз Архитектора оказался верен с точностью до одной строки:
ограничение было во фронтенде. `boundary::type_text` принимал только
`TypeSyntax::Name`, поэтому схема не могла объявить результат, который не
является голым именем; `lower::resolve_type` разрешал путь интерфейса в
номинальную запись, лишь совпадающую с ним по имени, вместо типа способности,
конструктор которого лежит в IR с самого его написания. Пять стадий проверяются
по отдельности, а не сквозным прогоном: «оно запустилось» — самое слабое из
того, что должно было оказаться истиной.

**Наделение перестало быть таблицей, принадлежащей никому.** До этого набор
способностей ребёнка был четырьмя записями в собственном argument-регионе
родителя: годен для одного вызова, между решением и созданием не удерживается
никем, и потому перезапуск — это *второе* решение, выведенное заново из того, до
чего супервизор дотягивался в момент перезапуска. Два запуска одной службы по
одной политике могли различаться, и система бы об этом не сказала.

План запуска (ADR-0077) — объект: 21 создаёт строителя, 22 пишет в него по одной
записи, 23 запечатывает. 22 достигается **через саму передаваемую способность**,
и отсюда всё остальное: право положить endpoint в план — это владение этим
endpoint'ом, права пересекаются с удерживаемыми, а селектор ABI один на все виды
полномочий сразу. Открытым является множество интерфейсов, а не ABI. План берёт
собственную ссылку на всё, что называют его записи, — создатель может отпустить
свой хэндл, и план продолжит держать; это противоположность наследованию.

Запечатывание — та же форма, что у `region_freeze`: тот же слот способности,
поколение вперёд, тот же объект, ни одна ссылка не взята и не отдана. Операции 19
и 20 берут запечатанный план и **не потребляют его**: ровно в этом смысл —
перезапуск есть та же политика, применённая к новому экземпляру процесса.
`CREATE_ENDOWMENT` удалён, а не объявлен устаревшим: таблица, которую всё ещё
читают, — это второй способ наделить ребёнка.

**Что теперь пишется текстом:** план создаётся, наполняется через *два разных*
номинальных интерфейса (`system.ipc.Endpoint` и `system.memory.Authority`),
запечатывается и создаёт финансируемый процесс. Никакого `AnyCapability`, ни
одного сырого хэндла ни в исходнике, ни в артефакте — точный номинальный тип
сохраняется на каждой позиции.

**Найденная граница, и она архитектурная.** `Op::Capability` в `tos-ir/v1`
называет *собственную* способность операции индексом импорта и ничем иным.
Способности после первой ездят операндами — так план и доходит до 22, 23, 19 и 20
без единого изменения IR, — но операция, чья **первая** способность получена во
время работы, выразить себя не может. Из текста поэтому недостижимы:
`process_terminate` над ребёнком, `capability_release`/`capability_attenuate` над
полученным во время работы, и наделение ребёнка *ограниченной*
`MemoryAuthority`. Все три обхода рассмотрены и отвергнуты в
`docs/evidence/STAGE3_LAUNCH_PLANS.md` §6: два меняют `tos-ir/v1`, третий
объявляет в схеме полномочие, которое система не проверяет.

**Запас на четыре обычных процесса ушёл в минус, и Архитектор это предвидел.**
Заряд процесса сдвинулся с 14 356 кадров на 14 357, root — с 57 424 на 57 415.
Утверждение «root >= MAX_PROCESSES × обычный заряд» делало **каждую страницу
роста кода архитектурным STOP**, поэтому решением от 2026-09-03 смысл
`MAX_PROCESSES` зафиксирован: это число *слотов*, а не резервирование. Слоты и
память — независимые конечные ресурсы, и свободный слот при неоплачивающей
authority даёт `E_LIMIT` — правильное поведение, а не отказ. `RUNTIME_GRANT`
остался 54 MiB, `MAX_PROCESSES` — 4, эталонная машина не увеличена; изменился
gate. Он теперь *сообщает*, сколько обычных процессов финансирует root, и
утверждает топологии, которые система действительно исполняет: супервизор с
целью, и супервизор с временным сборщиком — с 112.11 MiB, остающимися под
bundle. ADR-0069 §7 переписан под это.

### 2026-09-03 — операция действует на полномочие, полученное во время работы

**Архитектор постановил по §6: вариант 1, явный дискриминатор источника.** Оба
отвергнутых обхода остались отвергнутыми: переинтерпретация поля `import` меняла
бы смысл уже закодированного поля — старый верификатор читал бы новый артефакт и
сообщал, что проверил не ту способность; вторая, «лицензирующая» способность
объявляла бы в схеме полномочие, которого система не проверяет.

Суть в том, что противоречие было **не в языке**. TOS Core V1 давно допускает
значения-способности и полномочия, производные от них: `TypeDef::Capability`
лежит в IR с момента его написания, формат образа его кодирует, граница движка
возвращает значение, а не целое. Не мог этого выразить только
`Op::Capability`, называвший способности операции индексами импортов и ничем
иным. Представление было уже принятой семантики — ADR-0078 чинит представление.

`CapabilitySource::Import(index) | ::Value(operand)`, и **на каждой позиции**, а
не только на собственной способности операции. Иначе дыра просто переехала бы на
позицию вперёд: второй способностью операции 19 является `MemoryAuthority`, а у
супервизора она ограничена во время работы. В доказательстве первая позиция —
импорт, вторая — бюджет, произведённый операцией 16.

Верификатор проверяет по позициям и по артефакту, а не по слову фронтенда:
импорт лежит в таблице модуля; значение имеет тип `TypeDef::Capability`; на
нулевой позиции интерфейс совпадает с записанным на инструкции; на **каждой**
позиции интерфейс объявлен эффектом объемлющей функции — это и есть точная
номинальная проверка там, где сравнивать не с чем; и ни один источник не
повторяется. Отрицательные случаи доказаны **порчей артефакта**: константа в
позиции способности, способность чужого номинального интерфейса, один источник
вместо двух.

Версия хранения TOSIMAGE 3 → 4: байты одной инструкции изменились, значит
изменилась версия, и старый читатель обязан отказать, а не прочесть тег
источника как индекс. Версия 3 читается и разворачивается в `Import` — это
ограниченно и канонично, потому что индекс импорта был единственным источником,
который она умела записать. Семантическая схема осталась `tos-ir/v1`.

**И это работает против настоящего ядра.** Новый gate `runtime-authority`:
загрузчик даёт ровно два полномочия — `create|terminate` над собой и остаток
корня, — и gate утверждает, что больше ничего не запрашивалось. Всё остальное в
прогоне произведено операциями: ограниченный бюджет, план, ребёнок, уточнённое
полномочие над ребёнком. Девять операций, все со статусом 0 от ring 0, и
`process_terminate` действует на уточнённое полномочие над ребёнком как на
собственную способность — тот самый случай, который до ADR-0078 нельзя было
записать.

Загрузка научила тому, чего не мог хост-тест: первая версия вектора отпускала
широкий хэндл ребёнка **после** его завершения, и ядро ответило
`E_NO_CAPABILITY`. Это работает `CAPABILITY_V1` §3 — время жизни способности
ограничено её объектом, — и вектор теперь отпускает, пока ребёнок жив, что и
честнее как супервизорский жест.

### 2026-09-03 — супервизор, написанный на TOS Core

**Секция H закрыта.** `/system/policy/services.tos` говорит, что супервизировать,
насколько упорно пытаться и что от чего зависит; `/system/boot/init.tos` —
машина состояний. Каждое число, на котором действует супервизор, приходит из
политики, и в его собственном тексте не названо ни одной службы. Хост-тесты
гоняют те же два модуля против скриптованной системы: они наблюдают и назначают,
когда ребёнок закончится и на каком тике, — политики и машины состояний в них
нет, и быть не может, потому что нужные числа лежат в модуле политики.

**Четыре состояния, и различия между ними — это и есть суть.** BLOCKED — это
утверждение о *настоящем моменте*: экземпляр не запущен и зависимость мешает
честному запуску. Оно пересматривается каждый круг и снимается, как только
зависимость вернулась; бюджет перезапусков оно не тратит, потому что ничего не
падало. FAILED защёлкивается: служба больше не пересматривается, а вопрос,
который раньше её бы запустил, получает в ответ саму защёлку, а не молчание.

**Правило перезапуска — плотность отказов, а не общий счёт.** Часы —
`ChildEnding.ended_tick`, монотонные от загрузки (ADR-0067): собственных часов
супервизору не нужно, и он их не просит. Граница окна доказана **на точном
тике** в обе стороны: два отказа ровно на ширину окна друг от друга не
накапливаются, на тик ближе — накапливаются, и больше в двух прогонах не
различается ничего. Реализация, написавшая `<=` вместо `<`, падает ровно там.
С другой стороны: у службы с окном в один тик десять отказов не копятся никогда,
пока две другие уже защёлкнулись, — это тот тест, который падает, если отказы
*считать*, а не *датировать*.

**`endpoint_send_text`** — вторая строка схемы над операцией 1 ABI, и это
единственный способ для модуля TOS Core сказать что-то в мир своими словами.
Журнал держит врозь то, что наблюдало ядро, что вывел супервизор, что решила
политика, что он попробовал и что вернулось. Границы — уже существующие: 256
байт `IPC_V1` §3 и очередь на четыре, которую супервизор сам и вычерпывает.
Кто *потребляет* операторский журнал, Stage 3 не решает, и здесь это названо
открытым вопросом, а не спроектировано молча.

**T1 против Capsule v1, выполненный.** Супервизор создаёт временного сборщика на
гранте его собственной роли (96 MiB); сборщик собирает настоящий `TOSBUNDLE/v1`
над канонической исходной базой этой капсулы, замораживает, разделяет и передаёт
регион; супервизор **собирает его окончание через `process_wait_child`** — и
только после этого создаёт цель. Порядок читается из лога, а не предполагается из
кода. С обеими ролями резидентно измерено: 39466 кадров из 57410, **70.09 MiB**
остаётся под bundle backing, что держит крупнейший bundle любой конфигурации
Capsule v1 (50.52 MiB) с запасом 19.57 MiB.

**`CreatedProcess` и `ChildEnding`** — номинальные записи, объявленные схемой
(§4.2), а не кортежи с запомненными позициями. Три необязательных факта окончания
— `Option<u64>`: ADR-0067 говорит, что отсутствие и есть истинное значение, а ноль
— это утверждение, которого никто не делал.

**Два настоящих дефекта, каждый пойман тестом, который мог упасть только от
него.** Первый: супервизор строил план на каждый запуск и не отпускал, так что
`MAX_PLANS` перезапусков исчерпывали таблицу — хост-тесты этого не видели,
загрузка показала восемь подряд `result.refused`. ADR-0077 §5 уже говорил, что
делать: запечатанный план переживает создание, которое его читает. Второй:
`launch_plan_endow` объявлял `rdx` только входом, тогда как любая операция
отвечает в `rax` и `rdx`, — и **вторая** запись плана получала имя нулевой длины.
Одной записи это не показывает; двух — показывает, а наделение сборщика как раз
из двух. Нашёл T1.

### 2026-09-03 — раунд закрытия Stage 3

**ADR-0074 сверен с системой, которая теперь существует.** Он объявлял себя
черновиком, в котором «ничего не реализовано», — это перестало быть правдой.
Добавлена §0: посекционная карта того, что реализовано, что вытеснено более
поздними принятыми ADR и что осталось историей. §4a — жизненный цикл **как он
выполняется**, а не как предлагался; §5c — почему Stage 3 не зависит от
фиксированного размера BuildWorkspace (арена сборщика есть финансируемый грант
роли); §5d — измеренный счёт T1; §6a — операция 20 как построена, с одним
содержательным разворотом относительно наброска: пути входа нет вовсе, потому что
bundle объявляет свой собственный, и caller-supplied вход был бы второй правдой о
том, какая это программа; §7a — все шесть открытых вопросов, отвеченные принятыми
контрактами. Статус — **Proposed for acceptance**: содержание сверено, остаётся
одобрение Архитектора.

**Центральный операторский вид ошибок.** Новой подсистемы не построено, и не
нужно: место, где сходится всё, уже существует — диагностический транспорт, чей
namespace `TOS.RUN.*` делегирован `RUNTIME_OBSERVABILITY_V1` по ADR-0042. §9 этого
контракта добавляет ровно три вещи: словарь важности (`DEBUG`…`FATAL`),
классификацию **по виду события** — важность есть свойство вида, а не выбор
эмиттера, поэтому ни один эмиттер не изменился, — и форму записи процесса, где
важность идёт первым сегментом имени. Границы все уже существующие: 256 байт
`IPC_V1` §3, очередь на четыре, report-регион в оплаченном следе. Персистентность,
ротация, восстановление между загрузками — **не решены** и не спроектированы;
§9.6 говорит это прямо.

**И аудит нашёл настоящую вещь.** Локально оба observer-gate выходят с кодом 2
ещё до загрузки — «у выбранного QEMU нет observer-build.json», — и я записывал это
как ограничение среды. Чтение собственного CI репозитория, который этот observer
**собирает**, показало другое: там они падали с кодом 101 — **ошибкой сборки** в
measurement-нагрузке (`Prepared` получил параметр времени жизни, а `struct Work`
за ним не пошёл). То есть результат конформанса *отсутствовал*, а не был
недоступен здесь, и так было ещё до этого раунда. Это обычная поломка, а не
противоречие, поэтому она исправлена, а не вынесена в отчёт: `Prepared<'static>`
— ровно то, что `launch` всегда и возвращал. Прогнан полный перебор: все
feature-сборки образа и ядра теперь собираются.

Заодно исправлена собственная ошибка учёта: MANIFEST.txt собирался **до**
`git add`, а инструмент перечисляет отслеживаемые файлы, — CI поймал это дважды.

**И после починки конформанс наконец получен.** Прогон рабочего процесса
репозитория на коммите закрытия — с тем самым принятым observer'ом, собранным по
`build-simple-observer.sh` из digest-pinned исходников QEMU 10.0.11:

    QUALIFY-OBSERVER PASS: evidence=P2 positive_pairs=21/21 sign_p=4.77e-07
    QUALIFY-IPC PASS: evidence=P2 p99=39.147 us of 300 samples <= 200.0 us
    PREFLIGHT PASS: 45 gate(s) passed

`p99 = 39.147 мкс` против бюджета `200 мкс` — впятеро внутри, на уровне
свидетельства P2. Ни один порог не тронут: diff по всем файлам с порогами за всю
эту работу пуст. Относительное отношение `7.957x` записано как **наблюдение**, а
не бюджет: ADR-0068 убрал его из конформанса.

`docs/evidence/STAGE3_CLOSURE_AUDIT.md` — одна карта: требование → нормативный
источник → реализация → gate → вердикт, четыре вердикта и никаких «в основном
сделано».

### 2026-09-03 — Stage 3 закрыт

Архитектор одобрил ADR-0074 в сверенном виде, амендмент `RUNTIME_OBSERVABILITY_V1`
§9 и формальное закрытие Stage 3 на коммите свидетельства `77970cb`. Этот коммит —
только бухгалтерия закрытия: статусы, записи одобрения и текст о текущем
состоянии. Ни поведение ядра, ни семантика TOS Core, ни формат IR или образа, ни
операции ABI, ни поведение супервизора, ни политика перезапуска, ни цифры памяти,
ни пороги производительности не изменены.

Явно **вне** закрытия и не начаты: installed-source backend (ADR-0074 §5 C),
фиксированный размер `BuildWorkspace` (§5a, §5c), персистентность журнала и
восстановление между загрузками (`RUNTIME_OBSERVABILITY_V1` §9.6), относительное
отношение `<= 8x` (наблюдение по ADR-0068), service manifests, и доставка
способности интерфейса, который модуль не запрашивал (ADR-0078 §6).

Stage 4 не начат.

### Требуют решения Project Architect

**H. Судьба относительного бюджета `8x` — вынесено ADR-0068 (Proposed,
переработан 2026-08-25 в итоговое решение).** Предложено убрать `<=8x` из
Stage 3 conformance budgets — не заменяя другим коэффициентом и не подгоняя
denominator под проходной результат.

Основание — три профиля, и все три измерены. Асимметричный (сегодняшний) даёт
вердикт, решаемый попаданием тика: `47,8 %` на серию, откуда green `7,106x` и
red `8,046x` на побайтно одинаковых артефактах. Active/active невозможен из-за
разной длительности сторон: попадание в *компаратор* давало бы `1,74x`, то есть
измерение вознаграждало бы шум прибора. Inactive/inactive закрыт matrix 5:
доставка прерываний сама по себе меняет IPC-путь на `+16,62`/`+18,45 µs` при
одном и том же бинарнике, и такое отношение мерило бы overhead плюс артефакт
эмулятора.

Что остаётся: абсолютный `p99 <= 200 µs` как conformance budget на настоящем
64-байтном IPC с активным production preemption и с хвостами; fixed-call
benchmark и само отношение — как observational/regression data, ничего не
закрывающие и не блокирующие; счётные бюджеты `IPC_V1` §8 без изменений; серия
латентности — 300 удержанных сэмплов после 3 warm-up (observer qualification с
её 21 парой и sign-test не трогается — это другой эксперимент); quantum и APIC
divider — qualifier-bound идентичность active-preemption reference profile.

Старые green и red P2 остаются историческими записями прежней структуры;
ничего не переименовывается в pass, красный остаётся красным.

`docs/35`, `IPC_V1` и гейт не менялись — ADR ждёт утверждения.

**G. Как супервизор узнаёт, что сервис закончился — ADR-0067, вариант D
архитектурно одобрен 2026-08-25, ADR остаётся Proposed до финального
утверждения после четырёх поправок.** Реализация не начата.

Поправка, которая исправила мою ошибку: я написал, что `rdx` у `process_create`
на возврате свободен. Это неверно — операция 8 уже отдаёт там child capability
handle (`Answer::value(handle)` в `syscall.rs`). Вместе с тем, что старый caller
не обязан инициализировать `r8`, требование трактовать его содержимое как
domain error ломало бы правило minor version. Поэтому **операция 8 не меняется
вовсе**: её дети получают instance id и родительскую связь от ядра, их
restart_generation равен `0` как фиксированное значение legacy-формы, а `rdx`
по-прежнему возвращает handle.

Новая форма — **операция 15 `process_create_with_generation`**: `r8` несёт
supervisor-asserted restart_generation, `rdx` так же возвращает capability
handle, а instance id ядро пишет в фиксированный слот argument region
(`CREATE_INSTANCE_ID`). Новый супервизор использует 15 для каждого запуска —
первый с generation 0, restart с +1; смешивать формы для одной службы нельзя,
иначе первое звено lineage никем не заявлено.

Вторая поправка: `restart_generation` ребёнка добавлен в tombstone и
`WAIT_CHILD_RECORD` явно как **сохранённое утверждение супервизора**, а не
утверждение ядра — ядро повторяет чужое заявление, и запись это называет.

Третья: orphan case уточнён. После смерти родителя capability-scoped получателя
его lifecycle-событий больше нет, поэтому уже накопленные tombstone
освобождаются в момент его смерти, а ребёнок, закончившийся **после** смерти
родителя, сохраняет audit event, но tombstone не удерживает вовсе — слот сразу
переиспользуем. Reparenting не вводится.

Четвёртая: тест на исчерпание исправлен — живой супервизор сам занимает слот,
поэтому заполняются все *оставшиеся*, следующий create даёт `E_LIMIT`, один
`process_wait_child` освобождает ровно один слот и разрешает ровно один новый
create, а следующий снова `E_LIMIT`.

Остальное D без изменений: операция 14 `process_wait_child` с правом
`RIGHT_WAIT_CHILD`, наблюдение только прямых детей именованного process object,
уведомление не является IPC и очередь не трогает, tombstone живёт в слоте,
generation поднимается в момент завершения, число pending записей ограничено
таблицей процессов, `E_LIMIT` — видимая форма того же давления. В ADR отдельно:
threat model, boundedness, семантика slot-reuse/generation и conformance-тесты,
включая двух детей, завершившихся до единственного `wait`.

**F. Что обязан гарантировать локальный preflight и что — CI — ЗАКРЫТО
2026-08-21 решением ADR-0065 (вариант A′).** Семантика паритета, инвентарь,
профили и parity-гейт — в записи журнала выше и в самом ADR. Формулировка ниже
сохранена как запись того, чем это было до решения.


**0a. Конфликт по времени — ЗАКРЫТО ADR-0066, прежнее чтение отменено.**
ADR-0049 ограничивает production time semantics, а не возможность измерить
систему внешним прибором. Обе временные границы остаются Stage 3 requirements;
точное решение и текущий отрицательный observer-result записаны выше. Исходная
формулировка ниже сохранена как историческая запись ошибочного чтения.

**0. Конфликт контрактов: временна́я половина бюджетов IPC.** Найден при работе
над `IPC_V1` §9.7 и выносится, а не решается тихо. `docs/35` (через `IPC_V1` §8)
требует измерить p99 request/reply и сопоставить его с абсолютной границей
200 µs и с относительной «не более 8× внутрипроцессного вызова». **ADR-0049 §6
говорит, что этот этап не имеет часов:** тик — это счётчик прерываний, не
калибруется ни по какому эталону, и `docs/34` относит временны́е угрозы к Stage 7;
число, поданное как секунды, было бы утверждением, которое это ядро не может
подкрепить. Два принятых документа требуют несовместимого: один — измерения
времени, другой — отсутствия времени, которым мерить.

Счётная половина §8 (копии, пересечения, отсутствие аллокации, константность
проверки capability) от этого не зависит и делается отдельно. Временна́я — зависит
целиком. Варианты, как мне видится: (а) мерить на хосте вне QEMU и объявить это
не свойством системы; (б) откалибровать тик против эталона и тем самым внести
время в Stage 3 вопреки ADR-0049 §6; (в) признать относительную границу
неизмеримой на этом этапе и перенести её вместе с обоснованием. Ни один я не
выбираю — это решение уровня 2.

**0b. Расхождение с контрактом — ЗАКРЫТО ADR-0063 (принят вариант A, он же B
этого ADR).** Операция 13 добавлена, реализована и измерена: обмен стоит четыре
пересечения, наклон снят двумя загрузками на форму сервера (см. запись
«Обмен стоит четыре пересечения»). Формулировка ниже сохранена как запись того,
чем это выглядело до решения.

**0b. Расхождение с контрактом: request/reply стоит шесть пересечений, §8
разрешает четыре.** Измерено сбалансированным счётчиком (см. запись выше), не
оценено. Операция IPC стоит два пересечения, обмен — это три операции
(`endpoint_call`, `endpoint_receive`, `endpoint_reply`), итого шесть. Уложиться в
четыре может только операция, отвечающая и снова встающая в ожидание одной парой
пересечений — то есть **новая операция `SYSTEM_ABI_V1`**, а не настройка ядра.
Это решение уровня 2: добавление операции в закрытый ABI. Варианты, как мне
видится: (а) добавить `endpoint_reply_and_receive` и поднять версию ABI;
(б) признать границу §8 недостижимой для этой формы сервера и пересмотреть её
вместе с обоснованием; (в) оставить как зафиксированное расхождение до Stage 4,
где появятся драйверы с собственными требованиями к латентности. Ни один не
выбираю.

**C. Contract gaps ADR-0036…0039 — ЗАКРЫТО 2026-08-11/12; запись ниже
устарела и вводила в заблуждение.** Все четыре приняты Project Architect
(2026-08-11, коммиты `d648db7`, `b3832fe`, `86602e9`) и реализованы:
ADR-0036 — `b16cc6c` (guard-типы, `E1402`, `V2031_SYNC`); ADR-0037 revision 3 —
вместе с `E1215` и `share`; ADR-0038 — принят и реализован (см. запись от
2026-08-11); ADR-0039 revision 3 — `86602e9` (`E1213`). Вектора обоих —
`98533e9`.

**Этот пункт был оставлен в списке «требуют решения» после того, как решения
были приняты, и из-за него ADR-0036/0037/0039 были названы Proposed 2026-08-21.
Ошибка журнала, а не статуса:** заголовок раздела читался как актуальный, хотя
позднейшие записи журнала («ADR-0038 принят и реализован; 0036/0037/0039
пересмотрены», «ADR-0036 и ADR-0040 приняты») говорят обратное. Правило на
будущее: статус ADR берётся из строки `- Status:` самого файла ADR, а не из
этого раздела; раздел — историческая запись.

**Что действительно осталось от этой четвёрки: расхождение ADR-0036 ↔ ADR-0039
revision 3 — вынесено как ADR-0064 и ЗАКРЫТО 2026-08-21** (вариант B, ADR-0039
revision 4). См. две записи журнала от 2026-08-21.

Ниже — исходный текст пункта C, сохранён как запись того, чем это было до
принятия.

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
