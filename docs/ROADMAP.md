# Roadmap v3

Один active task и зависимости вместо календарных обещаний. `planning/tasks.json` — единственные task definitions и acceptance ownership; `progress/STATE.json` — только текущее исполнение. Число задач не является product invariant.

**M0: Контракт и foundation.** См. `roadmap/M0.md`.

**M1: Первый вертикальный slice.** См. `roadmap/M1.md`.

**M2: Config и tools.** См. `roadmap/M2.md`.

**M3: OpenProxy без хардкода моделей.** См. `roadmap/M3.md`.

**M4: DCP и MCP.** См. `roadmap/M4.md`.

**M5: Повседневный workflow.** См. `roadmap/M5.md`.

**M6: Квалификация и handoff.** См. `roadmap/M6.md`.

## Порядок работы

Выбирать первый ready task по ID, но при blocked live/network task продолжать независимые offline задачи. Например, отсутствие ключей блокирует T16/T27, но не DCP/MCP fake suite/TUI/soak. Никакой обязательной реализации serve/attach, OAuth, migration или Code Mode в этом графе нет.

До каждого stage только кратко уточнить ближайший task: affected modules, failing test, done condition. Не переписывать весь roadmap до функций. Новый действительно необходимый task требует rationale, dependency/test mapping и обновления registry, не неограниченный backlog.

Не путать task T30 (честный итоговый отчёт) и product goal READY: отчёт нужен и при внешнем blocker. READY требует всех обязательных A01–A13, включая live evidence. Remote push статус независим от качества бинарника.
