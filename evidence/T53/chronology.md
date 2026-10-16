# T53 — durable chronological effort and declared system capabilities

Code base: `d88261a0d`; concurrent docs-only T57 plan landed as `6e7fa2ed4`
during this slice. Partial R3/GO03, not full T53 PASS.

## Implemented contract

- Committed model/variant selection stores the previous and new effective effort
  as structured metadata in the existing atomic `session_model_selected` event.
  No-op selection emits no new fact; preference/event failure rolls back together.
- `storage_effort.rs` reads durable ordered facts and next-message boundaries;
  `runtime/context.rs` inserts them only at retained positions without changing
  public/raw messages. Busy primary requests capture new facts in TurnLog input
  at the next prepared request, not the already-issued request. Event identities
  prevent duplicate replay after a model-switch history rebuild.
- `InputItem::EffortUpdate` has a strict durable codec. Lowering never parses
  natural-language text. DCP/content accounting does not treat this metadata as
  generated/public text; existing ordinary result and tool graph owners remain.
- Explicit `compatibility.supportsEffortUpdates` enables Responses and Messages
  per-position updates. Chat always strips markers. Unsupported/mismatching final
  effort strips markers and uses the captured current effort; otherwise the first
  `previous` freezes the initial effort. Consecutive updates coalesce and Default
  resets send `medium`, including absent initial top-level effort.
- Messages emits empty-content system/output_config updates and adds
  `mid-conversation-output-config-2026-07-01` only when actually emitted, preserving
  the always-required interleaved-thinking beta and configured/profile beta union.
- Messages text updates use native system only with explicit
  `compatibility.supportsNativeSystemUpdates` and valid placement; otherwise they
  remain escaped lower-authority user updates. Updates may not split outstanding
  local tool calls/results. This explicit fork capability deliberately replaces
  donor model-name inference; it does not imply undocumented model support.

## Checks and diagnosis

Approved disk TMPDIR, Cargo jobs 3, test threads 2:

```text
cargo test --locked -p oc-adapters --lib                         602/0/0 before final added regression
cargo test --locked -p oc-adapters --test runtime --test subagent 120/0 + 38/0
cargo test --locked --workspace                                  1624/0/10
cargo clippy --locked --workspace --all-targets -- -D warnings    PASS
cargo fmt --all -- --check                                       PASS
cargo build --locked                                            PASS
target/debug/oc --help                                           PASS
git diff --check                                                PASS
```

The final workspace includes 603 adapter unit tests, actual-binary PTY suites,
runtime/retry/child/fork/DCP/recovery suites. The ten existing opt-in live/internal
ignores are unchanged and are not live PASS. No paid/live generation was attempted.

New tests cover twelve real fake-server requests across the three protocols,
supported/unsupported/mismatch/default effort, conditional beta and native text
placement; strict marker codec, pending-tool rejection, actual busy switches in
allow/ask modes, idle boundary/reopen and atomic/no-op acknowledgements.

The first busy-switch assertion found duplicate low updates from the event and
the current TurnLog during history reconstruction. The projection now deduplicates
by durable event identity; the original assertion and tool-effects tests pass.
The first workspace run exposed a reproducible default-thread-stack overflow in
the real permission controller test. A bounded GDB backtrace located nested
`apply_outcome -> apply_intent_with_origin -> refresh_approvals` polling. The
production input controller now boxes that large intent future at the existing
owner boundary. No test stack/timeout or expectation was enlarged; the unchanged
test and final full workspace pass. Temporary probes were removed.

## Remaining qualification

GO04 still requires full protocol/API model/deployment/auth-scope provenance,
legacy/unknown decoding and checkpoint/SQL/DCP/fork compatibility qualification,
including effort facts at fork/compaction boundaries. GO05 configless connect and
provider-qualified selection, then final bounded Go live, remain open. No T53
finish or all-GO PASS is claimed by this slice.
