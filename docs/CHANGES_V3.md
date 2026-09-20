# Изменения v3: configured workspace

V3 делает обязательным daily-driver workflow, который v2 откладывал: global и Location-local `opencode.json/jsonc`, ordered `AGENTS.md`, `.opencode`, skills, selectable primary agents, current-session commands и exact native mappings DCP/OpenProxy discovery. Goal расширен до A13; исходный V3 acceptance registry содержал 81 спецификацию.

Runner contract теперь agent-neutral: authoring model/provider/CLI не фиксируются, а compatible agent обязан только уметь работать с assigned worktree, shell/tests, Git и progress checkpoints. Добавлен `OPS05` к существующему T00 без нового task ID или DAG edge; acceptance registry содержит 82 спецификации. `apply_patch` остаётся product-tool contract, но не требованием интерфейса authoring-agent.

T02 фиксирует source-derived fixtures и provenance без заявления executable parity. T07 строит bounded immutable catalogs, source/field provenance и diagnostics, выполняет trust/capability validation и exact plugin classification. T13 добавляет native `skill`; T14/T19 связывают exact markers с compiled modules; T22 показывает redacted catalogs/actions; T24 только оркестрирует prompt/generation/session lifecycle; T25 один раз квалифицирует полный configured-workspace E2E.

Поддержка намеренно уже upstream: только primary agents; commands — literal `$ARGUMENTS`/`$1...` templates через текущую session; skill body — snapshot и ordinary tool result. Subagents, shell interpolation, recursive/background commands, remote/executable skills, watchers, UI authoring, arbitrary plugin paths/packages, JS/TS execution, Node/Bun и npm install остаются вне scope.

Known plugin identities: bare и pinned `@tarquinen/opencode-dcp@3.1.15`, а также exact `openproxy-models.js` непосредственно под `{plugin,plugins}` admitted config root. Duplicate aliases активируют один module. `@latest`, ranges, другие versions, `.ts`, basename lookalikes и external paths дают source-qualified `UnsupportedPlugin` до import/process/network.

Исторический `docs/CHANGES_V2.md` не переписывается. V3 не утверждает, что Rust product или новые acceptance tests уже реализованы.
