# Attempt 03 — post-hover-fix capture closeout

**BOUNDED_BEHAVIOR_VERIFIED_WITH_WIRE_DIFFERENCE; VIS07_NOT_PASS. Cargo idle.**

## Actual completion and former race

All ten unchanged scenario/geometry pairs were freshly built and captured:
chip and extra at 79/80/120/121×40, plus raw-end repeat/navigation and real
suffix-space repeat/navigation at 120×40. All twenty sides are `OBSERVED`.

The native suffix-space submission now finishes: exactly one valid main request
and one valid auxiliary title, both completed, then a stable
`leader-repeat-enter-restored` capture containing the actual assistant answer
and empty composer. Original completes the same bounded exchange. The dedicated
analyzer explicitly requires both completion events and a stable restored frame;
it also checks both navigation runs' actual raw VT contains no Rust `panicked at`.
Both suffix-expanded copies reach the wire intact; six actual visual Up/Down
samples return to the pre-navigation caret in each binary.

Cold actual-chip frames now wait for one visible `[Pasted` chip plus five
unchanged full styled-grid/cursor polls, 200 ms apart, before the normal capture.
Pending leader samples retain the previous short delay. This is a settlement
predicate, not frame replacement or masked comparison. For chip-80 the actual
settlement took 2020.873449 ms original / 2191.623374 ms native; both settled
cursors are `(13,22)`. The normal/pending/restored symbol/cursor/bold checks pass,
and every capture is stable. Attempt 02's unstable original frame and actual
native panic remain preserved in its separate immutable directory.

## Counts and actual differences

- 404 full per-side captures, 202 paired full grids and 202 paired full PNGs.
- All 202 grid comparisons and all 202 PNG comparisons are `DIFFERENT`;
  zero `EQUAL`, `BLOCKED` or unstable captures. No cells or pixels are masked.
- 16 paired cursor differences: four each in chip-79, chip-121, extra-79 and
  extra-121. All other runs have zero cursor differences.
- 24 valid isolated local fake-provider requests: 12 original + 12 native;
  12 main + 12 title; zero rejected requests and zero live/remote requests.
- Zero current behavioral failures and zero provenance failures. All ten runner
  exits remain 1 because the full visual comparisons differ; analyzer and
  summarizer exit 0 do not mean VIS07 PASS.

All four chip-normal/pending/restored key grids differ by six version cells;
actual mouse-expanded grids differ by seven cells. Longdraft restored grids
differ by 1/2/3/6 cells at 120/121/79/80 columns. Raw-end repeat expanded/pending
grids differ by seven cells and restored by three. Suffix two-chip normal differs
by six, first expansion by 25, second expansion/pending by 20, restored by 16.
Full differing rows, per-field counts, PNG pixel counts/bounds and cursors remain
in the dedicated analysis and per-frame comparator reports. Versions, durations,
footer, paths, tokens, styled blanks and all other actual differences remain.

Actual pending RGB on both binaries: draft `#eeeeee` → `#808080`; stripe/agent
`#5c9cf5` → `#484848`. The chip retains foreground `#0a0a0a`, background
`#f5a742`, and bold. Restored colors return to the observed normal values.

## Exact wire gap — remaining unapproved, no waiver

Define the exact strings, using JSON escapes for newlines:

```text
P = "VIS07 visual UpDown draft αβ caret-middle preserving every word with a boundary actual separator "
T = "VIS11-PASTE-0\nVIS11-PASTE-1\nVIS11-PASTE-2"
raw-end native   = P + T
raw-end original = P + T + " "
suffix native    = P + T + " " + T
suffix original  = P + T + "  " + T + " "
```

Native preserves the original typed/pasted bytes; original adds a chip spacer
outside each extmark. Both actual input arrays are retained without trimming in
the protocols, dedicated analysis and summary. The fixture accepts these
explicitly different observed wires separately. This observed difference remains
an **unapproved parity gap**, not acceptance authorization or a VIS07 waiver.
The longdraft wire is exactly identical in all eight main requests:
`VIS11 full draft αβ caret-middle preserving every wVIS11 Enter bounded actual requestord`.

## Exact native source association, including the parent's documentation commit

Every run executed its own `--build-oc true` / `cargo build --locked` successfully.
All ten runs use the same freshly built native executable:

- Path: `/home/opencode/ai/oc/target/debug/oc`.
- SHA-256: `c0343d6e359bdbdff3c9fe1baa7158c85ecc37ce84f2fdecdb45a9308fe9a1bc`.
- Sorted Rust-input JSON digest:
  `ac20f8cf300f9fb4b0d50170ca53b9a8e76e3571f89f613c45c646b9cbe590b5`.
- Complete canonical capture-time source-manifest digest:
  `9f98308ebed9304e35ef1c79bbd63b77c16cc044033f8d95aac590a5038e6962`.

The parent committed documentation during the serial campaign; capture-time HEAD
is therefore per-run, not one campaign-wide commit. `report.md` names the first
captured HEAD. The exact association in `summary.json`/locks is:

| Runs | Captured HEAD |
|---|---|
| chip-120, extra-120, repeat-nav-120, suffix-space-120, chip-79, extra-79, chip-80 | `5a2ec11411ab4276d2e13cf4379065aadc64475b` |
| extra-80, chip-121, extra-121 | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` |

Current HEAD at closeout is `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2`
(`docs(startup): require OC2 service failure and MCP config parity`). Rust and
capture-tool input manifests remained identical across all ten runs. Complete
per-run dirty diffs/digests, including documentation changes, remain sealed;
neither captured HEAD alone contains the uncommitted TUI fix.

All four actual TUI source digests match current source at summarization. Relative
to attempt 02 only `app.rs` changed among these four inputs:

| TUI input | Captured SHA-256 |
|---|---|
| app.rs | `477210f63c6a045e512427f96ed459eb7c894af1948d8632e442c8432c04d42c` |
| approval_view.rs | `8936b7b4d6c55d424de6d1194ea784d54ed4f5ee0a3d334f6e867f47b20df711` |
| editor.rs | `d39c42ce8257997d57bc5bc0943473c557d13c60548ae37329302f7374d75419` |
| shell.rs | `8ed45964899c064760b1bea42e6a2a9bfd33281f0b97cf8d4ef498331ae622ae` |

The actual app source has `rows.get(row)?` before `tool_hover_range` in
`paint_transcript_at` (`app.rs:3033–3039`). The parent's reported regression test
PASS is separate from this agent's executed capture checks.

Pinned original executable:
`/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`,
SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
source commit `2670273ff17da96f85c5826ced57aa1b368754fa`.

## Executed checks and handback

JS syntax checks for capture, leader, navigation, serial runner, dedicated
analyzer and summarizer; Python bridge AST; actual frontend and geometry checks;
and `git diff --check` passed before execution. Analyzer verified all build/source
unchanged/seal/geometry/pinned-binary checks. Every build/capture source seal is
unchanged before and after its operation. Tool changes were made before the first
capture, and no captured file or historical evidence was overwritten.

Only scripts and new attempt-03 evidence were edited by this delegated work;
Rust, acceptance, `.opencode/` and task state were not edited. The independent
16-full-PNG sample review is recorded in `visual-review.md`. Cargo/rustc and owned
capture/bridge/campaign processes were absent at handback. Continue from
`VIS07_NOT_PASS`: resolve/qualify the unapproved raw-space gap and remaining
mandatory full-frame cases before any acceptance PASS claim.
