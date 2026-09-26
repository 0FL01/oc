# VIS10 user-message hover — paired diagnostic

Command (exit 1, full-frame comparisons DIFFERENT):

```sh
node scripts/tui_capture/capture.mjs --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --geometry true --sample tools --sidebar hide --agent-profile true --columns 120 --rows 40 --user-hover true --output /home/opencode/ai/oc/evidence/tui/message-hover-20260926-01
```

`cargo build --locked` succeeded inside the runner; executable SHA-256, source manifest,
runner hash, fixture, profile and build command are in `capture.lock.json`. Both
isolated PTYs completed two valid transcript requests (including real `read`
result) and one title request, with no invalid request. Both painted the unique
user prompt at `(5,3)` (zero-based), bordered by `┃` at x=2 for rows 2–4.
The runner sent **only** SGR mouse motion to the interior cell `(17,3)`
(PTY column 18, row 4; `inputs.json`), with no mouse press or fabricated menu.
`user-hover-checks.json` on both sides records changed user-block styled cells:
the background of padded cells changes from `#141414` to `#1e1e1e`;
both normal and hovered frames are stable and provider request counts stay fixed.
No `Message Actions` dialog appeared, as expected before click.

Unmasked full 120×40 captures: `upstream/` and `oc/` each contain
`message-hover-normal` and `message-hover-hovered` `.cells.json`, `.png`,
`.txt`, `.vt` and `.render.json`, alongside `session-wide-completed`.
The normal **and** hover cross-binary comparisons report **DIFFERENT** in
both grid and PNG modes. Each grid has 2 differing cells at y=10, x=37 and
x=39 (assistant elapsed-time digits: 116ms original, 212ms native); each PNG
has 110 differing pixels. See `message-hover-{normal,hovered}.{grid,png}-diff.json`.
This verifies the hover transition on both binaries, not whole-frame pixel parity
or Message Actions behavior. The externally built original, shared Chromium/xterm
frontend and fixture-backed native executable remain identified in the lock.
