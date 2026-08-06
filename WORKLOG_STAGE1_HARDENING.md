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
