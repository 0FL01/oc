# VIS46 — structured producer/progress facet

## Result

**PRODUCER_FACET_QUALIFIED; execute UI, PTY and paired qualification OPEN.**
This is an additive step of the existing VIS46 contract, not full VIS46/T44 PASS.

The existing execute invocation now supplies optional typed presentation facts:
captured redacted code, host calls in admission order, running/completed/error
states, structured input previews, limited flags, terminal error and real elapsed
time. Tool paths come from the captured catalog, not code reconstruction or a
dot-split wire name. The ordinary display path omits the `tools` root like the
pinned donor; bracket notation preserves literal path segments.

Snapshots retain at most16 calls,4096 code bytes and512 bytes each for a tool
path/serialized input. The three typed output sections share the existing2048
byte preview budget. These are **presentation bounds**, not new execution stops.
The actual escaped operation/presentation journal envelope still fits the
existing32768 byte limit, with honest reduction flags. An irreducible foreign
operation envelope uses the existing optional-facts fallback rather than turning
a settled effect into Unknown. RAW input/output and record guards are unchanged.

`ToolCallProgress` uses the existing callback/broadcast path and coalesces
disposable replacements for the same operation. It creates no durable operation,
approval or effect and does not trigger a durable TurnPresentation refresh.
The terminal snapshot is fitted before both persistence and finish publication.
Legacy records without execute facts retain their existing Generic fallback.

Warnings/Logs come from the interpreter's typed outcome sections. Arbitrary
returned strings containing those words are not parsed as section boundaries.
Fetch previews contain only URL/method, never request headers/bodies or response
bodies. Captured secret and structured-key redaction happen before preview bounds.

## Checks

- Current core presentation tests:2 PASS (legacy serde, receiving/terminal facts,
  bounds, section ordering, family exclusivity, retention).
- Adapter `execute_` owner tests:3 PASS, including actual escaped-envelope fitting
  and structured redaction/truncation; existing RAW remains unchanged.
- Existing confined-interpreter suite:22 PASS with exact output assertions.
- Current runtime Code Mode suite:9 PASS. A real Python MCP peer handles two
  parallel additions and a real local HTTP fetch; progress shows running and
  completed calls, while final event/DB/reopen facts are identical. One execute
  operation and exactly two provider requests remain unchanged through reopen.
  Literal path identities, nested Ask failure and denied-catalog no-effect guards
  retain their existing assertions. Cancellation after one real marker effect
  preserves the completed nested-call fact, one `M\n` effect, one provider request
  and a valid durable cancelled parent snapshot.
- TUI progress owner regression:1 PASS; unknown progress cannot mint a card and
  late progress cannot resurrect a completed operation or alter the draft/caret.
- Strict `cargo clippy --locked --workspace --all-targets -- -D warnings`: PASS
  after correcting actual compile/lint findings, with no suppression.
- `cargo fmt --all` and reviewed diff checks: PASS.

Logs: `.local/evidence/t44-vis46-core-02.log`, `producer-unit-02`,
`interpreter-01`, `runtime-05`, `live-progress-01`, `clippy-04` with the same
`t44-vis46-` prefix. Commands use approved TMPDIR, three Cargo jobs/two test threads.
All peers/credentials are synthetic; T65's published0.2.0 artifacts are unchanged.

## Risks

No Execute renderer/dialog is implemented yet. Current progress plumbing does
not qualify visual rows, mouse targeting, focus, clipboard or live dialog behavior.
No current Execute PTY/cell/PNG comparison or whole-workspace PASS is claimed.
Existing Shell success omission, Explored-group anchors and cursor requirements
remain frozen. Earlier fixture/compile/lint failures were diagnosed and corrected,
not accepted as product behavior or external blockers.

## Next

Implement the typed Execute renderer and selection-safe parent/child targets,
then the live bounded Code/Output dialog through existing modal/syntax/clipboard
owners. Qualify actual progress, failure, cancellation, replay/reopen/restart and
full narrow/wide original/native frames; retain all remaining T44 requirements.
