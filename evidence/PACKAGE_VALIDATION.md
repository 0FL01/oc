# Проверка пакета исполнения v3

Дата: 2026-09-20. Объект: документация, примеры, graph и вспомогательные utility configured-workspace редакции. Это НЕ acceptance Rust-продукта и не полный preflight пользовательского host. T00 только начат; A01–A13 не пройдены.

## Выполнено в среде подготовки

`python3 scripts/check_docs.py` — exit 0. Проверены JSON/JSONC/TOML, уникальность и ацикличность графа, exact A01–A13 в goal/final template, равенство acceptance registry и TEST_PLAN, assigned test IDs, обязательные acceptance fields, runner-neutral active surfaces, pinned SHA shape, source reference, примеры enabled/disabled integrations и derived journal views. 31 задача, 82 спецификации acceptance; generic `AGENT_GOAL.txt` не требует конкретного objective transport или platform-specific size limit.

`python3 -m unittest discover -s scripts -p 'test_*.py' -v` — exit 0, 24 теста PASS. Пятнадцать проверяют journal: one active, dependencies, checkpoint/finish/block/resume, evidence requirement, byte limits, symlink/path escape отказ, stale indices, orphan detection, reopen guards. Девять проверяют validator и отказ на испорченных graph/goal/final template/TEST_PLAN/spec inputs, включая runner-neutral contract.

В journal stress-unit создано 40 checkpoints одной задачи: все 40 immutable files сохранены, task index содержит последние 12, NOW/indices остаются ≤8192 UTF-8 bytes. Проверено сохранение handoff последнего finished task, включая ситуацию до Git commit. Это проверка файловой логики, не фактический прогон compaction внешнего runner на host.

`node --check references/openproxy-models.user.mjs` — exit 0.

`node --test scripts/test_discovery_reference.mjs` — exit 0, 15 тестов PASS. Все fetch подменены; настоящих HTTP запросов нет. Проверены model ID без registry, URL/auth shape, context/input clamp, override/source/variant behavior, atomic publication, remote-field filtering, 401, disabled/enabled selection, duplicate last-wins, missing limits, invalid envelope, empty/invalid JSON retry, максимум 4 попытки при 503 и credential URL refusal.

Эти Node tests исполняют форматированную копию пользовательского JS, НЕ Rust discovery. Полный fake-clock 30-second deadline/stream body suite ещё должен быть реализован в Rust (DISC07). Node не требуется будущему `oc` в production.

`SHA256SUMS` регенерируется после финального checkpoint этой редакции и проверяется `sha256sum -c SHA256SUMS`. Контрольные суммы относятся к зафиксированному пакету: последующие изменения документации/журнала закономерно требуют нового manifest.

## Не выполнялось

Не созданы и не собраны crates oc, не выполнялись Rust unit/integration/PTY/soak/configured-workspace acceptance tests, upstream OpenCode/DCP runner, реальные OpenProxy/MCP/image/reasoning проверки или browser smoke. Не завершён фактический host/toolchain/rootless Docker preflight. Не измерены RSS/PSS/latency и не подтверждена стоимость execution campaign. Package validation не является доказательством реализации product behavior.

Проверки configured-workspace источников документарные; scope и новые технические defaults указаны отдельно от source-derived facts в docs/DECISIONS.md и docs/RECON.md. 82 acceptance specifications нельзя представлять как 82 выполненных теста.
