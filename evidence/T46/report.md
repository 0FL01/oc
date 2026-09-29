# T46 — complete assigned MCP backend qualification (2026-09-29)

## Result

**T46 R1–R7 and mandatory backend follow-up PASS**, based on reviewed delivered
implementation commits and fresh actual offline/live checks. Implementation chain:
`4d39bbfc2` (config/admission), `5f5d4a347` (independent lifecycle/controls),
`b582dd367` (lookups), `e739cfbf3` (media), `2d34e9b87` (late warnings/envelope),
`1ce4cda09` (safe receipts), `bbeb28ee2` (live qualification/receipt fixture).
Earlier `3ce5e03`/`b7ab06d` degradation/User-Agent work is preserved and freshly
regressed, not inherited as a new full-live claim. No whole T44/V09/READY claim.

| Assigned outcome | Current evidence / result |
| --- | --- |
| R1 per-server degradation; fatal cancel/cleanup/caps | `lifecycle.md`, `late-warnings.md`, current full workspace: PASS |
| R2 visible scoped safe warnings, including late failure | actual headless R4 warning, native lifecycle and fresh/existing turn barrier: PASS |
| R3 outbound User-Agent | current provider/discovery/webfetch/HTTP MCP regressions and actual CRW initialization/catalog: PASS |
| R4 required owner-live | `live-qualified.md`: native completed turn, actual CRW/codex schemas, completed short search and one unavailable warning: PASS |
| R5 MCP08 real typed scoped status/control | `lifecycle.md`: actions, coalescing, leases, safe request boundary, reload/Location/restart/reaping: PASS |
| R6 MCP09 normalization/cwd/env/deadlines/trust | `config-admission.md`: both forms, per-entry isolation, disabled zero-spawn, actual process effects and orphan-secret regression: PASS |
| R7 MCP10 independent initial startup | `lifecycle.md`: pre-prompt healthy/held/failed peers, no launch barrier, fatal caps/late completion/owned cleanup: PASS |
| Structured/resource/link results and safe guidance | `backend-parity.md` + current transport/runtime tests: PASS |
| Explicit bounded prompts/resources/templates/read | `lookups.md`: existing owner/client leases, exact admission, pages/body/identity bounds, cancellation/quarantine: PASS |
| Ordered native media result bridge | `media.md`: real bytes, call IDs, durable native attachment, restart/DCP/compaction, no URI fetch/replay: PASS |

## Checks

Fresh serial actual-host commands: fmt check; locked workspace/all-target Clippy
`-D warnings`; `cargo test --workspace --locked --no-fail-fast`; locked debug and
release workspace builds; both `oc --help`. **1263 passed / 0 failed / 10 opt-in
ignored**. Old nine ignores plus the separately opt-in R4 runner remain; no failing
test was disabled. Full Rust/build log `tool_0ec27401a001OAxiyXPsdjReKY` (runner
tool-output directory). Actual final MCP application target 41/0; runtime 94/0;
adapters lib 333/0; TUI 417/0; core 26/0. Jobs 3 / threads 1, existing owned TMPDIR.

The post-Rust Python receipt test exposed an asynchronous acknowledgement race,
not a dispatcher failure. `bbeb28ee2` changes Python fixture only: close streamed
response and wait within its existing three-second fixture budget for the actual
fsynced uncertain receipt, then assert its exact response-limit category/stage.
The earlier unchanged Rust gate remains current. Final Python: envelope 13/0,
code-size 5/0, progress 15/0, docs 14/0; docs/progress/diff checks PASS. Final direct
normal-binary envelope 6/0 and native lifecycle 4/0; hashes unchanged before/after:

- debug `0b263d57efd089bdc7ff4cde23afd810a5ba01b3b5b3a806d2e89dfd589fd56f`
- release `b0c394701711ebb5efe37f6afcd03cdf12bb9461f2da71b8cb5dc881417479b3`

**Actual live PASS**: strict rebuilt public native run, exit 0, watchdog false,
reaped, pipe I/O OK; actual both catalogs in provider tool schemas before/after
one completed short codex search and exactly one native unavailable warning.
One unchanged inode-bound campaign totals **8 generation / 1 search / 19 control**,
74,733 input bytes, including all failed attempts and title work; ceilings 24/4,
<=2048 output and original body/worker/SSRF/deadline bounds enforced. Same nonce
`ceb2142492354cfc8d479bf6bfa4e864`. Source facts and selector correction are in
`live-qualified.md`; old failures remain in `live-attempt-01.md` and
`live-attempt-02-diagnosis.md`. No reset/refund/legacy backfill/secret/raw-response
claim. Exact published wire ID stayed intact under an explicit fixture namespace.

## Risks

Native no-OAuth/direct CodeMode/protocol boundaries remain as approved; recognized
unsupported entries fail truthfully without app-wide rejection. Ordinary shell
minimal environment is unchanged; local MCP inherits only after source/resource/
credential-domain admission. Runtime toggles do not rewrite config or persist fake
connected state. Fatal caps, unknown-effect quarantine and cleanup/cancellation
remain non-success. Browser stays disabled; this is not a real-browser claim.

Full T44 VIS19/VIS40/VIS42 paired presentation, T51 plugin/provider isolation,
T45/T50 functionality, A09 coding and whole product FINAL remain independent
open owners/gates. Current R4 proves a safe declared negative fixture, not a
remote service outage or direct unguarded access. Remaining campaign allowance
is 16 generation / 3 searches; continuation must reuse that journal and identity.

## Next

Close assigned T46 through the sole progress owner, then continue ready T51 and
remaining T44/backend/product work. No final partial stop or product READY claim.

---

# Historical initial T46 attach/User-Agent report (unchanged)

Task: T46 (phase M8). Spec: `docs/goals/2026-09-22-mcp-attach-parity.md`.
Decision: `docs/DECISIONS.md` D13. Upstream reference: anomalyco/opencode `v2.0.12`
(commit `2670273ff17da96f85c5826ced57aa1b368754fa`).

## Result

**Slice A — MCP attach failure degrades one server instead of aborting the turn.**

- `crates/oc-adapters/src/runtime.rs`: `McpGeneration` carries `degraded: Vec<RuntimeError>`
  (`McpAttach` notices only) and exposes `warnings()`; `attach_mcp` records a per-server
  failure and continues, while `Cancelled`, cleanup/`McpShutdown`, `MAX_MCP_SERVERS` and
  generation catalog caps stay fatal; `refresh_if_changed` keeps the previous catalog on a
  failed relist and clears the notice when the server recovers; `TurnReport.warnings`
  carries the sanitized notices (`mcp <id> <stage>: <code> (retryable=<bool>)`).
- `crates/oc-adapters/src/application.rs`: terminal `CoreEvent::TurnFinished`/`TurnFailed`
  carry the turn's warnings.
- `crates/oc-core/src/core_app.rs`: both variants gained `warnings: Vec<String>`.
- `crates/oc/src/tui_cmd.rs`: degradation is rendered as a synthetic transcript row
  (`(warning: …)`) after the durable page attach, so it stays visible; `crates/oc-tui/src/app.rs`
  gained `push_warning` (same synthetic-row mechanism as `(error: …)`).
- `crates/oc/src/headless.rs`: prints `warning: …` on stderr for both terminal events.
- Contract rewrites (deliberate, per D13 — not gate weakening):
  `mcp_attach_failure_is_loud` → `mcp_attach_failure_degrades_the_server`;
  `aud23_partial_attach_failure_reaps_previously_connected_child` →
  `aud23_partial_attach_failure_degrades_and_still_reaps_child` (AUD23 reaping still asserted
  via `shutdown_mcp`); soak degraded-server block; binary scenarios
  `v01_anonymous_remote_and_codex_required_auth`, `v01_required_remote_diagnostic_…` →
  `v01_degraded_remote_diagnostic_is_staged_and_redacted_in_tui`, `aud22_…`,
  `aud24_…` now assert: run answers, warning visible, no MCP/partial catalog reaches the
  model, no secret leakage. Upstream parity: `packages/core/src/mcp/index.ts` keeps per-server
  `failed`/`needs_auth` status and continues; no upstream failure kind aborts a turn.

**Slice B — outbound `User-Agent` (root cause of the crw Cloudflare 403).**

- `crates/oc-adapters/src/lib.rs`: `USER_AGENT = oc/<version>` and `WEB_USER_AGENT = oc-user/1.0`
  (upstream pattern `OpenCode-User/1.0`; JS runtimes always send a UA, `reqwest` sends none).
- Applied to the remote MCP client, provider generation client, discovery headers
  (reserved like accept/auth) and webfetch.
- Tests assert UA presence on each client: provider, discovery, webfetch unit tests and
  `tests/mcp_remote.rs`.

**Live check (real crw endpoint, no secrets printed).** Isolated config with the real
`mcp.crw` entry (templates resolved from the process env) + a local logging provider;
harness `/home/opencode/.cache/opencode-tmp/crw-live-check.py`:
`provider UA: oc/0.1.0`, `crw tools in provider request: 9`
(`crw__crw_crawl`, `crw__crw_extract`, `crw__crw_check_crawl_status`, …),
`invalid_config present: False`. The prior `mcp crw config: invalid_config` abort and the
recorded T27 conclusion “Cloudflare 1010 = browser-signature block, non-browser clients
cannot attach” are both refuted: 1010 is answered to UA-less requests; with any honest UA
the endpoint returns a valid MCP `initialize` (probe: `undici`, `rmcp/0.1`, browser UA → 200;
default `Python-urllib` UA → 403). T27 evidence leaves were not rewritten.

## Checks

- `cargo test --locked --workspace --no-fail-fast` → all binaries ok, 0 failed
  (includes 155 `oc-adapters` lib tests, 35 runtime, 4 soak, 12 mcp_application, 106 oc-tui).
- `cargo clippy --locked --workspace --all-targets -- -D warnings` → exit 0.
- `cargo fmt --all` → clean; `git diff --check` clean.
- Targeted: `mcp_attach_failure_degrades_the_server`,
  `aud23_partial_attach_failure_degrades_and_still_reaps_child`, soak, `mcp_application`
  (12 passed), `oc-adapters --lib` (155 passed), `--test mcp_remote` (23 passed).
- Journal: `scripts/progress.py reindex`/`check` → OK. `scripts/check_docs.py` still reports
  the pre-existing stale acceptance ids in T43/T44/T45 (`A02`/`A03`/`A05` are not in
  `planning/acceptance.json`); unchanged by this task.

## Risks

- Degradation is reported once per generation state: a server that stays broken warns on
  every turn of that generation (no persistent status panel yet), and a repaired server's
  notice clears only on a successful relist.
- `codex_web` is now degradable like any server: A06's live requirement (“codex_web attaches
  and serves search”) is asserted by the live campaign, no longer by refusing the turn.
- No `oc mcp list` status surface (upstream CLI parity) and no `{file:}` trim parity
  (upstream `variable.ts:79` trims; native keeps bytes verbatim) — both remain open.
- Live provider answer with crw enabled was not re-run (T27 campaign scope); only MCP attach
  and tool publication were verified.

## Next

1. Optional slice C: trim `{file:}` content to upstream semantics (`config.rs`
   `read_trusted_file`) + test.
2. Optional: `oc mcp list` / persistent status surface for degraded servers.
3. T27 live campaign re-run with `crw` enabled (codex_web mandatory) to close A06 end to end.
