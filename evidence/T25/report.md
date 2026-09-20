# T25 — Offline coding/compression E2E + live harness

Status: PASS (offline 3/3 E2E + 179 workspace). Live: harness reusable, per-phase live evidence partial (wire + tool loop + one full coding fix demonstrated; single fully-green campaign run flaky on gateway/model variance — T27).

## E2E05 Configured workspace A→B — PASS (offline)

- Global + Location `opencode.json` assemble (later wins, permissions most-restrictive), ordered `AGENTS.md` with once-only sentinels, `.opencode` definitions via new `defs.rs` loader: singular-before-plural, inline JSON before Markdown, sorted enumeration, later-source replace with shadowing provenance, path/field/reason diagnostics with sibling survival; flat skill files, symlinks, subagent modes, reserved command ids, shell interpolation refused.
- Skill body absent from projection until the native `skill` call serves it bounded; `publish_skills` pins files between turns (new `Runtime` API).
- Both native aliases classify (`@tarquinen/opencode-dcp` bare/pinned → dcp, `<root>/{plugin,plugins}/openproxy-models.js` → discovery); lookalikes/`@latest`/`.ts`/basename-outside-root → `UnsupportedPlugin`.
- Switch B: global retained, A-local removed, A session `LocationMismatch` on the B runtime, B turn completes; command bodies expand literally, never execute (marker asserted absent); no bash/webfetch calls in the flow.

## E2E01 Seeded coding — PASS (offline)

- `fixtures/e2e-coding` (buggy `add`, protected `version()` API): scripted provider drives read → apply_patch → `<cargo> test` → completion; fixture tests pass; only `src/lib.rs` changes (`Cargo.lock` runner artifact allowed); outside sentinel untouched.
- Toolchain pattern: absolute `cargo` argv + `RUSTC` parent-env extra (scrubbed child `PATH` cannot resolve them; extras pass the scrub).

## E2E03 Compress → restart — PASS (offline)

- New `dcp::project_history`: covered members collapse to `[compressed {id}]` system entries in place, prune mark drops the prefix, nested placeholders expand under guards, unknown ids ignored, history never mutated; wired into `run_turn_inner` (with idempotent DCP migration in `Runtime::new`).
- Restart over the same root reopens the session; recorded request bodies prove the retained fact reaches the model while covered filler leaves outbound context (bytes reduced).

## Product bugs found by this slice (fixed)

- `messages.id` was per-session `m{seq}` under a global UNIQUE → second session broke (`storage.rs`: global sequence now).
- `stream_generation` never sent `model` (live 401-class failure); `request_body` now takes exact model + explicit variant effort only; `stream:true` was missing (server returned plain JSON → `incomplete stream`); empty-event EOF silently `Ok("")` → now `Incomplete`; pre-commit `Incomplete`/`IdleTimeout` retry once (side-effect-free: no tools ran pre-completion).
- `builtin_tool_defs` descriptions now carry the patch format, cwd confinement, argv/timeout semantics (models guessed before).

## Live test credentials (T16/T27)

- Owner-issued test creds live in `.local/live.env` (gitignored, verified absent from git index); documented without values in `docs/PROVIDER_OPENPROXY.md` (deployment, models, env names, no-commit/no-log rules).
- Verified live: `/models` 200 with 39 ids incl. both test models; Responses SSE text roundtrip (`PONG` + reasoning blocks, completed); full turn loop with read/patch/bash executed live; one complete live coding fix (`a + b`, fixture tests green).
- Variance (not product defects): gateway truncates streams under burst (~1 in 4–6 requests; pacing + pre-side-effect continuation turns mitigate; committed truncations never auto-retry by design); small models are inconsistent at the patch step across runs (reads-only runs observed on both models).
- `e2e_live.rs` (ignored): goal-oriented coding (continuation turns until fixed), reopen/next, compress (tail-excluded ranges), restart/resume, optional `codex_web` (skipped without `LUDKA2_MCP_URL`); BLOCKED-and-pass without creds.
- T16 stays blocked: its bounded-campaign discipline (counters/watchdog) is T27's execution job; core text+tool roundtrip demonstrated here.

## Checks

- `cargo fmt --check`, `clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --locked` (179: 4+94+3+15+6+12+16+29, +3 ignored live), `cargo build --locked`, `check_docs.py` — all exit 0.
- New: `defs.rs` (+6 unit), `project_history` (+1 unit), `publish_skills`, `fixtures/e2e-coding`, `tests/e2e_offline.rs` (3/3), `tests/e2e_live.rs` (ignored).

## Scope and limitations

- Inline `<kind>.json` filename is a documented assumption (fixture pins order only).
- Truncated-with-content EOF still returns partial (documented; tightening is T26/T27 material).
- Live single-run green pending calmer gateway/stronger model; no paid bursts chased here.
