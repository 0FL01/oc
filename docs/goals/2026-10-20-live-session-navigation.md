# Goal: live session navigation and root background lifecycle

Status: complete
Source: пользовательские screenshots running Subagent/tab refusal; выбор «Полный background lifecycle»; четырёхчастный read-only audit; запрос «Делай копию плана в цель и итеративно реализовать и коммит пуш».
Last updated: 2026-10-20

## Objective

Клик открывает работающий child, root-сессии переключаются и выполняются независимо, а завершение background child автоматически продолжает живой idle root parent. Поведение и существующая визуальная модель сверяются с локальным OC2 TS `bc5ebf00a4c169ece10f276e6ce02eaf18c326e6`. Проверенные итерации доставляются обычными commits/push в verified `origin/master`.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: running Subagent открывается настоящим mouse click, включая spinner/stream redraw между Down/Up.
  - Source: первый screenshot и исправленный audit §B.
  - Acceptance: actual child transcript и существующий нижний Subagents/Shell/Terminals открываются до completion и без Ctrl+B; selection/drag/resize/overlay/foreign launch остаются безопасны.
  - Primary evidence: deterministic pointer regression и pre-Ctrl+B actual capture.
  - Status: verified
  - Evidence: running-click RED → deterministic PASS на 80/120/160, включая spinner/unpainted stream/current facts и same-cell card→notice source fence. Fresh current-source paired captures `.local/evidence/live-navigation-r1-120-03` и `live-navigation-r1-80-01`: both products pass all 17 stages; natural Down→spinner change→Up opens the real child before Ctrl+B/completion, original foreground/graph/effects preserved. Each `running-pane-qualification.json` is PASS_SCOPED_COMPONENT: complete 11-row lower pane, 1320/880 styled cells with zero differences, equal decoded RGBA pixels/geometry and hidden cursor shape/visibility. All frame producer commits verified (TS `bc5ebf00`, native `52a150cb`). Full unmasked scenes remain DIFFERENT; no whole-scene pixel claim.
- R2: accepted execution не блокирует root tab/New/Add/Sessions navigation и независимый запуск B при работающем A.
  - Source: третий screenshot, требование параллельных вкладок, audit §C.
  - Acceptance: одна TUI переключает A/B и создаёт B; оба stream прогрессируют до release; нет Cancel/Background/repeated Submit; linked и standalone read-only provenance не расширяется.
  - Primary evidence: existing binary routing tests + actual-binary PTY concurrency.
  - Status: verified
  - Evidence: one actual TUI A → /new → B → A/B/A, two physical provider streams advance before shutdown, exactly one request per root — PASS (2.17s). Actual T39 slash /new,/clear, leader, palette and Sessions navigate a held accepted turn while profile mutations still refuse — PASS (7.17s). Binary TUI 120 PASS includes parked-child discovery, same-root linked return, standalone-child provenance, pending model receipt close/delete preflight and typed child /new draft isolation; full TUI 621 PASS, strict affected all-target Clippy PASS.
- R3: running-target picker/live attach корректны в пределах существующих retention bounds.
  - Source: opening working session/child, audit §D.
  - Acceptance: retained window/draft сохраняется через WorkspaceChanged; available prefix появляется один раз; old T1/reordered snapshots не откатывают T2/controls; linked gap repair не теряет owner; truncated projection явно ограничен.
  - Primary evidence: native client ordering and shared adoption tests, actual running-target interaction.
  - Status: verified
  - Evidence: actual native running target RED `TurnBusy` → 2 PTY/native-owner picker PASS (3.54s), including real fork pending-adoption with unchanged deck revision and exact source/target cancel isolation. Native bridge ordering/owned receipts — 9 PASS. Snapshot→canonical→target replay, T1→T2 floor, linked staged gap/read failure, historical Source, and retained picker window through shared sync — binary TUI 116 PASS (4.18s). Gap repair retains existing compaction/Shell/MCP reconciliation. R2 actual held-turn Sessions and A/B reattachment now PASS; scoped paired qualification remains R6.
- R4: background tool contract явно описывает существующий async режим как TS.
  - Source: user background tool suspicion, audit §A.
  - Acceptance: optional Boolean, foreground default, explicit asynchronous/no-poll guidance в actual serialized tool definition; true не ждёт child terminal.
  - Primary evidence: actual provider definition/parser tests + existing background overlap integration.
  - Status: verified
  - Evidence: actual serialized provider definition test `r8_subagent_guidance_separates_automatic_assembly_from_caller_context` PASS; `tools::tests::subagent` — 3 PASS (omitted/false/true, invalid types, original arguments); compiled integration `background_children::background_child_posts_effects_and_completes_before_parent_response` — 1 PASS, 1.57s. Background launch engine/default/routing не менялись; automatic idle-root continuation остаётся R5.
- R5: terminal background child автоматически продолжает live idle canonical root parent без нового user input и без повторного child execution.
  - Source: выбор «Полный background lifecycle», audit §E/§F.
  - Acceptance: durable report → one bounded assigned batch → real parent request; active/user race оставляет один runner; cancellation/last TUI/Run release/shutdown suppress; original Source и current committed parent selection; fork/DCP/Undo/Revert chronology сохранена.
  - Primary evidence: storage/runtime/authority regressions + existing actual background capture provider witnesses.
  - Status: verified
  - Evidence: foundation `4d676301d` (865 adapters + 44 subagent PASS), actual Owner idle-root RED→GREEN with fresh committed beta selection/no new Submit, Owner lifecycle 12 + child resource 8 + shared runtime 11 PASS. Fresh paired `.local/evidence/live-navigation-background-120-01` and `live-navigation-background-80-01`: both products pass 15 lifecycle stages. Original parent is idle before child release; B then A each causes a real parent request (steps 3/4), matching provider completion and causal idle/notice witnesses; native has exactly two assigned reports, no child/effect replay. Both `background-component-comparison.json` report TARGETED_COMPONENT_PASS for complete cards/notices, styled cells, pixels and relative parent anchors. Frame producer commits verified; whole scenes remain DIFFERENT.
- R6: проверенные изменения закоммичены и отправлены в verified origin.
  - Source: последний пользовательский запрос.
  - Acceptance: relevant tests, full offline gate, scoped current-TS paired qualification и diff review; normal commits/push, remote HEAD verified.
  - Primary evidence: команды/exit codes, capture witnesses, commit IDs и remote HEAD.
  - Status: verified
  - Evidence: runtime/UI through `52a150cb` and checked capture delivery `e0ecb26a9` published to verified origin/master. Final `TMPDIR=/home/stfu/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=2 bash scripts/test.sh` exited 0: fmt, strict workspace/all-target Clippy, nextest 2196 PASS / 12 existing SKIP (396.136s), doctest commands, debug build/help/version PASS. Test artifact code generation took 13m02s, distinct from runtime; build jobs only were bounded, test load/deadlines/retries unchanged. Four current-source paired episodes and Node 8/Python 17 checks qualify the scoped UI/lifecycle; whole frames remain DIFFERENT.

### Constraints

- Sole native owner/Db/private UDS; existing per-session runners; no second scheduler/client-per-tab/wake registry.
- Captured model/config/authority полного issued batch; fresh continuation использует committed parent selection, не child model/picker draft. Lower trust не расширяет permissions.
- RAW append-only, exact parent/child/op/generation/Location/delivery provenance; no guessed identity from prose; unknown effects не replay.
- Existing resource caps/finite failure retries; no new timer polling. Один mutation/Cargo owner, no clean/target override, no weakening gates/deadlines/assertions.
- User owner/client/config/credentials не трогать. Reference/captures только isolated fake-provider fixtures; исторические seals/results не переписывать.

### Non-goals

- Busy close/delete/rename/Undo mutation-policy changes; ordinary /location busy contract; same-session steer/queue.
- Forced background default, nested-child promotion to primary, historical/startup auto-resume, replay failed/unknown children.
- New SQL tables/DDL migration, generic scheduler/inbox/hit framework, full-product parity/backlog.

## Копия исправленного плана

### A. Tool contract

В `runtime/turn.rs::subagent_tool_def` выровнять top-level/property описание с TS: async, return after admission not completion, automatic notice, independent work, no polling. Сохранить optional Boolean/default/native routing. Проверить actual serialized definition и parser matrix.

### B. Running child pointer

В существующем indexed renderer сохранять viewport-bounded painted eligible spans/immutable launch identities рядом с user targets. Down/Up проверяют конкретную ссылку/source/frame, не mutable job и весь кадр. Согласовать selection invalidation; сохранить drag/resize/overlay/modifiers/source fencing, failed-error expansion. Не вводить generic registry, не отключать animation.

### C. Root navigation

Убрать accepted-running veto из tab/New/Add/Sessions и всех ранних dispatch gates. Одна узкая departure policy защищает unresolved submission/Shell/model receipts и finite conversation/reload/recovery transitions. Capacity только для создания, не existing tab/Home. Проверять настоящий writable root и сохранять standalone-child read-only provenance. Fallible operations до view swap; logical root lookup до detach child; same-index root click возвращает root. Parked-child lifecycle обновляет exact parent. Вынутые views восстанавливать до errors; global Location adoption не должна воскрешать cleared cache.

### D. Picker/live attach

Удалить target execution reservation и family-running refusal в native open_picker. Построить final deck candidate до reservations actual removals; сохранить root/scope/Location/CAS/commit-before-route. Adoption: snapshot → exact canonical page → replace transient overlay → replay target → session-owned revision floor. Не подавлять notices/permissions/titles. Stream cursor не меняется snapshot reply. Enrich existing operations map монотонной revision/idle/owned Pending-Bound provenance и sent-epoch, с явным bounded transient budget; без interest registry. Healthy views сохранять через ordinary shared sync; gap repair covers roots/child_parent/selected/cached children, staged acquisition before mutation. Propagate truncation.

### E. Native notice batch admission

Существующие notices неизменны. Бounded exact reports читаются с length preflight и полными trust wrappers, не через composer 1MiB input. Новая acceptance атомарно проверяет canonical root/source/visibility/no Revert/no assignment, создаёт новый turn и ОДИН empty typed tail control anchor на captured batch, seed journal и assignment всех IDs. Старый notice нельзя использовать anchor: это нарушает fork/DCP chronology. Один optional typed native-input journal marker переносится через working/DCP/fork rebasing как inert representation metadata, не authority. Controls/reports исключаются из user-task selection/Revert composer restoration/fork boundary; скрывается только typed control bubble. Ordinary active/user notice assignment коммитится с actual final prepared request, не первоначальным history read. Accepted failed/cancelled/unknown batch не requeue.

### F. Owner handoff

В existing child Work удерживать optional Arc<Origin> + Origin::Work; Source уже в Origin. Existing job permit живёт до конца execution OR handoff, per-session execution reservation освобождается после child execution. Capture native launches включая future foreground conversion. Reap/changed/join разделяют actual execution и lightweight handoff; obsolete parent rejection не spin. Existing session roots admission и roots→child-work lock order, persistent child notifier after assignment/root/mutation/barrier/suppression transitions. Automatic root reports admitted/permanent-refusal disposition. Cancel origin suppresses wake before settle; parent cancel suppresses старые handoffs; individual child interrupt сохраняет report eligibility. Closing запрещает admissions до shutdown awaits. Origin weak-registration dedup. Canonical ROOT only; nested ceiling не теряется. No timer retry/startup scan/durable wake queue.

### Dependencies and verification

A/B independent; D before complete C; E before F. Existing test owners only. Pointer Down→80ms redraw→Up; A/New/B streams overlap; standalone child→grandchild return; snapshots both directions/reordered/old epoch; picker through WorkspaceChanged; final-gap→auto→fork and DCP; batch rollback/pre-checkpoint failure/Revert; pending handoff cancellation/Run/last TUI. Existing captures add actual idle-followup settlement fences before next explicit submit and pre-Ctrl+B running click; full unmasked TS/native evidence, no cartesian matrix without need. Targeted checks → affected crates → one full offline gate → scoped paired qualification → normal commits/push. Historical reports unchanged; update current ARCHITECTURE/capture README contracts only when behavior changes.

## Change Envelope

- `oc-tui` app/messages/shell/commands/child presentation and nearest tests; `oc` tui_cmd/shared/child_controls and PTY fixtures.
- `oc-adapters` native client/server, application/authority, runtime/children/context/turn, tools TurnLog/native representation, storage child/turn/conversation/fork/DCP direct consumers and tests.
- Direct `oc-core` history/event DTO consumers: typed empty native-control classification and one session-scoped automatic-continuation refusal diagnostic; no new public request/admission API.
- Existing capture scripts/fixtures/README; `docs/ARCHITECTURE.md`, this goal and root GOAL link. Bounded gitignored local evidence.
- No production dependency/service, donor TS edits, paid live calls, schema migration or unrelated baseline changes.

## Current Checkpoint

- Closes: R1–R6, all verified.
- Smallest next action: none; closure is terminal. Commit/push this completion record without further implementation or backlog work.
- Evidence: final full offline gate exit 0, four scoped current-source paired episodes, checked normal commits and verified origin/master.

## Current State

- Resolved: plan `5384c03a5`, R4 `e3f7bf150`, R1 pointer `f30f15f02`, R3 `0ef86762b`, R2 `9420451d1`, R5 admission `4d676301d` and Owner integration `2b198d7d5` published. Initial Rust HEAD `c70e1c9a7`, verified origin `git@github.com:0FL01/oc.git`, branch `master`.
- Last relevant evidence: final full offline gate PASS: 2196 tests passed, 12 existing skips, strict workspace Clippy/fmt/doctest commands/debug build/help/version passed. Correct-provenance foreground and idle-background captures pass at 120×40/80×24, including complete lower-pane/card/notice scoped styled-cell/pixel equality; Node 8/Python 17 capture tests PASS. The historical first-gate 2188 PASS / 6 FAIL / 12 SKIP is preserved; its diagnosed causes are fixed. No deadlines, retries or test load changed.
- Blocker: none.
- Next: no substantive work; publish the completion record. Whole unmasked frames remain DIFFERENT, and no live external-provider or whole-product qualification is claimed.

## Material Decisions

- 2026-10-20: user approved full root background lifecycle and commits/push. Root-only/no restart wake preserves native authority/recovery contracts; not full nested-session parity.
- 2026-10-20: audit replaces unsafe old-notice anchor with one typed tail control per automatic batch; one assigned event kind, no wake queue/DDL.
- 2026-10-20: R5 refusal before acceptance has no valid `TurnFailed` identity. The existing owner task result is an application-health failure, not a per-batch refusal channel. Use a private one-shot start receipt and one bounded typed session diagnostic event rather than fabricating a turn, poisoning the owner, or adding a store/queue. Coalesce live reports within their original parent/origin lineage; do not merge Run and Interactive cancellation ownership.
- 2026-10-20: full-gate evidence requires retaining zero-height typed native-control input boundaries, not dropping them. Native move selection must preserve the existing primary/Source ownership split: primaries consume only their prepared operations; Source fallback excludes a still-reserved issuing caller and rechecks canonical pending phase. No new move queue, claim table or lock.
- 2026-10-20: actual running-child pane comparison found only its border background and bold separator spaces differed (14 styled cells at 120×40). Match the existing raised-surface role and TS `<b>label</b>` followed by a plain space; no general theme change, new widget or masked whole-frame claim.

## Checkpoint History

- 2026-10-20: copied corrected plan and froze R1–R6 before implementation.
- 2026-10-20: R4 verified: explicit TS-like async/no-poll guidance, optional Boolean/default and actual wire assertions; parser 3 PASS, advertised definition PASS, physical child/parent overlap PASS. No new launch engine or forced background default. R1 running redraw RED→GREEN; independent review found missing pressed surface flag, now retained; finish its typed notice replacement regression before pointer delivery.
- 2026-10-20: R1 pointer checked: 620 TUI PASS + strict affected-crate all-target Clippy. Same-cell card→notice regression uses an actual started canonical window and Tail receipt; a legacy unowned footer in the initial test fixture correctly prevented notice movement and was corrected, not a production chronology change. Pointer capture no longer depends on unrelated paint/mutable job, but preserves Down surface/source and actual current link. Proceed R3/R2; visual proof remains pending.
- 2026-10-20: R3 native/projection checkpoint: actual busy picker RED→GREEN; final candidate prevents false removal of running fork adoption. Snapshot replies no longer consume shared wire events or clear unrelated controls; monotonic observations and exact owned request lifecycle include idle/reordered/old-epoch/gap/title cases within a bounded existing map. Target-only adoption and linked gap staging retain canonical windows/drafts, acknowledge truncation, and keep historical Source reads fenced without inferring idle from foreign snapshot absence. 9 bridge + 2 picker + 116 binary TUI + 621 TUI PASS; strict all-target affected Clippy PASS. Next R2, not a new runtime scheduler.

- 2026-10-20: R2/R3 integrated: accepted turns no longer veto New/Add/tab/Sessions; exact parked child launch updates its parent before completion. Original standalone read-only provenance survives nested return. Review found close/delete post-commit activation and typed child command draft hazards; both corrected with explicit preflight/source consumption and regressions. Actual one-TUI A/B streams progress across New and repeated switches, no resubmits; actual held-turn slash/chord/palette/Sessions aliases PASS. Finite receipts, profile/destructive and ordinary Location guards remain. Next R5 native batch, not a scheduler rewrite.
- 2026-10-20: R5 admission foundation: one typed empty tail control and atomic whole-batch assignment preserve final-gap fork/DCP chronology; ordinary requests assign only emitted report IDs at prepared checkpoint. Exact original input coordinates survive working/fork projections without granting wake authority. Review exposed protected-checkpoint metadata/exclusion holes; current-task and retained-journal dedup now have a direct regression. Full adapters lib 865 PASS + 1 existing ignored (133.77s suite), subagent integration 44 PASS (31.21s suite), affected strict Clippy PASS. No idle wake or paired capture claim: next existing Work/Owner wiring.
- 2026-10-20: R5 Owner handoff: original root fully joins before held child completion, then one actual parent request uses the newly committed parent selection. Existing Work holds its existing permit through execution/handoff; one original Origin and session reservation preserve cancellation/source/consumer authority. Review fixed idle-cancel/admission serialization and a failed-launch barrier notification. Owner 12, child 8 and actual shared-runtime 11 PASS, strict four-crate all-target Clippy PASS. Individual child interruption reports without cancelling its parent; foreground conversion keeps the original wake lineage. No new scheduler, queue, schema or startup scan. Next scoped paired capture/full gate.
- 2026-10-20: first full gate found six failures, not completion. Three tests still asserted superseded busy navigation. A real hidden native boundary regression is fixed without a visible bubble or ownership-guard relaxation. VAR01 proved a finite ACK wake omission rather than a slow model; existing receipt polling now reconciles it. The scoped-move stream failure led to a held-gate actual-Owner RED proving competing move consumers; verify the minimal exclusion next. Cold unified test compilation was diagnosed separately from runtime deadlines; none were raised.
- 2026-10-20: full-gate corrections checked: zero-height native input boundary preserves both predecessor and automatic-turn notice order; existing finite model-receipt wake fixes VAR01 without relaxing departure safety. Native primaries consume only their prepared moves; Source fallback checks the still-pending issuing caller under existing roots/Db synchronization. Held-gate Source/sibling and stale-list regressions, scoped actual streams/pickers and strict Clippy PASS. Deliver these corrections, then actual paired qualification/final gate; no whole-product readiness claim.
- 2026-10-20: actual sealed foreground episode passes both products before Ctrl+B, including natural redraw, live child, original foreground ownership and later automatic parent settlement. Initial observer-before-admission snapshot now correctly reports no root as not idle. Producer provenance is corrected for fresh captures. Complete lower-pane comparison exposed 14 local styling differences; targeted raised-surface/separator regression is under verification, full scene differences remain disclosed.
- 2026-10-20: final scoped paired qualification: fresh foreground 120×40/80×24 has 17 behavioral stages per product and complete lower-pane styled-cell/RGBA equality after the local semantic fix. Idle-background 120×40/80×24 has 15 stages, two actual parent followups, exact assignments/no child replay, and complete card/notice component equality. Consistent read-only snapshots and delivered-message causality prevent false idle witnesses. Full unmasked scenes remain DIFFERENT; no generic harness, donor change or masked whole-scene PASS. Proceed only final offline gate/delivery/closure.
- 2026-10-20: capture delivery `e0ecb26a9` pushed; final full offline gate exited 0 with 2196 PASS / 12 existing SKIP, strict workspace Clippy/fmt/doctest commands/build/help/version PASS. Frozen R1–R6 closure passed; finish without opening further work.

## Completion

- Resolved outcomes: R1–R6 verified; no pending or waived outcomes.
- Commands and artifacts: full `bash scripts/test.sh` exit 0 on `e0ecb26a9` (managed log `sh_159a681e4001L8A4YEDENe5M4I`); Node capture tests 8 PASS and Python capture tests 9+8 PASS. Primary paired artifacts are `.local/evidence/live-navigation-r1-120-03`, `live-navigation-r1-80-01`, `live-navigation-background-120-01`, and `live-navigation-background-80-01`; their component reports/producers and causal behavior witnesses are qualified. Full scene comparisons remain DIFFERENT.
- Constraint and diff-scope check: existing sole owner/Db/private UDS and bounded Jobs reused; no new dependency/service/scheduler/queue/SQL DDL, donor edits, user-owner restart, credentials or paid live calls. Source/model/batch/cancel/unknown-effect and read-only/pending-receipt protections remain. Root-only live-origin continuation, no historical wake and no same-session steering are the frozen boundaries. Evidence stays gitignored within the existing retention limits; historical failures/seals are unchanged. Final completion edit is documentation-only, so it does not invalidate the checked runtime/capture inputs.
- Final status: complete; normal iterative commits/push delivered to verified origin/master. The user's already-running owner was not replaced; rebuild does not update that process in place.
