# VIS36 — retain the Settings filter after permission-mode ACK

Date: 2026-10-13. Task T44 remains **ACTIVE_NOT_PASS**.
This receipt closes the observed filter-reset defect, not whole VIS36/R6/V09/T44.
Captured producer is `a8d7f0483c64036efe1f5bc1be5531f4f81a36c3` plus reviewed
source bytes; delivery HEAD is separate and does not rewrite acquisition identity.

## Observed contract and minimal change

Pinned TUI `component/dialog-config.tsx:356–386` updates the setting without
clearing DialogSelect's query. Native `permission_mode_applied` reset the selector
after the owner ACK, replacing the visible `Permissions` filter with `Search`.
The existing Settings select owner now synchronizes its cursor against the
updated list instead of resetting filter/focus. Other panels and API semantics
are unchanged. No new state owner, store, policy, schema, command or timer.

New `vis36_settings_mode_ack_keeps_filter_selection_and_search_caret` uses the
real TUI state, typed mode intent and 121×40 whole TestBackend frame. RED observed
an empty query instead of `Permissions`; GREEN retains query, selection, value,
search caret, panel and unsent Unicode draft on both Autoaccept and Prompt ACKs.
The existing actual-owner case had asserted the old incorrect Search state;
that expectation alone was corrected after checking pinned behavior. It now
verifies the filtered item at 79/80/120/121 columns and separately verifies first
Escape clears the filter while the second closes Settings. Error ACK, pending/new
approval, CLI override, reload, cancel and binding checks remain in the seven
real-owner cases. No timeout, baseline, ignore or validation was weakened.

## Current actual acquisition

Fresh exclusive `vis36-mode-current-004/cli-wide-r4`, 121×40, uses the existing
permission harness, actual rebuilt native ELF and running pinned Original. Both
sides pass 38 lifecycle checks. The same 16 full stages produce 32 whole PNGs and
32 strict grid/PNG comparisons; **all 32 are DIFFERENT**, none omitted or masked.
The two mode-change filters retain all eleven query glyphs and their styles.
The one-column odd-width dialog offset remains visible and is the next owner
gap; filter-token equality is a diagnostic, never a replacement for full-frame
comparison. Shell/transcript/other Settings differences remain separate.

Each side records 18 valid requests and completions (16 main, 2 title), 8 real
calls, 11 read-only SQLite snapshots, two natural exit-0 generations and zero
Always grants. Saved Autoaccept→Prompt, process CLI precedence, root/child Once,
mixed Deny, actual patch/Shell/read effects and relaunch are preserved. Relaunch
admits a fresh root while retaining saved preference/profile; it is not evidence
of same-root restart. Native final history has 8 operations (7 completed, 1 failed),
8 completed turns and 16 messages; Original V2 RAW has 32 messages and eight bound
tool parts. Read-only SQLite is not a syscall or transient CoreEvent trace; the
seven typed owner cases establish ordering/bindings separately.

## Byte-exact retention and independent recount

- Archive: `vis36-settings-proof.tar.xz`, **1,261,404 B**.
  SHA-256 `ae2e5454ca6d689bed736985caa492f80e91cde8647257b9afed1f61583e8785`.
- Decoded USTAR: **22,149,120 B**;
  SHA-256 `76920076f56c260630a2704c07f62185f14960da4fb7d0e276418346a679b86d`.
- All **212 original files / 22,071,995 logical B**, including parent logs and
  acquisition driver. Two direct same-run/same-side byte-identical encoded-PNG
  aliases preserve the original bytes; no PNG re-encoding or cross-run predictor.
- Member inventory SHA-256
  `873284fb68f4b2c21f9905b84e793eedb5aab23dd28687567b7477fdc24130bd`.
- Retainer source at pack:
  `d133d694c0de664b2e3efbdc767a7c16420f05c894bed8ddcf2b9a5424e16e1f`.

`vis36-settings-audit.py` independently reads single-XZ headers/footer/index/CRCs,
LZMA2 dictionary and SHA framing, then canonical USTAR and every original member.
It does not import the producer codec. Before/after seals verify file and directory
metadata, all current source/ELFs, helpers and **111 prior proof files** unchanged.
The full-grid/PNG observer recounts every styled field, cursor and RGBA pixel and
checks actual comparator commands/input provenance. It uses no mask, crop,
tolerance, golden, clock/cursor normalization or discarded failing observation.

Current totals: **16 full pairs / 32 PNGs**, 77,440 cells and 10,444,800 pixels
checked; **20,318 different cells / 2,395,128 different pixels**, 9/16 complete
cursor pairs equal. Whole parity is not PASS. Independent after report is
`vis36-settings-after.json`, 9,087 B, SHA-256
`18ddc32d8f3fc66a66e94045f34e58e376ae4b998406d18ab994895f7cf96c15`.
VmPeak 156,418,048 B / VmHWM 98,918,400 B, below hard 384 MiB.
The sealed audit's BASE/outputs are immutable; do not blindly rerun its entrypoint
after delivery. Historical current003 flags and all failed001/002 bytes remain
unchanged; current-source qualification applies only to004.

## Current-source seals

Canonical 713-file source:
`cf12d8212bce7376843892f5b3598b4053e1f2e2dcdf13efd733b4d85e5e8272`.
Changed acquisition inputs relative003 are only `app/input.rs`, `app/tests/input.rs`
and `oc/src/approval_tests.rs`. Retention support scripts are outside that manifest.

Native debug ELF: 484,373,008 B, ET_DYN/x86-64,
`e48b61c11bb6667f8725bddd10dc5ab0cb4a48a164f465d602f82bfad59d38ba`.
Original ELF: 200,529,376 B, ET_EXEC/x86-64,
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Original pin: `2670273ff17da96f85c5826ced57aa1b368754fa`.

## Actually executed code gates

- `cargo test --workspace --locked --no-fail-fast`: **1865 PASS / 0 FAIL /
  11 unchanged opt-in ignore**, 46 targets (838.457 s), TUI 531 / PTY T39 58.
- Targeted VIS36 TUI: 2 PASS; actual owner cases: 7 PASS after the recorded old
  expectation failure. Root Python: 70 PASS after the final support-script change.
- Strict `cargo clippy --workspace --all-targets --locked -- -D warnings`, fmt
  check, locked/ordinary debug builds and help, locked release build/help PASS.
  Release build 142.005 s; no manual ELF substitution.
- Actual release/resource cases: startup/discovery 2, TERM01 1, panic restoration 1,
  VIS31 pacing 2, S07 archive 1, DCP equal-active archive 1, AUD32 memory 1 — **9 PASS**.
- Complete RED/old-owner-failure/success command JSON/full logs, unchanged gate
  wrapper, model006 independent archival helper and pack helper are retained in
  `vis36-settings-gates.tar.xz` with a separate manual memory/durable inverse.
  Its inventory records exact command status, totals and archive/member SHA seals.
  All 32 actual command records/full logs plus three helpers: 67 files / 258,724
  logical B; archive 56,980 B, SHA-256
  `dc4afa7440bd63b48465b339c06e6ec9fd20e1761eba899f39c8ecf4c977d7dd`;
  member inventory `7aa348c74be690af3b5cb98e5b71f89ca0ad065e6b210183ebd539b11ae0b773`.
  Manual memory/durable inverse PASS; VmPeak 244,264,960 B / VmHWM 72,613,888 B.
- Docs/progress structure and reviewed diff checks PASS. These do not themselves
  establish pixel parity or satisfy remaining frozen outcomes.

## Capacity, preservation and continuation

Own untracked/unstaged actual FAILED `model-shell-006` was admitted literally by
the existing archiver, not a wildcard or a DIFFERENT-as-failure rule. All 69 files,
three directories, bytes/paths/mode/UID/GID/mtime and failure visibility survive
manual PAX/XZ inverse in `model-shell-006.tar.xz` plus receipt and original-path
FAILED pointer. Source allocation 5,988,352 B; archive 330,272 B, SHA-256
`0ade5215d51594bf762e7143ab70e9a6539c148cc891f185b64df549f79362d8`.
The 3 MiB reserve and actual post-write guard passed; 5,632,000 allocated B were
reclaimed only after verifying the durable archive and private redundant copy.
Qualified proofs, old receipts and all other diagnostics were not deleted or
rewritten; PASS-behavior/Different model003 remains untouched.

After current004 report, actual additive-prefix allocation is 1,069,088,768 /
1,073,741,824 B, headroom 4,653,056 B, **before human receipt/gate-bundle growth**.
Final delivery recount is authoritative. All established prefixes plus `vis36-`
are included; no scope removal. Same 16 MiB file/encoded, 128 MiB logical/TAR,
2,000 members, dictionary16/decoder32 MiB, Python AS384 MiB and 1 GiB owned cap;
TMP is separately transactional, not durable offload. No paid calls or user
configuration/auth/environment edits, no RAW/policy/preimage/child-authority change.
Post receipt/gate-bundle actual allocation: **1,069,162,496 B**, headroom
**4,579,328 B**; final source/original-byte delivery seals are checked separately.

Next: exact reviewed stage/ordinary commit/push, then source-verify odd-width
DialogSelect centering and continue **all** frozen R1–R6 / VIS01–VIS45 supplements.
Final current-source R6/V09 and full closure remain mandatory; no task finish here.
