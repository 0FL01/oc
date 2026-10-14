# T58 — bundled native configuration manual

Date: 2026-10-14. Status: COMPLETE; frozen **R1–R3 / SKILL01 verified**.
Reviewed implementation commit: `650fa99ac` (base `e5a7256ee`). This report is not
overall product READY or T44 visual PASS.

## Frozen outcome evidence

| Outcome | Implementation and direct proof |
| --- | --- |
| R1 — ordinary built-in skill delivery | `defs::register_builtins` embeds the asset with `include_str!`, parses ordinary skill metadata and registers one `SkillDef`/order entry with `origin: builtin`. Existing composition, snapshots, tool admission and UI consume it without a new loader or execution path. Three SKILL01 owner cases cover metadata-only projection, complete captured body, builtin→global→project overrides/provenance, malformed replacement retention, hidden preview/explicit loading, replacement at count256 and rejection of new IDs/oversized replacement. |
| R2 — standalone native manual | `crates/oc-adapters/assets/skills/customize-opencode/SKILL.md` is **15,695 bytes / 293 lines**, below default51,200 bytes / 2000 lines. It describes native inputs, authority, source/domain precedence, provider/model/MCP/permission/profile/skill/command examples, instructions, DCP and CLI preferences, plus read→minimal edit→idle reload/owned restart→safe diagnostics. Planned/upstream/parser-only behavior is distinguished. Real-binary default output contains the entire matching compiled asset including its last line; stricter128-byte/4-line user limits still truncate normally. |
| R3 — concise navigation and touched corrections | `AGENTS.md` replaces the pending-plan pointer with one shipped-asset line. `docs/CONFIG.md` links to the asset and corrects the nonexistent config-explain command, Proposed `oc-rs.toml` status and supported flat skill IDs; existing native CLI source/persistence boundaries are retained. No general documentation audit. |

The existing `configured_workspace` Cargo target/Fixture hosts three new thematic
SKILL01 cases. They use the actual binary in isolated HOME/XDG/project roots, no
runtime skill file or checkout, and a loopback scripted Responses peer. The first
request advertises metadata, not body-only content. Explicit loading returns the
matching complete asset; user output limits are not bypassed. Per-ID Deny removes
metadata and rejects a forced call without the body. Fresh headless Ask has no saved
grant or auto-once: it exits nonzero with `approval_required`, no next main provider
request and zero durable skill execution intents. Ancillary title requests are
counted separately. Input config bytes/absence of newly created source files and
existing cargo/node/bun/npx PATH traps prove loading does not mutate configuration
or execute a package/loader.

## Native-consumer review

- `composition.rs::load_with_env` and `merge_config_sources`: runtime/DCP layers,
  inline-versus-Markdown definition order, trust and atomic publication. Markdown
  roots are G and Location `.opencode`; bare Location has inline definitions only.
- `composition.rs` CLI reader and `save_permission_mode`, `cli_permissions::update`:
  admitted CLI layering/enums and narrow explicit `session.permissions` persistence,
  not automatic whole-config rewrites.
- `config::package_protocol`: the documented package spellings select native wire
  implementations; they do not load npm/JS. Unknown packages are unsupported.
- `permissions::PermissionRules`, `tools` resource admission, `defs` flat/directory
  skill loader: ordered permission examples, per-ID skill rules, captured bodies,
  override provenance and ordinary limits match working consumers.
- Existing DCP/tool/profile/MCP owners remain authoritative. There is no claimed
  `oc config explain/check/validate`, config wizard, complete upstream CLI/schema,
  automatic watcher/shared service or executable skill support.

## Executed checks

All Cargo builds/tests run sequentially with normal stacks,
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`, `CARGO_BUILD_JOBS=3` and
`RUST_TEST_THREADS=2`. Logs are in gitignored `.local/evidence/t58-*.log`.

| Command | Result |
| --- | --- |
| `cargo test -p oc-adapters builtin_tests --locked` | 9 passed, 0 failed; includes all three new SKILL01 owner tests. Final-source workspace also passes them. |
| `cargo test -p oc --test configured_workspace --locked` | 18 passed, 0 failed, including all three new actual-binary SKILL01 scenarios and existing pinned-generation/AUD17 regressions; same18/0 again in final-source workspace. |
| `cargo test -p oc-adapters --locked` | 1006 passed / 0 failed / 5 existing opt-in ignored;19 result records. Same affected tests pass after final asset edits in the workspace. |
| `cargo fmt --all -- --check` | PASS. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | PASS. |
| `cargo test --workspace --locked` | **1906 passed / 0 failed / 12 existing opt-in ignored**,46 result records. Current source after final manual edits; no test/ignore/baseline disabled or relaxed. |
| `cargo build --locked`; `target/debug/oc --help` | Both exit0; native debug binary rebuilt from final source. |
| `python3 -B -m unittest discover -s scripts -p 'test_progress.py'` | 15 passed. |
| `python3 -B -m unittest discover -s scripts -p 'test_check_docs.py'` | 14 passed after final manual/docs edits. |
| `python3 scripts/check_docs.py`, `python3 scripts/progress.py check`, `git diff --check` | PASS; final journal closeout is validated separately from product behavior. |

## Scope and delivery boundaries

The production diff is only normal builtin registration and a replacement-safe
skill-count guard. Count256 and aggregate4MiB budgets, output limits, trust,
permission/parent ceilings, immutable raw history and captured request semantics
are not relaxed. No new dependency, registry, schema, public API, framework, JS host,
runtime install/config rewrite, paid/live model call or visual campaign was added.
Existing TOOL11/CFG07/UI06/E2E05/AUD17 and root/child/autoinvoke owners are reused,
not copied or reassigned. Historical evidence and unrelated untracked data remain.

Git delivery is separate: the verified origin is `0FL01/oc`, branch
`agent/oc-rust-port`. Approved dedicated-account write authentication was already
proved unavailable at T44 checkpoint147 (SSH publickey rejection and noninteractive
HTTPS missing Username). No unchanged push retry, foreign/root credentials,
credential search, force push or origin change is attempted here. Remote delivery
is BLOCKED_REMOTE. Implementation is preserved locally in `650fa99ac`; this report
and normal journal finish form the subsequent documentation closeout commit.

## Closure

Closure check: all frozen R1–R3 are verified with current source/evidence, required
relevant/workspace gates are green and the reviewed7-file implementation stays
inside the envelope. No known blocker was created by the diff. The only source
review corrections were exact inline/Markdown root order and explicit CLI permission
persistence; no new runtime mechanism was needed. T58 is complete. T44/T45/T57 and
the overall GOAL keep independent unresolved contracts; this evidence does not waive
live, paired-visual or release qualification. No further T58 substantive work remains.
