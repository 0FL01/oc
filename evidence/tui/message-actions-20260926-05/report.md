# Opt-in native Fork/Revert actual-action qualification

Fresh isolated attempt, same source-built binary as -03; no Cargo invocation.
Command is the -03 paired command with `--message-fork-revert true` and
`--output /home/opencode/ai/oc/evidence/tui/message-actions-20260926-05`.
Overall exit **1**, including an actual failed Revert on copied fork history.
Intermediate attempt -04 is preserved (Fork succeeds; fork Revert fails).

## Actual behavior

Paired Copy passes on both binaries: exact prompt OSC 52, popup closes, no new
requests. Native two undo/two redo pass at fixed **5 requests / 5 completions /
0 invalid**. Then the new opt-in uses real Message Actions clicks:

1. **Source Revert PASS:** second turn disappears, first answer remains,
   `Second same-session spacing check?` is restored in prompt, popup closes.
   Counts stay 5/5/0. Source redo restores the second turn without requests.
2. **Fork PASS observation:** clicking Fork on second user creates another tab,
   keeps the first answer, omits the selected second turn, restores its prompt.
   No Subagents chrome appears and counts stay 5/5/0.
3. **Fork retained-turn Revert FAILED:** after clearing the restored draft and
   clicking the retained first user's Revert, popup stays open, first answer
   remains, prompt is not restored. Bounded 8-second predicate times out.
   Failed actual styled grid/cursor/PNG/VT is `oc/native-conversation-failure.*`.
   No provider requests occur during the failure.

Read-only SQLite inspection after teardown (mode=ro) confirms actual root and
missing context points:

| Session | parent_id | conversation_points count |
|---|---|---:|
| `s-tui-18d8d6ed16dec3f9-32a0b3-0` | NULL | 2 |
| `fork-9e5f07db9658ba18c086b588193aa9ef` | NULL | 0 |

Root status is therefore confirmed from owner storage, not inferred only from
paint. Suggested next engineering step: inspect creation/rebasing of causal
conversation points for copied completed fork turns and their Revert eligibility.
No Rust changes were made by the probe. The source-session Revert success and
fork-history Revert failure are distinct results, not a general Revert PASS.

## Exact unmasked paired diffs

| Stage | Different cells | Different PNG pixels |
|---|---:|---:|
| Home | 27 | 1,247 |
| Completed turn | 3 | 169 |
| User hover | 3 | 169 |
| Popup | 14 | 850 |
| Copy hover | 14 | 850 |
| Copy after | 87 | 10,969 |

All paired cursors equal. All paired grid/PNG comparator exits **1**. No masks;
native-only action frames have no original stepwise/fork-Revert parity claim.
Source Revert, fork root and failure each have independent full captures.
Syntax/frontend/geometry/diff checks exit 0. Evidence hashes and exact inputs are
sealed in `capture.lock.json`, `source-manifest.json`, and per-side protocol files.
