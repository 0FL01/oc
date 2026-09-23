# T44 V07 S08 — existing runtime/security contracts

Base `c1d4f77`, test commit `27582be`. No production defect reproduced on
this HEAD; all checks use fixture-owned temporary roots and fake endpoints.

## Negative effect boundary

New `s08_bash_intent_store_fault_prevents_effect_and_recovers_unknown_turn`
(`crates/oc-adapters/tests/runtime.rs`) injects a SQLite trigger rejecting the
specific bash intent insert. The scripted provider calls `/bin/touch marker`
in the isolated project. Turn acceptance and one provider call are observed;
runtime returns `Storage`, creates no marker and emits no tool-start event.
After dropping the trigger and reopening the DB, no tool operation/card is
present, the accepted user message persists, and the interrupted turn is
explicitly `unknown` with one `turn_unknown` event. It never reports a failed
tool operation for an intent that was not written, nor replays the effect.

## Repeated existing gates (current code SHA)

- `cargo test --locked -p oc-adapters --test patch_audit`: **10 PASS**,
  including no-follow, symlink swap, concurrent preimage and precise partial
  committed outcome; `aud06_intent_failure_prevents_patch`: **PASS**.
- `cargo test --locked -p oc-adapters --test blob_audit`: **11 PASS**,
  including store-row failure, orphan retry and temp cleanup; `--test soak
  crash_injection_recovers_and_restarts -- --exact`: **1 PASS**.
- `cargo test --locked -p oc-adapters --test mcp_stdio`: **12 PASS**,
  one real-server opt-in ignored; includes kill/reap, descendant group,
  cancellation cleanup and bounded/redacted stderr. `--test shell_watchdog
  aud27_pathological_children_are_bounded_under_watchdog -- --exact`: **PASS**.
- `cargo test --locked -p oc-adapters --lib aud11_`: **3 PASS**
  (incomplete/EOF/terminal error); `--lib aud12_cancel_silent_body_and_headers`:
  **PASS**. `--lib mcp_result::tests`: **6 PASS** for safe redaction and typed
  outcomes.
- `cargo test --locked -p oc-adapters --test permissions`: **8 PASS**;
  `--test subagent`: **12 PASS** (child-only authority narrowing, parent
  retention, cancellation); `--test runtime resource_permissions_gate_real_dispatch_before_side_effects -- --exact`:
  **PASS** (denied/ask does not produce side effect; no approval channel).
- New test and `aud06_intent_failure_prevents_patch` through `--test runtime`:
  **PASS**, 1 each. `cargo fmt --all -- --check`, workspace all-target clippy
  with `-D warnings`, serialized `cargo test --locked --workspace --no-fail-fast
  --quiet`, `cargo build --locked`, `git diff --check`: **PASS**, zero workspace
  failures, existing opt-in live tests ignored.

This is a directed negative suite rather than a full security audit. No
paired original/Rust UI comparison is inferred from S08. S07 resource
qualification and VIS01–VIS24 remain open; T44 remains active. The old
untracked `.opencode/` was not read or modified.
