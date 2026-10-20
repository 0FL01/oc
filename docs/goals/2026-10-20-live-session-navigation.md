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
  - Status: pending
  - Evidence:
- R2: accepted execution не блокирует root tab/New/Add/Sessions navigation и независимый запуск B при работающем A.
  - Source: третий screenshot, требование параллельных вкладок, audit §C.
  - Acceptance: одна TUI переключает A/B и создаёт B; оба stream прогрессируют до release; нет Cancel/Background/repeated Submit; linked и standalone read-only provenance не расширяется.
  - Primary evidence: existing binary routing tests + actual-binary PTY concurrency.
  - Status: pending
  - Evidence:
- R3: running-target picker/live attach корректны в пределах существующих retention bounds.
  - Source: opening working session/child, audit §D.
  - Acceptance: retained window/draft сохраняется через WorkspaceChanged; available prefix появляется один раз; old T1/reordered snapshots не откатывают T2/controls; linked gap repair не теряет owner; truncated projection явно ограничен.
  - Primary evidence: native client ordering and shared adoption tests, actual running-target interaction.
  - Status: pending
  - Evidence:
- R4: background tool contract явно описывает существующий async режим как TS.
  - Source: user background tool suspicion, audit §A.
  - Acceptance: optional Boolean, foreground default, explicit asynchronous/no-poll guidance в actual serialized tool definition; true не ждёт child terminal.
  - Primary evidence: actual provider definition/parser tests + existing background overlap integration.
  - Status: pending
  - Evidence:
- R5: terminal background child автоматически продолжает live idle canonical root parent без нового user input и без повторного child execution.
  - Source: выбор «Полный background lifecycle», audit §E/§F.
  - Acceptance: durable report → one bounded assigned batch → real parent request; active/user race оставляет один runner; cancellation/last TUI/Run release/shutdown suppress; original Source и current committed parent selection; fork/DCP/Undo/Revert chronology сохранена.
  - Primary evidence: storage/runtime/authority regressions + existing actual background capture provider witnesses.
  - Status: pending
  - Evidence:
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
- Existing capture scripts/fixtures/README; `docs/ARCHITECTURE.md`, this goal and root GOAL link. Bounded gitignored local evidence.
- No production dependency/service, donor TS edits, paid live calls, schema migration or unrelated baseline changes.

## Current Checkpoint

- Closes: R1, R4 (independent narrow initial iteration).
- Smallest next action: establish pointer redraw regression; implement local painted-target capture, and explicit background guidance.
- Expected evidence: targeted TUI child pointer and actual advertised tool tests.
- Stop or replan if: source/provenance/retention guard cannot be preserved; expand only the directly required consumer.

## Current State

- Resolved: frozen audited plan copied; initial Rust HEAD `c70e1c9a7`, clean worktree, verified origin `git@github.com:0FL01/oc.git`, branch `master` tracking origin/master.
- Last relevant evidence: read-only source audit; no implementation tests yet.
- Blocker: none observed.
- Next: R1/R4 regression → minimal fix → focused gates → checked iteration commit/push.

## Material Decisions

- 2026-10-20: user approved full root background lifecycle and commits/push. Root-only/no restart wake preserves native authority/recovery contracts; not full nested-session parity.
- 2026-10-20: audit replaces unsafe old-notice anchor with one typed tail control per automatic batch; one assigned event kind, no wake queue/DDL.

## Checkpoint History

- 2026-10-20: copied corrected plan and froze R1–R6 before implementation.

## Completion

- Resolved outcomes: pending.
- Commands and artifacts: pending.
- Constraint and diff-scope check: pending.
- Final status: active.
