# T44 — fresh tab title and standalone Shell padding

## Result and scope

The26 completed-frame styled-cell differences in the previous
[typed Shell slice](user-shell-presentation.md) are closed in real paired frames:
16 title cells and10 blank-padding foreground cells. Completed, recalled-normal
and second-completed now match full styled cells, PNG and cursor at80×24 and120×40.
Whole frozen R1–R6/VIS01–VIS45/T44 remains ACTIVE/NOT_PASS. Other stages differ.

## Existing owners and invariants

Pinned OC2 creates a fresh Shell session without a durable title and does not
generate one (`packages/app/src/new-session/composer-adapter.ts:87–96,145–148`,
`packages/core/src/session.ts:246–280`, `session/session.ts:173–214`). Its TUI
`context/session-tabs.tsx:114–117,143–148,176–197` persists a tab-only `New session`
fallback on Home promotion; the live durable title wins. `Untitled session` remains
the correct ordinary title-less fallback. No global replacement is appropriate.

- Existing Location-scoped/CAS tab preference now carries at most16 aligned boolean
  `new_session_titles` flags, separate from durable session metadata. Empty means
  legacy/all false. Nonempty misalignment is rejected before effects; the existing
  strict v1 parser,4096-byte bound, root/Location validation and CAS remain intact.
- Projection, failed-middle-tab pruning, pending adoption, picker append/adoption,
  close/delete and source move preserve flag indices. All-false removal results
  normalize to empty. No new table/schema/store, title event or model request.
- Existing Home-promotion/receipt and restored views retain `new_session_tab`;
  history with titleNone does not erase it. Explicit/model titles take precedence.
  Normally opened title-less roots keep the unchanged untitled fallback.
- `BlockFrame` keeps model-tool padding unchanged. Only direct-user Shell padding
  inherits the terminal foreground, independently of its muted output child, as
  pinned `ShellDisplay` does. No fake RGB/theme role or output-text transformation.

Permissions, generations, RAW history, Shell execution/outcomes, bounded paging,
redaction, no unknown replay and all security/resource guards remain unchanged.
No `.opencode`, env/credentials, GOAL/acceptance, Cargo.lock or auth-ledger changes;
original24/24 and Go13/24 remain unchanged. No paid generation or live API call.

## Current-source association

All four built capture locks independently retain base
`6644f4a867cf2d6e535c76c5c76fb39ac0618c3a`, tree
`0efc3f3886fce8b65bdd2d79b06c8f821e39c125` and:

- Dirty source diff: `4eb4fce24095377dd9bbcaaba174fdb1b35dce9086aa754400c475900a297693`.
- Source manifest: `79af9540f7bef8c6b20017305448c550df44f6415a7fc9b073de388667147a8e`.
- Actual debug ELF: `88d3d89693d69e753f3e4e034da65a5e20c0cf8bd68c22ed03ea3629f6345508`.
- Running pinned original: v2.0.12, `2670273ff17da96f85c5826ced57aa1b368754fa`.

Each uses `--build-oc true` / `cargo build --locked`. Factual docs do not rewrite
immutable captures. Implementation is this reviewed source diff; Git records its
commit and delivery, not a guessed future SHA.

## Actual paired processes and full frames

[user-shell-tab-title-002/](user-shell-tab-title-002/)80×24 and
[user-shell-tab-title-003/](user-shell-tab-title-003/)120×40 each retain12 stages,
6 EQUAL/18 DIFFERENT strict comparisons. The three completed-stage pairs each
have0 differing styled cells, identical PNGs and cursor. Final native completed
PNGs were independently viewed at both geometries. No masking/cropping/alignment
or source-golden substitution.

Both sides execute exactly2 explicit commands/effects,0 Responses/title/MCP calls,
and no restart replay. Native has2 NULL-turn operations/no model turn; its launched
process reads owned SQLite URI modeRO BEFORE effect to verify committed exact
deduplicated global history/started intent. The finite three-second fixture hold
is a genuine running job, not a product timer or a fabricated phase. This hold
has no stdout until completion and does not qualify live streaming output.

Native text-only recall versus original restored Shell mode remains disclosed,
explicitly outside frozen VIS12's full parts/mode/Mini scope (2235–2238).
Remaining differing grid cells, with equal cursor, are:

| Stage |80×24|120×40|
|---|---:|---:|
| Home |6|46|
| Escape |6|6|
| Mode |87|84|
| Input |78|78|
| Running |56|92|
| Recalled |111|150|
| Recalled-mode |73|112|
| Restarted-home |45|46|
| Restarted-recalled |116|116|

Independent same-source regressions:

- [user-shell-tab-title-tool-regression-001/](user-shell-tab-title-tool-regression-001/):
  both27 stages/6 fixture requests/4 effects/3 MCP calls/17-byte model Shell effect;
  no reexecution switch/reopen/restart.5 EQUAL/49 DIFFERENT strict comparisons,
  plus4 native-only authorized capture-detail snapshots. Model padding unchanged.
- [user-shell-tab-title-temporal-001/](user-shell-tab-title-temporal-001/)80×24,
  WebGL/default/collapsed/supported, six input owners both sides: zero cycles
  appropriate to DEFAULT, no phantom/lost caret/draft/replay. Independent
  [matched audit](user-shell-tab-title-temporal-matched-001/) checks144 opaque full
  PNGs/72 phase pairs/17,694,720 pixels per side; QUALIFIED_CURSOR_BEHAVIOR_ONLY,
  pixel NOT_PASS/all160 comparisons DIFFERENT. DEFAULT is not blink-cadence proof.

## Current checks

Fmt-check; strict locked workspace all-target Clippy `-D warnings`; full locked
workspace tests; locked/ordinary debug and locked release builds; both ELF help;
actual release startup19 cases and GET-only discovery8 cases PASS.
**1772 passed/0 failed/11 unchanged opt-in ignores across46 workspace results**.
The same log also contains one separately focused T42 test; it is not counted as
a1773rd workspace test:
`/home/opencode/.local/share/opencode/tool-output/tool_11dd44088001SAtkGokk2j0C6I`.

Closest tab-deck14, UI user-Shell7 and binary lifecycle48 cases pass. New scenarios
cover legacy decoding, aligned bounded flags/CAS refusal/restart/title precedence/
delete, failed-middle-tab projection, accepted Home promotion and title-less refresh.
Painted indexed/materialized Shell cells still agree; model padding is preserved.
The first full run exposed the old T42 Home expectation (`Untitled session`);
its single wait now expects the required promoted label. Ordinary READY remains
unchanged, as do actual root/turn/message/binding/draft/request/final-title effects.
Focused actual PTY and all34 T42 tests then pass. No assertion/threshold disabled.

Python unittest discovery47, Node capture/tool-preview/user-Shell syntax and fixture
py_compile PASS. Source whitespace/docs/progress/advisory code-size checks PASS.
Application facade5425 lines (+3): shared generation/worker admission remains
coupled; acknowledged query dispatch versus transitions is the next natural seam.

Additional actual-binary checks preserve the unchanged thresholds:

- VIS31 idle1.008308704s/0 CPU ticks;165/250Hz p50/p95/max2.702/12.906/15.118ms and
  5.887/10.770/13.513ms; burst14.856/31.167/37.656ms and22.737/62.310/66.148ms,
  queue peaks87/162/no lag/settled live0, below100ms.
- Equal-view archive0→3000 RSS66840→68028KiB/PSS64552→65723KiB, retained22896bytes/
  152rows/cache7517bytes unchanged, queues7/7/no lag/children0, frames90/88,
  elapsed2359/2364ms.
- AUD32 archive8→3000 peak RSS56040→56344KiB (+304KiB), peak PSS53240→53531KiB,
  end PSS47496→47501KiB, children0/threads8/Db942080→152944640bytes;
  unchanged64MiB guard. Release fixtures restore terminals/no Responses calls.

## Remaining contract

Diagnostic title001 remains untracked, not delivery evidence. Completed26-cell
gap is closed, not the whole episode. Next resolve the actual running Shell
status/footer and one running-card foreground cell through existing owners, then
every remaining frozen prompt/pacing/scroll/profile/model/live-selection/tabs/
session/Revert/compaction/files/approval/question/DCP/child/Shell/Terminals/MCP/
retry/auth outcome and final current-source R6/V09. No full T44 finish on this slice.
