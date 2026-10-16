# T27 / R4 — bounded backend harness repair

Date: 2026-09-22. Base HEAD: `15719e9`.
Scope: `crates/oc/tests/live_bounded.rs` and this additive evidence file.
This is offline harness qualification, not T27/live acceptance. No live/paid
provider, external MCP, commit or push was performed by this slice.
**External live execution is blocked:** the runbook's durable campaign envelope
has no verified enforcement authority connected to this harness. Credentials,
tool permissions and the watchdogs alone cannot unlock it.

## Changes and interfaces

- Offline catalog preflight invokes the existing public `oc_adapters::composition::load` in an
  isolated test subprocess. It uses the product's JSONC/substitution/root ordering,
  enabled-provider policy, native discovery and effective catalog. There is no
  second HTTP discovery implementation. Probe startup is covered by the campaign
  deadline; it does not generate model responses or connect MCP servers. External
  preflight stops at the envelope gate **before** this subprocess is spawned.
- `models::select_model` and `select_variant` validate the exact requested ID and
  variant. Only the first provider/model separator is split; remaining `/` and
  `~` are literal ID characters. Unknown or disabled variants and missing catalogs
  block before generation.
- The CLI currently has no `--config`, model flag or catalog command.
  `OC_TEST_CONFIG` must name an existing standard `opencode.json` or
  `opencode.jsonc`; its parent is routed through `OPENCODE_CONFIG_DIR` to both
  preflight and every actual binary process. This selects the **product root**,
  including JSON then JSONC precedence, definitions, instructions and native DCP
  files. It does not mean “load only this one file” if both standard files exist.
  An arbitrary filename is rejected explicitly rather than silently ignored.
  Without `OC_TEST_CONFIG`, the product uses its normal
  `OPENCODE_CONFIG_DIR` / `XDG_CONFIG_HOME` / `HOME` resolution.
- Both temporary projects contain a model-only `opencode.json` overlay. The
  validated provider/model/variant is persisted with `Db::set_pref` and the product
  `oc_core::queries::PREF_MODEL_SELECTION` constant in the fresh private data root.
  Each `oc run --data-dir … --session …` uses that selection, including restart
  and the second Location. A configured primary agent selecting a different
  provider is an explicit preflight blocker. Central permissions and primary-agent
  policies are not changed by the harness.
- Permission preflight uses the product `RuntimePolicy` / `PermissionRules`,
  including the selected primary agent's intersected constraints and path
  normalization. Required headless resources must resolve to **allow**:
  `read` and `apply_patch` on `src/lib.rs`, `bash` on `cargo test`, and `compress`
  on `*`. With enabled `codex_web`, search requires `codex_web__search` on `*`
  (the product's exact upstream alias `codex_web_search` is also recognized).
  A bare `codex_web` action does not grant search. Missing, ask, denied or mismatched
  resource permissions block before any generation. A repository-only grant is
  not present in the temporary fixture: the harness never copies/grants it.
- Unknown context/output remain `null` model metadata. Request-policy values come
  from `models::budget(selection, 0, provider.options.native_fallback_limits)`.
  The summary separates `model_limits` from `request_policy` and reports
  `uses_local_fallback`; these caps are not claimed as discovered capacities.
- Offline children clear the inherited environment and set isolated HOME/XDG/
  config roots. Only build-tool locations pass through. Live children inherit the
  authorized product environment so `{env:…}` works; relative `{file:…}` secrets
  remain subject to the product's admitted-root/no-follow checks. Config bytes,
  credentials and headers are not copied into the selection overlay or summary.
- MCP verification compares durable `Db::list_tool_ops` records before/after the
  MCP step. At least one new **exact `codex_web__search`** operation must have state `completed`.
  Successful text, old completed operations, failed/unknown/started operations or
  unreadable storage do not qualify. Live verification no longer needs a fake
  server's in-memory call counter. Other servers' searches and other `codex_web`
  tools cannot qualify. The offline server uses this same identity; the prompt and
  scripted search request ask for `response_length: short`. This is not a search
  quota implementation.
- A successful campaign requires all five named steps to pass. Blocked, skipped,
  failed or unexecuted required work cannot pass the summary assertion. Missing
  work is named with a reason in metadata reports.
- Both stdout/stderr pipes are drained nonblockingly with fixed-size buffers and
  bounded work per poll. Only exit/watchdog/reap/I/O flags and byte counters are
  retained. The helper kills its private process group and waits/reaps the direct
  child, including timeout/error paths; no raw live stdout/stderr is summarized.
  The independent fixture `cargo test --offline` check uses the same watchdog.

## Offline evidence

Commands run from the repository root:

```sh
cargo test --locked -p oc --test live_bounded --no-run
cargo test --locked -p oc --test live_bounded live_bounded_dry_run_branches -- --exact --nocapture
rustfmt --edition 2024 --check crates/oc/tests/live_bounded.rs
cargo clippy --locked -p oc --test live_bounded -- -D warnings
cargo test --locked -p oc --test live_bounded
git diff --check -- crates/oc/tests/live_bounded.rs evidence/T27/backend-harness.md
```

The full targeted suite passed: **9 passed, 0 failed, 2 ignored** (3.45 s in the
recorded verification). The ignores are the real live campaign and an internal
catalog-probe entrypoint; offline tests explicitly execute the latter in isolated
subprocesses. Clippy initially rejected two unsafe `fcntl` operations in one block;
split documented blocks fixed it, and the rerun passed with `-D warnings`.
The new negative permission regression was first run against the old preflight:
it failed on `missing bash` (exit 101), proving the admission gap before the fix.

Coverage:

1. Static campaign: 5/5 with MCP; missing MCP: 4 passed, 1 blocked, aggregate
   non-success (expected negative assertion).
2. Discovery-only `ludka2/org/discovered`, no static entry: product discovery,
   actual binary requests, all five campaign steps, restart and Location switch.
   The fixture MCP handle is removed while the loopback server remains alive,
   proving that the durable verifier is sufficient for the live-shaped case.
3. Literal `fixture/org/future~model`, explicit custom variant, intentionally
   conflicting global default, unknown limits: actual requests carry the exact
   model/reasoning selection and the configured local output cap.
4. Missing static/discovered catalog, absent exact model, unknown/disabled variant,
   missing substituted credential, missing config path and malformed selection:
   non-success before any Responses generation.
5. Successful plain text and stale/failed/interrupted/inflight tool operations
   cannot qualify as new completed MCP work.
6. Blocked/failed/skipped/unexecuted summary states fail the required-step gate.
7. Output larger than pipe capacity is drained; private output markers do not
   appear in detail; hung subprocess and inherited child pipes are bounded/reaped.
8. Missing bash, denied patch/compress, ask/read-path mismatch, bare MCP server
   grants, ask/search and primary-agent deny all fail preflight with zero generation
   requests and zero MCP calls. Exact resource grants plus the upstream search
   action alias pass the actual five-step offline campaign.
9. A live-shaped fixture aimed only at the retained loopback test server is blocked
   at both preflight and product-step launch: no catalog subprocess/report,
   discovery request, generation request or MCP call. No real live test was invoked.

## Envelope limitation and fail-closed boundary

`docs/AGENT_RUNBOOK.md:39` requires at most **24 generation HTTP requests including
retries**, **4 short MCP searches**, and output at most **2048 tokens for smoke /
8192 for coding**, with counters preserved across restart. Inspection found:

- `provider::MAX_ATTEMPTS = 2` bounds retries for one generation only.
- `runtime::MAX_ROUNDS = 8` bounds one turn, not the whole campaign. Title and
  configured subagent generation can also consume requests.
- `application` supplies `max_output: 0`; `models::budget` resolves local request
  policy (default output 4096), not the smoke/coding envelope or durable counters.
- No verifiable runner/proxy quota interface is integrated into the current CLI or
  harness. External quota state was not inspected or changed; its presence is
  unverified, not asserted absent.

`Fixture::verify_external_envelope` therefore refuses every external fixture
before catalog preflight and again before product launch. Only an owned loopback
peer created by offline test code is exempt. There is **no environment approval
flag**, credential-based bypass, claimed hard cap or automatic campaign reset.
Reports explicitly set `live_envelope_verified: false`. Wiring an actual approved
authority that accounts for retries/title/subagents/search and persists counters
is an engineering prerequisite before the explicit blocker can be replaced.

## Exact invocation (currently returns BLOCKED)

The retained opt-in command is below for the parent handoff. **With this patch it
returns BLOCKED/non-success before external discovery, MCP or generation**, even
with valid credentials and sufficient permissions. Do not treat it as budget-ready:

```sh
OC_TEST_CONFIG='/absolute/approved/root/opencode.jsonc' \
OC_TEST_MODEL='provider/exact/model-id' \
OC_TEST_VARIANT='' \
OC_LIVE_SUMMARY='/absolute/private/existing-directory/t27-bounded.json' \
cargo test --locked -p oc --test live_bounded live_bounded_campaign -- \
  --exact --ignored --nocapture --test-threads=1
```

Replace the model with an exact effective-catalog ID and the empty variant with
an explicit available variant only when intended. Empty/unset variant means no
variant overlay. Credentials are read by the product from the approved config
and its substitutions/inherited environment; no credential values belong in this
command or evidence. After an enforceable envelope is integrated, enabled
`codex_web` advertising `search` and the exact headless permissions listed above
remain prerequisites. Disabled servers are not enabled by the harness. The offline
no-MCP branch deliberately executes the other steps to test a blocked aggregate;
it is not permission to perform a partial paid campaign.

Bounds remain **5 campaign steps, 300 seconds per subprocess, 900 seconds total**;
each subprocess receives the lesser of 300 seconds and the remaining campaign
time. Preflight counts against that total. Five steps are not a five-HTTP-request
cap: tool-loop rounds, adapter retries and product title generation can make
additional generation requests within those bounds. Native discovery is separate
read-only HTTP activity. The campaign does not retry a failed step. This is only
the same **five-step smoke**, not full T27 qualification: no claims for other
models, webfetch, browser smoke or mandatory live acceptance.
