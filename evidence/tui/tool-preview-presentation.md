# T44 — bounded Generic/MCP/Shell presentation (2026-10-07)

Status: **FUNCTIONAL_FACET_QUALIFIED; PIXEL/TEMPORAL GATES OPEN; T44 ACTIVE**.
Git base: `f4f9db5342c8a7cb58f323c9f3a30a458775cf0f`.
This receipt accompanies the reviewed consumer/probe slice, not full VIS16,
VIS17, VIS31, R6, V09 or T44 completion. Acceptance, baselines and ignores remain.

## Behavior and owners

- `history.rs` selects validated bounded body facts instead of generated model
  guidance; absent provenance retains recorded legacy data. Structured omitted
  parts/bytes become compact `Preview limited` status, not synthetic answer/card
  rows. Literal marker strings remain payload. Clipped RAW guidance alone does
  not imply a clipped body, including interrupted turns anchored to a user row.
- `tools.rs` gives Generic/MCP the pinned primitive `[key=value, ...]` summary,
  default collapse, bounded available parameters/body and second-click collapse.
  Genuine failed calls open their error detail, as pinned InlineTool does; denied
  remains muted/struck through and can reveal recorded parameters/output.
- Full/indexed rendering and mouse hit testing share header-only Generic ranges.
  Hover changes feedback, not expansion or the editor owner; parameter/body text
  remains selectable. Pending approval and streamed arguments keep their existing
  specialized owner paths. Ordinary Read has no full-file expansion.
- Typed Shell stdout/stderr/exit/signal/timeout/cancel facts override ambiguous
  envelopes. UI projection flags no longer synthesize `[truncated]`; literal
  markers, `(N earlier lines)` and genuine abnormal/unknown outcomes remain.
  Producer/capture loss has separate truthful status and `/cards` details.
- The existing `/cards` owner retains its bounded DTO and exposes body/capture/
  reference facts in actual detail geometry, not only an ellipsized list row.
  References are explicitly not readability guarantees. Its authorized 240-byte
  pages may serve a capture or recorded RAW; the caption says so. Page content,
  byte offsets, escape safety and unseen-row admission are unchanged. A contiguous
  final paint now updates the next-page footer immediately; jumping past unseen
  rows still cannot request a page.

No regex/keyword marker stripping, RAW/provider rewrite, automatic cold loading,
reexecution, new store/schema/service/framework, expanded grants, cap/TTL/quota
change or permanent repaint timer was introduced. Existing T50/R10 owners and
`evidence/tui/tool-presentation-facts.md` retain capture/access/fault qualification.

## Current actual paired attempt012

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 120 --rows 40 \
  --tool-preview true \
  --output /home/opencode/ai/oc/evidence/tui/tool-preview-attempt-012
```

Pinned donor source `2670273ff17da96f85c5826ced57aa1b368754fa`; executable
SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Native locked build/executable association is in immutable `capture.lock.json`
and `source-manifest.json`, including the new untracked-at-capture source files;
native executable SHA-256
`12d488efc75e24a0444b6e98cc7a215f2193022657612553f76c9ecab1665f6e`.
No source-changing edit follows that capture in this slice's delivery.

Both binaries execute two consecutive >5 KiB MCP results, real 80-line Shell
output and a genuine MCP `isError` result through ordinary Responses function
calls. Configured preview is 12 lines/1024 bytes; native presentation remains
independently bounded at 2048 bytes. The real peer's literal strings
`[Part preview truncated]`, `[output preview truncated; full result retained]`
and `[truncated]` survive the available expanded body. Discarded distant-end
bytes do not appear through ordinary expansion. Model-facing guidance remains
separate in native dialogue.

Fourteen paired stages per side: Home, completion, hover, each MCP
expanded/recollapsed, failed detail expanded/recollapsed, Shell
expanded/resize/recollapsed, `/new`, history reopen, clean same-root restart.
Actual draft/caret invariance is checked across hover/click states at 120×80;
the profile then returns to 120×40. Both side reports are `PASS_BEHAVIOR_ONLY`:
exactly **six provider requests, four tool effects and three MCP calls**, unchanged
through view changes/reopen/restart. Native four bounded presentation events,
operation/resource records and both sides' artifact hashes/MCP call records are
identical before/after. Each real Shell appends exactly one17-byte fixed line to
its per-side isolated project counter; unchanged count/bytes/hash after view
changes and restart independently prove no Shell replay, rather than relying on
provider counters alone. The pinned donor's different SQLite schema is not decoded
as native operation records; its no-reexecution proof is the actual call/provider
graph, unchanged artifacts and actual Shell counter. No history import or test
SQL write is used.

Two additional full native-only 120×20 frames show visible typed capture facts
and a genuine bytes240–480 next page after consuming the preceding rows. They are
`NATIVE_ONLY_RESOURCE_DETAILS`, not missing donor counterparts or visual PASS.

**All 28 paired unmasked grid/PNG comparisons are DIFFERENT (runner exit1).**
Complete styled cells, PNGs, cursor, VT, safe protocol/input logs and diffs remain
under `tool-preview-attempt-012/`. Native wire names/safe MCP failure normalization,
bounded presentation/status and original recorded guidance are disclosed facts;
other geometry/style/duration differences are not masked or excused. Settled
cursor equality does not establish absence of transient caret or preserved blink.

## Experiments, failures and verification

Attempts001–011 remain immutable diagnostics, not additional acceptance. Exact
original compound-Shell permission matching was corrected only in the isolated
fixture; native exact command admission remains. Fixture marker prefixes were
made digit-free because existing redaction legitimately treated fixture `1` as a
secret value: redaction was not bypassed. Actual native MCP failure normalization
was observed rather than coerced to donor prose. Tall-card geometry, actual four
events, ordered Escape handling, bounded viewer caption and same-paint footer
were corrected from observed results. Attempt004 was interrupted by the outer
120-second tool timeout; subsequent bounded captures use 600 seconds. Attempts005,
009–011 passed their recorded behavior predicates but qualify only their own
source/probe versions, not the final source.
Final review strengthened the proof with an actual Shell effect counter in012;
Rust executable/source are unchanged from the completed final4 chain.

The first broad gate caught Generic interception of prepared approval geometry;
excluding `permission_pending` restored the existing approval owner. The second
caught obsolete always-expanded S04 error expectations: its actual mouse-open
flow now matches default collapse, while raw VT/control/OSC, Unicode, RAW/wire,
request-count and replay security assertions remain. Review found the interrupted
user-anchor part-state offset and added both user/assistant guidance-only cases.
Final3 caught `clone()` on Copy Role in that test; it was removed, not allowed.
Failed logs remain separate; only the completed final4 chain proves current source.

Serial Cargo, normal stacks, build jobs3/test threads2, approved disk TMPDIR:

- `cargo fmt --all -- --check` and strict locked workspace/all-target Clippy — PASS.
- `cargo test --locked --workspace` — **1728 passed / 0 failed / 11 unchanged
  opt-in ignores**, independently summed from46 result records. Includes TUI456,
  binary97, PTY T3952, MCP41/S04, adapter670/0/1, runtime120, subagents39 and the
  unchanged actual-binary memory/archive/output/resource guards.
- `cargo build --locked`, ordinary `cargo build`, locked release build and both
  `--help` invocations — PASS.
- Current actual release `support/startup.py` and `discovery_startup.py` — PASS:
  safe fatal/refusal/detail channels, trust/storage boundaries, no Responses/one
  discovery GET, oversized/slow 401/403 privacy, terminal restoration.
- Node syntax and Python fixture/bridge syntax checks, operational/doc tests and
  journal/documentation/diff checks — PASS (structure, not pixel qualification).
  Python suites47/0. Staged whitespace check excludes only immutable padded
  `.txt` and raw `.vt` terminal exports from attempts011/012: spaces/CR are captured
  data, not source formatting. Their bytes and full unmasked comparisons are
  retained unchanged; the full whitespace invocation reports them as expected.

Retained log `.local/t44-tool-preview-gates-final4-20261007.log` contains the full
completed Cargo/release-startup chain. No new live generation was used; AUTH06
deferral, original24/24 and Go13/24 ledgers, user configuration and `.opencode/`
are untouched.

## Next contract

Continue the frozen phantom-caret/prompt-blink VIS16/VIS31 output-boundary slice:
safe frame/cursor ordering plus actual blink-enabled idle/continuous hover/
restored temporal proof, at least three full cycles per state and unsupported
synchronized-output fallback. Then qualify every remaining R1–R6/VIS01–VIS45/
SAFETY/V09 outcome on current source. No task finish, native-golden parity or
whole-product READY follows from this receipt.
