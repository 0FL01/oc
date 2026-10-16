# T67 — owner live dev launcher `oc2-dev`

## Result

**DONE.** The owner's `~/.zshrc` (outside Git) gains `with-oc2-dev-secrets` and
`alias oc2-dev`. It loads `secrets.env` exactly like `oc2`, sets
`TMPDIR=~/.cache/opencode-tmp` and `exec`s `~/ai/oc/target/debug/oc2` from the
current directory. Config and the `~/.local/share/oc` store are shared with release
`oc2` (owner decision), so the two cannot run at once (`DataRootBusy`). `oc2-dev`
never builds; a missing binary prints the build hint and exits 1. Existing `oc`,
`oc2` and `opencode` wrappers are unchanged. `AGENTS.md` records the soft agent rule
(build `cargo build -p oc2 --bin oc2 --locked` when a live owner check makes sense,
keep `target/debug`, never launch `oc2-dev`, flag storage schema changes) and
updates the config line to D28/T66. No runtime code changed; `secrets.env` was not
read or printed.

## Checks

- `zsh -n ~/.zshrc` OK; interactive `type`: `oc2-dev`, `oc2`, `oc` aliases resolve.
- `cargo build -p oc2 --bin oc2 --locked` (debug) → `oc2-dev --version`: `oc2 0.2.0`.
- `oc2-dev models` from `~/ai/oc` on the real config: exit 0, no diagnostics,
  providers a6api, a6api-claude, llama.cpp, llama-lain, ludka2, openai, opencode-go.
- Bounded PTY `zsh -ic oc2-dev` (140×40, no prompt): no startup error frame, Build
  visible, ctrl+c natural exit 0.
- Concurrent `oc2` + `oc2-dev` lock behavior not exercised (would touch the owner's
  live store while in use).
