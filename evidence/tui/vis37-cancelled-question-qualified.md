# VIS37 typed cancelled Question presentation — qualified slice, T44 ACTIVE_NOT_PASS

2026-10-12; producer base `7a7ccf564ae7c593327070fcc51edbd32e95f5f7`,
branch `agent/oc-rust-port`. This receipt qualifies the observed cancellation
presentation and its saved consumer, not whole VIS37/R6/V09/T44 pixel parity.

## Actual TUI owner and correction of the previous Next

The weak right-aligned “Questions dismissed” suggestion in checkpoint0129/NOW
and the focus-paint receipt's Next was traced to **web**
`opencode/packages/session-ui/src/tools/tool-renderer.tsx`, not the pinned TUI.
Those historical records are not rewritten. The actual pinned TUI owners are
`packages/tui/src/routes/session/index.tsx:2688–2750,3508–3541` and
`packages/tui/src/routes/session/message-parts.tsx:176–253`: a dismissed question
keeps `→ Asked N questions` in muted strikethrough, with no raw error row.
The two-cell icon box styles the glyph, not its empty layout cell.

Native now uses the existing typed state `cancelled` and validated complete
Question input count, not output-word matching or partial argument bytes.
The arrow/label are muted and struck; the layout gap remains unstyled. This
identifies a cancelled Question operation, **not** a unique deliberate user
dismissal: turn cancellation while waiting converges on the same saved facts.
State/output/RAW IDs/binding/store/schema are unchanged. Ordinary failed,
unknown, denied, missing/invalid-input and Generic/MCP error detail stays on its
existing presentation/expansion path. No web-only status, invented answer result,
new phase/DTO/registry or authority widening was introduced.

## RED → GREEN and actual consumer

- New unit scenario first failed on two rows versus the pinned single row.
  One subsequent assertion incorrectly expected a Generic tool's collapsed raw
  detail; it was corrected to the existing explicit expanded contract, without
  changing Generic production behavior. Both attempts are retained.
- Glyph/layout role assertions then failed on the styled `→ ` span; splitting
  only Question glyph and layout blank made all10 targeted `vis37_` cases PASS.
- Existing actual PTY dismissal scenario now checks `Asked 2 questions` and no
  visible raw error before and after restart, while still verifying saved
  `cancelled`, exact raw `error: question dismissed`, no QuestionResult, unchanged
  RAW IDs and no additional provider request/reask. All3 actual question
  PTY/headless scenarios PASS, also included in the complete workspace gate.
- Strict Clippy subsequently found one needless borrow in the new negative
  Generic fixture. The one-character correction was followed by strict Clippy
  and a fresh complete workspace run, not a warning suppression.

## Current-source gates actually executed

- `cargo test --workspace --locked --no-fail-fast`: **1857 PASS / 0 FAIL /
  11 unchanged opt-in ignore, 46 complete targets**.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` and
  `cargo fmt --all -- --check`: PASS.
- `cargo build --locked`, `cargo build`, `target/debug/oc --help`,
  `cargo build --locked --release`, `target/release/oc --help`: PASS.
- Actual release tests: isolated startup/discovery2; TERM01 session terminals1;
  panic terminal restoration1; VIS31 idle/high-rate/provider-burst2; S07 equal
  view archive resource1; VIS38 DCP active/archive resource1; AUD32 growing
  archive1: **9 PASS / 0 FAIL / 0 ignore**.
- S07 archives0/3000 both retained24232B, event queue lag0, children0. AUD32
  peak HWM20724→23028 KiB, children0. Complete measurements are retained, not
  thresholds or baselines changed to obtain green.
- Root Python68 (including retainer15 and archiver6) and question fixture2 PASS.
  Documentation/progress checks and reviewed diff are part of delivery records.

The earlier workspace pass before the Clippy borrow correction is intermediate;
the final recorded complete run is the one counted above. No new ignore,
test disabling, limit relaxation, user/auth/env/config or manifest change.

## All three acquisition attempts retained, current008 independently verified

Existing18-stage probe is unchanged from005; three profiles80×24/120×40/160×48,
each side18 stages PASS_BEHAVIOR_ONLY, fresh exclusive roots, sequential builds.

| Attempt | Exact retained archive | Bytes | Scope now |
| --- | --- | ---: | --- |
| 006 | `vis37-form-r6-proof.tar.xz` | 1972776 | historical, known icon-gap FG/strike residual |
| 007 | `vis37-form-r7-proof.tar.xz` | 1901492 | historical after test-only Clippy correction; six small whole pairs EQUAL |
| 008 | `vis37-form-r8-proof.tar.xz` | 1972564 | current713 source and actual rebuilt native ELF |

Every attempt's whole images/styled cells/cursor/VT/clocks/commands/build/source/
typed effects/parent logs/driver is retained. No existing episode was replayed,
masked, cropped, tolerated, re-encoded or normalized. Old pack-time source flags
remain immutable historical facts, not present qualification.

Current008 archive SHA256:
`6a29576f674d709e6f727b7a2fcd1c8d1fea1e96cf71ea40eb47a91110fa6764`.
Decoded TAR55603200B,701 original files55972004 logical bytes,24 direct same-run/
same-side exact encoded-PNG aliases. Member inventory SHA256:
`765d53ca441d31f2051d9aa035aecf3ff53cf7dc1d8ff6bbc7152d6c23dc8ef5`.
Measure/pack/audit PASS; independent manual single-XZ/USTAR inverse validates all
original bytes, aliases, metadata stability and the complete captured commands.

Independent current008 recount checks **54 complete pairs / 108 whole PNGs /
259200 styled cells / 34953984 RGBA pixels**: **2823 differing cells /
215832 differing pixels,54/54 complete cursor pairs equal**. All27 pending-owner,
3 answered-owner and6 cancelled-inline row regions match every styled cell and
geometry; named-region diagnosis does not replace full-frame comparison.
**All108 current full comparisons remain DIFFERENT.** The six small whole EQUAL
pairs in007 are real preserved observations, not promoted to008.

Current713 source seal:
`dfa9bf029e868459360db3eb2bd513a1501cddd59464125294e6856a5322a0e9`.
Only three Rust renderer/test/consumer paths differ from005. Native ELF484335960B:
`9a190f804546746861286aab6e97a0bfe1e1096716896d4b96b64fbe191e4d9f`.
Pinned donor2670273ff17da96f85c5826ced57aa1b368754fa, ELF200529376B:
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Producer identity in captured locks remains original, not rewritten at delivery.

Independent audit programs/before/after reports for006/007/008 remain separate;
007's first reader failed on nonexistent numeric-metric `status`. Its immutable
reader/before/artifacts were preserved; a bounded observer refinement derives
equality from zero full differences, records its identity and validates all
original data. Current008 explicitly validates comparator PASS/FAIL against the
independently counted exact metrics. Current008 VmPeak240074752B/VmHWM166350848B
under hard384MiB; all83 preexisting proof/archive receipts and original metadata
remain unchanged after its inverse/recount.

Actual current wire:24 valid providers18main6title/12 question calls/12 user
submissions6 explicit answer submissions/30 read-only snapshots; native typed and
actual OriginalV2 RAW/part IDs survive restart/cancel. No observed editing/review/
restart/cancel continuation/reask or extra project/Shell/MCP effect. DCP tables
unread; generation0 natural exit0, generation1 forced harness teardown is not
clean-final-lifecycle proof. No blanket effects claim beyond observed surfaces.

## Existing lossless diagnostic archival, unchanged storage caps

Only literal own untracked/unstaged actual-FAILED `model-shell-002` was added to
the existing archiver. All72 files6396506 logical bytes and3 directories retain
paths/bytes/mode/UID/GID/mtime_ns in the independently inverted durable archive,
with failure-visible original-path pointer. Archive312208B SHA256:
`46a616737579f123b6157b3d9d97dd0556dc0a53a751e8c3e78570b90fa9d648`.
Actual before1072160768/after1065922560 allocation, reservation3145728B. Old
qualified proofs, historical receipts and foreign work were not removed or
rewritten. Metadata tests/reservation/interruption guards remain enforced.

After current008 inverse/report, owned evidence1072730112/1073741824B,
headroom1011712B **before final human/gate/progress records**; final actual recount
is authoritative. File/encoded16MiB,TAR/logical128MiB,2000 members,XZ dict16MiB,
decoder32MiB,Python hard AS384MiB,owned1GiB caps unchanged. TMP is separate,
not a durable offload; no cap raise, deletion of qualified evidence or PNG codec.

## Continue

Complete code/retention gate bundle `vis37-cancel-gates.tar.xz`60352B SHA256
`8edca9df0a39d46ebb88a9aed76bfa8d5b2a62bc5a819a1712899f3d50148a88`
retains51 actual command records plus full logs and four helper programs:
106 members466334 logical bytes, no aliases. Independent manual XZ/USTAR inverse
checks every original gate/program byte in memory and after durable write.
The gate packer's first local attempts diagnosed missing scripts import path and
the inverse reader's expected metadata-map shape, before any output write; both
were corrected without changing producer/reader or captured proof. The saved
bundle contains all failed renderer/layout/Clippy/007-observer gates.
After complete bounded delivery records: owned1072807936/1073741824B,
headroom933888B, same caps and scopes. Future proof retention uses only approved
lossless actual-FAILED diagnostic archival if needed, never an arbitrary stop.

Next source-proven static gap:11 session-tab layout blanks on row0 use native
white versus pinned base foreground, independent of an additional pulse-phase
background difference. Fix only actual tab layout roles and qualify fresh source;
do not globally recolor blank/caret cells or normalize clocks/backgrounds.
Continue every frozen R1–R6/VIS01–VIS45 and supplement, checked commit/push after
major slices, then final current-source R6/V09. This is not task finish or PASS.
