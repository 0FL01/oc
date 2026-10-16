# T44 — current cursor temporal qualification

## Result

T44 remains active. Current-source temporal probes use delivered `ca37eca0a`,
the pinned original2.0.12, the existing real WebGL terminal frontend and unchanged
full comparators. No terminal implementation was changed on a source-only
hypothesis. This is not full VIS16/VIS31 or pixel-parity completion.

`evidence/tui/cursor-resume-20261016-01`:120×40, expanded, explicit blink,
synchronized output supported. Both sides report
`QUALIFIED_CURSOR_BEHAVIOR_ONLY` in all six composer/Search states. Native cycles
are4/3/4/4/4/4; original3/3/4/4/4/4. Owned caret, sampling and cadence are preserved,
with zero phantom states and no generation/tool replay during view actions.

`evidence/tui/cursor-resume-20261016-02`:80×24, collapsed, blink, unsupported
synchronized output. Native again qualifies all six states:3/3/4/4/4/4 cycles,
zero phantom states and preserved cadence/caret. The **original fails** its
composer-hover cadence check: observed periods1432.3/438.2/212.9ms against idle
1195.3–1211.4ms. Sampling is adequate and both sides have zero phantom-command
states; this is not relabelled a successful paired qualification or a native bug.
The failed receipt is retained unchanged.

Further completed controls use the same actual Rust inputs:

| Episode | Mode / geometry / card / synchronization | Original | Native |
| --- | --- | --- | --- |
|03|steady /160×48 /expanded /supported|qualified, all0 cycles|qualified, all0 cycles|
|04|default /120×40 /collapsed /unsupported|FAILED, hover12 cycles|FAILED, hover11 cycles|
|05|blink /120×40 /expanded /unsupported|qualified,3/4/4/4/4/4|qualified,3/4/3/4/4/4|
|06|blink /80×24 /collapsed /supported|FAILED, hover0 cycles|qualified,3/3/4/4/4/4|
|07|default /120×40 /collapsed /supported|qualified, all0 cycles|qualified, all0 cycles|
|08|default /120×40 /collapsed /unsupported, eager delivery|qualified, all0 cycles|qualified, all0 cycles|
|09|blink /80×24 /collapsed /supported, eager delivery|FAILED, hover0 cycles|qualified,3/4/4/4/4/4|

All nine receipts retain zero phantom-command states. Episode04 is an actual
native **raster** failure too, not merely an original-only control failure. Its
nonblinking cursor manager remains visible: native29/194 samples show background
instead of the cursor, including17 while the parser is hidden and12 after it is
visible; original22/206 show the same distinction. This does not establish a
hardware-blink or product implementation cause.

The native backend already writes one buffered Hide/paint/final-Move/Show frame.
Episode07's new raw PTY arrival timeline shows real kernel fragmentation of
2917/2965-byte output into2048+869/917 bytes. Consecutive arrival gaps are median
0.280ms/native and0.264ms/original. The temporal capture transport had awaited a
browser parser-callback RPC **before delivering the next available chunk**.
That can stretch a naturally split hidden-cursor paint across renderer frames;
Node and browser clocks are not subtracted from one another in this diagnosis.

The controlled acquisition experiment removed only that artificial
per-chunk callback backpressure for temporal episodes. Available bytes retain
their arrival order, content and real parser/renderer; ordinary captures retain
their prior serial path. No command coalescing, observer override or qualifier
change is involved. Actual DOM/WebGL frontend checks pass complete styled-grid
and cursor equivalence for serial versus eager writes fragmented across UTF-8
and VT control bytes. Fresh08 repeats04 with this acquisition change: both sides
now have zero cycles in every default/nonblink state. This isolates that failure
to serialized acquisition; **no native terminal fix or relaxed criterion** was
needed. Fresh09 repeats06: native cadence remains qualified while the original's
narrow-hover blink starvation persists. That failed reference control is retained.

## Checks

- Updated DOM/WebGL frontend conformance and all10 capture Python tests: PASS;
  existing SQLite ResourceWarnings remain visible. JavaScript syntax checks pass.
- Each completed episode rebuilt the actual native binary, used isolated local
  provider/MCP/Shell effects and retained the source/binary association.
- Existing checks report twelve complete temporal frames per state and no replay.
  Independent full PNG/grid/provenance audit passes on both-qualified
  episodes01/03/05/07/08. Failed02/04/06/09 remain explicitly outside that qualified
  subset, not silently relabelled successful paired episodes.
- `check_cursor_temporal.py --compare-output evidence/tui/cursor-resume-audit-20261016-01`
  validates720 full opaque PNGs /495,452,160 raster pixels, captured source/build
  association, fixed-owner RGBA and348 actual matched-phase frame pairs. Result:
  `QUALIFIED_CURSOR_BEHAVIOR_ONLY`, **pixel parity NOT_PASS**. Full comparators
  report776 DIFFERENT and48 UNMATCHED_TEMPORAL_SAMPLE results, not equality.
  Actual audit log: `.local/evidence/t44-cursor-resume-audit-01.log`.
- All capture commands exit1 for full comparison differences/unmatched real
  phase counts; behavioral qualification is recorded separately. The first
  control series stops on02; a new serial series actually runs03–06, and07
  changes synchronization only. No criteria or immutable metadata were changed.
- No product clock, blink timer, paint substitution, crop or mask was introduced.

## Risks

No external blocker. Failed controls remain evidence, not product diagnoses
without transport isolation; native stability must not be degraded to imitate
the original's narrow-hover behavior. The already
qualified Generic sticky-follow facet and explicit Shell success-status omission
remain unchanged. Existing full-frame differences remain open.

## Next

Continue frozen T44 work with structured execute presentation (VIS46): bounded
redacted producer facts, disposable progress, nested rows and the Code/Output
dialog through existing owners. Native temporal behavior is qualified in all
nine profiles except diagnosed old-acquisition04; full VIS16/VIS31 stays open.
