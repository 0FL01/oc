# Карта владельцев кода

Навигация по текущему дереву; читать нужную строку, не все перечисленные файлы. Root-правила: `AGENTS.md`; инварианты: `ARCHITECTURE.md`; первый перенос: T52 в `goals/2026-09-28-code-slices.md`.

На момент принятия плана исходный Git commit — `ddb6ae2f0cba18c4aa161159899228a7343398eb`. Пути в колонке T52 ещё **не реализованы** этим patch. Исполнитель обновляет соответствующую строку в одном commit с переносом; не поддерживать вечные параллельные карты «old/new». Текущие LOC не дублируются здесь: baseline в T52, новый отчёт строится по Git/файлам.

| Вопрос / владелец | Читать сейчас | Назначение после T52 и ближайшие тесты |
| --- | --- | --- |
| UI state, input, selection, turn projections | `crates/oc-tui/src/app.rs` — state/reset/API; `app/input.rs` — composer/panels/pointer dispatch; `app/transcript.rs` — history/viewport/selection/copy; `app/tabs.rs` — tab presentation/clocks; `app/live.rs` — receipts/worker/tool/DCP events | `app/tests.rs` shared fixtures + `app/tests/{input,transcript,tabs,lifecycle}.rs`; tab suite подключена в `app/tabs.rs`, чтобы private animation state не раскрывать ради tests. `cargo test -p oc-tui --lib app::`; `oc --test tui_render_alloc`. |
| Markdown / transcript rendering и wrapping | `crates/oc-tui/src/messages.rs`; `styled.rs`, `history.rs`, `tools.rs`, `dcp_view.rs` только по зависимости | `messages/tests.rs`; `cargo test -p oc-tui --lib messages::`. Shared wrapping в `styled.rs`, не создавать второй renderer. |
| Геометрия кадра, prompt/sidebar/tabs/toast | `crates/oc-tui/src/shell.rs`, затем `layout.rs` | `shell/tests.rs`; `cargo test -p oc-tui --lib shell::`. Не путать с shell executor в adapters. |
| TUI lifecycle, bounded event loop, PanelIntent и tab deck | `crates/oc/src/tui_cmd.rs`; `clipboard.rs`/`bootstrap.rs` при необходимости | `tui_cmd/tests.rs` + `tui_cmd/tests/{routing,lifecycle}.rs`. `src/approval_tests.rs` сохраняет прежнее подключение; `cargo test -p oc --bin oc tui_cmd::`. |
| Application worker, generations, queries, title/session ownership | `crates/oc-adapters/src/application.rs`, существующие `application_selection.rs`, `application_tab_deck.rs` | Production владельцы сохраняются; inline test packs → `application/tests.rs`; существующие conversation/fork test-файлы не плодить повторно. `cargo test -p oc-adapters --lib application::`. |
| Provider turn + admission + tool execution | `crates/oc-adapters/src/runtime.rs`; `runtime/tool_stream.rs` по вопросу stream | `runtime.rs` сохраняет Runtime/state/API, `runtime/turn.rs` — turn/tool/child flow; `runtime/tests.rs`; `cargo test -p oc-adapters --test runtime`. |
| Runtime DCP projection / continuation / token accounting | `crates/oc-adapters/src/runtime.rs`, `runtime_compaction.rs`; `dcp.rs`, `dcp_auto.rs` по policy | `runtime/context.rs`, existing compaction/tool_stream files; не делать новый DCP engine. `runtime`, `dcp_atomic`, `context_bounds` adapter targets; `oc` target `dcp_runtime`. |
| MCP generation, attach, degradation, quarantine/close | `crates/oc-adapters/src/runtime.rs`; `mcp_remote.rs`, `mcp_stdio.rs`, `mcp_http_lifecycle.rs` | `runtime/mcp.rs` — runtime orchestration, transport owners не менять. Adapter targets `mcp_remote`, `mcp_stdio`, `runtime`; binary `mcp_application`. |
| Db, history, DCP, durable tool outcomes / recovery | `crates/oc-adapters/src/storage.rs` и только нужный `storage_*.rs` | `storage.rs` после выноса inline suite → `storage/tests.rs`; existing fork/conversation/DCP modules остаются. `cargo test -p oc-adapters --lib storage::`, targets `storage_lock`, `dcp_atomic`, `tab_deck`. |
| Общие типы, CoreApp API и ports | `crates/oc-core/src/{core_app,application,queries,ports,domain}.rs` | Не превращать этот набор в новую иерархию в T52; core без UI/storage/network зависимости. Читать конкретный DTO/метод, не весь crate. |
| Config/composition/provider/tools | соответствующие `crates/oc-adapters/src/{config,composition,defs,discovery,provider,tools,patch,shell,webfetch}.rs` | Нет массового переноса в T52. При новом feature сначала найти существующего владельца; существенные новые tests отдельно по общему правилу. |
| Runtime fake-wire integration suite | `crates/oc-adapters/tests/runtime.rs`, при approval — `tests/fixtures/approval_lifecycle.rs` | Target `runtime` сохранить; сценарии в `tests/runtime/{turns,context,tool_lifecycle}.rs`, harness/shared fixture в прежнем root-файле. |
| Actual-binary PTY suite T39 | `crates/oc/tests/pty_t39.rs`, `tests/support/{terminal,title}.rs` | Target `pty_t39` сохранить; сценарии в `tests/pty_t39/{interaction,lifecycle}.rs`, общий fixture в прежнем root-файле. |
| Capture/oracle/frontend tooling | конкретный script в `scripts/tui_capture/`, указанный активным VIS-контрактом | Это dev tooling, не production TS host. Не читать весь каталог/`capture.mjs` без конкретной задачи; пути source inventory поправить только в живом producer/consumer. |

## Как обновлять

Изменившийся owner указывает точку входа, свои части и один ближайший test target/filter. Карта — маршрутизатор, не полный symbol index и не копия архитектурного spec. No generic helpers/common-manager dumping ground. Для малого изменения внутри неизменного владельца новая строка не нужна.

Обоснованное временное превышение 5k указывать в строке владельца: `path — причина связанности/риска — следующий естественный шов или условие пересмотра`. Это warning, не бессрочное разрешение растить монолит. На старте T52 восемь превышений ещё подлежат работе, а не считаются принятыми исключениями.
