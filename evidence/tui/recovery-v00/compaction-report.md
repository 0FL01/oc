# VIS34 — actual compaction capture campaign, 2026-09-27

## Latest correction — attempt 23 and final gates

After the source-backed terminal-paragraph/footer spacer fix, the fresh immutable
`compaction20260927-23` explicit-TPS-false threshold run passes both behavioral probes.
Its completed frame has **3/4800 differing styled cells**, exclusively independent
elapsed digits; archived paragraph tail and compaction divider/body now match.
The fix removes only one structural terminal paragraph newline before its matching
completed footer, preserving explicit blanks and fenced/list/heading boundaries.
Full/cached/indexed regression and final serial workspace/release/oracle gates PASS:
`tool_0e0ac5a5c001YMn8ZMaXNoXIrZ` (parent execution). Rebuilt release actual
headless/TUI entry with `prune=true/false` and 13 pinned-source normalization cases
pass. Whole-frame comparisons still fail; this is not VIS34/V09 exact PASS.
Attempts 01–22 and all prior diagnoses/provenance remain unchanged below.

## Previous production-source qualification — attempts 18–22

**All exercised lifecycle/projection checks PASS on both actual binaries. All
118 full grid/PNG comparisons remain DIFFERENT. Completed summary divider/body
has zero differing styled cells in all five campaigns.** No failed predicate,
unstable capture, BLOCKED or INVALID comparison occurred in 18–22. Historical
01–17 locks, requests, source manifests and failures remain preserved. The prior
15–17 source is not represented as current.

### Actual source/binary association and commands

HEAD remains `b9760200c8c782b61382520fd5c2c6943fc6f7fe`; each fresh manifest includes
the owner's dirty production Rust inputs. Each of 18–22 independently executed
`cargo build --locked`, exit 0, then captured `target/debug/oc` with SHA-256:

`56a004185a05853e74f8283f7afc7d5dd1bcacdd5b758231455dcf5d14692e70`

The validator checks matching production-source hashes and native executable
hashes across all five. Reference remains pinned OC2 v2.0.12 donor
`2670273ff17da96f85c5826ced57aa1b368754fa`, executable SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
18–20 use the unchanged fixtures/probe/bridge of 15–17; 21–22 add declared
tooling-only option fields for the explicit TPS setting and animated observation.
The Responses handler is byte-identical across all five; actual seeded
input/content/usage in 21–22 is independently checked against 19. No threshold
relaxation, transcript import or success substitution was added.

These are the executed capture commands (also sealed separately in each
`commands.json`):

```sh
node scripts/tui_capture/capture.mjs --compaction true --compaction-trigger manual --geometry true --sidebar hide --sample short --columns 120 --rows 40 --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --output evidence/tui/recovery-v00/compaction20260927-18
node scripts/tui_capture/capture.mjs --compaction true --compaction-trigger threshold --geometry true --sidebar hide --sample short --columns 120 --rows 40 --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --output evidence/tui/recovery-v00/compaction20260927-19
node scripts/tui_capture/capture.mjs --compaction true --compaction-trigger overflow --geometry true --sidebar hide --sample short --columns 120 --rows 40 --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --output evidence/tui/recovery-v00/compaction20260927-20
node scripts/tui_capture/capture.mjs --compaction true --compaction-trigger threshold --compaction-tps false --geometry true --sidebar hide --sample short --columns 120 --rows 40 --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --output evidence/tui/recovery-v00/compaction20260927-21
node scripts/tui_capture/capture.mjs --compaction true --compaction-trigger threshold --compaction-tps false --compaction-animation true --geometry true --sidebar hide --sample short --columns 120 --rows 40 --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --output evidence/tui/recovery-v00/compaction20260927-22
```

| Attempt / mode | Actual requests per side (title / main / summary) | Real tool calls per side | Behavioral probes | Whole grid + PNG |
|---|---:|---:|---|---|
| 18 manual | 11 (1 / 7 / 3) | 1 | PASS / PASS | 38 DIFFERENT |
| 19 threshold | 7 (1 / 5 / 1) | 0 | PASS / PASS | 20 DIFFERENT |
| 20 overflow | 8 (1 / 6 including rejection / 1) | 0 | PASS / PASS | 20 DIFFERENT |
| 21 threshold, explicit TPS false | 7 (1 / 5 / 1) | 0 | PASS / PASS | 20 DIFFERENT |
| 22 threshold, TPS false + animated | 7 (1 / 5 / 1) | 0 | PASS / PASS | 20 DIFFERENT |

Every runner exits 1 because of visual differences; every build exits 0. Each
side/campaign has two natural code-0 application exits. Across all five there
are 59 paired states, 118 complete styled 120×40 grids and 118 full 1011×640 PNGs.
No timestamps, titles, elapsed durations, phase glyphs or dynamic cells are masked.

### Verified actual lifecycle and contexts

- Manual `/compact` and palette Compact session coalesce while actual authorized
  `sleep 25` is held. Palette ACK dismisses; read-only queued/running DB snapshots
  show the one operation waiting for real tool completion before summary.
- Actual main turns seed enough text. Actual differentiated summarizer Responses
  input is retained: native Developer + User JSON transcript, no offered tools;
  original structured prior roles plus final User summarizer instruction, with
  its admitted shell definition still offered and instructions in their actual
  provider field. The fixture streams actual Markdown and completed usage
  1234 input / 321 output (234 cached / 21 reasoning).
- Canonical configuration is `auto=false` for manual, `auto=true` for automatic,
  `keep.tokens=0`, `buffer=20000`; automatic catalog context is 40000/output 2048.
  Actual third main usage remains 23000 input + 1800 output. Threshold summary
  precedes the next real main request. Current user is durable during running,
  and native operation anchor equals that user's actual message ID.
- Overflow receives one actual HTTP 400 `context_length_exceeded`, requests one
  summary, then rebuilds the actual main request once. The next and restarted
  provider projections contain installed summary plus recent/current prompts,
  exclude old `ARCHIVE-1-` prefix and retain valid causal tool pairs.
- Original raw seeded messages remain unchanged; actual completed checkpoint
  survives clean restart/reopen. Manual failed and cancelled summary operations
  leave the prior checkpoint, settled raw rows and nonempty workspace file hashes
  unchanged. Native displays actual typed failure diagnostic
  `HTTP status 400: VIS34 bounded summary failure`.
- Latest post-checkpoint `/undo` restores the user prompt to the editor draft;
  `/redo` restores the restart exchange with no additional provider/tool request,
  raw-row mutation or workspace write. The existing probe clears that restored
  draft before typing `/redo`. A separately typed pending draft during summary
  is not exercised by this campaign.

Actual config/CLI leaf values and file hashes, full requests, SQLite `mode=ro`
observations, source/binary locks and per-frame hashes are in the immutable
campaigns and derivative validation reports. The workspace sentinel is seeded
before capture; the fixture does not write a transcript or install a checkpoint.

### Exact residual visuals and explicit footer setting

| State | Different / 4800 cells | Different / 647040 pixels | Completed summary rows 16–29 |
|---|---:|---:|---:|
| 18 manual running-1 | 823 | 86995 | — |
| 18 manual completed | 829 | 87055 | 0 |
| 19 threshold running | 93 | 3573 | — |
| 19 threshold completed | 97 | 3477 | 0 |
| 20 overflow running | 96 | 3693 | — |
| 20 overflow completed | 100 | 3593 | 0 |
| 21 explicit TPS false running | 77 | 3043 | — |
| 21 explicit TPS false completed | 66 | 2515 | 0 |
| 22 animated running, independent phases | 103 | 5814 | — |
| 22 animated completed | 66 | 2493 | 0 |

Completed summary heading/list blank rows and fg/bg/modifiers match exactly,
at rows 18/20, 22/24, 26/28; divider row 16 and continuation row 30 match.
This row observation accompanies the unchanged full-frame gate, not a cropped
replacement for it.

**Correction to the earlier 15–17 narrative:** original `cli.json` explicitly
sets `session.tps=false`; native leaves it unset. The older claim that neither
fixture sets it was incorrect. It was a fixture-setting asymmetry, not a measured
native default-parity defect. In 21 and 22 the real config explicitly sets false
on both sides; every captured text frame lacks throughput `tok/s`/TPS footers.
Canonical 18–20 preserve the old fixture exactly, including this asymmetry.

Correctable residual layout/style remains independent of timing/version:

- Manual original shell displays `(no output)` and `Command exited with code 0.`;
  native bash chrome omits these rows, shifting held-user/archive-tail placement.
  Native held-user row 9 versus original 6 is still observed. Completed 18 has
  277 symbol and 552 style-only differences, overlapping fg 315/bg 580.
- In completed 21, 63 differing cells are archive-tail rhythm: rows 2–7 differ
  by one archive sequence digit each and row 8 differs in 57 cells because native
  has an extra blank row before attribution. The remaining 3 cells are actual
  elapsed-duration digits in attribution rows 10 (2 cells) and 32 (1 cell).
  Total 66 = 60 symbol + 6 style-only; overlapping fg 57, no bg/modifier mismatch.
  Thus this explicit-setting frame's remaining grid mismatch is precisely located;
  pixel totals are not arbitrarily partitioned into causal classes.
- Animated 22 deliberately captures unequal real phases (original `⠴`, native
  `⠧` on the running divider), with independent active-tab/interrupt glyphs.
  Phase differences are not an invented timing-normalized comparison.
- Native failure has an actionable HTTP status prefix absent from original.
  Version branding is relevant to Home where shown, not completed transcript.
  Real elapsed numerals stay in the comparator; no blanket time/version blocker
  or parity PASS is claimed.

### Bounded animated running observation

22 opts both actual applications into animations=true. The real provider holds
partial summary while the runner records raw PTY chunks with the bridge's actual
monotonic read timestamp, plus live browser samples. It then pauses its owned
PTY child/process group using SIGSTOP, drains/ACKs, captures one stable full grid
and PNG, and resumes with SIGCONT in a finally block. Pause/resume ACKs and all
phases remain in protocol/VT; phases are never selected to equal the reference.

Read-only replay of those chunks through pinned xterm observes all ten Braille
phases on each Compaction row, with 33 phase observations / 32 changes per side:

| Side | Raw chunks, all generations | Observed phase-delta min / median / max ms | Live browser samples | No Compaction Braille after completion, generation-0 observed window |
|---|---:|---:|---:|---:|
| original | 443 | 62.973 / 83.504 / 2546.291 | 7 | 3685.955 ms |
| native | 228 | 66.770 / 79.770 / 2483.916 | 7 | 2714.684 ms |

All raw deltas, including the intentional screenshot-pause gap, remain recorded.
Browser sample windows are 1446.993/1478.714 ms, undersampled by IPC/browser cost;
they are not 20ms or 80ms cadence proof. Raw chunk glyph-change medians are
compatible with the specified approximately 80ms cycle, but are PTY-read
observations, not paint timestamps, hard deadlines or FPS. No later Compaction
Braille is observed in generation-0 output after completed divider; this does
not measure worker wakeups or idle CPU. See `compaction-animation-observations.json`
and `compaction20260927-22/{upstream,oc}/{output-timeline.jsonl,compaction-animation-samples.json}`.

### Verification and precise remaining qualification gaps

Executed independent checks, exit 0:

```sh
node scripts/tui_capture/check_compaction_evidence.mjs evidence/tui/recovery-v00 --latest --quiet
node scripts/tui_capture/check_compaction_evidence.mjs evidence/tui/recovery-v00 --latest --optional --quiet
node scripts/tui_capture/analyze_compaction_frames.mjs evidence/tui/recovery-v00 --latest --quiet
node scripts/tui_capture/analyze_compaction_frames.mjs evidence/tui/recovery-v00 --latest --optional --quiet
node scripts/tui_capture/observe_compaction_timeline.mjs evidence/tui/recovery-v00
```

Validation status: `PASS_BEHAVIOR_VISUAL_DIFFERENCES_REMAIN`. Separate derivative
reports: `compaction-validation-latest.json`, `compaction-validation-latest-options.json`,
`compaction-visual-measurements-latest.json`, `compaction-visual-measurements-latest-options.json`,
`compaction-animation-observations.json`; earlier reports are preserved. Full frame
paths are `compaction20260927-{18,19,20,21,22}/{upstream,oc}/compaction-STATE.{cells.json,png,txt,render.json,vt}`;
whole grid/PNG diff reports are alongside immutable locks/manifests/commands.

Node syntax checks for capture/probe/validator/frame analyzer/timeline observer,
Python compilation for bridge/provider fixture, existing frontend and geometry
checks, and `git diff --check` all PASS. `pgrep` finds no owned bridge/capture
processes after cleanup. No additional Cargo test/release invocation was run by
this agent in this cycle. Changes are capture tooling/docs/evidence only; no
product Rust, `.opencode`, commits or pushes were changed by this agent. Cargo
exclusivity is released after these final captures.

Scope of remaining existing VIS34 gates:

- Parent reports PASS for the 13-fixture pinned TS differential normalizer,
  actual headless/TUI `prune=true/false` diagnostic entry checks, `auto=false`
  threshold/overflow suppression with manual preserved, restart guard before new
  primary usage, and per-logical-step overflow limit. This capture task did not
  execute those backend tests. Source-backed nearest normalizer test is
  `crates/oc-adapters/src/compaction.rs::compaction_pinned_normalization_differential_fixtures_and_layers`
  using `crates/oc-adapters/tests/fixtures/compaction_normalization.json` and
  `scripts/compaction_normalization_oracle.ts`; actual prune entry test is
  `crates/oc/tests/mcp_application.rs::vis34_compaction_prune_normalization_headless_and_tui_entry`.
  Aliases, native precedence, conflicts, unsupported prune/tail_turns, invalid
  leaves and layered diagnostics are source/parent-test evidence, **not paired
  config fixtures executed in 18–22**.
- Parent reports `cargo build --release --locked -p oc` and actual release-entry
  test PASS at `/home/opencode/ai/oc/target/release/oc`. That is a parent-reported
  release gate, not an agent-run build/test or release paired-capture attestation.
  This agent's five source builds/captures target `target/debug/oc` only.
- These restarted UI requests occur after a post-checkpoint primary response;
  they do not independently isolate the before-primary-usage restart guard.
  Automatic below/at boundary, auto=false automatic suppression and multi-overflow
  per-logical-step failure paths are not newly paired here. Canonical manual
  with auto=false and bounded threshold/one-overflow are observed directly.
- Admission schema-floor/no-spend, adversarial secret redaction, separately
  typed pending draft during summary, idle CPU/wakeups and exact animation deadline
  remain unexercised by these captures. No requirements outside VIS34 are added.
- Production provider-native compaction has no registered strategy. A fake opaque
  unit capability is not attributed to production; no native summary-free success
  is fabricated. Whole-frame parity is still DIFFERENT, so these captures do not
  close VIS34's paired UI gate.

## Final-source qualification — attempts 15–17

**Both binaries PASS all exercised lifecycle/projection invariants. Completed
summary divider/body styled cells now match exactly in all three modes; whole
frames remain DIFFERENT.** Earlier campaigns and derivative reports below are
preserved, including their production and harness failures.

Native source-built SHA-256 for 15/16/17 is
`63228dd168d99a04e53c12419e895fa64b1e1839b564caf617f20c2eae4ac274`.
Each capture independently runs `cargo build --locked` (exit 0); validator verifies
matching production source inputs and binary hashes across all three. HEAD is
`b9760200c8c782b61382520fd5c2c6943fc6f7fe`, with the owner's current dirty Rust
changes included in each source manifest. The pinned original hash/commit below
is unchanged. Capture/probe/provider/bridge hashes match 12–14: **no new success
workaround, fixture change or threshold relaxation** was introduced.

Exact commands are the fresh invocation template below with these substitutions:

| `TRIGGER` / `SUFFIX` | Actual requests per side | Summaries / tool calls | Whole grid + PNG results |
|---|---:|---:|---|
| manual / 15 | 11 (1 title, 7 main, 3 summary) | 3 / 1 | 38 DIFFERENT |
| threshold / 16 | 7 (1 title, 5 main, 1 summary) | 1 / 0 | 20 DIFFERENT |
| overflow / 17 | 8 (1 title, 6 main including rejection, 1 summary) | 1 / 0 | 20 DIFFERENT |

Runner exits are 1 from visual comparison results. Both sides' behavioral probes
PASS, with two natural code-0 exits each. There are no BLOCKED/INVALID comparisons
or failed captures in 15–17. The 39 paired states include 78 full 120×40 styled
grids and 78 full 1011×640 PNGs. All timestamps/dynamic cells remain unmasked.

### Source-backed behavior

- Manual slash and palette requests coalesce while the real `sleep 25` tool runs;
  palette dismisses without a workaround. Read-only DB states prove summary
  delivery after tool completion. Both real running frames render the held
  partial summary and `⋯ Compaction`; completion uses actual 1234-input/321-output
  usage (234 cached/21 reasoning), with no invented transcript installation.
- Unchanged threshold measurement is 23000 input + 1800 output. Canonical
  `auto=true`, `keep.tokens=0`, `buffer=20000`, context 40000/output 2048 is loaded
  by both actual entry points. One summary precedes the next main request.
- Automatic **running snapshots** contain the accepted current user in durable
  raw rows. Native running operation's `anchor.message` equals that user's actual
  message ID (threshold and overflow). The user appears before summary in actual
  running/completed UI, matching original chronology. This observes acceptance
  at running, not a separate pre-queued timestamp proof.
- Overflow emits one real HTTP 400 `context_length_exceeded`, one summary and one
  rebuilt main request. Next/restarted requests retain summary/current/recent
  prompts, exclude old `ARCHIVE-1-`, and preserve causal tool pairs.
- Raw completed rows, saved checkpoint and nonempty filesystem hashes survive
  compaction/reopen. Failed/cancelled manual summaries install no checkpoint.
  Native failure now displays the real typed diagnostic:
  `HTTP status 400: VIS34 bounded summary failure`; original displays the same
  fixture message without the native status prefix. Redaction adversarial cases
  are not exercised by this nonsecret fixture.
- Real `/undo` restores the latest user prompt to the draft, and `/redo` restores
  the latest restart exchange without provider requests, filesystem writes,
  raw-row mutation or tool replay. The probe clears that restored draft using its
  existing input sequence. An independently typed pending draft during summary
  is not added/qualified by this campaign.
- `Build` now appears in actual user/assistant/prompt metadata. Native summary
  requests remain actual Developer + User JSON input with no tools; original has
  structured history plus User summary instruction and its admitted shell tool.
  Seed input/body/usage equivalence versus historical 07 is independently checked.
- Schema-inclusive irreducible admission-before-accept/spend is **not exercised**
  by these admitted requests. The owner's reported unit/Clippy gates are separate
  from this capture task; no extra Cargo tests are attributed to this agent.

### Exact residual visual measurements

| State | Different / 4800 cells | Symbol / style-only | Different / 647040 pixels |
|---|---:|---:|---:|
| 15 manual running-1 | 823 | 272 / 551 | 86997 |
| 15 manual completed | 829 | 277 / 552 | 87031 |
| 16 threshold running | 93 | 82 / 11 | 3524 |
| 16 threshold completed | 98 | 86 / 12 | 3477 |
| 17 overflow running | 93 | 82 / 11 | 3533 |
| 17 overflow completed | 97 | 85 / 12 | 3476 |

**Residual completed summary blank-row/style differences: zero.** In all three
completed full grids, rows 16–29 (divider and complete summary body) have **zero
different styled cells**, including fg/bg/modifiers. Heading/list rows match at
18/20, 22/24, 26/28; continuation row 30 matches. This is a diagnostic observation
within the unchanged full comparison, not a substitute cropped parity gate.
Actual summary bodies are identical real streamed fixture output. Previous
missing-heading-gap claims apply to 12–14, not final source 15–17.

Correctable/setting-dependent residuals, distinct from timing/version:

- Manual original shell chrome has `(no output)` and `Command exited with code
  0.`; native actual bash chrome lacks them. Held-user row is 6 original versus 9
  native; three rows of tool-chrome difference change which archive tail fits
  above it. Manual completed overlapping field counts: fg 315, bg 580, no modifier
  differences. Shifted tool/background cells are not an isolated palette defect.
- Correction after reading the actual fixture/CLI leaves: original explicitly
  sets `session.tps=false`; native leaves it unset and shows `tok/s`. This is a
  fixture-setting asymmetry, not proof of a native default-parity defect or an
  explicit false-setting failure. See paired explicit-false attempt 21 above.
- Threshold/overflow completed summary and user ordering now match (user row 13,
  divider row 16). Residual archive-tail rhythm has original last archive row 8
  then attribution row 10, versus native last archive row 7 then two blank rows
  and attribution row 10. Native throughput text extends attribution rows 10/32.
  Completed threshold fields: fg 89, symbol 86; no bg/modifier differences.
- Native failure retains actionable HTTP status prefix; original lacks it. This
  is a real diagnostic-format difference, not a fake-response discrepancy.

Real elapsed durations and throughput numerals vary across runs; those numerals
remain in the comparator. Native 0.1.0 versus original v2.0.12 branding is only
relevant to frames where that version text is actually present (e.g. Home), not
an explanation for completed transcript differences. No exact causal pixel
partition or blanket “time/version blocked” classification is claimed.

### Verification, navigation and gaps

```sh
node scripts/tui_capture/check_compaction_evidence.mjs evidence/tui/recovery-v00 --final --quiet
node scripts/tui_capture/analyze_compaction_frames.mjs evidence/tui/recovery-v00 --final --quiet
```

Both exit 0. Derivative reports are `compaction-validation-final.json` and
`compaction-visual-measurements-final.json`, preserving historical/fresh reports.
Node syntax checks for validator/analyzer/probe, existing frontend and geometry
checks, and `git diff --check` all exit 0. No owned bridge/capture processes remain
(`pgrep` has no matches). Cargo ownership is released after final captures; this
pass changed only capture tooling/docs/evidence, not Rust, `.opencode`, Git commits
or pushes.
The latter includes every frame's per-row and field counts and marker positions.
Full evidence paths:
`compaction20260927-{15,16,17}/{upstream,oc}/compaction-STATE.{cells.json,png,txt,render.json,vt}`.
Each root also contains full grid/PNG diff reports, immutable source manifest,
capture lock and commands; protocols contain complete real request bodies and
read-only snapshots.

Running observation is **held partial-summary Markdown plus ellipsis with
animations disabled**, and an actual top-level active-session glyph. No animated
Braille cadence, FPS, idle wakeups or post-terminal deadline claim is made.
Production `provider_native` has no registered strategy; none was fabricated.
These canonical captures do not qualify legacy `reserved`/
`preserve_recent_tokens` precedence against native `buffer`/`keep.tokens`, ordered
conflict diagnostics, or explicit unsupported `prune`. Those remain config
differential/entry-point gaps. Release, all threshold boundaries and adversarial
redaction/admission remain separate gates. VIS34 full parity remains open.

## Fresh post-fix qualification — attempts 11–14

**Attempts 12/13/14: both actual binaries PASS the bounded manual, unchanged
usage-threshold and known-overflow lifecycle, including restart and conversation
Undo/Redo. Full-frame parity remains DIFFERENT, with measurable correctable UI
differences.** The historical 07 failure below remains immutable.

All fresh captures run `cargo build --locked` through `--build-oc true` (build
exit 0), against the same pinned original. Native SHA-256 for 12/13/14:
`72c802b4b63123924d6731f9c88c002b3e39862c98ef8b2db1d88b744a87ae30`.
Their production source manifests match exactly; HEAD remains `b9760200` with
the owner's dirty backend/UI. Capture locks now carry compaction-specific
qualification and `animations=false`.

Exact invocation template (substitute the three rows below):

```sh
node scripts/tui_capture/capture.mjs --compaction true \
  --compaction-trigger TRIGGER --geometry true --sidebar hide --sample short \
  --columns 120 --rows 40 \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --output evidence/tui/recovery-v00/compaction20260927-SUFFIX
```

| Trigger / suffix | Requests per side | Summaries / real tools per side | Full comparisons | Probe / exits |
|---|---:|---:|---|---|
| manual / 12 | 11 (1 title, 7 main, 3 summaries) | 3 / 1 | 38 DIFFERENT | Both PASS; two natural exit 0 per side |
| threshold / 13 | 7 (1 title, 5 main, 1 summary) | 1 / 0 | 20 DIFFERENT | Both PASS; two natural exit 0 per side |
| overflow / 14 | 8 (1 title, 6 main including rejected request, 1 summary) | 1 / 0 | 20 DIFFERENT | Both PASS; two natural exit 0 per side |

All three runner exits are 1 because of visual differences, with **no BLOCKED
or INVALID comparisons in 12–14**. There are 78 full comparisons across 39 paired
states: 78 actual full styled-cell captures and 78 full PNGs. Each grid is 120×40;
each decoded PNG is 1011×640 (647040 pixels). No timestamps, titles or dynamic
cells were masked.

### Behavior and fixture identity

- Manual `/compact` plus actual palette click while the sleep tool is running
  coalesce to one queued operation. The palette ACK dismisses on both sides;
  no `palette-remained-open` observation/Esc workaround is needed. Read-only DB
  snapshots prove tool started/running while queued and completed at summary.
- The unchanged third primary measurement is **23000 input + 1800 output =
  24800**, followed by the newly submitted prompt. Canonical actual config on
  both sides is `{"auto":true,"keep":{"tokens":0},"buffer":20000}`,
  context 40000/output 2048. Threshold 13 emits exactly one summary **before**
  the next actual main request; that request has summary/current prompt and no
  `ARCHIVE-1-` prefix. No threshold was lowered. Overflow 14 has exactly one
  actual `context_length_exceeded` rejection, then summary and rebuilt request.
- Independent validation compares the first three actual seed request inputs
  and completed usage against 07. They match. **Tooling files are not byte
  identical to 07**: snapshot coverage and the nonempty filesystem sentinel were
  added before 10. Provider/bridge hashes match 10 exactly; the fresh probe adds
  restart/Undo/Redo observations. Hashes and this distinction are recorded in
  `compaction-validation-fresh.json`; no byte-identical-07 tooling claim is made.
- Actual summary usage remains 1234 input (234 cached), 321 output (21 reasoning).
  Raw completed rows and nonempty workspace bytes survive. Failure/cancel in 12
  install no checkpoint. Reopen retains the checkpoint in all three modes; the
  next actual restarted main request retains summary/recent prompt and excludes
  the old prefix. Automatic runs remain at **one** summary after restart.
- Real `/undo` stages the latest restart exchange and `/redo` restores it. These
  controls make **zero provider requests and zero additional tool calls**;
  raw rows and workspace bytes remain unchanged. This exercises the latest
  post-checkpoint exchange, not every boundary crossing across a checkpoint.

### Visual measurements and actionable causes

`compaction-visual-measurements.json` lists every fresh frame, full differing-cell
and decoded-pixel counts, overlapping field counts and exact 1-based marker
rows. `analyze_compaction_frames.mjs` derives these from immutable whole grids.

| State | Different / 4800 cells | Symbol differences | Style-only cells | Different / 647040 pixels |
|---|---:|---:|---:|---:|
| 12 manual running-1 | 1388 | 769 | 619 | 129381 |
| 12 manual completed | 1740 | 901 | 839 | 165627 |
| 13 threshold running | 1174 | 798 | 376 | 88222 |
| 13 threshold completed | 1351 | 753 | 598 | 116558 |
| 14 overflow running | 835 | 573 | 262 | 75251 |
| 14 overflow completed | 1253 | 664 | 589 | 114191 |

Correctable production UI observations (not attributed to irreducible timing):

1. **Markdown spacing:** original completed heading/list rows are 18/20, 22/24,
   26/28; native manual rows 21/22, 24/25, 27/28. Native lacks one blank row after
   each heading. Both display the same real streamed SUMMARY body; this is render
   rhythm, not different generated content. Running Objective/checkpoint is
   original 26/28 versus native 28/29.
2. **Threshold chronology:** original next-user prompt is row 13 before divider
   row 16; native next-user prompt is row 27 **after** divider row 15 and summary
   rows 17–24. This is a semantic anchor/order difference, not merely a vertical
   shift. Overflow places the user before summary on both sides (13 versus 16).
3. **Manual placement is now chronologically aligned:** both put the held user
   and tool before summary and held continuation afterward, at row 30 on both.
   Dividers differ vertically (16 versus 19) because preceding tool chrome and
   summary spacing differ. The earlier report's blanket manual order observation
   must not be carried forward to fresh 12.
4. **Tool chrome:** original displays `(no output)` and `Command exited with code
   0.` for its actual admitted shell call. Native renders the actual bash call
   without those rows. This changes the visible archived tail at sticky bottom.
5. **Attribution/metadata:** original has `Build ·` in assistant attribution and
   prompt metadata; native lacks it in this campaign and adds `tok/s` to assistant
   attribution. Missing profile label and extra throughput chrome are correctable
   shape/content differences; they are not justified by runtime duration.
6. **Styles:** manual completed field differences: fg 894, bg 1044, modifiers 28;
   threshold completed fg 720, bg 696, modifiers 28. Counts overlap symbol changes
   and shifted rows, so they are not an isolated palette-error pixel budget.
7. **Failure detail:** original shows actual `VIS34 bounded summary failure`;
   native shows generic `summary request failed or invalid response`.

Inherently run-dependent values include real duration and throughput numerals.
Pinned original v2.0.12 versus native 0.1.0 version branding also contributes in
Home. These remain in equality comparisons. They do **not** explain blank rows,
user/summary ordering, metadata presence or theme roles. No exact causal pixel
allocation is claimed: styles/geometry overlap and full frames are unmasked.

### Fresh harness failures and checks

Attempt **11** preserves both real manual completion/restart observations and
palette-dismiss success, but the new Undo predicate expected plural `messages
reverted`; both binaries correctly displayed singular `1 message reverted`.
The predicate timed out; diagnostic captures/teardown remain retained. Its failed
grid comparison additionally reports INVALID; it is not included in clean fresh
qualification. An outer 240-second command timeout occurred after runner output.
Attempt 12 fixes only the singular/plural harness predicate.

The first fresh validator run also correctly rejected an overstrong byte-identical
07 fixture assertion. It was replaced by explicit unchanged seed input/usage
comparison plus recorded hashes and the already-existing 10 snapshot/sentinel
changes. No capture or threshold was rewritten to pass this check.

```sh
node scripts/tui_capture/check_compaction_evidence.mjs evidence/tui/recovery-v00 --fresh
node scripts/tui_capture/analyze_compaction_frames.mjs evidence/tui/recovery-v00
```

Both exit 0. Fresh validation status is
`PASS_BEHAVIOR_VISUAL_DIFFERENCES_REMAIN`; historical validation is preserved.
Node syntax checks for probe/validator/analyzer, existing `check_frontend.mjs`,
`check_capture_geometry.mjs`, and final `git diff --check` all exit 0. No owned
bridge/capture processes remain (`pgrep` has no matches). Cargo ownership is
released after these source-built captures; no additional Cargo tests or product
source edits were performed in this fresh tooling-only pass.
Canonical config path/hash and selected safe leaves are recorded from actual
isolated config files. No existing config-alias capture flag is available;
legacy aliases/prune normalization remain outside this capture qualification.
Release, animated Braille/deadlines/idle wakeups, the entire normalization matrix,
and all automatic boundary/guard variants remain unqualified. Production
`provider_native` remains unsupported: no fixture capability is invented.

## Historical qualification — attempts 01–10 (preserved)

**Manual lifecycle and known-overflow recovery work in the exercised actual binaries.
Usage-driven automatic threshold parity fails on native. Full styled-grid/PNG
parity is DIFFERENT; VIS34 remains open.** This report qualifies capture/tooling
and bounded observations, not the full product.

## Source and execution identity

- Actual checkout HEAD: `b9760200c8c782b61382520fd5c2c6943fc6f7fe`, rather than the
  initial requested `343d060b`. Existing dirty compaction backend/UI was retained.
- Reference: pinned v2.0.12 commit `2670273ff17da96f85c5826ced57aa1b368754fa`,
  `/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`,
  SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
- Native: `/home/opencode/ai/oc/target/debug/oc`, source-built by each attempt
  except 09 with `cargo build --locked` through `--build-oc true`.
  Final source-built manual attempt 10 SHA-256:
  `af00c4e0b52155db98eba889e4cc113ce717cfc5b999b68ec3762e8b546473b7`.
- Each attempt has `source-manifest.json`, `capture.lock.json`, `commands.json`,
  copied external npm lock, both binary hashes, per-side bridge specs, complete
  fake-provider requests, PTY inputs/output, and immutable captures. Manifests
  include unstaged/new Rust modules and omit authoring `.opencode/` discovery.
- Manual 10 and overflow 08 have identical production source-input hashes.
  Capture tooling evolved independently; each attempt seals its actual version.
- This work edits capture scripts/docs/evidence only. No Rust, `.opencode/`, Git
  commit, push, or donor implementation edits were made by this capture task.

## Exact commands and results

Final bounded manual execution:

```sh
node scripts/tui_capture/capture.mjs --compaction true --geometry true \
  --sidebar hide --sample short --columns 120 --rows 40 \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --output evidence/tui/recovery-v00/compaction20260927-10
```

Result: build exit **0**; both bounded behavioral probes **PASS**; runner exit
**1**, because **34/34** full comparisons (17 states × grid/PNG) are DIFFERENT.

Automatic campaigns used the identical base flags and `--build-oc true`:

| Additional flag | Output suffix | Actual behavior | Full comparator results |
|---|---|---|---|
| `--compaction-trigger threshold` | `07` | OC2 PASS, native FAILED/no compaction | 8 DIFFERENT, 6 missing-pair BLOCKED |
| `--compaction-trigger overflow` | `08` | Both PASS, one overflow + one summary + one rebuilt main request | 12/12 DIFFERENT |

Both runner exits are **1**. The native threshold timeout was followed by forced
diagnostic teardown (exit -15), not relabeled a clean successful exit.

Nearest existing checks, serial Cargo ownership:

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 cargo test --locked -p oc-adapters compaction --lib
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 cargo test --locked -p oc-tui compaction --lib
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 cargo test --locked -p oc compaction
node scripts/tui_capture/check_compaction_evidence.mjs evidence/tui/recovery-v00
```

- Adapter: **15 passed**, 0 failed; TUI: **3 passed**, 0 failed.
- Binary filter: exit **0**, **zero matching tests**; not a binary compaction-test PASS claim.
- Independent evidence checker: exit **0**, `PASS_WITH_RECORDED_PRODUCTION_GAP`.
  Its derivative `compaction-validation.json` preserves the native threshold
  failure. This is evidence consistency, not parity success.
- `node --check` for capture/probe/checker, Python `py_compile` for bridge/fixture,
  `node scripts/tui_capture/check_frontend.mjs`,
  `node scripts/tui_capture/check_capture_geometry.mjs` and `git diff --check`:
  all exit **0**. No owned compaction bridge processes remain (`pgrep` returned
  no matches); the campaign evidence totals approximately 162 MiB.
  No workspace/release qualification was attempted by this capture task.

## Actual request assembly and source grounding

Pinned `opencode/packages/core/src/session/compaction.ts:46–85,364–386,596–641`
defines the Markdown summary format, user buildPrompt and causal prefix/tail
split. `:174–207,745–765` defines usage-anchor token estimation and threshold
guards. The actual OC2 summary request contains structured user/assistant history
plus the final real user summary instruction, retains the admitted shell tool
definition, and carries its system instructions in the Responses `instructions`
field. Captured `roles` describe `input`; they do not omit `instructions` from the
recorded request or imply the original had no system instructions.

Native `crates/oc-adapters/src/runtime_compaction.rs:30,446–465` instead submits
a Developer summary instruction and a User JSON-serialized real causal transcript,
with no tool definitions. The fixture discriminates these actual role/input
shapes. The fake provider returns protocol-valid Responses events to real
requests; there is no imported/fabricated raw transcript, direct DB write or
renderer replacement. Fake main outputs intentionally seed 120 archive lines
per exchange (three real accepted main exchanges). The independent title request
is classified separately. All requests are offline; no live secrets are involved.

## Final manual behavior (attempt 10)

Each side makes **11 actual provider requests**: 1 auxiliary title, 7 main
requests including the real tool continuation and restart continuation, and 3
summarizer requests (successful, failed, cancelled). Exactly **one real tool**,
`sleep 25`, executes. Each side exits naturally with code 0 twice.

1. Three seeded main exchanges complete; raw SQLite rows are observed read-only.
2. During the real sleep tool, `/compact` and palette **Compact session** are
   both activated. The queued snapshot records **one** pending operation:
   original `session_inbox` length 1; native `session_compactions` length 1 in
   queued state. No summarizer request has arrived yet.
3. At the held summary stream, original durable tool state is `completed`, and
   native `tool_operations.state` is `completed`, versus running/started in the
   queued snapshot. This independently proves safe-boundary delivery.
4. The real summary request streams the first 80 characters and remains held.
   Two full running frames show the actual partial Markdown summary and `⋯`.
   Releasing the stream delivers the rest and the completed Responses usage.
5. Completed usage is **1234 input**, including **234 cached**, and **321 output**,
   including **21 reasoning**, from the summary request itself. Both dividers
   display `Compaction · 1.2K in · 321 out`. This is not context size.
6. The next actual main request contains `VIS34-CHECKPOINT`, current user prompt
   and retained recent context; the older `ARCHIVE-1-` prefix is absent. Raw
   previously completed message rows remain byte-for-byte identical; every
   recorded Responses function output has its causal function call.
7. HTTP summary failure and a genuinely interrupted held summary install no new
   checkpoint. Original completed-compaction rows and native `session_checkpoint`
   are unchanged relative to the pre-failure checkpoint.
8. The original and native same-root applications exit naturally, relaunch,
   reopen the real saved session, show the checkpoint, and submit a new actual
   provider request. It still contains the checkpoint and excludes old prefix.
   No tool is reexecuted. The saved checkpoint is unchanged after reopen.
9. A real seeded `vis34-owner-approved.txt` has the same nonempty filesystem hash
   map before/after compaction. SQLite observations use `?mode=ro`; capture code
   does not install state. The tool only sleeps; it writes no fixture files.

The validation file independently checks capture SHA-256, complete 120×40
dimensions, tool boundary/coalescing, raw rows, checkpoint preservation, request
pairs, actual usage, process exits, filesystem sentinel and source-input identity.

## Production gaps and visual observations

### Usage threshold mismatch — reproducible, not a harness rejection

Attempt 07 config: `auto=true`, `keep.tokens=0`, `buffer=20000`, catalog context
40000/output 2048. First two primary responses report input 6000/output 1800;
the third reports **input 23000/output 1800**. Both applications receive the same
usage anchor. On the next submitted user request, pinned OC2 emits one genuine
summary request, checkpoints and continues. Native emits **zero** summary
requests and sends the next real request with the old `ARCHIVE-1-` history.

Current native `runtime.rs:2066–2077` uses `estimate_tokens` over local serialized
input/history/tool definitions as `boundary_estimate`; OC2
`session/compaction.ts:174–192` uses the latest primary-response usage plus new
parts. Existing nearest threshold tests pass but do not qualify this real
usage-driven scenario. Next engineering step: implement the pinned usage-anchor
estimate/guards and rerun this unchanged campaign; do not lower its threshold to
hide the mismatch.

### Other visible differences

- Native palette activation while compaction is pending leaves the Commands
  dialog open/reset; the probe records `palette-remained-open` and sends Esc to
  regain transcript visibility. OC2 dismisses the dialog. This is a production
  interaction observation, not a reason to fake selection.
- Original completed summary headings have a blank row before each list/body;
  native headings and first list items are adjacent. Visible vertical rhythm,
  summary placement relative to held-tool continuation, assistant attribution,
  tool chrome and prompt metadata differ in the whole frame.
- Original failure body displays `VIS34 bounded summary failure`; native displays
  `summary request failed or invalid response`. Both have a genuine failed
  divider and preserve the existing checkpoint.
- Completed/cancelled divider strings and formatted summary usage match in the
  exercised states, but their matching strings do not close whole-frame parity.

### Known-overflow recovery

Attempt 08 sends exactly one actual HTTP 400 Responses error with code
`context_length_exceeded` on the next primary request. Both binaries make one
real summary request, preserve raw history, install a checkpoint and issue a
rebuilt main request containing summary/current prompt without the old prefix.
Both finish naturally with exit 0. This is the bounded known-overflow path only,
not the entire error/retry/auto=false matrix.

## Preserved attempts and harness failures

| Attempt | Result / reason |
|---|---|
| 01 | Source build PASS; original summarizer incorrectly rejected because it retained tool definitions. Native fixture incorrectly selected a tool for a summary request whose serialized history mentioned the held prompt. Outer 120-second command timeout closed the browser; all diagnostics retained. |
| 02 | Palette readiness/coalesced predicate raced the 12-second tool; native modal and original running/queued diagnostics retained. |
| 03 | Actual running summaries captured. Original fixture tried to issue sleep again after its earlier call was outside compacted context; native palette stayed open and obstructed completion predicate. |
| 04 | Native full lifecycle PASS. Original actual failure card had “failure”, while harness expected “failed”; predicate timeout retained. |
| 05 | Native full lifecycle PASS. Adjacent raw Escape bytes were ambiguous to original terminal key parser; cancellation predicate timed out. |
| 06 | Both source-built manual lifecycles PASS; 34/34 comparisons DIFFERENT. Workspace hash map was empty, so this was not sufficient nonempty-file preservation evidence. |
| 07 | Genuine usage-threshold production mismatch documented above. |
| 08 | Both source-built known-overflow lifecycles PASS; 12/12 comparisons DIFFERENT. |
| 09 | Both manual lifecycles plus nonempty sentinel PASS, but existing binary SHA changed after binary-filter Cargo tests; `--build-oc false` association remains unqualified. No build attestation was copied from 08. |
| 10 | Final fresh source-built manual lifecycle, safe/coalesced DB states and nonempty filesystem verification PASS; 34/34 comparisons DIFFERENT. |

No failed attempt or raw lock was rewritten. Later tooling makes future compaction
qualification metadata mode-specific; recorded locks 01–10 retain the older
generic runner qualification prose. For their compaction interpretation use
per-side `compaction-checks.json`, actual bridge specs/protocol and this report.

## Full-grid/PNG navigation

Final complete pairs live under
`compaction20260927-10/{upstream,oc}/compaction-{STATE}.{cells.json,png,txt,render.json,vt}`.
States include `queued`, `palette`, `queued-coalesced`, `running-1`, `running-2`,
`completed`, `next`, `failed`, `cancel-running`, `cancelled`, `reopened`,
`restart-next` and three seed frames/Home/held-tool. Whole-grid and whole-PNG
reports are `compaction-{STATE}.{grid,png}-diff.json` at the attempt root.
These are full terminal screenshots, with no masked titles/timestamps/dynamic
cells and no selected comparison crop.

Overflow pairs: `compaction20260927-08/{upstream,oc}/compaction-overflow-{running,completed}.*`.
Threshold success versus native diagnostic: `compaction20260927-07/upstream/compaction-threshold-*.png`
and `compaction20260927-07/oc/compaction-failure.png`.

## Limits — no full VIS34 claim

- No provider-native strategy is registered in production. No fake capability was
  installed; `Provider compaction` is not accepted as real-route evidence.
- Running captures use animations=false/ellipsis. Animated Braille cadence,
  post-terminal animation deadlines, CPU/wakeups and pending-input preservation
  are not qualified by these frames.
- No release build, TS/Rust config-normalization differential matrix, shared
  legacy `prune` entry-point check, all threshold boundaries, usage guards after
  restart, automatic failure/cancel matrix or auto=false overflow suppression is
  claimed. Manual auto=false and actual manual restart are exercised.
- Existing adapter tests cover DCP/Undo/Redo/fork invariants; the new PTY campaign
  does not additionally exercise those controls. Separate instruction-update
  events are not fabricated or claimed as compaction.
- Raw transcript remains durable; this campaign is fake-provider functional
  evidence, not real-model summary quality or paid-provider capability evidence.
