# T57 — acknowledged application auth actions (partial R1/R2/R3)

Date: 2026-10-06. Reviewed base: `0bdc1b47b`. This checked slice connects the
qualified attempt/credential owners to Core/application, not a login UI, native
OAuth model request, WebSocket transport, real authorization or full T57 PASS.

## Owning boundaries

- `CoreApp::{auth_methods,authenticate}` use the existing inbox/oneshot receipt;
  scripted Core explicitly refuses both instead of simulating native success.
  Selectable typed methods are browser, headless, Key in that order; inherited
  environment credentials are not a selectable login method.
- `application/authentication.rs` admits only built-in OpenAI OAuth, delegates to
  the existing `OpenAiAttempts`, and returns its actual safe typed state. No session,
  model or provider request is needed to query/start/status/cancel authorization.
  A configured foreign `openai` endpoint retains its separate namespace/Key method,
  never receives subscription login material or becomes an OAuth issuer.
- One attempt owner belongs to the outer application lifetime, not a Location
  runtime. Status/cancel use the captured owned ID even after a Location reload or
  switch to a different endpoint. The outer worker always cancels/joins it after
  every return/failure, including failed publication/move paths; cleanup failure is
  not success. No detached auth bridge jobs, extra store or generic framework.
- Query dispatch is now async solely to await the real attempt cancel/join; all
  callers, including picker preparation and held model/title/compaction loops, keep
  acknowledged owner semantics. Auth controls do not cancel the active model turn.
- Accounts expose optional method ID, local credential identity/kind/label/active
  facts only. Access/refresh, routing account ID, verifier/state and callback code
  never enter account Debug/events/history. Unknown/provider-unready execution is
  not relabelled connected: OAuth still reports unsupported until the R4 wire slice.

## Direct evidence

Three application scenarios in `application/authentication_tests.rs`:

1. Native configuration-free/model-free auth methods/account read; safe method
   projection from an actual native SQLite OAuth row. Real default-port browser
   listener survives Location switch/reload, foreign OAuth admission refuses,
   original owned status/cancel still work and cancel acknowledgement closes the
   socket. Invalid-state callback returns safe HTML without its synthetic code,
   stores no new account; shutdown joins another pending listener. Native reopen
   retains exactly the original account. No issuer login request was sent.
2. Real held native Responses request against an isolated loopback peer. While its
   model stream is held, Core methods/start/status/account-read/cancel remain usable.
   Cancelling OAuth does not cancel or replace that request; after the barrier it
   finishes normally. Login labels/authorization never become conversation history.
3. Scripted Core refuses native auth methods/actions explicitly.

These complement the earlier fake issuer/device/exchange/refresh tests; they do not
replace rebuilt CLI/TUI consumers or dedicated live authorization.

## Current checks

All Cargo serial, approved TMPDIR, `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2`, normal
thread stacks. After the final owner-lifetime edit:

- `cargo test --locked -p oc-adapters --lib auth0`: **15 passed / 0 failed**.
- `cargo test --locked -p oc-adapters --lib`: **644 / 0 / 1 unchanged Go opt-in ignored**,
  100.12s. Full local log
  `/home/opencode/.local/share/opencode/tool-output/tool_1114f07c0001exVEzbJKaRN39u`.
- `cargo test --locked -p oc-core --lib`: **32 / 0**.
- `cargo test --locked -p oc-tui --lib`: **444 / 0**, including original masked key,
  account, picker, raw PTY focus and styling fixtures unchanged.
- Strict workspace all-target Clippy, fmt, journal and diff checks: exit 0.
- Size review: `application.rs` 5,309 lines (+48); one shared worker/generation owner
  still couples dispatch/transitions, with that natural seam documented in CODE_MAP.
  New scoped auth delegate 62 lines, separate application scenarios 364 lines.

One cleanup-boundary patch failed atomically on rustfmt context; exact lines were
re-read and applied without partial edits. No dependency, timeout/stack override,
test disable, baseline rewrite, security/validation weakening or live API call.

## Next

R4 admitted built-in OpenAI Key/OAuth catalog/bindings and native Responses WS/HTTP,
then shared actual CLI/TUI consumers. Dedicated AUTH06 owner-operated browser/device
confirmation and OpenAI test key remain separate prerequisites; no donor/user/runner
auth extraction. T44 stays PAUSED and owns independent paired VIS45; T53/T56 and
their evidence/campaigns unchanged. User-owned `.opencode/` unread/unstaged.
