# T44 VIS26 — `@` file mention, paired PTY evidence

**Status: OPEN.** Current native behavior is exercised by a real bare-`oc` PTY test, and both binaries display the fixture file and insert a Location-relative mention on Tab in the paired capture. All six required trigger/filtered/after-Tab full-frame styled-grid **and** PNG comparisons in the final attempt are **DIFFERENT** (comparator exit 1). This is not VIS26 pixel parity or T44 completion.

## Implementation and behavioral evidence

The current uncommitted implementation routes an inline `@` query from the Home or session composer through `crates/oc-tui/src/autocomplete.rs`, `app.rs`, `shell.rs`, `crates/oc/src/tui_cmd.rs`, and an application-owned `FileSuggestionsSnapshot` in `oc-core`/`oc-adapters`. `Files::suggest` in `crates/oc-adapters/src/files.rs` returns sorted Location-relative file paths with bounded traversal/results, skips symlinks and excluded directories, and does not read file contents. The UI rejects stale edit/view/Location/owner-generation responses; Tab inserts an ordinary textual `@path` (with a separator) and closes the list. This does not synthesize a structured prompt part or implicitly call `read`. The native list omits upstream skills and agents, a documented supported difference under the T44 amendment (non-primary agent exposure belongs to T45).

`crates/oc/tests/pty_t42.rs::vis26_bare_oc_mention_tab_submits_durable_location_relative_prompt` **passed after the filtered-result wait fix** in the supplied workspace gate output. The test starts a bare actual binary in Location A, excludes an application-data-root symlink/private file from the empty-query suggestions, filters to `vis26-alpha.txt`, selects with Tab, and submits `@vis26-alpha.txt check`. It checks the real Responses request, one durable user/assistant history pair, the session's Location-A binding, and that a switch to Location B offers `vis26-beta.txt` rather than an old A result. The capture below tests pre-submit draft/menu behavior; this PTY test supplies the separate submit/durability evidence.

## Immutable paired attempts

All three attempts are retained under `evidence/tui/mention-20260924-{01,02,03}/` with independent `upstream/` and `oc/` PTY input, protocol, raw VT, styled cells, PNG, predicate JSON, comparator reports, commands, and `capture.lock.json` (the first stops before session capture). `-01` timed out waiting for an *empty* cleared Home draft on **both** sides: the cleared Home actually shows its placeholder; its `failure-diagnostic` is a failed-state frame, not a session result. `-02` completed both routes before the native selected-file row fix: the native filtered row displayed `@fixture-note.txt`, whereas upstream displayed `fixture-note.txt`; the original trigger menu was also recorded absent in that attempt. `-03` records the current post-fix row and the observed original trigger sections; earlier attempts have not been rewritten.

Reproduction command recorded in `-03/commands.json` (runner exit **1**, from differing comparisons):

```sh
node scripts/tui_capture/capture.mjs --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --geometry true --sample tools --sidebar hide --agent-profile true --columns 120 --rows 40 --mention true --output /home/opencode/ai/oc/evidence/tui/mention-20260924-03
```

`-03/capture.lock.json` records pinned original source `2670273ff17da96f85c5826ced57aa1b368754fa`, original executable SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` (`opencode v2.0.12`), native executable SHA-256 `36eca8846b951ae8388d2c341940dcaefd0b3fbba1bd8a70489f9894cb0d6bc5` (`oc 0.1.0`), fixture SHA-256 `16a8757ab6b87d86b9db94e5b5beda3c1117cfa15997522339b86479524bc143`, and common paired environment ID `75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`. Profile: 120×40, truecolor xterm, dark `opencode`, hidden sidebar, horizontal tabs, DejaVu Sans Mono 14 / xterm.js 6; no version/elapsed-time masking. The source lock identifies native HEAD `574efe667a5114d5950ad831e0ee9d702fde0994`, tree `992eab980b0cebf97f8a970bc24b57320bdf14e4`, **dirty** diff SHA-256 `3baa82d53a94c5d68f53059f019be3e5d1d6d5dc01846c0ec7d281ec668afba8`, and source manifest SHA-256 `bbca8886d5bf30a5b86bd8e6ac905e56ac1e053a93fbe5f663e1e7787cca75f4`. Its `build_provenance` explicitly says **"Existing binary; build/source association not attested by this run"**; the hash of a debug executable and the dirty source lock are not a final-code SHA or an attested final build.

In `-03/{upstream,oc}/mention-checks.json`, Home and session both show `@` and an open menu at trigger; the original lists `@opencode`, `@report` (skills), `@general`, `@explore` (agents), **not** the fixture file in the four visible rows, while native shows only `fixture-note.txt`. Filtering to `@fixture` produces the same visible file-row text and row position on both sides (Home y=19, session y=32). Tab produces the same visible `@fixture-note.txt` draft, hides the menu, and changes the grid on both sides; clearing the draft sends no provider request. Cursor coordinates coincide at each paired stage. The recorded `no_provider_request=true` predicate means no *new* request during the mention keystrokes: Home counts remain 0; session counts remain 3 completed fixture requests on each side (`provider_contract=true`), not zero lifetime requests.

### Unmasked `-03` whole-frame comparisons

Each grid comparison checks **4800** cells; each PNG checks **647040** decoded RGBA pixels. Values below are differing cells / pixels, with **DIFFERENT** and exit 1 in *both* modes for every row; see the correspondingly named `-03/*.grid-diff.json` and `*.png-diff.json` and the actual `-03/{upstream,oc}/*.cells.json` / `*.png`.

| Frame | Styled cells | PNG pixels | Visible cause / residual |
| --- | ---: | ---: | --- |
| Home baseline | 27 | 1247 | Actual placeholder/version differences. |
| Home trigger `@` | 306 | 39939 | Original skills/agents sections versus native file-only row; additional styles/version. |
| Home filtered `@fixture` | 65 | 538 | Row symbols and geometry align after fix; selection border/background/foreground styles still differ, plus version. |
| Home after Tab | 23 | 1065 | Same draft text/cursor; mention foreground/modifiers differ, plus version. |
| Completed session baseline | 2 | 111 | Real elapsed-time digits at grid x=38–39, y=10. |
| Session trigger `@` | 466 | 61602 | Original skills/agents versus native file-only row; elapsed-time digits/styles. |
| Session filtered `@fixture` | 102 | 303 | Matching row text/position; selected-row foreground/background styles differ, plus elapsed-time digits. |
| Session after Tab | 19 | 890 | Same draft text/cursor; mention foreground/modifiers differ, plus elapsed-time digits. |

The small filtered PNG counts and matching plain-text rows do **not** waive full-frame styled/PNG failures. In particular the after-Tab text is styled differently, and real version/time/random Home placeholder values are not normalized or masked. `-03/capture.lock.json` labels the run `DIAGNOSTIC_BASELINES_ONLY`, with unfrozen wall-clock measurements; it does not attest final source-to-binary provenance.

## Checks and remaining gate

Handover supplied the following **PASS** workspace command and output at `/home/opencode/.local/share/opencode/tool-output/tool_0d45c85ca001255HdpW9QsyiGa` (the log includes the named PTY test `... ok`, 31/31 in its PTY target, 248/248 TUI unit tests, 0 test failures, and successful clippy/build/docs/journal output):

```sh
cargo fmt --all -- --check && CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924 cargo test --locked --workspace --no-fail-fast && CARGO_BUILD_JOBS=3 cargo clippy --locked --workspace --all-targets -- -D warnings && CARGO_BUILD_JOBS=3 cargo build --locked && python3 scripts/check_docs.py && python3 scripts/progress.py check && node --check scripts/tui_capture/capture.mjs && git diff --check
```

That green workspace gate demonstrates the tested implementation, **not** external visual parity. VIS26 and active T44 remain **OPEN** pending a final-source-qualified paired run with matching whole styled frames, PNG and interaction; no final code SHA or VIS26 PASS is asserted here.
