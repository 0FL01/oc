# T57 — shared mounted TUI authentication (checked consumer slice)

Date: 2026-10-06. Base: `2880a90ab`. Partial AUTH01/AUTH02/AUTH05 consumer proof;
rebuilt production TUI PTY, release and dedicated live qualification remain pending.
This is not T44/VIS45 paired presentation or full auth-segment parity.

## Source and ownership

Pinned `packages/tui/src/component/dialog-integration.tsx` supplies accounts/method
ordering, Add-first, already-active no-op, two-trigger removal/final disconnect,
Starting/Waiting, explicit open/copy, terminal failure toast/close and cancellation
on unmount. Existing Go/custom masked-key/account behavior remains unchanged.

`oc-tui/app/accounts.rs` extends the existing ephemeral account surface. Native
OpenAI existing rows open sorted accounts; an empty connection/Add opens the shared
Browser/Device/Key methods. Selecting Key uses the same bounded masked input, never
the composer/undo/copy path. Selecting OAuth captures only provider/method plus a
unique mount revision, not its URL/code/token. Account actions acknowledge before
changing views; selecting the active OpenAI account does not submit a write. Final
confirmed removal closes with disconnected feedback only after the empty receipt.

The single `oc/src/tui_cmd/auth_controls.rs` owner manages mounted Begin/status/
cancel work and an explicit bounded browser launcher through existing Core actions.
No second auth lifecycle, credential store, issuer client, task queue, perpetual
poller or generation/retry engine was added. Polling500ms is an event-loop deadline
only while the acknowledged attempt remains pending. Terminal states remove that
deadline. One owned job at a time is joined; unmount before Begin acknowledgement
waits for its actual ID and then cancels exactly that ID, rather than aborting Begin
and losing ownership. Shutdown joins pending work and reports cleanup failure.

Parked tab/Home/child auth surfaces immediately discard active URL/code; restoring
them cannot restart an old attempt. Esc, mouse close/backdrop and higher-priority
panels retire the current surface, preserving draft/chips/cursor and cancelling
only owned unsettled auth. Existing conversation/reload account-write/model guards
are unchanged; read/auth controls remain usable without waiting for a model turn.

## Active details and acknowledged model view

URL/instructions appear only in the active auth surface. `o` explicitly invokes the
already qualified fixed-issuer credential-free bounded desktop launcher; there is
no TUI automatic browser launch. `c` explicitly sends the current URL or structured
device code through the existing bounded OSC52 clipboard writer. Generic feedback
never includes that detail. Debug, PanelIntent, history and composer omit it.

`AuthAttempt.user_code` is populated atomically with device presentation and cleared
on completion/failure/cancel/expiry. Native code does not extract a code from prose.
Browser has no device code. Failure/expiry uses source close-and-toast behavior,
not a fabricated connected state or an endlessly polling failed panel.

OAuth completion first reads the matching durable local account receipt. Only then
does it fetch that exact provider's catalog and open a provider-filtered picker;
neither completion nor key storage commits a model. Missing/alien acknowledgement
does not fetch a catalog or claim connected. Server authorization is not inferred
from saved keys or public metadata.

Account/model browsing uses the same metadata-only credential/public-source
projection, without token exchange, refresh reservation or a new GET. Idle native
account reads publish fresh safe views; a held turn gets a fresh catalog projection
without changing its captured composition/request tokens. Near-expiry preview is
tested to leave `refresh_pending=0`; selected request preparation still owns refresh.

Mouse actions are paired painted-row hits, not arbitrary inventory indices. The
eight-row window excludes footer/blank rows; close requires matching press/release.

## Current checks

Approved TMPDIR, `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2`, serial normal-stack Cargo.
No dependency, timeout/stack increase, test disable, baseline or validation weakening.

| Command | Result |
| --- | --- |
| `cargo test --locked -p oc-adapters --lib auth0` | **36 passed / 0 failed**, including structured device code and current metadata-preview assertions. |
| `cargo test --locked -p oc-adapters --lib` | **665 passed / 0 failed / 1 unchanged Go opt-in ignored**, 110.48s; current backend source. |
| `cargo test --locked -p oc-tui --lib` | **446 passed / 0 failed**, current final account behavior, 27.80s. |
| `cargo test --locked -p oc --bin oc` | **97 passed / 0 failed**, current final UI owner, 4.36s. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Exit0, current final source. |
| `cargo fmt --all -- --check`, `git diff --check`, progress check | Exit0. |

Backend full-suite receipt: local
`/home/opencode/.cache/opencode-tmp/opencode/t57-tui-slice.log`; later UI/binary and
strict checks above were repeated after the private allocation/source review.
Two new UI scenarios and three controller scenarios prove ordered methods/accounts,
paired mouse/device choice, structured copy, masked key/expiry/cancel, mount fencing,
late-Begin cancellation, bounded polling and acknowledgement-before-picker/no commit.
They use owning projections/Core brokers, not a real issuer or production ELF login.
Existing Go/custom, dialogs, drafts, parked restoration and runtime tests remain green.

## Material failures and corrections

The initial larger inline account surface caused a normal-stack overflow in the
unchanged `prepared_patch_geometry_cache_busy_add_rejection_and_palette_are_real`
reopen scenario. A boxed-sync-future experiment did not fix it and was removed.
Own-fixture gdb with frame arguments disabled located restoration/history copying,
not OAuth polling. Keeping the same private account owner behind one `Box` removed
the increased inline restoration footprint. Existing default-stack regression and
full suites pass; no stack override, assertion change or API relaxation was used.
Clippy's large Outcome was fixed by boxing its catalog result only; replacement
allocation warnings by reusing the same account Box. A new mouse assertion initially
missed its test-only KeyModifiers qualification; corrected without source changes.
Source review also bounded painted-row hits and retained already-active/final-remove
behavior; the owning account scenario asserts these without altering old tests.

## Next and boundaries

Primary paths: existing account/input/dialog owner, typed Core auth projection,
new coarse `tui_cmd/auth_controls.rs` and separate tests; native metadata preview
stays in existing composition/account owners. No raw URL/code/callback dump is
published here. Next rebuilt normal TUI PTY and current debug/release/workspace,
then mandatory dedicated owner-operated browser/device/key authorization/request
receipts. T57 remains active; T44/VIS45 remains independently PAUSED.
