# Goal: background Subagent lifecycle — current OC2 TS visual parity

Status: active
Source: три пользовательских скриншота и уточнение «нам нужен визуальный паритет с OC2 TS в этом scope»; исправленный после четырёхчастного аудита план; запрос «делай копию плана в цель и итеративно реализовать и коммит пуш».
Last updated: 2026-10-19

## Objective

Настоящие background Subagent отображаются как в текущем локальном OC2 TS: анимированная исходная строка с Background, отдельный finished notice и обновление той же исходной строки в ✓ с сохранённым Background. Проверить два одновременных child, правильную хронологию и повторное открытие; публиковать проверенные итерации в verified origin/master.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: исходные launch rows и отдельные completion notices сохраняют точную identity и обновляются без дубликатов.
  - Source: screenshots 1–3, corrected plan §2.
  - Acceptance: две Explore строки с descriptions «Сравнение subagent с upstream» / «Состояние Git и тесты»; завершение B меняет только старую B на ✓, Background остаётся; отдельный `↳ Explore finished · description`; A продолжает работу. Terminal notice обновляет и retained card вне inventory16, не ретаргетирует fence/result/link, не выключает genuine continuation; повтор notice идемпотентен.
  - Primary evidence: real handler regression с exact owner notice, B вне inventory и warm cache; existing child/foreground/continuation/navigation tests.
  - Status: pending
  - Evidence: —
- R2: actual-running spinner соответствует TS и lifecycle интерфейса.
  - Source: screenshot 1, corrected plan §3.
  - Acceptance: 10 frames/80ms, независимая фаза строки, parent idle/live→history/shared refresh не сбрасывают phase; canonical/live winner до eligibility; off→on reset, None=true, отсутствие own idle deadlines после завершения/removal/rebind. Off-mode foreground labeled gap отличается от bare Background gap. Pending Delegating/Linked hover не регрессируют.
  - Primary evidence: deterministic Instant/cache/owner tests и actual unpaused paired glyph observation.
  - Status: pending
  - Evidence: —
- R3: notice имеет TS step chronology при streaming, consumption, canonical refresh и reopen.
  - Source: screenshot 2 «вставляется вниз чата» и current TS pending/steer semantics, corrected audit §4.
  - Acceptance: pending notice после текущего step; consumed notice перед последующим assistant step; не за всем Rust turn. Позднее обычное сообщение не перетягивает старый notice. Unknown/legacy/partial provenance не вызывает guessed rotation; paging/RAW/order/dedup неизменны. Paint/hit/count/anchor используют actual live index; canonical-ahead возвращает None.
  - Primary evidence: owner read projection tests original input/request/RAW/EffortUpdate; controlled live→final→reopen chronology + routing/geometry tests.
  - Status: pending
  - Evidence: —
- R4: scoped visual parity подтверждён current local TS runtime, а не старым pending proof.
  - Source: user current OC2 TS scope, corrected plan §1/§5.
  - Acceptance: изолированный runtime checkout `bc5ebf00…`, sealed runtime/source/runner/font provenance; 120×40 и 80×24 on/off: A+B running, B done/A running, оба done, правильные notices/clicks. Отдельный long-description badge-wrap и held-parent terminal/continuation cases; реальные correlated child owners и no duplicate/effects. Сравнивать целые компоненты, значимые spaces/background и relative anchors; полные grids/PNG/cursors unmasked. Независимые A/B phases допускают только component-specific observed pairing при доказанном concurrent state, не whole-scene PASS.
  - Primary evidence: paired actual captures, structured owner/provider witnesses, negative fixture/comparator tests.
  - Status: pending
  - Evidence: —
- R5: код проверен и доставлен в Git.
  - Source: commit/push request, corrected plan §6.
  - Acceptance: targeted crate/fixture/comparator/syntax checks и `bash scripts/test.sh` PASS; review/diff-check и closure; own commits pushed без force в verified origin/master.
  - Primary evidence: команды/exit codes, commit IDs, remote HEAD.
  - Status: pending
  - Evidence: —

### Constraints

- Один native authority/Db, исходные launch/parent/child/op/generation/location/delivery fences и bounded inventory16; никаких execution facts из prose/preview.
- RAW append-only, SQL schema и stored message sequence неизменны; notice delivery/admission/scheduler не переписывать.
- Только небольшая additive optional `HistoryTurn` placement DTO с serde default; нет нового RPC/transport envelope. Original journal/RAW, не DCP working projection.
- UI clocks presentation-only; selection/capture/security/permission/cancellation и retention guarantees сохраняются.
- Один mutation/Cargo owner, без clean/target override, ослабления tests/assertions/deadlines/load/guards, paid providers и user owner restart/config changes.
- Reference — current checkout isolated standalone; старый 2.0.12 supplementary only, не current pixel proof; старые seals и global comparators не переписывать/маскировать.

### Non-goals

- Общий animation framework, timer thread/poller, runtime/scheduler rewrite, расширение inventory, SQL migrations, новый executable registry.
- Полный TUI parity, новые Shell campaigns/model overrides, повторная реализация pending Delegating…, несвязанный backlog.

## Change Envelope

- Cards: `oc-tui/src/{history,child_view}.rs`, `app/transcript.rs`, existing tests; `oc/src/tui_cmd.rs` exact ChildNotice consumer/tests.
- Actual clocks: `oc-tui/src/{app,tools,messages}.rs`, `app/{live,model_selection,transcript}.rs`, direct prepaint/shared-state consumers/tests.
- Chronology: `oc-core/src/queries.rs` optional typed sidecar/compat tests; `oc-adapters/src/{storage,storage_turn_history}.rs` read-only projection and nearest tests; existing TUI history/live/composer/geometry consumers.
- Qualification: existing `scripts/tui_capture/{capture,composer,tool_preview}.mjs`, composer fixture/config/bridge/snapshot consumers and nearest tests; narrowly sealed current-TS launcher/build inputs in approved owned scratch. No edits to donor TS product source/dependency contract.
- Artifacts: bounded gitignored per-run evidence; this repo-local goal. Current worktree/non-root only.

## Копия исправленного плана

1. Зафиксировать регрессии и current-reference seam. Два actual background children с уникальными call/op/child/delivery IDs; release B, дождаться durable notice и B✓, затем A. Различить notice во время terminal step и consumed перед continuation. Старый 2.0.12 executable — только supplementary oracle; current checkout isolated runtime обязателен для current TS claim.
2. Exact notice reconciliation: извлечь owner-projected typed notice из existing exact history_message read; оставить busy selective/idle normal refresh. После последней замены page применить terminal capture только по полному fence к существующим history/live cards, затем activity из полного текущего bounded inventory. One-job slice не должен сбрасывать continuation; rejected fence — no-op. Не менять tool state/result/link/background; bookkeeping revision/bounds/cache сохранять. Meaningful regression — B вне inventory16, warm paint.
3. Одна bounded `BTreeMap<op, Clock>` actual-running presentation в TuiState отдельно от pending delegation. Borrowed effective canonical/live winners выбираются до eligibility, включая terminal winners. Membership sync до animation guard; key heap bytes учитывать, cap по retained history+live (240+64), не inventory16. Glyph только в render-copy; default frame0 до arm, existing 80ms nanos arithmetic, invalidate только changed glyph. Сохранить clocks через parent finish/live→history/same-owner shared reconstruction; reset effective toggle и owner/rebind/eviction, не reseed old-owner rows после reset. Existing tick/deadline/prepaint; без hidden parked polling. Общий actual running predicate, foreground/static labeled gap против Background/bare gap. Pending hover semantic пустого input/Linked, не spinner.is_some().
4. Никакой whole-turn rotation. Private read projection сопоставляет original native User inputs после первого prompt с ordered shell+child notice IDs, затем фильтрует child. First request с input_start > notice original position даёт consuming span; original display span→before_part_sequence. Accepted prompt — первый User, не index0 (EffortUpdate). RAW rollover original bases/receipts, не последние 192 spans/working copies. Optional typed sidecar: accepted prompt ID, notice ID→BeforeRequest{span,before_part_sequence} либо доказанный Tail; absence=unknown. Одинаковый reader для live/canonical/reopen, no new storage facts. Передать sidecar через HistoryTurn/revision/history flatten/selective refresh. Один composer `(rows, actual_open_live_index)` для paint/hit/count/anchors; no crossing gaps/ordinary inputs, canonical-ahead None, индекс корректен после compaction/reasoning/banners. Small local ownership tag только если нужен для наблюдаемого fallback, не generic segment state.
5. Isolated opt-in background fixture в существующем pipeline, несовместимый с pending/root-handoff/model-shell. Actual Explore, две исходные русские descriptions, без model suffix/Shell. Registry из двух issued calls, независимые finite releases, structured task/context/call/result IDs, child без tools допустим, positive classification до title; не arrival order/assistant prose. Корректные SSE item/output indices; unique ACK ID+payload validation. Owner-filtered bounded schema snapshots с overflow detection и реальными operation→child→notice relationships. All mode inputs через guards/build/source seals/fonts/TMPDIR/animation config/bridge/timeline/qualification. Main scenes 120×40/80×24 on/off, отдельные badge-wrap и held parent terminal/continuation; correct card+notice navigation. Естественный unpaused VT progress, observed glyph pairing каждого whole component при concurrent owners; нет clock reset/splicing/masks. Comparator negative cases missing B/notice/wrong badge colors, full unmasked differences сохраняются.
6. Targeted Rust tests (`oc-tui --lib`, `oc2 --bin oc2`, adapter projection и `--test subagent`), Python fixture/comparator tests и Node syntax, actual paired qualification; затем full offline gate/diff review/closure/commit/push. Последовательный Cargo owner включает capture build. Зелёный offline gate не заменяет visual proof.

## Current Checkpoint

- Closes: R1.
- Smallest next action: regression вне inventory и exact fenced terminal reconciliation.
- Expected evidence: handler/card regression сначала RED, затем targeted GREEN; no other card/continuation changes.

## Current State

- Resolved: corrected plan copied; preflight clean.
- Last relevant evidence: Rust HEAD `1d78b56bb59916a6c2682ddec22a4452e4f731eb`; TS `bc5ebf00a4c169ece10f276e6ce02eaf18c326e6`; both clean; verified origin/master same Rust SHA; UID1000, free disk >90GiB, MemAvailable >12GiB.
- Blocker: нет подтверждённого.
- Next: R1; independent read-only preflight current TS/runtime and durable chronology helper may inform later checkpoints.

## Material Decisions

- 2026-10-19: source audit corrected whole-turn notice rotation to actual request-boundary projection; additive DTO required, SQL/runtime mutation is not.
- 2026-10-19: exact terminal state and current child activity are separate; apply receipt then full-inventory activity, preserve continuation and immutable Background provenance.
- 2026-10-19: current TS runtime required; identical presentation source with old binary alone is insufficient current renderer/backend proof.

## Checkpoint History

- 2026-10-19: verified clean Rust/TS/origin; frozen corrected plan copied before implementation.

## Completion

- Final status: active.
