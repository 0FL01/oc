# Test plan — executable acceptance

Плановые сценарии хранятся в planning/acceptance.json. Они НЕ помечены реализованными этим архивом. Rust-agent создаёт тесты и reports с теми же IDs, source-derived fixtures отмечает отдельно от differential executed tests.

## Команды качества

После появления workspace:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --locked
target/debug/oc --help
```

Live tests по умолчанию исключены из cargo test и запускаются явным opt-in harness. Не делать игнорированными обязательные offline tests. Missing tool/Cargo manifest = fail/not-ready, не skip-success. Точный executable script live/soak/PTY harness создаётся в соответствующем task и documented один раз.

## Эталоны и недетерминизм

Clock/IDs/RNG/remote streams заменяются fake fixtures; normalized timestamps/IDs с сохранением identity, порядок независимых операций не навязывается. Данные с секретами в fixtures запрещены. Производный fixture хранит source path/commit/license и normalization recipe.

Mock-provider tests проверяют протокол/state machine, а не интеллект модели. Live coding acceptance проверяет поведение/файлы/tests, не точную формулировку ответа. Tool success нельзя выводить из финального текста модели без tool events и результатов проверки.

## Live model selection

При наличии OC_TEST_MODEL использовать exact provider/model-id, включая slashes после первого provider prefix; требовать его presence в effective catalog. Иначе explicit configured default. Если оба отсутствуют, исключительно test runner выбирает первый lexicographically sorted discovery ID с заявленными text input/output, tool_call:true и валидными limits; selection записывается до request и не сохраняется как product default. Нет кандидата/неизвестна capability → explicit external blocker, без угадывания по vendor/model name. Variant — explicit OC_TEST_VARIANT либо provider default без выдуманного effort; disabled variant отказ.

Все live checks одного campaign соблюдают общий counter/envelope runbook. Смена model после failure не автоматическая. Наличие модели в каталоге доказывает metadata publication, не реальную wire/tool/reasoning поддержку: квалификация только по пройденным сценариям.

## E2E fixture

Маленький Rust crate с намеренным off-by-one/edge-case bug, тестами и protected public API. Task: прочитать код, исправить через apply_patch, выполнить cargo test через bash, показать результат. После закрытого tool span вызвать compress, затем спросить retained fact/ограничение и выполнить второе локальное изменение после session restart. Assert diff только expected paths, fixture tests PASS, историю можно открыть, raw history checksum сохранён.

Дополнительно webfetch разрешённого публичного test page/fixture и codex_web search. Browser default disabled проверяется обязательно; actual Chrome smoke только при explicit opt-in и доступном browser. Отсутствие browser не является evidence реализации реального browser control.

## Soak и память

10000 scripted turn cycles с фиксированным bounded active context; не 10000 live paid prompts. ≥100 session creations/switches, periodic DCP, большие outputs/slow consumer, MCP errors, interruptions. История на диске растёт; resident active references/queues/caches должны иметь plateau. Набор включает cold/warm baseline, peak, post-idle, время и machine/cgroup metadata.

На shared user host RSS fluctuation не равна leak. Сравнить минимум два повторения, task/process/queue/blob/cache counters, PSS и DB/WAL. Численные regression thresholds выбрать по результатам baseline и зафиксировать до optimisation candidate. Нельзя обещать cap только потому, что машина имеет 11.68 GiB. Heavy compile и soak не запускать параллельно по default profile.

## Каталог сценариев


### ENV

**ENV01 — Actual host.** non-root uid; verified worktree/origin; supplied Rust target/toolchain; rootless Docker checked separately, no secrets in report.

**ENV02 — Recovery utility.** start/checkpoint/finish/block/resume; bounded indices; orphan detection; dirty Git reconciliation; no reset on resume.

### BUILD

**BUILD01 — Workspace.** edition 2024/resolver3; dependency directions; mock slice compiles; Cargo.lock and exact toolchain checked in.

**BUILD02 — Quality.** cargo fmt --all -- --check; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace --locked; no ignored required tests.

**BUILD03 — Standalone.** cargo build --locked and cargo build; target/debug/oc --help; core app runs with PATH without node/bun/upstream, external MCP disabled.

### CFG

**CFG01 — User forms.** JSONC fragments assembled into valid config; native TOML separate; ludka static/ludka2 discovery; timeout:false and chunkTimeout preserved.

**CFG02 — Substitution/merge.** baseline source-derived env/file escaping, precedence and permission merge; source/field diagnostics; original configs byte-unchanged.

**CFG03 — Capability failures.** unknown npm, explicit codemode:true, OAuth:true or unsupported DCP mode give actionable errors; not silent acceptance.

**CFG04 — Secrets/disabled.** missing env on unselected provider does not block selected one; disabled MCP launches nothing; config explain/logs redacted.

**CFG05 — Config roots.** Source-derived fixture locks admitted global/Location `opencode.json/jsonc` and `.opencode` order, canonical dedup and field provenance; existing explicit config and separate `cli.json` behavior stay intact; input bytes unchanged.

**CFG06 — Instructions.** Distinct global/applicable Location `AGENTS.md` sentinels enter provider instructions exactly once in pinned order with provenance; unreadable/disappeared files are diagnosed and a candidate never retains stale text.

**CFG07 — Definitions.** Singular/plural skill/agent/command roots, supported frontmatter and domain-specific collision rules are deterministic. One invalid definition reports path/field/reason while valid siblings survive; invalid selected/default reference, reserved command collision and subagent behavior fail explicitly.

**CFG08 — Native plugins.** Exact bare/pinned DCP identities and admitted `{plugin,plugins}/openproxy-models.js` classify idempotently. `@latest`, ranges, other versions/packages, `.ts`, lookalikes and outside-root paths fail before resolver/import/process/network; recognized JS contents are never evaluated. DISC/DCP suites own module semantics.

### STORE

**STORE01 — Vertical session.** input→mock stream→persist→resume, identical application behavior local/headless.

**STORE02 — Lock/atomicity.** second data root owner refused; state and event transactional; durable ack only after commit.

**STORE03 — Crash operations.** kill after intent before/after side effect; recover unknown/partial, never autoreplay mutation.

**STORE04 — Blob failures.** disk-full/read-only/blob-before-DB orphan behavior; no referenced blob deletion; quotas enforced.

**STORE05 — Cancel/shutdown.** worker responsive during streaming; DB drained, terminal restored, own children reaped; no tasks retained per past session.

### TOOL

**TOOL01 — Read/search.** bounded file chunks, glob/grep stable sorted results, pagination, binary/large file diagnostics, no own data dir recursion.

**TOOL02 — Patch create.** new/empty file, append to existing empty file, Unicode/CRLF/end newline; exact patch grammar fixtures.

**TOOL03 — Patch edit/delete/move.** targeted hunks/full text replacement/delete/move supported; no write/edit in model registry.

**TOOL04 — Patch conflicts.** Add existing, stale preimage, invalid later entry, protected/outside/symlink path; no success on partial; per-file result.

**TOOL05 — Shell output.** interleaved stdout/stderr >caps; no deadlock/unbounded accumulation; correct exit status/truncation.

**TOOL06 — Shell cancel.** timeout/TERM-resistant child/grandchild; KILL/wait/reap; elapsed bound; unknown versus cancelled outcomes.

**TOOL07 — Web text.** GET text/HTML/JSON, status/content type/original URL, bounded readable output and redirects.

**TOOL08 — Web safety.** private/link-local/loopback, DNS rebinding, IPv6 mapped address, redirect-to-private, large body, deadline; auth not inherited.

**TOOL09 — Permissions.** same path for all tools incl MCP/compress; ask headless fails; invalid config never allows; patch legacy deny wins.

**TOOL10 — Tool graph.** unknown tool/invalid JSON/duplicate call IDs/replayed done event do not execute; results match original call IDs.

**TOOL11 — Skill loading.** Provider metadata has bounded id/name/description without bodies; exact invocation uses central permission and pinned bounded generation snapshot. Unknown/removed/oversized/unreadable/stale calls fail visibly; call/result graph and provenance remain valid.

### DISC

**DISC01 — Basic dynamic.** unknown/new model IDs including slash, source suffix once, name formatting; no code changes/model list fallback.

**DISC02 — Metadata.** JS safe integer/null-object/booleans/modalities validation; one invalid row leaves whole old catalog unchanged.

**DISC03 — Authority.** ignore remote URL/npm/auth/options fields; credentials remain original; redirect:error.

**DISC04 — Merge.** original local snapshot only; shallow nested limit/variants; explicit local override wins; local-only IDs removed on success.

**DISC05 — Limits.** discovery context/input capped 500000; output unchanged; missing context/output removes limit; static ludka 628000 unaffected.

**DISC06 — Variants.** remote allowlist disables missing standard names; custom names/effort retained; disabled hidden; local explicit override works.

**DISC07 — Timing.** fake clock: each attempt ≤15000ms, total ≤30000ms incl body, ≤4 attempts and clipped 250/750/1500 delays; cancellation cleanup.

**DISC08 — Retry classes.** 408/425/429/5xx/network/invalid JSON/empty retry; auth/bad shape/invalid row terminal; no retry layer multiplication.

**DISC09 — Publication.** empty cold list never clears configured models; success removes retired IDs; duplicate ID last wins; bounded size failures atomic.

**DISC10 — Isolation.** disabled provider no fetch; two provider/config generations independent; failed refresh never turns remote data into local overrides.

### PROV

**PROV01 — Request.** exact base prefix+/responses, Bearer, store:false, input/tool schemas and stable prompt_cache_key; no OAuth or API fallback.

**PROV02 — SSE.** arbitrary byte split, multiline/CRLF/comments/UTF8, text/item/argument stream incremental; event cap enforced.

**PROV03 — Tool roundtrip.** complete arguments→native tool→function_call_output→next response; multi-call order and ID validity.

**PROV04 — Reasoning.** opaque/reasoning items persisted/replayed at correct boundary; model/provider switch does not reuse alien state; no opaque UI logging.

**PROV05 — Images.** advertised text/image accepts bounded verified attachment; encoded request cap, unsupported audio/video/pdf visible.

**PROV06 — Errors.** 401/403/429/5xx, incomplete/EOF, failure after delta; one retry owner, never repeat committed generation/tool.

**PROV07 — Timeouts.** timeout:false no total deadline; chunkTimeout 6000000 not 6000; explicit cancel still closes request; connect separately bounded.

**PROV08 — Live wire.** real proxy published model text+tool roundtrip; record actual model/variant/deployment evidence; not infer all-model parity.

### DCP

**DCP01 — Projection.** raw history checksum stable; durable compression/prune state changes only outbound context; no second history copy.

**DCP02 — Range schema.** topic/content/startId/endId/summary; upstream reference parsing, invalid/stale/cross-session IDs and unfinished groups rejected.

**DCP03 — Nested/protected.** upstream-derived overlapping/nested summaries, cycle/size checks; protected tool/file/tag/user bytes preserved.

**DCP04 — Patch protection.** all affected paths parsed from patch; apply_patch treated as protected mutation equivalent; large protected content visible failure not loss.

**DCP05 — Nudges.** threshold/percent/per-model, summary buffer, frequency/iteration/turn reset; no accumulating prompt-message copies.

**DCP06 — Prune timing.** dedup and error input purge recalc at compression; latest required output and error text retained; protected/manual policy tested.

**DCP07 — Config/control.** DCP precedence/read-only files, range permission and manual mode, notifications/stats; unsupported experimental options explicit.

**DCP08 — Recovery.** cancel/stale generation/transaction failure cannot half-apply projection; restart reproduces same effective context and continuation.

**DCP09 — Effectiveness.** synthetic large closed span yields smaller outbound serialized context, required facts retained; no-gain compression bounded, not loop.

### MCP

**MCP01 — Remote JSON.** 2025-11-25 initialize/headers, bearer, POST/JSON, no session ID/GET dependency; observed OpenProxy shape supported.

**MCP02 — Remote lifecycle.** JSON/SSE adapter paths, 405 optional GET, auth/timeout/invalid replies, cancellation and list update; no reconnect storm.

**MCP03 — Registry.** namespaced collision-safe names map exact server/tool, paginated bounded schema/catalog, isError surfaced.

**MCP04 — Stdio.** fake child JSON-RPC stdout + bounded stderr, kill/reap, restart config generation, no cross-auth/cwd sharing.

**MCP05 — Disabled browser.** exact npx argv preserved; enabled:false zero process/browser probe and no Node requirement; true explicitly selected fake/real smoke.

**MCP06 — Live search.** real codex_web search returns non-fabricated content/result; actual errors propagate; no auto OAuth or unknown-effect retry.

### UI

**UI01 — Basic PTY.** input/paste/Unicode/resize, streamed response and Ctrl-C, terminal restoration on error/panic/exit.

**UI02 — Model picker.** dynamic catalog/variants/refresh status, user choice persisted, retired model actionable, no automatic silent fallback.

**UI03 — History/diff.** pagination/session switch/resume/tool cards/diff without whole-history rendering/backing-store growth.

**UI04 — DCP panel.** context/stats/manual controls, /dcp-compress focus becomes bounded instruction, notification not duplicate history.

**UI05 — Headless JSON.** NDJSON only stdout; errors stderr; bounded slow consumer; interrupted exit code non-success.

**UI06 — Workspace definitions.** One shared current-generation registry drives primary-agent selection, slash completion/invocation, skill catalog/tool card, redacted provenance/diagnostics and Location switch action. Stale actions fail; removed project entries are not unioned into the target Location.

### E2E

**E2E01 — Offline coding.** scripted provider fixes seeded Rust bug through read/apply_patch/bash; fixture tests pass; no edits outside fixture.

**E2E02 — Live coding.** published model solves same small fixture with actual tool use; API/facts/tests verified, bounded requests; semantic output not exact text match.

**E2E03 — Compress then resume.** model calls compress then uses retained fact to patch/test; restart and next prompt succeed; output context reduced.

**E2E04 — Web workflow.** webfetch tool and codex_web search preserve source info; browser smoke conditional on explicit opt-in, never claimed if not run.

**E2E05 — Configured workspace.** Offline scripted provider uses global plus Location A instructions/agent/command/skill; skill body is absent until its tool call; exact DCP and OpenProxy aliases activate compiled modules. Switching to B retains global and removes every A-local input, with no JS/Node/Bun/network.

### LOAD

**LOAD01 — Soak.** 10000 scripted turns over bounded active context, ≥100 sessions/switches, periodic compress/cancel/MCP error, stable resource counters.

**LOAD02 — Output pressure.** large shell/provider/tool outputs, slow UI, queue cap, blob quota, cancellation under load; no hidden second transcript.

**LOAD03 — Measurements.** baseline/peak/post-idle RSS/PSS plus task/process/queue/cache/DB/WAL metrics; allocator RSS alone not leak conclusion.

**LOAD04 — Regression.** fixed workload/seeds/toolchain/machine; adopt baseline-derived caps, compare no linear retained-history slope; clean shutdown checked.

### OPS

**OPS01 — Provenance.** DCP/OpenCode borrowed code/prompts/tests have pinned source/license/notices; no unsupported license claims for OpenProxy copied code.

**OPS02 — Secrets Git.** review staged tracked content/fixtures/reports; no keys/URL credentials/raw payload/DB/.local; push normal own branch only.

**OPS03 — Goal handoff.** A01–A13 independent status, command exits/commit/live blockers; READY only required live passes; delivery status separate.

**OPS04 — Fresh restart.** new test HOME/XDG/data, built oc starts without upstream/Node, examples read-only, new sessions persist; no migration/release code.
