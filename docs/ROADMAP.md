# Roadmap v3

Один active task и зависимости вместо календарных обещаний. `planning/tasks.json` — единственные task definitions и acceptance ownership; `progress/STATE.json` — только текущее исполнение. Число задач не является product invariant.

**M0: Контракт и foundation.** См. `roadmap/M0.md`.

**M1: Первый вертикальный slice.** См. `roadmap/M1.md`.

**M2: Config и tools.** См. `roadmap/M2.md`.

**M3: OpenProxy без хардкода моделей.** См. `roadmap/M3.md`.

**M4: DCP и MCP.** См. `roadmap/M4.md`.

**M5: Повседневный workflow.** См. `roadmap/M5.md`.

**M6: Квалификация и handoff.** См. `roadmap/M6.md`.

**M8 additions: per-process hot reload.** [T61](goals/2026-10-14-hot-reload-shared-service.md), approved 2026-10-14, amended 2026-10-15: shared service descoped (D26), HOT01/HOT02/HOT04 and final T30 dependency; paused by owner.

**M8 additions: canonical OpenCode 2 config only.** [T62](goals/2026-10-15-canonical-v2-config.md) (D27), CFG11–CFG13 and final T30 dependency.

**M10: Укрупнённые code slices для agent-only разработки.** См. `roadmap/M10.md`, task T52. Это структурная работа без нового product scope, смены baseline и расширения A01–A13.

## Порядок работы

Утверждённое исключение для первого прохода T52: после явного resume с этой задачей выполнить её до нового функционального роста затрагиваемых T44/T45/T46/T50/T51 owners. Это исторический порядок первого прохода, не текущее состояние задач; actual active/status берутся только из progress/STATE.json, без автоматического start/resume. Порядок pause/start/resume — в `roadmap/M10.md`. Не добавлять зависимости T44 → T52 или T52 → done T44: это не минимальные feature prerequisites и ломает текущий active-state. T30 получает done-dependency T52; завершённые задачи/исторические PASS не переписываются.

Выбирать первый ready task по ID, но при blocked live/network task продолжать независимые offline задачи. Например, отсутствие ключей блокирует T16/T27, но не DCP/MCP fake suite/TUI/soak. Исходные exclusions уточняются dated owner amendments: T57 OAuth, T60 Code Mode и T61 local shared service/hot reload обязательны в своих узких контрактах. General HTTP/remote serve/attach parity и migration/importer не добавляются; planning registration не runtime PASS.

До каждого stage только кратко уточнить ближайший task: affected modules, failing test, done condition. Не переписывать весь roadmap до функций. Новый действительно необходимый task требует rationale, dependency/test mapping и обновления registry, не неограниченный backlog.

Не путать task T30 (честный итоговый отчёт) и product goal READY: отчёт нужен и при внешнем blocker. READY требует всех обязательных A01–A13, включая live evidence. Remote push статус независим от качества бинарника.
