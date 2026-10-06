# T53 — unchosen local startup slice

Base: `b0c5fd031`. Partial GO05 implementation; not a T53/GO05 completion claim.

## Delivered contract

- Missing config or missing selected model admits the normal local composition,
  built-in agents, policy, storage and history owners. Go is a connection/catalog
  view, not an invented selected model. No model or variant is auto-selected.
- `CatalogSnapshot::selected_model()` exposes an optional provider-qualified
  composer reference. Existing presentation fields remain source-compatible:
  empty `model_id` denotes absence and is never committed/remembered by the TUI.
- Missing/disabled provider, unavailable model and missing endpoint are honest
  unavailable local state. Base transports without an endpoint cannot dispatch;
  independently admitted model transports still resolve their scoped accounts.
- Unchosen submit/headless fails through existing selection admission before
  session/turn/title/tool effects. Existing history remains readable on restart.
- Invalid policy, malformed endpoint/header shape, trust, storage/recovery and
  corrupt saved preferences remain fatal. Configured discovery authentication
  failures retain their typed cause instead of being replaced with model errors.
- Disabled Go does not attach/fetch even a warm public catalog.

## Checks

All Cargo commands used `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2` for tests and
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`.

- Targeted `go05_` application tests: 2 PASS; configless restart/history and
  optional provider/model/disabled/missing-endpoint pre-effect refusal.
- TUI `go05_`: 1 PASS; no committed/recent fabricated model.
- Actual binary configured-workspace headless refusal: 1 PASS; no root, turn,
  provider socket or external executable.
- Existing actual-binary startup PTY now qualifies configless Home, success exit
  and terminal restoration; malformed policy/config still refuses safely.
- `cargo test --locked --workspace`: **1633 PASS / 0 FAIL / 10 IGNORED**.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: PASS.
- `cargo fmt --all -- --check`, `cargo build --locked`, `target/debug/oc --help`,
  `git diff --check`: PASS.
- Final review added the missing-endpoint send fence, independent model auth and
  absent-model variant clearing. Full adapter lib: 610 PASS, then final targeted
  GO05 and strict workspace clippy/fmt/diff PASS.

## Experiments and corrections

Broad qualification caught an over-broad static-unavailable classification that
masked configured dynamic discovery `unauthorized`; narrowed it to preserve the
existing dynamic cause. The unchanged actual PTY regression passed afterward.
Old Location rollback fixtures treated missing models/config as fatal. T53
explicitly changes that contract: those fixtures now use malformed mandatory
policy and retain their original rollback, privacy and surviving-deck assertions.
The startup harness separately asserts configless success. No test was disabled,
deadline increased or policy validation weakened.

## Remaining

Masked connect/accounts owner commands and UI, provider-qualified cross-provider
selection, complete GO05 PTY flow, final contract audit and bounded GO06 live
remain pending. No paid/live generation was performed in this slice.
