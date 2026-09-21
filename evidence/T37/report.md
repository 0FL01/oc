# T37 — MCP config, registry и владение ресурсами

Status: **PASS for T37 mandatory offline scope**, not overall product READY.
Implementation: `885fcd1` (client/registry work) and `b02d75a` (shutdown,
cancellation-lease, per-server refresh, caps) on base `839279c`.
Findings: **F10, F11**. Contract: `audit/repairs/T37.md`.
Initial red: [regression.md](regression.md). Executed gates: [checks.md](checks.md).
No live/paid endpoint, real credential or valuable user file was touched.

## Result

The actual binary now uses unmodified user MCP configuration, owns MCP
resources for the whole Location/config generation, and keeps registry/result
semantics exact.

`headers.Authorization` from the user config reaches a strict fake in any
case spelling; case-insensitive duplicates with different values are a terminal
config error before any network use; safe custom headers pass through, while
native Authorization/Accept/Content-Type and transport headers stay
provider-controlled. Header values are redacted from `Debug`. URL validation
uses a standard parser (scheme, host, userinfo, query, fragment), OAuth stays
refused, and no probing/rewriting happens.

`codex_web` remains rmcp streamable HTTP at the exact configured URL with
per-request `MCP-Protocol-Version: 2025-11-25`, JSON and SSE accepted, optional
GET 405 harmless, one attempt per request. Search emits the server schema
(`query`, `response_length`) and never the old helper `limit`.

Clients live on the config generation: two turns of one generation use one
handshake and one catalog, reload/disable closes the old generation before the
new one is used, and cleanup runs in an owned task so a dropped caller future
cannot abandon it. Partial attach, cancelled attach and aborted turns release
the single-flight lease through RAII and reap what was already connected.
`WorkerGuard::join` now surfaces cleanup failure instead of discarding it, so
the binary exits non-zero rather than claiming a clean shutdown.

Registry identity is a map, not a string split: collision-renamed wire names
route to the exact original server/tool, duplicates and oversized schemas fail
the whole catalog, and nothing is silently truncated. Non-text results and
`isError` stay distinct failures. Local servers receive the trusted project cwd
and a minimal non-credential environment, run in their own process group, and
are terminated TERM→grace→KILL with the SIGKILL fallback kept armed until reap
is confirmed.

## Acceptance mapping

| ID / finding | Evidence and result |
|---|---|
| AUD22 / F10 | Actual binary with `headers.Authorization` connects to a strict fake that checks method/path/bearer; lowercase/mixed-case equivalents pass; conflicting duplicates fail with an `authorization`/`conflict` diagnostic and zero MCP/provider network. `aud22_binary_accepts_user_authorization_spelling_at_strict_mcp`, `aud22_binary_rejects_conflicting_authorization_duplicates_before_network` PASS. Unit headers/URL/search tests execute in `mcp_remote`. |
| AUD23 / F11 | Real PTY TUI sends two prompts and the fixture log shows exactly one spawn + one initialize + one list; a disabled `chrome-devtools` trap never executes; restarting the real binary with the entry disabled spawns nothing and still answers. `aud23_tui_two_turns_own_one_stdio_child_and_disabled_entry_zero_spawns` PASS. Runtime regressions add generation reuse + reload/disable reaping, partial-attach cleanup, aborted-turn lease release with shutdown reap, dirty-server-only relist with no lost notification, and the pre-spawn server cap. |
| AUD24 / F11 | Collision-prone `a__b__c` variants route to the correct original server/tool through the retained map; 65-tool catalog fails visibly with no partial provider tools; `isError` and image-only modality produce distinct visible failures; search call carries exactly `query`/`response_length`. Four actual-binary tests PASS. |

## Hardening after review

Independent adversarial review of the first T37 delivery found four concrete
gaps; all are fixed with regressions:

1. shutdown/reap errors were discarded and `QuitReason::JoinError` counted as
   success — now typed failures propagate to `WorkerGuard::join` and a non-zero
   exit, and disarm happens only after confirmed teardown;
2. a dropped turn/reload future could leave the single-flight flag stuck —
   replaced by an RAII lease plus owned-task cleanup;
3. `tools/list_changed` relisted every server and could lose a notification
   arriving during relist — now claim-per-server with restore on failure;
4. aggregate caps did not bound spawned children or shutdown time — enabled
   servers are capped at 8 before the first spawn and closing a generation is
   bounded by a 10 s generation-wide budget.

## Verification

Final targeted and full workspace commands with exact counts are in
`checks.md`: runtime 31, remote MCP 20, stdio MCP 10, actual MCP 7, core 17,
adapter unit 124, plus the full workspace suites; all exit 0. Workspace
all-target clippy `-D warnings`, fmt check, locked build, `oc --help`,
progress/docs structural checks and `git diff --check` pass. Three pre-existing
external harnesses remain ignored/NOT RUN, not PASS.

## Supported differences and remaining scope

- Raw HTTP body/stdin frame preallocation bounds and overall retained
  output/queue lifetime stay explicitly with T40; retained MCP call text is
  capped at 1 MiB and catalog metadata at 1 MiB per generation.
- User-facing reload/location/config controls are T39; T37 evidence is the
  adapter-owned generation lifecycle plus actual-binary restart/disable.
- Live OpenProxy MCP qualification remains with T27 after T42. No live probe
  was performed and no `READY` claim is made.
- `MAX_MCP_SERVERS = 8` is a declared bounded-profile limit; exceeding it is an
  actionable error, not a hidden truncation.

Next: T38 — remaining shell/webfetch findings, reproduced offline first.
Audit fragments were merged exactly once and are not rerun.
