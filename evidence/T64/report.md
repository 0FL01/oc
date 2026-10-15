# T64 — native oc2 published on crates.io

## Result

Published 2026-10-15 with the configured crates.io credential, after the owner
explicitly authorized immutable registry publication. Implementation commit:
`750f377c8`; committed pre-upload checkpoint: `1b8a1cf58` and
[prepublish.md](prepublish.md). No upload retry or version correction was needed.

Public API confirmed all four versions unyanked and published by `0FL01`:

| Package | Version | Published archive bytes | Registry |
| --- | --- | ---: | --- |
| oc-core | 0.1.0 | 72,538 | https://crates.io/crates/oc-core/0.1.0 |
| oc-adapters | 0.1.0 | 1,465,085 | https://crates.io/crates/oc-adapters/0.1.0 |
| oc-tui | 0.1.0 | 8,057,467 | https://crates.io/crates/oc-tui/0.1.0 |
| oc2 | 0.1.0 | 456,385 | https://crates.io/crates/oc2/0.1.0 |

The binary package is now named oc2, matching the unchanged sole executable
target. The previous 0.0.1 bootstrap is not overwritten. Library boundaries,
native data/config identity and runtime behavior remain unchanged. README now
documents installation, prerequisites and current qualification limits.

## Checks

- All 39 source archives were losslessly converted from gzip to XZ: each complete
  uncompressed tar stream compared byte-for-byte, 19,100,096 → 7,320,508 encoded
  bytes. Only archive filenames/digests and the reference manifest checksum
  changed; source/license/query metadata and behavioral oracle stayed intact.
  The decoder is a minimal build-only dependency; no runtime download was added.
- `cargo test -p oc-tui --lib syntax::`: 5 passed, 0 failed, including the complete
  inventory and actual-reference token/style comparisons.
- `cargo publish --workspace --dry-run --locked --allow-dirty`: PASS. All four
  extracted packages built with staged registry dependencies, not sibling paths.
  Archive inspection confirmed normalized versions, README, applicable complete
  licenses/notices, all 39 grammars/build inputs and only the oc2 executable.
- `cargo publish --workspace --no-verify --locked`: PASS after the committed
  checkpoint; verification was not redundantly repeated. Cargo reported each
  version published and available. Public API independently confirmed the table.
- From approved scratch outside the checkout:
  `cargo install oc2 --version 0.1.0 --locked --root <isolated-root>`: PASS,
  release build 7m27s. Sources came from crates.io; the existing target cache was
  reused. No default user installation or user config was replaced.
- Installed root contains exactly one binary, an ELF named oc2. In a fresh HOME,
  isolated project/data dirs and empty PATH: `--version` printed `oc2 0.1.0`,
  `--help` exposed the real commands, and the existing `--smoke` exercised core,
  adapter/SQLite/client and TUI composition.
- The installed ELF completed a headless turn against a local fake Responses
  endpoint with canonical OC2 config, returned the fixture marker, and exposed
  the persisted session via `sessions list`. Two local requests (title/main),
  no paid provider request and no Cargo/Node/Bun/external tool executable on PATH.
- `rustfmt --check --edition 2024 crates/oc-tui/build.rs`, importer Python syntax
  compilation, `python3 scripts/check_docs.py` (64 tasks / 192 specifications),
  whitespace checks and reviewed implementation diff: PASS.

## Risks

This verifies packaging/installation and the affected syntax path, not a new full
runtime or full-product READY qualification. Existing owner-config/E2E06 and
VIS38 caveats remain. Fedora/macOS/Windows were not newly tested; documented
target remains GNU/Linux x86_64. Source installation needs Rust 1.93+ and native
C/C++/make/CMake build prerequisites.

Cargo reports the pre-existing locked yoke-derive 0.8.3 as yanked. The locked
archive builds and registry installation passed; unrelated dependencies were
not updated. Own source stays MIT; packages containing/linking source-derived
DCP retain AGPL notices/full text and composite metadata. Other vendored license
bytes were preserved. No credential, raw live response or user-config content
was recorded. Subsequent published corrections must use new versions.

## Next

Close the task and push the implementation/checkpoint/report on the existing
branch. Users install with `cargo install oc2 --locked`, then launch `oc2`;
the newer version replaces the 0.0.1 bootstrap. Unrelated runtime follow-up stays
with its existing owners and paused tasks.
