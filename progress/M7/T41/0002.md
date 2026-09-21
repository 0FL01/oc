## Result

T41 (F18, AUD35–AUD37) implemented in the working tree:

- `crates/oc/tests/golden_binary.rs`: strict binary golden campaign
  (effective config → two-round tool loop with apply_patch + bash → MCP →
  model compress → process restart → workspace switch A→B + cross-location
  refusal), machine-readable summary with counts and per-step reasons.
- `crates/oc/tests/live_bounded.rs`: one bounded campaign runner for the
  product binary; dry-run branches (with/without MCP) run in the normal
  offline suite, the credential-gated live campaign is ignored and reports a
  machine-readable BLOCKED non-success when invoked without credentials.
- `crates/oc-adapters/tests/e2e_live.rs`: early return replaced by the same
  BLOCKED JSON + failure.
- Model/variant/limits are read from the real configuration; no invented
  context limits remain in the live path.

## Checks

- `OC_GOLDEN_SUMMARY=evidence/T41/summary.json cargo test -p oc --test
  golden_binary -- --nocapture` → exit 0, 11/11 passed, strict peer clean.
- `cargo test -p oc --test live_bounded -- --nocapture` → exit 0, dry run
  5/5 passed; without MCP 4 passed + 1 explicit blocked.
- `cargo test -p oc --test live_bounded live_bounded_campaign -- --ignored
  --exact` → exit 101 with the BLOCKED JSON.
- Pre-repair RED: explicit ignored run of `e2e_live` printed
  `BUILD_READY_LIVE_BLOCKED` and reported `ok. 1 passed` (`red.txt`).
- Workspace: 324 passed / 0 failed / 4 ignored; clippy, fmt, build, help,
  progress check, check_docs, `git diff --check` exit 0.

## Risks

- Ignored count 3 → 4 by design (mandated live campaign); documented in the
  report, branches covered offline.
- Live endpoint behaviour untested until T27; blocked reasons are explicit.

## Next

Commit T41 with evidence, `progress.py finish`, push, then start T42
(mandatory offline qualification).
