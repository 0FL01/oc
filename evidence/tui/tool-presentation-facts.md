# T44 — structured tool presentation prerequisite

## Scope and source association

This receipt belongs to the commit adding it, based on `59614a348`
(`fix(catalog): bound public wire retention during metadata refresh`). It qualifies
the minimal T50/R10 producer/query/event seam required by T44 VIS16/VIS17. The
renderer, Generic/MCP toggle and full paired styled-cell/PNG/cursor outcomes are
not qualified here. T44 remains ACTIVE with its complete frozen R1–R6/VIS01–45
contract; there is no whole-task or READY claim.

## Result and preserved boundaries

- `oc-core/src/tool_output.rs` carries one bounded safe body, its known original
  byte count, projection/guidance facts, optional current capture extent/state/ref
  and optional native shell streams/status. Missing facts mean legacy/unknown,
  not complete capture. Body and combined shell streams are each at most 2,048
  bytes; the reference is at most 4,096 bytes; serialized journal record is capped
  at 32,768 bytes. These are existing preview/serving bounds, not bigger payload
  budgets.
- Common preparation records the body before generated model guidance. Native
  MCP and shell owners supply their own facts; shell exit/signal/timeout/cancel
  are separate from capture loss and viewing loss. Literal marker-like payloads
  remain data. No keyword/regex stripping of tool/LLM content is introduced.
- Facts commit in the same existing Db transaction as the operation outcome and
  turn log, including instruction-bearing reads. They use the existing append-only
  event journal, not a second store, schema, lifecycle or event bus. RAW output and
  provider text, typed controls/media and recorded effects are unchanged.
- Bounded queries validate the optional shape. Legacy/future-invalid facts fall
  back to unknown provenance. Current projection removes expired/missing/foreign
  descriptor references; a reference is not a promise of filesystem readability.
  Expiry is explicit. Forks copy presentation without becoming file
  owners or gaining model filesystem authority. No cold output is loaded by the
  new projection.
- Presentation bytes share the unchanged per-turn serving cap and TUI resident
  history budget. Existing Core events, operation DTOs, application routing and
  live/durable cards transport the facts; rendering is a subsequent slice.
- Existing redaction, resource permissions, no-follow identity, generation/source,
  16 MiB capture, shared 2 GiB quota, seven-day retention, cancellation and
  no-unknown-effect-replay guarantees remain intact.

## Nearest checks

Existing output/storage/shell scenarios were extended, not disabled:

- MCP joined text and structured envelopes preserve body versus guidance, literal
  marker payload, error/control fields and independent media admission.
- Invalid presentation rolls back the outcome/log transaction. A real oversized
  question preserves one cold payload, bounded facts, reopen, fork-without-grant,
  expiry and unchanged RAW.
- The bounded turn query charges additive presentation bytes and honestly omits
  facts when its original budget is exhausted.
- A real short interrupted shell keeps exit zero/effect success separate from
  incomplete capture, including restart; no fabricated lost suffix or guidance
  enters its body.
- The existing actual-binary stdio MCP restart scenario verifies exactly one
  durable presentation for the actual operation after two runs, redacted body,
  no binary media in that record, unchanged wire graph/config and one tools/call.
  Targeted current test: **1 passed, 0 failed**, 1.00 s.

## Actual normal ELF checks

`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode` and
`PYTHONDONTWRITEBYTECODE=1` were used for the owned offline fixtures. Both normal
`target/debug/oc` and `target/release/oc` ran
`evidence/T50/native_tool_output_common.py <binary> --case <case>`.
Every case below passed on both binaries; their before/after binary digests were
unchanged. No paid endpoint, real HOME or authoring configuration was used.

| Case | Fake provider requests (main/auxiliary) | Actual retained capture | Facts / independent effects |
| --- | --- | --- | --- |
| chain | 18 (13/5) | 3,740,084 B, Complete | 7 facts; distant read/grep, restart/move/Deny; exactly one shell effect, producer reaped |
| head | 18 (13/5) | 425,108 B, Complete | 7 facts; skill head, distant read/grep, restart/move/Deny |
| quota | 3 (2/1) | 0 B, Quota | 1 fact; failed tool and one logging-failure fact |
| fault_effect | 3 (2/1) | 70,010 B, Interrupted descriptor | 1 fact; publication failure has no usable path, actual exit 17 and exactly one effect |
| shell_cap | 3 (2/1) | 16,777,216 B, ArtifactCap | 1 fact; output exceeded 20 MiB, exit 0 separate from failed capture, one effect, producer reaped |
| shell_quota | 3 (2/1) | 0 B, Quota | 1 fact; actual exit 0, one effect, producer reaped |
| shell_interrupted | 6 (4/2) | 22 B, Interrupted | 2 facts; known exit 0, short body without projection loss, readable prefix after restart, no replay, producer reaped |

All cases retain the original assertions for served text, known-secret redaction,
artifact extent/state, registered path access, unsupported native-root/glob/write
boundaries, immutable move provenance, request counts and effects. All owned HTTP
handler threads/processes joined before exact fixture cleanup. Producer capture
status does not overwrite the real execution outcome.

## Gates and qualification boundary

The preceding current production tree passed fmt, strict locked workspace
all-target Clippy, `cargo test --locked --workspace --no-fail-fast`
(**1,723 passed / 0 failed / 11 unchanged ignored**, independently summed 46
records), normal debug/release builds and both `oc --help` commands in
`t44-public-catalog-stream-current2.log`. The current MCP fact assertions above
were then added and passed on the actual binary.

Final current-source chain `t44-tool-presentation-current.log` ends with
`T44_TOOL_PRESENTATION_CURRENT_ALL_GATES_PASS`:

- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: PASS, 2.24 s.
- `cargo test --locked --workspace --no-fail-fast`: **1,723 passed / 0 failed /
  11 unchanged ignored**; independently summed and asserted across all 46
  result records. It includes the extended actual MCP case, unchanged AUD32,
  52 T39 PTYs, all 34 T42 cases, 41 MCP application cases, 120 adapter runtime
  and 39 subagent cases, 451 TUI and 97 binary tests.
- `cargo build --locked` / `cargo build --locked --release`: PASS, 0.78/0.53 s;
  both actual `oc --help` invocations PASS.
- Docs/progress structure checks and `git diff --check`: PASS, not product
  qualification. Physical-size advisory confirms net +3/+46 lines in the
  existing 5,413/5,220-line application/turn files; rationale and natural seams
  are recorded in `docs/CODE_MAP.md`, without cosmetic splitting or new owners.

The unchanged AUD32 active-context/archive gate and release startup/discovery
checks are documented in [public-catalog-memory.md](public-catalog-memory.md).
No acceptance threshold, workload, timeout, baseline, ignore, privacy assertion
or failure classifier was weakened. Original live allowance 24/24, Go 13/24
and AUTH06 deferral are unchanged.

This receipt does not claim Generic/MCP collapse/expand, hidden generated UI
markers, compact status rendering, full TOOL21 requalification or T44 pixel
parity. Those consumers and every remaining frozen T44 outcome still require
their own actual effects and running-original/native evidence.
