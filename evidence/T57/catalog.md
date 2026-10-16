# T57 — shared public OpenAI catalog (partial AUTH04)

Date: 2026-10-06. Base: reviewed/pushed `5b85cb36c`. This is checked public
metadata/composition work, not default WS, a real authorized model request,
rebuilt login consumers, AUTH06 live, VIS45 or full T57 completion.

## Source and one owner

Pinned donor `opencode/packages/core/src/plugin/provider/openai.ts:24–25,271–295`
defines the subscription overlay. It applies only to active admitted built-in
OpenAI OAuth: `body.reasoning.mode == pro` disables first; API identity is
`modelID ?? id`; exact gpt-5.5 / gpt-5.3-codex-spark allow, exact gpt-5.5-pro /
gpt-5.6 deny, otherwise numeric gpt-major[.minor] prefix qualifies major >5 or
major5/minor >4. The native bounded digit comparison preserves leading-zero,
suffix and oversized-number semantics without general model/reasoning allowlists.
Qualified models receive cost `[]`, context400000/input272000; output remains.
Ordinary Key metadata is unchanged. This is the explicit scoped name exception.

`models_dev.rs` retains the existing GoCatalog/Db public-cache owner, source URL,
cache key, body/row/label bounds, five-minute TTL, 15-second deadline, single flight,
atomic complete validation, last-good failure/cancel behavior and protected
read-only access. Its optional OpenAI slice shares the same public GET and bounded
cache record; old Go-only records still parse and retain their Go view. No provider
registry, authenticated `/models` probe, token-bearing public request, new table,
migration or dependency. Remote API/auth/env/header data remains discarded.

## Actual consumers and capture

- Canonical native OpenAI is an available scoped connection without model/config.
  Foreign endpoints named openai stay generic; enable/disable filters remain.
- Selected and independent views use one cached unfiltered source plus current
  admitted subscription overlay. OAuth→Key restores all Key rows/pricing/limits
  and original route, not an already-filtered catalog. Public variants/package
  aliases preserve existing native wire admission and exact local overrides.
- Finite model request leaves carry the current account marker into the existing
  captured generation; `opencode_models` reads both slices from this same owner,
  applies its captured subscription marker and retains explicit empty-cost meaning.
- Catalog-only listing selects only safe method/kind/expiry/pending metadata with
  SQL JSON extraction through the existing no-follow/UID/shared-lock/read-only-WAL
  path. It never reads token values into Rust, refreshes, migrates or recovers turns.
- Independent connection previews are metadata-only. A near-expiry unselected
  OpenAI account is not exchanged/reserved just by opening another provider.
  Per-request refresh/preparation across every executable lane is still pending.
- Existing Go background GET behavior is retained. OpenAI uses that same response,
  cache or explicit selection/catalog request, not an extra unsolicited GET in
  Go-disabled offline compositions. Changed sibling slices are published; stable
  cache projections do not repeatedly emit ProviderChanged and reload themselves.
- Reads/connection expansion do not select a model, accept a turn or create history.

## Checked evidence

New separate fixtures: `models_dev/openai_tests.rs`,
`composition/openai_catalog/tests.rs`, and the owning catalog test module. Four
new scenarios prove one concurrent physical public GET for both slices, safe cache
persistence/reopen/legacy extension, exact transform differential boundaries,
shared composition/account/lookup/native application views and protected read-only
listing with a genuine pending turn left unrecovered. Real Db and application
owners are used; public transport is a bounded fake source, no model/issuer calls.

Final current source, approved TMPDIR, CARGO_BUILD_JOBS=3, RUST_TEST_THREADS=2,
normal default stacks and serial Cargo:

| Check | Result |
| --- | --- |
| `cargo test --locked -p oc-adapters --lib auth04_` | 5/0, including prior binding scenario. |
| `cargo test --locked -p oc-adapters --lib go02_` | 13/0; original public/source/retirement/read-only assertions. |
| `cargo test --locked -p oc-adapters --lib go05_` | 8/0; scoped accounts, exact choices/busy captures, child/fork controls. |
| `cargo test --locked -p oc-adapters --lib` | **649 passed / 0 failed / 1 unchanged Go opt-in ignored**, 95.80s. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Exit0. |
| fmt/diff/journal structure checks | Exit0. |

Full current lib log:
`/home/opencode/.local/share/opencode/tool-output/tool_111b7f55f001qKXa5T1UtwVCWz`.
No test disable, timeout/stack/threshold increase, baseline rewrite, validation
weakening or live call. Existing integration fixture structs explicitly retain
public_openai_enabled=false. The one exact connection-list expectation now includes
the required native OpenAI target; all original scope/privacy/no-selection checks stay.

## Diagnoses and remaining work

Initial compile found required new Generation fixture fields and borrowed temporary
maps in a concurrent test; explicit fields/stable local bindings fixed them. A
read-only query used guessed column names and returned false; actual T53 schema
names fixed it. An initial fixture used the process-env listing API and hit unrelated
configuration; it was moved to the existing controlled-env catalog owner test, with
no global env mutation, content logging or policy relaxation. First broad lib run
had only the old three-connection expectation fail after native OpenAI admission;
that explicit contract-driven expectation was extended, then the full lib gate passed.
Final review also made an explicit OpenAI selection fetch its cold public slice even
with Go disabled, while unrelated Go-only documents still do not cause a second GET;
the same owning read-only fixture proves both cold and cached paths.

Application5331 (+19 here) and runtime/turn5109 (+45 here) retain their existing
shared mutation/turn owners; CODE_MAP records their next natural query/transition
and capture/preparation seams. No framework or minification to hide growth.

Next: actual native Responses WS/HTTP, all-lane preparation and refresh, affinity /
fully-consumed checkpoints, safe same-route fallback and no ambiguous replay plus
the mandatory restricted-error amendment. Then shared CLI/TUI and actual-binary
gates, dedicated owner-operated AUTH06. T44 PAUSED, T53/T56 evidence untouched;
user-owned .opencode unread/unstaged. Do not finish from partial catalog proof.
