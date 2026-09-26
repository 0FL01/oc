# T44 — conversation-only undo/redo and Message Actions implementation

Contract: approved 2026-09-26 amendment in `tui-recovery/T44_CONTRACT_AMENDMENT.md`.
This is a functional milestone, **not final VIS10/V09/T44 parity qualification**.

## Delivered behavior

- `/undo` moves back one accepted user/agent turn and restores its submitted prompt.
  `/redo` restores one saved turn without provider requests or tool execution.
  Message Actions Revert moves before the selected durable user-message identity.
- Saved points reference immutable, incrementally versioned DCP metadata rows:
  summaries/members, prune marks, tool projection exclusions and nudge state. Active
  paging, model comparison and provider history follow the same durable boundary.
  Accepted replacement input invalidates normal redo of the inactive tail without
  deleting archived messages, turns, tools or events. Restart preserves the point.
- Active execution is cancelled and cleaned up before boundary acknowledgement.
  Title work cannot commit obsolete results after movement. Terminal turn results
  and tool outcomes cannot be replaced by late settlement.
- Filesystem snapshots are disabled by default; singular/plural false configuration
  is accepted, true yields an explicit unsupported diagnostic. No filesystem scan,
  checkpoint, preimage journal or Git mutation runs for conversation actions.
  Tests confirm unchanged file bytes/modes, tracked/untracked files, index and HEAD.
- Painted user targets carry owner message IDs. Real Jump to/Copy/Revert/Fork support
  keyboard and mouse, suppress stale presses/selections and preserve failures.
  Copy reads exact owner text and invokes clipboard transport; OSC52 emission is
  proved, OS destination readback is not.
- Fork creates a Location-bound independent root with rebased preceding turn/tool
  history, genuine saved context points and the unsent selected prompt. Deck adoption
  is atomic. Refresh failure or orderly quit cannot discard a committed receipt;
  recovery refreshes queries rather than replaying mutation.

## Verification

Final serial workspace gate **PASS**: fmt; locked workspace tests, zero failures;
all-target Clippy `-D warnings`; locked build; capture JS/Python syntax and xterm
frontend; docs/progress and diff checks. Output:
`/home/opencode/.local/share/opencode/tool-output/tool_0dd439272001QBBRuIC8o4FhaP`.
Owner tests inspect actual next Responses input after undo/branch/fork with historical
DCP state, causal tool pairs and provider-lane isolation; request-free redo; restart;
admission rollback; immutable settlement; title cancellation; large legacy acceptance
migration without a new aggregate quota; and workspace/Git immutability. UI/binary
tests exercise clipboard transport, keyboard navigation, press/repaint identity,
accepted mutation followed by refresh failure and quit-time fork deck persistence.
No tests were disabled to qualify the diff.

Immutable paired `evidence/tui/message-actions-20260926-{01..06}/` preserves earlier
failed Copy and fork-history Revert attempts. Final
[`-06/report.md`](../tui/message-actions-20260926-06/report.md): paired Copy closes
the popup and emits the exact prompt; native performs two undo/two redo, source Revert,
real Fork and Revert on retained fork history. Provider counts stay **5 requests /
5 completions / 0 invalid** throughout actions. The fork is a root (`parent_id=NULL`),
has one rebased saved point, and Revert moves its boundary to 0. Full unmasked grid/PNG
comparisons remain DIFFERENT: completed/hover 3 cells; popup/Copy-hover 14 cells;
after-Copy 87 cells. Cursors match. Differences include independent timing, truthful
approved Revert wording and feedback. Runner exit 1 is preserved, not parity PASS.

## Explicit limitations

Legacy history without genuine saved context points reports an unavailable boundary,
never fabricated old DCP state. Fork retains a bounded 4,096-row / 16-MiB atomic copy
envelope. Client history/DCP restoration cannot rewind remote model KV or unavailable
reasoning. External tool/file effects remain as-is by design. Other T44 requirements
and exact comparisons remain open.
