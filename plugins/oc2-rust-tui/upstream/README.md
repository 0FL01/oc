# Awaited terminal lifecycle candidate

These are reviewable upstream candidates, **not official releases or an installed
fork**. They are applied only to disposable source projects outside this worktree.
The [goal](../../../docs/goals/2026-11-12-rust-tui-opencode-plugin.md) records current
qualification; the [parent README](../README.md) preserves the unmodified stock
2.2.7 failure and the minimal Rust child build. A lifecycle probe is not the
integrated Rust interface or feature/perceptual parity.

## Owners and API

- `opencode.patch` reuses the TUI's existing lifecycle finalizer set. Managed
  signals and UI exit request teardown rather than destroying the renderer first.
  The host awaits registered resources before releasing the renderer. The terminal
  owner stops/joins its child and awaits the host renderer's `resume()` in its
  callback; the host must not resume a suspension owned by another consumer, such
  as the built-in external editor. New activation/registration is refused after shutdown starts.
- Public `context.lifecycle.onShutdown(asyncCallback)` registers synchronously,
  before handoff, and returns unregister; activation disposal also unregisters it.
  `context.lifecycle.shuttingDown` is a live, non-reactive shutdown flag. The same
  idempotent child stop/join promise must serve this callback and setup's returned
  reload/disable cleanup. Callbacks may run concurrently and must bound cleanup.
  Resume only the callback owner's completed suspension, never infer ownership
  from the renderer's global suspended state. This is not plugin-owned ANSI reset.
- `opentui.patch` normalizes externally enabled focus/mouse modes **only during
  native resume**, before reapplying host feature/mouse policy. Tracked host state
  cannot account for a crashed external TUI's `1004`/`1015` commands. It reuses the
  native control packet and existing mode setters; no frame path, scheduling,
  transport, public FFI, storage, or backend API changes are involved.
- SIGKILL, a failed native terminal, and third-party direct `renderer.destroy()`
  cannot promise awaited cleanup. A supplied handoff renderer must have been
  created with `exitSignals: []`; the host owns signal teardown. The actual 2.2.7
  CLI preflight closes its renderer and returns `undefined`, so it does not supply
  an independently signal-owning renderer to this app.

## Exact inputs

| Input | Pin / archive SHA256 |
| --- | --- |
| `anomalyco/opencode` | `fedc6f208aa2bd54c1019227214568083f6380ad` (v2.2.7) |
| GitHub source tar.gz | `9391b30118b53d8855e9e6972a27374df9e8193d189aa6b353b934078b9eb299` |
| `anomalyco/opentui` | `9a09079c0c7ad0fb2d55404f66a5c453204328dd` (npm 0.8.1 `gitHead`) |
| GitHub source tar.gz | `215969a66f7ca090405ef47d6ea350c2c11987084c688111d7a78fa9132b22fd` |
| Official Bun linux-x64 1.4.4 zip | `bf8ccbfb2b0b80159e0f9a6d15c55317add8077534dbc6130a3edb52fccb395c` |
| Official Zig x86_64-linux 0.17.0 tar.xz | `1cbe9df9f27e6b78d14ccbca43b6703a404ef79ef1c463de901d7f088d4e2026` |
| Official Node linux-x64 24.15.0 tar.xz (static checks) | `472655581fb851559730c48763e0c9d3bc25975c59d518003fc0849d3e4ba0f6` |

Source archives are from `https://codeload.github.com/<owner>/<repo>/tar.gz/<SHA>`;
Bun is from `https://github.com/oven-sh/bun/releases/download/bun-v1.4.4/bun-linux-x64.zip`,
Zig from `https://ziglang.org/download/0.17.0/zig-x86_64-linux-0.17.0.tar.xz`.
Node is from `https://nodejs.org/dist/v24.15.0/node-v24.15.0-linux-x64.tar.xz`,
verified against that directory's official `SHASUMS256.txt`.
Upstream context retains its MIT notices in `LICENSE.opencode` and
`LICENSE.opentui`. Preserve each source tree's vendor licenses as well.

## Reproduce in fresh disposable sources

Use the exact archives above, verify their digests, and extract into fresh
directories under approved TMPDIR. Do not apply to `.local/opencode`, installed
packages, the user's binary, or their service. Commands below are recipes, not a
claim that every gate passed; actual outcomes and artifact hashes are in the goal.

```sh
ROOT=/home/opencode/ai/oc
P="${TMPDIR:-/home/opencode/.cache/opencode-tmp}/opencode/rust-tui-plugin-v227"
CODE="$P/source"
UI="$P/opentui-source"
BUN="$P/bun-1.4.4/bun-linux-x64/bun"
ZIG="$P/zig-x86_64-linux-0.17.0"
# The standalone Bun zip contains the binary; bunx is its standard executable alias.
test -e "$(dirname "$BUN")/bunx" || ln -s bun "$(dirname "$BUN")/bunx"
mkdir -p "$P/build-home" "$P/bun-cache"
CPUS=$(python3 -I -c 'import os; print(",".join(map(str,sorted(os.sched_getaffinity(0))[:4])))')
build_env() {
  env -i HOME="$P/build-home" PATH="$(dirname "$BUN"):$ZIG:/usr/bin:/bin" \
    TMPDIR="${TMPDIR:-/home/opencode/.cache/opencode-tmp}" \
    BUN_INSTALL_CACHE_DIR="$P/bun-cache" GIT_CONFIG_NOSYSTEM=1 taskset -c "$CPUS" "$@"
}
# OpenTUI's native packager records the real Git HEAD. Supply metadata in this
# disposable source only; mixed reset populates the index without rewriting files.
(cd "$UI" && build_env git init -q && build_env git -c credential.helper= fetch \
  --depth=1 https://github.com/anomalyco/opentui.git 9a09079c0c7ad0fb2d55404f66a5c453204328dd && \
  build_env git reset --mixed FETCH_HEAD)
(cd "$CODE" && patch --batch --fuzz=0 -p1 < "$ROOT/plugins/oc2-rust-tui/upstream/opencode.patch")
(cd "$UI" && patch --batch --fuzz=0 -p1 < "$ROOT/plugins/oc2-rust-tui/upstream/opentui.patch")
(cd "$CODE" && build_env "$BUN" install --frozen-lockfile --ignore-scripts && \
  build_env "$BUN" run --cwd packages/core fix-node-pty)
(cd "$UI" && build_env "$BUN" install --frozen-lockfile --ignore-scripts)
(cd "$UI/packages/core" && build_env "$BUN" run test:native \
  '-Dtest-filter=Session resume normalizes external input modes' -j4)
(cd "$UI" && build_env "$BUN" run build)
```

The native build prepares the pinned vendored Zig dependencies and FFmpeg assets;
do not disable video or other renderer features to bypass compilation. Both
patches use ordinary unified diff with suppressed blank context prefixes. They
have been applied with zero fuzz to pristine pinned files and compared byte-for-
byte with the tested source. `patch` supports this format without whitespace
exceptions in the Rust repository.

Embed the **new native library**, not the old npm `.so`, into a fresh candidate:

```sh
SO="$CODE/node_modules/.bun/@opentui+core-linux-x64@0.8.1/node_modules/@opentui/core-linux-x64/libopentui.so"
cp "$SO" "$P/original-libopentui.so"
cp "$UI/packages/native/lib/x86_64-linux/libopentui.so" "$SO"
(cd "$CODE/packages/cli" && build_env env OPENCODE_CHANNEL=dev OPENCODE_VERSION=2.2.7 \
  "$BUN" run build --single --skip-install --skip-web-ui)
sha256sum "$SO" "$CODE/packages/cli/dist/cli-linux-x64/bin/opencode"
```

This is a Linux/x64 TUI-only dev candidate with an empty embedded Web UI, not a
qualified Web UI distribution. Keep its provenance distinct from the stock ELF.
After building the child as in the parent README, use the existing kernel-isolated
PTY gate; it neither mounts user credentials/service sockets nor performs LLM
generation:

```sh
for case in shutdown-crash normal crash disabled disable reload sighup slow-sighup \
  sigterm sigint sigkill resize missing bad-hello silent session stuck; do
  python3 "$ROOT/plugins/oc2-rust-tui/test/lifecycle.py" \
    --opencode "$CODE/packages/cli/dist/cli-linux-x64/bin/opencode" \
    --child "$P/rust-child/target/debug/terminal-child" --candidate --case "$case" || exit
done
```

`--candidate` requires the new public hook; omit it only when reproducing the
stock baseline with the verified stock ELF. The gate checks join-before-destroy,
termios, cursor, child input-mode reset commands and owned PID absence. Command
audits are not a universal terminal-emulator model. `stuck` requires SIGKILL
escalation of a child that ignores cooperative stop; parent SIGKILL instead tests
the separate existing IPC-loss child cleanup, not an unexecutable parent hook.

The actual-owner OpenCode regression is `packages/tui/test/app-lifecycle.test.tsx`
(filter `SIGHUP`): the held callback awaits join/resume, and destruction follows
its settlement even when it then rejects. Run it from `packages/tui` with Bun 1.4.4 in
the `SIGHUP` slice, which also checks that a no-plugin suspension owned by another
consumer is not resumed during shutdown. Use
the same loopback-only isolation as other synthetic TS runtime checks; do not use
`dev:live`. Scoped static checks are `bun run typecheck` from `packages/tui` and
`packages/plugin`; upstream canonical static check is root `bun run check`.
Use Node 24 for the latter, as in the pinned upstream setup action: oxlint's
TypeScript plugins cannot load under the host's Node 20. Put the disposable
`node-v24.15.0-linux-x64/bin` first in PATH for that check; no installed runtime
needs changing. The candidate ELF build itself uses the pinned Bun above.
