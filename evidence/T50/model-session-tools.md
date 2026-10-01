# T50 R7 / TOOL18 — frozen obligations (2026-10-01)

## Coordinator independent qualification

R7's frozen obligations are verified against `cd2a95926e3c3c5f3eeda451cf8a1a8b814914fd`
plus the reviewed implementation below. Concurrent owner changes are planning-only
and remain separate, unstaged work; they do not establish T56 or the new TOOL13
inventory/conversion extension as implemented. No T50 completion claim.

- Reviewed all runtime/permission/catalog/storage seams, the three new Rust parts,
  native fixture consumers and the pinned donor models/rename implementation.
  Independent target-authority review found no introduced violation. Native output
  uses declared `nextOffset` and nullable family metadata, not fabricated unknowns.
- Independently reran `tool18_` owner tests: three library and three runtime cases
  passed. Workspace fmt, strict locked all-target Clippy and normal locked build
  exited 0. The current full workspace log `t50-r7-workspace-qualified.log` contains
  42 successful summaries: **1392 passed, zero failed, ten unchanged opt-in ignores**.
- After the last normal build, independently exercised both ELFs: nineteen models/
  rename cases and one actual-title PTY case each. Each nineteen-case run observed
  **106 provider requests, 38 auxiliary titles, two startup discoveries, 38 durable
  tool rows and four session updates**; the separate PTY observed two main requests
  and one real title event. Lookup itself caused zero discovery requests.
- Python47, docs, read-only progress and diff checks exited 0. Detailed independent
  receipt: managed output `tool_0f67940fd0014xJDkQBD4yeTvF`.
- Normal SHA256 before/after all independent direct proofs was unchanged:
  debug `e94b79ab1105c3e7b2ae545f94c99f15a93c83c5bde600ddc7c7e996b2de4878`;
  release `25bb0554f3860b37c1232e7efe30daacf45c18fdf364423f6d14cc5a7472a306`.
  These are compiled dirty-implementation artifacts, not clean-base artifacts.
  All owned native/PTY groups and HTTP workers joined; temporary roots removed.

Next: durable safe-boundary R8 move, then the newly approved R2 inventory/live-output/
same-process conversion extension. T44 remains PAUSED; no live requests or exhausted
campaign mutation occurred. Progress/planning files modified by the owner are not
overwritten or staged merely to record this independently complete source slice.

Base: `cd2a95926e3c3c5f3eeda451cf8a1a8b814914fd`. Sole temporary mutation/Cargo/native-fixture owner; inherited `.opencode/` is excluded from all inspection and edits. T50 remains active, R8 pending, T44 PAUSED.

Authority: native-tool-parity R7; TEST_PLAN TOOL18; donor `opencode/packages/core/src/tool/plugin/opencode.ts` at `2670273ff17da96f85c5826ced57aa1b368754fa` lines 11–16, 27–66, 89–107, 134–196. This freeze precedes RED.

## Required observable obligations

1. Actual root and child provider requests advertise strict direct models/rename schemas subject to effective policy; move is absent. Preserve canonical shell/hidden bash and model-independent catalog.
2. Models accepts only query/provider/all/limit/offset (limit 1..100, default20; offset nonnegative default0). Query matches all lowercase whitespace-separated terms against provider/model ID and model name; provider matches ID or name. Own provider first, provider ID then release descending; one newest known family per provider after filtering unless all. Unknown family/release/cost/status remain null/absent, never fabricated.
3. Bounded grouped metadata, total and nextOffset use already admitted generation providers, replacing only the selected provider's static entries with its actual merged discovery/local catalog. Variants use models::ordered_variants after merge. No raw provider configuration, headers, credentials, extra metadata or discovery/network side effects. Exact call/result IDs; calling model/provider/variant and wire remain unchanged.
4. Rename validates through normalized_session_title; trimmed title and session_updated commit atomically through the existing storage owner. Publish existing SessionTitleUpdated only after durable success, so actual frontend projections receive it. Reopen/restart sees the real title.
5. Omitted target resolves current before policy admission; permission resource is the exact target ID. Root may rename locally known roots in its pinned Location; child may rename only itself, never parent/sibling/foreign roots. Structural validation/missing/foreign/stale target fails before intent or effects. Effective Deny/Ask and resource-bound saved grants remain common admission, with revalidation after waits. General/Explore structural read-only ceilings remain.
6. Owner failure rolls back title+event; operation remains truthful and no unknown-effect replay. No second SQL mutation owner, store/catalog, generic bus, schema/dependency/crate or test-only public API.

## Minimal private seam frozen before edits

The existing application RenameSession command is idle/root-only and cannot be awaited from its own running turn. Share a private application-owned durable rename/publisher function with runtime instead of an Inbox roundtrip. Generalize the existing storage rename transaction to support an explicitly self-authorized child while retaining root-only checks for existing application commands. Native runtime validates the exact locally known target and pinned Location before admission and again after waiting. Busy native self-rename is admitted because it mutates metadata only; explicit other roots must be idle. These are declared native donor differences (title 256-byte/control-text ceiling, Location/lineage/idle-target ceilings).

## Falsifiers

Inspection prerequisite: selected-only assembly currently discards unselected provider metadata. The minimal generation-owner change retains only shape-admitted name/models/npm with default inert connection options for unselected entries, without substitutions or routing admission; enabled/disabled filters still apply. Existing selected-provider binding and discovery are unchanged. This is not a second catalog and is not T53 protocol support.

- Tool absent or loose schema, move advertised, duplicate/mismatched call/result, selection changed, second network/discovery request caused by lookup.
- Incorrect query/filter/family/order/page, lexical variant ranking, secret/unbounded metadata or synthetic unknowns.
- Success without durable title+event+publication; Inbox self-deadlock; injected event failure leaves title changed.
- Deny/Ask/unknown/foreign/child-parent/sibling rejection creates intent/effects or grants broader authority; stale target survives wait.
- Existing workspace regressions, mutated normal ELF between final builds and direct scenarios, incomplete joined cleanup or missing mandatory current checks.

## Qualification

Directed-regression fixture seam: native_search retains its historical R3 default catalog assertions. An explicit current R7 catalog mode will require models and root rename, forbid child rename and move, and preserve every search/result/barrier/resource assertion. This is needed because the original fixture correctly forbids the then-unimplemented tools; it does not change a historical baseline.

R7/TOOL18 qualified on the reviewed dirty implementation associated with base `cd2a95926e3c3c5f3eeda451cf8a1a8b814914fd`; HEAD remains that base. No commit/delivery claim. R8 remains pending, T50 active, T44 PAUSED. Later owner planning/contract/T56 changes are present in the shared worktree and were preserved; they are not this coordinator's mutations.

### Obligation → owner → evidence

| Obligation | Implementation / tests | Actual result |
|---|---|---|
| Strict direct effective root/child catalog; no move | `runtime.rs::builtin_tool_defs`, `tools.rs::MODEL_TOOL_NAMES`, `composition.rs` module policy, `defs.rs` General/Explore constraints; `models/tool_tests.rs` | Root/helper/General/Explore actual provider schemas checked; Deny removes the relevant tool; General/Explore rename absent, models available; move/bash absent. |
| Filter/order/family/page/unknowns/static + selected discovery merge/shared variants | `models/lookup.rs`, existing `models::ordered_variants`, inert read-only metadata retained by `config.rs` in existing Generation | Static + dynamic + actual child lookup pass; provider ID/name, all-term query, family-after-filter, own-first release order, pagination/total/nextOffset, unknown null fields and local/discovery overrides checked. No second discovery/catalog/selection owner. |
| Read-only lookup, policy/redaction/bounds and exact call graph | `runtime/turn.rs` common admission/intent/outcome, `runtime/mcp.rs::mcp_redactions`; models test and native fixture | Calling model `m`, provider binding and high variant remain identical on all main Responses requests; exact call IDs match durable operation IDs/results, no duplicate result ID; models Deny terminal refusal, models Ask zero operation rows. Dynamic GET count is two startup discoveries across two native processes, zero lookup-added GETs. |
| Atomic real title/event + durable frontend publication | Private `application.rs::commit_session_rename` → existing `storage.rs::rename_session`; `runtime/turn.rs` direct seam; existing `oc-tui/src/app/live.rs::SessionTitleUpdated` projection | Current/explicit/child-self rename succeeds; title trimmed through existing normalized title owner. Restart preserves title and one event. Runtime broadcast test observes exactly two successful title updates. Actual normal-ELF PTY tab paints native title, with matching real DB title + one session_updated and exact wire result. No Inbox self-wait. |
| Exact resource, root Location/idle scope, child self only; Deny/Ask/stale checks before effects | `tools.rs::permission_resources`, `approval.rs::save_patterns`, `runtime/turn.rs::rename_target`; `tests/runtime/model_session_tools.rs` | Unknown/foreign/child-parent/sibling/foreign/invalid/General calls return non-success, titles/events unchanged. Ask has zero operation rows. Ask target binding altered while waiting rejects before intent; saved patterns literal target only. Existing session rename root/UI suite passes. |
| Failure/effect truthfulness and existing regressions | Existing storage transaction generalized narrowly; separate runtime tests inject event insert failure | Event failure rolls back title+event, no publication; failed operation remains truthful. Denied/missing calls have no Started/intent. Immutable history, existing unknown-effect persistence rules and manual application rename behavior preserved. |

### Current mandatory checks

All Cargo commands serialized, offline, locked where applicable: `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 CARGO_NET_OFFLINE=true TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`. Non-root uid1003. No dependency update/clean, real network/browser/paid API or authoring/user configuration access. Recorder is existing `evidence/T50/webfetch_check.py`, max262144 bytes per log with truncation treated as failure. Log directory below is `/home/opencode/.cache/opencode-tmp/opencode/`.

| Command / gate | Exit / exact result | Log basename |
|---|---|---|
| Nearest RED strict direct catalog | Failed as expected: 0 passed, 1 failed (R7 tool absent) | `t50-r7-red.log` |
| Nearest GREEN catalog | 0; 1 passed | `t50-r7-green.log` |
| Models owner TOOL18 | 0; 3 passed | `t50-r7-models.log` |
| Runtime owner TOOL18 | 0; 3 passed | `t50-r7-runtime-final.log` |
| Existing session rename | 0; 3 passed | `t50-r7-rename.log` |
| Corrected composition regression | 0; 1 passed | `t50-r7-reg-composition2.log` |
| Corrected existing title suite | 0; 8 passed | `t50-r7-reg-title2.log` |
| `cargo test --workspace --locked --no-run` | 0; separate precompile | `t50-r7-precompile2.log` |
| `cargo test --workspace --locked --no-fail-fast` | **0; 1392 passed, 0 failed, 10 ignored; 42 summaries; 850 seconds; complete untruncated actual log** | `t50-r7-workspace-qualified.log` |
| `cargo fmt --all -- --check` | 0 | `t50-r7-fmt-qualified.log` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | `t50-r7-clippy-qualified.log` |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | 0; 47 tests: bounded_live13 + code_size5 + progress15 + check_docs14 | `t50-r7-python-qualified.log` |
| `python3 scripts/check_docs.py` | 0; documentation structure only | `t50-r7-docs-qualified.log` |
| `python3 scripts/progress.py check` | 0; journal structure only | `t50-r7-progress-check-qualified.log` |
| `python3 scripts/code_size.py --base cd2a95926e3c3c5f3eeda451cf8a1a8b814914fd --changed` | 0; advisory, largest touched file 4272 lines, new lookup158 / model tests120 / runtime tests234 | `t50-r7-size-qualified.log` |
| Normal `cargo build --locked` / `cargo build --release --locked` | 0 / 0; last Cargo operations before all final direct ELF checks | `t50-r7-debug-build-qualified.log`, `t50-r7-release-build-qualified.log` |
| Direct normal debug / release `oc --help` | 0 / 0 | `t50-r7-debug-help-qualified.log`, `t50-r7-release-help-qualified.log` |

After the factual report/fixture additions, final read-only docs/progress checks also exited0 (`t50-r7-docs-final.log`, `t50-r7-progress-final.log`); final `git diff --check` passed and both ELF hashes still match. These structure checks do not certify another task's owner-added planning changes as product PASS.

### Normal artifact / direct fixture qualification

Both are actual Linux x86-64 dynamically linked normal ELF binaries, debug with debug_info and release stripped. No copies or special feature/profile build. SHA256 stays identical before/after every final campaign and at final inspection; no Cargo intervenes after the normal builds:

- `target/debug/oc`: `e94b79ab1105c3e7b2ae545f94c99f15a93c83c5bde600ddc7c7e996b2de4878`, BuildID `1a01f79662f14a5c47ffdb11f35b9bab8aec9fd2`.
- `target/release/oc`: `25bb0554f3860b37c1232e7efe30daacf45c18fdf364423f6d14cc5a7472a306`, BuildID `bb3591c73dfa2e311407a60b0b449e0b2ba02c0c`.

`native_model_session.py` runs bounded synthetic Responses/discovery only, captures schemas/results in RAM and stores only safe bounded facts. Fresh actual native initialization supplies real DB/schema/Location authority; fixture SQL inserts locally known target identities, never implements rename effects. All model-driven rename changes use the real production transaction. Each owned application PGID and HTTP thread joins before exact TempDir removal.

Per ELF: **19/19 model/session scenarios** (16 original + focused models_deny/models_ask/child_models), **0 failures**, **106 provider requests including 38 auxiliary title requests**, **2 startup discovery GETs**, **38 durable tool rows**, **4 session_updated events**. Denied calls store terminal refusals; Ask stores no operation rows. Final logs (all exit0):

- `t50-r7-native-debug-qualified.log`, `t50-r7-native-release-qualified.log`: original16 each, identical counters 90 requests / 32 auxiliary / 2 GET / 30 tool rows / 4 title events.
- `t50-r7-models-deny-{debug,release}.log`: one each, 5 requests / 2 auxiliary / 1 refused row / 0 title events.
- `t50-r7-models-ask-{debug,release}.log`: one each, 4 requests / 2 auxiliary / 0 tool rows / 0 title events.
- `t50-r7-child-models-{debug,release}.log`: one each, 7 requests / 2 auxiliary / 7 tool rows / 0 title events; actual helper child schema/results.
- `t50-r7-tui-title-{debug,release}.log`: **one additional TOOL18 PTY projection scenario per ELF**, 2 main requests, one native title event, actual rendered tab title + DB/wire correlation; SHA unchanged. Thus **20 TOOL18 checks per ELF**.
- `t50-r7-regressions-{debug,release}-current.log`: **15/15 directed regressions per ELF** through `model_session_regressions.py`: question2 (headless Ask-auto and real custom-answer reopen), search2 (root + Explore), read2 (nested lifecycle + image), webfetch2 (markdown + Ask), instructions Ask6 (relative/absolute and file/source authority), RET01 actual Revert1 (2 real jobs, 2 terminal/notices, oldVisible0/newVisible1, effects2). All exit0, joined cleanup and unchanged SHA. These are directed checks, not a rerun of every historical matrix.

### Failed attempts / diagnosis

- First full workspace run (`t50-r7-workspace.log`) reached the outer900-second timeout and exposed three genuine regressions: retaining empty unselected providers changed the existing selected-credential contract; publishing SessionTitleUpdated for the old manual application command changed two existing title-concurrency expectations. Fixed production by retaining only nonempty inert static metadata and preserving the old manual command path; native publisher remains. No original assertion/deadline/cap was changed. Partial timeout is not a PASS.
- `t50-r7-reg-composition.log` / `t50-r7-workspace-final.log`: transient E0425 from a condition patch landing in the earlier compaction clause; corrected exact context, then targeted composition/title, precompile and complete workspace passed.
- `t50-r7-dev-native{,2,3}.log`: peer/fixture development failures (auxiliary title Requests mixed into tool script; malformed model-list peer shape; explicit helper spawn permission; baseline child birth title). Fixed the peer/fixture to match real owner/discovery contracts, not product assertions. Final actual normal ELF campaigns above pass.
- `t50-r7-regressions-debug-qualified.log`: two question scenarios passed, historical R3 search catalog rejected newly implemented R7 tools. Explicit current-catalog mode now requires models/root rename and rejects child rename/move; historical mode remains the default. Both final directed campaigns pass.
- `t50-r7-progress-qualified.log`: CLI misuse `progress.py validate` exited2 (unknown subcommand), with no mutation; corrected read-only `check` exited0.

### Reviewed association / limits / ownership

Reviewed source diff against base covers application/storage seam, approval literal resources, generation inert metadata + composition filters, existing definitions constraints, builtin schemas/names, private lookup and runtime dispatch/admission, redaction visibility, separate owner tests and existing registry expected-name update. `docs/CODE_MAP.md` records actual new owner paths. Only additional existing fixture change is native_search's explicit current-catalog mode; new evidence scripts are native_model_session and model_session_regressions. Cargo/dependencies/core/TUI/schema/licensing were not modified. Existing donor provenance/notices remain; semantic authority is the pinned donor cited above, no JS/CodeMode runtime or new provider/protocol family.

Declared native ceilings: limit100; query/provider4096 UTF-8 bytes; page64KiB; matching catalog100000; bounded identity metadata; unknown release/family/cost/status remain unknown. Unselected admitted providers expose inert configured static metadata only, not remote discovery/readiness/routing (T53 later). Rename title uses existing max256-byte/control/invisible-text policy; explicit roots require known same Location and idle family, children only self. Refusals are truthful typed operation states/results, not success. Publication uses the existing optional runtime event publisher; production application installs it, verified by runtime event and real PTY projection. No promise of cross-store exactly-once replay beyond existing transactional title/event and unknown-effect rules.

Bounded retention: at the pre-report inspection, all own `t50-r7-*.log` files total316534 bytes; two final PTY logs add approximately1.5KiB. Logs plus this report/scripts stay well below1MiB, every log below16MiB. No large Cargo tree/artifact copies, broad prune, journal/live reset, user configuration change, `.local/live.env` or inherited `.opencode/` inspection. Normal build artifacts remain in their existing target paths. Native TempDirs/HTTP/PTY owners joined and exact directories removed; no new native fixture directory remains. No staging, commit, push, progress/spec/GOAL/acceptance mutation by this coordinator.

**R7 temporary mutation/Cargo/native-fixture ownership released on return. Parent may resume as sole owner. Full T50/R8 completion is pending; T44 remains PAUSED until explicit owner resume.**
