# VIS11 actual-binary paired leader qualification — 2026-09-27

**Result: VIS11 NOT PASS.** Final selected set: **152 full styled-grid comparisons
DIFFERENT; 152 full PNG comparisons DIFFERENT**, no unstable final-selected frames.
Evidence01 and every failed/superseded attempt in evidence02 remain intact.

## Source/build provenance

HEAD: `0a86cf67c1f3e620f5385911a3f51783fe747663`, plus captured dirty source.
Every selected campaign ran `cargo build --locked` through `--build-oc true`, exit 0.
All seven source-built selected campaigns have identical Rust-input digest
`45f24dd9b0e3ebf24d90cb81b354acd3684bcbe3f13b367da0e091347937959b`
and native executable SHA-256
`633a81b1173c80ade38d8a555a37c4d72f003d09e2bee42f6b830edba93a581b`.
Script changes between campaigns are reflected in their full source manifests.
This qualifies that captured dirty source, not an unbuilt claim about HEAD alone.

Original SHA-256:
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
All frames use real PTY bytes → shared xterm/Chromium, 120×40, complete cells/PNG,
no masks, no application clock replacement, no fake panels. Real title/version,
path and effects are retained. Normal frame differences include the actual
version footer: six styled cells / 298 pixels, with equal caret.

Selected campaigns: `paired-default`, `paired-nested`, `paired-configured`,
`paired-precedence`, `paired-legacy-v1-02`, `paired-extra`, `paired-enter-isolated`.
`paired-analysis.json` verifies source builds and every sealed grid/PNG/render hash,
records every full-frame difference count/bounds and cursor result, and preserves
observed semantic failures as failures. Analyzer exit 0 means evidence integrity,
not product PASS.

## Additional original oracle, before source-built pairing

`original-extra` retained the first Enter-first oracle. Enter submitted the draft,
so later cases had an empty composer; these later cases do not establish nonempty
draft semantics. It also retains one unstable frame. `original-extra-02` reorders
cases so Left/Right, modal expiry and Ctrl+C all run against a nonempty draft.

- Leader→Left and leader→Right restore colors; draft and existing middle caret
  remain unchanged in this fixture. Default donor bindings are pane-focus chords;
  this does not establish swallowing of every unmatched nonprintable key.
- Leader→Ctrl+C preserves the nonempty draft/caret in original; no exit/request.
- In Commands, filtering to `zzzz-no-such-command` exposes the underlying draft.
  Leader changes underlying text `#626262`→`#353535`; idle expiry changes it back
  to `#626262`, with unchanged modal filter/caret. Modal dismissal returns normal
  composer `#eeeeee` and original middle caret.
- Leader→Enter **submits**, issuing a real local Responses transcript request and
  auxiliary title request. No live API or external effects were used/replayed.

## Concrete native semantic failures

All paths below have actual `.cells.json`, `.png`, `.txt`, `.vt` and render metadata.

1. **Enter swallowed:** `paired-enter-isolated/oc/leader-enter-only-next`.
   Identical starting draft and caret on both sides. Native retains the draft and
   produces zero provider requests. Original submits the exact entire Unicode draft,
   clears composer, and produces **two valid local fixture requests**: transcript
   and title. Actual request input is retained in protocol and paired-analysis.
   This isolated test removes the prior Ctrl+C state difference as a confounder.
2. **Ctrl+C clears nonempty draft:** `paired-extra/oc/leader-extra-ctrlc-next`.
   Native draft disappears and caret becomes `(26,21)`; original retains full draft
   and caret `(77,21)`. Both had the same nonempty draft/caret before the key.
3. **Modal leader lifecycle missing:** `paired-extra/oc/leader-extra-modal-pending`.
   Native underlying text stays `#626262`; original becomes `#353535`, then returns
   on expiry. Both keep modal search caret `(54,13)`. Native modal filter focus is
   correct, but pending presentation/idle restoration is not equivalent.
4. **Actual paste chip expansion absent:**
   `paired-default/oc/leader-chip-expanded-fulltext` (also all other main pairs).
   Mouse-down/up at the actual displayed chip expands original's retained three
   lines. Native leaves the placeholder intact. This establishes an expansion
   mismatch; it does not prove hidden draft content was lost.
5. **Chip wrap/caret differs:** main pairs' `leader-chip-normal`, `-pending`,
   `-restored`, `-expanded-fulltext`: original compact caret `(33,22)` versus
   native `(29,22)`. Native pending keeps chip RGB, but chip geometry/full caret
   equality still fails. Twenty main-pair grid comparisons have cursor differences.

`paired-extra` Enter input differs after the preceding Ctrl+C mismatch; its result
is retained, but isolated Enter is the decisive equal-start-state proof.
No semantic failure is labeled consumed-correct or VIS11 PASS.

## Observed matching core lifecycle, with full comparisons still failing

In five main pairs, normal/pending/restored draft symbols and existing caret are
preserved across leader-alone/idle expiry. Invalid `z` inserts at the actual middle
caret, producing `wzord`; Esc/Backspace/repeated leader restore without changing
draft/caret. Repeated leader ends rather than restarts pending. Configured palette
chords open actual Commands, and model chords open actual Select model.

Observed RGB: draft/model `#eeeeee`→`#808080`; prompt/Build highlight
`#5c9cf5`→`#484848`; provider remains `#808080`; variant remains `#f5a742`;
actual chip retains foreground `#0a0a0a` / background `#f5a742`.
Whole-frame DIFFERENT verdicts include version and other actual differences;
these matching observations do not replace the whole-frame gate.

All main pairs have **zero provider requests**. Extra-pair requests occur only
after Enter; isolated original Enter has two valid requests, native has zero.

## Actual timeout admission and no-input restoration writes

Times below are raw PTY substantial restoration writes relative to the bridge's
acknowledged leader input. Full pending/restored grids prove the color restoration;
each idle window contains exactly **one input: the leader**. There is no injected
expiry input. `output-timeline.jsonl`, `inputs.json`, protocol and raw VT retain
actual timestamped bytes; small terminal-control writes remain unfiltered there.

| Pair | Actual configuration | Original / native restoration write (ms) |
|---|---|---|
| default | Ctrl+X, default | 2006.95 / 2006.45 |
| nested | Ctrl+X, `leader.timeout=1200` | 1207.49 / 1205.15 |
| configured | Ctrl+G, palette remap, nested 1200 | 1209.75 / 1212.13 |
| precedence | Ctrl+G, nested 1800 + legacy sibling 600 | 1825.88 / 1804.57 |
| legacy-v1-02 | admitted v1 migration / explicit native legacy 1400 | 1408.00 / 1406.25 |

For legacy original, fixture writes actual v1 `tui.json` with `leader_timeout=1400`,
leader Ctrl+G and `command_list:<leader>p`, with no preexisting cli.json; pinned
original performs its normal migration. Native receives its admitted explicit
legacy field and `command_list` alias. Config manifests truthfully record the
different admission routes. **Raw v2 sibling leader_timeout is not treated as an
equivalent original oracle:** original ignores it, as evidence01 demonstrated.
Nested precedence matches both effective observed timeouts, with that distinction.

The first `paired-legacy-v1` used inconsistent palette remaps on the two sides;
it is preserved with 50 DIFFERENT / 2 BLOCKED comparisons. Fresh `-02` uses real
legacy command_list remapping and captures all 26 stages per side.

Raw writes show timer-triggered restoration with no extra input. These PTY
observations alone do not establish a production scheduler/CPU polling audit.

## Checks and reproducibility

Syntax checks: capture/probe/analyzers `node --check`, bridge Python AST — PASS.
Existing frontend checker previously PASS; same frontend remains in use.
`git diff --check` — PASS. No Rust/acceptance/progress/.opencode edits by this work.

Representative selected source-built invocation:

```sh
node scripts/tui_capture/capture.mjs --leader-pending true --leader-config default \
  --geometry true --sidebar hide --sample short --columns 120 --rows 40 \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --output NEW_IMMUTABLE_PATH
```

Modes: default/nested/configured/precedence/legacy-v1. Extra oracle uses
`--leader-extra true`; equal-start Enter uses `--leader-enter-only true`.
Analyzer: `node scripts/tui_capture/analyze_leader_pairs.mjs EVIDENCE02_ROOT`;
it refuses to overwrite its analysis output.

**Cargo ownership returned after this report.** Rust owner should address the
concrete semantics above, then request fresh source-built paired evidence.
