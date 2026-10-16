# T65 — configuration delivery and installed-registry proof

## Result

R1–R6 and R8 are verified. The specified native V2 configuration subset is
implemented and source/checkpoint `feeea77fc` was pushed before publication.
All four crates are publicly available as0.2.0, unyanked/by0FL01. The separate
registry installation and actual installed-binary check now pass. This is not
full TS application or TUI parity and does not claim product READY.
The journal subsequently finished T65 and started T44; its preserved stash was
applied once without conflicts. [Actual return checkpoint](../T44/configuration-return-20261016.md).

## Checks

- Shared pinned TS oracle:79 passed/192 expectations. Native final package gates:
  adapters1086/0/5 existing opt-in ignored; oc2 333/0/7 existing ignored.
- Fresh normal two-thread workspace:1998 passed/0 failed/12 unchanged opt-in
  ignored,46 targets. Fmt/strict workspace all-target Clippy/debug/release/actual
  release CLI and latest four extracted-package publication verification pass.
- Single actual `cargo publish --workspace --no-verify --locked`: PASS after the
  latest archive verification and committed checkpoint; public metadata agrees.
- From outside the checkout, explicit crates-io `cargo install oc2 --version
  0.2.0 --locked` into a new private scratch root: PASS,6m37s. Cargo's installation
  metadata records `registry+https://github.com/rust-lang/crates.io-index`; the
  root's bin directory contains only `oc2`, not a path/git fallback or launcher.
- Actual installed ELF with fresh HOME/XDG/project/data and **empty PATH**:
  version0.2.0, help and native core/adapter/TUI smoke identities pass. Canonical
  modelID/compatibility config completes a real synthetic loopback headless turn
  with the cache key and affinity intact. Exactly two requests (main/title);
  persisted-session listing causes zero additional requests. Owned server closes.

Logs: `.local/evidence/t65-registry-install-01.log` and
`.local/evidence/t65-installed-smoke-02.log`. Fresh probe root:
`/home/opencode/.cache/opencode-tmp/opencode/t65-registry-probe-020-02`.
The first smoke helper used obsolete model `id` and was correctly refused as
legacy_config before provider effects. Only the test fixture was corrected to
canonical `modelID`, then rerun in a new root; no production gate was weakened.

## Risks

No known configuration/publication blocker remains. Publication is a completed
external effect: **never repeat it**. Existing Cargo credentials were used
implicitly, never read or printed. No real provider credentials, user config,
default installation, release/tag, third-party lock update or paid campaign.
Native Plan/Explore/authority and other documented application differences remain.

## Next

T65 is complete; continue the frozen full TUI objective under active T44.
Its restored Shell-success notice regression is unfinished TUI work, not part
of the published green configuration snapshot. Preserve the stash and do not
repeat restoration or publication.
