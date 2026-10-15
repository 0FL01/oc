# NOW — актуальный handoff

State updated: 2026-10-15T20:52:33+00:00
Active: T65

Сверить Git status/diff до выполнения команд.
Task: T65 — Native V2 provider/cache/transport and ordered permission semantics
Spec: docs/goals/2026-10-15-v2-config-semantics.md
Evidence target: evidence/T65/report.md

Owner plan 2026-10-15 pauses T44 until this configuration slice is implemented, verified and normally committed/pushed; subsequent explicit authorization also requires a coherent version bump, crates.io publication using existing Cargo credentials without printing them, and isolated registry install verification before returning to full TUI parity. Configuration oracle: TS V2 at b4f19bfe74a972c2919aa4c9d7445fa61f41b226 (before the upstream fifteen-minute chunk default), separate from frozen TUI2670273. Separately merge provider/model documents then effective compatibility provider defaults→model overrides preserving explicit false across protocols; remove settings.setCacheKey runtime gate, preserve tri-state/protocol cache-key/cache:none/header-affinity/Anthropic policies; keep timeout/headerTimeout/chunkTimeout/transport provider-only and separate HTTP300000ms header/chunk, false-disabled and optional total deadlines, with WS-specific behavior and cancellation. Ordinary permissions use TS ordered overrides (agent defaults→top-level docs→agent-specific docs→session handling), while saved allow cannot bypass final configured deny and authority/sandbox/delegation ceilings remain non-widenable. V1 remains legacy_config before effects; valid unsupported V2 capability and malformed V2 value diagnostics stay distinct, arbitrary package settings are not legacy by name. Shared synthetic V2 fixtures/expected results for covered TS/Rust subset; existing redaction, admission, cancellation, MCP/package/Markdown/authority regressions and affected/full Cargo gates. No real credentials in fixtures/user config/secrets.env changes, V1 migration, plugin host, shared service or full application/built-in-agent parity. Preserve T44 stash1087067b3025322885c6a964579d80cea4931b06 for the explicit return after delivery.

Ready (до 5): нет
Blocked: T27, T44, T45, T57, T61

Done в журнале не означает READY всего продукта; см. GOAL.md.
