# T29 — Clean build и audit (BUILD02/BUILD03/OPS01/OPS04)

Status: PASS (offline; no live needed).

## Clean environment (all gates re-run inside it)

Scratch `~/.cache/opencode-tmp/opencode/t29` (outside repo):
fresh `HOME`/`XDG_CONFIG_HOME`/`XDG_DATA_HOME`, shim `PATH` with only
`cargo rustc sh cc gcc ar as ld ranlib strip objcopy env sleep`
(no `node`/`bun` anywhere on PATH), real `CARGO_HOME` registry reuse,
`CARGO_NET_OFFLINE=true`, fresh `CARGO_TARGET_DIR` (full from-scratch
build, ~129 MB debug binary).

## BUILD02 Quality — PASS

- `cargo fmt --all -- --check` exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` exit 0.
- `cargo test --workspace --locked`: 194 passed, 3 ignored (live
  harnesses only), 0 failed. No ignored required tests.
- One clean-env-only finding: `mcp_stdio::kill_reap_leaves_no_zombie`
  needs `sleep` on PATH (T21 design resolves argv0 via parent PATH).
  Added to the shim; product code unaffected. Same class: `as`/`ld`
  required to compile `aws-lc-sys` (standard binutils, documented).

## BUILD03 Standalone — PASS

- `cargo build --locked` from scratch in the clean env: exit 0.
- `env -i oc --help` prints usage; `oc --smoke` prints crate versions.
- `ldd` shows only system libs (linux-vdso, ld-linux, libc, libm,
  libgcc); no Node/Bun/JS host linkage. No `package.json` in tree;
  fixtures use POSIX `sh` only.

## OPS01 Provenance — PASS (audit, no legal judgment)

- `planning/baseline.lock.json` pins opencode v2.0.10
  (`b8cedc1…`), DCP 3.1.15 (`11f6517…`, AGPL-3.0-or-later),
  openproxy (`4ef76db…`); `docs/SOURCES.md` lists every primary
  source with commit/path; `docs/DCP.md` declares the native-port
  boundary (functional core only, no npm package/updater).
- `references/` holds only the owner-provided
  `openproxy-models.user.mjs` (183 lines); no copied OpenProxy
  implementation in `crates/` (native Responses/MCP clients).
- Dependency licenses (`cargo metadata --offline`, 300 packages):
  no GPL/AGPL/SSPL. Single triple-licensed hit `r-efi`
  (MIT OR Apache-2.0 OR LGPL-2.1-or-later), UEFI-only, not linked on
  this target; MIT/Apache choice available.
- Workspace declares `MIT OR Apache-2.0`; DCP AGPL provenance notices
  preserved per D06/AGENTS.md (no unsupported license claims).

## OPS04 Fresh restart — PASS

- Fresh data root: `sessions list` empty; `run "say hello"`
  (local MockProvider, no upstream/Node) exit 0; `sessions list`
  then shows `ops04` persisted. No migration/release code touched.
- `examples/`, `fixtures/`, `references/` untouched (`git status`
  clean there): examples effectively read-only.

## Secrets scan — PASS

- Tree scan for `sk-*`, private-key headers, `ghp_*`, `xox*`:
  only two prose false positives (`task-done-is-…`, docs prose),
  no key material.
- Real owner key lives only in `.local/live.env` (gitignored);
  `git ls-files` shows zero `.local/` paths. Test fixtures use
  `test-key`/`live-key`/`fake-secret` literals only.

## Checks

- Clean-env: fmt exit 0, clippy exit 0, tests 194+3 ignored, build
  exit 0 (all with node/bun-free PATH, fresh HOME/XDG/target).
- `python3 scripts/check_docs.py` exit 0 (normal env).

## Risks

- Shim `PATH` allowlist is harness-specific; reproducing hosts need
  standard binutils + coreutils (documented above).
- License audit is manifest/source-based, not a legal review (per D06
  any license conflict blocks only the affected push).
