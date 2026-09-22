## Result

T44 (TUI pixel parity с opencode v2.0.12) — R1/R2/R3 закрыты, R4 закрыт по рендеру
(3 итерации закоммичены и запушены), R5 (интеракции) впереди.

- R1 recon: `evidence/tui/upstream-inventory.md` (тема `opencode` dark, 74 token slots + hue
  scales, ~45 компонентов, layout-регионы, keymap, видимые строки, все с path:line) и
  `evidence/tui/current-gaps.md` (гэпы + тест-инфраструктура).
- Iter 1 (`b75e063`): vendored upstream theme asset + provenance, `theme.rs` (173 palette
  entries/режим, role API, alpha-композитинг), `styled.rs` (Span/Line, байт-идентичные
  строковые конверсии), палитра проверяется generically против JSON.
- Iter 2 (`8112dee`): `layout.rs` + `shell.rs` — tabs rail, main column, prompt box `┃/╹/▀`,
  status row h1, devtools bar, padding 1|2 и breakpoints 44/60/80/120, sticky-bottom,
  word-wrap toasts; golden frames 80x24 и 120x40 + resize/glyph/sticky тесты.
- Iter 3a (`db2f115`): `messages.rs` — user-блок с `┃`/chips, assistant markdown (headings,
  lists, fenced code с syntax-цветами, blockquote, inline code, paddingLeft 3), reasoning
  `Thinking` → `Thought: … · duration`, footer `agent · model · dur · tok/s · interrupted`;
  additive DTO `ReasoningDelta`/`TurnUsage`/`duration_ms` из provider stream.
- Iter 3b (`22ba1b7`): `tools.rs` — inline tool rows, shell-карты (stdout/stderr/exit/
  truncation), apply_patch `# Created`/`← Patched`/`# Deleted` с diff-ханками в upstream
  ролях, subagent-карта из реального `<subagent>` wrapper, состояния pending/running/
  completed/error/cancelled; additive `ToolCallStarted/Finished` после durable writes.

## Checks

`cargo fmt --all --check` clean; `cargo clippy --locked --workspace --all-targets -- -D
warnings` exit 0; `cargo test --locked --workspace --no-fail-fast` → 413 passed / 0 failed /
4 ignored (32 binaries). Ни одно утверждение в тестах не удалено (0 removed asserts в
`crates/oc/tests`). Pre-existing флейки: `pty_t39::aud30` и `pty::aud02_store01` в полном
параллельном прогоне (в изоляции проходят 3/3).

## Risks

- R4 residual: после перезагрузки страницы истории коммитнутые сообщения не несут tool-карты
  (карты живых turns); `agent.color`/display names отсутствуют в DTO; syntax-подсветка — 4
  языка (эвристика) против ~40 tree-sitter грамматик upstream.
- R5 не начат: keymap-таблица, command palette, диалоги (60/88/116 wide, backdrop),
  editor (multi-line/paste/history/completion).
- T45 (остаток subagent system: background/notices/reap, command routing, DCP
  `allowSubAgents`, TUI child rows) остаётся blocked-задачей.

## Next

Итерация 4: keymap из `packages/tui/src/config/keybind.ts:45-299` (leader ctrl+x, palette
ctrl+p, esc×2 interrupt), command palette, диалоги и editor; затем финальная сверка
инвентаря компонентов с upstream и доклад об отклонениях (R6-коммиты продолжаются).
