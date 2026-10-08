# T44 — typed direct-user Shell presentation

## Result and scope

Direct-user Shell admission, running receipt and durable completion now project to
one standalone command/output block, rather than ordinary USER technical notices.
This consumes the [landed admission owner](user-shell-admission.md). It does not
manufacture an assistant turn, provider tool call/result or conversation checkpoint.
Whole R1–R6/VIS01–VIS45/T44 remains ACTIVE/NOT_PASS; strict pixel parity is NOT_PASS.

## Owners and preserved invariants

- `HistoryMessage.user_shell` contains bounded typed command/output/status facts.
  `storage_shell_jobs.rs` resolves the exact existing job/message association and
  requires same-session explicit `user_shell_admitted`, NULL-turn Shell operation
  and valid versioned provenance/outcome. Admission input uses the adjacent message
  event in the SAME transaction; completion uses the existing job's `message_id`.
  Notice-like prose, a NULL turn alone or a copied message never classifies origin.
- The existing producer publishes `ShellNotice.user_requested`; current-branch
  visibility is resolved by `CoreApp::history_message` through the SAME bounded
  History inbox/query and `conversation_messages` view. A delayed notice outside
  the newest100 rows is not misclassified as a model warning. Hidden/foreign rows
  do not expose a RAW toast. Model background notices retain their existing path.
- `history.rs` creates a UI-only Shell card over the actual operation. Pending
  receipt uses its real operation ID and no invented durable message ID. Consumed
  input retains its actual ID/sequence and bounded operation identity, but paints
  no duplicate block. Result-only detached updates retire the matching spinner;
  late receipt cannot revive it. Identity replacement/sorted insertion preserve
  paging order, caps and gaps; evicted older records are not appended at the tail.
  Selective refresh never duplicates an executing assistant's durable/live parts.
- `tools.rs` shares the existing Shell renderer. Literal output/status-like prose
  remains data; generated cancel/timeout/nonzero statuses follow the pinned TUI
  owner. Command/output viewing limits remain distinct from producer/capture loss.
  Legacy prepared model envelopes are never reinterpreted as process stdout;
  fallback uses bounded recent stream facts and no unvalidated cold reference.
  Terminal-untrusted text is sanitized; no standalone CWD or assistant footer is
  fabricated. Indexed and materialized rendering are checked at painted cells.
- Native admission after Undo/Revert atomically applies the EXISTING branch
  transition before history/intent commit. It invalidates the old Redo without
  creating an LLM turn/context point. Faults roll back that transition too. Shell
  input/result is not an ordinary Undo target or reverted user-message count;
  ordinary legacy/missing-tip refusal and whole-tail Redo semantics remain intact.

RAW text/IDs, job execution/outcome records, common permissions/Ask/Deny, redaction,
capture/resource limits and no-unknown-effect replay remain unchanged. No new
schema/store/registry/executor/poll/timer, production JS host or model request.
`.opencode`, env/credentials, GOAL, acceptance and Cargo.lock untouched; AUTH06 and
original24/24 / Go13/24 ledgers unchanged.

## Current-source association

The four built capture locks independently retain base
`6be8b57a20b42566f6c71729ef84bc04404fa4cc`, tree
`6483eddee54b9bde6f2b9442e80b9e7ee54f4688` and:

- Dirty source diff: `b29fc9f85ca109439cadc283aff8e0ab4f27a261e19c13f2f3438c83bfca3acd`.
- Source manifest: `806645ffa8554c15e11e2caa29b913dbdb396160580cc96b0e7a480ba84eaf65`.
- Actual locked debug ELF: `9decda33dc8124116afd89d3c91e1189c27a1815f5399bb8a4bb76eeb5b3b19e`.
- Pinned running original v2.0.12: `2670273ff17da96f85c5826ced57aa1b368754fa`.

All use `--build-oc true`; later factual docs do not rewrite immutable artifacts.

## Actual paired processes, effects and frames

[user-shell-presentation-007/](user-shell-presentation-007/)80×24 and
[user-shell-presentation-006/](user-shell-presentation-006/)120×40 each retain
12 stages per side and ALL24 strict grid/PNG comparisons DIFFERENT. Both actual
processes qualify2 explicit commands/effects, zero Responses/title/MCP calls and
restart without replay. Native has2 completed NULL-turn operations and no model
turn. Its ordinary launched process reads only the owned SQLite URI modeRO before
effect and verifies committed exact deduplicated global history/started intent.

The new running stage observes a genuine supervised command during a finite
three-second fixture hold AFTER the admission witness and BEFORE effect, not a
production timer or synthesized phase. Running shows the command/spinner;
completion shows the command and real output once, without native-admission/RAW
notice blocks or a generated successful-exit line. The second explicit command
shows exactly two blocks. Original uses actual `session.shell`, not decoded RAW.

Native text-only recall versus original restored Shell mode remains disclosed and
explicitly outside frozen VIS12's full parts/mode/Mini scope (2235–2238). It is not
a newly invented mandatory gap or a claim that arbitrary history parts are restored.

Independent same-source regressions:

- [user-shell-presentation-tool-regression-001/](user-shell-presentation-tool-regression-001/):
  both27 stages,6 fixture requests/4 effects/3 MCP calls/17-byte model Shell effect;
  no reexecution after switch/reopen/restart.4 EQUAL/50 DIFFERENT strict comparisons
  and4 native-only authorized capture-detail snapshots.
- [user-shell-presentation-temporal-001/](user-shell-presentation-temporal-001/):
 80×24 WebGL/default/collapsed/supported, six genuine input owners each side;
  default zero cycles, no phantom/lost caret/draft/replay. Independent
  [user-shell-presentation-temporal-matched-001/](user-shell-presentation-temporal-matched-001/)
  audits144 opaque full PNGs/72 matching-phase pairs,17,694,720 pixels per side.
  QUALIFIED_CURSOR_BEHAVIOR_ONLY, all160 full comparisons DIFFERENT. This default
  control is not blink-cadence proof. No mask/crop/clock/CSS/command rewriting.

## Current checks

Fmt-check; strict locked workspace Clippy all-targets `-D warnings`; full locked
workspace tests; locked/ordinary debug and locked release builds; both ELF help;
actual release startup and GET-only discovery fixtures PASS.
**1770 passed/0 failed/11 unchanged opt-in ignores across46 results**: TUI476,
Core34, adapters685+1ignore, binary99, T39PTY55, runtime120, subagent39. Full output:
`/home/opencode/.local/share/opencode/tool-output/tool_11d8c49e4001muYwZVQNO2MiE4`.
An earlier combined cold compile/test command timed out and is not a PASS claim;
the completed warm run qualified the unchanged source, without altered deadlines.

Closest cases: actual Allow/Deny/Ask/effects and exact Core query; spoof/foreign/
missing-admission/recovery/prepared-envelope bounds; busy/late/branch-hidden native
notice versus unchanged model notice; detached paging/eviction/late receipt/cells;
Undo/Revert, rollback and restart without RAW change or replay. Python unittest
discovery47 PASS; Node capture/tool-preview/user-Shell syntax and fixture py_compile
PASS. Source whitespace, documentation/journal checks and advisory size check PASS.
Application facade is5,422 physical lines (+17); shared admission/generation/query
ownership remains coupled, with query dispatch versus transitions the next seam.

Additional actual-binary checks preserve unchanged thresholds:

- VIS31 idle1.007706334s/0 CPU ticks;165/250Hz p50/p95/max2.623/7.190/7.645ms and
  5.950/9.307/9.773ms; provider burst15.226/21.098/26.967ms and18.157/35.195/39.006ms,
  queue peaks58/96/no lag/settled live0, all below100ms.
- Equal-view archive0→3000 RSS68620→67140KiB/PSS66361→64907KiB, retained22896bytes/
  152rows/cache7517bytes unchanged, queues7→6/no lag/children0, frames89/93,
  elapsed2343/2451ms. No archive-dependent retained growth.
- AUD32 archive8→3000 peak RSS56336→58072KiB (+1736KiB), peak PSS53704→55436KiB,
  end PSS53704→53780KiB, children0/threads8, Db942080→152944640bytes;
  unchanged64MiB bound. Release startup/discovery restore terminals and make no
  Responses calls, including refused/absent/oversized/slow fixtures.

## Diagnosed attempts and remaining contract

Untracked presentation001–005 remain diagnostic, not delivery evidence: initial
RAW admission block, extra leading blank, then pre-review-fix frames and a
`--build-oc false` association respectively. Four confirmed review issues were
fixed and rerun, not waived: detached completion, older-row reordering, delayed
notice origin and staged branch admission. The rollback test's initial trigger
targeted UPDATE on an absent preference; INSERT fault injection now proves the
actual admission upsert's complete rollback. No test or bound was disabled.

Completed native/reference frames still differ by26 styled cells/cursor equal:
16 fresh-session title cells plus10 blank-padding foreground cells atx3–4/y2–6.
The reference fallback really is `Untitled session`; do not globally rename it to
`New session`. Fresh backend title/promotion and actual Shell padding style need
their own owner fixes and new real frames. Continue those and every remaining
frozen prompt/pacing/scroll/profile/model/live-selection/tabs/session/Revert/
compaction/files/approval/question/DCP/child/Shell/Terminals/MCP/retry/auth outcome,
then final current-source R6/V09. This major slice is not full T44 acceptance.
