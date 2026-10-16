# T41 — Evidence rebuilt around the binary (F18, AUD35–AUD37)

Status: DONE (offline qualification complete; the paid live run is T27).

## What was wrong

- The only workflow-level evidence was component-level: `e2e_live.rs`
  constructed `Runtime` directly with an invented
  `limit: {context: 1_000_000, output: 100_000}` catalog, and its ignored
  test returned early when credentials were absent — an explicit run
  reported `test result: ok. 1 passed` (`red.txt`).
- No single binary campaign chained the A01–A13 paths, and the binary-level
  suites (pty/pty_t39/configured_workspace/mcp_application) each covered one
  segment with peers that answered success to any text.

## AUD35 — One binary golden workflow

`crates/oc/tests/golden_binary.rs` runs the actual `oc run` process five
times against one isolated HOME/data root and a strict scripted Responses
peer. Every request must match the next expected step (prompt, project rule,
tool definitions, call ids, prior call/output pairs, DCP anchors); an
unexpected request is recorded as a violation and answered with HTTP 500
instead of success. Covered paths:

1. effective config: global config + project A `opencode.jsonc`/`AGENTS.md`;
2. Responses tool loop, two rounds: `apply_patch` (call `g-patch`) then
   `bash cargo test --quiet` (call `g-test`), with the round-2 check
   asserting both call ids and the real `test result: ok` output;
3. MCP: `codex_web__search` through a configured remote server
   (`golden:search`), with the server's own record of the arguments;
4. model-driven compress over a closed anchor range, verified by the
   `"status":"compressed"` output and a durable compression block;
5. process restart on the same session: a new process replays the prior
   turn, including the MCP tool definition;
6. configured workspace switch to project B: B's rule only, no A history,
   plus the cross-location refusal of A's session from B.

Independent observations (never a Rust helper standing in for a step): the
fixture's own `cargo test` run from this process, durable rows
(`history_a=6 ops=4 blocks=1 history_b=2`), MCP records, exit codes,
`workspace-isolation requests=1`, `strict-peer` (0 violations) and
`unexecuted-steps` (0).

Command/exit: `OC_GOLDEN_SUMMARY=evidence/T41/summary.json cargo test -p oc
--test golden_binary -- --nocapture` → exit 0, counts
`attempted 11 / passed 11 / failed 0 / blocked 0 / skipped 0`.

## AUD36 — Evidence cannot pass by skipping required work

- Explicit invocation of the mandatory live harness without credentials is
  now a machine-readable BLOCKED non-success:
  `cargo test -p oc --test live_bounded live_bounded_campaign -- --ignored
  --exact` → exit 101 with
  `{"harness":"live_bounded","status":"blocked","reason":"OC_TEST_MODEL is
  not set","counts":{"blocked":1,...}}` (`live-blocked-summary.json`).
- The pre-repair behaviour (early return reported as `ok`) is captured in
  `red.txt`.
- The component harness (`crates/oc-adapters/tests/e2e_live.rs`) got the same
  treatment: an explicit run without credentials prints the BLOCKED object
  and fails (`test result: FAILED`, exit 101).
- The normal offline suite still does not run the ignored live tests; the
  dry-run branch test below is *not* ignored and is part of the suite.
- Counts and reasons live in the JSON summaries, including `unexecuted`
  (step budget minus attempted) and `blocked` entries with their reasons.

## AUD37 — Bounded live harness ready before the paid run

`crates/oc/tests/live_bounded.rs` runs one campaign through the product
binary with the same runner in both modes:

- dry run (`live_bounded_dry_run_branches`, offline, in the normal suite):
  loopback Responses peer plus an optional loopback MCP server. Two
  campaigns prove the branches: with MCP 5/5 passed; without MCP 4 passed +
  `mcp: blocked ("no MCP server is declared ... blocked, never silently
  skipped")` — the compulsory MCP step can never disappear silently
  (`dry-run-summary.json`).
- live (`live_bounded_campaign`, ignored, credentials-gated): model,
  variant and declared `limit.context`/`limit.output` are read from the real
  configuration (`OC_TEST_CONFIG` or `~/.config/opencode/opencode.json[c]`);
  the campaign fails when the model is not declared instead of inventing
  limits. MCP is compulsory when the configuration declares a server.
- Bounds: per-call watchdog 300 s (the process is killed), whole-campaign
  watchdog 900 s and a hard step budget of 5; the summary records all three.
- Failure kinds are separated: `protocol` (unexpected HTTP/exit shape),
  `model` (the step's own outcome), `harness`, `blocked` — a broken wire is
  never explained away as model variance before the strict golden campaign
  has passed.

## Gates

`cargo test --workspace --locked` → **324 passed / 0 failed / 4 ignored**;
clippy `-D warnings`, fmt check, build, `oc --help`, `progress.py check`,
`check_docs.py`, `git diff --check` all exit 0.

Ignored count moved 3 → 4 deliberately: the mandated binary live campaign
(`live_bounded_campaign`) is credential-gated and cannot run offline; its
branches are covered by the non-ignored dry-run test, and an explicit run
without credentials fails loudly. The three pre-existing ignored harnesses
are unchanged.

## Remaining risk

- The live campaign has not been executed against the paid endpoint (T27).
  Everything except the real model behaviour is exercised offline; the
  campaign reports `blocked` reasons instead of guessing.
- The golden campaign's MCP server is a loopback fake; live MCP transport
  variants stay with T27/T20 evidence.

## Next

T42 (mandatory offline qualification), then T27 (bounded live), then T30.
