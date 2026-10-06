# T53 masked account UI slice

Base: `d4abea7cc`. Partial R1/R5 evidence, not full GO05 or T53 PASS.

## Implemented

- Native `/connect` (Go) and `/accounts` (current catalog provider) route through
  `PanelIntent::ProviderAccounts` to the existing `CoreApp`/SQLite owner.
- Separate ephemeral label/key fields; API key painting is masked. No secret
  accessor, serialization, Debug value, composer, Editor undo, clipboard or
  history path. The key moves once into redacted `KeyInput`, then the view is
  empty before awaiting ACK. Cancel/rejection and panel replacement clear it.
- Safe metadata list, activation, rename, and a second explicit `d` confirmation
  for removal. Busy-owner refusal remains in the application, not a UI bypass.
- Successful add ACK opens the current catalog picker without committing a
  model. The configless Go path is qualified; cross-provider routing and the
  Go-filtered picker from a different current provider remain the next slice.
- Newly arriving priority approval/question overlays clear the account form
  and consume its triggering input instead of sending secret paste elsewhere.

## Verification

With approved TMPDIR, Cargo jobs=3 and test threads=2:

```text
cargo test --locked -p oc-tui -p oc
  native binary unit/integration/PTY targets passed; TUI 437 passed / 1 failed
cargo test --locked -p oc-tui
  final 439 passed / 0 failed / 0 ignored
cargo test --locked -p oc --test pty_t39 accounts::go05_
  1 passed / 0 failed (actual native PTY)
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
  PASS
```

The first affected-crate run found the existing `/a` completion expectation
excluded the newly required `accounts` command. Updated its exact expected list,
not dispatch/search behavior. Compile errors were confined to exact owner paths,
test fixture constructor/signatures and exhaustive panel matches, then corrected.

Actual PTY starts without config using a fresh public metadata cache and isolated
HOME. It verifies cancelled key not stored, successful masked key add, picker ACK
ordering, no automatic model, rename/restart persistence, confirmed final removal,
terminal restoration and zero generation/configured-discovery requests. Complete
raw captured terminal output excludes both synthetic keys. Existing binary suite
also passed all 49 T39 tests and other native targets before the completion-only
fixture correction. Final focused PTY was rerun after priority input isolation.

No real generation calls, secret reads, new dependencies, ignored-test changes or
deadline changes. Remaining T53 gates include full provider-qualified selection,
final offline workspace, bounded real Go qualification and factual task finish.

## Admitted provider views foundation

Base: `d3c3a26ae`. The local composition now retains independent connection views
for supported enabled providers, admitted from the same frozen source bytes and
descriptor-root authority. This does not publish another policy/Location, restart
MCP, open another Db, execute foreign packages or select a model. Optional invalid
views retain bounded safe diagnostics instead of blocking the healthy selection.
Each view resolves its own configured/scoped stored auth, never the current
connection's resolved key. Go uses the fixed Go authority and its public last-good
cache, independent of the selected custom provider.

`CoreApp::provider_catalog` exposes a safe cached view without changing selection.
Account commands now address independently admitted nonselected connections; their
metadata and credential-only refresh use the same owner. Querying another view
has no session/turn/tool or preference-selection effects. Same slash-containing ID
under Alpha, Beta and Go stays provider-qualified in separate snapshots. Source
edits do not change the frozen view until an admitted reload.

Checks (approved TMPDIR, jobs=3, test threads=2):

```text
cargo test --locked -p oc-adapters --lib go05_                  5 / 0 / 0
cargo test --locked -p oc-adapters --lib -p oc-core            613 + 32 / 0 / 0
cargo clippy --locked --workspace --all-targets -- -D warnings PASS
```

The first integration run found that assigning the selected-provider catalog job
to a nonselected Go view addressed the wrong generation entry. The new view reads
the shared last-good cache but does not commandeer that job. Explicit cold/stale
refresh and executable cross-provider selection are the following consumer slice;
this foundation does not claim full GO05 or change the currently selected wire.
No live generation, new dependencies, ignored-test or deadline changes.

## Independent public refresh and Go-filtered connect (2026-10-06)

Base: `50615d743`. Public refresh is no longer tied to the currently selected
connection. The application owns independent configured/public jobs, both joined
or aborted on retirement; the public source still has one shared cache/flight owner.
Only the Go view receives its outcome. Local read queries return immediately and
unchanged cached results do not cause an event/query feedback loop. Optional public
client-construction failure retains last-good metadata with a network failure fact.

Account success ACK now queries that provider's catalog and changes only the
picker browse scope. Committed selection/chrome/agent remain untouched. The picker
captures its own provider/location/generation and same slash ID under Go does not
borrow the custom connection's remembered variant; recent choices are qualified.
Actual binary PTY from a custom connection verifies Go-only rows, dismisses without
choosing, then `/accounts` still addresses the custom connection. No generation or
configured-discovery request occurs; the synthetic key is absent from raw terminal
output. Configless account cancel/add/rename/restart/removal remains green.

Checks (`TMPDIR` approved disk path, Cargo jobs 3/test threads 2):

```text
cargo test --locked -p oc-adapters --lib go05_independent_public_job   1 / 0 / 0
cargo test --locked -p oc-tui --lib go05_filtered_picker               1 / 0 / 0
cargo test --locked -p oc --test pty_t39 accounts::                    2 / 0 / 0
cargo test --locked -p oc --test pty_t39                              50 / 0 / 0
cargo test --locked --workspace                                      1643 / 0 / 10
cargo clippy --locked --workspace --all-targets -- -D warnings         PASS
cargo fmt --all -- --check                                           PASS
cargo build --locked; target/debug/oc --help                          PASS
git diff --check                                                     PASS
```

Final same-ID recent-choice review added provider comparisons; the full TUI suite
then passed 440/0/0 and strict workspace clippy/fmt/diff checks passed again. Ten
existing opt-in live/internal ignores are unchanged, not live PASS.

Experiments and corrections:

- A broad run exposed that the old reload guard treated any optional job as the
  selected configured discovery. It was reproduced unchanged, then narrowed to the
  actual selected/configured job. MCP pending-scope reload qualification passes.
- Startup public metadata is intentionally eligible for a custom connection too.
  Legacy configured-peer PTY fixtures accidentally fetched the real public source,
  mutating its cache and emitting unrelated metadata notifications during strict
  idle/prefs probes. They now explicitly disable Go in their configured-peer scope;
  Go-specific PTYs explicitly enable it and seed the source-qualified cache. The
  title-only event fixture likewise excludes public catalog jobs. No assertion,
  deadline, ignored test or product refresh was weakened.
- Test compile fixes used existing catalog fields/screen APIs rather than widening
  production interfaces. Clippy clone-on-Copy findings were corrected directly.

This is not full GO05/T53 PASS. Executable provider-qualified selection/persistence
and captured next-request switching are still the following owner integration.
No paid/live generation, credentials from live files, or dependencies were used.
