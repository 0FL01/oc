# T44 / VIS12 — shared durable prompt-input history

## Result and scope

Functional qualification of shared text input history is complete for existing
normal/root, local slash and retained-clear admission paths. Home, different
sessions and restart use one application/Db list, not conversation-window rows.
Newest50, exact consecutive deduplication, legacy filtering and effective history
bindings are implemented. Whole VIS12/R1–R6/VIS01–VIS45/T44 remains ACTIVE/NOT_PASS.
At this historical slice, structured user Shell was the next mandatory dependency.
It is now independently qualified in [user-shell-admission.md](user-shell-admission.md):
actual `!` commands, atomic pre-effect history/intent, Ask/refusal and restart without
replay. Raw PTY bytes and model shell tools still are not user commands.
Full mode/files/agents/pasted-part restoration and Mini are outside this narrow
history slice under the frozen supplement, not newly claimed features.

Implementation commit `0defd6f2adb71d93abb25286af72f237069206ee`
(`feat(tui): share durable bounded prompt input history`) is reviewed, verified and
PUSHED to the verified own branch. Whole task status is unchanged.
No schema, second store/registry, RAW-history read, production JS host, polling
clock, permission/resource relaxation or automatic tool effect was introduced.

## Owners and invariants

- Existing `Db` preferences own `tui.prompt_history.v1`. A bounded SQL read and
  streaming JSON visitor discard invalid entries and retain at most50 valid
  strings; malformed/oversized legacy values normalize to an empty list.
  Each input keeps the existing1MiB limit. The encoded read limit is derived from
  `50 * (6 * MAX_INPUT_BYTES + 3) + 2`, not a borrowed title/preview cap.
- Fresh and ordinary root admission append raw input/invocation inside the SAME
  accepted-turn transaction, before acknowledgement/provider execution. A history
  write failure rolls back acceptance. Resume, manual compression, child/synthetic
  owning operations do not re-append accepted input.
- `CoreApp::prompt_history` uses the existing bounded inbox and acknowledged
  owner query during idle and active turns. InputTooLarge, QueueFull, Shutdown and
  application/storage errors remain non-success. Scripted tests use the same50/
  dedup policy, never a production fake acknowledgement or session-row fallback.
- The existing Editor owns the active browsing snapshot, undo, edited-copy refusal,
  recent resolvable mention marks and native unfinished-draft return. Ordinary
  visual movement and raw-edge movement precede history, including remapped keys.
  The shared list is loaded once per browsing visit; Down back to the draft retires
  that snapshot. No full prompt-parts history or archive rehydration was added.
- Accepted built-in slash input is recorded before local dispatch, including the
  autocomplete acceptance path; completion alone does not invent a submission.
  Retained Ctrl+C clear uses the exact donor ECMAScript trim/UTF16 predicate:
  trimmed length at least20, or existing pasted/mention parts. History refusal
  preserves the draft and prevents dispatch/clear. Empty Enter does not append.
- Existing admitted config/catalog projects `prompt.history.previous/next` and
  legacy `history_previous/next`; canonical values win, final leader expansion,
  alternatives/none/false work. Dispatch uses the same effective values. Modal,
  autocomplete, approval/question, terminal/Shell and linked-child owners win.

## Current-source association

All four final capture locks physically agree on these inputs:

- Base: `7ef6a58312bcd60e97956dcc4942a57191a43f3a`;
  tree `b9ba8229f3da30be3f125bb0f9fb58dd73dabdc2`.
- Dirty source diff: `0c8f7dd15b4f968e2ab1bad8486fe052f659d9dc261c689ffc2f1a4a69148500`.
- Source manifest: `7e228ec249870aef9a4fbf53ec4140335c9a63f3864cbe6327913ca43c762eab`.
- Actual `cargo build --locked` native ELF:
  `792d437f64442ac5753596fe07a66c52c4dbd63f148b64c17ad04c4e3897872d`.
- Pinned original commit `2670273ff17da96f85c5826ced57aa1b368754fa`, executable
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
- Runner `01b12a6222075d338ddf33ede60c7f823d69a384f899aacba4d8fe535d151e15`;
  frontend `3d5960cdba4803110b9aafb5b59dac01faf62bc08d11f21c65b4df4e9651ebb5`;
  history probe `c7e968671a6f163f6a0aac855fc479724b88f8695e9705f706ddc934ed9110b0`;
  fixture `6009603a11968d9b2ae75a40cbfe63eb8a61cd8731226dacf8897055d95e3f8b`.

No Rust/capture-runtime edits follow the final qualification; documentation and
progress only. Captures retain immutable source inputs, actual commands, PTY keys,
provider wire, complete styled grids/PNGs/VT/cursors and unmasked comparison reports.

## Actual binary and paired proof

The mandatory ordinary test remains in the existing `pty_t39` Cargo target:
`vis12_actual_shared_history_home_restart_recalled_wire_and_active_owner` PASS
(1/0,2.94s). Three real accepted inputs include multiline, Unicode and literal
`@note.txt`; Home/new-session recall resubmits the exact latest bytes with a fresh
one-user context, consecutive dedup holds, and restart shares history. An existing
held SSE stream proves busy-root editable recall cannot create another request or
change the captured request. Real autocomplete owns Down until dismissal. No file
canary or function-call output appears in wire; normal exit restores the terminal.

Paired commands use the existing actual-PTY runner and isolated local Responses
fixture, with `--build-oc true`, `--geometry true --sample short --sidebar hide`,
`--tool-preview true --prompt-history default|remap`. Each run is individually
bounded to900 seconds; no paid provider or authoring configuration is involved.

| Final immutable attempt | Profile | Behavior | Full grid + PNG |
| --- | --- | --- | --- |
| `prompt-history-default-attempt-003` | 80×24, Up/Down | both PASS_BEHAVIOR_ONLY,25 paired stages | all50 DIFFERENT |
| `prompt-history-remap-attempt-001` | 120×40,F2/F3 + actual Ctrl+G p | both PASS_BEHAVIOR_ONLY,28 paired stages | all56 DIFFERENT |

Each side makes four explicit main requests and two title requests; zero model
effects before Enter, zero tool/MCP/Shell calls. Native read-only SQL observations
show exactly First/Second/Third in the global input list, four immutable user rows
after re-submitting Third in another session, unchanged list/RAW through restart,
then `/mcps` appended before the local modal. No canary file body enters requests.
Native previous/next and registered leader dispatch are qualified; old remapped
arrows do not recall. Original RAW/schema is not decoded or claimed native-verified.

Running original refuses an initial arbitrary nonempty scratch draft because its
history index/current-text check does not match it, and its forward boundary is
an empty prompt. Native deliberately browses from and restores the pre-browse
unfinished draft under the frozen native supplement. The probe captures the real
refusal, sends a real reference-only clear to exercise its remaining behavior,
and never imitates lost native draft or fabricates reference state. These differences
and full visual/cursor comparisons remain unmasked; behavior is not pixel PASS.

Normal regression `tool-preview-attempt-029`: both27 paired stages and two native
read-only resource frames;4 EQUAL/50 DIFFERENT +4 NATIVE_ONLY_RESOURCE_DETAILS.
Six provider requests, four effects, three real MCP tool calls, Shell1line/17bytes,
bounded/separate body-guidance/four presentations, unchanged artifacts/RAW through
real new/reopen/restart. Capture/paging/permission and literal-marker guards remain.

Temporal regression `cursor-temporal-attempt-033`: both six states qualified,
native cycles4/3/3/3/4/3, original3/3/4/3/3/3; exact draft/caret/shape/restoration,
zero phantom states and no replay. Native hover1165/Search512 parsed commands;
original Search25 commands is not claimed continuous repaint. The strict audit
`cursor-temporal-matched-014` checks144 opaque actual PNGs/88,473,600 fixed-owner
RGBA pixels,68 actual phase-matched pairs/136 full comparisons ALL DIFFERENT.
Runner152 DIFFERENT/16 UNMATCHED_TEMPORAL_SAMPLE; pixel_parity NOT_PASS. Earlier
default/steady/unsupported-sync/representative geometry proofs remain separate.

## Checks and measurements

Current `.local/t44-prompt-history-gates-final3-20261008.log` completes serial
jobs3/tests2/approved TMPDIR: fmt check, strict locked workspace Clippy all-targets
`-D warnings`, locked workspace tests, locked and ordinary debug builds, locked
release, debug/release help, actual release startup and discovery probes. All46
completed workspace records independently sum **1752 passed/0 failed/11 unchanged
opt-in ignores**: TUI469,adapter678,binary98,MCP41,PTYT39 53,PTYT42 34,CoreApp33,
runtime120,subagents39. Release3m22. Startup/refusal/lock/credential/config cases
retain expected exits, owner state/retry and terminal restoration; discovery
unauthorized/forbidden/oversized/slow/absent/present uses GET and no Responses.

Focused storage/Core/UI/config/PTY tests cover invalid/newest50/dedup/reopen/atomic
rollback, bounded query/errors, shared empty-window recall, undo/edit refusal,
UTF16 clear, slash failure preserving draft, remaps and current mention policy.
Manual-channel binary fixtures now ACK exact history requests using the real bound/
dedup policy on their SAME inbox, retaining original control/receipt assertions.

Extra `.local/t44-prompt-history-resource-gates-20261008.log` four tests PASS:

- Idle1.008442769s,0 CPU ticks/no new terminal output. At165/250Hz independent input,
  p50/p95/max4.011/8.220/9.654ms and5.986/9.193/10.538ms; during real provider burst
  19.033/39.582/40.837ms and15.867/24.510/27.246ms, below unchanged100ms bound.
- Equal-view0→3000 archive: RSS67008→66952KiB/PSS64749→64713KiB/HWM same as RSS;
  retained22896bytes/152rows unchanged, queues7/6 peak with0lag, children0,
  settled live text/reasoning/parts0, Markdown cache7517bytes under5242880 bound.
  Frames93/88, draw sums1156207201/1173732243ns, maxima60646896/57565341ns,
  CPU142/139ticks, elapsed2497/2511ms. No archive-dependent retained growth.
- Actual AUD32 8→3000 archive pairs with equal active context: peak RSS54768→57784KiB
  (+3016KiB), peak PSS52169→55161KiB, children0/threads8, Db942080→152944640bytes;
  unchanged64MiB delta threshold. No new timer, queue, poll or global RAW cache.

Repo Python, Node/Python syntax, source diff, docs/journal checks are re-run with
delivery. Captured padded `.txt` and raw `.vt` retain intentional whitespace;
only exact final capture prefixes are exempt from Git's whitespace diagnostic,
not source/docs/JSON or full-grid/PNG/cursor comparisons. No artifact is trimmed.

## Failed experiments retained and open scope

Initial binary/compaction manual channels lacked a history owner and deadlocked;
exact bounded test acknowledgements fixed fixtures, not production. Inherited
selection byte comparisons now exclude ONLY the mandated global key and separately
assert exact recorded entries/refusal retention. Busy alias checks use the existing
held stream instead of a two-second timing race. Recovery geometry uses real wheel
scroll in the transcript after global history legitimately owns Up; Page keys stay
inert outside Select and all wrapped-anchor assertions remain. No tests disabled.
Default001 exposed original scratch refusal;002 sent Ctrl+C at original's already
empty forward boundary and legitimately quit it; positive empty-paint observation
fixed the probe. Temporal032 had sparse502.5/450.1ms reference raster gaps and
305.7ms native restored gap with zero commands; strict audit refused qualification.
033 independently qualified unchanged source and unchanged criteria. No threshold,
CSS phase, parser, clock, frame, model ID or donor binary was rewritten for green.

Structured user Shell admission/history is now functionally qualified separately;
continue every remaining frozen
prompt/pacing/scroll/profile/model/tabs/session/Revert/compaction/files/approval/
question/DCP/child/Shell/Terminals/MCP/retry/auth outcome and final current R6/V09.
AUTH06 deferred; original24/24 and Go13/24 ledgers remain unchanged. No whole task
finish, acceptance reset, or external-blocker claim follows this functional slice.
