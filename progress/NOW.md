# NOW — актуальный handoff

State updated: 2026-10-17T19:37:39+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12, включая Approve (VIS36), question (VIS37), DCP (VIS38), child/Subagents/Shell/Terminals (VIS39), MCP modal/startup (VIS19/VIS40), tab hover-marquee (VIS41), plugin/service errors и чистый диалог без diagnostic spam (VIS42), компактный live-preview indicator (VIS17), скрытые synthetic tool-preview markers и Generic/MCP expand (VIS16/VIS17), provider retry (VIS43), Middle Click tab close (VIS44), tool-output phantom caret / prompt blink (VIS16/VIS31), историю промптов Up/Down и установку каретки мышью (VIS12), Codex/OpenAI connect/accounts/browser/device/key auth (VIS45), Code Mode execute inline calls/detail dialog (VIS46)
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Full frozen R1-R6/VIS01-VIS46 and supplements remain in spec, T44_CONTRACT_AMENDMENT.md, ACCEPTANCE.json and SAFETY_REGRESSIONS.md; navigation is not reduced acceptance. Owner explicitly resumed2026-10-07: full closure, realtime todos, checked commit/push per major slice. First clean-dialogue e2120e418 qualified1720/0/11; pinned attempt005 eleven stages per side/22 stable frames, ALL22 strict comparisons DIFFERENT. Next concise typed submitted-refusal facet qualified current1721/0/11 plus strict Clippy/debug-release/release-startup; safe captured detail and pre-effect draft preserved. Receipts evidence/tui/vis42-clean-dialogue.md and vis42-request-refusal.md. Whole VIS42/V09/T44 is not PASS. Continue ordered structured tool-body/guidance/ref/capture and Generic-MCP VIS16/VIS17, remaining VIS42 styled/detail integration and every approved prompt/caret/blink/pacing/scroll/profile/model/live-selection/tabs/session/Revert/compaction/file/approval/question/DCP/child/Shell/Terminals/retry/MCP/auth slice. Consume landed backend owners without whole-task done cycles. Actual effects plus running OC2/native full styled-cell/PNG/cursor proof, not own goldens/historical PASS; disclose native differences. Four crates/one owner/immutable RAW/permissions/redaction/caps/no unknown replay; no new registry/store/poll/paid campaign/user config edits. AUTH06 deferred; original24/24 and Go13/24 unchanged. Owner-approved 2026-10-14 VIS46: Code Mode execute producer facts/live ToolCallProgress, inline nested-call rows and execute detail dialog (Code/Output/c/o/esc) with paired qualification after T59 finish; supersedes only T60 generic-card difference, NOT_RUN. Finish only after all mandatory T44 outcomes and final current-source R6/V09 gates.

Последний checkpoint этой задачи (проверить актуальность по Git):

# Clipboard PNG — delivered T44 slice

## Result

Implementation: `a4b456652d472e3d863abf390284056ca7b5e315`, PUSHED to verified
`origin/master` (`github.com/0FL01/oc`); base `54d1b3f58763e9814e11d5f8d4c083ac07e58b99`.
Goal: `docs/goals/2026-10-17-clipboard-images.md`, complete for this slice only.

Ctrl+V/effective `prompt.paste` reads host PNG then UTF-8 text. Empty bracketed
paste uses that reader; nonempty terminal paste remains text-only. One captured
cancellable job uses existing scheduling/shutdown, a shared 1s read deadline,
bounded output and helper kill/reap; Enter cannot submit a partial paste.

Draft `[Image N]` markers carry separate shared PNG bytes, edit atomically across
Unicode/selection/delete/Undo/Redo and survive unfinished draft/Location recovery.
Accepted snapshots own their images; later edits survive acknowledgement. Live
and reopened messages show `file clipboard`. Full PNG/range/capability admission
and actual byte limits apply; base64 is not estimated as ordinary text tokens.

The initial typed journal/user anchor commit in the existing acceptance SQLite
transaction before acknowledgement. History transfers metadata only; explicit
Revert/Fork restores original PNGs from existing HOT/RAW after DCP forgetting.
No schema migration, blob store, Cargo dependency, provider wire, shared service,
user config edit or paid generation. Original limits/security/RAW guards remain.
CODE_MAP and embedded configuration manual describe the implemented path.
Existing large application/turn owners retain their invariant ownership; query
dispatch/admission are the next natural seams, not unrelated splits in this diff.

## Checks

`CARGO_BUILD_JOBS=8 TMPDIR="$HOME/.cache/opencode-tmp" bash scripts/test.sh`: exit 0.
Fmt, strict workspace/all-target Clippy, doctests, locked debug build, help/version
pass. Nextest `218c6aa3-df14-487a-b3a6-b378af88ee8f`: **2062 PASS / 0 FAIL / 12
original opt-in skips**, no retries; 362.924s nextest, 399s whole command.
Log: `.local/evidence/clipboard-images/offline-gate-delivery.log`.

Seven new scenarios pass: actual ELF PNG/text/wire/chip/restart 3.003s; atomic
admission/continuation/RAW retirement/reopen/Revert/Fork 1.988s; invalid media and
unknown capability refusal 1.462s; editor/receipt/history and effective binding
0.009s each; helper caps/deadline/cancel/reap 0.016s; pending Enter/stale job 0.010s.
Existing media/provider/DCP/OSC52/terminal/model tests and CLI image refusal pass.
Earlier failure logs remain: the post-accept failure trigger now injects after
the new atomic seed, and three PTY fixtures observe idle before idle-only actions
instead of assuming visible echo/title proves completion. Assertions, timeouts,
workloads and production guards were not weakened.

Fresh paired actual binaries at 80x24 and 120x40:
`.local/evidence/clipboard-images/fresh/{80,120}/{upstream,oc}/`.
All four behavioral checks PASS: draft has no generation; exact PNG reaches fake
Responses; sent chip and natural exit verified. Private headless Wayland clipboard
never accesses the owner's desktop; no owned compositor remains after capture.
Supplementary official 2.0.31 reference is source-qualified against supplied
`.local/opencode`; global pinned 2.0.12 admission is unchanged.

All **eight unmasked full-grid/PNG comparisons DIFFERENT**, not whole-frame parity
PASS. Changed styled cells: 80 draft/sent 50/22; 120 draft/sent 50/21; all cursors
equal. Inspected image marker/highlight and file clipboard chip agree at both
widths. Remaining frame differences include Location path, hints and footer
version/time/TPS. Original frames/diffs are preserved, not masked or baselined.

## Risks

The unrelated soak is still **33.580s > 30s**, so its existing speed objective is
UNMET; no exception is granted. UI07 32.471s fits its sole owner-approved 40s
contract. Functional command success does not qualify those unmet budgets or
whole T44/READY. No unrelated soak optimization is included.

Linux PNG only: host `wl-paste` (Wayland) or `xclip` (X11) must exist; no auto
installation or ordinary SSH binary image transport. Raw image limits remain
8MiB each / 16MiB aggregate, with independent active-context/request guards.
Shell/slash image attachments refuse; durable Up/Down history remains text-only.

## Next

Feature objective complete. Debug executable is ready at
`/home/stfu/ai/agent-frameworks/oc/target/debug/oc2`; launch it from the desired
Location and use Ctrl+V with an image-capable model. The interactive owner alias
was not launched or edited. T44 remains active/incomplete; its remaining parity
and the separate unmet soak-budget work retain their existing contracts.


Ready (до 5): нет
Blocked: T27, T45, T57, T61

Done в журнале не означает READY всего продукта; см. GOAL.md.
