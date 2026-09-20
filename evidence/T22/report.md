# T22 — Ежедневный TUI

Status: PASS (offline TestBackend + MockProvider + tempdir Db). Live model data: not required here.

## UI02 Model picker — PASS

- `oc-tui/picker.rs`: exact-id selection over a dynamic `ModelCatalog` via `models::select_model/select_variant`; bounded browse window (10); refresh re-resolves the persisted `tui.model_selection` pref.
- Retired model is actionable: persisted id missing from the refreshed catalog → `Retired{wanted, available}`, choice kept empty, no silent fallback; re-pick recovers explicitly. Unknown ids and disabled variants surface visible errors.
- Choice persists across picker instances (Db roundtrip asserted).

## UI03 History/diff — PASS

- Storage: `history_len`, newest-first `read_history_page` (clamped to 100), `list_tool_ops` (capped 200), `tui.*` prefs kv; unknown sessions fail, never render empty.
- `oc-tui/history.rs`: `HistoryPager` loads older pages on demand (oldest-first render order, `exhausted` tracking); `tool_cards` pair intent/outcome with 512-char previews; `apply_patch` cards list parsed affected paths (up to 5 + truncation flag), parse failures show no files.
- App: session switch clears view state, `resume_session` + `history_older` refill from the pager; slash `/sessions` panel navigates and switches.

## UI06 Workspace definitions — PASS

- `oc-tui/commands.rs`: exact built-in table (`/quit /model /sessions /skills /help`), bounded parsing, prefix completion; unknown input falls back to help, never a silent no-op.
- `oc-tui/workspace.rs`: single-generation registry (agents + real `SkillMeta` cards without bodies + `explain_redacted` provenance view); primary selection persists per generation; stale generations, vanished agents, and foreign Locations fail visibly (`require_primary` blocks the next turn).
- App: `/skills` requires a wired registry; panels are one-at-a-time bounded view state; Esc closes.
- Boundary D12 recorded in `docs/DECISIONS.md`: `oc-tui` depends on `oc-adapters` read-side domain only; DAG `core <- adapters <- tui <- oc` preserved, no network/process spawn from TUI, Db lifecycle stays in the binary.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- Workspace 152 total (oc 4 + adapters 87 + mcp_remote 15 + mcp_stdio 6 + core 16 + tui 24); `cargo build --locked`, `check_docs.py` exit 0.

## Scope and limitations

- Agent/command *definitions* load with the configured workspace (T25/A13); T22 proves registry mechanics with caller-supplied entries. Full D10 `ConfigGenerationId` plumbing and Location-scoped sessions arrive with the runtime (T24/T25); the registry takes a loader-assigned monotonic generation.
- DCP panel/commands arrive in T23; PTY/resize/terminal-restore qualification in T26.
