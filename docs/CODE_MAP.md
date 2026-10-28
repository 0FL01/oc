# Карта владельцев кода

Поведение → owner → ближайшая проверка. Читать нужный маршрут, не всю карту.
Границы/раскладка — [ARCHITECTURE](ARCHITECTURE.md), команды/live safety — [TEST_PLAN](TEST_PLAN.md).
Пути внутри раздела относительно указанного `src/`; test filters — подсказки для локализации, не отдельные gates.

## Команды

```sh
cargo test -p oc-core --lib --locked <filter>
cargo test -p oc-adapters --lib --locked <filter>
cargo test -p oc-tui --lib --locked <filter>
cargo test -p oc2 --bin oc2 --locked <filter>
cargo test -p <package> --test <target> --locked <filter>
```

Ниже `unit X` — фильтр соответствующего crate; `target X` — integration target.
PTY: `cargo test -p oc2 --test pty_t39 --locked -- --test-threads=1`.
Общая integration boundary: `bash scripts/test.sh`.

## Core — `crates/oc-core/src/`

- Typed façade/inbox/events: `core_app.rs`; DTO/query/ports/domain: `application.rs`, `queries.rs`, `ports.rs`, `domain.rs`. Tests: unit `core_app::`.
- UI completion wake (receipts/jobs wake the event loop instead of a timer poll): `wake.rs`. Tests: unit `wake::`.
- Questions/prompt images/tool facts: `question.rs`, `prompt.rs`, `tool_output.rs`. Tests: unit `question::`, `prompt::`, `tool_output::`.
- Selection/accounts/auth/PTY DTOs: `queries/model_preference.rs`, `queries/accounts.rs`, `queries/auth.rs`, `queries/terminals.rs`. Tests: unit `queries::`.

## Authority/application — `crates/oc-adapters/src/`

- Shared owner/UDS/leases/wire: `native/`; sole Db/recovery/runners: `application/authority.rs`; bounded events: `application/authority/live.rs`. Tests: unit `native::`, `application::authority::`; `oc2` target `pty_t42`, filter `srv`.
- Generations/queries/session/forks: `application.rs`; durable tab deck: `application_tab_deck.rs`. Tests: unit `application::`; target `tab_deck`; `oc2` target `memory_bounds`.
- Committed model vs picker draft: `application_selection.rs`, `application.rs`, `runtime/turn.rs`. Tests: unit `tool12_`, `application::live_switch_tests`; TUI unit `model_selection`.
- Unavailable/configless start: `application.rs`, `application_selection.rs`; catalogs/readiness: `application/provider_catalog.rs`, `composition/provider_readiness.rs`, `composition/provider_views.rs`. Tests: unit `r4a_`, `ui07_`; `oc2` target `pty_t39`, filters `inherited_selection`, `provider_readiness`.
- Picker/accounts/connect: `application/model_picker.rs`, `application/accounts.rs`, `application/authentication.rs`, `application/provider_connections.rs`. Tests: unit `application::model_picker`, `application::accounts`, `conn0`.
- Reload/source publication: `application/reload.rs`, `runtime/publication.rs`, `composition/watch_inputs.rs`, `composition/reload_inputs.rs`, `composition/watch_inputs/observer.rs`. Tests: unit `hot02_`, `runtime::publication_tests`, `application::tests::reload_tests`.
- User Shell: `application/user_shell.rs`, `runtime/user_shell.rs`, `storage_shell_jobs.rs`; recall: `storage/prompt_history.rs`. Tests: unit `application::user_shell`, `storage::prompt_history`.
- Same-ID move: `application/session_move.rs`, `storage_session_move.rs`, `runtime/turn.rs`, `runtime/context.rs`. Tests: unit `tool19_`; target `runtime`, filter `session_move`.
- Session PTY: `application/terminals.rs`, `runtime/terminals.rs`, `terminals.rs`, `storage_terminals.rs`. Tests: unit `term01_`; `oc2` target `terminals`.

## Runtime/context/children — `crates/oc-adapters/src/`

- Execution/admission/prepared request/full batch: `runtime.rs`, `runtime/turn.rs`, `runtime/tool_stream.rs`. Tests: unit `runtime::`; target `runtime`.
- Retry/partial continuation: `runtime/retry.rs`, `runtime/turn.rs`; summary retry/install: `runtime_compaction.rs`. Tests: unit `runtime::retry`, `runtime::binding_replay_tests`; target `runtime`.
- Children/jobs/notices: `runtime/children.rs`, `runtime/turn/foreground.rs`, `runtime/turn/commands.rs`, `storage_children.rs`, `storage_conversation.rs`. Tests: unit `runtime::children`, `storage_children`, `storage_conversation`; target `subagent`.
- Environment/instructions: `runtime/environment.rs`, `composition.rs`, `instructions.rs`, `runtime/instructions.rs`, `storage_instructions.rs`. Tests: unit `runtime::environment`, `instructions::`, `prm01_`.
- DCP/context/compaction: `runtime/context.rs`, `runtime_compaction.rs`, `dcp.rs`, `dcp_auto.rs`. Tests: unit `dcp::`; targets `runtime`, `dcp_atomic`, `context_bounds`; `oc2` target `dcp_runtime`.
- RAW/HOT/renewal: `tools/turn_history.rs`, `storage_turn_history.rs`, `storage_dcp_view.rs`, `storage_compaction.rs`, `storage_fork_context.rs`. Tests: unit `storage_turn_history`, `dcp::renewal_tests`, `runtime::compaction_renewal_tests`.
- Wire authority/opaque/effort: `tools/model_history.rs`, `storage_dcp_view.rs`, `storage_effort.rs`, `runtime/context.rs`. Tests: unit `go04_`, `go03_`, `prm01_`; target `runtime`.

## Config/provider/auth/MCP — `crates/oc-adapters/src/`

- Allowlist/diagnostics/overlays: `config.rs`, `config/admission.rs`, `config/diagnostic.rs`, `config/providers.rs`, `config/request_bindings.rs`, `composition.rs`. Tests: unit `config::`, `application::effective_wire_tests`, `cfg10_`.
- Profiles/definitions/models/variants: `defs.rs`, `models.rs`, `models/lookup.rs`, `composition.rs`. Tests: unit `r6_`, `models::`, `defs::`; target `runtime`, filter `model_session_tools`.
- Process memory: `memory.rs` (owner allocator policy, post-turn/idle heap release), `shared.rs` (`Shared<T>` copy-on-write for captured bindings/catalogs). Tests: unit `models::tests::captured_catalog_clones_share_entries_and_mutate_copy_on_write`; target `memory_soak` (crate `oc2`).
- Compiled plugin admission: `config.rs::classify_plugin`, `composition.rs`, `application.rs`. Tests: unit `cfg09_`; `oc2` targets `pty_t39`, `configured_workspace`.
- Public catalogs/cache/list: `models_dev.rs`, `models_dev/document.rs`, `composition/go_catalog.rs`, `composition/openai_catalog.rs`, `composition/catalog.rs`. Tests: unit `models_dev::`, `composition::catalog`, `auth04_`.
- Discovery/custom endpoints: `discovery.rs`, `custom_discovery.rs`, `endpoint.rs`, `composition/custom_connections.rs`, `storage_provider_connections.rs`. Tests: unit `endpoint::`, `conn0`; `oc2` target `configured_workspace`.
- Responses/Chat/Messages/settings/errors: `provider.rs`, `provider/{chat,messages,protocol,settings,context,failure}.rs`. Tests: unit `provider::`; target `runtime`, filter `reconciliation`.
- Credentials/browser/device/WS: `auth.rs`, `auth/openai.rs`, `auth/attempts.rs`, `storage_credentials.rs`, `provider/websocket.rs`. Tests: unit `auth01_`, `auth03_`, `auth04_`, `auth05_`; `oc2` targets `auth_cli`, `auth_tui`, `auth_oauth`.
- MCP source/config admission: `config/mcp.rs`, `composition.rs::admit_local_mcp`; request leases/supervisor: `runtime/mcp.rs`, `runtime/mcp/lifecycle.rs`, `runtime/mcp/lifecycle/reconcile.rs`. Tests: unit `config::mcp`, `runtime::mcp`; targets `mcp_stdio`, `mcp_remote`; `oc2` target `mcp_application`.
- MCP transports: `mcp_stdio.rs`, `mcp_remote.rs`, `mcp_http_lifecycle.rs`; prompt/resource lookup: `application/mcp_lookup.rs`, `runtime/mcp/lifecycle/lookups.rs`, `mcp_lookup.rs`; media/logs: `mcp_result/media.rs`, `tools/mcp_log.rs`. Tests: unit `application::mcp`, `mcp_result::media`, `tools::mcp_log`; targets `mcp_stdio`, `mcp_remote`.

## Storage/tools — `crates/oc-adapters/src/`

- Db/transactions/recovery: `storage.rs`, related `storage_*.rs`; forks/conversation: `storage_fork.rs`, `storage_conversation.rs`. Tests: unit `storage::`; targets `storage_lock`, `dcp_atomic`, `tab_deck`.
- Tab unread/read watermarks: `storage_activity.rs`, core `queries/workspace.rs`, binary `tui_cmd/tab_activity.rs`; glyph/fade/drag: `oc-tui/src/app/tabs.rs`, `shell.rs`. Tests: `storage::activity`, `tui_cmd::tab_activity`, tab/shell unit tests, `pty_t42/shared_runtime.rs`.
- Canonical transcript/cursors: `storage_turn_history.rs`, `storage.rs::turn_presentation_window`, `application.rs`. Tests: unit `storage::transcript_tests`, `application::transcript_tests`, `application::authority::live`.
- Tool output/artifacts: `tools/output.rs`, `storage_tool_output.rs`, `storage_tool_output/access.rs`. Tests: unit `tools::output`, `storage_tool_output`, `tool21_`.
- Search/read/external scopes: `files.rs`, `files/search.rs`, `files/read.rs`, `tools/read.rs`; args/admission: `tools.rs`, `runtime/turn.rs`. Tests: unit `files::`, `tools::tests::search`, `tool16_`; target `runtime`, filter `approval_lifecycle`.
- Patch/edit/write: `patch.rs`, `patch/{mutation,fs,effects}.rs`; family: `runtime.rs::selected_tool_defs`. Tests: unit `patch::`, `runtime::file_tool_tests`; target `runtime`, filter `file_mutations`.
- Shell policy/process/jobs/capture: `tools/shell_call.rs`, `shell.rs`, `shell/jobs.rs`, `shell/jobs/output.rs`, `storage_shell_jobs.rs`. Tests: unit `shell::`, `tools::tests::shell`; target `runtime`, filters `approval_lifecycle`, `background_lifecycle`.
- Questions: `tools/output.rs::prepare_question`, `storage_tool_output.rs`, `application.rs`. Tests: unit `tool21_actual_oversized_question`; target `runtime`, filter `question_lifecycle`.
- Webfetch: `webfetch.rs`, `webfetch/render.rs`. Tests: unit `webfetch::`, `tools::tests::webfetch`.
- Code Mode: `codemode.rs`, `codemode/`, `runtime/code_mode.rs`; background owner/recovery: `runtime/code_mode/jobs.rs`, `storage_execute_jobs.rs`. Tests: unit `codemode`, `runtime::code_mode`, `application::authority::tests::execute`, `storage::execute_jobs`; target `runtime`, filter `code_mode`.
- Prompt recall/images: `storage/prompt_history.rs`, `attachments/prompt.rs`, `storage/user_images.rs`, `runtime/turn.rs`. Tests: unit `storage::prompt_history`, `runtime::user_images`; core unit `prompt_history`.

## TUI — `crates/oc-tui/src/`

- State/input/tabs/live/history: `app.rs`, `app/{input,tabs,live,transcript}.rs`, `history.rs`. Tests: unit `app::`, `history::`.
- Cold visible projection/cache ownership/cancel+join: `app/projection.rs`; immutable source/preview handles: `history/source.rs`. Tests: unit `app::projection::tests`, `semantic_footer_slice`, `indexed_normal_reasoning`, `shared_detail_lru`. Parked poll/close integration: `crates/oc/src/tui_cmd/{read_jobs,deck_jobs,navigation_jobs}.rs`, binary unit `parked_projection_close`.
- Markdown/wrapping/cards/DCP: `messages.rs`, `styled.rs`, `tools.rs`, `dcp_view.rs`, `views.rs`; execute: `tools/execute.rs`, `app/execute.rs`, `execute_view.rs`. Tests: unit `messages::`, `tools::`, `app::tests::execute`.
- Syntax/diff: `syntax.rs`, `syntax/styles.rs`, `patch_view.rs`; pinned grammars: `crates/oc-tui/assets/syntax/{manifest.json,PROVENANCE.md}`, `crates/oc-tui/build.rs`. Tests: unit `syntax::`, `patch_view`.
- Frame/prompt/caret/selection: `shell.rs`, `layout.rs`, `editor.rs`, `app/input.rs`, `app/text_selection.rs`. Tests: unit `shell::`, `app::tests::prompt`, `app::tests::text_selection`.
- Composer/child/Shell/PTY/question: `composer.rs`, `child_view.rs`, `shell_jobs_view.rs`, `terminal_view.rs`, `question_view.rs`, `app/foreground.rs`. Tests: unit `composer::`, `shell_jobs_view::`, `terminal_view::`, `question_view::`, `app::tests::foreground`.
- User Shell mode: `app/user_shell.rs`, `app/live.rs`, `history.rs`, `tools.rs`. Tests: unit `app::tests::user_shell`, `history::user_shell_tests`.
- MCP/services: `app/mcp.rs`, `app/services.rs`, `dialog.rs`, `app/input.rs`. Tests: unit `app::tests::mcp`, `app::tests::services`, `shell::`.
- Model/account/connect forms: `picker.rs`, `app/model_selection.rs`, `app/accounts.rs`, `dialog.rs`. Tests: unit `picker::`, `model_selection`, `accounts_tests`.
- Prompt recall/clipboard/retry: `app/input.rs`, `app/live.rs`, `app/transcript.rs`. Tests: unit `prompt_history`, `clipboard_images`, `app::tests::retry`.
- Terminal frame/cursor/restore: `terminal.rs::FrameBackend`; frames that move rows publish as DECSTBM scrolls plus the residual cell diff (`terminal/scroll.rs`). A change confined to the tab strip (sweep/fades at the display rate) is repainted alone: `shell::render_tab_strip_only` + `FrameBackend::draw_region`, which the next Ratatui frame reconciles (`tui_cmd::draw_tab_strip`). Tests: unit `terminal::` (vt100 equivalence); `oc2` targets `pty`, `pty_t39`, `pty_t42` (their screen parsers model DECSTBM/SU/SD).

## Binary и test tooling

- `crates/oc/src/tui_cmd.rs`, `tui_cmd/shared.rs` — event loop/shared deck; `tui_cmd/display_rate.rs` — paint cap from the display refresh (unit `tui_cmd::display_rate`); `tui_cmd/{child_controls,terminal_controls,auth_controls,connect_controls,clipboard}.rs` — captured effects/ACKs. Tests: binary unit `tui_cmd::`; targets `pty_t39`, `pty_t42`, `terminals`, `auth_tui`, `mcp_application`; opt-in long-session perf probe `perf_long_session`, retained-memory gate/probe `memory_soak` (see TEST_PLAN).
- `crates/oc/src/{cli,bootstrap,headless,models_cmd,auth_cmd}.rs`, `clipboard/read.rs` — CLI/cancel/catalog/auth/PNG. Tests: binary unit `models_cmd`, `auth_cmd`, `clipboard`; targets `responses`, `configured_workspace`, `auth_cli`.
- `crates/oc-adapters/tests/runtime.rs`, `tests/runtime/`, `tests/fixtures/` — fake-wire suites. `crates/oc/tests/pty_t39.rs`, `tests/pty_t39/`, `tests/support/` — actual-binary PTY fixtures; не отдельные targets на каждый файл.
- `scripts/bounded_live.py`, `scripts/test_bounded_live.py`, `crates/oc/tests/live_bounded/envelope.rs` — live guards; `scripts/tui_capture/`, `tui-recovery/scripts/` — capture/comparator tooling, не runtime dependency. Запуск только по TEST_PLAN.

При переносе owner обновлять пути/filters, не добавлять qualification report. Карта не является task registry или полным symbol index.
