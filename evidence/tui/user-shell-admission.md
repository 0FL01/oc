# T44 / VIS12 — structured direct-user Shell admission

## Result and scope

The mandatory user-Shell dependency of shared prompt-input history is now
functionally qualified. Actual `!` composer commands use the existing Shell
supervisor, not interactive-terminal bytes, model tool calls or a new executor.
Normal/root, accepted Shell, slash and retained-clear paths share the same bounded
application/Db preference. Whole R1–R6/VIS01–VIS45/T44 remains ACTIVE/NOT_PASS.

The frozen VIS12 supplement, `T44_CONTRACT_AMENDMENT.md:2235–2238`, explicitly
excludes full files/agents/skills/pasted/**mode** history parts, Mini's200-entry
ring, donor JSONL storage and cross-process synchronization. Native recalled text
therefore does not automatically restore Shell mode; the actual original does.
That difference is observed, not hidden or newly made a mandatory dependency.
Strict full styled-cell/PNG parity is **NOT_PASS**.

## Owners and invariants

- `CoreApp::request_user_shell` uses the existing bounded inbox and a typed,
  nonblocking receipt with captured Fresh/Existing selection. QueueFull,
  InputTooLarge, Shutdown and refusal remain non-success. Pending cancel is an
  atomic owner flag, not a second queue or model turn.
- `application/user_shell.rs` pumps one pinned admission future. Existing Ask
  replies, safe queries, cancellation, fatal cleanup and transitions remain
  reachable. Runtime single-flight, generation, Location/root/child checks, local
  model/variant validation and agent policy still apply. No provider credential
  preparation/discovery/model request substitutes for a user command. Deny/Reject
  use typed TrustRefused; cancellation is typed Cancelled.
- `runtime/user_shell.rs` shares `admit_tool`, canonical Shell parsing and pinned
  CWD preflight/Ask rechecks. It reserves the existing bounded Jobs slot BEFORE
  ONE existing-Db transaction records exact history, NULL-turn Shell intent, job
  provenance, explicit data-only USER command/event, captured selection and fresh
  root/Location/adoption marker. Any fault rolls everything back before effect.
  One non-Clone committed launch token is consumed by the SAME supervisor; recovery
  never recreates it. No fake provider graph, schema or separate executor exists.
- The receipt acknowledges only after commit and before launch. Post-commit launch
  failure is an owned truthful outcome, not a rejection/replay invitation. Model
  launch, process-group identity, capture/redaction/artifacts, limits and reaping
  remain shared. Terminal NULL-turn operation/presentation commits with job outcome.
  Durable completion is explicit untrusted USER data, not ToolCallResult; bounded
  output follows captured metadata for visibility. Legacy model notices unchanged.
- `app/user_shell.rs` owns a separate captured receipt/revision, with no optimistic
  history/clear or fake TurnStarted. Matching acceptance promotes fresh/current
  view to Idle, preserves newer edits and clears only unchanged non-cancelling
  input. Refusal preserves the editable command; quit reconciles the same receipt.
  `!` requires editable visual offset zero/no overlay or autocomplete. Esc, empty
  Shell Ctrl+C and start Backspace exit without editing; pending Esc cancels.
  Literal `/bin/...`/`@...` do not open model suggestions. Fresh-Home approval events
  reach the existing consumer before root creation, using the SAME pending wake.

No new store/registry/timer/production JS host, selection API rewrite, RAW rewrite,
raised threshold, permission exception or unknown-effect replay. `.opencode`, env/
credentials, GOAL, acceptance and Cargo.lock untouched; AUTH06 and original24/24 /
Go13/24 ledgers unchanged.

## Current-source association

Reviewed implementation/evidence commit
`999a5880a8b9c17f3824eaf7463b05f45fd12d01` is PUSHED to
`origin/agent/oc-rust-port`; [delivery receipt](user-shell-admission-delivery.md).

Capture locks use base `07437fc1c95a3680b59a9b399b4cfe2ce979e9a1` (external docs-only
T58 commit preserved), tree `253569d28eb7c391a8e9e13b7bc40dd7a8446900`:

- Dirty diff: `18221e0b609e1f207863a59e9f1e0a96671a7b58e10bbdc373b9c5f6c2724ffb`.
- Source manifest: `48ac13c9b446f68016600d4000d8a8e65037898aaa98da64cbd5090be860509b`.
- Actual `cargo build --locked` debug ELF:
  `b0aac07fc69f41fe65572cc38af2ae9ea2aded7e87f1000d1ef35eda18aca001`.
- Pinned running original: `2670273ff17da96f85c5826ced57aa1b368754fa` / v2.0.12.

Existing capture inventories include new owner modules/tests. Later factual docs
do not rewrite immutable artifacts or imply whole-task PASS.

## Actual effects and paired frames

Final [user-shell-002/](user-shell-002/)80×24 and
[user-shell-003/](user-shell-003/)120×40 each have both11 stages, ALL22 full grid+PNG
comparisons DIFFERENT, both PASS_BEHAVIOR_ONLY / OBSERVED_USER_SHELL. Real stages:
Home → `!` → Esc → reenter/exact input → completion → text-only recall → observed
mode difference → explicit second Enter → clean exit/relaunch → restart recall.
Each side: exactly2 explicit commands/effects; zero Responses/title/model-tool/MCP
calls; no restart replay. Native:2 distinct completed NULL-turn operations, zero
model turns, exact deduplicated global input. Its ACTUAL process reads only owned
SQLite URI modeRO BEFORE effect and asserts committed history/started intent/NULL
turn. Original uses actual `session.shell`; its RAW/schema is not decoded.

Existing actual-binary `pty_t39/user_shell.rs` additionally proves fresh-root
promotion, Allow/Ask Once/Reject/Deny, no intent/history/effect before approval,
refused-command retention, zero Responses/title or additional discovery relative
to startup, terminal restoration and restart without replay. Startup discovery
remains separate, not claimed absent.

Same-source regressions:

- [user-shell-regression-tool-preview-001/](user-shell-regression-tool-preview-001/):
  both27 stages,6 fixture requests/4 effects/3 MCP calls/17-byte model-Shell effect
  through expansion, switch/reopen/restart.4 EQUAL/50 DIFFERENT strict comparisons
  plus4 native-only resource-detail captures.
- [user-shell-regression-temporal-001/](user-shell-regression-temporal-001/):80×24
  WebGL/default/collapsed/supported, six owners qualified each side. Required default
  zero cycles, no phantom/lost caret/draft/replay. Independent audit in
  [user-shell-regression-temporal-matched-001/](user-shell-regression-temporal-matched-001/)
  validates144 opaque full PNGs/72 matching-phase pairs; all160 full comparisons
  DIFFERENT. No mask/crop/clock/CSS/parser/command rewriting or blink claim.

## Checks

Current fmt-check, strict locked workspace Clippy all-targets `-D warnings`, locked
workspace tests, locked/ordinary debug builds, locked release and debug/release
help PASS. **1761passed/0failed/11 unchanged opt-in ignores** across46 results:
TUI471/Core34/adapters682+1ignore/binary98/T39 PTY55/runtime120/subagent39. Focused
cases cover atomic fault/recovery, bounded receipt/cancel, actual Allow/Deny/Ask/
five refusals, shared real Jobs and UI authority. Full output is retained at
`/home/opencode/.local/share/opencode/tool-output/tool_11cf0ebff001sV7cSQS3DCo73i`.

Additional current actual-binary gates, unchanged bounds:

- VIS31 idle1.003495756s,0 CPU ticks/no terminal bytes.165/250Hz p50/p95/max
  2.257/3.366/3.986ms and5.904/7.067/8.801ms; provider burst17.600/29.683/30.710ms
  and16.703/27.290/29.036ms, below100ms.
- Equal-view0→3000 archive RSS66852→68608KiB/PSS64527→66291KiB, retained22896bytes/
  152rows/cache7517bytes unchanged, queue peak7/no lag/children0/settled live0.
  Frames85/87, elapsed2400/2391ms; no archive-dependent retained growth.
- AUD32 equal active context8→3000 archive peak RSS57108→57608KiB (+500KiB), peak
  PSS54276→54835KiB, children0/threads8, Db942080→152944640bytes; unchanged64MiB
  delta bound. Small process exited before final PSS sample (0); peak independently
  observed, not substituted.
- Actual release startup success/busy/lock/credential/query/selection/config/unsafe
  root and GET-only unauthorized/forbidden/oversized/slow/absent/present fixtures
  PASS, no Responses and restored terminals as required.

## Diagnosed attempts and remaining contract

Failed `user-shell-001` remains: shared fixture project mixed origin boundary files;
original Up already restored Shell mode, so extra `!` failed. Origin-owned bounded
files/actual mode observation fixed the probe, not product/comparison criteria.
PTY exposed fresh-Home approval/pending wake and buried output; owner fixes were
followed by full gates. Ordinary regression initially refused80×24 before launch
under unchanged120×40-only admission, then qualified at120×40. No disabled tests.

Continue every remaining frozen T44 outcome and final current-source R6/V09.
This closes a mandatory functional dependency, not whole VIS12/pixel/T44 acceptance.
