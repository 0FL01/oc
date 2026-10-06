# NOW — актуальный handoff

State updated: 2026-10-06T07:42:34+00:00
Active: T56

Сверить Git status/diff до выполнения команд.
Task: T56 — Native session-local interactive PTYs для Terminals
Spec: docs/goals/2026-10-01-native-session-terminals.md
Evidence target: evidence/T56/report.md

Owner-approved 2026-10-01 full child-TUI/Subagents/Shell/Terminals scope and explicit Terminals inclusion. R1-R4: real session-local PTY owner and bounded typed inventory/create/input/resize/snapshot/output/exit; actual frontend create/select/hide/show/focus with raw control-byte routing, VT cells/cursor/theme and gap-free replay; safe shutdown/reap/crash identity recovery without command restart. Preserve native trust, source Location/generation, sanitized shell env and no provider/runner credential inheritance. Separate from T50 shell jobs and T45 children; T44/VIS39 owns full paired list/picker/right-pane geometry/colors/keys. No daemon/serve/attach/HTTP parity, JS/WASM plugin host, new model terminal tool/store/framework or whole-task dependency. TERM01 only T56, behavior and visual results separate. Plan todo/NOT_RUN; current T50 and PAUSED T44 unchanged.

Ready (до 5): T57
Blocked: T27, T43, T44, T45

Done в журнале не означает READY всего продукта; см. GOAL.md.
