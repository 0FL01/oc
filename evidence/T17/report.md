# T17 — DCP projection и storage

Status: PASS. Implementation commit: `07475001978762a5ae71353910a9d4bbd8200dd4`. Method: offline `cargo` unit execution with `tempfile` isolation; no network, no live credentials, no Docker.

## DCP01 Projection — PASS

- New `oc-core/context_plan.rs`: immutable raw transcript + separately computed outbound projection. Stable sha256 checksum over id/role/text order; `(start_id, end_id)` specs resolve against stable ids (unknown/inverted/overlapping rejected); covered spans collapse into summary blocks with saved-token estimates; pruned prefixes drop from the outbound view only. Rough 4-chars-per-token estimator documented as planning-only.
- New `oc-adapters/dcp.rs` policy: pinned range-tool schema (`topic` ≤256, 1–32 entries, `startId`/`endId`/non-empty `summary` ≤8192; message-mode shapes rejected), DCP defaults as constants, durable block/member/prune writes through `Db`.
- Storage migration v2 (additive, idempotent): `compression_blocks` + `compression_members` (id references only, never text — no second history copy) + `prune_marks`; per-session `b0001…` ids; `turn_result`-style loaders.
- Projection carries role/text/summary only; assertions prove no `apiKey`/`provider` keys and raw checksum stability across projection.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- Core context_plan 3/3 + adapters dcp 3/3; workspace 105 total (oc 4 + adapters 78 + core 16 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies (`sha2` already pinned).

## Scope and limitations

- Model-driven range selection (compress tool calls), nudge scheduling and auto-pruning arrive in T18/T19; T17 proves plan/projection/policy/durability semantics.
