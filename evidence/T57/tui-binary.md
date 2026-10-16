# T57 — rebuilt native TUI authentication consumer

Date: 2026-10-06. Base: `cd141cba9`. Partial AUTH01/AUTH05 actual-binary
consumer evidence; not real issuer/device/model authorization or AUTH06 PASS.

## Native boundary

`oc/tests/auth_tui.rs` runs the normal rebuilt debug ELF through isolated actual
controlling PTYs. `support/auth_tui.py` also accepts an explicit release-binary
path. There is no production auth/endpoint override or special application mode.
The fixture uses a fresh allowlist environment, synthetic credential canaries,
its own data root/project/desktop launcher and a static non-dispatched provider.

The same protected SQLite owner is initialized through `oc auth list`. A bounded
source-qualified **synthetic** public OpenAI cache row is then seeded in that
isolated existing prefs store, with Go disabled, to exercise native shared cached
metadata without public requests. It is not evidence of current models.dev rows
or authenticated model discovery. Native validation/projection is not bypassed.

## Observed behavior

- Real `/connect` command and provider-row mouse activation lead to native OpenAI
  methods in Browser/Device/Key order, using the shared Core projection.
- Two actual masked key forms acknowledge stored accounts before the exact OpenAI
  picker. Ordinary and same-ID-collision rows are visible; the fixture provider's
  selection prefs remain byte-identical. No picker/model choice is committed.
- Add-first, sorted existing account rows, explicit activation, rename and two-step
  confirmed removal operate on actual SQLite receipts. The surviving account is
  retained across native TUI restart and read by the same CLI metadata command.
- The current view's Unicode draft survives auth/account/picker navigation. Keys
  never occur in captured host output, composer or history. No cross-process Home
  draft persistence is inferred from the account restart test.
- Native Browser creates its **real default-port owned listener** and presents
  Waiting/open/copy. There is no automatic desktop launch or implicit clipboard.
  Explicit `o` calls a fixture-owned sanitized launcher; a second failed opening
  retains the same pending attempt and manual-copy feedback.
- Explicit `c` emits the actual bounded OSC52 request. Its URL is decoded only in
  memory to verify fixed authority, PKCE/state and the owned redirect port; no URL,
  state, code, callback body or private screen is published.
- Esc unmount joins cancellation and closes the owned socket without creating an
  account. A fresh native attempt accepts a wrong-state callback, returns safe
  refusal HTML, closes and emits fixed failure feedback. No issuer exchange occurs.
- Esc/unmount followed by normal idle-Home native quit joins cleanup and restores
  original termios and the alternate screen. Listener identity is obtained only
  from the fixture process's descriptors and `/proc/net/tcp`, not foreign probes.

SQLite contains zero sessions, turns and tool operations; only the acknowledged
surviving synthetic key remains. The fixture's generation listener receives zero
connections. Real Device authorization/token success and native OpenAI model
dispatch are deliberately **NOT_RUN**, not inferred from method labels or fake tests.

## Checks and material fixture corrections

Approved TMPDIR, `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2`, serial normal-stack
Cargo. Production is unchanged from the checked TUI slice.

| Command | Result |
| --- | --- |
| `cargo test --locked -p oc --test auth_tui -- --nocapture` | **1 passed / 0 failed**, current full native PTY scenario, 13.42s. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Exit0. |
| `cargo fmt --all -- --check`, `git diff --check` | Exit0. |

Initial fixture quit assertions assumed a configured exit key and then attempted
to open Commands after an extra idle Esc had already cleanly exited the native
app. Source inspection confirmed idle Esc is quit, whereas mounted-auth Esc only
unmounts: the fixture now uses those actual transitions and drains shutdown output.
It also explicitly seeds a draft in each process rather than assuming unrelated
cross-process Home draft restoration. Original no-commit, acknowledgement, cleanup,
privacy and zero-generation assertions remain; no production behavior, timeout,
threshold, baseline or failing test was changed to obtain green.

## Next

Rebuild release and run current CLI/TUI fixtures plus mandatory workspace gates;
then qualify dedicated owner-operated browser/device login and ordinary OpenAI
test-key requests within the frozen AUTH06 envelope. Those prerequisites have not
been supplied. T44/VIS45 stays independently PAUSED; this is neither paired visual
parity nor overall T57/product completion.
