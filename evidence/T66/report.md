# T66 — strict OpenCode 2 key allowlist (D28)

## Result

**DONE LOCALLY.** R1–R5 of [the T66 contract](../../docs/goals/2026-10-16-v2-allowlist.md)
pass. Base `d8b37d9c3`. The owner converted `~/.config/opencode` himself (commit
`bcec58b` in that repository); the agent did not edit it and did not read
`secrets.env`.

### Required behavior

- **R1 — allowlist admission:** `config/legacy.rs` is replaced by
  `config/admission.rs`. Admitted `opencode.json(c)`, `cli.json(c)` and agent
  Markdown frontmatter are checked against the OC2 v2.0.12 schema keys (plus the
  T65-oracle provider `compatibility` and declared native extensions `dcp`,
  `animations`, `terminal`, variant `disabled`), including cli.json sections and the
  230 donor keybind names. Any other key refuses startup/reload/`oc2 models` with one
  `unknown_config_key` report (`scope: path`, `<id>`, no values, no replacements)
  before provider/MCP effects. Markdown commands keep isolated per-file diagnostics
  (CFG10); MCP cross-transport fields stay isolated server failures (T46).
- **R2 — V1 code removed:** replacement tables, `LegacyConfig`/`ReplaceLegacyConfig`,
  the specialised TUI screen text, the root `permission`/`tools` guard, the
  `/provider/openai` probe and V1 redaction aliases are gone. Reserved internal tool
  identities (`write`/`patch`/`apply_patch`/`bash`/`task`) are refused as rule
  actions: TS ignores them, the internal mapping would have applied them.
- **R3 — compatibility override:** a model override equal to the native default
  (`max_tokens` under provider `max_completion_tokens`) was dropped from the per-model
  map and lost to the provider default. Fixed in `provider_wire`; the shared CFG14
  fixture case is RED before / GREEN after in Rust and passes in the pinned TS
  oracle (`scripts/check_v2_config_semantics.py`: 82 pass).
- **R4 — repo forms:** root `opencode.json` now uses `permissions` with `edit`/`shell`;
  examples, CONFIG/README/DECISIONS/GOAL/TEST_PLAN/CODE_MAP/ROADMAP/PROVIDER docs and
  the bundled `customize-opencode` runbook describe only OC2 forms.
- **R5 — real-user catalog:** current models.dev lists five `openai` image-generation
  rows with all-zero limits; the strict row check rejected the whole public document,
  so `oc2 models` on the converted config exited 1 with two `invalid_catalog` lines
  (`opencode-go`, `openai`). All-zero rows are now omitted; partial zero limits still
  reject atomically (GO02 regression extended).

### Checks

- `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D
  warnings` exit 0; `python3 scripts/check_docs.py` OK.
- Final `cargo test --workspace --locked --no-fail-fast` with runbook settings
  (`CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2`, owned `TMPDIR`): **2012 passed,
  0 failed, 12 ignored** (unchanged opt-in cases), exit 0.
- An earlier run without owned `TMPDIR` failed `live_bounded` with
  `untrusted_ancestor` under `/tmp` (environment, per runbook) and timing tests while a
  release build ran concurrently; not counted.
- `mcp08_actual_retry_and_reload_location_retire_pending_scope_without_late_effects`
  became flaky under CPU load (HEAD 0/6 vs candidate 3/6 failures). Cause: the test's
  watcher auto-reload can publish before the explicit forced `/reload` republishes
  (two `late` spawns, both before the acknowledgement); the valid public catalog cache
  (R5) widened that window. Integration fixtures reach the real models.dev, a
  pre-existing hermeticity gap. The assertion now checks the contract — no spawn after
  the Location retires the scope (1–2 before); 8/8 pass under the same load.
- Rebuilt `target/release/oc2` (via `~/.local/bin/oc2`, alias `oc2` from `~/.zshrc`):
  `oc2 models` on the real converted config exit 0, no diagnostics, providers
  a6api, a6api-claude, llama.cpp, llama-lain, ludka2, openai, opencode-go listed.
  Bounded PTY launch (140×40, no prompt): no startup error frame, Build visible,
  ctrl+c natural exit 0. Not a full E2E06 strace qualification.

## Risks

- The allowlist is stricter than TS (`onExcessProperty: "ignore"`): a key from a newer
  V2 release that `oc2` does not know refuses startup explicitly.
- Unknown key names are echoed when they are plain identifiers (≤64 chars); other
  spellings are `<key>`. Values are never printed.
- Integration tests are not hermetic against models.dev (pre-existing).
