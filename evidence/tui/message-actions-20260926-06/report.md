# T44 post-fork-context-fix actual-executable probe

Executed fresh immutable attempt on 2026-09-26. **All exercised interaction
observations pass**, including Revert of retained fork history. Overall runner
exit **1** because unmasked whole-frame comparisons differ. No final VIS10/V09
PASS assertion. Historical failed attempt -05 is preserved unchanged.

## Exact command and executable

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --message-actions true --message-fork-revert true \
  --output /home/opencode/ai/oc/evidence/tui/message-actions-20260926-06
```

Native SHA-256: `7a2b093017d0bb33fa816c57e475e13cf1d2a182517215f238abc36f802f12f9`.
Parent supplied the freshly source-built binary following reported full workspace
gate `tool_0dd439272001QBBRuIC8o4FhaP`; this probe did not invoke Cargo or independently
rerun that gate. Source/executable/runner/fixture hashes are in capture lock and
source manifest. Actual application provider calls use the bounded loopback
Responses fixture and genuine read-tool roundtrip, not paid OpenProxy.

## End-to-end observations

- Paired user hover and click open the actual Message Actions popup. Unique
  options and exact real SGR motion/down/up bytes are recorded per side.
- Copy succeeds on both sides: popup closes and exact prompt
  `Какие тебе тулы доступны?` is emitted through OSC 52. Native has success toast;
  original has no observed Copy toast. Counts stay 3/3/0.
- Native two `/undo` and two `/redo` pass. Undo restores second then first prompt
  and hides the corresponding turns. Redo restores one then both answers with
  empty draft. Counts stay **5 requests / 5 completions / 0 invalid**.
- Source Message Actions Revert on second user closes popup, hides second answer,
  retains first answer, restores second prompt. Subsequent source redo restores
  the turn. No provider requests are added.
- Fork on second user creates a new root tab, retains first answer, excludes
  selected second turn, restores second prompt. No provider requests are added.
- After clearing the fork draft, Revert on its retained first user now succeeds:
  popup closes, both answer markers are absent, original first prompt is restored
  with cursor (30,34). Counts remain **5/5/0**. Full grid/PNG/VT/render evidence:
  `oc/native-conversation-revert-after.*`.

Read-only SQLite inspection after teardown, connection `mode=ro`, confirms:

| Owner session | parent_id | conversation_points |
|---|---|---:|
| `s-tui-18d8d85bcaeec7ff-333847-0` | NULL | 2 |
| `fork-b35ca90ab0ae8ca9fbb50f8ba6a537a6` | NULL | 1 |

Fork point `fork-b35ca90ab0ae8ca9fbb50f8ba6a537a6:turn:0` has pre_seq=0,
post_seq=6, active=1, pre_context=`revision:8`, post_context=`revision:9`.
Source points have sequence intervals 0→2 and 2→4, contexts revision:0→1 and 1→2.
Thus the retained completed turn has an actual copied/rebased point, unlike -05.
After Revert, fork `conversation_state.upper_seq=0`; source upper_seq is NULL.
The post-Revert screenshot visibly contains the restored prompt and empty history.

## Exact whole-frame comparisons, no masks

Grids check all 4,800 styled cells plus cursor. PNGs compare 647,040 pixels,
1,011×640. All paired cursors match. Each paired grid/PNG comparator exits **1**.

| Stage | Different cells | Different PNG pixels | Grid bbox (zero-based inclusive) |
|---|---:|---:|---|
| Home | 6 | 298 | [112,38,117,38] |
| Completed turn | 3 | 176 | [37,10,39,10] |
| User hover | 3 | 176 | [37,10,39,10] |
| Popup | 14 | 850 | [59,16,72,16] |
| Copy hover | 14 | 850 | [59,16,72,16] |
| Copy after | 87 | 10,976 | [37,1,117,10] |

Differences include Home version, actual elapsed-time digits, Revert description
(`file changes` versus `restore prompt`) and native Copy success toast. Each
independent comparator JSON contains exact pixel bbox and grid field samples.
Native conversation/Fork/Revert captures are explicitly approved-divergence
observations, not original stepwise-redo or whole-frame parity assertions.

## Checks and boundaries

`node --check` for capture.mjs and message_actions.mjs, xterm frontend qualification,
and capture geometry qualification all exit 0. No runner source changes were
needed for this run. No Rust changes, Cargo, commits, pushes or .opencode access.
This bounded campaign establishes displayed behavior, copied point/root metadata
and provider-count invariants. It does not independently qualify historical DCP
restoration, restart, filesystem immutability or system clipboard destination.
