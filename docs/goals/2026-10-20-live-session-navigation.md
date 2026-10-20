# Goal: live session navigation and root background lifecycle

Status: active
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
  - Status: in_progress
  - Evidence: running-click RED на 80×24 spinner redraw → PASS на 80/120/160 (spinner, unpainted stream, current job facts, page replacement). Дополнительный actual indexed card→same-launch notice repaint в той же cell — PASS, pressed surface flag сохраняется. Full `oc-tui --lib` — 620 PASS (6.13s), strict affected-crate all-target Clippy PASS. Selection/failed/error/drag/resize guards зелёные. Actual capture pending.
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
  - Status: in_progress
  - Evidence: native batch admission/history foundation checked: full adapters lib 865 PASS + 1 pre-existing ignored; full subagent integration 44 PASS; strict affected core/adapters/TUI all-target Clippy PASS. Atomic batch rollback/assignment, fresh tail chronology, recursive inert fork, Revert, >1MiB native report and old-DCP/new-journal regressions PASS. Protected checkpoint reports are assigned only when actually emitted and appear once in native admission and later history. Owner idle-wake wiring and actual lifecycle proof remain pending.
- R6: проверенные изменения закоммичены и отправлены в verified origin.
  - Source: последний пользовательский запрос.
  - Acceptance: relevant tests, full offline gate, scoped current-TS paired qualification и diff review; normal commits/push, remote HEAD verified.
  - Primary evidence: команды/exit codes, capture witnesses, commit IDs и remote HEAD.
  - Status: pending
  - Evidence:

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

- Closes: R5 (R2/R3/R4 verified; R1 actual capture remains pending).
- Smallest next action: reproduce actual Owner idle-root wake with a held physical child response, then wire the bounded existing child Work/owner handoff to the checked private native batch path.
- Expected evidence: after original root has fully joined, child completion causes one real parent request with current committed parent selection, no second Submit/child execution; cancellation/settlement and capacity remain bounded.
- Stop or replan if: source/provenance/retention guard cannot be preserved; expand only the directly required consumer.

## Current State

- Resolved: plan `5384c03a5`, R4 `e3f7bf150`, R1 pointer `f30f15f02`, R3 `0ef86762b`, R2 `9420451d1` published. R5 native batch/history foundation checked. Initial Rust HEAD `c70e1c9a7`, verified origin `git@github.com:0FL01/oc.git`, branch `master`.
- Last relevant evidence: 865 adapters lib PASS + 1 existing ignored; 44 subagent integration PASS; 23 runtime compaction PASS; native-control TUI regression PASS; strict affected all-target Clippy PASS. Prior actual navigation evidence remains; no full offline gate/capture claim yet.
- Blocker: none observed.
- Next: bounded Owner continuation and current-source captures. Automatic idle-root scheduling is not implemented yet; R1/R6 actual captures and final full gate remain pending. Running-card current-reference capture preparation is a separate uncommitted script diff, not visual qualification.

## Material Decisions

- 2026-10-20: user approved full root background lifecycle and commits/push. Root-only/no restart wake preserves native authority/recovery contracts; not full nested-session parity.
- 2026-10-20: audit replaces unsafe old-notice anchor with one typed tail control per automatic batch; one assigned event kind, no wake queue/DDL.
- 2026-10-20: R5 refusal before acceptance has no valid `TurnFailed` identity. The existing owner task result is an application-health failure, not a per-batch refusal channel. Use a private one-shot start receipt and one bounded typed session diagnostic event rather than fabricating a turn, poisoning the owner, or adding a store/queue. Coalesce live reports within their original parent/origin lineage; do not merge Run and Interactive cancellation ownership.

## Checkpoint History

- 2026-10-20: copied corrected plan and froze R1–R6 before implementation.
- 2026-10-20: R4 verified: explicit TS-like async/no-poll guidance, optional Boolean/default and actual wire assertions; parser 3 PASS, advertised definition PASS, physical child/parent overlap PASS. No new launch engine or forced background default. R1 running redraw RED→GREEN; independent review found missing pressed surface flag, now retained; finish its typed notice replacement regression before pointer delivery.
- 2026-10-20: R1 pointer checked: 620 TUI PASS + strict affected-crate all-target Clippy. Same-cell card→notice regression uses an actual started canonical window and Tail receipt; a legacy unowned footer in the initial test fixture correctly prevented notice movement and was corrected, not a production chronology change. Pointer capture no longer depends on unrelated paint/mutable job, but preserves Down surface/source and actual current link. Proceed R3/R2; visual proof remains pending.
- 2026-10-20: R3 native/projection checkpoint: actual busy picker RED→GREEN; final candidate prevents false removal of running fork adoption. Snapshot replies no longer consume shared wire events or clear unrelated controls; monotonic observations and exact owned request lifecycle include idle/reordered/old-epoch/gap/title cases within a bounded existing map. Target-only adoption and linked gap staging retain canonical windows/drafts, acknowledge truncation, and keep historical Source reads fenced without inferring idle from foreign snapshot absence. 9 bridge + 2 picker + 116 binary TUI + 621 TUI PASS; strict all-target affected Clippy PASS. Next R2, not a new runtime scheduler.

- 2026-10-20: R2/R3 integrated: accepted turns no longer veto New/Add/tab/Sessions; exact parked child launch updates its parent before completion. Original standalone read-only provenance survives nested return. Review found close/delete post-commit activation and typed child command draft hazards; both corrected with explicit preflight/source consumption and regressions. Actual one-TUI A/B streams progress across New and repeated switches, no resubmits; actual held-turn slash/chord/palette/Sessions aliases PASS. Finite receipts, profile/destructive and ordinary Location guards remain. Next R5 native batch, not a scheduler rewrite.
- 2026-10-20: R5 admission foundation: one typed empty tail control and atomic whole-batch assignment preserve final-gap fork/DCP chronology; ordinary requests assign only emitted report IDs at prepared checkpoint. Exact original input coordinates survive working/fork projections without granting wake authority. Review exposed protected-checkpoint metadata/exclusion holes; current-task and retained-journal dedup now have a direct regression. Full adapters lib 865 PASS + 1 existing ignored (133.77s suite), subagent integration 44 PASS (31.21s suite), affected strict Clippy PASS. No idle wake or paired capture claim: next existing Work/Owner wiring.

## Completion

- Resolved outcomes: pending.
- Commands and artifacts: pending.
- Constraint and diff-scope check: pending.
- Final status: active.
