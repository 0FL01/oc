# T08 — Read и search

Status: PASS. Implementation commit: `fc483d4c2cf62e6c58d76315557dc0750a7d8d04`. Method: offline `cargo` unit execution with `tempfile` isolation; no network, no live requests, no Docker.

## TOOL01 Read/search — PASS

- New `oc-adapters/files.rs` (`Files` bound to trusted project root + forbidden data root): `read(path, offset, limit)` returns bounded lines with `truncated`/`next_offset` (`READ_BYTES_CAP 65536`, `READ_LINES_CAP 2000`); `glob(pattern, offset, limit)` stable sorted paginated with hand-rolled `*`/`?`/`**` matcher and `WALK_FILES_CAP 10000`; `grep(pattern, literal, offset, limit)` literal-substring only, sorted by path then line, per-file 1 MiB cap, binary files skipped.
- Own data root refused in all shapes: direct path and `..` recursion landing inside it → `OwnDataRoot` (lexical walk from root preserves the landing: data-root hit checked before project confinement); absolute paths outside → `OutsideRoot`; symlink components refused no-follow, links landing in data root → `OwnDataRoot`, others → `SymlinkEscape`. Walk never descends into data root or follows symlinks, so glob/grep never surface its contents.
- Binary diagnostic: NUL byte → `Binary` without loading; large files truncate with cursor. Regex grep mode explicitly refused (`InvalidPattern`, no custom engine) until a vetted engine is pinned — literal/regex modes stay distinct by construction.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0 (fixed `manual_clamp`, `collapsible_if`, `needless_borrows_for_generic_args`, tuple-struct type-ascription parse error).
- `cargo test -p oc-adapters files` 5/5 (chunks+cursor, glob sorted/paginated, grep literal sorted/paginated + regex refusal, binary/large, data-root direct/recursion/symlink + glob/grep silence).
- Workspace 49 total (oc 4 + adapters 25 + core 13 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies.

## Scope and limitations

- `apply_patch` is T09; shell/webfetch/MCP are later tasks. `Files` is not yet wired into a model-tool registry — registration arrives with the tool pipeline.
- Time-bound grep (wall-clock cap) not implemented; file-count/byte caps bound the work instead. Full upstream fixture diffing stays documentary per T02.
