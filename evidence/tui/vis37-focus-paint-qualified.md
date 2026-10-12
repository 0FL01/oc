# VIS37 observed focus-paint boundary — qualified slice, T44 ACTIVE_NOT_PASS

Date: 2026-10-12. Implementation base: `30f4e707e5c3b870d3ad8b571d321ac601ec8c4a`.
This is a capture/proof refinement, not a Rust cursor-position workaround or whole
VIS37/R6/V09/T44 PASS. The complete original004 burst observations remain preserved.

## Cause and change

The three004 hidden multiple-selection cursor positions were Original x18 versus
Native x10. Inspection of the complete preserved VT streams showed different paint
boundaries: Native painted Down's focused Python row, then Space's checkbox alone;
Original coalesced both changes and printed through the newly focused Python label.
Both backends leave an invisible physical cursor after their last emitted changed
cell. Ratatui's absent frame cursor hides it, without repositioning; the pinned
OpenTUI renderer also does not restore a hidden cursor position or print unchanged
label cells. There is no evidence of a misplaced visible custom-answer caret here.

The existing question probe now sends Down, observes the actual Python-row focused
style against the previously observed Rust-row style, and captures the **whole**
`question-multiple-focused` frame before sending Space. This supplies the previously
missing focus transition and compares the selected state after the same observed
paint boundary. Both inputs and frames survive. No clock, cursor, style, coordinate,
input stream or renderer output is normalized, masked or synthesized. The original
burst discrepancy is not reclassified as equal, and burst paint parity is not claimed.

Only `scripts/tui_capture/question_forms.mjs` changes a captured source input.
`scripts/retain_syntax_proof.py` and its tests admit the fixed005 campaign and verify
the separate exact Down/Space events plus the observed focus-stage fact. Existing
001–004 roots, helpers, archives, receipts and default campaign behavior are unchanged.
No Rust runtime, backend scheduling, approval/modal routing, policy, RAW or API change.

## Actual fresh acquisition

Exclusive driver: approved TMP `vis37-form-capture-r5.py`; exclusive roots
`vis37-form-current-005/{small,wide,tall}`. Profiles80×24,120×40,160×48 ran sequentially
with one successful `cargo build --locked` recorded per profile, never overlapping.
Each side completed all18 stages: **PASS_BEHAVIOR_ONLY**. No prior episode was replayed.

- Complete54 paired frames /108 original encoded PNGs, full styled cells/cursor/VT,
  clocks, input/protocol/effects, actual command/build/source seals, parent logs/driver.
- Small36 DIFFERENT; wide28 DIFFERENT and8 EQUAL; tall36 DIFFERENT. No BLOCKED pairs.
- Wide's completed, answered-card, reopened and restarted frames each have both
  full-grid and full-PNG EQUAL results. All other100 comparisons remain DIFFERENT.
- Independent whole-frame recount:259200 styled cells and34953984 RGBA pixels checked;
  4461 different cells /317181 different pixels; **54/54 complete cursor pairs equal**.
- **27/27 pending owner and3/3 answered-card owner full styled-cell regions equal**,
  including geometry, blank foreground/background and the additional focused state.
  These owner diagnostics do not replace any full-frame comparison.
- 24 valid provider requests =18 main +6 title;12 question calls;12 explicit user
  submissions +6 explicit answer submissions;30 read-only snapshots. Actual native
  typed answers and Original V2 parts/RAW IDs remain bound through reopen/restart/cancel.
  No observed edit/review/reopen/restart/cancel continuation or additional Shell/MCP/
  project effects. DCP tables were not read. Generation0 exits naturally with code0;
  generation1 uses recorded forced harness teardown, not clean-final-lifecycle proof.

Current canonical713 source seal:
`7178f1bb8abe558cd093a1e1a30a273829c6786f19f2b657cc2a8872a3ea7f6b`.
Native ELF484335256 bytes:
`3c2025c095e3618c4a03112646f8e0a0ac61a7c77372db38480a177af7436271`.
Original ELF200529376 bytes:
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`;
pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`. Both ELFs are unchanged.
004 is now historical because the probe source changed; its immutable pack-time
source flags, measurements and receipts are not rewritten or promoted.

## Durable byte-exact proof and independent inverse

- `vis37-form-r5-proof.tar.xz`:1953940 encoded bytes; decoded TAR55613440 bytes;
  all701 files56049246 logical bytes and27 direct same-run/same-side exact PNG aliases.
- Archive SHA-256:
  `f8417af26c27c1688641b344dba2096c31ed54473f25259df25acd4e32a0a653`.
- TAR SHA-256:
  `8305cb0f3c067fbca610099e0b855f3843f8bc20106882657f22a0ff6e63de89`.
- Member inventory SHA-256:
  `611a21e22421dd0674e640b92971acc2467125cc8a3e988ad56658b384e6fcc8`.
- `vis37-form-r5-proof.inventory.json` records complete producer reports and codec
  identity. Existing measure → exclusive pack → audit: **FULL_BYTE_EXACT**.
- `vis37-focus-paint-audit.py` and before/after reports independently seal all actual
  originals/metadata, prior proofs, helpers, source and ELFs. Manual single XZ header/
  footer/index/CRC/LZMA2/SHA verification and direct canonical USTAR parsing compare
  every701 original byte, inventory and exact PNG alias. Separate full-grid/PNG
  recount verifies actual EQUAL and DIFFERENT reports, provenance and commands.
- After report SHA-256:
  `2532867d65908dcc3efe41e20b4bf87a4245d32bd55f77d87f5a8c6be24bbe72`.
  Independent VmPeak240185344 /VmHWM165904384 bytes <384MiB hard address-space limit.
- `vis37-focus-paint-gates.tar.xz` and inventory retain full actual gate logs, the
  VT diagnostic helper and bundle helper, including the failed RED and first
  independent-reader attempt. All27 members50234 bytes receive separate manual
  memory/durable inverse; archive6308 bytes, decoded TAR71680 bytes. Archive SHA-256:
  `3ebeff6befd6ac64385590185d025a02f6cfece36c4af773a8512ebc991a2caa`.

The first independent before-reader correctly rejected a real wide EQUAL result
because its inherited command guard assumed all comparisons failed. A local strict
reader now accepts only actual EQUAL/0/PASS or DIFFERENT/1/FAIL and independently
recounts each full artifact. Old readers and original data were not modified; no
before report was written by the failed attempt. Both attempts remain visible.

## Actually executed gates and bounds

Retainer RED→GREEN: new exact Down/Space/focus guard;15/15 retainer tests and68/68
root Python tests PASS. Probe `node --check`, diff whitespace, docs/progress checks
PASS. No Rust gates are newly claimed: prior1856/0/11 all46 native tests/release9
cover unchanged Rust inputs/ELFs, but do not constitute final current R6/V09.

After the independent after-report, owned allocation was1072025600 /1073741824
bytes, headroom1716224, **before** final bounded gate/receipt/progress records. Their
actual post-bundle allocation is1072046080/headroom1695744, before this small receipt
addition; delivery rechecks the live allocation. Same16MiB file/encoded,128MiB logical/
TAR,2000 members,16MiB dictionary/32MiB decoder/384MiB hard AS/1GiB aggregate bounds.
No old proof deletion, offload, cap raise, PNG reencoding, mask, crop or tolerance.

## Continue full frozen scope

The next source-proven mismatch is the persisted dismissed-question transcript:
Native paints raw `error: question dismissed`; pinned donor presents the weak
right-aligned `Questions dismissed` status. Correct presentation through existing
typed tools/history owners, retaining real cancelled outcome/RAW and no continuation.
Other chrome/underlay/footer/history differences remain. Continue every frozen
R1–R6/VIS01–VIS45 and supplement, deliver checked major slices, then perform final
current-source R6/V09. This slice does not finish T44.
