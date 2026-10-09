# T44 — bounded live standalone Shell output

## Result and scope

Direct-user Shell cards now show actual partial stdout/stderr while the owned
process is still running, before its terminal effect. The existing capture,
ShellJobs query, ShellChanged routing and transcript owner supply the projection.
Whole frozen R1–R6/VIS01–VIS45/T44 remains ACTIVE/NOT_PASS. Separate native lower
panels still do not qualify the combined Subagents/Shell/Terminals VIS39 composer.

## Existing owners and invariants

`shell/jobs/output.rs::StreamCapture` retains a2048-byte display tail of already
admitted, normalized/redacted text in serialized safe publication order. This is
not OS emission-order attribution. It is populated before generated transport
labels: literal `[stdout]`/`[stderr]` payload remains data, never parsed metadata.
Secret/UTF-8 carries remain private until the existing admission releases them.
The same capture supplies separate960-byte stdout/stderr previews and real byte
counts under ingress→stdout→stderr lock order, without cloning64KiB windows.
At most eight owned jobs add16KiB of display-tail retention; existing capture,
resource, queue and active-job limits are unchanged.

`ShellJob.output` is an optional bounded Presentation, populated by the existing
`Jobs::running` only for direct-user provenance. Model jobs and stored identity
snapshots leave it unset. It conveys no exit, signal, file or control authority.
The existing Core query signature, inventory owner and ShellChanged scheduling
are unchanged: no new timer, store, registry, executor or query owner.

`HistoryWindow` updates only a known started standalone input with matching source
session and operation and empty model-turn identity. Invalid/terminal facts are
ignored; equal facts are idempotent. Inventory cannot add history, replace a
command, touch model/assistant parts or revive a terminal/hidden admission. The
existing bounded window and transcript invalidation account for the preview.
Receipt/history attachment reconciles already-arrived inventory, preserving
draft, paging and original message identity. Both actual UI event consumers use
the existing apply-shell-jobs path.

The direct-user renderer displays the owner's ordered safe body with donor muted
stream styling, without synthetic stderr labels. Model Shell's header-only
streaming path and existing terminal status logic remain unchanged. Final outcome
keeps frozen typed stream/exit facts after the supervisor moves drain buffers;
only its display body comes from the retained ordered projection. RAW notice,
provider text, ToolCallResult graph, permissions and execution provenance are not
rewritten. No env/.opencode/GOAL/acceptance/Cargo.lock/auth changes or paid API calls.

## Current source association

Four independently read final built locks share base
`cf834d28c613bbdb80fe4590a3fb982044d0d4a9`, tree
`f44dfa63c26b4c37e10c9c667fb67eea94e99477`, and:

- Dirty diff: `7ea9465ab2dc404359d959cfe561ef06137828ce794c5b9dd31e73b822d17f2e`.
- Source manifest: `9074e8537b5e73754df01084690d247106dc1d90753396efe62c744177838503`.
- Actual debug ELF: `a3e47aa62eba7387cc6f275e77bd9ffd5607d8a1de92576c5ba0a5743b1a3dc0`.
- Running original v2.0.12: `2670273ff17da96f85c5826ced57aa1b368754fa`.

Each uses `--build-oc true` / `cargo build --locked`. Later factual docs do not
rewrite immutable captures; reviewed Git records actual commit/push delivery.

## Actual process and full-frame evidence

[user-shell-live-output-001/](user-shell-live-output-001/)80×24 and
[user-shell-live-output-002/](user-shell-live-output-002/)120×40 each retain12 stages,
**10 EQUAL/14 DIFFERENT** full grid/PNG comparisons. Running with real partial
stdout AND stderr, completed, recalled-normal, explicitly entered recalled-mode
and second-completed are exact full styled-cell/PNG/cursor pairs. Running grids
check1920/4800 cells, zero differences and equal cursors; paired actual PNGs were
physically viewed. This is not whole-episode/T44 pixel PASS.

The ordinary real command publishes stdout, then stderr after0.2s, then holds nine
seconds for the first command/three later before its effect and terminal stdout.
Original/native use their ordinary executors, not a test execution substitute.
The finite hold is screenshot/read-only-witness margin, not a production guard
change, forced refresh or fake phase. Native running witness has an Active
72-byte framed resource with27-byte stdout and27-byte stderr, a started NULL-turn
operation, no provider call/LLM turn, exact committed deduplicated input history
and no effect yet. Both sides complete exactly two explicit effects, zero
Responses/title/MCP calls and no restart replay. Native final effect is44 bytes /
two lines with two distinct operations and process-owned SQLite URI-modeRO
history-before-effect witnesses. Original RAW is not decoded.

All remaining Home/mode/input/restart differences remain unmasked, including real
product versions and independently selected examples. No crop, realignment, seed,
version substitution or source golden. Frozen recalled full-parts/mode/Mini
exclusion at amendment2235–2238 remains unchanged.

Independent same-source regressions:

- [Normal tools](user-shell-live-output-tool-regression-001/): both27 stages,
  six fixture requests/four effects/three MCP calls/17-byte model Shell effect;
  no replay across switch/reopen/restart.4 EQUAL/50 DIFFERENT comparisons plus four
  native-only authorized capture-detail snapshots.
- [Temporal](user-shell-live-output-temporal-001/): default/WebGL/collapsed/supported,
  six owners per side, all zero cycles appropriate to DEFAULT, no phantom/lost
  caret/draft/replay; all160 comparisons DIFFERENT. Independent
  [matched audit](user-shell-live-output-temporal-matched-001/) validates144 opaque
  PNGs,72 phase pairs and17,694,720 full-raster pixels per side. Result:
  QUALIFIED_CURSOR_BEHAVIOR_ONLY/pixel NOT_PASS; DEFAULT is not blink proof.

## Current checks

Fmt-check, strict locked workspace all-target Clippy `-D warnings`, full locked
workspace tests, locked/ordinary debug and locked release builds, both ELF help,
actual release startup19 and GET-only discovery8 cases PASS.
**1778 passed/0 failed/11 unchanged opt-in ignores across46 workspace results**.
Full current log:
`/home/opencode/.local/share/opencode/tool-output/tool_11eb0c13d001WplADVWdt6fZrY`.

Two new scenarios prove genuine event-driven held-process typed streams, split
secret redaction before effect, terminal delivery/reopen/no replay, and UI
inventory-before-attachment, source/operation/model isolation, idempotence,
unchanged draft and no terminal revival. Existing capture tests cover deferred
secret carry/publication order, UTF-8, multi-MiB preview bounds, IO loss and
secret-admission failure. A first terminal test exposed moved drain buffers;
implementation now preserves frozen Outcome stream facts rather than weakening
the assertion. Private-owner delegation avoids test-only API widening.

Python unittest discovery47, Node capture/tool-preview/user-Shell syntax, two
fixture py_compile, source whitespace/docs/progress and advisory code-size PASS.
Changed-file footprint16 files/14752 physical lines/557322 bytes; no new >5k file.
Application facade5425 is unchanged. Independent source review and full reviewed
diff found no remaining concrete risk in this slice; no test/guard was disabled.

Additional unchanged actual-binary gates:

- VIS31 idle1.005451113s/zero CPU ticks;165/250Hz p50/p95/max2.952/14.313/15.174ms
  and5.739/7.142/7.466ms. Burst17.914/28.706/29.382ms and
  16.016/29.175/33.117ms, queue peaks74/96/no lag/settled live0, below100ms.
- Equal-view archive0→3000 RSS69768→70092KiB/PSS67453→67881KiB, retained22896
  bytes/152 rows/cache7517 unchanged, queues6/5/no lag/children0,
  frames88/90, elapsed2506/2353ms.
- AUD32 archive8→3000 peak RSS55616→56608KiB (+992KiB), peak PSS52977→53969KiB,
  end PSS52977→53937KiB, children0/threads8/Db942080→152944640 bytes;
  unchanged64MiB guard.

## Remaining contract

Next replace competing lower panels with the combined VIS39 composer using their
existing inventory/control owners, preserving source identities and authority.
Then every remaining frozen outcome and genuine full-frame difference, followed
by final current-source R6/V09. This qualified live-output slice does not finish T44.
