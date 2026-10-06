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
