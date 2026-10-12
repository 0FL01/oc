# T44 retention recovery — current004 fully retained (2026-10-12)

Status: **RETENTION_UNBLOCKED / T44 ACTIVE_NOT_PASS**. This supersedes only the
current storage-blocker interpretation in `vis37-form-qualified-blocked.md` and
checkpoint0127; their measured failures and immutable historical bytes remain.
Base: `d376d12fdd7c245df00194bc62611e8114c0d88c`, own `agent/oc-rust-port`.

## Cause and bounded correction

The previous stop had not exhausted the existing lossless archival path. The own
untracked/unstaged `model-shell-001` was an actual FAILED diagnostic, not a qualified
proof: Native FAILED / Original PASS_BEHAVIOR_ONLY. Its 58 files occupied5189632
allocated bytes. Existing archival retained all5040210 original bytes, complete
paths and mode/UID/GID/mtime_ns in278616 encoded bytes; the original path now has a
visible **FAILED / NOT_QUALIFIED** pointer. No qualified proof or old receipt was
deleted/rewritten, no external side effect was replayed and no cap/accounting scope
was changed. This is the same operation already documented in `syntax-qualified.md`.

The existing `archive_failed_capture.py` now admits only this additional literal
campaign and has `--reserve-bytes` (default0). Both preflight and post-write checks
preserve that capacity before the verified quarantine cleanup. Actual reservation
was2183168 bytes:1921024 for full004 plus262144 bounded delivery allowance.
Before transaction allocation1072680960; after1067798528: **4882432 bytes released**.
Docs now distinguish checked lossless archival from proof deletion/permanent
offload and disallow blocking merely by attempt count or unexamined quota.

## Durable artifacts and independent verification

- `model-shell-001.tar.xz`:278616 bytes, SHA-256
  `d464e6a96bf2c5b1a2bcb0e7d91fcf391bd52cb6a83c6960f59f579201bdbe4b`.
  Receipt `model-shell-001.archived.json`; pointer `model-shell-001/ARCHIVED.md`.
  Independent manual PAX records/header/checksum and XZ CRC/index/block-SHA inverse
  confirms every58 file byte and all3 directory/file metadata against sealed before.
- `vis37-form-r4-proof.tar.xz`:1913176 bytes, decoded TAR53002240 bytes;
  all665 files53495947 logical bytes and28 direct same-run/same-side identical PNG
  aliases. Archive SHA-256
  `6e30ed8195c5898067ce7a31f9b4e35dfd9c2c7656a35da91c69ac610d117ddb`;
  inventory digest `1d55e6e80dfc9c5815e7a387b799b7c2f6df1a4d97d3674ae7d0551a4e09fe90`.
  Existing measure → exclusive pack → audit all FULL_BYTE_EXACT. Fresh independent
  manual USTAR/XZ inverse compared every member with the unchanged actual original.
- `vis37-retention-recovery-audit.py`, `-before.json`, `-after.json` preserve the
  independent verifier and sealed reports. No producer encoder/audit is imported.
  All63 preexisting retained archives/inventories kept exact bytes and metadata.
  Current004 originals and their directory metadata, old helpers, source and ELFs
  remain unchanged. Independent VmPeak238796800/VmHWM161677312 bytes <384MiB AS.

## Current-source and acceptance scope

Canonical713-source seal remains
`540195999c68df1710ced7274273eca77f7c178ebb4cd563500a966a9c1a6928`;
debug ELF `3c2025c095e3618c4a03112646f8e0a0ac61a7c77372db38480a177af7436271`.
Storage-only scripts/docs are outside that native/capture manifest. No build or
capture replay was needed; captured producer identities were not rewritten to HEAD.

The retained004 contains102 whole PNGs/full styled cells/cursor/VT, commands,
source/build/protocol/effects, parent logs and original r4 driver at all3 profiles.
Both sides passed17 behavior stages. All102 strict comparisons remain DIFFERENT;
the preserved independent recount is4444 cells/317372 pixels different,48/51 cursor
pairs equal,24 pending-owner/3 answered-owner full styled regions equal. Exact
restored bytes preserve this prior recount; this recovery did not rerun raster
metrics or turn owner equality into full-frame PASS. Fresh typed-effects audit
confirms24 valid provider requests18 main6 title/12 calls/12 user6 answer submissions/
30 read-only snapshots, real native/Original V2 RAW, no observed edit/review/restart/
cancel continuation or extra project/Shell/MCP effects. DCP tables remain unread;
final generation1 was forced harness teardown, not clean-final-lifecycle proof.

## Checks and allocation

- Initial targeted RED: model-shell selection refused (old literal list); GREEN.
- Root Python **67/67 PASS**, including6 archival tests and14 retainer tests. Actual
  FAILED versus raster DIFFERENT, exact reservation boundary/invalid reserve,
  public-write interruption and final-cap refusal retain original bytes/quarantine.
- The first `retention-model-archive` command omitted `--archive` and was only a
  no-write preflight; its following measure honestly failed the unchanged cap.
  The explicit `--archive` command then completed the real verified transaction;
  subsequent measure/pack/audit and independent before/after all passed. Both
  command records are preserved; no unknown-effect retry or hidden failure.
- After proof pack, before independent after-report allocation1069719552 of
  1073741824, headroom4022272. Subsequent bounded records are charged normally;
  final delivery recount must remain under the same cap.
- Limits remain16MiB file/encoded,128MiB logical/TAR,2000 members,16MiB dictionary,
  32MiB decoder,384MiB Python AS and1GiB own evidence. Diagnostic dictionary8MiB.
- Complete gate commands/logs, including RED and the no-write failed measure, are
  retained in `vis37-retention-recovery-gates.tar.xz`:4920 bytes,29 members26509
  logical bytes, SHA-256 `322121f4ad9d3ca04a6f203f442be26cc7eef6f17c8951fbba13a99bfecd310b`.
  Independent manual USTAR/XZ inverse compared every original gate/helper byte.
  Docs/progress/diff checks PASS. After that bundle and bounded recovery records,
  actual allocation1069748224/headroom3993600; subsequent delivery recount remains
  authoritative if another file block is allocated.

Historical Rust1856/0/11 all46, strict Clippy/fmt/debug/release/release9 qualification
still applies to the identical713 native/capture inputs; it is not newly executed
by this storage slice and does not replace final current-source R6/V09.

## Continuation

No authorized-capacity dependency remains for current004. Continue the3 actual
VIS37 hidden-cursor residuals through the existing input/render owner, then remaining
whole-frame/chrome/footer/history and every frozen R1–R6/VIS01–VIS45 supplement.
Each major verified slice is reviewed/committed/pushed; T44 is not finished until
all required outcomes and final current-source R6/V09 pass.
