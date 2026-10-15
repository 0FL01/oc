# NOW — актуальный handoff

State updated: 2026-10-15T19:13:38+00:00
Active: T64

Сверить Git status/diff до выполнения команд.
Task: T64 — Publish native oc2 on crates.io
Spec: docs/goals/2026-10-15-crates-io.md
Evidence target: evidence/T64/report.md

Owner-authorized 2026-10-15 publication: replace the oc2 bootstrap with native 0.1.0, preserve modular library boundaries and all embedded assets, publish versioned registry dependencies, verify archive builds and an isolated cargo install from crates.io. Original source MIT; retain and ship third-party/source-derived licenses and provenance with truthful package metadata. No runtime scope expansion, auto-update, full-product READY or user-config conversion. Subsequent corrections use new versions.

Последний checkpoint этой задачи (проверить актуальность по Git):

# T64 — pre-publication checkpoint

## Result

Implementation commit: `750f377c8` (`feat(packaging): prepare native oc2 for crates.io`).
The reviewed workspace prepares oc-core, oc-adapters, oc-tui and oc2 at 0.1.0.
The installed executable remains oc2; no launcher or runtime service was added.
No real registry upload has been attempted at this checkpoint.

## Checks

- Lossless conversion: all 39 gzip tar streams equal their XZ decompression
  byte-for-byte; archive total 19,100,096 → 7,320,508 bytes. Only archive paths,
  encoded digests and the reference manifest checksum changed. Source/query/
  license metadata, fixtures.json and the behavioral reference oracle are intact.
- `cargo test -p oc-tui --lib syntax::`: 5 passed, 0 failed. This generated the
  lockfile entries for the build-only decoder and the oc → oc2 package rename.
- `cargo publish --workspace --dry-run --locked --allow-dirty`: PASS; all four
  normalized packages built from extracted archives, with staged registry
  dependencies instead of sibling workspace paths. Uploads explicitly aborted
  by dry-run. No production source changed after verification.
- Archive inspection: README, byte-exact MIT/license copies, applicable complete
  AGPL notices, internal registry versions and sole oc2 binary target present.
  All 39 XZ grammars and build/reference inputs are in the TUI archive.
- Dry-run sizes: oc-core 72,547; oc-adapters 1,465,093; oc-tui 8,057,478;
  oc2 456,410 bytes. Every archive is below 10,000,000 bytes.
- `rustfmt --check --edition 2024 crates/oc-tui/build.rs`: PASS.
  Development importer Python compilation: PASS.
- `python3 scripts/check_docs.py`: PASS, 64 tasks / 192 acceptance specifications.
  `git diff --check` and staged diff review: PASS.

## Risks

Registry credential authorization/scopes and the real registry installation are
not yet verified. Cargo warns that the pre-existing locked yoke-derive 0.8.3 is
yanked; the locked package builds succeeded and no unrelated dependency update
was made. GNU/Linux x86_64 only; Fedora/macOS/Windows are not newly qualified.
Existing owner-config/E2E06 and VIS38 caveats remain; this is not full-product
READY or a fresh complete runtime qualification. Own source remains MIT;
source-derived AGPL and other third-party licenses are preserved, not relabeled.

## Next

Commit this checkpoint, then publish only these four packages with
`cargo publish --workspace --no-verify --locked` (archive builds already passed).
Check actual registry state before any retry after an ambiguous upload result.
Install oc2 0.1.0 from crates.io into an isolated root; verify version/help and an
offline actual-binary smoke, then record the factual outcome and push the branch.


Ready (до 5): нет
Blocked: T27, T44, T45, T57, T61

Done в журнале не означает READY всего продукта; см. GOAL.md.
