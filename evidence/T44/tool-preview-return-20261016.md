# T44 — current tool-preview qualification

## Result

T65 is complete and all four0.2.0 packages are published and registry-installed
behavior is verified. T44 remains active; this receipt does not close VIS16/17
or the full visual/temporal contract.

The restored Shell-success hypothesis was **rejected**, not shipped. An initial
two-line change synthesized a zero-exit notice from owned exit metadata. The
new uncommitted test passed, but three existing tests correctly rejected it.
Reviewing their explicit `fee0123a0` source requirement and the current VIS16
acceptance confirmed that native Shell must **omit synthesized exit-zero UI
status**, preserve actual output and abnormal outcomes, and avoid a duplicate
success golden. The implementation experiment and its new50-line test were
removed; all three existing assertions remain unchanged. The stash object is
preserved as historical WIP, not reapplied.

The pinned original's small fixture does display a producer-generated zero-exit
notice. That recorded producer difference remains visible in full comparisons;
it does not authorize violating the explicit native requirement to force equality.
No contract, comparator or baseline was weakened.

Current0.2.0 baseline `evidence/tui/resume-tool-small-20261016-01` passes13
behavioral stages per side: five requests, three tool effects, real5.5s footer
buckets and natural generation0/1 exits. All26 full grid/PNG comparisons differ;
all13 final cursor states match. Generic expansion has1710/1700 differing cells:
the original remains at the conversation bottom, while native anchors the old
top, detaches and hides the final answer behind `Jump to latest`.

The narrow fix keeps sticky follow for typed Generic cards only. Detached and
older-page readers retain their top anchor. An initial broader attempt failed
the existing **Explored group** anchor regression; it was narrowed instead of
changing that test's expectations. Shell policy and group anchoring are unchanged.

Fresh capture `evidence/tui/resume-tool-small-20261016-02` verifies that fix:
MCP expanded comparisons fall from1710/1700 to34 differing cells, and both
re-collapse stages now have34 rather than428. The final answer/footer remain
visible, with no erroneous `Jump to latest`. Every remaining cell difference in
those frames is inside the Shell output region, reflecting its explicit native
exit-zero omission and consequent collapsed tail. The full comparator is still
FAIL, not a masked Generic PASS. Expanded Shell has304 differences from its
two fewer producer/status rows; it does not show an erroneous detach.

Both sides again pass13 stages, five requests/three effects, real5.5s buckets,
unchanged effects/artifacts across reopen/restart, and natural generation0/1
exit0. All13 paired final cursor states are equal. All26 full grid/PNG comparisons
remain different (33 cells at80×24;34 in the other collapsed/MCP stages).
Captured native0.2.0 binary SHA256:
`218c1ca23b212753d7cbfbafbe98ca8ec49e79ff7096ec354dd86d5a6177f0e0`,
HEAD `7dd378d69` plus recorded exact dirty diff/source manifest.

## Checks

- Restored capture Python tests:10 PASS; changed JavaScript syntax checks PASS.
- Rejected implementation: targeted new test PASS; full TUI539 PASS/3 FAIL.
  This was useful contradictory evidence, not a qualification result.
- Source review: current goal Shell success acceptance and
  `tui-recovery/ACCEPTANCE.json` VIS16 retain the omission requirement, introduced
  by `fee0123a05007155f7253330f89a262951c23749`.
- Configuration source/gates and publication remain independently qualified at
  `feeea77fc`; no production change from the rejected experiment remains.
- Unchanged Shell owner regressions:24 PASS.
- New Generic follow regression plus nearest Generic tests:9 PASS. The broadened
  attempt's full TUI run had541 PASS/1 FAIL. After narrowing the change:
  expansion/anchor regressions5 PASS; full TUI542 PASS; strict affected-crate
  all-targets Clippy PASS. No existing assertion or golden was changed.
- `cargo fmt --all --check`, docs/progress and diff checks PASS.

## Risks

No external blocker. Generic sticky-follow is a qualified narrow facet, not
whole VIS16/17 or prompt-blink qualification. Full comparisons retain actual
producer/policy differences and must not be renamed pixel PASS. The known Shell
omission is not a reason to add forbidden synthetic success text.

## Next

Commit/push this verified tool-preview slice, then continue prompt/caret/blink/
scroll qualification with the existing actual temporal profiles. Preserve typed
facts, RAW, draft/caret, detached/older-page and group anchors and no replay.
Remaining question/tabs, execute presentation and full frozen T44 work stay open.
