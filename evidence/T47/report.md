# T47 / R1 — unknown model limits and absent variant

Date: 2026-09-22. Base HEAD: `15719e976030a8fdaadd843ac13556de237f559d`.
Status: implemented and targeted offline checks pass; uncommitted working-tree
delivery to parent. No commit or push performed.

## Result

- Missing, partial, zero, or otherwise non-positive/unusable context/output
  metadata resolves to request-local native fallback caps for the missing fields.
  Positive supplied fields remain authoritative; optional positive input limits
  constrain admission too. Catalog entries are never filled with fallback values.
- `select_variant(None)` clears the overlay, including when standard/custom
  variants are enabled. Explicit variants still validate against the enabled
  allowlist. This follows upstream v2.0.12 `packages/core/src/model-resolver.ts`
  `withVariant`, lines 134–158, at `2670273ff17da96f85c5826ced57aa1b368754fa`.
- Admission reserves output and 1,024 tokens for uncertainty. It checks the
  assembled input, tool schemas, summaries, and accumulated tool results before
  every provider request. Exceeding the budget stops the request; completed tool
  results stay durable. No silent truncation/retry loop was introduced.
- Fallback use produces a warning with unknown fields, configured caps, resolved
  request budgets, and the statement that these are not discovered capacities.
  Primary warnings reach existing completion/failure events and headless stderr;
  they are appended alongside MCP warnings. Completed/failed child tool results
  retain their child warning using the existing text DTO. Early admission errors
  also identify fallback use.
- After reviewer follow-up, ancillary title generation also resolves the shared
  budget for its own selected model, admits its serialized developer/user input,
  and sends the resolved output. Fallback and skipped-title diagnostics append to
  the existing turn warnings. Rejected title input leaves the main turn completed.
- The application reports zero metadata limits as unknown via existing boolean
  DTO flags. Discovery implementation, DISC05, and the discovery oracle were not
  changed. No T44/TUI source, tests, evidence, goals, or task state was modified.

## Native configuration and defaults

The selected provider's options accept this native-only field in ordinary
global/Location JSON or JSONC configuration:

```json
{
  "provider": {
    "ludka2": {
      "options": {
        "nativeFallbackLimits": {
          "context": 32768,
          "output": 4096
        }
      }
    }
  }
}
```

This fragment illustrates the additional option; retain the provider's existing
connection options. It is local runtime policy, not a remote metadata field and
not a Responses request extension. Existing provider replacement/precedence rules
apply. The two defaults are 32,768 estimated context tokens and 4,096 output
tokens. Both fields are positive integers; unknown keys and invalid types are
rejected. For the selected provider, context must exceed output + 1,024. Omitted
fields use the defaults; an invalid resulting pair is rejected.

`TurnParams.max_output == 0` now requests the configured native output default.
Primary and child application turns use this default rather than treating model
capacity as requested output. An explicit positive requested output is clamped to
the known output capacity, or the fallback output cap when unknown. The output
field therefore doubles as the default request for known models; it is only a
hard fallback output cap when output metadata is unknown.

Title generation retains its explicit request of 256 output tokens, clamped by
the selected title model's known output or configured fallback output cap. For
unknown output with `nativeFallbackLimits: {"context":8192,"output":64}`, both
the primary default request and title request send at most 64 output tokens.
Title input uses the same safety reserve and positive model input/context bounds;
its fallback warning is prefixed `title generation:` and admission rejection is
reported as `title generation skipped:` through existing completion warnings.

Input admission is:
`min(positive model.input if present, effective context - output budget) - 1024`,
using saturating arithmetic and rejecting budgets that cannot reserve output and
the margin. With both limits unknown and defaults selected, input is capped at
27,648 estimated tokens. Native caps bound local requests; they do not assert that
an arbitrary upstream model can accept that workload.

## Verification

Initial implementation commands below exited 0; the title-path follow-up and its
separate verification are recorded below this table:

| Command | Actual result |
| --- | --- |
| `cargo test --locked -p oc-adapters --lib models::tests` | 5 passed |
| `cargo test --locked -p oc-adapters --lib config::tests::native_fallback` | 1 passed |
| `cargo test --locked -p oc-adapters --lib` | 157 passed, including unchanged `discovery::tests::disc05_limits_and_deletion` and the full discovery unit suite |
| `cargo test --locked -p oc-adapters --test runtime --test subagent` | Final run: 37 runtime + 11 subagent tests passed |
| `cargo test --locked -p oc-adapters --test subagent --test context_bounds --test e2e_offline` | Earlier run: 10 subagent + 3 context-bounds + 3 offline E2E tests passed; subagent suite subsequently expanded and passed as above |
| `cargo test --locked -p oc --test application` | 2 actual-binary tests passed; unknown metadata case captured `max_output_tokens:256`, absent reasoning overlay, and visible stderr warning from configured 16,384/256 caps |
| `cargo clippy --locked -p oc-adapters -p oc --all-targets -- -D warnings` | Passed after final runtime/child changes |
| `cargo fmt --all -- --check` | Passed |
| `git diff --check` | Passed before this report addition |

The runtime matrix covers absent limits, context-only, output-only (including
output capacity greater than fallback context), both-zero, and mixed positive/zero
limits. It asserts unchanged metadata and no extra HTTP request for oversized
input. A separate real runtime regression proves schema accounting before the
first HTTP request and admission failure after a large successful read, preserving
the durable result. The child fake-provider regression proves default caps and
warning propagation without auto reasoning.

During test construction one compile attempt failed from using a three-element
history tuple instead of its two-element API. Subsequent new-fixture assertions
exposed a wrong `filePath` tool argument (native read requires `path`), a short
fixture reduced by the existing read limit, and a check of messages rather than
durable tool operations. These fixtures were corrected; no tests were disabled or
production limits relaxed. The first runtime test run also emitted an unused
shutdown Result warning; it was corrected to assert shutdown success.

### Reviewer follow-up: ancillary title request

The initial actual-binary fake server captured only the primary request. Extending
it to serve and record the title request reproduced the gap:
`cargo test --locked -p oc --test application t47_binary_unknown_limits_warn_and_do_not_select_reasoning_variant -- --nocapture`
exited 101 with title `max_output_tokens` 256 instead of expected 64. The primary
request already sent 64. The production fix adds shared budget/admission calls in
the existing application title branch, using the pinned provider configuration.

The server now remains listening until the binary exits, so a forbidden title
request is observed rather than hidden by a closed endpoint. Four tests exercise
nine fixtures: existing known primary/title model, shared unknown model, four
separate title models (missing, context-only, zero, output-only), and three title
input rejections (large configured title body under fallback context, a smaller
positive title context, and a positive title input limit). Successful titles use
the exact selected model, no tools, two assembled messages, and no auto reasoning
overlay. Rejections send only the main request and preserve its successful stdout
with visible title warnings on stderr. A separate title with output 32 sends 32;
unknown-output titles send the configured 64.

| Follow-up command | Actual result |
| --- | --- |
| `cargo test --locked -p oc --test application -- --nocapture` | Exit 0; 4 passed, 0 failed/ignored; nine binary fixtures |
| `cargo clippy --locked -p oc-adapters --lib -- -D warnings` | Exit 0 |
| `cargo clippy --locked -p oc --test application -- -D warnings` | Exit 0 |
| `cargo fmt --all -- --check` | Exit 1; concurrent permission-agent additions in `crates/oc-adapters/tests/permissions.rs` and permission sections of `crates/oc-adapters/tests/runtime.rs` needed formatting; no title-file diff |
| `rustfmt --edition 2024 --check crates/oc-adapters/src/application.rs crates/oc/tests/application.rs` | Exit 0 |
| `git diff --check` | Exit 0 |

Only `crates/oc-adapters/src/application.rs`, `crates/oc/tests/application.rs`, and
this report were edited during the follow-up. Concurrent permission/application
hunks were preserved. No new public interface/configuration field or DTO change
was needed, and no T44 files were edited. Parent retains the concurrent workspace
formatting/integration check.

## Files and interfaces

- `crates/oc-adapters/src/models.rs`: fallback/budget types and shared admission;
  absent-variant semantics; unit tests. Existing `admit` signature retained.
- `crates/oc-adapters/src/config.rs`: additive
  `ProviderOptions.native_fallback_limits`, serde key `nativeFallbackLimits`,
  selected-provider validation, option recognition, tests. Permission parsing
  hunks were not edited.
- `crates/oc-adapters/src/application.rs`: default output request and unknown
  flags, plus shared title budget/admission and warnings; no public core/UI DTO
  field or type changes.
- `crates/oc-adapters/src/runtime.rs`: budget resolution, per-round admission,
  clamped wire output, warning retention, shared child default.
- `crates/oc-adapters/tests/runtime.rs`: fallback and bounded-request regressions.
- `crates/oc-adapters/tests/subagent.rs`: child fallback/warning wire regression.
- `crates/oc/tests/application.rs`: additional actual-binary fallback regression;
  existing AUD01 test still runs with its original known-limit scenario.
- `evidence/T47/report.md`: this factual report.

Rust embedders constructing `ProviderOptions` with explicit struct literals need
the new field or `..Default::default()`. Existing configuration stays compatible.
`TurnParams` and public core/application DTO shapes remain unchanged. Output
overflow now clamps according to CONTRACTS instead of returning `OverOutput`;
legacy public error variants are retained for source compatibility.

## Remaining risks

The estimator is the existing bytes/4 heuristic, not a universal tokenizer;
provider-side context errors remain possible, particularly for unusual content or
models with capacities below the chosen native caps. Configure lower caps when
needed. No live OpenProxy call was made in this slice. This report establishes
targeted offline R1 behavior, not final product qualification or completion of
the separate subagent/TUI workstreams. Parent owns registry/decision updates and
integration with the concurrent permission/MCP edits.
