# VIS38 pinned DCP display reference

These opt-in helpers implement amendment lines 493–587, sources D05–D10 and U34.
They are development/reference tooling; production `oc` has no JS dependency.
They leave existing capture modes and historical attempts intact.

## Pinned executable oracle

```sh
bun scripts/tui_capture/dcp_oracle.ts \
  /home/opencode/ai/oc/evidence/tui/recovery-v00/dcp-oracleNEW
```

Use a fresh lowercase/digit/hyphen attempt suffix. The direct parent must already
be `evidence/tui/recovery-v00`. The oracle fetches exact commit-addressed source
files and licenses and saves their SHA-256 values. It executes the full pinned
`lib/ui/notification.ts`, `lib/ui/utils.ts`, and `lib/state/utils.ts` modules.
Only imports change in executable copies; original source bodies are preserved.
Unexercised tokenizer/query services throw if called. Prompt/toast delivery is
intercepted as a fixture external service. Token estimates are explicit inputs;
no tokenizer or native compression execution is claimed.

Outputs:

- `goldens.json`: 13 notification cases, five categorical bars, 13 token values.
- `native-equivalent-fixtures.json`: typed `DcpRunSnapshot`/controls/summary inputs
  for display-unit tests, including pinned payloads and named differences.
  These synthetic inputs **must not be imported into native oc for paired
  acceptance**. The acceptance reference is regenerated from actual qualified
  native snapshots, not from this file.
- `sources/`, complete AGPL/MIT licenses, `NOTICE.md`, `source-manifest.json`.
- `executable/` and import-only normalization manifests for independent checking.

Fixture families: detailed/minimal/off, independent showCompression, single and
multi-range summary headings, overlap-ID deduplication, zero tools/summary,
summary recompression with zero newly covered Items and inactive-summary
exclusion, empty entries/positions, Unicode/long text, toast truncation, ordinary /
previous / recent bar categories, empty/one/50/101 message positions, K thresholds,
decimal half-up versus JavaScript floating rounding, and native M promotion.

## Actual original TUI rendering

```sh
node scripts/tui_capture/dcp_reference_capture.mjs \
  --oracle /home/opencode/ai/oc/evidence/tui/recovery-v00/dcp-oracleNEW \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v00/dcp-referenceNEW
```

The default campaign renders 12 isolated sessions and 14 full-terminal captures:
representative controls at 120×40, detailed at 80×24/120×40/160×48, and long
topic/summary at 80×24/160×48, with top and bottom frames for long summaries.
`--case detailed --columns 120 --rows 40` restricts the campaign to one probe.
Only the three approved terminal profiles are accepted.

The explicitly supplied OC2 v2.0.12 binary must have SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
The exact D05 ignored/noReply payload becomes a `type:user` message through
original **public** `session import --standalone`. Original public export must
return byte-for-byte equivalent message data. This mapping deliberately drives
the actual U34 UserMessage wrapper between two assistant steps. No direct DB
mutation, renderer substitution, plugin installation, or provider generation is
used. It is **source-derived display comparison**, not legacy plugin compatibility.

The bridge constructs a clean HOME/XDG/environment and an empty project outside
the repository; all generation requests are rejected and recorded. Dependencies
and Chromium come from the existing external `t44-reference` cache. Source
execution needs only the existing Bun executable, no package install. Full styled
cells, full-terminal PNGs, cursor, text, raw VT, input events, commands, import/export
proof, source manifests, and tooling lock are retained in fresh immutable attempts.

```sh
node scripts/tui_capture/dcp_check_reference.mjs \
  evidence/tui/recovery-v00/dcp-oracleNEW \
  evidence/tui/recovery-v00/dcp-referenceNEW \
  evidence/tui/recovery-v00/dcp-reference-checkNEW.json
```

The independent checker verifies source/body/capture hashes, public transfer,
zero generation calls, natural exit, cursor/dimensions, PNG/screen geometry,
unique header, U34 left border, two-cell padding, and header/blank/bar/run spacing.
Its success means reference evidence integrity, not VIS38 or native pixel parity.

## Released actual native owner probe and stage capture

After the parent releases and attests a built native binary:

```sh
python3 scripts/tui_capture/dcp_native_qualify.py \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --released-sha256 PARENT_ATTESTED_FINAL_BINARY_SHA256 \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v00/dcp-nativeNEW
```

This probe uses public `oc run`, an env-cleared fresh project/data root and an
offline loopback Responses fixture (at most 16 requests; zero authenticated calls).
It seeds three real closed turns with substantial user content and executes an
ordinary single-range call, a two-range call, and real `b0001`→shorter-summary
recompression. Only actual advertised closed anchors are selected. Exactly one
durable typed snapshot is required per completed operation. It checks immutable
raw history, call/result causality, actual wire-content gain, independently
estimated UTF-16/4 gross removal and net savings, active summaries, new coverage
delta (2/4/0 messages; 0 calls), 50 canonical bar positions, cumulative
accounting, and session-local high-water. Recompression subtracts its consumed
wrapper from gross removed; inactive `b0001` is excluded from active summary.
Raw assistant output retained by the owner is never counted as removed just
because its message is covered. Whole JSON-input bytes may grow during small
summary recompression although measured projected content genuinely shrinks.

Immediately after each commit, **before the next owner run**, the helper opens
the very same real session through a native PTY at 80×24, 120×40 and 160×48,
collecting full styled cells, PNG, cursor, raw VT, scroll and natural exit. The
open/close must issue zero additional provider calls and leave the observed
database unchanged. A restarted actual process checks frozen runs and raw history,
then captures the same real session. The admitted project-local `dcp.jsonc`
options are applied to real reopened TUI for minimal, off, showCompression:false
and toast; each has three full-frame captures with no replay. Each attempt is
immutable and individually SHA-256 sealed. This is not an injection interface.

## Actual qualified run → original U34 paired reference

The inspected read-only typed API is `CoreHandle::tool_ops_page` →
`ToolOpView::dcp: Option<DcpRunSnapshot>` and live `ToolCallFinished::dcp`, with
`CoreHandle::dcp_summary_page(session, operation, block_index, offset, limit)` for
real bounded summaries. Snapshot accounting and the 50-cell bar are frozen at
commit; full covered-ID lists and summaries are not embedded in the snapshot.
There is no native snapshot-import API. The offline helper observes only real
SQLite owner commits; public API/live-event equality remains an owner gate.

After **successful actual owner qualification**, the prepared adapter runs:

```sh
bun scripts/tui_capture/dcp_from_native.ts \
  /home/opencode/ai/oc/evidence/tui/recovery-v00/dcp-oraclePINNED \
  /home/opencode/ai/oc/evidence/tui/recovery-v00/dcp-nativeQUALIFIED \
  /home/opencode/ai/oc/evidence/tui/recovery-v00/dcp-oracleNATIVE
```

It rejects synthetic fixture JSON and non-PASS owner results, verifies owner seals
and stored operation identity, verifies pinned source hashes and unchanged bodies,
and uses real per-range counts/summaries and commit-time canonical IDs to build
only the **reference formatter's** input. It checks aggregate metrics, active
summary tokens and the D06 bar against the actual snapshot before executing D05.
Numeric donor block IDs are a documented reference-only schema mapping; native
block/operation/session identity is retained. It neither writes to native SQLite
nor exposes fixture-ingress flags to production oc. Complete source/licenses and
owner/source association are copied into a fresh attempt.

Pass the resulting `dcp-oracleNATIVE` to `dcp_reference_capture.mjs`. It maps
the actual committed conversation to original public `session import --standalone`
and the **unchanged pinned D05 output** to original U34's real UserMessage wrapper.
Single, multi-range, recompression, restart and real reopened controls use the
actual owner contexts. An imported prior report is placed at the original operation
boundary; no fake new compress step is added at restart. Exact public export,
zero reference provider calls, original natural exit and source hashes are checked.
The imported user text is a deliberate display mapping, never a claim that OC2
runs the old JS plugin. A toast is transient and is absent on both reopened chats.

```sh
node scripts/tui_capture/dcp_pair_audit.mjs \
  evidence/tui/recovery-v00/dcp-nativeQUALIFIED \
  evidence/tui/recovery-v00/dcp-oracleNATIVE \
  evidence/tui/recovery-v00/dcp-referenceNATIVE \
  evidence/tui/recovery-v00/dcp-pair-checkNEW.json
```

This verifies held-stage seals, context/snapshot association, dimensions and
full unmasked grid/cell/PNG/cursor **differences** for all eight stages × three
profiles, without normalizing timestamps, worktree paths, footer or agent labels.
An integrity PASS is not full visual parity. Remaining owner gates: failed/cancelled/
no-gain, live/public DTO equality, Undo/Redo, archive/paging/resource and routing.
The pinned donor's K-only baseline and native approved M/promotion/half-up rules
remain separate named differences; no native M-scale operation is implied by this
small offline campaign.

## Source-traced comparable default display (bounded first pass)

Use `dcp_native_qualify.py --default-detail-only` with the usual explicit released
binary digest and fresh output. It still executes all three real compress calls
and restart, but captures only the held recompression stage at the three sizes.
It records the real session/turn/acceptance metadata, selects detailed/chat with
showCompression:false, and matches supported theme, sidebar, TPS, animation,
cursor and debug-footer settings. No fixture enters native history.

After `dcp_from_native.ts`, prepare the original's public transfer:

```sh
python3 scripts/tui_capture/dcp_display_fixture.py \
  --native evidence/tui/recovery-v00/dcp-nativeQUALIFIED \
  --oracle evidence/tui/recovery-v00/dcp-oracleNATIVE \
  --case native-recompression \
  --output evidence/tui/recovery-v00/dcp-displayNEW
```

Pass `--case native-recompression --profiles all --display-fixture /absolute/path/to/fixture.json`
to `dcp_reference_capture.mjs`. The transfer uses the real title, selected
agent/model, model limits, public text, operation boundaries, latest-response
context usage and recorded display durations. Each field has a native source
trace and pinned OC2 schema/rendering locator. Public import/export checks all
message data exactly. Original `session/info.ts` normalizes the absent session
variant to `default`; original `rows.ts` requires the supported normalized `stop`
finish for footers and actual completed-turn idle boundaries for timing attribution.

**Time mapping limit:** native stores elapsed display duration and the admission
epoch inside the turn ID, but no absolute assistant event timestamps. The imported
completion coordinate is explicitly derived as admission epoch + recorded duration;
it is not a new absolute-event measurement. Streamed time is omitted. Import's
own session `time.updated` remains its real import time. Required zero cost and
session-token fields are documented reference scaffolding, not measured billing.

```sh
python3 scripts/tui_capture/dcp_display_audit.py \
  --native evidence/tui/recovery-v00/dcp-nativeQUALIFIED \
  --oracle evidence/tui/recovery-v00/dcp-oracleNATIVE \
  --reference evidence/tui/recovery-v00/dcp-referenceNATIVE \
  --fixture evidence/tui/recovery-v00/dcp-displayNEW/fixture.json \
  --released-sha256 PARENT_ATTESTED_FINAL_BINARY_SHA256 \
  --output evidence/tui/recovery-v00/dcp-pair-checkNEW.json
```

The bounded auditor verifies owner/source/public-export/capture seals, all full
styled cells (including width), complete cursors, geometry, and every decoded
RGBA PNG pixel using the existing Pillow installation. All differing cells retain
both complete values and exact coordinates. No normalization, mask or tolerance.
Expand controls only after reviewing this comparable default pass. The actual
2026-09-28 result and residual exact-fit word-wrap diagnosis are recorded in
`evidence/tui/recovery-v00/dcp-report20260928-03.md`.
