# T44 VIS25 — paired autocomplete keyboard probe

`scripts/tui_capture/capture.mjs --autocomplete-keys true --autocomplete-keys-rename true`
exercised real keys in isolated original/native PTYs at 120×40 (Reader/tools,
dark `opencode`, truecolor, sidebar hidden). The completed pair is
[`autocomplete-keys-20260924-02/`](autocomplete-keys-20260924-02/):
`commands.json` records the invocation and exit **1**; `capture.lock.json`
records the input fixture SHA-256
`16a8757ab6b87d86b9db94e5b5beda3c1117cfa15997522339b86479524bc143`,
shared terminal profile ID
`75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`,
separate executable hashes, capture hashes, predicates and comparator exits.
Both sides retain actual inputs, protocol, raw VT, full `.cells.json`, `.txt`,
`.vt` and `.png` frames, and grid/PNG diff JSON. No content or clock masking
was applied.

The reference is pinned opencode v2.0.12, source commit
`2670273ff17da96f85c5826ced57aa1b368754fa`, executable SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Native source lock: HEAD `f5321bb6308291d48b4d0a8f52eb5f89c3eed46f`,
tree `8f749db3b881e810f155d65d4bae08322e4938a4`, dirty diff SHA-256
`c1b9bc70e6e03ed562b5ac31697fbc8ca4e85f373232cd23d3d26959e3dfe554`
and `source-manifest.json` SHA-256
`545fcb74aef0c75c64d8ca4994bf79b050d9d1423c35ed548925442987fa4142`.
The native executable SHA-256 is
`b653ec5dc5502312114de944fe7cf97b6ce188326182224b3eddfebcc85c4eeb`;
this run used an **existing** binary (`--build-oc` absent), so the source lock
does not attest a binary-to-source build association. The runner hash for this
attempt is `bc70f2e28363a23dddcf417b87473d006e9b550dbc5ccae9db9c2f2e5a6dba42`.

## Actual actions and checks

Both `upstream/autocomplete-keys-checks.json` and
`oc/autocomplete-keys-checks.json` record **15 stages and 51 true predicates
per side**; `capture.lock.json` marks both `autocomplete_keys` entries **PASS**
and both fixture/provider contracts true. Home `/reload` showed its suggestion;
Enter displayed **Configuration reloaded**, cleared the draft and hid the menu.
After the real success toast expired, Home `/ren` showed a menu; Esc hid it
while retaining `/ren`, and Backspace cleared it. The same sequence succeeded
in the completed session. There, typing `/rename` then Enter inserted
`/rename ` (the captured text is `/rename` and the cursor is one cell past it),
hid the menu and allowed clearing the draft. The baseline provider counts were
0 requests / 0 completions / 0 invalid on Home and 3 / 3 / 0 after the
fixture-backed session turn, unchanged throughout each keyboard sequence:
**zero extra provider requests on either side**.

Menu contents differ despite those working keys. With Home `/ren`, the
original offers `/reload`, `/review`, `/btw`; native offers `/reload`,
`/review`, `/dcp-compress`. With session `/ren`, the original offers seven
rows (`/rename`, `/reload`, `/review`, `/share`, `/fork`, `/export`, `/btw`),
native four (`/rename`, `/reload`, `/review`, `/dcp-compress`). At session
`/rename`, both show `/rename` and `/review`, with different spacing/selection
styles. These are observed inventories, not assertions of equivalent commands.

## Unmasked full-frame comparisons

These are the **12** named paired frames in `-02/capture.lock.json` (including
the Home and completed-session baselines), not 11. Every grid has 4,800 styled
cells, every PNG 647,040 pixels. Each of the **12 grid and 12 PNG** comparisons
is `DIFFERENT`, comparator exit **1**; all 12 cursor-equality flags are false
for `cursor_differs` (the paired cursor positions match).

| Paired frame | Different cells / 4,800 | Different pixels / 647,040 |
| --- | ---: | ---: |
| `home` | 45 | 2,037 |
| `autocomplete-keys-home-before-enter` | 68 | 1,663 |
| `autocomplete-keys-home-after-enter` | 45 | 2,037 |
| `autocomplete-keys-home-before-esc` | 203 | 7,845 |
| `autocomplete-keys-home-after-esc` | 6 | 298 |
| `session-wide-completed` | 2 | 127 |
| `autocomplete-keys-session-before-enter` | 105 | 1,428 |
| `autocomplete-keys-session-after-enter` | 2 | 127 |
| `autocomplete-keys-session-before-esc` | 796 | 69,892 |
| `autocomplete-keys-session-after-esc` | 2 | 127 |
| `autocomplete-keys-session-rename-before-enter` | 208 | 4,469 |
| `autocomplete-keys-session-rename-after-enter` | 2 | 127 |

Home uses distinct **randomly selected real placeholders**, and its footer
shows the real version strings `2.0.12` versus `0.1.0`. After the fixture
turn, the completed session differs in its real elapsed reading (for example,
`102ms` versus `196ms` in `session-wide-completed.txt`, grid x38–39,y10);
that reading varies across frames and was not frozen. Suggestion-row inventory,
label spacing and styled selection also account for larger filtered-menu
differences. Small post-action differences are still full-frame inequality,
not permission to treat a region as pixel parity.

## Preserved first attempt and qualification

[`autocomplete-keys-20260924-01/`](autocomplete-keys-20260924-01/) is an
immutable failed **capture** attempt. Its `/reload` success toast was still
moving/expiring during the later `/ren` PNG screenshot, producing `VT grid
changed during PNG capture`: upstream `session-before-esc` and native
`home-before-esc` are marked `UNSTABLE_CAPTURE`, and both sides' runner status
is `FAILED`. Its actual partial frames and failure diagnostics remain intact;
five named grid/PNG pairs are `DIFFERENT` and five requested pairs are
`BLOCKED` for missing actual captures. This is not a product-key failure or
a parity result for the blocked frames. Attempt `-02` waited for the observed
toast to disappear on both routes before typing `/ren`; it records the
`reload-notice-expired` predicates and complete stable paired frames.

The `-02` lock labels the run `DIAGNOSTIC_BASELINES_ONLY`. Keyboard predicates
**PASS** on both binaries, but full-frame grid/PNG equality **FAILS**, so
**VIS25 and V09 are not PASS**. No broader T44 qualification follows from
this keyboard probe.
