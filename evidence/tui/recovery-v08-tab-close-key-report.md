# T44 — paired keyboard and Commands tab close

Pinned OpenCode v2.0.12 registers `session.tab.close` with the `ctrl+x w`
binding in `packages/tui/src/app.tsx:805-809` and
`packages/tui/src/config/keybind.ts:41,118`. It closes the active retained tab
or the synthetic Home slot without deleting the durable session. Native `oc`
now exposes **Close tab** in Commands, accepts the two-key chord and a native
`/close-tab` alias, and sends all three through the existing application-owned,
CAS-backed `CloseTab` intent. Bare Home and busy/read-only routes cannot perform
an unsupported close. A refused owner operation does not discard the draft or
modal. After an accepted keyboard close, the real recorded pointer is re-hit-
tested for *visual hover only*: it does not synthesize a mouse click or close
hold. Production and test-only runner change: `ec7eb4e`.

The real-binary PTY+SQLite tests exercise both chord and Ctrl+P search/Enter,
busy refusal, no extra root/provider call, and reopening the closed durable
root through Sessions and restart. TUI tests check availability, correct active
slot, draft/modal survival and the add-control hover colors after keyboard
close. This does not implement other missing upstream Commands actions.

## Immutable paired attempts

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --tab-click true --tab-close-key true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-tab-close-key-20260924-qualified
```

Both `-01` and `-qualified` were retained, rather than replacing a failed
comparison. The original executable is SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
source commit `2670273ff17da96f85c5826ced57aa1b368754fa`. Qualified native
executable SHA-256 is
`ac1947d32e63618296866a39bf14f785e5c8458677e9ab34c794be9426a51c1c`;
fixture SHA-256 is
`a816400f909a6b252465ece82967a72b0e41c42eec01e27ad72423d2f1557fbf`,
and shared terminal-profile ID is
`75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`.
Both independent PTYs report `provider_contract=true`,
`TAB_INTERACTION_CHECKS_PASS`, `TAB_CLOSE_KEY_CHECKS_PASS`, and unchanged 3
provider requests/completions (2 transcript, 1 title) through close. The
runner saves actual PTY input, protocol, VT, styled cells, PNG and comparators.

The first attempt had 5 differing styled cells in `keyboard-close-after`:
three hover styles on ` + ` and two genuine elapsed-time digits. After the
hover fix the qualified frame differs in **2/4800** styled cells, only elapsed
digits at x38–39,y10; PNG differs in **122/647040** pixels. The comparator
correctly exits 1 `DIFFERENT`, not parity. In the independent 160×48 Commands
capture `evidence/tui/recovery-v04/dialogs-close-key-20260924-01/`, native
has a real Close tab entry with `ctrl+x w`, but the Commands frame still differs
in **300/7680** cells (other actual palette entries and elapsed text). Those
captures are diagnostic; neither establishes VIS08 nor full VIS01–24 parity.

## Checks and remaining work

On final code, `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked
--workspace --no-fail-fast --quiet` passed (225 TUI tests, 23 `pty_t42`, 35
binary unit tests, existing opt-in live ignores). `cargo fmt --all -- --check`,
`cargo clippy --locked --workspace --all-targets -- -D warnings`,
`cargo build --locked`, `node --check scripts/tui_capture/capture.mjs`,
`python3 scripts/check_docs.py`, `python3 scripts/progress.py check`, and
`git diff --check` passed. Independent diff review found no actionable P1/P2
issues. No synthetic timer/version or unsupported palette action was added.

T44 stays active: a matching interaction and near-matching frame cannot close
the mandatory dialog, model, error/replay, resource, and pixel gates. The
existing untracked `.opencode/` was not accessed or staged.
