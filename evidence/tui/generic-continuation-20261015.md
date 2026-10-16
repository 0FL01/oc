# VIS16/17: Generic continuation foreground facet

Status: **CONTINUATION_STYLE_VERIFIED / full visual and temporal parity OPEN**.

## Change and direct evidence

The current paired baseline isolated a common renderer fault, not a producer
difference: continuation indentation inherited Generic's muted **key text**
foreground. Original GenericTool layout leaves those cells at terminal defaults.
`generic_block_expanded` now keeps the first key/colon muted but emits subsequent
indentation as plain layout spans. Text, wrapping, bounded body/guidance facts,
permissions, capture ownership, mouse targets and replay are unchanged.

The separate owner regression
`generic_continuation_indents_inherit_canvas_not_muted_labels` checks hard/soft
continuations at widths32/120 and retains muted first-line labels. All **541
oc-tui library tests pass**, including the existing real live/page/mouse/caret
owner test; the new targeted regression passes independently.

Fresh immutable actual-binary capture:
`evidence/tui/resume-tool-preview-20261015-03`, same command/profile as
[the resumed baseline](resume-20261015.md), using two Cargo build jobs.
Captured HEAD `5caa28385` plus its exact dirty diff/source manifest; native SHA256
`f77b9efe8c411d37f4556706a76c001172e3455ea21b724223abec8a301e89e6`.
The later needless-reference cleanup affects only the regression's call site,
not that captured production binary.

Both actual binaries pass all27 behavioral stages: six provider requests, four
tool calls, three MCP calls and one17-byte Shell effect. Existing snapshots
assert unchanged calls, effect and artifact hashes through view changes, reopen
and restart. Native owner checks remain bounded/separate-body-guidance with
three artifacts and frozen foreground Shell facts.

| Full grid comparison | Baseline first differing row | New first differing row | New differing cells |
| --- | ---: | ---: | ---: |
| MCP first expanded | 10 | 16 | 1698 / 9600 |
| MCP second expanded | 12 | 18 | 1567 / 9600 |

The previously mismatched continuation spaces now match complete cell records,
including foreground. The row numbers above diagnose the repaired facet; they
are **not** cropped comparison PASS claims. The original comparator still reads
every cell of every full frame. Overall: **6 PASS / 48 FAIL grid/PNG comparisons**,
all27 paired cursor states equal; four comparisons separately identify native
resource-only details. No masks, normalization, fake durations or comparator
changes. This does not close VIS16/17/42 or qualify prompt blink.

## Checks and continuation

- `cargo test --locked -p oc-tui --lib`: 541 passed.
- Targeted continuation regression: PASS.
- `rustfmt --check --edition 2024` on both changed Rust files: PASS.
- `cargo clippy --locked -p oc-tui --all-targets -- -D warnings`: PASS after its
  real test-only `needless_borrow` finding was corrected; no lint was suppressed.
  The targeted regression was rerun after that cleanup and passed.
- Full workspace baseline gates were launched before this renderer edit and
  must not be presented as final-source qualification of this change.

Next comparable-state experiment: small direct MCP/Shell output without
preceding service failures; retain real no-replay proof. If elapsed footer text
is the remaining difference, use a bounded **real** fixture-response wait tied
to actual PTY submission, not production clock overrides, history rewriting or
paint substitution. Original footer duration is completed-message time minus
input creation (`routes/session/rows.ts::turnDuration`); native duration comes
from the runtime's real `Instant`. Any outcome remains a full unmasked comparison.
