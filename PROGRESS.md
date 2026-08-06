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
| 6 | crates/capsule: парсер (no_std, тотальный) | done | 10/10 lib-тестов, фаззинг 300k раундов без паники |
| 7 | crates/capsule: host-билдер (feature="host") | done | детерминизм: builder == golden vector |
| 8 | crates/tos-serial (16550 COM1, no_std) | done | используется loader + nucleus |
| 9 | boot/uefi-loader (EFI app, рукописные биндинги) | код написан | НЕ собран: нет таргета x86_64-unknown-uefi |
| 10 | nucleus (freestanding, boot ABI v1, serial, halt-код) | код написан | НЕ собран: нет таргета x86_64-unknown-none |
| 11 | system/boot/init.tos + NOTICES.txt | done | source/system/boot/ |
| 12 | host-tools/capsule (CLI-билдер) | done | регенерация векторов через него |
| 13 | Golden-векторы (7 .bin, коммитимые) | done | source/tests/vectors/capsule-v1/ |
| 14 | tests/integration | done | 8/8 (golden, tamper, determinism, truncation, perf) |
| 15 | tests/fuzz (детерминированный мутационный) | done | FUZZ PASS rounds=300000 |
| 16 | Сборка всех таргетов (host + uefi + none) | частично | host собирается; uefi/none — rustup target add |
| 17 | host-tools/qemu-test (ESP-образ + OVMF) | pending | — |
| 18 | QEMU-прогоны: success + corrupted capsule | pending | — |
| 19 | scripts/check-spdx, check-dco | pending | — |
| 20 | Архитектур-импакт-стейтмент (AGENTS.md §5, Level 2) | done | source/ARCHITECTURE_IMPACT_STATEMENT.md, коммит dc16726 |
| 21 | Stage 1 отчёт + identity record | pending | — |
| 22 | Коммит source/ + PROGRESS.md (DCO) | done | коммиты 8435698..1bc8c16 + 3226077, dc16726 |

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

## Открытые вопросы / риски

- Сборка uefi/none-таргетов требует `rustup target add x86_64-unknown-uefi x86_64-unknown-none`
  (через прокси, см. память окружения).
- OVMF: нужен пакет/файл OVMF.fd для QEMU (проверить наличие на хосте).
- Подпись DCO: `mirivlad <mirvtop@yandex.ru>` (совпадает с git config и базовым
  коммитом c5b818c; вопрос про mir@yandex.ru закрыт — в историю не вносим).
