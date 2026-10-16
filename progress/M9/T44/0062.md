## Result

Delivered c452180 behavior: Undo chooses preceding nonempty user text; Redo restores
the whole saved staged tail without generation/tool replay/filesystem mutation.
Durable owner-projected reverted count and boundary render a real card with hover,
selection-protected click and configured shortcut hint. Click, slash, palette and
shortcut use the same owner operation; errors retain card/boundary/draft. Admitted
config sources project effective conversation shortcuts and reload/Location scope;
editor Ctrl-minus remains editor undo. Branch invalidation/archive/DCP remain intact.

## Checks

Full serial fmt/locked workspace tests/strict all-target Clippy/locked build,
capture syntax/frontend/docs/progress/diff PASS: tool_0deab074e001sKzMIryMp9yIZ2.
Real-owner five-turn test exercises all four Redo entry paths with zero extra
provider calls. Paired source-built revert-redo-20260926-07 verifies three turns,
Revert count2, selection guard, whole-tail click/shortcut/slash/palette/restart on
both executables: four actual requests each, no extra action requests. All 58
unmasked comparisons DIFFERENT. Evidence: recovery-v00/revert-redo-report.md.

## Risks

Three-turn pager capture does not qualify large-history loading. Optional custom
shortcut paired capture not run (owner/binary tests cover config). Current profile,
tps, palette differences persist; no VIS33/V09 exact PASS. Older step-Redo claims
are historical and superseded. Owner 4bee144 adds compaction parity, preserved.
Inherited .opencode/ untouched.

## Next

Deliver verified whole-tail/card slice, then implement high-refresh event-driven
redraw and wheel qualification VIS31/VIS32; subsequently address 4bee144 session
compaction requirements without removing existing DCP semantics.
