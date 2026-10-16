# VIS11 pinned-original leader oracle — 2026-09-27

Status: original behavior established; paired native qualification pending.
HEAD at start: `0a86cf67c1f3e620f5385911a3f51783fe747663`.
Pinned executable: `/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`.
SHA-256: `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

## Exact observed key semantics

Actual composer starts with `VIS11 full draft αβ caret-middle preserving every word`,
three Left keys move the caret before `ord`. Leader alone preserves the entire
visible draft, caret, and actual paste placeholder. Every run issued zero provider
requests; no draft was submitted.

| Sequence | Pinned original result |
|---|---|
| Ctrl+X, `z` | Pending ends; printable `z` falls through to editor, producing `wzord`; caret advances one cell. |
| Ctrl+X, Esc | Pending ends; draft/caret unchanged. |
| Ctrl+X, Backspace (DEL byte `7f`) | Pending ends; no deletion and no caret movement. |
| Ctrl+X, Ctrl+X | Pending ends; does not restart the timer; draft/caret unchanged. |
| Ctrl+X, `m` | Real Select model dialog opens; Esc restores full draft and caret. |
| Configured Ctrl+G, `p` | Real Commands dialog opens through admitted `command.palette.show: <leader>p`; dismissal restores draft/caret. |
| Leader in Commands modal, `z` | Modal retains focus; `z` enters modal search (cursor x34→35), never composer. Esc dismisses modal and restores composer. |
| Idle expiry | Restores normal colors without any further input; draft/caret unchanged. |

Successful fresh chords are sent together after the separately captured pending
sequence expires, avoiding capture overhead consuming the short timeout. Inputs
retain this distinction. Modal pending does not move the modal search caret.
The default-final modal dismissal frame was caught during transition and is
marked UNSTABLE_CAPTURE; stable configured/legacy-v1/precedence captures establish
the completed lifecycle. All failures remain in their original directories.

## Actual color roles

`configured-final/upstream/leader-normal` → `leader-invalid-pending`, full 120×40
styled grids and PNGs, contain the real model/provider/variant metadata:
`Build · MiMo-V2.6-Flash Free OpenCode Zen · fast`.

- Composer text and model: `#eeeeee` → `#808080` (text.muted).
- Prompt border and Build highlight: `#5c9cf5` → `#484848` (border.base).
- Provider retains `#808080`; variant `fast` retains `#f5a742`; they are outside
  the foreground-change cells. Background and all symbols remain unchanged.
  Actual paste chip retains foreground `#0a0a0a` on background `#f5a742`.
- Actual three-line bracketed paste creates `[Pasted ~3 lines]`. Normal, pending,
  restored frames preserve its text and caret. Mouse expansion after restoration
  exposes all three original `VIS11-PASTE-0..2` lines: full content is retained.

## Timeout and configuration paths

Final sequential observations, including transport/render latency:

| Run | Input configuration | Last pending / first restored observation (ms) |
|---|---|---|
| default-final | default Ctrl+X | 2024.9 / 2063.7 |
| configured-final | Ctrl+G; `leader.timeout=1200` | 1193.4 / 1231.6 |
| legacy-v1-final | actual v1 `tui.json`, Ctrl+G; `leader_timeout=1400`, no preexisting cli.json | 1410.5 / 1439.9 |
| precedence-final | Ctrl+G; v2 `leader.timeout=1800`, sibling legacy field 600 | 1798.4 / 1828.2 |

Important admission distinction: `legacy/` puts `leader_timeout=1400` in v2
`cli.json`; pinned original ignores it and expires around 2000ms. V1 migration
is the admitted legacy route: donor `packages/cli/src/config/migrate.ts:100–115,
195` converts `tui.json` leader_timeout into leader.timeout. U24 keymap lines
137–144 contain nested → legacy → 2000 fallback, but v2 config schema strips the
sibling legacy field. Do not claim that raw v2 CLI legacy field is effective.

## Evidence and checks

- `default/`: retained bridge NameError before launch, fixed by moving test-only
  fixture configuration after normal config construction.
- `default-02/`, `configured/`, `legacy/`, `precedence/`: initial observations,
  retain short-timeout/capture-overhead and unstable-frame failures.
- `*-final/`: sequential original captures, 26 complete grids/PNGs per run,
  raw VT, inputs, protocol/config/launch manifests and sealed hashes.
- Runner comparator exits 1 because native was intentionally omitted; missing
  pairs stay BLOCKED. `OBSERVED` is not visual parity PASS.
- `node --check` runner/probe/analyzer and Python AST syntax: PASS.
- Existing `node scripts/tui_capture/check_frontend.mjs`: PASS, real xterm RGB,
  blank styles, attributes, Unicode, caret and DSR.
- Independent analyzer checks sealed hashes, full-grid symbols/caret preservation,
  actual expanded paste text and request zero; see `original-analysis.json`.

Only scripts/evidence changed by this work. No Cargo/build was run. Native owner
must reuse the same effective lifecycle/deadline for chord and presentation,
schedule idle expiry redraw without constant production polling, then run fresh
paired evidence after Cargo ownership is released.
