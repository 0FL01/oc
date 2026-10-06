# T57 — native channel runtime qualification (partial AUTH04)

Date: 2026-10-06. Reviewed base `a318fa52f`; this slice changes only the owning
channel test fixture/scenarios and factual docs. No production override or API
was added. Real native Runtime, SQLite, file tools, child workers, compaction and
local WS peers execute together. The private fixture supplies a synthetic captured
native Key/OAuth binding and trusted local authority; it does **not** authenticate
against OpenAI/Codex or prove a rebuilt CLI/TUI/live login.

## Required behavior and primary evidence

`crates/oc-adapters/src/provider/websocket/tests.rs` adds three coarse `auth04_`
runtime scenarios using the existing real-peer fixture and joined peer workers:

1. `auth04_runtime_channels_settle_read_followup_fork_and_partition_opaque_authority`
   runs both captured Key and OAuth markers. Actual `read` is offered and settled
   once; two WS frames perform tool request → function result → final. SQLite has
   one tool operation and two captured request receipts. The earlier done-item
   reasoning ciphertext wins completion re-encryption. Actual fork copies the
   settled receipt, gets its own full-request channel and causes no new tool effect.
   A changed captured account authority opens a fresh full channel and withholds
   old opaque ciphertext while retaining ordinary complete tool pairs. Original
   raw receipts remain byte-identical. Account activation/refresh itself is proved
   by the separate shared-owner binding/preparation tests, not simulated here as
   a second credential owner.
2. `auth04_runtime_channel_recovery_is_counted_and_delivered_failures_do_not_replay`
   observes one affirmative continuation rejection after the settled read. The
   existing finite Runtime retry owner schedules physical attempt two; exactly
   three frame dispatches and one durable retry event occur, with a full recovery
   request and no effect replay. Delivered partial/ambiguous/policy failures have
   one dispatch, failed turns, no scheduled retry and no HTTP fallback. Restricted
   policy remains content-policy with the exact admitted Daybreak public URI,
   without synthetic access/account canaries in durable diagnostics.
3. `auth04_runtime_child_and_summary_channels_keep_actual_actors_and_restricted_outcomes`
   launches an actual native foreground subagent and parent follow-up. Root/child
   have separate channels and actual session headers, with the same captured
   account. A policy-blocked child is durably failed; the parent receives a failed
   tool result with the safe explanation, rather than a fabricated child success.
   The successful path adds a second root boundary, queues/delivers ordinary
   compaction and commits its checkpoint through a no-tools full WS summary request.
   The durable last dispatch lane is `compaction`. Child jobs, channels and peers
   are joined. This is a T57 consumer proof, not completion of T45 or a new native
   remote-compaction capability.

The prior preparation/title binding tests plus the current full adapter suite
retain the two application title paths and all-lane capture guards. These three
new scenarios are not claimed as an end-to-end WS title/CLI/TUI or live campaign.
Cancellation/checkpoint publication has its direct owning channel test; existing
runtime cancellation regressions remain in the current broad gate.

## Current checks

All Cargo commands were sequential, with the approved TMPDIR,
`CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2` and normal stacks:

- `cargo test --locked -p oc-adapters --lib auth04_`: **21 passed / 0 failed**.
- `cargo test --locked -p oc-adapters --lib`: **665 passed / 0 failed / 1 ignored**,
  666 tests, 109.14 s. The unchanged Go opt-in ignore is not a live PASS.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: exit 0, 16.71 s.
- `cargo fmt --all -- --check` and `git diff --check`: exit 0.

Sequential gate log:
`/home/opencode/.cache/opencode-tmp/opencode/t57-runtime-ws-slice.log`;
`T57_RUNTIME_WS_SLICE_GATES_PASS` appears only after all commands returned zero.
No timeout/stack override, baseline rewriting, disabled test, dependency change,
real credential read, paid request or T53 campaign mutation was used.

## Material experiments and boundaries

Initial fixture compile assumptions were corrected against the actual runtime,
fork and child APIs; no production API was widened. A fork copies a settled tool
receipt, not zero receipts; an explicit closed boundary includes the completed
turn. The final successful RAW span legitimately clears transient retry scheduling,
so retry evidence uses the durable event plus exact frame/dispatch counts, not a
fabricated countdown. Ordinary summary validation requires its documented section
template and a compactable closed prefix; the scripted peer now provides that
template and the fixture creates a second real root boundary with explicit local
retention. Production validation/thresholds were not changed.

Shared SQL/HOT/raw/DCP/replay authority and prior Go/custom/HTTP contracts stay
unchanged. There is no new native provider registry, replay engine, store, daemon,
model tool, endpoint escape or generic fixture framework. `.opencode/` is untouched.
T44/VIS45 stays PAUSED; no paired visual or overall READY claim is made.

Next: shared CLI/TUI auth consumers and actual-binary pipe/PTY qualification, then
current final gates and the still-mandatory dedicated owner-operated browser/device
and ordinary OpenAI test-key live campaign. R1–R4 remain in progress until the full
frozen outcomes are qualified; this checked slice is not T57 completion.
