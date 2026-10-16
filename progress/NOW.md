# NOW — актуальный handoff

State updated: 2026-10-16T00:48:12+00:00
Active: T65

Сверить Git status/diff до выполнения команд.
Task: T65 — Native V2 provider/cache/transport and ordered permission semantics
Spec: docs/goals/2026-10-15-v2-config-semantics.md
Evidence target: evidence/T65/report.md

Owner plan 2026-10-15 pauses T44 until this configuration slice is implemented, verified and normally committed/pushed; subsequent explicit authorization also requires a coherent version bump, crates.io publication using existing Cargo credentials without printing them, and isolated registry install verification before returning to full TUI parity. Configuration oracle: TS V2 at b4f19bfe74a972c2919aa4c9d7445fa61f41b226 (before the upstream fifteen-minute chunk default), separate from frozen TUI2670273. Separately merge provider/model documents then effective compatibility provider defaults→model overrides preserving explicit false across protocols; remove settings.setCacheKey runtime gate, preserve tri-state/protocol cache-key/cache:none/header-affinity/Anthropic policies; keep timeout/headerTimeout/chunkTimeout/transport provider-only and separate HTTP300000ms header/chunk, false-disabled and optional total deadlines, with WS-specific behavior and cancellation. Ordinary permissions use TS ordered overrides (agent defaults→top-level docs→agent-specific docs→session handling), while saved allow cannot bypass final configured deny and authority/sandbox/delegation ceilings remain non-widenable. V1 remains legacy_config before effects; valid unsupported V2 capability and malformed V2 value diagnostics stay distinct, arbitrary package settings are not legacy by name. Shared synthetic V2 fixtures/expected results for covered TS/Rust subset; existing redaction, admission, cancellation, MCP/package/Markdown/authority regressions and affected/full Cargo gates. No real credentials in fixtures/user config/secrets.env changes, V1 migration, plugin host, shared service or full application/built-in-agent parity. Preserve T44 stash1087067b3025322885c6a964579d80cea4931b06 for the explicit return after delivery.

Последний checkpoint этой задачи (проверить актуальность по Git):

# T65 — verified0.2.0 pre-publication checkpoint

## Result

Configuration R1–R6 is qualified. All four packages and six internal requirements
are0.2.0; only four local lock versions changed. This checkpoint is committed
before any actual upload. Public lookups still show all four0.2.0 versions absent.
Implementation commits:2df915e0e,0d773d72e,ea49f2cfe,c5e9c3765,1b48f77fa.

## Checks

Pinned TS/shared79 PASS; adapters1086/0/5existingignored, binary333/0/7existingignored,
fresh normal two-thread workspace1998/0/12existingignored across46 targets PASS.
Fmt, strict workspace/all-target Clippy, debug/release build and actual release
help/version/smoke with empty PATH PASS. Latest extracted four-package workspace
publish dry run PASS, all uploads explicitly aborted; licenses/assets retained.
Full evidence: evidence/T65/report.md and its linked candidate history/logs.

## Risks

R8 actual publication and isolated registry install are still pending; R7 journal
return follows them. No full product/TUI READY claim. Existing locked yanked
dependency warning is unchanged. Use only Cargo's existing credential mechanism;
never print credentials or repeat an unknown publication effect after interruption.
T44 WIP remains stash1087067b3025322885c6a964579d80cea4931b06, not applied.

## Next

After committing/pushing this checkpoint, run the already-verified workspace's
ordinary `cargo publish --workspace --no-verify --locked` once. Reconcile public
versions if interrupted; publish only confirmed missing packages in dependency
order. Verify public0.2.0 metadata and registry-only isolated `cargo install oc2
--version 0.2.0 --locked`, then actual installed help/version/smoke/headless turn
and session persistence. Record receipts, finish T65, start T44 and restore the
parked WIP once after checking Git. No release/tag or user-config replacement.


Ready (до 5): нет
Blocked: T27, T44, T45, T57, T61

Done в журнале не означает READY всего продукта; см. GOAL.md.
