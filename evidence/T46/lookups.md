# T46 — explicit native prompt/resource lookup

## Coordinator qualification after independent review

Base remains `5f5d4a3476e7514ab6a8e794a2f8ac69504813fa`. The coordinator reviewed
all changed owner/transport/DTO/test paths and the new modules below. This section
is the current qualification; the agent's 1245-test/build record remains the
historical pre-coordinator phase, not relabelled as the repaired source.

One reproduced privacy defect was repaired. Prompt name and argument keys from
the caller were copied into `McpLookupData::Prompt` without the protected-identity
check applied to server metadata. The new focused stdio regression failed with
exit 101 (`Ok(Prompt)` instead of `SensitiveIdentity`). The shared lookup validates
caller targets and prompt argument keys before RPC, preserving exact identities
or returning typed non-success rather than fabricating a redacted identity.

The stronger pre-dispatch check also exposed that `StdioClient.config.secrets`
had already been augmented with ordinary argv for stderr/body redaction. The held
read test consequently returned before its expected RPC. The existing client's
private identity set now captures the original config/environment protection
before that augmentation; full stderr/body/argv redaction is unchanged. The
regression proves an ordinary argv word is a legitimate prompt name, while all
three protected caller identities issue zero disallowed body RPCs. This is not
exclusion of credential values merely because they also appear in argv.

Current serialized gates: fmt, workspace/all-target strict Clippy, locked workspace
tests **1246 passed / 0 failed / 9 existing opt-in ignored**, debug/release locked
workspace builds and both help commands, Python code_size 5/progress 15/check_docs
14, docs/progress and diff checks all exited 0. Full coordinator log:
`/home/opencode/.local/share/opencode/tool-output/tool_0eafddd7f001DGv3mVlWgPc3HN`.
The targeted `mcp11_` checks passed 5 backend + 5 stdio + 2 HTTP + 2 native Core
caller scenarios. No deadline, cap, baseline, assertion or existing ignore was
loosened; no dependency/toolchain/schema/config authority change was made.

Current debug SHA256:
`3836d9f559326bb225360dff16bf1556dfa325fbe00415b9e93f3665e9bbb45e`.
Current release SHA256:
`dfb4f13fc796ac29a1a90c5aa701d9774762ce48138b320263447a21fb1a49f3`.
After those builds, direct existing `mcp_application-c554f016ee604ae9` runs with
filters `lookups::`, `lifecycle::`, `mcp09_`, and `v07b_stdio` passed 2/3/4/4 cases
in 0.52/4.32/2.22/14.39 seconds; both binary digests stayed identical. The two
lookup cases are real native-crate CoreApp callers, not a nonexistent CLI lookup
command. The remaining cases exercise the actual built debug ELF.

Prompt/resource catalogs and explicit bounded caller reads are PASS. Media output
bridging, mandatory R4 live, full T44/V09 and product READY remain open; continue
them rather than finish T46 here.

## Frozen atomic plan (before implementation)

Base `5f5d4a3476e7514ab6a8e794a2f8ac69504813fa`; active T46. This slice
qualifies prompt list/get, resource/resource-template catalogs and resource read
through the existing clients, resource supervisor and typed CoreApp caller.
Media tool-output bridging, mandatory R4 live and T44 paired presentation remain
parent work. No task/GOAL/progress/config/dependency/storage-schema changes.

Pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`,
`packages/core/src/mcp/client.ts:240–283`, `mcp/index.ts:649–735` separates
metadata catalogs from targeted prompt/resource retrieval. Native failures retain
typed non-success and the last healthy metadata; donor empty/undefined error
fallbacks do not authorize successful partial output or clearing healthy data.
Use pinned rmcp 3.4.0 explicit request handles, not unbounded `list_all_*`, stale
SDK cache fallback or modern input-required continuation helpers.

### Narrow caller/admission interpretation

- A backend-only typed lookup carries the exact current MCP binding, opaque
  server identity, existing root SessionId and operation/target/arguments. It
  shares the already connected client; metadata/lookup never activates disabled
  entries or waits for unrelated startup. Pending/missing capability is a typed
  unavailable outcome, not an inferred connected state or RPC global failure.
- Existing session Location validation and session-scoped primary-agent
  selection precede I/O. Child sessions are rejected by this root-caller API;
  their capability narrowing is not replaced with primary authority. Central
  permissions are intersected with that actual primary's rules using the same
  PermissionRules owner, without modifying an in-flight runtime workspace.
- Permission-only action `mcp_lookup` uses the exact JSON identity tuple
  `[raw server, MCP method, target]` as resource (catalog target `*`). No alias or
  model-visible tool is registered. Body retrieval additionally requires existing
  `read` Allow for the exact remote URI/prompt identity; Deny and Ask remain
  non-success. URI is an MCP server argument, never an HTTP/filesystem fetch.
- Catalogs are metadata only, fetched only on explicit request and published
  atomically per successful catalog. Failed refresh retains last healthy data.
  Exact names/URIs/templates and ordered arguments remain identity facts.
- Prompt messages preserve actual ordered roles/content; resources preserve
  URI/MIME and typed text/blob. Returned prose never becomes policy/system
  authority, provider projection or history by default. Complete payloads have
  the existing 1 MiB result bound; diagnostics/Debug never dump them. Successful
  human text uses existing configured-value redaction. Binary data stays typed,
  not decoded into purported text or a model-visible media result.
- Reuse page 16, per-server entry 64, generation entry 128/catalog 1 MiB and
  normalized catalog/execution/cleanup budgets. Validate cursor/duplicates and
  aggregate metadata before publication; no unbounded archive/list helper.
- Lookup jobs/cancel flags/client leases belong to the existing Scope JoinSet.
  Stale binding is rejected before network; cancellation/drop is request-specific,
  cleanup failure stays fatal, unknown remote outcome stays quarantined and is
  never automatically replayed. Reload/Location/shutdown cancel/join owned work.

### Obligation matrix

| Obligation | Result before code | Planned proof |
| --- | --- | --- |
| bounded prompt/resource/template catalogs; capability absence | NOT_RUN | source-derived failing stdio catalog first; HTTP/stdio paginated counters, caps/duplicates/loops/absent capability |
| prompt name/arguments/ordered roles/content; resource URI/MIME/text/blob | NOT_RUN | actual transport fixtures with exact typed synthetic results, no implicit body loads/fetch/role escalation |
| real CoreApp root/session/policy/source/binding boundary | NOT_RUN | exact and stale binding, deny/ask/agent narrowing/other Location/child rejection before RPC |
| one owned connection; slow neighbor and responsive cancel/status | NOT_RUN | held lookup barriers, initialize/list counts stable, status/control reads independent, request-specific cancellation |
| retained healthy metadata; safe current/next request catalog | NOT_RUN | failed explicit refresh preserves catalog, tools/guidance/history unchanged by lookup, successful next metadata snapshot |
| bounded payload/redaction/safe errors and unsupported modern interaction | NOT_RUN | known canaries, payload cap, opaque error body, explicit unsupported response, typed binary proof |
| drop/reload/Location/shutdown join/reap; fatal cleanup/quarantine | NOT_RUN | actual Core/native caller owned PID/HTTP/cancel counters and no stale completion/replay |
| fmt/strict Clippy/full workspace/build debug-release/help | NOT_RUN | serialized targeted/affected and fresh required final gates |
| media/live/T44 full presentation | NOT_RUN | separate parent continuation |

## Result

**PASS — this atomic prompt/resource backend slice.** The frozen plan above is
retained as its pre-code record. Source base remains
`5f5d4a3476e7514ab6a8e794a2f8ac69504813fa`; delivered implementation is the
reviewed working-tree delta, pending parent review/commit. No task, progress,
GOAL, ACCEPTANCE, user config, dependency/toolchain or storage-schema mutation.
Inherited `.opencode` was never inspected/edited/staged.

The typed caller port is `CoreApp::mcp_lookup(McpLookup)`. Its operations are
ListPrompts, ListResources, ListResourceTemplates, GetPrompt with exact name and
string arguments, and ReadResource with exact server URI. `McpLookupReply` binds
the response to Location/generation/instance, opaque server and safe source.
This is an actual CoreApp/backend port; there is no claimed CLI/lookup UI or
model-visible alias. The native-crate caller tests execute the real application
worker/CoreApp. Separately, the built `oc` ELF is qualified below by actual PTY
config/lifecycle/stdio scenarios, not by claiming that those scenarios invoke
the new lookup API through a nonexistent command.

### Final obligation matrix

| Obligation | Result | Actual evidence |
| --- | --- | --- |
| bounded catalogs and honest absence | PASS | stdio + HTTP paginated prompts; resource/template catalogs; 65 entries rejected, cursor loop/duplicate/16-page bounds; absent capability yields `available=false` without RPC |
| prompt and resource source facts | PASS | ordered declared arguments, User/Assistant messages, exact prompt name/arguments/URI/MIME; actual text and blob `AAEC/w==`; no eager get/read or URI HTTP/filesystem fetch |
| root/session/agent/policy/source/binding admission | PASS | session-selected agent Deny and Ask before RPC; auto approval cannot grant; exact JSON identity rule; foreign/child/missing sessions and disabled entry rejected; old binding stale before I/O |
| same owner/client, responsive neighbors/status/control | PASS | one initialize/tools-list per connection, target-only Arc lease; held read while status returns within 500 ms and sibling disconnect actually reaps; no unrelated startup wait |
| healthy metadata and request boundaries | PASS | failed refresh `RemoteFailure` retains prior cached catalog/Connected client; later explicit metadata snapshot succeeds; lookups add no model tools/history/operations and do not mutate in-flight catalog/guidance |
| bounds/redaction/unsupported result | PASS | 1 MiB body bound, known text/header/env canaries redacted, sensitive binary/identity rejected entirely, payload-free Debug/errors; actual rmcp `input_required` response rejected without continuation/retry |
| owned cancel/drop/reload/Location/shutdown | PASS | exact request-ID cancellation, observed stdio cancellation; reload/Location cancel and join held jobs, owned PIDs gone, no late catalog/read replay; remote unknown sticky; failed cancellation is fatal |
| generation and cleanup caps | PASS | 2 tools + 64 + 64 prompt entries exceed 128: typed cap error, fatal worker, both clients closed/reaped; original cleanup budgets unchanged |
| fmt/strict Clippy/full workspace/debug-release build/help | PASS | commands/exits and current artifacts below |
| media bridge, R4 live, full T44/V09/A01–A13 | NOT_RUN | parent continuation; this report does not finish T46 or claim READY |

### Implementation ownership

- `oc-core/src/queries/mcp_lookup.rs` owns payload-safe typed DTOs; existing
  `core_app.rs` inbox/oneshot port cancels its request when the caller future drops.
- `oc-adapters/src/application/mcp_lookup.rs` validates current binding/root
  session and intersects central rules with that actual session's primary agent
  through the existing permission owner before enqueueing. A child does not gain
  primary authority, and a passive catalog does not authorize body execution.
- Existing `runtime/mcp/lifecycle.rs::Scope` owns lookup jobs in its JoinSet;
  child `lifecycle/lookups.rs` uses only its exact existing client, bounded
  metadata slots and target-only leases. Connection weak identity prevents stale
  completion replacing a reconnected client's cache. There is no second client
  registry/store/eventbus/daemon, Arc<Db>, or duplicated durable counter owner.
- Shared `oc-adapters/src/mcp_lookup.rs` performs explicit request-once calls on
  the existing peer. It uses actual rmcp 3.4.0 constructors/cancellable request
  IDs, manual bounded pagination and configured catalog/execution deadlines.
  No unbounded SDK list helper, SDK stale-success cache, modern fulfillment or
  new protocol implementation is used.
- Successful exact identities stay unchanged. Returning a known protected
  config/environment value as an identity is `SensitiveIdentity`, rather than
  fabricating a redacted URI/name. Text/body redaction retains all admitted env
  and argv values; ordinary argv words are not identity credentials. Binary is
  validated and retained as actual base64, never decoded into alleged text;
  known protected decoded content is `SensitiveBinary`, not partial success.
- Unknown dispatched remote body outcome quarantines the existing generation;
  later body/tool/provider dispatch is refused, not replayed. Cancellation
  cleanup failure is `McpShutdown`/fatal. Retirement re-reads remote quarantine
  after joining all owned jobs before transfer to replacement runtime.

## Checks

All Cargo commands were serialized with `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=1`,
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`,
timeout 900000 ms. Host preflight: uid 1003; initially 7319 MiB available RAM /
176 GiB disk, before final heavy checks 7003 MiB / 175 GiB. No paid/live/browser,
npx, runner configuration/auth/env extraction or host/global cleanup operations.

Log directory for all names below:
`/home/opencode/.cache/opencode-tmp/opencode/`.

Final retained raw-log maximum is 112880 bytes (`T46-lookups-workspace.log`),
below 16 MiB. This report plus the one new source/counter fixture is under
26 KiB; no new repository raw-frame/payload campaign was captured.

| Command / gate | Exit / actual result | Log |
| --- | --- | --- |
| `cargo test -p oc-adapters --test mcp_stdio --locked mcp11_` before API | 101; compile-absence RED, not executed cap proof | `T46-lookups-catalog-red.log` |
| same initial bounded-catalog test after implementation | 0; 1 passed, actual 65-entry rejection/reap | `T46-lookups-catalog.log` |
| `cargo test -p oc-adapters --lib --test mcp_stdio --test mcp_remote --locked mcp11_` | 0; 5 Core/backend + 2 HTTP + 4 stdio = 11 new scenarios | `T46-lookups-targeted.log` |
| `cargo test -p oc --test mcp_application --locked mcp11_` | 0; 2 native-crate real Core caller scenarios | `T46-lookups-native.log` |
| `cargo fmt --all -- --check` | 0 | terminal gate |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0; no added allows | `T46-lookups-clippy.log` |
| `cargo test -p oc-adapters --lib --test mcp_stdio --test mcp_remote --test runtime --locked` | 0; lib 327, HTTP 27 + 1 existing ignored, stdio 20 + 1 existing ignored, runtime 93 | `T46-lookups-affected.log` |
| `cargo test -p oc --test mcp_application --locked` | 0; 37 passed, 70.98 s | `T46-lookups-native-full.log` |
| `cargo test --workspace --locked` | **0; 1245 passed / 0 failed / 9 existing opt-in ignored**; every target and doc-test completed | `T46-lookups-workspace.log` |
| `cargo build --workspace --locked` / `target/debug/oc --help` | 0 / 0; debug build 12.37 s | `T46-lookups-build-debug.log`, `T46-lookups-help-debug.log` |
| `cargo build --workspace --release --locked` / `target/release/oc --help` | 0 / 0; release build 1m 40s | `T46-lookups-build-release.log`, `T46-lookups-help-release.log` |
| `git diff --check`; staged diff; source/donor HEAD verification | 0; staged empty; base and donor pins unchanged | terminal gate |

New scenario count is **13** (5 backend + 4 stdio + 2 HTTP + 2 native Core caller),
parent baseline 1232 → final 1245. Existing ignores, caps, deadlines, assertion
strength and baselines were not loosened. The complete workspace includes lib
327, Core 26, TUI 417, runtime 93, native MCP 37, T39 PTY 39 and T42 PTY 34.

### Current build/native association

After the final workspace test and workspace debug/release builds, the existing
native test executable `target/debug/deps/mcp_application-c554f016ee604ae9`
was run **directly** (no Cargo rebuild). Its `CARGO_BIN_EXE_oc` fixture path is
`target/debug/oc`. Hashes of both artifacts below were identical before/after:

| Artifact | SHA256 | Qualification |
| --- | --- | --- |
| `target/debug/oc` | `ac05704c5886ddfc3209a64ee4cd6193908e8a43646122283ddc58d45155ae14` | build/help + direct actual PTY lifecycle/config/stdio cases |
| `target/release/oc` | `1c231f06c41b40748fec5fdd021338c1655881e844e5ba9520570574b8828acc` | build/help; no release functional PTY claim |

Direct command prefix: `RUST_TEST_THREADS=1 TMPDIR=<owned path>` followed by
`target/debug/deps/mcp_application-c554f016ee604ae9 <filter> --test-threads=1`:

| Filter | Exit / result | Log |
| --- | --- | --- |
| `lifecycle::` | 0; 3 passed, 4.29 s; actual preprompt startup/healthy tool call/controls/reload/Location/reap | `T46-lookups-built-lifecycle.log` |
| `mcp09_` | 0; 4 passed, 2.13 s; actual disabled-zero-effects/environment/cwd/credential-domain | `T46-lookups-built-config.log` |
| `v07b_stdio` | 0; 4 passed, 14.27 s; actual stdio unknown/cancel/quarantine/reap | `T46-lookups-built-stdio.log` |
| `lookups::` | 0; 2 passed, 0.43 s; native-crate real Core HTTP drop/fatal-cleanup caller proof, not CLI lookup invocation | `T46-lookups-built-core-lookup.log` |

### Sanitized counters and failure diagnosis

Fixtures record method/PID or safe boolean/counter facts only, not fetched
payloads, env values, keys or raw remote errors. `fixtures/mcp11-lookups.py` is
offline source-derived stdio; HTTP fixtures are existing loopback owners.

- Stdio basic connection: initialize 1; prompt pages 2; no get/read before
  explicit caller action; exact arguments boolean true; ordered roles and actual
  blob preserved; normal shutdown's owned PID gone. Malicious loops/pages stop
  at actual 2/16 RPCs, duplicate at 1, 65-entry catalog at 1; no partial result.
- Body-limit/sensitive-binary/modern-input-required failures each make one read
  only; modern result is never automatically continued. Metadata-sensitive
  identity makes one list and returns typed non-success, not a leaked name.
- HTTP lookup uses one initialize and the exact configured MCP path; execution
  deadline's cancellation ID matches its one held read request. No target URI
  HTTP/filesystem dereference and no resources converted into model tools.
- Held Core stdio reads: caller drop/reload/Location account for 3 reads across
  2 initialized connections, cancellation observed; status stays responsive and
  sibling disconnect joins/reaps independently. Late old barrier cannot mutate
  replacement inventory/catalog. Deny/Ask/child/foreign/disabled paths issue zero
  disallowed body RPCs. Combined catalog cap closes/reaps both actual children.
- Native Core HTTP unknown-drop case: initialize/read 1 each, exact cancellation
  ID, later body query `UnsafeRetry`, zero model/title requests; shutdown closes
  owned held HTTP. Cancellation HTTP 500 case: `CleanupFailed`, worker join Err,
  zero additional reads/model calls, no private body in diagnostics.

Failures were diagnosed and corrected, not labelled external blockers or hidden
by broader retries:

1. Initial compile-absence RED is recorded above. Actual SDK corrections used
   `PaginatedRequestParams::default().with_cursor(...)` and guarded peer-info
   availability rather than inventing non-exhaustive struct/borrow APIs.
2. HTTP automatically supplies SDK `_meta`. The original exact entire-param
   assertion failed; exact name/arguments remain asserted and ONLY
   `name`/`arguments`/`_meta` keys are admitted. Failure retained in
   `T46-lookups-targeted-wire-shape-failure.log`.
3. The combined-generation cap test exposed an owned R5 stopping-loop defect:
   already-stopping `begin_stop` repeatedly published the same fatal state;
   publication dropped a view, woke the supervisor and starved the executor.
   Two 900000 ms attempts are preserved as
   `T46-lookups-generation-cap-hang.log` and
   `T46-lookups-generation-cap-diagnosis.log`. Fixed by publishing only a changed
   fatal result while already stopping; later cleanup failure still wakes fatal
   monitors. No deadline/cap/cleanup relaxation or detached worker was added.
4. A temporary static-only diagnostic trace exceeded the 16 MiB single-log limit
   during that busy loop. Only the exact owned debug Cargo/test/fixture processes
   were stopped after parent/argv verification; foreign processes were untouched.
   Only that new duplicate diagnostic trace was removed and replaced by bounded
   factual `T46-lookups-generation-cap-trace-summary.log`; the two original failed
   experiment logs remain. All tracing was removed; final retained logs are
   bounded. No repository evidence/raw campaign or quota increase resulted.
5. Identity checks initially included ordinary argv `held` and wrongly prevented
   its fixture URI read before dispatch. The bounded failure and concrete split
   correction, plus strict-Clippy repairs, are recorded in
   `T46-lookups-refinement-diagnosis.log`. Config/env identity protection and
   full text/argv redaction remain, and final targeted/full gates pass.

## Risks / Next

- Ready-only explicit root Core caller interpretation is intentional: pending
  clients return `Unavailable`; catalogs never activate disabled entries, and
  there is no whole-startup/query barrier. Ask requires a future explicit caller
  approval integration and remains `ApprovalRequired`; child lookup is explicitly
  unsupported rather than silently borrowing root authority.
- Known protected identities/binary cannot be faithfully published safely:
  explicit typed non-success replaces partial/redacted identity fabrication.
  Prompt facts do not authorize executable reads or become policy; bodies remain
  caller-only. Modern input-required/other unsupported response stays unsupported
  on native legacy negotiation, with no SDK/protocol upgrade or hidden replay.
- Parent reviews/delivers this delta, then continues the media tool-output bridge,
  mandatory R4 live, T44 paired VIS19/VIS40/VIS42/full V09 and whole GOAL A01–A13
  plus independent T45/T50/T51. Whole T46/READY remains open; no finish/status
  mutation was performed in this atomic slice.
