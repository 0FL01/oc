# T44 VIS25 — paired autocomplete selection movement

The new immutable paired capture is [`autocomplete-move-20260925-01/`](autocomplete-move-20260925-01/). The real runner exited **1**: all **18** named full-frame grid pairs and all **18** PNG pairs are **DIFFERENT** (comparator exit 1). This is an interaction-check PASS on each side, **not** pixel parity or a qualified final-code result. `capture.lock.json`, `commands.json`, each `*.grid-diff.json` / `*.png-diff.json`, and the separate `upstream/` and `oc/` raw VT, cells, text, PNG, input, protocol and keyboard checks retain the observations without masking or fabricated rows.

Recorded command (120×40, dark `opencode`, Reader/tools fixture, sidebar hidden):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --autocomplete-keys true --autocomplete-keys-move true \
  --output /home/opencode/ai/oc/evidence/tui/autocomplete-move-20260925-01
```

The pinned reference is opencode v2.0.12 (source commit `2670273ff17da96f85c5826ced57aa1b368754fa`, executable SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`). The native **source lock** records HEAD `d354e84ccf5f91ed3c821b9164d39e6fdc33e939`, tree `e6f0cb2904fc49672d4b2199b536d831dce1c730`, dirty diff SHA-256 `4c775eb9bb5811d3dda9e48a574e1eac3a91f226d7afce6171d3830a36cdc8f7` and source-manifest SHA-256 `050eea5e5b858ae1c4a88c9053b32b9a586120ab9ae68980c555808c1fd910dc`. Its executable SHA-256 is `b653ec5dc5502312114de944fe7cf97b6ce188326182224b3eddfebcc85c4eeb`. Per lock, this was an **existing binary**: the run did **not** attest its build/source association. HEAD is therefore a source snapshot label, **not a qualified final binary SHA**. The fixture SHA-256 is `16a8757ab6b87d86b9db94e5b5beda3c1117cfa15997522339b86479524bc143`; both sides share terminal profile ID `75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`. The lock labels qualification `DIAGNOSTIC_BASELINES_ONLY`.

## Actual keyboard effects

Both `upstream/autocomplete-keys-checks.json` and `oc/autocomplete-keys-checks.json` contain **22 stages and 84 true predicates per side** (11 stages on each route); the lock reports `autocomplete_keys` **PASS**, `AUTOCOMPLETE_KEYS_CHECKS_PASS` and `provider_contract=true` for each real PTY. Pinned `packages/tui/src/config/keybind.ts:270–274` assigns Up/Ctrl+P and Down/Ctrl+N; `packages/tui/src/component/prompt/autocomplete.tsx:584–587` specifies wrap. After typing `/ren`, the runner sampled the slash-label cells' foreground/background on each painted option row, requiring one unique selected background distinct from the other rows. On **both Home and session**, initial index 0 → Up last → Ctrl+P penultimate → Down last → Ctrl+N index 0, while the `/ren` draft and option inventory stayed unchanged. Home had three visible options on each side (indices `0 → 2 → 1 → 2 → 0`); session had **seven upstream** (`0 → 6 → 5 → 6 → 0`) and **four native** (`0 → 3 → 2 → 3 → 0`). Home options were upstream `/reload`, `/review`, `/btw` versus native `/reload`, `/review`, `/dcp-compress`; session options were upstream `/rename`, `/reload`, `/review`, `/share`, `/fork`, `/export`, `/btw` versus native `/rename`, `/reload`, `/review`, `/dcp-compress`. Movement results do not imply equal menus.

The existing Enter/Esc sequence also passed on each route: `/reload` + Enter showed the success notice, cleared the draft and hid the menu; after the notice expired, Esc closed the `/ren` menu but retained the draft, and clearing it restored the empty prompt. Provider counts remained **0 requests / 0 completions / 0 invalid** on Home and **3 / 3 / 0** on the fixture-backed session throughout the keyboard probes; no extra provider request was recorded. `--autocomplete-keys-rename` was not used in this run, so the optional `/rename ` insertion probe was not repeated here.

## Unmasked full-frame comparisons

Each grid checks **4,800 styled cells**; each PNG checks **647,040 pixels** (1011×640). All 18 grid reports have `cursor_differs: false`, but no grid or PNG is equal. Counts below come from this capture's per-scenario comparator JSON, not an earlier attempt:

| Named paired frame | Different cells | Different pixels |
| --- | ---: | ---: |
| `home` | 45 | 2,037 |
| `autocomplete-keys-home-before-enter` | 68 | 1,663 |
| `autocomplete-keys-home-after-enter` | 45 | 2,037 |
| `autocomplete-keys-home-before-esc` | 203 | 7,845 |
| `autocomplete-keys-home-movement-up` | 199 | 7,857 |
| `autocomplete-keys-home-movement-ctrl-p` | 203 | 7,851 |
| `autocomplete-keys-home-movement-down` | 199 | 7,857 |
| `autocomplete-keys-home-movement-ctrl-n` | 203 | 7,845 |
| `autocomplete-keys-home-after-esc` | 6 | 298 |
| `session-wide-completed` | 2 | 123 |
| `autocomplete-keys-session-before-enter` | 105 | 1,424 |
| `autocomplete-keys-session-after-enter` | 2 | 123 |
| `autocomplete-keys-session-before-esc` | 796 | 69,888 |
| `autocomplete-keys-session-movement-up` | 783 | 55,800 |
| `autocomplete-keys-session-movement-ctrl-p` | 789 | 55,775 |
| `autocomplete-keys-session-movement-down` | 783 | 55,800 |
| `autocomplete-keys-session-movement-ctrl-n` | 796 | 69,888 |
| `autocomplete-keys-session-after-esc` | 2 | 123 |

The Home placeholder/version strings, live completed-session elapsed value, and the different autocomplete inventories/spacing/styles remain visible; no region was cropped to claim parity and no timing or contents were masked. **VIS25 and T44 remain OPEN**: this run verifies the exercised movement and existing Enter/Esc interactions, while full-frame parity and the remaining VIS25 trigger/filtered/Tab/empty-filter coverage still need qualification against an attested final build. Existing `evidence/tui/.gitattributes` patterns `autocomplete-*/*/*.txt` and `autocomplete-*/*/*.vt` already cover this capture's text whitespace and VT binaries.
