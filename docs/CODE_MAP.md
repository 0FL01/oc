# Карта владельцев кода

Навигация по текущему дереву; читать нужную строку, не все перечисленные файлы. Root-правила: `AGENTS.md`; инварианты: `ARCHITECTURE.md`; первый перенос: T52 в `goals/2026-09-28-code-slices.md`.

Текущая раскладка отражает выполненные переносы T52; исходный Git commit — `ddb6ae2f0cba18c4aa161159899228a7343398eb`. Baseline/qualification — `evidence/T52/`; текущий размер строится `python3 scripts/code_size.py`, без вечных параллельных карт «old/new» и дублирования LOC.

| Вопрос / владелец | Читать сейчас | Ближайшие тесты / граница |
| --- | --- | --- |
| Question exchange / native FormPrompt (TOOL15) | `oc-core/src/question.rs` — bounded waits, exact replies and terminal result DTO; `runtime/turn.rs` — common admission and existing ToolOp/turn transactions; `tools/output.rs::prepare_question` / `storage_tool_output.rs` — oversized result's sole cold artifact, small scoped presentation ref and bounded identity-checked native read; `storage.rs` — three presentation queries; `application.rs` — Core inbox and generation retirement; `oc-tui/src/question_view.rs` — lower composer consumer; `oc/src/tui_cmd.rs` — source-session routing and process-memory drafts | `question/tests.rs`, `question_view/tests.rs`, runtime target `tests/fixtures/question_lifecycle.rs`; `storage_tool_output/tests.rs::tool21_actual_oversized_question_has_one_cold_payload_and_restart_presentation` covers typed answers/fork/no model grant/expiry/immutable graph; direct ELF `evidence/T50/native_question.py`, `native_tool_output_question.py`. Terminal history never restores an active form; permission consent is separate. |
| UI state, input, selection, turn projections | `crates/oc-tui/src/app.rs` — state/reset/API; `app/input.rs` — composer/panels/pointer dispatch; `app/transcript.rs` — history/viewport/selection/copy; `app/tabs.rs` — tab presentation/clocks; `app/live.rs` — receipts/worker/tool/DCP events | `app/tests.rs` shared fixtures + `app/tests/{input,transcript,tabs,lifecycle}.rs`; tab suite подключена в `app/tabs.rs`, чтобы private animation state не раскрывать ради tests. `cargo test -p oc-tui --lib app::`; `oc --test tui_render_alloc`. |
| Markdown / transcript rendering и wrapping | `crates/oc-tui/src/messages.rs`; `styled.rs`, `history.rs`, `tools.rs`, `dcp_view.rs` только по зависимости | `messages/tests.rs`; `cargo test -p oc-tui --lib messages::`. Shared wrapping в `styled.rs`, не создавать второй renderer. |
| Геометрия кадра, prompt/sidebar/tabs/toast | `crates/oc-tui/src/shell.rs`, затем `layout.rs` | `shell/tests.rs`; `cargo test -p oc-tui --lib shell::`. Не путать с shell executor в adapters. |
| TUI lifecycle, bounded event loop, PanelIntent и tab deck | `crates/oc/src/tui_cmd.rs`; `clipboard.rs`/`bootstrap.rs` при необходимости; `headless.rs` — one SIGINT subscription across submit acknowledgement and event consumption | `tui_cmd/tests.rs` + `tui_cmd/tests/{routing,lifecycle}.rs`. `src/approval_tests.rs` сохраняет прежнее подключение; `cargo test -p oc --bin oc tui_cmd::`; target `responses` / AUD12 for headless cancellation. |
| Application worker, generations, queries, title/session ownership | `crates/oc-adapters/src/application.rs`, `application_selection.rs`, `application_tab_deck.rs` | Review/file_suggestion/reload packs в `application/tests.rs`; existing conversation/fork test-файлы сохранены. `cargo test -p oc-adapters --lib application::`. Existing facade 5,402 lines: shared worker/generation admission remains coupled; T57 auth/bindings/catalog/title/channel/preview lifetime grows it 141 lines, scoped behavior is in owner children. Next natural seam on substantial growth: acknowledged query dispatch versus worker transition loop, retaining one owner. |
| Provider turn + admission + tool execution | `crates/oc-adapters/src/runtime.rs` — owner/entry API; `runtime/turn.rs` — admission/turn/tool/child flow; `runtime/tool_stream.rs` по stream | `runtime/tests.rs` сохраняет identity/VIS38 packs; `cargo test -p oc-adapters --lib runtime::`, target `runtime`. |
| Owned concurrent foreground children / linked controls (T45/R3 atomic) | `runtime/turn/foreground.rs` — ordered admission/intents and explicit-child actual predecessor join; `runtime/children.rs` — same bounded owned execution for FG/BG, exact conversion releases only waiter, selected cancellation and plain permission-rejection propagation; `storage_children.rs` — immutable launch/current mode/terminal/notice projection; `application.rs` — exact linked read without Location selection; `oc-tui/src/child_view.rs`, `oc/src/tui_cmd/child_controls.rs` — bounded live family consumer, captured child view and parked parent | Existing `subagent` target and `tests/fixtures/{foreground_children,background_children}.rs`; owner `runtime/children/tests.rs`, `storage_children/tests.rs`, binary `tui_cmd/child_controls/tests.rs`. Actual normal ELF PTY `evidence/T45/native_child_controls.py`; frozen/current `child-controls.md`. Ctrl+G opens children after configured key priority; Ctrl+B converts exact child unless real Shell selected; Ctrl+C/Ctrl+D interrupt exact selected child; Esc hides/returns. Same-child continuation joins execution after conversion. |
| Application-owned background children (T45/R3 atomic) | `runtime/children.rs` — same `Db::shared_handle`, retained actual joins and captured source resources; `storage_children.rs` — atomic admission/current terminal/delivery/recovery facts; `runtime/turn.rs`, `application.rs` — safe continuation notices and idle/fatal/shutdown ownership; `storage_fork.rs` — native launch provenance copied without child ownership; `oc-core/src/{queries,core_app}.rs` — typed family inventory/read/conversion/interrupt | Existing `subagent` target, `tests/fixtures/background_children.rs`, private `storage_children/tests.rs`; normal ELF `evidence/T45/native_background_children.py`, receipt `background-children.md`. Immutable running launch differs from current state; unknown effects never replay. Parent placement readiness waits for actual family terminal (`storage_session_move.rs`), preserving the transactional family guard and child source. Safe unfinished resumption and command routing remain subsequent atomics. |
| Closed current-task RAW/HOT boundary (T45/R9) | `tools/turn_history.rs` — fixed original coordinates, selected working checkpoint and closed-delta preparation; `storage_turn_history.rs` — same-Db immutable segments, atomic HOT CAS, indexed latest windows and explicit admitted RAW/fork copy; `runtime/turn.rs` — settled batch capture; `runtime_compaction.rs` — existing admitted counted summary policy | `storage_turn_history/tests.rs`, existing compaction/retry/model-history/fork tests; normal ELF `evidence/T45/native_hot_raw.py`. No archive reload on ordinary continuation/retry; explicit RAW pages retain original local coordinate domain plus scalar base. Qualification status: `evidence/T45/hot-raw.md`. |
| Shared initial/nested AGENTS lifecycle (R10 prerequisite) | `oc-adapters/src/composition.rs` — canonical initial snapshots and pinned roots from the admitted config generation; `instructions.rs` — bounded descriptor-safe successful-read hook, typed sources/projection; `runtime/instructions.rs`, `runtime/{turn,context}.rs` — common root/child reconciliation; `permissions.rs`/`runtime.rs` — automatic source permanent-Allow gate distinct from approved original-file Ask; `storage_instructions.rs` — existing events/prefs/TurnLog atomic source facts; `storage_dcp_view.rs`, `storage_fork.rs` — position rebasing/current-reference adoption | `instructions/tests.rs`, `storage_instructions_tests.rs`, `runtime/instructions/tests.rs` under the existing compaction fake owner; filter `prm01_`. Direct normal ELF: `evidence/T45/native_instructions.py` (also `--ask-only`); `evidence/T50/native_read.py --case nested_lifecycle`. Initial config sources remain generation-pinned across failed reload; nested source permanent Allow/dataRoot/nofollow/cancel precede IO. `after_read(..., scope_dir)` is the T50 text/directory/image caller seam. |
| Request-local Linux execution context / shared base fallback (R7) | `runtime/environment.rs` — bounded native facts, Git metadata probe, deterministic escaped layer; `runtime/turn.rs` — shared root/child fallback and fresh fixed input; existing shell selector/root and title/compaction input consumers | `runtime/environment/tests.rs`, filter `runtime::environment`; normal debug/release `evidence/T45/native_host_context.py` captures default/custom/own-model child, restart and live A→B→A source-request pinning. `oc/tests/support/context_ids.rs` validates volatile harness lanes before legacy conversation comparisons; native receipts retain full wire. Metadata grants no permissions. |
| Runtime DCP projection / continuation / token accounting | `crates/oc-adapters/src/runtime/context.rs`, `runtime_compaction.rs`; `runtime.rs` только для общей facade/generation; `dcp.rs`, `dcp_auto.rs` по policy | `runtime/tests.rs`, filter `runtime::tests::vis38_review_tests::`; integration `runtime`, `dcp_atomic`, `context_bounds`; `oc` target `dcp_runtime`. Existing compaction/tool_stream files сохранены; не делать новый DCP engine. |
| Whole-past hot renewal / compact recovery (T45/R9/DCP11 atomic) | `dcp.rs` — standalone replacement; `storage_dcp_view.rs` — indexed active/addressed endpoints, selected packs and causal metadata; `runtime/context.rs` — current-policy selection and real wire forgetting; `runtime_compaction.rs`, `storage_compaction.rs`, `storage_turn_history.rs` — metadata-first admitted recovery and atomic progress/selection/mark retirement; `storage_conversation.rs`, `storage_fork_context.rs` — versioned selection and explicit rebasing | `dcp/renewal_tests.rs`, `runtime/compaction_renewal_tests.rs`; unchanged DCP/media/Fork/RAW owners. Normal ELF `evidence/T45/native_hot_renewal.py`; frozen obligations/results `evidence/T45/hot-renewal.md`. Defaults/child controls/profiles are separate. |
| MCP generation, attach, status/controls, quarantine/close | `crates/oc-adapters/src/runtime/mcp.rs` — immutable request leases/dispatch; `runtime/mcp/lifecycle.rs` — existing resource supervisor; `runtime.rs` — shared owner; `application.rs` — Core commands; `mcp_remote.rs`, `mcp_stdio.rs`, `mcp_http_lifecycle.rs` — transports. Typed DTO/events: `oc-core/src/{queries,core_app}.rs`; `/mcps`: `oc-tui/src/app/mcp.rs` → `oc/src/tui_cmd.rs` | Adapter targets `mcp_remote`, `mcp_stdio`, `runtime` (new `runtime/mcp_lifecycle.rs` pack); `src/application/mcp_tests.rs`; binary `mcp_application/lifecycle.rs` under the existing target. `fixtures/mcp10-lifecycle.py` records bounded synthetic counters. MCP lease/close/admission порядок не переносить между owners. |
| MCP config forms, stage timeouts, local resource/credential admission | `crates/oc-adapters/src/config/mcp.rs` — legacy/canonical normalization under `config.rs`; `composition.rs::admit_local_mcp` — Location/source authority; shared `queries.rs::ServiceDiagnostic` → `tui_cmd.rs` startup consumer | `src/config/mcp_tests.rs`, `src/composition/mcp_tests.rs`; integration packs `tests/{mcp_stdio,mcp_remote}/config_admission.rs`, `oc/tests/mcp_application/config_admission.rs` under existing targets/filter `mcp09_`. `fixtures/mcp09-normalization.json` and `mcp09-stdio.py` are source/boolean-counter fixtures, not new Cargo targets. |
| MCP explicit prompt/resource lookups | `oc-core/src/queries/mcp_lookup.rs`, `core_app.rs` → `oc-adapters/src/application/mcp_lookup.rs`, `runtime/mcp/lifecycle/lookups.rs` → `mcp_lookup.rs`, existing stdio/remote clients | `src/application/mcp_tests/lookups.rs`, `tests/{mcp_stdio,mcp_remote}/lookups.rs`, `oc/tests/mcp_application/lookups.rs`; `fixtures/mcp11-lookups.py`. Root-session permission/scoped client lease; bounded catalogs and caller-only bodies, no implicit model projection. |
| MCP native media → Responses / durable replay | `oc-adapters/src/mcp_result/media.rs` — validated/redacted facts and wire lowering; existing clients → `runtime/mcp.rs`, `runtime/turn.rs`; `provider.rs` additive output; `tools/mcp_log.rs` attachment in the existing turn row; `storage_dcp_view.rs`, `runtime/context.rs`, `runtime_compaction.rs` projections | `mcp_result/media/tests.rs`, `tools/mcp_log/tests.rs`, `runtime/media_tests.rs`; actual binary `oc/tests/mcp_application/media.rs` under existing target/filter `mcp12_`; tiny `fixtures/mcp12-media.{json,py}`. No URI fetching, new storage schema or media renderer. |
| Db, history, DCP, durable tool outcomes / recovery | `crates/oc-adapters/src/storage.rs` и только нужный `storage_*.rs` | Main suite в `storage/tests.rs`; existing fork/conversation/DCP packs сохранены. Малый inline byte-bounded preview case оставлен рядом с bound_preview (локальный invariant, не растущий suite). `cargo test -p oc-adapters --lib storage::`, targets `storage_lock`, `dcp_atomic`, `tab_deck`. |
| Common admitted tool text / registered artifact continuation | `oc-adapters/src/tools/output.rs` — captured config limits, logical-line/UTF-8 head/tail preparation, shared incremental redactor and typed-envelope preservation before TurnLog/provider/outcome copies; `config.rs`, `config/diagnostic.rs` — whole-section replacement and safe validation; `storage_tool_output.rs` — migration7, descriptor-pinned generated resources, same Db/flock/connection/shared blob quota, durable extent publication, leases/7-day cleanup and interrupted recovery; `storage_tool_output/access.rs` — exact operation/session/owned-lineage regular-file streaming read/grep and source/cursor budgets; `runtime/turn.rs` — common permission/Ask/Deny and joined worker, no artifact AGENTS discovery; `mcp_result/media.rs` — textual facts preparation with separate media admission | `config/tool_output_tests.rs`, `tools/output/tests.rs`, `storage_tool_output/tests.rs`, application `tool21_` reload fixture; direct normal ELF `evidence/T50/native_tool_output_common.py`. OC2 MIT counting/config fixtures pinned at `2670273ff17da96f85c5826ced57aa1b368754fa`, `packages/core/src/tool-output.ts`, schema/config and config/plugin/tool-output. Native served64KiB/capture16MiB/shared2GiB/7-day differences are explicit; metadata.truncated bypass is not ported. Old full-inline records unchanged. |
| Shell before-discard resource capture | `shell.rs` — owned nonblocking/joined drain readers and existing supervisor; `shell/jobs.rs`, `shell/jobs/output.rs` — one Capture/registered Writer, per-stream incremental UTF-8/redaction,64KiB recent windows/combined tail, serialized safe publication labels plus bounded original ingress/carry provenance and synchronous backpressure without a queue; `storage_shell_jobs.rs` — same descriptor in terminal outcome/notice/recovery; common `prepare_stream` reuses its artifact | `shell/jobs/output_tests.rs` (distant stdout, split secret/UTF-8 and publication-order facts, short Interrupted/known exit0, cap/quota, IO/leases, stalled publication/cancel); existing TOOL13 controls; native common chain, `native_shell_controls.py`, `native_shell_projection.py` (same-task switch/manual compact/restart). Capture loss stays separate from actual exit/effects, including short incomplete output; no new shell/store owner or terminal recapture. |
| Общие типы, CoreApp API и ports | `crates/oc-core/src/{core_app,application,queries,ports,domain}.rs` | Не превращать этот набор в новую иерархию в T52; core без UI/storage/network зависимости. Читать конкретный DTO/метод, не весь crate. |
| Config/composition/provider/tools | соответствующие `crates/oc-adapters/src/{config,composition,defs,discovery,provider,tools,patch,shell,webfetch}.rs` | Нет массового переноса в T52. При новом feature сначала найти существующего владельца; существенные новые tests отдельно по общему правилу. |
| Profile discovery / exact model references / primary eligibility (T45/R6) | `defs.rs` — admitted recursive agent/agents and flat mode/modes, supplied fields and registration order; `models.rs::parse_reference`; `composition.rs` — complete-catalog admission and initial selection; `application.rs` — explicit selection and automatic primary catalog; `runtime/turn.rs` — own-model child; `provider.rs::RequestOverlay` — profile `request` headers/body (V1 `options`/`temperature`/`top_p`) validated at load and applied per root/child lane; `defs.rs::agent_color` → `TuiChrome.agent_colors` → `oc-tui` `TuiState::agent_color` explicit-first | `defs/profile_tests.rs`, `application/profile_tests.rs`, filter `r6_` (also `tests/subagent.rs::r6_profile_request_overlays_*`); normal ELF `evidence/T45/native_profile_discovery.py`. Truncated catalogs fail before fallback; hidden explicit addressing and saved unavailable choices remain separate. |
| Native webfetch formats / total deadline (TOOL17) | `oc-adapters/src/webfetch.rs` — strict network-free input, existing guarded per-hop actual dialing, capped body and joined conversion; `webfetch/render.rs` — bounded streaming html5ever token renderer, no DOM/JS/resource loading; `tools.rs`/`runtime.rs` — result metadata/catalog through the existing permission/typed terminal owner | `webfetch/format_tests.rs`, `tools/tests/webfetch.rs`, existing TOOL07/08/AUD25/26 and approval lifecycle; direct normal ELF `evidence/T50/native_webfetch.py`. Native calls never send auth; public legacy `fetch` keeps explicit first-hop bearer compatibility. |
| Native grep/glob semantics and scan admission | `oc-adapters/src/files/search.rs` — pinned regex/ignore engines, descriptor-relative traversal, scopes and cancellable budgets under `Files`; `tools.rs` — canonical argument validation, read-Deny ceiling and single-page lookahead; `runtime/turn.rs` — owned blocking scan, invocation-token bridge and join-before-finish; `runtime.rs`/`approval.rs` — catalog and shared Ask preview | `files/tests.rs` under `files::search::tests` includes private traversal barrier + current-thread real Runtime cancellation/next-query proof; `tools/tests/search.rs`; existing TOOL01/AUD19 regressions and direct held fake-provider ELF `evidence/T50/native_search.py`. Legacy patch `glob_match` is not the search engine. |
| Native read text/directory/images (TOOL16) | `oc-adapters/src/files/read.rs` — descriptor-pinned regular reads, sorted directory pages and bounded vetted pixel/animation validation, reusing `files/search.rs::Budget` for cooperative 30-second checks; `tools/read.rs` — strict arguments, String compatibility, typed local-source image facts and ordered Responses lowering; stored reconstruction revalidates bytes through the same decoder without filesystem access, live construction keeps single-pass validation and the invocation token; `runtime/turn.rs` — shared admission/blocking worker, retained-facts/model budgets and atomic tool/instruction finish; `tools/mcp_log.rs`, existing DCP/fork/compaction owners — separate local attachment, original call graph and immutable replay | `files/read/tests.rs`, `runtime/read_tests.rs` under existing media tests, filter `tool16_`; shared `files::search::tests::tool14_active_scan_cancel_refuses_partial_and_joins_before_next_query` also exercises read. Direct normal ELF `evidence/T50/native_read.py`. PDF excluded; no implicit URI fetch or MCP provenance for local files; decoder exact `image 0.25.9`, png/jpeg/gif/webp only. Cooperative checks do not preempt kernel-stalled IO. |
| Command shell / legacy argv compatibility | `oc-adapters/src/tools/shell_call.rs` — typed invocation admission; `shell.rs` — Linux selector and single process-group supervisor; `approval.rs`, `permissions.rs`, `runtime/turn.rs` — pinned cwd, alias policy and catalog; `storage_grants.rs` — distinct command/argv saved-resource domains; `oc-tui/src/tools.rs` — existing shell-card consumer | `tools/tests/shell.rs`, `tests/fixtures/approval_lifecycle.rs` under existing runtime target; direct ELF `evidence/T50/native_foreground.py --background-supported`. Canonical `shell`, hidden legacy `bash`. |
| Owned foreground/background shell / live controls / automatic notice | `oc-adapters/src/shell/jobs.rs` — bounded common admission, actual drain buffers/shared joins, same-process conversion and selected control; `storage_shell_jobs.rs` — original versioned identity/outcome and existing event-ledger mode/delivery in the same Db/flock; `application.rs` — scoped current inventory/snapshots and idle/busy pumping; `runtime/{turn,context}.rs`, `storage_dcp_view.rs`, `storage_fork.rs` — original result/notice projection; `oc-core/src/{queries,core_app}.rs` — source-bound running/snapshot/control ports; `oc-tui/src/shell_jobs_view.rs`, `app/{live,input}.rs`, `oc/src/tui_cmd.rs` — lower Shell composer, pinned viewer and same-ID Location adoption | `shell/jobs/tests.rs`, `shell_jobs_view/tests.rs`, existing `tests/fixtures/background_lifecycle.rs` and `storage_fork_dcp_tests.rs`; direct normal ELF `evidence/T50/native_shell_controls.py` and `native_background.py --shell-controls-supported`. Ctrl+S opens the functional consumer; selected Ctrl+B/Ctrl+D reuse the owner. Original tool results and execution provenance stay immutable; restart reports unknown and never replays. Common cold artifacts/R10 and PTYs/T56 remain separate. |
| Responses physical request / typed provider failures | `oc-adapters/src/provider.rs` — one physical attempt, bounded completed-item reconciliation with actual emitted reasoning ciphertext provenance, local structural diagnostics and counted dispatch boundary; `provider/failure.rs` — source-derived structured classifier and safe observed header/delivery facts; `runtime/turn.rs` — typed compatibility and truthful length finish | Existing inline fixtures plus `provider/tests/{typed_failures,reconciliation}.rs`, filter `provider::`; target `runtime`, pack `tests/runtime/reconciliation.rs` for admission/effect guards; `storage_fork_dcp_tests.rs` for canonical opaque/binding retention. T55 normalized fixture/evidence: `evidence/T55/{sparse-completed.fixture.json,reconciliation.md,reasoning-replay.md}`; direct normal ELF headless/PTY/effect/reopen/owned relay proof: `evidence/T55/native_completed.py` (optional `--reasoning-replay`), report `native-qualification.md`. R1 historical native fixture: `evidence/T54/native_typed.py`; current runtime native proof below. No adapter retry/delay loop. |
| Logical-step retry / durable assistant spans | `oc-adapters/src/runtime/retry.rs` — shared finite policy and cancellable epoch wait; `runtime/turn.rs` — transparent retry/partial continuation; `runtime_compaction.rs` — native/generated auxiliary allowance and one correction; `tools.rs`/`storage.rs` — existing turn JSON, safe span projection/recovery and operation dispatch events; `application.rs` — database-free title accounting through its owner channel; `oc-core/src/{queries,core_app}.rs` → headless/TUI scoped events and existing deadline scheduler | `runtime/retry/tests.rs`, integration `runtime`, `runtime_compaction_tests.rs`, `oc-tui/src/app/tests/retry.rs`; direct ELF `evidence/T54/native_runtime.py`. One request per adapter call; retry never admits incomplete tools or dispatches from cached due facts. VIS43 paired visuals remain T44. |
| Compiled plugin admission / current inventory | `oc-adapters/src/config.rs::classify_plugin` — exact pre-resolution gate; `composition.rs` — per-request binding; `oc-core/src/queries.rs::{PluginInventory,ServiceDiagnostic}` → existing application catalog → Settings / `tui_cmd.rs` | `src/{config,composition,application}/plugin_tests.rs`, filter `cfg09_`; actual binary `oc/tests/pty_t39/plugin_admission.rs` plus `configured_workspace::aud17_binary_unknown_plugin_is_isolated_without_loader_side_effect`. Presentation bound never limits admission; no JS/plugin loader or second resource owner. |
| Provider cold availability / native catalog readiness | `oc-adapters/src/composition/provider_readiness.rs` — admitted binding facts, original `discovery.rs` oracle; `application/provider_catalog.rs` — single owner job and safe-boundary publication; `runtime.rs::admit_provider` / `runtime/turn.rs` — pre-effect admission; `provider.rs::same_request_binding` — canonical actual URL/headers; `oc-core/src/queries.rs::ProviderReadiness` → scoped catalog / `ProviderChanged` → `oc/src/{tui_cmd,headless}.rs` | `src/application/{provider_tests.rs,provider_catalog/tests.rs}`, `tests/runtime/provider_readiness.rs`, `oc-tui/src/app/tests/input.rs`, filter `ui07_`; actual binary `oc/tests/pty_t39/provider_readiness.rs` under existing target. Provider availability is ephemeral; mandatory source/policy admission and complete-generation reload remain atomic. |
| Safe config/fatal causes and optional-definition inventory | `oc-adapters/src/config/diagnostic.rs` — schema-only fields and shared source qualifier; `composition.rs::LoadFailure`, `defs.rs::Diagnostic` → `application.rs::{SpawnDiagnostic,runtime_issue}`; `oc-core/src/queries.rs::ServiceDiagnostic` / `core_app.rs::WorkerGuard` → `oc/src/{tui_cmd,headless}.rs`, `oc-tui/src/shell.rs` fatal frame and `app/input.rs` read-only details/copy/investigation | `src/application/fatal_tests.rs`, `oc-core/src/core_app/diagnostic_tests.rs`, TUI `app/tests/input.rs`, filter `cfg10_`; existing binary targets `recovery_startup/fatal_diagnostics.rs`, `pty_t39/fatal_diagnostics.rs`. Broken policy documents stay fatal; optional definition namespaces are the narrow recoverable authority, cleanup/caps/cancel keep typed non-success. No second error registry or source engine. |
| Effective model admission / variants | `crates/oc-adapters/src/models.rs` (including unavailable-selection display identity); `application.rs::Effective::snapshot`; `oc-tui/src/picker.rs` | Один stable canonical effort view после merge; metadata disabled/reserved сохраняется, available consumers фильтруют её без отдельной lexical sort. Retired display aliases never replace raw admission choices. `models/tests.rs`, filter `models::ordering_tests::`; actual cycle/wire/restart — `oc/tests/pty_t39/interaction.rs::var01_`. |
| Native model lookup / session rename (TOOL18) | `models/lookup.rs` — borrowed generation + selected merged catalog read-view, bounded schema-known metadata/shared variants; `config.rs` — inert unselected static metadata in the existing generation; `runtime/turn.rs` — default-current exact-resource admission and Location/lineage rechecks; `application.rs::commit_session_rename` → `storage.rs::rename_session` — existing atomic title/event transaction and durable frontend publication | `models/tool_tests.rs`, `tests/runtime/model_session_tools.rs` under existing runtime target; `tests/session_rename.rs` existing root/UI contract; direct normal ELF `evidence/T50/native_model_session.py`. Root explicit target must be idle; child only self, General/Explore mutation ceiling. No discovery/select/auth side effects, second store or Inbox self-wait. |
| Catalog-only CLI `oc models` (TOOL18 supplement) | `oc/src/{cli,bootstrap,models_cmd}.rs` — pre-storage dispatch and ID-only output; `oc-adapters/src/composition.rs::{admit_sources,admit_dcp}` — shared pinned sources; `composition/catalog.rs` — selection-independent read-view; `config.rs::assemble_catalog_admitted` — policy/metadata assembly with only dynamic source credentials and inert MCP; `composition/provider_readiness.rs` / `discovery.rs` — original bounded atomic discovery and safe causes | `composition/catalog/tests.rs`, `oc/src/models_cmd/tests.rs`; actual normal debug/release ELF `evidence/T50/cli_models.py`, receipt `cli-models.md`. Exact full-reference lexical order, no family/page reduction or Db/recovery; required discovery failure is incomplete/nonzero. Future T53 source integration and T45 profile binding retain their owners. |
| Native same-ID placement move / full terminal handoff (TOOL19) | `application/session_move.rs` — original-context resolution, descriptor/trust/full-generation admission and pinned private handoff; `runtime/turn.rs` — exact target/directory common policy and immutable source execution; `storage_session_move.rs` — migration6, one bounded pending identity and atomic placement/deck/event commit in the existing Db; `application.rs` — full-terminal supervisor, owned stop/join then sticky quarantine and shared Jobs transfer; `runtime/context.rs` — placement-epoch fresh provider projection; `oc/src/tui_cmd.rs` — typed applied-location/deck adoption | `storage/session_move_tests.rs`, `tests/runtime/session_move.rs`, `models/tool_tests.rs`, filter `tool19_`; actual normal ELF `evidence/T50/native_session_move.py`. Pending results stay immutable; unknown source recovery cannot apply/replay, known-terminal pending recovery re-admits the same fingerprint. Parent move retains child placement and original background provenance; ordinary UI scoped selection remains distinct. |
| Native edit/write and captured file-family admission (TOOL20/R1) | `patch/mutation.rs` — bounded donor text preparation/matching, preview identity and result; `patch.rs::commit_prepared`, private `patch/{fs,effects}.rs` — single descriptor-relative commit/effects owner shared with strict patch; `runtime.rs::selected_tool_defs`, `runtime/turn.rs` — captured request family, common permission/grant/preimage/intent admission; `runtime/context.rs` — parsed mutation path only; `oc-tui/src/{history,tools,patch_view}.rs` — persisted result-derived effects consumer | `patch/mutation/tests.rs`, `tools/tests/file_mutations.rs`, `runtime/file_tool_tests.rs`, integration `tests/runtime/file_mutations.rs`; direct normal ELF `evidence/T50/native_file_mutations.py`. Native patch grammar/matching unchanged. Issuing request retains its family through later committed switches; live-switch owner below. VIS35/36 paired geometry remains T44 paused. |
| Same-task model draft/commit + actual request identity (TOOL12) | `application_selection.rs` — existing scoped prefs + atomic selection fact; `application.rs` — busy authorized commits through the existing inbox and monotonic configuration/Location epoch; `oc-core/src/{queries,core_app}.rs` — scoped caller/revision receipts; `oc-tui/src/app/model_selection.rs` — composer-owned agent drafts and bounded acknowledgements; `runtime/turn.rs` — one per-attempt prepared model/budget/DCP/tools/guidance view, finite retry/compact rebuild; `tools/model_history.rs` — compatible closed ordinary groups without alien opaque state, original instruction positions; existing turn JSON/span identity → `storage.rs`, `app/{live,transcript}.rs`, `history.rs` actual attribution | `application/live_switch_tests.rs`, `app/tests/model_selection.rs`, existing retry/runtime/compaction/instruction targets; filter `tool12_`. Direct normal debug/release ELF `evidence/T50/native_live_model_switch.py`: stream/tool/Ask, variant-only, retry, summary/rebuild, restart/no-wake and independent own-model child. No schema, second store/queue, archive reload or tool replay; configuration/agent/Location and old prepared requests remain pinned. T45 PRM01 and T44 paired visuals retain separate ownership. |
| Saved unavailable agent/model/variant / explicit scoped repair | `oc-adapters/src/application.rs::Effective::{apply_persisted_agent,selection_issue,admit_selection}`, `application_selection.rs::{base_for_agent,home_current,for_turn}` — only prefs owner; `oc-core/src/queries.rs::SelectionReadiness` → `oc-tui/src/app/{live,input}.rs` and `picker.rs`, `oc/src/{tui_cmd,headless}.rs` | `application/inherited_tests.rs`, TUI `app/tests/input.rs`, filter `r4a_`; actual offline binary `oc/tests/pty_t39/inherited_selection.rs`. A saved valid-shaped missing reference remains visible but cannot accept a turn; explicit per-Location/agent/model/variant choices use the existing store, not a UI-owned JSON rewrite. |
| Runtime fake-wire integration suite | `crates/oc-adapters/tests/runtime.rs` — общий Fake/Harness/SSE; `tests/runtime/{turns,context,tool_lifecycle}.rs` — только нужная группа сценариев | Один прежний target `runtime`, без новых Cargo targets; `tests/fixtures/approval_lifecycle.rs` подключён единожды из root. `cargo test -p oc-adapters --test runtime --locked`. |
| Actual-binary PTY suite T39 | `crates/oc/tests/pty_t39.rs` — общий fixture; `tests/pty_t39/{interaction,lifecycle,plugin_admission,provider_readiness,fatal_diagnostics,inherited_selection}.rs`; `tests/support/{terminal,title,screen}.rs` только по зависимости | Один прежний target `pty_t39`; support declarations и shared fixture остаются в root. Bounded text-screen reconstruction из `support/screen.rs` также используется `recovery_v02` для cursor-aware assertions. `cargo test -p oc --test pty_t39 --locked -- --test-threads=1`. |
| Test-only durable bounded live envelope | `scripts/bounded_live.py` — explicit campaign identity, fsynced request journal and owned HTTP interposition; `crates/oc/tests/live_bounded/envelope.rs` — child ownership and native restart proof | `scripts/test_bounded_live.py` uses loopback peers only; existing target `live_bounded`, filter `envelope::`; T55 direct-ELF caller `evidence/T55/native_completed.py::Relay` requires explicit existing identity and RAM manifest. Live runners require explicit opt-in and an existing campaign ID; restart must reuse that journal. No product quota or automatic campaign reset. |
| Capture/oracle/frontend tooling | конкретный script в `scripts/tui_capture/`, указанный активным VIS-контрактом | Это dev tooling, не production TS host. Не читать весь каталог/`capture.mjs` без конкретной задачи; пути source inventory поправить только в живом producer/consumer. |

External read/search (TOOL14/TOOL16): `files.rs::{concrete_scope,pin_external,resolve_read,open_read_scope}` extends the same read/search descriptors with invocation-only authority; `runtime/turn.rs` home/Location normalization and `approval.rs` reuse shared boundary/action admission. Mutation resolution and instruction/config roots stay distinct. Nearest proofs: `files/external_tests.rs`, the existing active-scan cancellation test, runtime `approval_lifecycle::external_read_shared_boundary_and_action_admission_emit_only_genuine_asks`, and direct normal ELF `evidence/T50/native_external_read.py`; results in `evidence/T50/external-read.md`.

T53 local config: `oc-adapters/src/config/providers.rs` normalizes both provider
roots and recursively merges supplied overlays, with `providers_tests.rs`.
`config.rs` retains field provenance; model/variant readers accept canonical arrays.
`config/request_bindings.rs` captures provider→model→variant templates with tuple
source provenance; application resolves each admitted auth scope before publication.
`provider.rs::for_selection` is the common immutable dispatch/admission consumer;
catalog IDs stay distinct from wire modelID. Nearest tests: `request_bindings_tests.rs`
and `application/effective_wire_tests.rs`.

T53 public Go source: `oc-adapters/src/models_dev.rs::GoCatalog` owns bounded,
credential-free `https://models.dev/api.json` refresh, sanitized source-qualified
SQLite preference cache and single-flight/TTL/last-good state. Only Go metadata
survives normalization; tests `models_dev/tests.rs`. The existing Db shares this
owner with all its handles; `composition/go_catalog.rs` attaches last-good data,
builds finite model/variant bindings and separates public fetch status from paid
auth readiness. Application `provider_catalog.rs` owns startup/picker refresh and
cancels/joins before Location replacement; cached reads do not await GET. Tests
`composition/go_catalog/tests.rs`. Selection-independent `composition/catalog.rs`
feeds `oc models` through a bounded read-only public-pref snapshot (no Db startup);
`models/lookup.rs` feeds the native tool through the existing Db cache owner. Live
WAL/no-store-mutation CLI test: `oc/tests/configured_workspace/models_cli.rs`;
public merge/retirement tests: `models/tool_tests.rs`, `models_dev/tests.rs`.
Authenticated OpenProxy discovery remains independent.

T53 native wires: `oc-adapters/src/provider/{chat,messages,protocol}.rs` owns
finite lowering and independent decoders; `provider.rs` shares HTTP/SSE caps,
cancel and one-attempt dispatch; `config.rs::provider_wire` admits explicit
package/compatibility/static Messages auth. `provider/settings.rs` admits typed
wire options and consumes provider body overlays across the common dispatch;
`settings_tests.rs` covers typed failures and all three real fake-server routes.
`provider/timeout_tests.rs` qualifies native positive-millisecond total deadlines
and independent idle timeout without weakening one-attempt/delivery facts.
`ProviderTimeout` admission is in `config.rs`; numeric budgets are captured in the
wire binding and consumed by the common `provider.rs::stream_body` transport.

Chronology: `application_selection.rs` commits typed effort metadata through the
existing event transaction; `storage_effort.rs` and `runtime/context.rs` project
durable positions, `runtime/turn.rs` captures busy updates, `provider/protocol.rs`
lowers declared capabilities. Tests: `provider/protocol_tests.rs`,
`application/live_switch_tests.rs`, `runtime/tests.rs` (`go03_` filters).

Durable wire authority: core `queries::{NativeProtocol,WireProvenance}` and adapter
`ResponsesConfig::provenance` feed prepared receipts/TurnLog. `tools/model_history.rs`
withholds opaque state without an exact binding; SQL `storage_dcp_view.rs` and HOT
`tools/turn_history.rs` preserve original provenance. Nearest tests:
`tools/model_history_tests.rs`, `storage_turn_history/tests.rs` (`go04_`). Native
checkpoint projection and main/compaction/child fences use captured variant routes;
`runtime/binding_replay_tests.rs` qualifies real tool pairs across all wires,
reopen and fork. Native origin/variant cases are in `runtime_compaction_tests.rs`.

`provider/context.rs` captures project/session/parent identity and supported lineage
cache fields at the common send boundary; `context_tests.rs`, effective-wire title,
compaction and recursive-fork tests cover it. `application.rs` currently exceeds
5k because its supervisor/title paths retain one mutation owner; its qualified
selection growth is still owner-local. Next natural seam is title preparation and
catalog/query projection, not another supervisor or credential owner. `runtime/turn.rs`
also exceeds 5k (5,157 lines, +47 in auth preparation and +1 for operation capture);
target capture/preparation is its
next natural seam. Large turn/child futures are heap-pinned at execution boundaries.
Nearest tests:
`provider/{chat,messages,protocol}_tests.rs`, `application/{chat,messages}_wire_tests.rs`;
receipts `evidence/T53/{chat,messages}-wire.md`. Full Go/catalog/credentials/
replay/metadata qualification remains T53.

New-wire runtime retry qualification: `runtime/binding_replay_tests.rs` drives
Chat/Messages through pre-output retry and durable post-output continuation,
asserting counted physical attempts and no partial/settled tool replay.

T53 accounts: `oc-adapters/src/storage_credentials.rs` owns the additive account
schema/lifecycle in `Db`, with `storage_credentials/tests.rs`; `auth.rs` is the narrow
admitted Go/custom credential resolver. Application startup/reload resolves once per
generation. `application/accounts.rs` routes typed `CoreApp::provider_accounts`
through that same owner; safe DTOs live in core `queries/accounts.rs`. Credential-only
idle publication retains configured auth inputs and does not restart MCP or select
a model. Tests: `application/accounts_tests.rs` and held live-switch regression.
Qualified model choices use `application_selection.rs` and `Effective::request`;
legacy unqualified records remain bound to their original provider namespace.

Endpoint authority: `oc-adapters/src/endpoint.rs` captures origin/prefix/source trust;
generation and configured discovery share its DNS pin and peer guard. Private boundary,
redirect/no-forwarding and anonymous discovery tests: `endpoint/tests.rs`.

T53 unchosen startup stays in `composition.rs` and application selection admission;
`CatalogSnapshot::selected_model()` is the optional composer query, with empty
legacy display id only for absence. Tests: `application/configless_tests.rs`,
`oc-tui/src/app/model_selection_tests.rs`, actual-binary configured-workspace and
startup PTY. Missing optional connections are local state; invalid policy stays fatal.

T53 masked connection UI lives in `oc-tui/src/app/accounts.rs` (ephemeral field,
provider chooser from typed `CoreApp::provider_connections`, admitted IDs only,
safe metadata and confirmation), with `dialog.rs` painting and binary
`tui_cmd.rs` typed owner ACK routing. Tests: `app/accounts_tests.rs` and actual
`oc/tests/pty_t39/accounts.rs`. It does not reuse composer/editor undo or copy.
Independent frozen connection views: `composition/provider_views.rs` admits and
resolves connection inputs under the existing Location/source/Db owners;
`CoreApp::provider_catalog` exposes cached safe rows without committing selection.
Public and configured discovery jobs are independently joined/cancelled by
`application/provider_catalog.rs`; the shared Go cache still owns single-flight.
`TuiState::apply_picker_catalog` updates a filtered browse scope without replacing
the committed composer. Tests: `application/provider_view_tests.rs`,
`application/provider_catalog/tests.rs`, `app/model_selection_tests.rs` and account
PTYs. `Composition::request_provider` captures a finite map of admitted leaf views;
each runtime prepared attempt selects exactly one catalog/config from that map.
`application/provider_view_tests.rs` qualifies a real Responses→Messages→Responses
busy switch, same slash-containing ID, independent credentials, receipts and restart.
Qualified child/profile/command targets resolve from the issuer's immutable map
in `runtime/turn.rs` and `runtime/turn/commands.rs`; application forks pass the
frozen admitted provider set to `storage_fork.rs`, preserving original receipts
and settled pairs. The same provider-view test owner covers actual mixed-wire
child and qualified fork/reopen regressions.

T53 fixed-authority opt-in qualification: `provider/go_live_tests.rs` and
`scripts/t53_go_live.py`; a `cfg(test)` captured hook reserves a shared fsynced
ledger before DNS/dial, with 24 attempts/2048 output and no production authority,
retry or successful-step change. Evidence/receipt: `evidence/T53/go-live.md` and
`evidence/T53/live-campaign.json`.

T56 session PTYs: `oc-adapters/src/terminals.rs` owns bounded Linux descriptors,
interactive process groups, joined drains and pinned vt100 cells/cursor/replay;
`storage_terminals.rs` adds identity/selection/lifecycle only in the existing Db;
`application/terminals.rs` admits acknowledged controls; `runtime/terminals.rs`
uses current-root or retained-child source/shell/environment, not parent rebinding.
`oc-core/src/queries/terminals.rs` is OS/UI-free DTOs. UI consumer
`oc-tui/src/terminal_view.rs` owns lower composer/raw focus/cell projection;
`oc/src/tui_cmd/terminal_controls.rs` owns captured Core dispatch/attach/resize,
including child-close-before-target. Nearest tests: `terminals/tests.rs`,
`application/terminal_tests.rs`, `runtime/children/tests.rs`, `terminal_view/tests.rs`
and binary `tui_cmd/terminal_controls/tests.rs`, filter `term01_`. Actual native
debug/release PTY/process/crash fixture: `oc/tests/terminals.rs` with
`support/terminals.py`. Receipts:
`evidence/T56/{owner,application,frontend,term01,signals,migrations}.md`;
`evidence/T56/report.md` closes R1–R4 with actual debug/release TERM01 and final
workspace gates. T44/VIS39 paired presentation remains a separate owner.

T57 OpenAI credentials: `auth/openai.rs` owns source-derived bounded token parsing
and one shared-Db refresh flight; `storage_credentials.rs` owns additive tagged
method/metadata, identity/version CAS and durable unknown-refresh reservation.
Nearest tests `auth/openai/tests.rs` and `storage_credentials/tests.rs`, filters
`auth03_` / `auth05_`; existing `go01_` remains T53-owned. Partial receipt
`evidence/T57/credentials.md`. `auth/attempts.rs` owns bounded ephemeral browser/device
workers/listeners/cancel/expiry and store-once acknowledgement; `queries/auth.rs`
contains redacted typed projections, `auth/callback.html` the source-derived local
page. Nearest `auth/attempts/tests.rs`, filters `auth01_`/`auth02_`/`auth03_`, receipt
`evidence/T57/attempts.md`. `application/authentication.rs` admits typed Core auth
methods/actions and joins the same owner at every application exit; safe method
metadata is projected through `application/accounts.rs`. Nearest scenarios
`application/authentication_tests.rs`, filter `auth01_`, receipt
`evidence/T57/application.md`. `composition/provider_readiness.rs` shares async
scope resolution with independent views; `provider.rs`/`provider/context.rs` project
captured OpenAI account/session headers and replay authority. Nearest
`auth/openai/binding_tests.rs`, `auth04_`, receipt `evidence/T57/bindings.md`.
`models_dev.rs` is the single public Go/OpenAI source/cache/flight owner;
`composition/openai_catalog.rs` the exact subscription-only numeric overlay.
`composition/{catalog,go_catalog,provider_views}.rs` and `models/lookup.rs` project
that same source through read-only listings/captured views/model lookup. Protected
metadata-only SQLite reads share `storage.rs`'s existing read-only connection guard.
Nearest `models_dev/openai_tests.rs`, `composition/openai_catalog/tests.rs` and
`composition/catalog/tests.rs`, filter `auth04_`; receipt `evidence/T57/catalog.md`.
`auth/openai.rs::prepare_request` is the native selected-leaf refresh/capture boundary
for `runtime/turn.rs`, summary retries and both `application.rs` title lanes. Existing
`ServiceDiagnostic` carries explicit reauth/model-selection refusals; `provider.rs`
re-applies native identity after profile overlays. Nearest `auth/openai/binding_tests.rs`
and `provider/tests/openai.rs`, filter `auth04_`; `evidence/T57/preparation.md` is partial
request-preparation proof. `provider/websocket.rs` owns one shared-Db bounded native
session channel/affinity/continuation; `provider/failure.rs` and the shared decoder
own delivery/retry/classification/redaction facts. Nearest real-peer scenarios
`provider/websocket/tests.rs`, filter `auth04_`; `evidence/T57/websocket.md` is checked
local channel evidence. The same coarse fixture now qualifies actual Runtime
read/follow-up/fork/opaque/retry/child/ordinary-summary consumers;
`evidence/T57/runtime-ws.md` records those effects, not rebuilt CLI/TUI or live login.
`oc/src/auth_cmd.rs` is the built-in auth CLI consumer of the same credential/attempt
owner, without runtime model/session/MCP startup. `auth.rs::AuthScope::methods` shares
selectable method labels/order with Core/application. Nearest `auth_cmd/tests.rs` and
actual-binary `oc/tests/auth_cli.rs` / `support/auth_cli.py`, filter `auth05_`;
`evidence/T57/cli.md` qualifies metadata, masked TTY inputs and owned callback/cancel,
not successful real authorization or T44/VIS45 presentation.
`oc-tui/app/accounts.rs` owns one boxed ephemeral method/account/auth surface;
`oc/src/tui_cmd/auth_controls.rs` joins mounted Begin/status/cancel/open work through
the same Core owner and bounded deadlines. `queries/auth.rs` carries a structured
ephemeral device code; no prose parsing. Native account/model previews are in
`composition/{provider_readiness,provider_views}.rs`, without mutating issued requests
or exchanging tokens. Nearest `app/accounts_tests.rs`, `auth_controls/tests.rs` and
`composition/openai_catalog/tests.rs`; receipt `evidence/T57/tui.md`.

## Как обновлять

Изменившийся owner указывает точку входа, свои части и один ближайший test target/filter. Карта — маршрутизатор, не полный symbol index и не копия архитектурного spec. No generic helpers/common-manager dumping ground. Для малого изменения внутри неизменного владельца новая строка не нужна.

Обоснованное временное превышение 5k указывать в строке владельца: `path — причина связанности/риска — следующий естественный шов или условие пересмотра`. Это warning, не бессрочное разрешение растить монолит. Текущий список предупреждений и размерные изменения доступны через `python3 scripts/code_size.py --base <commit> --changed`.
