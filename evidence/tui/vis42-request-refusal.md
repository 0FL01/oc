# T44 — concise typed request refusal (2026-10-07)

Status: **FUNCTIONAL_SLICE_QUALIFIED; T44 ACTIVE; PIXEL GATES OPEN**.
Git base: `e2120e418e2d266b70cc5756d2f4754c4abb534a`.
Source: VIS42's concise actionable refusal for a genuinely unavailable submitted
request, preserving exact safe details, drafts, redaction and pre-effect admission.

## Change

Both synchronous submission failure and asynchronous pre-acceptance rejection now
use one private typed UI helper. `CoreError::ProviderUnavailable` and
`CoreError::Diagnostic` with Provider/Selection kind display the finite code and
owner action, for example `Request unavailable: model_unavailable · /models`.
No service/source/field identities are expanded in the brief toast.

One existing TuiState-owned optional fact retains only the last captured safe
refusal. Settings exposes a read-only `Last request — refused` row; Enter, copy
and unsent investigation use its exact typed diagnostic. It is not fabricated
current provider readiness: a newer binding/catalog can become ready independently.
Accepted submission, changed Location or a different error clears the fact.

Other CoreError families preserve the prior full visible error. In particular,
literal `model_unavailable` inside Application prose is not classified or hidden.
Core error types/Display, headless output, runtime admission, selection identity,
RAW messages, model context and side-effect ownership are unchanged. No new API,
store/schema, registry or warning-text parser.

## Evidence

The new private service-owner test was RED against the first-slice source: full
opaque diagnostic text appeared instead of the exact short summary. It now uses
a real bounded CoreApp channel and SubmitFresh acknowledgements to prove both
typed families, no session/history/effects before acceptance, preserved draft,
exact safe detail/copy/investigation, separation from newer current chrome,
Location clearing, arbitrary-prose visibility and accepted retry clearing with
one user row/no duplicate acknowledgement.

Actual R4A inherited-selection and blocked-primary fixtures initially exposed
that native selection rejection is `Diagnostic(Selection)`, not only
`ProviderUnavailable`. The helper was corrected by type, not string matching.
Both actual R4A cases and the four UI07 readiness/credential/authorization cases
retain request/effect counters, saved identities, no fallback/replay, privacy,
headless nonzero/full-safe details and explicit repair assertions.

The actual unsent-refusal Settings route also qualifies safe detail while the
draft survives. First Escape clears the Settings filter; observing its real
empty Search state precedes the second Escape which dismisses it. No focus/cancel
policy, watchdog or test timeout changed.

An old UI07 post-reload background `forbidden` expectation was superseded by the
first slice's clean-dialogue contract. It now waits for actual terminal reload
feedback, then inspects that same code in Settings before the unchanged actual
submitted-refusal/counter/privacy assertions. Immediate navigation while the
owned reload job was still pending was correctly refused; no production guard
was weakened to make the fixture pass.

## Current final gates

Approved TMPDIR, serial Cargo, build jobs 3/test threads 2, normal stacks:

- `cargo fmt --all -- --check` — PASS.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --locked --workspace --no-fail-fast` — **1721/0/11**;
  independently summed from 46 workspace result records, unchanged opt-in ignores.
  Includes TUI451, binary97, PTY T3952, MCP41, recovery startup4,
  adapter library668/0/1, runtime120 and subagents39.
- `cargo build --locked` / `cargo build --locked --release` — PASS.
- Actual release startup/discovery scripts — PASS: safe TUI details, fatal and
  headless channels, no Responses/one discovery GET, private fields withheld,
  native terminal restoration and trust/storage/Location boundaries.
- Documentation/journal/diff checks — PASS (structure, not pixel evidence).

Current approved-cache log `t44-typed-request-refusal-final2.log` ends
`T44_TYPED_REQUEST_REFUSAL_CURRENT_ALL_GATES_PASS`. Failed first-chain output is
retained separately; only the current completed chain proves this source.

## Boundary and next

This closes the typed functional refusal facet, not full VIS42/T44. The prior
actual paired service attempt005 remains associated with its recorded first-slice
source and all22 full comparisons DIFFERENT; it is not new-source pixel evidence.
Continue the ordered structured body/guidance/ref/capture and Generic/MCP
VIS16/VIS17 slice, then remaining styled/temporal integration and every frozen
VIS01–VIS45/SAFETY/R6/V09 outcome. No finish/native-golden/whole READY claim.
AUTH06 stays deferred; `.opencode/`, user configuration and existing live budgets
are untouched. No new live generation was used.
