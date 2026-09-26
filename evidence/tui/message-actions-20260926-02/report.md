# T44 Message Actions and native conversation boundary probe

Executed 2026-09-26 against actual PTYs, 120×40 Reader/tools profile. Overall
runner exit **1**: native Copy failed and whole-frame comparisons differ.
No TUI parity PASS is claimed. Attempt `message-actions-20260926-01` is preserved;
its native executable predated the current implementation and failed popup/undo.

## Commands and provenance

- `node --check scripts/tui_capture/capture.mjs`: exit 0.
- `node --check scripts/tui_capture/message_actions.mjs`: exit 0.
- Python `ast.parse` of `scripts/tui_capture/bridge.py`: exit 0.
- `node scripts/tui_capture/check_frontend.mjs`: exit 0.
- `node scripts/tui_capture/check_capture_geometry.mjs`: exit 0.
- After informing the parent, `CARGO_BUILD_JOBS=3 TMPDIR=/home/opencode/.cache/opencode-tmp/opencode cargo build --locked`: exit 0, 16.36 s.
- Both real capture runs: exit 1. Python bridge exit 0 on each side, both runs.

Second capture command:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --message-actions true \
  --output /home/opencode/ai/oc/evidence/tui/message-actions-20260926-02
```

HEAD `1f759dda28b5cae7e2f5b960659a091a32a1c9a3`, with parent's uncommitted Rust
implementation. Native binary SHA-256:
`6adcd8339f8a294d73241ec012220d77026056969f828c4f505b1b802d0b1153`.
The runner records an existing binary (no internal Cargo invocation); the separate
build above supplies the observed build association. `source-manifest.json`
includes untracked Rust modules; `capture.lock.json` locks source, binary, tooling,
fixture and frame hashes. This exercised the bounded loopback Responses fixture
through actual application provider calls, not the external paid OpenProxy.

## Whole-frame results (no masks)

All grids check 4,800 styled cells plus cursor. PNGs are 1,011×640 (647,040 pixels).
Coordinates below are zero-based inclusive grid bounding boxes.

| Stage | Different cells | Cursor differs | Different PNG pixels | Comparator exits grid/PNG |
|---|---:|---|---:|---|
| Home | 27 | no | 1,247 | 1 / 1 |
| Completed provider turn | 0 | no | 0 | 0 / 0 |
| User hover | 0 | no | 0 | 0 / 0 |
| Message Actions popup | 14 | no | 850 | 1 / 1 |
| Copy hover | 14 | no | 850 | 1 / 1 |
| Copy after click | 4,800 | yes | 646,043 | 1 / 1 |

Popup and Copy-hover grid differences are exclusively symbols at
`[59,16,72,16]`: original Revert description is `undo messages and file changes`;
native is `undo messages and restore prompt`. This reflects the approved
conversation-only contract, but remains an actual failed unmasked comparison.
Their PNG difference bbox is `[286,208,613,271]`; PNG also observes actual rendered
glyph styling. Copy-after grid differs throughout `[0,0,119,39]`: original closes
the dimmed popup, native leaves it open. Full reports accompany every paired frame.

## Actual mouse actions and Copy failure

Both sides complete two transcript requests (real `read` roundtrip) and one title
request, all valid and completed, before popup input. The real user-block hover
and click target is x=9,y=3; SGR coordinates are column 10,row 4. Popup options are
uniquely found in each side's own grid: Jump to (34,15), Revert (34,16), Copy
(34,17), Fork (34,18). Copy hover requires a styled-row change before capture.
`inputs.json` preserves exact motion/down/up bytes; no source-drawn frame is used.

Original Copy closes the popup and emits an OSC 52 payload exactly equal to
`Какие тебе тулы доступны?`. No Copy toast is observed. Native Copy remains in the
popup and emits no OSC 52 payload in the bounded post-click interval; status is
`FAILED_COPY_EFFECT`. Requests/completions stay 3/3, invalid requests zero.
OSC 52 proves PTY transport only; system clipboard destination is unverified.

Read-only diagnosis: `crates/oc/src/tui_cmd.rs:1183–1194` calls
`report_copy_request` before applying the mouse outcome; `CopyMessage` handling
at 1374–1386 then invokes `copy_message_text`, which only queues `pending_copy`
(`crates/oc-tui/src/app.rs:2302–2310`). That intent branch does not close the
popup or drain the newly queued copy. Suggested engineering next step: ensure the
actual message-copy intent writes clipboard transport and closes on success,
then rerun under a fresh attempt path. Rust was not edited by this probe.

## Native-only approved stepwise undo/redo

Independently after the failed Copy observation, Escape dismisses the popup.
A second real turn completes another read roundtrip. Baseline becomes exactly
5 requests / 5 completions (four transcript, one title), invalid zero.

| Command | First answer | Second answer | Restored draft | Cursor (x,y) |
|---|---|---|---|---|
| `/undo` #1 | visible | absent | `Second same-session spacing check?` | (39,34) |
| `/undo` #2 | absent | absent | `Какие тебе тулы доступны?` | (30,34) |
| `/redo` #1 | visible | absent | empty | (5,34) |
| `/redo` #2 | visible | visible | empty | (5,34) |

Each stage reaches a stable full styled grid/cursor and saves PNG/VT/render/text.
Counts stay **5/5 at every stage**, proving zero extra provider calls for these
commands. Native conversation observation status is PASS. Frames are explicitly
`NATIVE_ONLY_APPROVED_DIVERGENCE`, without an original stepwise-redo comparator.
This bounded probe does not establish restart, branch replacement, DCP restoration
or filesystem immutability; those require the owner's separate runtime gates.
