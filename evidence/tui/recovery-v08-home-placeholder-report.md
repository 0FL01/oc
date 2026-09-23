# T44 — Home normal-mode prompt hint (paired diagnostic)

## RECON and change

Pinned upstream v2.0.12 commit `2670273ff17da96f85c5826ced57aa1b368754fa`:
`packages/tui/src/routes/home.tsx:19-23` supplies three normal-mode
examples; `component/prompt/index.tsx:103-106,1583-1595,1736-1740`
chooses one at random and paints `Ask anything… "<example>"` in muted color
only while the textarea is empty. The Session prompt supplies no example
list. The native Home previously left that prompt row blank. The native
TUI now samples one of the source-backed Home examples at construction,
clips at whole graphemes to the textarea width, paints its characters and
interior spaces muted, and leaves the remaining row blank in the upstream
canvas foreground. Typed input and the Session prompt are unchanged;
shell mode is not represented as a working capability by this change.

## Independent reference captures

Attempts `evidence/tui/recovery-v08-home-placeholder-{01,02,03,04,05}/`
use installed original binary SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
original commit above, shared tools fixture SHA-256
`dbfc93c470dd18d3af79b33195a13c898e83944ec5e0f242cd32d88b65531644`
and the 120×40 terminal profile ID
`e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`.
All attempts execute both real PTYs and fake-provider tool calls with
`provider_contract=true`; the existing exploration group expands and
recollapses on both sides. No live credentials are used. Each command uses
the same options, changing only its immutable output suffix:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --columns 120 --rows 40 \
  --exploration-click true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-home-placeholder-01
```

Initial attempt 01 independently selected the same `What is the tech
stack of this project?` on both sides. Its Home prompt prefix at x=26–38,
y=21 is **0/13** differing cells versus **13/13** in the preceding
`recovery-v08-tab-indicator-01` capture. Attempt 01 exposed **15**
foreground-only differences after the matching example (x=81–95);
the following native fix restricts muted styling to the example and
retains the white blank foreground. Attempt 02 verifies the unchanged
prefix **0/13**; attempts 02–05 selected *different valid* examples
independently in the two running applications, so their larger placeholder
rectangles cannot be used as equal-state parity evidence. For example,
attempt 02 differs in the x=26–95,y=21 rectangle by **20 symbol cells**
and **10 styled cells**, only where the different example texts or their
lengths differ. TestBackend regression checks the non-text blanks, the
three allowed examples, the prompt caret, typing, Session no-hint, and
24/43/44/120-column clipping, including grapheme boundaries.

Full Home grids remain DIFFERENT: attempt 02 **212/4,800** cells. This
also includes the original's implicit Build profile absent from this
native fixture, border color, version, and other chrome differences. All
whole-frame grid/PNG runner results are DIFFERENT (exit 1); neither
the matched prefix nor Rust-only tests qualify a VIS PASS.

## Checks and open outcomes

`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace
--no-fail-fast --quiet` PASS (TUI 183; opt-in live ignores); workspace fmt,
all-target clippy with `-D warnings`, locked build, Node runner syntax,
docs/progress and diff checks PASS. The initial unscoped `--exact` filter
matched zero tests; rerun with the full test name matched and passed.
T44 remains active: true multi-tab/add-tab behavior, unread/attention
states, prompt/footer equivalent profile, streaming/recovery and all
remaining VIS01–VIS24/V08–V09 gates are unqualified.
