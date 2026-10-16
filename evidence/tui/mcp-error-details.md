# T44 VIS40 — dedicated MCP ErrorDetails

Functional facet qualified on current source; full VIS40/R6/V09/T44 is **not PASS**.
This consumes the existing safe MCP diagnostic, current inventory and clipboard
owners. It does not introduce a raw peer-error channel or a second error store.

## Implemented boundary

- Failed Enter (and truthful native unsupported-auth diagnostics) opens a dedicated
  medium `MCP server: <safe label>` surface instead of diagnostic Select rows.
  Healthy/disabled/pending Enter neither connects nor dismisses the list.
- Detail state retains only the opaque server ID, captured `McpBinding`, scroll,
  copy feedback and pointer press. Label/diagnostic remain borrowed from the
  current owner snapshot; replacement/recovery retires the detail. Newly masked
  labels clear queued MCP copy and copied feedback before later rendering.
- One `McpDetailLayout` owns wrapped safe body rows, finite viewport, scroll limits
  and Back/Copy/Investigate hits. The detail never renders Search or assigns a
  hardware cursor. Wheel/body scroll, arrows, Page/Home/End, resize and drag guards
  do not mutate the list filter or composer.
- Back restores the originating filtered list/selection; closing restores the
  original Unicode composer and caret. Read-only text/paste/Space remain inert.
- Bare `c` and the Copy hit use the existing clipboard gate/size cap and actual
  synchronous transport result. `✓ copied` appears only after successful write
  and flush; failure remains non-success. The binary now drains key and legacy
  shortcut copy requests before applying outcomes, as it already did for mouse.
  Existing Ctrl-Shift diagnostic shortcuts retain their public behavior.
- Explicit bare `i` or its mouse hit replaces the composer with bounded safe
  owner diagnostic text, without submission, reconnect, tool or provider request.
  Oversized input is refused without prefix truncation/draft mutation; linked
  child read-only restrictions remain. It does not invent session navigation.

The frozen source contract requires dedicated details and intrinsic Connected
bold. Safe native `ServiceDiagnostic` text intentionally differs from the raw
exception/context used by the pinned reference; native unsupported OAuth remains
truthful. These differences are not masks or a whole-modal parity claim.

## Current source association

Final immutable roots:

- `evidence/tui/mcp-status-attempt-012` — dedicated detail/actions and mixed list;
- `evidence/tui/tool-preview-attempt-025` — ordinary Home/body/reopen/restart;
- `evidence/tui/cursor-temporal-attempt-028` — expanded-card blink regression;
- `evidence/tui/cursor-temporal-matched-009` — full actual-phase comparisons/audit.

All three capture locks record one successful actual `cargo build --locked` and
the same source/binary inputs:

- base `43db2fe87fbe08d83b47821cbe215118404d8653`, tree
  `54601f6ff111811d9679206bc4cf351fe94e8f2b`;
- reviewed dirty diff
  `9dcefcf5b2daa5483a961e43860a4d7dd60ef3a4d86f5e310cf8513edcd3318e`;
- source manifest
  `943df01bedffa83ce40dcce4565b52178a733e54b9125555a22d57ecfffe71e6`;
- native executable
  `1ff534a7d66259d5770bf9319a4e51c30c02535ab2f1cd9dcf5718c72295fb9a`;
- pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`, executable
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

No Rust/capture runtime edits follow these final qualifications. Documentation
and delivery association are separate from the immutable capture metadata.
Reviewed implementation/evidence commit
`d5bafad7ec7b3e0d23b12747647143001ff311b2` is **PUSHED** by ordinary fast-forward
to the verified own `agent/oc-rust-port` branch at `github.com/0FL01/oc`.

## Actual paired MCP detail/actions

Command (fresh output, one bounded run; test-only Node/Chromium, not native runtime):

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 120 --rows 40 \
  --tool-preview true --mcp-status true \
  --output /home/opencode/ai/oc/evidence/tui/mcp-status-attempt-012
```

Both actual binaries complete 18 paired stages: Home; every mixed-status selection
at120×40/80×24/160×48; configured-label search; read-only details; short-body
navigation; keyboard/mouse Copy; Back/restored composer; keyboard/mouse investigation.
All 36 strict whole styled-grid/PNG comparisons are **DIFFERENT**, runner exit1
diagnostic, not pixel gate success. No crop, mask, identity/version rewrite or
normalization. Exact safe configured labels/status tones stay visible; source-required
native Connected bold and running reference's absent bold remain disclosed.

The real failed/healthy peers initialize; disabled stays inert. Provider requests,
tool effects, MCP tool calls and Shell counter effects are zero. Native detail
cursor is hidden, no Search remains, Back restores filter/selection and exact
`preserve MCP Ω界` draft/cursor `{x,y,visible,shape}`. The short real diagnostic fits:
navigation proves no unintended mutation, **not** actual overflow; the owner test
separately exercises long Unicode body, scroll limits and resize/mouse hit guards.

Keyboard and mouse Copy each emit one actual OSC52 payload. Native payloads agree
exactly and carry current safe title/owner DTO, not `VIS-MCP-CONNECTION-ERROR`.
Reference payload includes its real status/configuration context and peer error.
Copied feedback follows transport success. System clipboard acceptance is
**NOT_VERIFIED_BY_PTY**; no desktop/SSH clipboard guarantee is inferred.

First keyboard investigation populates an unsent draft on both. Repeating identical
investigation by mouse after Ctrl-C clear populates native again; the pinned Home
route closes without reinjecting the same reconciled prompt. The reference receipt
records `DIALOG_CLOSED_DRAFT_NOT_REINJECTED` and false mouse-draft observation; its
full empty-Home frame is retained. This observed reference defect is neither fixed
in the donor nor imitated/accepted as native behavior.

## Current-source neighboring regressions

Ordinary025: both 27 paired stages behavior-qualified, two native-only resource
frames. Whole comparisons:4 EQUAL/50 DIFFERENT+4 NATIVE_ONLY_RESOURCE_DETAILS.
Home failure/retry uses zero model requests; completion has six total requests
(one title/five main), four tool effects, three actual MCP calls and one Shell
effect (one line/17 bytes). Capture hashes, native RAW operation/resource facts
and four bounded presentations remain unchanged through `/new`, reopen and clean
same-root restart. Read-only `/cards` pages, literals, guidance separation and
draft/caret guards remain; no rehydration/reexecution/cap/access change.

Temporal028: both six real composer/Search states qualify ≥3 full raster cycles,
same-owner idle cadence/sampling bounds, exact draft/caret/shape restoration,
zero unsynchronized visible phantom commands and no tool replay. Native cycles
3/3/3/3/4/3; reference3/3/4/4/4/4. Native hover1992 commands and Search hover1242;
reference Search25 commands is **not** claimed continuous reference repaint.
Auditor validates144 actual opaque PNGs/88,473,600 fixed-owner RGBA pixels;
69 actual-phase matched pairs/138 whole-grid+PNG comparisons ALL DIFFERENT.
Runner154 DIFFERENT/12 unmatched temporal samples; no inferred missing-phase PASS.
Older representative geometry/unsupported-sync/steady/default proofs stay historical.

## Checks actually executed

Current `.local/t44-mcp-details-gates-final-20261008.log`: serial jobs3/tests2 and
approved TMPDIR; all successful:

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked
cargo build
cargo build --locked --release
target/debug/oc --help
target/release/oc --help
python3 crates/oc/tests/support/startup.py target/release/oc
python3 crates/oc/tests/support/discovery_startup.py target/release/oc
```

46 completed results:1743 passed/0 failed/11 unchanged opt-in ignores. Binary98,
MCP41, adapters677, TUI463; release2m22s. Actual release startup success/failure,
busy/lock release, missing/present credentials, malformed selection/config,
unsafe/unavailable cases restore terminal and retain refusal/retry ownership.
Discovery authorization/forbidden/oversized/slow/absent/present cases keep exact
GET/no-Responses and expected exit/restoration. No live/paid campaign was run.
New owner read-only test and binary key-copy success/failure regression pass;
existing pending lifecycle now positively acknowledges queued input via filter
paint while held, retaining all effect/coalescing/refusal/cleanup assertions.
Repo Python47/0, Node syntax, docs/journal and source diff checks pass.

## Attempts and remaining scope

- Attempt009 proved actual native keyboard-copy queue was not drained; binary event-owner
  fix closes it. The original OSC52 included context, not only painted error text.
- Attempt010 native behavior passed; reference check corrected to actual pinned context.
- Attempt011 exposed repeated original route prompt not reinjected; final012 records the
  genuine full reference state without weakening native draft assertions.
- First broad gate exposed obsolete Select diagnostic-row/pending-Enter tests.
  Tests now assert dedicated safe body, no Search and pending Enter inert, with
  wrap-aware exact code/retryable facts; privacy/toolgraph/lease checks retained.

Immutable failed attempts remain; no errors/tests were disabled. Captured padded
`.txt` and raw `.vt` are excluded only from Git whitespace validation for the
exact final capture roots, never from comparisons or source/docs/JSON validation.
No artifact bytes are trimmed. AUTH06 deferred, original24/24/Go13/24 unchanged.
Effective Select remaps/footer action focus/hits, full VIS40 and every remaining
frozen T44 outcome plus final current-source R6/V09 remain open. No task finish.
