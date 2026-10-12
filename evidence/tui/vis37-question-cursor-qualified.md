# VIS37 — focused Question caret color / current012 qualification

Status: checked source slice, **T44 ACTIVE_NOT_PASS**. This receipt does not
qualify all VIS37 frames, R6/V09, or the remaining frozen R1–R6/VIS01–VIS45 and
supplements. Producer base is `eceb08c3247d4fded43394eba22896337ff638ad`; capture
locks retain that actual identity, independently of the later delivery commit.

## Observed source seam and minimal change

Current011 had six custom-empty/custom-edited pairs with equal whole styled-cell
grids and cursor positions but 795 differing pixels confined to the active caret.
Pinned TUI `routes/session/form.tsx:1038–1058` explicitly selects
`theme.text.formfield.focused` as the custom textarea cursor color. Native
`views::cursor_color` previously returned `Reset` when no modal panel was open.
The source mismatch was concrete; the earlier raster observation alone did not
prove how much was color versus blink phase.

The existing terminal cursor-color owner now selects `text.formfield.$focused`
only for the active visible Question request's editing draft and `panel == None`.
`QuestionView::custom_editing` reads the existing shared draft owner and matches
the complete bound request. It creates no new registry/store/query, mutation,
phase enum, or public test API. Existing dialog token selection remains separate.
Approval and MCP-detail guards still precede focused-field coloring; Rename and
ordinary composer retain Reset. The existing OSC 12/reset/cleanup path is used.

No cell foreground/background, caret coordinates, visibility, frame scheduling,
clipping, RAW, schema, permission, request binding, or ordinary composer ownership
was changed. No cursor/clock normalization, crop, mask, tolerance or golden was
used. Source review found no remaining proven regression in these three paths.

## RED → GREEN and affected invariants

New substantial test lives in existing `app/tests/input.rs`:
`vis37_custom_caret_color_uses_the_focused_field_without_stealing_overlay_ownership`.
Actual RED: 0 PASS / 1 FAIL (`Reset` versus the focused token), retained in full.
After the minimal patch: 13 VIS37 targeted tests and three real PTY/headless
question consumers PASS. The test exercises empty and edited Unicode custom
fields, visible TestBackend caret, exact production OSC 12 bytes derived from the
theme token, modal paste ownership, Model/Rename, Approval priority/retirement,
closing/re-entering edit, request retirement and unchanged ordinary draft.
No existing expectations, deadline, baseline, ignore or validation were weakened.

Canonical capture scope still has 713 paths. Relative to current011 only these
three Rust inputs changed:
- `crates/oc-tui/src/app/tests/input.rs`
- `crates/oc-tui/src/question_view.rs`
- `crates/oc-tui/src/views.rs`

Current source seal:
`ed309316934f99322de6ec538a22958eb40601f62dd44657fd6492faf743c185`.
Actual native debug ELF: 484,339,768 bytes, ET_DYN/x86_64,
`32b90fe9a0474adfe08e60dcf2ce65aaf54c91e461203a451de57b14024349fc`.
Pinned Original ELF: 200,529,376 bytes, ET_EXEC/x86_64,
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`;
donor `2670273ff17da96f85c5826ced57aa1b368754fa`.

## Actual running pairs and independent inverse

Exclusive `vis37-form-current-012/{small,wide,tall}` uses unchanged actual
18-stage probe at 80×24, 120×40 and 160×48, with sequential locked native builds.
Each side of each profile is `PASS_BEHAVIOR_ONLY`. All 108 original whole PNGs,
styled cells, cursor/VT, clocks, command/build/source records, provider traces,
readonly snapshots, parent logs and the acquisition driver survive byte-exact.
Old001–011 episodes/helpers are not replayed or modified; 011 is now historical,
and its immutable pack-time flags are not rewritten or promoted.

Strict comparison slots: small 18 DIFFERENT / 18 EQUAL, wide 20 / 16, tall 20 / 16;
total **58 DIFFERENT / 50 EQUAL**, no BLOCKED. Independent recount of 54 full pairs:
- 259,200 styled cells and 34,953,984 RGBA pixels checked;
- 259 differing cells / 11,532 differing pixels;
- 54/54 cursor pairs equal; 25 whole-grid and 25 whole-PNG pairs equal;
- **all six custom-empty/custom-edited whole frames** equal in grid, PNG and cursor;
- all 51 complete header rows, 27 pending-form regions, three answered regions,
  six cancelled rows and three small transcript/status underlays remain equal.

Named owner diagnostics do not substitute for full-frame comparisons. Remaining
random examples, actual native version and real duration observations are kept,
not forced to donor literals or normalized. This is not whole VIS37 PASS.

`vis37-question-cursor-audit.py` uses immutable independent manual XZ/USTAR and
PNG/styled-cell readers, not the producer decoder. Before/after seal every
original file and directory metadata, current source/ELFs and helpers; all 101
prior archive/inventory/archived-receipt files retain exact bytes and metadata.
Manual single-XZ framing/index CRC/dictionary/decoder/SHA and canonical USTAR
inverse verifies all 704 originals and direct same-run/same-side PNG aliases.
The audit also independently recounts actual whole frames and comparator inputs,
argv, provenance and exit/status results. Its BASE assertion and exclusive
outputs are historical execution records, not a program to blindly replay after
a later HEAD or source edit.

Durable proof `vis37-form-r12-proof.tar.xz`: 1,482,060 bytes,
`ef56d773351888a2312389566e4245a8636123cac62459738493bb67f7435841`.
Decoded TAR: 55,572,480 bytes,
`a64e8def46fa019061759da1b6089812bf71a5ad2c9191a843aad8befe580901`.
704 members / 55,993,176 logical bytes / 26 byte-identical encoded PNG aliases;
member inventory
`b48b0f6fe2c2f979c7e1f0cb03448d17a0d5442d6f24513591398e885128a436`.
Independent after report: 125,472 bytes,
`6e16e6305440f4d3ef671333126560af96fa354081aef50645eeba8a64b3adf4`;
VmPeak 239,693,824 / VmHWM 165,777,408 bytes, below hard 384 MiB.

## Effects and honest lifecycle scope

Actual traces retain 24 valid provider requests (18 main + six title), 12 question
calls, 12 user submissions + six explicit answer submissions, 30 readonly
snapshots, native typed output and real Original V2 RAW/part identities across
reopen/restart/cancel. No observed additional project/Shell/MCP effects, re-ask,
editing/review/restart/cancel continuation occurred. Generation 0 naturally exits
0; generation 1 is forced harness teardown, **not** clean-final-lifecycle proof.
DCP tables were not read. These facts do not close untested lifecycle/effect gates.

## Current-source code gates

Actual complete `cargo test --workspace --locked --no-fail-fast`:
**1860 PASS / 0 FAIL / 11 unchanged opt-in ignores / 46 targets** (TUI 530, PTY 58),
928.713 s. Strict locked workspace/all-target Clippy `-D warnings`, fmt check,
locked and ordinary debug build/help, locked release build/help all PASS.
Nine actual release/resource cases PASS via release Cargo binary targets:
two isolated startup/discovery routes; TERM01; panic restoration; two VIS31
idle/input/burst cases; S07 equal-view archive resources; VIS38 DCP archive
resources; AUD32 actual active-context bounds. No manual executable substitution.
Root Python 68 and nested question fixture two PASS. Complete command/log records
and the RED are retained in the bounded independent-inverse gate bundle.

`vis37-question-cursor-gates.tar.xz`: 54,036 bytes,
`dfc935240aeae2baaac1522edf51d4b509cee5e2d7773c51a15d3c1aa88fbbd2`;
27 actual command records/full logs and two helpers, 56 members / 243,935 logical
bytes. Every original byte passed independent manual XZ/USTAR inverse before and
after durable publication. Gate inverse VmPeak 241,496,064 bytes <384 MiB.

Actual release S07 retained bytes 24,231 at archive 0/3000, queue lag 0 and child
max 0; HWM 27,804 → 28,136 KiB. AUD32 HWM 20,736 → 22,028 KiB, children 0.
TERM01 actual release control/VT/resize/hide/focus/shutdown/crash/no-replay PASS,
idle CPU sample 1 tick / 300 ms. Other exact measurements remain in complete logs.

## Capacity and continuation

After current012 independent report, established owned allocation is
1,071,058,944 / 1,073,741,824 bytes; headroom 2,682,880 before delivery allocation.
After the receipt and gate bundle, actual allocation is 1,071,132,672 bytes;
headroom 2,609,152. This is an observed current inventory, not a future reservation.
Limits remain 16 MiB per file/encoded archive, 128 MiB TAR/logical, 2,000 members,
16 MiB XZ dictionary / 32 MiB decoder, hard Python AS 384 MiB and owned 1 GiB.
TMP transactional/acquisition allocation remains separate, not durable offload.
No qualified proof deletion, PNG re-encoding, old receipt rewriting or cap raise.

Continue all unresolved frozen R1–R6/VIS01–VIS45 and supplements, using current
sources and real Original/native pairs; do not invent parity by fixing version,
randomness, timing or raster metadata. Final current-source R6/V09 and whole-task
closure remain required. This source slice is delivered separately by ordinary
reviewed commit/push; T44 is not finished by this receipt.
