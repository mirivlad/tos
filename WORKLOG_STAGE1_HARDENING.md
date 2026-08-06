<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Worklog — Stage 1 hardening (Priority 1)

Рабочий журнал ветки `claude/stage1-hardening`. **Ненормативный**: не заменяет
CHANGELOG.md, ADR, отчёты по этапам и `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`.

Область работ ограничена Priority 1 аудита: A (документационный CI), B (checked
arithmetic в boot-protocol), C (сверка identity BootInfo ↔ capsule в нуклеусе),
D (удаление отладочного вывода из serial-потока). Бинарный формат capsule v1 и
boot ABI v1 в этой ветке **не меняются**.

## Исходное состояние

- База: `main` @ `38f00c25e5773393b5357298532cffe11a9e4844`
  («Stage 1: raw OID identity (ADR-0016), QEMU boot verified (exit 33), gates»).
- Рабочее дерево на момент старта: чистое (`git status --short` пуст).
- Ветка: `claude/stage1-hardening`, создана от `main`.

### Базовые прогоны гейтов (до правок)

| Гейт | Команда | Результат |
|---|---|---|
| Генерируемая спека | `python3 tools/build-specification.py --check` | **OK** — «generated specification is current» |
| Покрытие ADR в манифесте (CI job, шаг 2) | inline python из `.github/workflows/documentation-integrity.yml` | **FAIL** — `accepted ADRs missing from specification manifest: docs/adr/0016-capsule-git-raw-oid-identity.md` |
| SPDX | `sh scripts/check-spdx.sh` | OK |
| DCO | `sh scripts/check-dco.sh` | OK |
| Тесты воркспейса | `cargo test` (из `source/`) | OK — 39 passed / 0 failed (boot-protocol 13, capsule 11, tos-hash 7, integration 8) |

Вывод: на старте **documentation-integrity CI красный** из-за пункта A; тесты и
остальные гейты зелёные.

## Журнал изменений (append-only)

### A — ADR-0016 в манифесте спецификации

- Статус: **готово**.
- Проблема: `docs/adr/0016-capsule-git-raw-oid-identity.md` (Status: Accepted,
  коммит `38f00c2`) отсутствовал в `docs/SPECIFICATION_SOURCES.txt`. Из-за этого
  падал шаг «Validate source manifest and accepted ADR coverage» workflow
  `documentation-integrity`, а сам ADR не попадал в генерируемую консолидированную
  спецификацию. Требование — docs/30 «Documentation gate»: «source manifest
  contains every accepted ADR».
- Изменения:
  - `docs/SPECIFICATION_SOURCES.txt`: добавлена строка ADR-0016 (после ADR-0015,
    в порядке нумерации);
  - `TOS_DEVELOPMENT_SPECIFICATION.md`: регенерирован `python3
    tools/build-specification.py` (68 источников вместо 67), вручную не
    редактировался — AGENTS.md §2;
  - `SHA256SUMS`: обновлены две записи для изменённых файлов, чтобы манифест
    целостности не деградировал относительно предыдущего коммита.
- Нормативные контракты не затронуты: формат capsule v1, boot ABI v1 и код не
  менялись (изменение уровня «документация», docs/21).
- Проверки:
  - `python3 tools/build-specification.py --check` → «generated specification is current»;
  - inline-шаг покрытия ADR из workflow → `ADR coverage: ok` (был FAIL);
  - `sha256sum -c SHA256SUMS` → 0 несовпадений из 82 записей;
  - `sh scripts/check-spdx.sh` → OK; `sh scripts/check-dco.sh` → OK;
  - `cargo test` (из `source/`) → 39 passed / 0 failed.
- Известные ограничения (вне области A, не регрессия — состояние было таким и до
  ветки):
  - `SHA256SUMS` по-прежнему покрывает только доко-базлайн 0.2.1; 55 отслеживаемых
    файлов (весь `source/`, ADR-0016, `scripts/`, журналы) в него не входят;
  - `MANIFEST.txt` заявляет «15 accepted ADRs», фактически их 16;
  - в CI по-прежнему нет джоба для кода (cargo/clippy/QEMU) — это Priority 3.
- Коммит: `7f5b8b2ea9047e6763d7e22dc66b517d114e9ce2`, запушен в
  `origin/claude/stage1-hardening`.

### B — checked arithmetic в boot-protocol

- Статус: **готово**.
- Проблема: `MemoryRange::end()` складывал `phys_start + phys_length` без
  проверки. Подтверждено PoC до правки:
  - debug: `thread 'main' panicked at crates/boot-protocol/src/lib.rs:68:
    attempt to add with overflow` — валидатор, объявленный «total over arbitrary
    bytes», паниковал (AGENTS.md §8);
  - release: сложение заворачивалось, из-за чего
    `check_memory_map([{0x1000, u64::MAX}, {0x2000, 0x1000}])` возвращал
    **`Ok(())`** на перекрывающейся карте (fail-open против BOOT_ABI_V1 §8 п.6).
    То же в `check_capsule_in_memory`: `d.phys_start + d.phys_length`.
- Изменения (`source/crates/boot-protocol/src/lib.rs`):
  - `MemoryRange::end() -> u64` заменён на `checked_end() -> Option<u64>`;
  - новый вариант ошибки `BootInfoError::MemoryRangeOverflow`;
  - `check_memory_map`: проверка нулевой длины поднята перед вычислением конца,
    оба конца диапазонов вычисляются через `checked_end`;
  - `check_capsule_in_memory`: явный цикл, fail closed на переполняющемся
    дескрипторе вместо сравнения с завёрнутым концом.
- **Нормативные контракты не менялись**: 24-байтовая раскладка дескриптора,
  BootInfo v1 и capsule v1 не тронуты. `MemoryRangeOverflow` — реализация уже
  существующего правила BOOT_ABI_V1 §8 п.6 («region outside addressable
  bounds»), а не новое правило. `checked_end` — внутренний Rust-хелпер крейта
  (`publish = false`), единственные вызовы были внутри самого крейта.
- Тесты (5 новых regression-тестов, специально запускаются в обоих профилях):
  `checked_end_reports_overflow`, `memory_map_wrapping_range_rejected`,
  `wrapping_range_no_longer_hides_overlap`, `plain_overlap_still_reported_as_overlap`
  (гарантирует, что защита от переполнения не подменяет обычный диагноз overlap),
  `capsule_containment_wrapping_descriptor_rejected`.
- Проверки:
  - `cargo test -p tos-boot-protocol` → 18 passed / 0 failed (**debug**, профиль,
    где старый код паниковал);
  - `cargo test --release -p tos-boot-protocol` → 18 passed / 0 failed (профиль,
    где старый код молча заворачивал);
  - `cargo test` (воркспейс) → 44 passed / 0 failed (было 39);
  - `cargo build --release --target x86_64-unknown-uefi` и
    `--target x86_64-unknown-none` → собираются;
  - QEMU: `bash host-tools/qemu-test/run.sh …` → **exit 33 (HALT_OK)**, реальная
    карта памяти OVMF проходит ужесточённый валидатор,
    `TOS.IDENTITY source_kind=git source_digest=7f5b8b2e…` = HEAD;
  - `check-spdx`/`check-dco`/`build-specification.py --check` → OK.
- Известные ограничения: у крейта остаются 2 предупреждения clippy
  (`matches!`, `is_multiple_of`) — они были и до ветки, чистка предупреждений вне
  области Priority 1.
- Коммит: `294477a95478b11f20fc206a966e42fa035ed9fc`, запушен.

### C — сверка identity BootInfo ↔ заголовок capsule в нуклеусе

- Статус: **готово**.
- Проблема: нуклеус печатал `TOS.IDENTITY` из полей `BootInfo`
  (`capsule_identity_kind`, `capsule_source_identity`), сверяя с реальными
  байтами только `capsule_digest`. BOOT_ABI_V1 §6 объявляет эти поля *копией*
  заголовка капсулы, но ничто эту копию не проверяло: запись Stage 1 identity —
  главное доказательство гейта docs/37 — сообщала то, что заявил производитель
  записи, а не то, что несёт сам артефакт. Детач-капсула + подделанный BootInfo
  давали `source_kind=git` с произвольным «commit oid».
- Изменения:
  - `boot-protocol`: новая чистая функция `BootInfo::check_capsule_identity(kind,
    oid_alg, oid_length, value)` и вариант ошибки `CapsuleIdentityMismatch`;
  - `nucleus`: после успешного `parse()` четвёрка сверяется с `cap.header()`;
    при расхождении — `TOS.IDENTITY.MISMATCH bootinfo-vs-capsule-header` и
    fail closed;
  - `nucleus`: `TOS.IDENTITY` теперь печатается из проверенного заголовка
    капсулы, а не из handoff-записи (значения к этому моменту доказано равны).
- **Нормативные контракты не менялись.** Код выхода — существующий
  `RESULT_CAPSULE_INVALID` (0x21), тот же, что уже используется при
  несовпадении `capsule_digest` с байтами капсулы; новый result-код не вводился
  (это было бы изменением boot ABI). Новая serial-строка соответствует шаблону
  BOOT_ABI_V1 §7 `^TOS\.[A-Z0-9_.]+`.
- Тесты (5 новых, negative-ориентированные): `capsule_identity_match_accepted`,
  `..._kind_mismatch_rejected` (git-запись против detached-капсулы — ровно
  сценарий выдуманного provenance), `..._algorithm_mismatch_rejected`,
  `..._oid_length_mismatch_rejected`, `..._value_mismatch_rejected` (флип байта в
  oid и отдельно в нулевом хвосте).
- Проверки:
  - `cargo test -p tos-boot-protocol` → 23 passed / 0 failed (debug и release);
  - `cargo test` (воркспейс) → 49 passed / 0 failed;
  - сборка `x86_64-unknown-none` (нуклеус пересобран, 11 576 B) и
    `x86_64-unknown-uefi` — без предупреждений;
  - QEMU success → **exit 33**, сверка проходит на реальном пути,
    `TOS.IDENTITY source_kind=git source_digest=294477a9…` (= HEAD, значение
    теперь берётся из заголовка капсулы);
  - QEMU negative (`invalid-bootcanon-mismatch.bin`) → **exit 67**,
    `TOS.BOOT.FAILC capsule_err=BootCanonicalFlagMismatch` — fail-closed не
    сломан.
- Известные ограничения: сама ветка расхождения не проверяется в QEMU — для
  этого нужен загрузчик, намеренно пишущий неверные поля (отдельный
  тестовый режим loader'а). Логика покрыта unit-тестами; e2e-негатив на identity
  mismatch остаётся открытым пунктом (кандидат в Priority 3/4).
- Коммит: `43b4a3538a41ad815caa3be5dbdacfcf1bae6fc2`, запушен.

### D — отладочный вывод в serial boot-event log

- Статус: **готово**.
- Проблема: загрузчик писал в serial bring-up-строку `HO nuc=… stk=…` **без
  завершения строки**, из-за чего она склеивалась со следующим событием:
  `HO nuc=0000000002000000 stk=000000000de80000TOS.BOOT.HANDOFF`. Это нарушало
  BOOT_ABI_V1 §7 («Serial lines match `^TOS\.[A-Z0-9_.]+` with structured
  fields») и делало поток неразбираемым для будущего теста, матчащего событийные
  идентификаторы (требование CODEX_START).
- Изменение: адреса сохранены, но как структурированные поля того события,
  которое они описывают:
  `TOS.BOOT.HANDOFF nucleus=0x… stack=0x… bootinfo=0x…`.
  Диагностическая информация не потеряна, а расширена (`bootinfo`).
- **Нормативные контракты не менялись**: идентификатор `TOS.BOOT.HANDOFF` уже
  существовал в реализации; §7 прямо предусматривает поля `key=value`.
  Расхождение перечня идентификаторов §7 с реализацией (`TOS.BOOT.ABI_OK`,
  `TOS.CAPSULE.VALID` и т.д.) — отдельный пункт Priority 2, здесь не трогается.
- Проверки:
  - `cargo build --release --target x86_64-unknown-uefi` → без предупреждений;
  - `cargo test` (воркспейс) → 49 passed / 0 failed;
  - QEMU → **exit 33**, полный поток событий:
    `TOS.BOOT.ENTRY → TOS.CAPSULE.OK files=1 → TOS.BOOT.HANDOFF nucleus=0x…
    stack=0x… bootinfo=0x… → TOS.NUCLEUS.ENTRY → TOS.CAPSULE.OK files=1 →
    TOS.BOOTTEXT.PATH/LINE/DIGEST → TOS.IDENTITY … → TOS.HALT ok=0x10`;
  - фильтр `grep -v '^TOS\.[A-Z0-9_.]*'` по serial.log: ни одной строки,
    записанной нами через serial, не осталось вне формата.
- Известные ограничения: в захваченном логе остаются строки, которые пишем не мы
  через serial-драйвер: сообщения `BdsDxe:` самого OVMF и наша консольная строка
  `TOS boot loader` (пишется в UEFI ConOut, firmware зеркалит её в serial).
  Будущему парсеру лога следует отбирать строки по `^TOS\.`, а не по `^TOS`.

## Итог Priority 1

Все четыре пункта (A, B, C, D) закрыты, каждый — отдельным DCO-коммитом,
опубликованным в `origin/claude/stage1-hardening`. Ни один бинарный формат и ни
один нормативный контракт не изменён. Priority 2 и далее не начинались.

---

# Priority 2 — канонизация capsule v1

База: `main` @ `78f6d3a` (Priority 1 влит fast-forward). Работа продолжается в
ветке `claude/stage1-hardening`.

Классификация по `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md` сделана до правок:
пункты, добавляющие новые условия отбраковки, — Level 2 (design note + ADR);
пункт про выравнивание таблиц в одном из вариантов Level 3 (меняет байты всех
капсул) и требует решения владельца. Поэтому Priority 2 начинается с
Level 0-части, где решения не требуется.

### E — устранение внутренних противоречий CAPSULE_FORMAT_V1.md

- Статус: **готово**. Уровень изменения: **Level 0** (редакторский) — ни байта
  формата, ни строки кода, ни одного условия accept/reject не изменено.
- Устранено:
  1. **§9 п.8 против §4.** §4: арена имён заканчивается ровно на
     `file_table_offset`; §9 п.8 требовал, чтобы она заканчивалась на
     `payload_offset`. Это прямое противоречие внутри одного нормативного
     документа (AGENTS.md §2). Разрешено в пользу §4: так устроены реализация,
     билдер и все golden-векторы, а прочтение §9 не согласуется с самой же
     раскладкой §4 (между ареной и payload лежит таблица файлов).
  2. **§7, фантомное поле `licence_notice_digest`.** Текст утверждал, что
     SHA-256 блока записан в заголовочное поле `licence_notice_digest`, и в
     следующем же предложении сообщал, что такого поля нет. В раскладке §3 его
     никогда не было. Формулировка заменена: поля нет, блок покрыт
     `whole_capsule_digest`, отдельное поле — вопрос будущей версии формата и
     отдельного ADR.
  3. **§7, разделение обязанностей.** Явно записано, что «блок перечисляет SPDX
     всех материалов» — обязанность билдера и она непроверяема парсером (SPDX не
     выводится из байтов капсулы), а парсер v1 проверяет только проверяемое:
     границы, точный хвост, согласованность offset/length, UTF-8.
  4. **§10, список векторов.** Перечислялись файлы, которых не существует
     (`valid-cap.bin`, `invalid-magic.bin`, `invalid-unsorted-path.bin`,
     `invalid-bad-digest.bin`, `invalid-overlap.bin`,
     `invalid-overflow-count.bin`). Заменено фактическим составом из 13 файлов.
     Также исправлено утверждение «each violating exactly one rule»: векторы,
     полученные патчем валидной капсулы, ломают ещё и whole-digest, а целевое
     правило срабатывает раньше по порядку проверок.
- Проверки: `python3 tools/build-specification.py --check` → current (файлы
  `source/interfaces/**` не входят в манифест генерируемой спецификации, поэтому
  регенерация не требуется); `check-spdx` → OK; `cargo test` → 49 passed /
  0 failed (код не затронут, прогон как регрессионная страховка).
- Открытые пункты, вскрытые при разборе §7 (требуют решения владельца, не
  делаются здесь): `system/boot/NOTICES.txt` — свободная проза, а не список SPDX
  по строке; приведение его к букве §7 изменит байты капсулы и все дайджесты.
- `vectors.tsv`, о котором говорит §10, будет создан отдельным пунктом (E2).

### F1 — ADR-0017: канонический layout capsule v1 (решение владельца)

- Статус: **готово**. Уровень: **Level 2** (расширение контракта), ADR принят
  владельцем; реализация — отдельными коммитами F2/F3 (AGENTS.md §5: сначала
  принятый ADR, потом код).
- Решения владельца, зафиксированные в `docs/adr/0017-capsule-v1-canonical-layout.md`:
  1. **Выравнивание — уточняем спеку, padding не вводим.** `ALIGNMENT = 8`
     нормативен для `HEADER_SIZE`/`PATH_ENTRY_SIZE`/`FILE_ENTRY_SIZE` и для
     `path_table_offset == HEADER_SIZE`; `file_table_offset`, `payload_offset`,
     `content_offset` кратности 8 не требуют. Реализация обязана декодировать
     поля побайтно и не рассчитывать на выровненный доступ. Вариант с padding
     арены **отклонён** как несоразмерное Level 3-изменение: он поменял бы байты
     всех капсул, обесценил golden-векторы, `capsule_sha256` и сохранённые
     evidence, а выигрыша при побайтном декодировании нет.
  2. **Щели закрываются без padding**: `path_table_offset == HEADER_SIZE`, имена
     упакованы подряд (`name_offset[0] == 0`, каждое следующее начинается там,
     где кончилось предыдущее, последнее кончается ровно на
     `file_table_offset`). Неописанных байтов в капсуле нет.
  3. **Каноническое отображение индексов**: `path_table_count == file_count` и
     `path_entry[i].file_index == i`. Биекция проверяется одним O(n)-проходом,
     парсер остаётся allocation-free/no_std. Официальный билдер уже пишет именно
     такую раскладку, поэтому корректные капсулы не требуют перестановки данных.
- Совместимость: **байтовая совместимость capsule v1 сохранена**, версия формата
  не поднимается, ни один существующий вектор/дайджест не инвалидирован.
- Изменения: новый `docs/adr/0017-capsule-v1-canonical-layout.md`; поправки в
  `CAPSULE_FORMAT_V1.md` — §2 (значение ALIGNMENT), новый §2.1, §4 (нет
  неописанных байтов, `path_table_offset == HEADER_SIZE`), §4.1 (упакованная
  арена, каноническое отображение вместо общей формулировки биекции), §9
  (уточнено правило 14, добавлены правила 24-26); ADR внесён в
  `docs/SPECIFICATION_SOURCES.txt`, спецификация регенерирована (69 источников),
  обновлены 2 записи `SHA256SUMS`.
- Проверки: `build-specification.py --check` → current; шаг покрытия ADR → ok;
  `sha256sum -c SHA256SUMS` → 0 несовпадений; `check-spdx` → OK; `cargo test` →
  49 passed / 0 failed (код ещё не менялся).
- Явно **не** решено этим ADR: максимальные границы размера капсулы
  (CODEX_START их требует) — остаётся открытым вопросом.
