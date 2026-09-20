# T18 — Compress range и protections

Status: PASS. Implementation commit: `94b0444a5fce68a5f16b800047552ef196e33545`. Method: offline `cargo` unit execution with `tempfile` isolation plus upstream source grounding (pinned DCP `range.ts`/`range-utils.ts`/`protected-content.ts` fetched to `.local`, contracts transcribed to `fixtures/dcp-compress.json` with sha256 provenance); no live credentials, no Docker, no JS execution.

## DCP02 Range schema — PASS

- Stable ids survive persistence/restart (drop + reopen same dir → identical blocks/members, ids still resolve against the transcript, raw checksum unchanged); invalid/stale/cross-session ids and unfinished-tail coverage rejected (tail rule stands in for invisible batch state).

## DCP03 Nested/protected — PASS

- `(bN)`/`{block_N}` placeholders parsed and expanded with cycle/depth/byte limits (cycle is a visible error, never a lossy stub); covered user messages + `<protect>` extracts appended verbatim with upstream-exact headings; complete-group/pair semantics documented to the turn-loop tool-part layer with exact upstream hook names referenced.

## DCP04 Patch protection — PASS

- New `patch::affected_paths` (op paths + rename targets, sorted dedup); all paths checked against protected globs with violations listed; `apply_patch` denied as a protected-mutation equivalent; oversized protected content → visible `Impossible`, never silent loss.

## DCP09 Effectiveness — PASS

- Synthetic 200-message closed span serializes smaller with facts retained and checksum stable; zero-saving summaries yield single-shot `NoGain` keeping the original projection (no loop).

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- Adapters dcp 7/7 (3×T17 + 4×T18); workspace 109 total (oc 4 + adapters 82 + core 16 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies.

## Scope and limitations

- Nudge scheduling/auto-pruning remain T19; call/result part hooks attach at the turn loop; message-mode compress shape stays a separate contract per T02.
