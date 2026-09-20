# T09 — Единый apply_patch

Status: PASS. Implementation commit: `588370177888e4cbb930e73142e280dff0cfb23e`. Method: offline `cargo` unit execution with `tempfile` isolation; no network, no live requests, no Docker, no provider-hosted schema.

## TOOL02 Patch create — PASS

- New `oc-adapters/patch.rs`: single `patchText` grammar (`Begin Patch`/`Add File:`/`Update File:`/`Delete File:`/`Move to:`/`End Patch`), unrelated to Responses hosted schemas.
- New text file with Unicode bytes preserved; new empty file via bodiless Add; append to existing empty file via Update with pure-addition hunks (anchorless `+` lines append at end); CRLF style and trailing-newline flag preserved verbatim from the original file (empty files adopt newline-terminated endings on first write).

## TOOL03 Patch edit/delete/move — PASS

- Targeted multi-hunk updates with offset tracking and full-file replacement via all-remove/all-add; delete removes and records `hash_before`; `Move to` after Add/Update renames with updated content and refuses existing targets (plan stands unmodified on refusal).
- `MODEL_TOOL_NAMES == ["apply_patch"]`; `write`/`edit` absent by assertion. Unix mode preserved on update (0755 stays 0755); binary files (NUL/non-UTF8) refused with precise diagnostics.

## TOOL04 Patch conflicts — PASS

- Add-existing → `already-exists`; file changed since patch authored → `stale-preimage` (exact context+removal matching, no fuzzy merge), original bytes untouched.
- Grammar failure in a later entry → plan-stage rejection with `failed_op` and zero commits; runtime failure in a later entry → `Partial` with earlier commits standing, never reported as success, never rolled back.
- Protected globs, outside-root, data-root, and symlink paths refused (data-landing links → `OwnDataRoot`, other escapes → `Symlink`); per-file commits via same-dir temp + fsync + atomic rename; results carry `hash_before`/`hash_after` op metadata for the runtime's own-storage log.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0 (fixed `collapsible_if`, dead helper, duplicated attribute).
- `cargo test -p oc-adapters patch` 10/10; workspace 59 total (oc 4 + adapters 35 + core 13 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies.

## Scope and limitations

- Registry wiring into the model-tool pipeline arrives with the tool executor; T09 proves parser/preflight/permissions/preimage/commit semantics.
- No all-files atomicity promised; crash mid-rename ambiguity surfaces as `unknown` at the runtime layer (T04 recovery pattern reused by callers).
