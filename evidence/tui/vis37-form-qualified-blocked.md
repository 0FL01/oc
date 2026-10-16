# VIS37 owned pending form — code qualified, current proof retention BLOCKED

2026-10-12. **CURRENT_CODE_GATES_PASS / CURRENT004_RETENTION_BLOCKED;
VIS37/R6/V09/T44 remain NOT PASS.** Full frozen R1–R6/VIS01–45 and every supplement
remain mandatory. This is a checked implementation slice, not task completion.

## Delivered behavior and invariants

The existing `QuestionView` now paints the pinned bordered/padded Questions panel,
24-character tabs/narrow fallback, separate description rows, semantic foregrounds
and backgrounds, wrapped question/custom text, owned custom editor/caret and review.
Review distinguishes untouched `(not answered)` from an explicitly empty multiple
answer `(none)`. Long content clips/scrolls only the body, retaining title/actions
and the full request, answers and binding. Ordinary composer text is untouched.

Real mouse hit targets come from the current paint. Down/Up require the same frame
and control; stale/replaced owners, drag, changed draft, covered modal/Approval and
remount retire transient paint/press/hover without deleting answers. The frontend
calls `share_drafts` on every loop: identical `Arc` owners must preserve an in-flight
gesture. The original implementation cleared it between Down and Up; failed real
capture001 exposed that defect despite unit tests. Same-owner synchronization is
now explicitly exercised between both halves of a click.

Tab/BackTab commit custom draft and navigate without a question reply. Left/Right
remain actual editor caret keys. Typing on a selected custom option/paste starts
editing; content changes update owned answers before Esc closes editing. Multiple
custom Enter commits without advancing/submitting; its selected answer toggles like
an option. Single nonempty Enter retains the donor's direct submission behavior.
Closed multiline custom text wraps without losing source newlines or RAW bytes.

Pinned `QuestionTool::toField` does **not** mark question fields required. Therefore
blank single custom Tab is allowed and clears that answer, rather than inventing
a new required-field error. Blank Enter closes editing without implicit submission.
This corrects the earlier assumption, not core validation: final explicit reply
still calls the original typed `QuestionInput::validate_answers`, preserving an
invalid reply/error instead of sending it. No schema, byte cap or policy was weakened.

Pending transcript presentation derives only a count from the operation's complete,
validated `QuestionInput`: `→ Asked N question(s)`. Partial `argument_stream` never
supplies a count/result; it stays `→ Asking questions…`. Full/cached transcript paths
avoid Generic expansion. Real completed `QuestionResult` exclusively owns `# Questions`.
Question block layout padding inherits its own default foreground; question/answer
text and source wrap spaces retain semantic roles. The previous 57 answered-card
blank-FG residuals and pending hint-internal-space residuals are gone in the owner
recount. No generic caret-FG paint hack was reinstated.

Production deck projection now supplies Question attention from the existing visible
request, with Permission precedence and independent busy state. No runtime phase
enum, new store/registry/query/poll or free-form model-output classifier was added.

An already-visible modal keeps keyboard, paste, pointer and cursor ownership when
the question arrives. Question targets retire behind it; modal backdrop closes the
modal normally, after which the same bound question still needs its own explicit
answer. A meaningful app test failed RED before the fix and passed GREEN after it.
The actual binary PTY case opens Model selection before releasing the provider's
question, filters/presses Enter without continuation, closes by real mouse and then
answers/restarts normally. Approval priority and immutable RAW/authority remain.

## Real paired acquisition and honest remaining differences

Exclusive drivers/roots under `/home/opencode/.cache/opencode-tmp/opencode`:

| Attempt | Actual outcome | Whole PNGs | Retention/currentness |
|---|---|---:|---|
| form001/wide | Original17 stages PASS; Native4 stages then lost-click FAILED | 22 | all failed bytes retained; historical |
| form002/small,wide,tall | each side17 stages PASS_BEHAVIOR_ONLY | 102 | retained intermediate, before final wrap/keys |
| form003/small,wide,tall | each side17 stages PASS_BEHAVIOR_ONLY | 102 | retained; historical after modal-fix |
| form004/small,wide,tall | each side17 stages PASS_BEHAVIOR_ONLY | 102 | current source; full proof retention BLOCKED |

All four attempts, original helpers and parents survive without episode replay or
metadata rewrite: **328 whole PNGs** observed in total. Failed001 has eight DIFFERENT
and 28 BLOCKED attempts; its six partial provider observations (Original4/Native2)
are not merged into current totals. Each complete profile has 34 strict DIFFERENT
comparisons. Current004 has **102 DIFFERENT**, never pixel PASS.

Profiles are 80×24, 120×40 and 160×48, sequential with no overlapping Cargo builds.
The actual PTY driver checks real Rust mouse selection, field-tab click, custom
mouse edit, optional empty Tab/Shift+Tab, custom Tab/Shift+Tab/reedit, review, one
explicit reply, reopen/restart, cancellation and saved cancellation. Current traces
independently contain **24 valid provider requests = 18 main + 6 title**, 12 question
calls, 12 explicit user submissions, six answer submissions and 30 read-only snapshots.
No observed edit/review/reopen/restart/cancel continuation, reask or extra project/
Shell/MCP effect. Native typed output and Original's actual V2 source/RAW associations
remain durable. DCP tables are unread, not declared equivalent. Generation0 naturally
exits0; final generation1 is forced harness teardown, not clean-final-lifecycle proof.

Independent004 recount checks all **244,800 styled cells / 33,012,096 RGBA pixels**
across 51 full pairs/102 PNGs: **4444 cells / 317,372 pixels differ**, 48/51 complete
cursor pairs agree. All **24/24 pending panel-owner** and **3/3 answered-card-owner**
full styled-cell regions agree, including blank foregrounds/backgrounds. This owner
diagnostic is separate from full-frame comparison: no cropping, masking, tolerance,
goldens, clock/cursor normalization or promotion of DIFFERENT to PASS. Three hidden
multiple-selected cursor positions still differ; outer chrome/footer/history/clocks
and other whole-frame residuals remain. Full VIS37 is not closed.

## Immutable source/build association and retained material

Acquisition base is `661a17bcd752b5ff60a511bd2f6b96c64b189c54`; delivery commit is
the commit containing this receipt. All713 producer paths and both captured ELFs
were checked independently and rechecked after final gates/locked debug rebuild.
Each acquisition records exactly one successful real `cargo build --locked`; later
quality commands are separate, not inserted into acquisition provenance.

| Object | SHA-256 |
|---|---|
| Current004 canonical713-source manifest | `540195999c68df1710ced7274273eca77f7c178ebb4cd563500a966a9c1a6928` |
| Native004 ELF, 484335256 bytes | `3c2025c095e3618c4a03112646f8e0a0ac61a7c77372db38480a177af7436271` |
| Pinned Original ELF, 200529376 bytes | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |
| Pinned donor commit | `2670273ff17da96f85c5826ced57aa1b368754fa` |
| Historical001–003 `vis37-form-proof.tar.xz` | `65f43236a011331adc6ea4685a7583bb8aa3da080000da25c729744bb44444b0` |
| Historical decoded TAR | `2aeef721b91cddd8c8a06fc1525846e92951cad55a7489e70911cd3546661155` |
| Historical member inventory | `4008b2d16f46eaa7c90ff8090c94b69c0c9b7e238b3def9f4df2dbcb6b90fc31` |
| Historical `vis37-form-proof.inventory.json` | `476d326c799f4f768476641f079fae77c55abf3a94c7b6d72ee06c7aa2786e77` |
| Complete offline `vis37-form-gates.tar.xz` | `4d6e33e9e14cddefbc750a23cbc6b08c679305e36556afb621c4061d4079f800` |
| Gate inventory/commands/totals | `81c6ece03e45ecb20f8fbdb1fb8e8f255a21c9c100972e97a0fe1fb15936ed88` |
| Programs/reports-only `vis37-form-audits.tar.xz` | `5cd9ffb786d7523ff3bcdb61b8918b95296b9728023962f03d9b1b2d12239018` |
| Audit inventory | `f3f6d7e38eeb56a66a330967a5bf7ec1ab6002dddaf9794a95da48a4bf875867` |

Historical proof retains **1467 original files / 117703696 logical bytes / 226 whole
PNGs / 60 direct same-run/same-side identical-PNG aliases** in 3319812 encoded bytes;
decoded TAR116664320. Measure/pack/reopen and independent manual XZ/USTAR inverse
restored every original byte. A final independent inverse rechecked all001–003
loose bytes/directory metadata and all52 current gate members. The frozen inventory's
`current_source:true` describes its pack-time003 snapshot; it is NOT current004 proof.
The old inventory/receipt was not rewritten to pretend otherwise.

The first large measure exposed real decoder allocation pressure under384 MiB AS.
The existing retainer now streams decoded TAR chunks rather than holding a third
complete TAR buffer. File/member/logical/TAR/single-stream/tail/alias checks, decoder
32 MiB and dictionary16 MiB remain intact. Tests and independent manual inverse verify
the result. Final independent inverse VmPeak318877696/VmHWM285917184 bytes. Current004
recount VmPeak129662976/VmHWM81514496 bytes. No resource threshold was increased.

Gate archive retains all52 complete offline log/receipt/helper files in56120 encoded
bytes; audit archive retains19 programs/reports in210056 bytes, including the sealed
003 before/after, corrected owner observer,004 recount and final manual inverse.
Current004 recount report SHA `bcf3aa2519c252cf01ae07d108662eb955e99ed73961fddaa18889e0259b8e30`;
final historical/gate inverse report SHA
`af39af6f42831bc893572adf15a27cbafe02929e431253c15f1003432b38b936`.
These audit records do **not** retain004 captured PNG/cells/VT/protocol bytes and
cannot replace its required full proof. Earlier observer-only ambiguous `Questions`
matching at cancel-pending was corrected by an exclusive owner-refinement wrapper;
the initial verifier/before remained unchanged. No blanket old11k-artifact byte audit
is claimed, only the explicitly inventoried sets and source/ELF/old proof bytes.

## Actually executed current code gates

- `cargo test --workspace --locked --no-fail-fast`: **1856 PASS / 0 FAIL / 11 unchanged
  opt-in ignored / 46 targets**, including TUI526 and actual PTY58. Initial interrupted
  and intermediate1855 gates are historical; only completed final-workspace counts here.
- `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked --
  -D warnings`; locked and ordinary debug builds; debug `--help`: PASS.
- `cargo build --locked --release`; release `--help`: PASS. Actual release tests:
  recovery_startup `isolated_`2, terminals `term01_actual_binary_session_terminals`1,
  pty `pty_panic_restores_terminal`1, pty_t39 `vis31_`2, S07 equal-view1, VIS38 DCP
  equal-active archive1, memory_bounds AUD32 actual binary1: **9/9 PASS**, no deadline,
  threshold or baseline change. Exact argv/times/full logs are in the gate archive.
- VIS31 stable one-second idle: zero CPU ticks and terminal writes; 165/250Hz p95
  input latency1197/697µs, provider-burst p957760/8117µs. Existing100ms guard intact.
  S07 active retained24232 bytes at0/3000 archive rows; queue lag0/children0. AUD32
  HWM20348→22304KiB while archive grows to3000 pairs; children0. TERM01 raw/control/
  VT/resize/focus/shutdown/crash/no replay PASS, idle300ms0 CPU ticks.
- Root Python63; nested question fixture2; retainer14 included in root; JS syntax;
  docs58 tasks/173 specifications; progress structure and diff whitespace: PASS.
- Changed-file warning-only inventory:13 files25564 physical lines; no changed file
  exceeds5000. `question_view.rs`1142 and external owner tests764 retain their real
  responsibility seam. No cosmetic split/minification/test API widening.

## Observed external retention blocker — bounds NOT waived

Before gate/audit/report delivery, owned evidence allocation was
**1072304128 / 1073741824 bytes**, remaining **1437696**. Default full004 measure
passed bounded encode/whole inverse but refused quota: archive1913176 bytes,
receipt2052 bytes, rounded allocation **1921024**; shortfall **483328** already.
Stronger preset9 extreme with unchanged16 MiB dictionary produced the exact same
1913176 bytes; the experimental parameter/code was removed. In-memory attribution
finds PNGs alone compress to1752548 bytes, already314852 beyond that headroom.
Reordering tiny metadata cannot resolve this; no mask/reencode/predictor was used.

After complete gate/audit retention, before this human receipt, owned allocation is
**1072607232 / 1073741824**, headroom **1134592**. The factual receipt consumes a
small additional allocation; the full004 deficit therefore only increases. TMP
scratch/original captures are separately accounted and are not durable proof offload.
Per-file/encoded16 MiB, TAR/logical128 MiB,2000 members, dictionary16 MiB,
decoder32 MiB, Python hard AS384 MiB and owned1 GiB caps remain unchanged.

There is no `vis37-form-r4-proof.tar.xz` or inventory: **no current004 durable pack
or independent inverse is claimed**. Deleting old proofs, offloading, rewriting old
receipts or raising the cap is explicitly unauthorized. The unavailable dependency
is permission/resource for enough durable byte-exact current and remaining mandatory
proofs. Complete independent code/gate delivery has proceeded despite that blocker.

T44 stays ACTIVE_NOT_PASS with this concrete blocked gate; no task finish or final
R6/V09 PASS. All remaining frozen approval/DCP/child/Shell/Terminals/MCP/retry/auth
and other VIS outcomes remain unclosed. Once retention is legitimately unblocked,
retain the existing004 originals without replay, diagnose full-frame/cursor residuals
through their owners, continue every mandatory outcome, then perform final current
R6/V09. No new paid campaign, user/authoring config/env change or unknown-effect replay.
