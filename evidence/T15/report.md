# T15 — Models, variants и admission

Status: PASS. Implementation commit: `1f1c2226491e84830f2fb8a095ea2723fe93f8c0`. Method: offline `cargo` unit execution; no network, no live credentials, no Docker. No acceptance tests are attached to T15; unit tests prove the contract instead.

## Work — PASS

- New `oc-adapters/models.rs`: `ModelCatalog` over merged discovery + static entries; exact-id `select_model` (prefix/case variants never match); `select_variant` against the entry allowlist (disabled/unknown named with the enabled set, default skips disabled `none` for the first enabled standard); `admit` requiring both positive context+output limits (over-context/over-output/missing-limit typed); `diagnose` for unknown metadata keys; redacted `explain` view.
- No production automatic fallback: unknown ids error with the bounded available list even for non-empty catalogs (asserted).

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- `cargo test -p oc-adapters models` 4/4; workspace 99 total (oc 4 + adapters 75 + core 13 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies.

## Scope and limitations

- Wiring into generation requests (effort propagation, admission estimates) arrives with the runtime turn loop; T15 proves selection semantics. Unblocks T16/T22 per the task graph.
