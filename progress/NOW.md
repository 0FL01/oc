# NOW — актуальный handoff

State updated: 2026-09-21T19:54:00+00:00
Active: T27

Сверить Git status/diff до выполнения команд.
Task: T27 — Live пользовательский workflow
Spec: roadmap/M5.md
Evidence target: evidence/T27/report.md

Явно opt-in выполнить готовый bounded live campaign: coding без edits вне fixture, reopen/next command, compress/resume, webfetch и codex_web search с counters/watchdog; optional browser smoke only explicit opt-in. Product/harness code здесь не добавлять; report remaining models unqualified.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

Owner-reported startup failure diagnosed and the diagnostic improved. `target/release/oc`
(bare) failed with `error: unsupported dcp option: allowSubAgents is out of goal scope`
before the TUI existed. Root cause: the owner's `/home/opencode/.config/opencode/dcp.jsonc`
sets `experimental.allowSubAgents: true`; the product rejects unsupported DCP options by
contract (T07/F17: typed `UnsupportedCapability`, field + reason, before any side effect),
because subagent-driven summarisation is out of goal scope. This is config rejection, not a
TUI regression: `oc tui`/`oc run` fail identically; bare `oc` only now reaches config
loading (T42).

Diagnosability fix (`crates/oc-adapters/src/composition.rs`): the final DCP validation error
now names every admitted source that contributed a fragment. Verified on the owner's real
config:
`error: unsupported dcp option: allowSubAgents is out of goal scope (dcp config sources:
/home/opencode/.config/opencode/dcp.jsonc)`. New test
`composition::tests::unsupported_dcp_option_names_its_source`.

Verified next blocker for the owner: with `allowSubAgents: false` the next error is
`model required: set top-level model to provider/model-id in opencode.json/jsonc` — the
global config declares providers/MCP but no top-level `model`, and the repo Location has no
`opencode.json`. So the owner needs one `model` line (e.g.
`ludka2/ocg/muse-spark-1.3-contributor`, a verified deployment model) in the global config or
a project config.

## Checks

`cargo test -p oc-adapters --lib` -> 137 passed / 0 failed. `cargo clippy --locked
--workspace --all-targets -- -D warnings` -> exit 0. `cargo fmt --all` applied.
`cargo build --locked --release` -> binary rebuilt. Reproduction with the rebuilt release
binary against the real HOME prints the source path (exit 0, no live call made).

## Risks

- No behaviour change to the DCP policy itself: unsupported options still fail closed; only
  the message gained the file list.
- The owner still needs a top-level model before the TUI can start.

## Next

1. Owner: set `"experimental": {"allowSubAgents": false}` (or drop the key) in
   `~/.config/opencode/dcp.jsonc`, then add a top-level `model` (or a project `opencode.json`
   in the repo).
2. Resume T27 (live campaign): the harness still requires a model that the configured
   provider resolves via native discovery — see `evidence/T27/checkpoint.md`.


Ready (до 5): T30
Blocked: нет

Done в журнале не означает READY всего продукта; см. GOAL.md.
