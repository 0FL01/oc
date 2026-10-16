# T13 — Tools, reasoning и images

Status: PASS. Implementation commit: `8d93e48258ca4e9886041857f45b5a198e0d5780`. Method: offline `cargo` unit execution with `tempfile` isolation and loopback SSE/HTTP servers; no external network, no live credentials, no Docker.

## PROV03 Tool roundtrip — PASS

- Parser extended (`ToolCallStarted` from `output_item.added`, first-wins terminal usage so replayed `done` never overwrites): two ordered calls assemble, execute (`read` real bytes; missing path fails visibly per-call), and render as `function_call_output` items keyed to original ids; a body-driven loopback server proves outputs → next response completes (`done`, usage 5/6).

## PROV04 Reasoning — PASS

- `TurnLog` ingests opaque/reasoning items, serializes to the turn row (`turns.result` via new `Db::turn_result`), replays verbatim only for the same `(model, provider)` (switches drop alien state with diagnostic), and `ui_projection` strips opaque + reasoning deltas while keeping text/tool activity.

## PROV05 Images — PASS

- `attachments.rs`: `text/*` + PNG/JPEG/GIF/WEBP verified by magic bytes, 8 MiB single / 16 MiB total caps, audio/video/PDF visibly refused, data-URL segments.

## TOOL10 Tool graph — PASS

- Duplicate call ids refuse the batch pre-execution; unknown tools / invalid JSON / unannounced-arg deltas become per-call error outputs in first-appearance order; outputs match original ids; registry is exactly `read/apply_patch/bash/webfetch/skill`.

## TOOL11 Skill loading — PASS

- Model projection carries id/name/description only (serialized assertion: no bodies); snapshot build warns-and-skips oversized/invalid files; unknown/stale calls fail visibly; deny-policy blocks without side effects (marker file absent); batch outputs persist through `Db` messages and read back (durable graph).

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- Adapters 61 tests incl. 8 new tools + 1 attachments; workspace 85 total (oc 4 + adapters 61 + core 13 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. New dep `base64 0.22` (MIT OR Apache-2.0, already in lockfile transitively).

## Scope and limitations

- Registry wiring into the turn loop (model-driven multi-turn) arrives with the runtime; T13 proves executor/assembly/persistence semantics.
- Skill discovery (filesystem walk into snapshot) reuses `SkillSnapshot::build`; not claimed here.
