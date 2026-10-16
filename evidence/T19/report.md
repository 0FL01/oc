# T19 — Nudges и auto-pruning

Status: PASS. Implementation commit: `0524b83ee22c839e943ea6d341c01e250cd19ec7`. Method: offline `cargo` unit execution with synthetic counters and JSON fragments; no network, no live credentials, no Docker, no JS hooks.

## DCP05 Nudges — PASS

- Threshold/percent-ready effective limits with per-model overrides (runtime keys, never hardcoded); summary-buffer/force/frequency knobs; cadence re-fire while over threshold, silence below; `on_compress_success` recalculation; manual mode silences autonomous nudges; emissions are transient hints, never accumulated messages (counter-asserted).

## DCP06 Prune timing — PASS

- `dedup_calls` keeps the last output per `(tool, canonical args)` with protected tools fully exempt; `purge_errors` replaces only old large errored inputs, retaining outcomes; recent/small/completed records pass through; strategies run at compress time by call-site contract.

## DCP07 Config/control — PASS

- Layered config (defaults < `dcp.jsonc` < per-model), unknown keys warned, inverted limits rejected, experimental options explicitly unsupported; bare/pinned aliases resolve to one visibly-revisioned module instance, others refused; stats/debug lines carry counts only.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- `cargo test -p oc-adapters dcp_auto` 3/3; workspace 112 total (oc 4 + adapters 85 + core 16 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies.

## Scope and limitations

- Turn-loop wiring (live counters, toast notices) arrives with the runtime; T19 proves policy/strategy/config semantics.
