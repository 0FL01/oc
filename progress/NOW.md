# NOW — актуальный handoff

State updated: 2026-09-21T21:24:41+00:00
Active: T43

Сверить Git status/diff до выполнения команд.
Task: T43 — Config compatibility parity и subagent system
Spec: docs/goals/2026-09-21-config-compat-and-subagents.md
Evidence target: evidence/T43/report.md

Закрыть owner-visible config warnings по upstream v2.0.12 (комментарии YAML, glob-map permissions, опциональный skill frontmatter, тихий skip каталогов без SKILL.md, flat skills, command agent/model/subagent/subtask, mode subagent/all), убрать искусственные size-лимиты, затем полностью реализовать subagent system по upstream v2.0.12 (spawn/child sessions/notices/reap/permissions/command routing/DCP allowSubAgents). Коммит и push каждого среза.

Ready (до 5): T30
Blocked: T27

Done в журнале не означает READY всего продукта; см. GOAL.md.
