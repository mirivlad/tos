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
