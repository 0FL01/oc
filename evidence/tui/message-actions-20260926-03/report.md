# T44 post-fix paired Message Actions

Actual-executable 120×40 run, 2026-09-26. Runner exit **1** (unmasked frame
differences); interaction checks PASS on both sides. No Cargo invoked in this
campaign: parent supplied the latest source-built final-gate binary.

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --message-actions true \
  --output /home/opencode/ai/oc/evidence/tui/message-actions-20260926-03
```

Native binary SHA-256 `5ceb43799e1f5aac9e60184e360e03e0307ad374828d49aad153781fb8407081`.
Captured source manifest, executable/runner/fixture hashes, exact inputs,
protocol and full styled grid/cursor/PNG/VT/render geometry are in this attempt.
The provider is a bounded loopback Responses fixture exercised through real
application requests and a real read-tool roundtrip, not paid OpenProxy.

## Functional observations

- User click opens Message Actions; actual unique options at x=34, y=15–18.
- Real Copy hover changes the painted option row. Both Copy clicks close the
  popup and emit exact OSC 52 payload `Какие тебе тулы доступны?`.
- Native shows `Copied to clipboard`; original has no observed toast.
- Counts remain 3 requests / 3 completions / 0 invalid during these actions.
- Native two `/undo` plus two `/redo`: PASS. Undo restores second then first
  prompts, hides second then first answers; redo restores one then both answers
  with empty draft. Counts stay 5/5/0 throughout, zero extra provider calls.
- Native-only approved divergence frames are preserved without comparing an
  original stepwise-redo behavior. System clipboard destination is unverified.

## Exact unmasked comparisons

Each grid compares all 4,800 styled cells and cursor; each PNG compares 647,040
pixels (1,011×640). Every paired grid and PNG comparator exits **1**, not PASS.

| Stage | Different cells | Different PNG pixels |
|---|---:|---:|
| Home | 6 | 298 |
| Completed turn | 3 | 175 |
| User hover | 3 | 175 |
| Message Actions popup | 14 | 850 |
| Copy hover | 14 | 850 |
| Copy after | 87 | 10,975 |

All paired cursors equal. Completed/hover grid bbox `[37,10,39,10]` is real elapsed
time digits. Popup/Copy-hover bbox `[59,16,72,16]` is Revert description:
original `undo messages and file changes`, native `undo messages and restore prompt`.
Copy-after bbox `[37,1,117,10]` includes native success toast plus elapsed time.
Home bbox `[112,38,117,38]` includes the displayed version difference. Nothing is
masked or normalized. Native Copy regression observed in -02 is resolved here;
full pixel parity remains unverified. Historical -01/-02 are preserved.

Final tooling checks: JS syntax for both runners, xterm frontend check, capture
geometry check and `git diff --check -- scripts/tui_capture/capture.mjs`: exit 0.
Extra Fork/Revert findings are in fresh attempts -04 and -05, with -05 report.
