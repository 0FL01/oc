# NOW — актуальный handoff

State updated: 2026-09-28T08:49:17+00:00
Active: T52

Сверить Git status/diff до выполнения команд.
Task: T52 — Укрупнённые code slices и отдельные тесты для agent-only разработки
Spec: docs/goals/2026-09-28-code-slices.md
Evidence target: evidence/T52/report.md

Структурный проход без изменения поведения: baseline/test inventory; отдельно крупные unit suites; coarse slices TuiState и Runtime; части integration targets runtime/pty_t39; актуальные owner/test maps и warning-only ориентир 5000 физических строк. Не создавать сотни микрофайлов, новые crates/traits или SHA256-манифесты. Сохранить ownership, публичные пути, test discovery и existing parity/resource gates. Окно 400k использовать выборочно, scouts read-only, один mutation owner. R0–R7 в spec. План не снимает PAUSED T44; старт только после явного resume и корректного scheduling handoff.

Ready (до 5): T45, T46, T47, T50, T51
Blocked: T27, T43, T44

Done в журнале не означает READY всего продукта; см. GOAL.md.
