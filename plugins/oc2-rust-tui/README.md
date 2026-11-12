# Rust TUI plugin — candidate integration

The package root contains the actual plugin; `test/fixture` is the separate,
minimal lifecycle probe. **The agreed Linux/x64 candidate slice passed offline
and isolated functional qualification, but release-artifact availability is
currently blocked**; see the
[execution contract](../../docs/goals/2026-11-12-rust-tui-opencode-plugin.md) for exact
gates, artifacts and limits. Do not enable the test package or modify a live owner to try
the candidate. Stock 2.2.7 has a demonstrated shutdown defect; the required
awaited lifecycle candidate is under [upstream/](upstream/README.md), not in an
official release by assertion.

## Entry point and boundaries

After building the hook-enabled host and the matching `oc2`, put both binary
directories on PATH. An isolated project configuration can then use:

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "plugins": ["/absolute/path/to/oc/plugins/oc2-rust-tui"]
}
```

Run ordinary `opencode`. Presence of this composite server/TUI package enables
the Rust child; absence leaves stock TUI. A missing hook, incompatible child or
failed handshake leaves stock TUI usable. The child command is private, not a
replacement shell wrapper. No npm publication is needed for the local package.

The TypeScript service remains the sole sessions, tools, models, configuration,
permissions and storage owner. The plugin uses its captured public client over
bounded private IPC, without exporting credentials or discovering another
service. The Rust child enters before native data-root/owner initialization and
reuses the existing UI loop, frame backend and display-rate scheduling.

The first slice includes sessions, process-local tabs, opaque-cursor history,
text/reasoning/tool presentation, prompt admission, explicit execution interrupt
and pending-input removal, model/agent selection, permissions and compatible
question forms. Picker selection is a draft; blank Enter or prompt submission
commits it. Create and prompt are separate effects: a known prompt refusal keeps
the created session and editable draft. Prompt ACK means durable inbox admission,
not execution start. Native atomic fresh submission is unchanged.

`/exit` returns to stock without interrupting backend work. Unknown effects,
stream loss and resource overflow fail-stop rather than replaying requests.
Unsupported forms remain pending and return visibly to stock. Native config,
auth, DCP, MCP controls, shell/terminal execution, attachments and general forms
are not implemented by this bridge. Use stock UI for those operations. There is
no public stock composer draft setter, so automatic draft transfer on failure is
not promised. Tabs are not shared or persisted as backend workspace state.

No low-refresh renderer or polling paint loop replaces the existing Rust path.
Code reuse and synthetic PTY tests are not perceptual or high-Hz qualification;
complete feature/performance parity remains a separate contract discussion.

## Functional qualification

Build the actual Rust binary with `CARGO_BUILD_JOBS=4 cargo build -p oc2 --bin oc2
--locked` (or the final release gate). Use the host built by the pinned recipe in
[upstream/](upstream/README.md), **not stock**, for the functional probes:

```sh
SCREEN_DIR=$(mktemp -d "${TMPDIR:-/home/opencode/.cache/opencode-tmp/opencode}/oc2-screen-XXXXXX")
rustc --edition=2024 -D warnings plugins/oc2-rust-tui/test/fixture/screen.rs \
  -o "$SCREEN_DIR/screen"
for case in basic tools selection history controls unsupported lifecycle parent-loss; do
  python3 plugins/oc2-rust-tui/test/functional.py \
    --opencode "$CANDIDATE" --oc2 target/debug/oc2 \
    --screen "$SCREEN_DIR/screen" --case "$case" || exit
done
```

The harness uses a fresh UID-preserving user/PID/mount/network namespace, only
loopback, an owned synthetic HTTP provider, private HOME/XDG and no product
capabilities. Namespace setup raises only its own loopback and drops all
capabilities before launching the product. No user's service/config or paid
generation is used. Public API imports seed paged history and a child-session
form; they do not write storage directly. Captures, synthetic provider bodies and
results stay outside the worktree under TMPDIR. Rendered text uses the existing
native PTY suite's bounded screen decoder, not raw adjacency in incremental VT
cell diffs; this adds no runtime dependency. A parent-SIGKILL probe tests
child-side IPC-loss cleanup, not an impossible parent callback guarantee.

## Result on stock 2.2.7

The isolated bare `opencode` fixture proves composite-plugin discovery, one
quiescent Rust child on tty 0–2 plus IPC 3/4, handshake before suspend/activation,
captured-client location, initial session routing, and ordinary cleanup. The
child links the existing `oc_tui::terminal::TerminalGuard`; it does not initialize
the native runtime, storage or providers. No model calls are made.

The blocking case is **a hard child failure during SIGHUP shutdown**:

1. Stock destroys its suspended renderer before awaiting plugin cleanup.
2. The child exits without unwinding, so its terminal guard cannot restore.
3. A `destroy` listener can start stop/join but cannot delay destruction.
4. The renderer is unusable; there is no guaranteed final terminal reset.
   Termios is restored, but the cursor remains hidden.

`shutdown-crash` is intentionally still a failing qualification command. It uses
the same `process::exit(7)` fault as ordinary `crash`, without an extra late tty
write. Ordinary `crash` succeeds because the live host can resume and repaint.
This historical probe does not qualify the full Rust interface.

The smallest unlock preserving the chosen restoration owner is an **awaited
pre-destroy/terminal-handoff finalizer**, allowing child stop/join and a guaranteed
reset through the still-usable renderer before destruction. A notification alone
is insufficient. A complete plugin-owned reset is a separate contract decision,
not an implemented workaround. Never resume a destroyed renderer or mask the
failure by adding only a cursor-show escape.

## Reproduce safely

Prerequisites: non-root Linux, UID-preserving `bwrap` user/PID/network namespaces,
`/usr/sbin/ip`, Python 3, Rust/Cargo and cached crate dependencies. Run from the
repository root. Test projects/build output and runtime state stay outside the
worktree. Do not override `CARGO_TARGET_DIR` or use a user's config/service.

The verified official archive is
[`opencode-linux-x64.tar.gz` for v2.2.7](https://github.com/anomalyco/opencode/releases/download/v2.2.7/opencode-linux-x64.tar.gz):

```text
archive SHA256: 89a6cee227b54bc011179c528a5e63176032462b1a5e21723b616801cddc2ca7
ELF SHA256:     589253d36810f21217edd51545cd435b328cdc3cf4f451fbe0cce6f88afcf5e1
source tag SHA: fedc6f208aa2bd54c1019227214568083f6380ad
```

The source contract uses OpenTUI 0.8.1. The local `.local/opencode` 2.2.5 oracle
was not updated or used as the executable qualification target.

Build the minimal Rust terminal child (this is **not** `target/release/oc2`):

```sh
REPO=$(pwd)
PROBE=$(mktemp -d "${TMPDIR:?set an owned disk scratch directory}/oc2-plugin-child.XXXXXX")
cat > "$PROBE/Cargo.toml" <<EOF
[package]
name = "oc2-plugin-lifecycle-child"
version = "0.0.0"
edition = "2024"

[[bin]]
name = "terminal-child"
path = "$REPO/plugins/oc2-rust-tui/test/fixture/child.rs"

[dependencies]
oc-tui = { path = "$REPO/crates/oc-tui" }
libc = "0.2"
serde_json = "1"
EOF
CARGO_BUILD_JOBS=4 cargo build --offline --manifest-path "$PROBE/Cargo.toml"
CARGO_BUILD_JOBS=4 cargo clippy --offline --manifest-path "$PROBE/Cargo.toml" --bin terminal-child -- -D warnings
CHILD="$PROBE/target/debug/terminal-child"
STOCK=/absolute/path/to/the/verified/v2.2.7/opencode
```

The wrapper mounts neither user HOME nor runtime sockets, clears the child
environment, checks that the network namespace has only `lo` and no external
routes, and launches the unmodified stock binary normally. It writes bounded
synthetic `pty.bin`, `events.jsonl` and `result.json` under a fresh `$TMPDIR`
directory. `terminal_restored` measures termios; `cursor_shown` is independent.
The fixture's session seed creates only its own empty session, without generation.

Passing cases on the checked revision:

```sh
for case in normal crash disabled disable reload sighup slow-sighup sigterm sigkill resize missing bad-hello silent session; do
  python3 plugins/oc2-rust-tui/test/lifecycle.py --opencode "$STOCK" --child "$CHILD" --case "$case" || exit
done
```

The preserved stock-only failure (exit 1, `AssertionError: cursor left hidden`):

```sh
python3 plugins/oc2-rust-tui/test/lifecycle.py --opencode "$STOCK" --child "$CHILD" --case shutdown-crash
```

Normal/crash and pre-handoff failures also check actual stock keyboard/paint
responsiveness with Ctrl+P → Commands. PID absence is checked before namespace
teardown, so namespace cleanup cannot manufacture an orphan-cleanup PASS.
This evidence does not qualify a functional TS-backend/Rust-UI adapter, all
future OpenCode versions, perceptual parity, or full high-refresh performance.
