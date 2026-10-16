# T46 — native MCP media tool-output bridge

## Result — independent coordinator review, 2026-09-29

**PASS for the frozen media atomic, not whole T46/READY.** Coordinator reviewed
all changed production owners, wire/persistence/SQL projection and native tests;
a separate read-only scout reviewed the four media/log source/test files and
direct trust consumers. No concrete frozen-contract violation was found.
Base is `b582dd367981202ea946a0c0eab151eb3d47c1e5` plus this reviewed diff.

## Checks — coordinator

- Independently inspected all 42 result summaries in
  `/home/opencode/.cache/opencode-tmp/opencode/T46-media-workspace.log`:
  **1254 passed / 0 failed / 9 unchanged opt-in ignored**. No source changed
  after those full gates; this preface/checkpoint is bookkeeping only.
- Re-ran `cargo test -p oc-adapters --lib --locked mcp12_`: **5/0**, 0.56 s;
  `cargo test -p oc --test mcp_application --locked mcp12_`: **3/0**, 1.55 s.
  Fmt, Python code-size/progress/docs tests (**5/15/14**), documentation/progress
  structure and diff checks all exited **0**. Changed-code size has no >5000
  warning. These checks do not promote unrun live/visual gates.
- Cargo test selected the test-profile debug ELF (digest `50cfb8e3f778bcd27706f15b50c0db07fd651a2c4e9530e9d15643b9da02d525`).
  Then `cargo build --workspace --locked` exited **0** and selected the final
  dev ELF below. Without another Cargo invocation, directly re-qualified the
  existing `mcp_application-c554f016ee604ae9` executable's media **3/0** (1.64 s),
  lifecycle **3/0** (4.24 s), MCP09 **4/0** (2.23 s), stdio **4/0** (14.36 s),
  public CoreApp lookup **2/0** (0.45 s). The 14 CLI/PTY and 2 CoreApp cases all
  pass; debug/release digests are identical before and after:

```text
debug   2be090047700711a49949eab77532d3366d055f31360f9ee3e8c28d302784661
release e724540586cab3594aa2c3235247c90c3c16af9422701a9f5c673f52fac7b84d
```

## Risks — coordinator

Mandatory R4 live, full T44/V09 and GOAL A01–A13 remain open. The existing live
runner is fail-closed because process-local counters do not yet enforce the
durable 24-generation/4-search envelope; this is a harness gap to implement,
not a proven external blocker. No live/authenticated request was made here.

## Next — coordinator

Deliver this reviewed media atomic, then implement/verify trusted durable bounded
live accounting before R4; continue the remaining backend and UI acceptance.

## Result — final atomic qualification

**PASS — native MCP media bridge, 2026-09-29.** Fresh workspace:
**1254 passed / 0 failed / 9 existing opt-in ignored**. Eight new scenarios
cover actual binary HTTP/stdio media, durable replay/projection/compaction,
permissions, sensitive data and malformed/capped results. Debug and release
build/help pass; final built debug ELF directly passes the native media proofs.
T46 remains open for mandatory R4 live; full T44/V09 and GOAL A01–A13 continue
with the parent. No task/progress/GOAL/acceptance or Git delivery mutation here.

## Result — frozen before implementation

**PENDING / NOT_RUN.** Atomic scope: admitted MCP image, audio, embedded blob and
mixed tool results must reach the next native Responses request, preserve the
call/result graph and bounded durable source facts, and survive restart and
projection without repeating tools or fetching resource URIs.

Base: `b582dd367981202ea946a0c0eab151eb3d47c1e5`. Existing R6, lifecycle and lookup
proofs remain historical. Active task T46; this report does not finish it.

### Source and integration decisions

- Donor pinned at `2670273ff17da96f85c5826ced57aa1b368754fa`:
  `packages/ai/src/protocols/open-responses.ts:146–159,203–206,507–537`;
  `packages/ai/test/provider/openai-responses.test.ts:1618–1705,1768–1816`.
  Ordered output arrays use `input_text`, `input_image` and `input_file`.
  Audio is a file; inline bytes use data URLs. Default file name is `file`
  (`document.pdf` for that exact MIME), without an implicit URI load.
- Pinned SDK `rmcp 3.4.0`, `src/model/content.rs:21–149`: MCP content declares
  text/image/audio/resource, MIME, annotations and `_meta`. The native bridge
  consumes this typed tool response, not model-authored text or guessed keywords.
- Existing clients, permissions, request leases and resource supervisor remain the
  owners. Legacy public string-returning `call_tool` remains compatible; runtime
  dispatch uses an additive rich path sharing the same actual request lifecycle.
- Native rich input has explicit Responses lowering. Original content order,
  structured JSON, resource URI/MIME, annotations and `_meta` are retained in the
  existing bounded turn-log JSON. Metadata is not privileged instruction input.
- The durable seam is an additive, native-owned attachment to the existing log,
  paired by input position and exact call id. Old string logs stay readable.
  Model/provider opaque output cannot acquire rich MCP authority by resembling
  a serialized result. No schema, store, tool alias, client registry or framework.
- Redact prose/metadata before capture. Validate exact identities and binary
  payloads with the current protected-value snapshot (including the parent's
  repaired, genuinely separate identity secrets). Reject sensitive/invalid binary
  or identity data as a whole; never corrupt base64, MIME or discriminators.
- Existing aggregate MCP 1 MiB, provider serialization, active-window and DCP
  budgets remain. Malformed/unsupported/capped results remain non-success and
  retain unknown-effect/quarantine, cancel, cleanup and owned-process guarantees.

## Checks — frozen obligations

| Obligation | Before implementation |
| --- | --- |
| Source-derived mixed-media result RED, then actual ordered next-request wire | NOT_RUN |
| Image/audio/blob/text/structured/resource metadata source facts preserved | NOT_RUN |
| Actual HTTP and stdio RPC paths; no duplicate connection/call or URI fetch | NOT_RUN |
| Real native application/binary tool RPC → next fake Responses request | NOT_RUN |
| Exact call id, truthful completed/unknown/failed graph and durable result | NOT_RUN |
| Restart/reopen, compaction and DCP projection; no media/tool replay | NOT_RUN |
| Legacy string API and old turn-log compatibility | NOT_RUN |
| Sensitive text/metadata/binary/identity, safe Debug, bounds and whole failure | NOT_RUN |
| Permissions, cancellation, quarantine, teardown and caps regressions | NOT_RUN |
| Workspace fmt, strict Clippy, locked tests, debug/release build and help | NOT_RUN |
| Final built-artifact direct proof and source/binary association | NOT_RUN |
| Paid/live R4, browser and full T44 visual qualification | NOT_RUN — parent next |

Commands use one Cargo at a time, `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=1`,
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`,
900000 ms command timeout. Bounded raw logs go to approved cache
`T46-media-*.log`, at most 16 MiB each. Repository additions are small fixtures,
synthetic counters and this report; no raw-frame campaign or quota increase.

## Risks

- Native rich results must not be reconstituted from a string or arbitrary
  provider output; log attachment and graph validation are an explicit seam.
- Existing image-as-unsupported runtime fixtures must change only because image
  becomes supported: their unknown-effect tests will use genuinely malformed or
  unsupported data and retain effect counts, reaping, caps and deadlines.
- Encoded protected bytes cannot be safely string-redacted. Reject the entire
  response before durable capture or provider publication.

## Next

Mixed-media actual-binary failing fixture → smallest shared projection/client
change → typed runtime/wire and durable-log seam → targeted transport/security/
replay proof → affected and whole-workspace gates → final built-artifact proof.
Parent continues mandatory R4 live, full T44/V09 and whole GOAL A01–A13.

## Checks — completed obligations

| Frozen obligation | Result / actual evidence |
| --- | --- |
| Source-derived mixed-media RED → real next-request wire | **PASS**. `mcp12_actual_binary_mixed_media_reaches_next_responses_request_with_exact_call_graph` initially failed after one actual MCP effect with unknown outcome; final native result reaches the next recorded Responses request. |
| Image/audio/blob/text/structured/resource facts | **PASS**. Actual mixed RPC gives seven ordered wire parts: text, image, audio-file, blob-file, embedded-resource text, trailing text, structured JSON. Native log retains URI/MIME, annotations and `_meta`; prose/metadata redacted. Single image/audio/blob lowering also independently checked. |
| Actual HTTP and stdio / exact existing connection | **PASS**. Actual admitted HTTP `tools/call=1`, `initialize=1`; stdio first/reopen `tools/call=1` total, `initialize=2`, `closed=2`, both owned PIDs gone. Clients share their original explicit cancellable request method, not a duplicate connection. |
| Native application/binary RPC → Responses | **PASS**. Real `oc run` receives actual fake MCP results and then emits the exact ordered array to fake Responses. No hardcoded provider-input injection or working-UI claim. |
| Call graph / durable truthful outcome | **PASS**. Native array is associated with the actual provider function call's `call_id` and advertised name. Completed media produces successful turn; denied dispatch has `state=denied`, zero MCP calls; sensitive post-effect binary has `state=unknown`, one call, no partial native result. Existing definitive failed/unknown/cancel tests pass. |
| Restart/reopen/DCP/compaction, no tool replay | **PASS**. Actual stdio restart replays persisted image bytes with exactly one tool call across both processes. Directed real-Db DCP test compresses ordinary messages, exercises SQL attachment index remapping and pair removal, then reopens with unchanged raw logs/history. Compaction receives actual native call/result pairs; checkpoint tail retains recent media after reopen, older raw facts remain unchanged. |
| Legacy String API / old logs | **PASS**. Existing public `call_tool -> String` and `FunctionCallOutput { output: String }` remain compatible. Rich runtime path is additive. Old logs roundtrip; model/provider arrays and text resembling media cannot become native attachments. |
| Redaction / exact identities / binary / Debug / caps | **PASS**. Text and arbitrary JSON metadata use existing redaction, including keys. Original protected identity snapshot validates URI/MIME and annotation timestamp without fabricating replacements. Actual decoded protected binary fails whole result; invalid base64/MIME, corrupt attachment, 65 parts and aggregate >1 MiB fail whole. Debug and tool presentation omit encoded bodies. |
| Permission/session/cancel/quarantine/cleanup/caps | **PASS**. Existing central dispatch used unchanged; actual Deny causes zero `tools/call`. All native cancellation/unknown-effect, sticky remote quarantine, explicit retry, scope-close/reap and generation-cap scenarios remain green. R6/MCP09 and root-session lookup tests remain green. |
| Workspace fmt / strict Clippy / locked tests / builds / helps | **PASS**, commands and exits below. No threshold, assertion deadline, cap or ignore increase. |
| Final built artifact association | **PASS**. Direct existing test executable runs 14 actual CLI/PTY cases against final debug `oc`, plus two native CoreApp lookup cases; binary hashes identical before/after. |
| R4 paid/live, optional browser and full T44 visual | **NOT_RUN** in this atomic; parent next. No whole-T46 or READY claim. |

### Wire, durable seam and source association

- Donor/source pin and integration choices are the frozen ones above, verified
  against `opencode` HEAD `2670273ff17da96f85c5826ced57aa1b368754fa` and local
  pinned `rmcp 3.4.0`. Source base remains
  `b582dd367981202ea946a0c0eab151eb3d47c1e5`; reviewed worktree diff is the media
  implementation, tests, `CODE_MAP.md` and this report. No dependency/toolchain
  or SQL-schema upgrade, new store/registry/framework, daemon or model allowlist.
- `mcp_result/media.rs` accepts typed native MCP facts. Binary is validated as
  base64 and checked in decoded bytes. Exact URI/MIME/timestamp stays exact or
  the entire response fails. Redacting structural MIME, content discriminators,
  base64 or a timestamp into an invalid synthetic value is not allowed.
- Responses output uses only donor standard fields. Actual PNG becomes
  `input_image.image_url = data:image/png;base64,...`; WAV becomes
  `input_file {filename:"file",file_data:"data:audio/wav;base64,..."}`;
  embedded octet-stream blob becomes the corresponding file data URL. No
  `input_audio`, custom `_meta`, URI or annotation fields are added to wire parts.
  Resource text/links and structured JSON remain text parts in original order.
  `file:///not-read-by-native` is a source fact, not a filesystem/HTTP fetch.
- Opaque `McpToolOutput` has safe Debug. Existing tool-card/journal presentation
  is at most 8 KiB for rich media and contains no base64. Complete bounded source
  facts are kept once in `native_mcp_results` in the existing turn JSON; input
  contains its compatible string presentation, not a duplicate media archive.
- Attachment decode validates input index, call id, preceding real call,
  source shape, whole bound and presentation equality. Neither a provider array
  nor a model-authored string or nested `native_mcp_results` field grants native
  provenance. SQLite's existing bounded presentation query preserves and
  reindexes attachments using the same retained-input filter before transfer.
- DCP handles native call/output as one graph pair and estimates actual prose;
  media-aware context estimates include binary/file cost. Native compaction
  strategy already receives the typed history. Fallback compaction keeps prose
  transcript as unprivileged JSON data and supplies actual media with its
  original call/result pairs, no tools offered and no duplicated base64 text.
  Checkpoint deliberately replaces older context with summary; retained recent
  media and immutable archive facts remain reconstructible, without re-fetch.
- Existing source/config/client leases, central permissions, durable intent and
  unknown-effect cleanup paths remain owners. Standalone legacy String methods
  may still return categorical unsupported for media; actual runtime uses the
  rich path and does support image/audio/blob/mixed for all model IDs, with any
  real provider rejection remaining explicit.

### Commands and exits

All Cargo commands were serial with the frozen environment above and 900000 ms
command timeout. Cache prefix:
`/home/opencode/.cache/opencode-tmp/opencode/T46-media-`.

| Command / proof | Exit | Result / cache suffix |
| --- | --- | --- |
| `cargo test -p oc-adapters --lib --locked mcp12_` | 0 | 5/0; `lib-targeted.log`, 0.41 s after fixes. Also all five pass in affected/final workspace runs. |
| `cargo test -p oc --test mcp_application --locked mcp12_` | 0 | 3/0; `native-targeted.log`, 1.52 s. |
| `cargo test -p oc-adapters --lib --test mcp_stdio --test mcp_remote --test runtime --locked` | 0 | lib 332/0; HTTP 27/0/1 existing ignored; stdio 21/0/1 existing ignored; Runtime 93/0; `affected.log`. |
| `cargo test -p oc --test mcp_application --locked` | 0 | 40/0, 72.54 s; `native-full.log`. |
| `cargo fmt --all -- --check` | 0 | Workspace formatting. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | Final strict workspace/all-targets check; `clippy.log`. |
| `cargo test --workspace --locked` | 0 | **1254/0/9 existing ignored**, all targets and doc-tests; `workspace.log`. Native MCP 40, T39 PTY 39, T42 PTY 34, adapter lib 332, Runtime 93, Core 26, TUI 417 all pass. |
| `cargo build --workspace --locked` | 0 | 9.14 s; `build-debug.log`. |
| `cargo build --workspace --release --locked` | 0 | 1m41s; `build-release.log`. |
| `target/debug/oc --help`, `target/release/oc --help` | 0, 0 | `help-debug.log`, `help-release.log`. Release functional media/PTY is not separately claimed. |
| `python3 -m py_compile fixtures/mcp12-media.py` with owned-cache `PYTHONPYCACHEPREFIX` | 0 | Syntax checked; no generated cache in the repository. |
| `python3 scripts/code_size.py --base b582dd367981202ea946a0c0eab151eb3d47c1e5 --changed` | 0 | 20 changed code/test files, no >5000 warning; largest production file 2423 lines; new media owner 362 lines, log slice 65 lines. |
| `git diff --check`, no staged diff / unchanged base verification | 0 | Source/diff reviewed; no commit or stage. |

After the **last** workspace build, without invoking Cargo again, executed
`target/debug/deps/mcp_application-c554f016ee604ae9 <filter> --test-threads=1`
with `RUST_TEST_THREADS=1` and the owned `TMPDIR`. Its compiled
`CARGO_BIN_EXE_oc` resolves the final `target/debug/oc`:

| Filter | Result | Cache suffix |
| --- | --- | --- |
| `mcp12_` | 3/0, 1.61 s — actual HTTP/stdio media/restart, Deny/sensitive binary | `built-media.log` |
| `lifecycle::` | 3/0, 4.22 s — actual pre-prompt independence, controls, stale scope/reap | `built-lifecycle.log` |
| `mcp09_` | 4/0, 2.05 s — actual admitted env/cwd/PATH/domain and disabled zero-effect startup | `built-config.log` |
| `v07b_stdio` | 4/0, 14.17 s — actual owned cancellation, unknown/malformed/unsupported/retry and definitive failure | `built-stdio.log` |
| `lookups::` | 2/0, 0.42 s — real public application/CoreApp path, not a CLI lookup UI claim | `built-lookups.log` |

`sha256sum target/debug/oc target/release/oc` before **and** after these tests:

```text
debug   2be090047700711a49949eab77532d3366d055f31360f9ee3e8c28d302784661
release e724540586cab3594aa2c3235247c90c3c16af9422701a9f5c673f52fac7b84d
```

These are native artifact associations, not per-file refactoring manifests.
No production/test source changed after final gates and builds; only this
factual report was completed afterward.

### Failed experiments and diagnosis retained

- `mixed-red.log`: actual binary **exit 101**, 0/1, 0.37 s. Media was previously
  unsupported and actual post-effect tool outcome became unknown. It was not a
  compile-only fixture or an injected successful provider array.
- `restart-red.log`: first stdio media attempt failed because a short product
  environment value coincided with the base64 alphabet. Corrected binary check
  compares decoded bytes, retaining all protected values, with no allowlist or
  string mutation. Actual sensitive decoded canary still fails whole.
- `restart-projection-red.log`: actual reopen produced **two** `tools/call`
  instead of one, 0/1, 0.65 s. SQL presentation omitted the attachment. Preserving
  and remapping it fixed replay. `binary-targeted.log` retains the intermediate
  owned SQL-parenthesis error; no baseline/schema or archive rewrite.
- `lib-first-failure.log`: 3/2, exit 101. One synthetic test expected an
  untyped conflicting resource body to survive the SDK's typed branch decode;
  moved that corruption assertion to direct durable-facts validation, where
  the whole attachment is rejected. Other failure exposed fallback compaction
  reducing media to quoted JSON, repaired by actual native call/result pairs.
- `clippy-first-failure.log`: exit 101, collapsible nested condition. Repaired
  using the existing Rust let-chain style; no lint allows or warning relaxation.
- `workspace-sigint-failure.log`: first full attempt failed unchanged
  `responses::aud12_binary_delta_before_terminal_and_silent_stream_sigint`
  at its **950 ms** actual binary exit deadline (`responses.rs:284`). The source
  has no diff, fixture has no MCP/media. Exact unchanged isolation:
  `cargo test -p oc --test responses --locked aud12_binary_delta_before_terminal_and_silent_stream_sigint -- --exact`
  **0, 1/0, 0.56 s** (`sigint-diagnosis.log`). Fresh unchanged full workspace
  subsequently passes, responses 4/0 in 1.25 s. Timing/transient observation,
  root cause not proven; no assertion/deadline/cap or test changes for green.
- Old runtime image-as-unsupported fixtures are intentionally superseded:
  real malformed base64 and genuinely unsupported MCP-2025 video now exercise
  unknown-effect gates. Original effect counts, unknown state, no partial
  payload, quarantine, explicit retry, real reaping and deadlines are retained.
  Valid image remains supported in the runtime; legacy String wrapper remains
  text-only. These changed fixtures pass both full and final-ELF direct gates.

## Risks — remaining boundaries

- New frozen scope is fully qualified offline. Mandatory R4 live, real
  OpenProxy/media capability behavior and full T44/V09/visual consumer proof are
  not performed here and do not become PASS from fake/native tests.
- `_meta`/annotations are typed native source facts, not additional Responses
  fields or policy/developer messages. Binary resource URIs never cause native
  external/local fetching. Unknown modern content/protocol remains categorical
  non-success, no new interpreter or protocol implementation.
- Raw history remains immutable; checkpoint/DCP intentionally affect provider
  projection. A whole bound/security/cleanup failure is not partial media
  success, and an uncertain admitted remote effect is not automatically retried.
- Only tiny source fixtures and this report are new evidence/data additions;
  raw counter logs live in disposable owned fixtures/cache. No raw-frame
  campaign, inherited/foreign output deletion, quota increase or paid call.
  Pre-heavy checks: uid 1003, 7683 MiB available RAM, 174 GiB available disk.

## Next — ownership handoff

Parent reviews/delivers this atomic media work, then continues mandatory R4 live
and full T44/V09 + GOAL A01–A13 and independent ready tasks. No T46 finish or
progress/status/GOAL/acceptance change in this atomic. Mutation/Cargo/PTY/build
ownership is released in the final handoff response.
