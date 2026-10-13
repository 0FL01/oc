# VIS36 — shared dialog geometry and Settings footer roles

Status: **T44 ACTIVE_NOT_PASS**. The shared geometry and four observed Settings
modal surfaces are qualified; whole VIS36/R6/V09/T44 pixel parity is not PASS.
Producer base: `4889a5ca38dfdc3bb4f5948d2ae5df65d3257236`.
Pinned donor: `2670273ff17da96f85c5826ced57aa1b368754fa` (OC2 v2.0.12).

## Observed contract and minimal changes

The retained 121-column original Settings dialog began at x31, whereas native
began at x30. Both had width60, y10 and height10. Query glyphs/styles matched,
but query/caret/hit targets were one column left. Pinned `ui/dialog.tsx` centers
the width60 medium surface and caps width at terminal-width−2. Actual paired
frames establish the half-cell tie resolving rightward; this receipt does not
claim that the cached JS layout wrapper itself calls `Math.round`.

`DialogFrame::rect` now uses ceiling division of the horizontal remainder.
Vertical quarter-height placement, width limits, clipping and other panel
behavior remain unchanged. Paint, query cursor and pointer hit testing share
that owner. The unit drives widths79/80/120/121, larger dialog sizes, nonzero
origins and narrow/clipped areas, and preserves the ordinary Unicode draft.

After this correction, every paired Settings modal cell matched except four
footer cells. Pinned `DialogConfig` supplies the non-action `←/→` / `change` hint;
`DialogSelect::FooterAction` paints its three title glyphs bold/base, the following
space base/unbolded, and the label muted. The native Settings-owned footer now
uses those three semantic segments rather than one muted string. Generic,
MCP and Sessions footer paths are unchanged. No numeric color/caret workaround,
new DTO/store/framework, permission policy, RAW/schema or public API change.

## Real tests and retained failures

- Geometry RED: after fixing a test-local missing `CommandAction` import,
  the actual test failed at width79: native x9 versus required x10.
- Geometry targeted tests and real owner suite passed; the whole affected TUI
  crate ran **532 PASS / 0 FAIL** before the footer assertion extension.
- The first new footer assertion used an absent private dialog token without
  the existing root fallback. Only that new test lookup was corrected. Actual
  RED then reproduced muted128 versus base238; the production role fix passed.
- Final VIS36 UI tests **3 PASS**, actual permission owner tests **7 PASS**,
  root Python **70 PASS**, strict locked workspace Clippy/all-targets `-D warnings`,
  fmt and `git diff --check` passed. No baseline, threshold or ignore changes.
- Two unsealed independent observer attempts failed on a missing commands
  argument and a borrowed Question comparator count36 versus Permission count32.
  Only the new observer was corrected; all command bytes and old readers remain.

Current complete workspace command:

```text
cargo test --workspace --locked --no-fail-fast
1866 PASS / 0 FAIL / 11 unchanged opt-in ignore, 46 targets
TUI 532; actual PTY T39 58; elapsed 820.565 seconds
```

Final locked and ordinary debug builds/help, locked release build/help, fmt and
strict Clippy passed. Release build elapsed142.717s. **Nine actual release cases**
passed: two isolated startup/discovery cases, TERM01, PTY panic restoration,
two VIS31 pacing cases, S07 equal-view archive, VIS38 DCP archive and AUD32 bounds.
The first S07/DCP exact filters ran zero tests because names are module-qualified;
those successful-but-zero records are retained and are not claimed as PASS cases.
Corrected filters ran one actual case each, without replaying unknown effects,
relaxing deadlines or substituting an ELF manually.

## Acquisition, source and executable binding

New exclusive episodes:

```text
$TMPDIR/opencode/vis36-mode-current-005/{cli-wide-r5,cli-small-r5}
$TMPDIR/opencode/vis36-mode-current-006/{cli-wide-r6,cli-small-r6}
```

Profiles121×40 and79×40 use the existing actual-binary permission fixture and
unchanged16-frame CLI-auto driver. All four current006 side/profile executions
passed38 behavioral checks. Every32 grid/PNG comparison per profile is retained;
**64 whole PNGs / 64 strict comparisons, all DIFFERENT**. Old001–005 episodes,
parents and drivers were not rewritten or replayed. There is no mask/crop,
tolerance, clock/cursor normalization, golden substitution or PNG re-encoding.

Current713-input canonical source seal:
`4a0a41038113d4a346cfd948953b311152131194867b79ce2b8270f67f4b6cf8`.
Only `crates/oc-tui/src/dialog.rs` and `app/tests/input.rs` differ from current004.
Captured producer identity remains4889, separate from the delivery commit.

| Actual executable | Bytes | SHA-256 |
|---|---:|---|
| Native debug, ET3/x86-64 | 484374672 | `3b40d0b2d715fb104371a7d57ab6c90b351eda52b68b749405ebbda0b90607b2` |
| Pinned Original, ET2/x86-64 | 200529376 | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |

Each captured profile records one actual locked build, exact commands, current
source and ELF hashes. Support retainer/archival scripts are outside that713-file
input set. Historical005 pack-time source flags and receipts are not rewritten.

## Byte-exact inverse and independent whole-frame recount

| Retained proof | Archive bytes | SHA-256 |
|---|---:|---|
| Geometry-only005, now historical | 2273696 | `91e185971b234e45c6256eaca505e52c3495c1985cb8f8c76370a08e4b491ef9` |
| Current006 geometry + hint | 2278516 | `f00c7062545f03de3d98db55171492a995d72d0e07f53c4e0129a443a252f667` |

Each archive retains422 original files and4 same-run/same-side byte-identical
original encoded-PNG direct aliases. Current logical bytes39842563; decoded
canonical USTAR39987200 bytes, SHA
`93be7f793642493ae7b3f23b1da98aab8236c9ee30c27a20a2454b8db1f989a2`;
member inventory `ab9d425d5fea0e55cfdea674b729db0e0bb538ae91e697548b2d73ea400de878`.

The retained independent programs use manual single-XZ header/footer/index/CRC/
dictionary/SHA validation, direct canonical USTAR inverse, and independent full
PNG/styled-cell/cursor readers—not the producer decoder. Every original byte,
directory metadata, source/ELF/helper binding and **119 prior proof files' bytes
and metadata** remained unchanged. Reports/programs are immutable and exclusive;
their old BASE checks are not a reason to replay them after a later commit.

Current whole recount: **32 pairs / 64 PNGs**,128000 styled cells and17264640 RGBA
pixels checked; **33431 differing cells / 3918102 differing pixels**,28/32 cursor
pairs equal. Four complete600-cell Settings modals, queries and caret geometry
match, including the footer: **2400/2400 actual modal cells equal**. This regional
diagnostic does not replace the failed whole-frame comparisons.

Each current profile/origin has18 valid/completed providers (16 main +2 title),
8 actual tool calls,11 read-only snapshots and2 natural exits0. Real saved
Autoaccept→Prompt, CLI root/child Once, mixed Deny/no mutation, zero Always grants
and relaunch effects remain verified. Relaunch admits a fresh root, not a
same-root restart claim. Native records8 ops (7 completed/1 failed),8 completed
turns; Original V2 RAW records32 messages/8 bound tool parts. SQLite RO is not a
CoreEvent/syscall trace; the real typed owner tests establish ordering separately.

## Capacity, preserved diagnostics and continuation

Only own untracked/unstaged actual FAILED `model-shell-008` was losslessly archived:
82 files4379427 logical bytes plus3 directories' mode/UID/GID/mtime, original
FAILED pointer retained; Original PASS_BEHAVIOR_ONLY status also retained.
Archive306184 bytes, SHA
`ad094e3bf0146ee481a5ac5c90b351a132c4db9f8b0466fa195fbc5682334ec4`.
Same-filesystem transaction/manual PAX inverse and3MiB reservation passed before
cleanup of only the fully verified redundant private copy. Allocation reclaimed
4214784 bytes. `model-shell-007` PASS_BEHAVIOR_ONLY/DIFFERENT was not selected.

After current006 report, before delivery receipt/gate bundle: actual additive
owned allocation1070067712/1073741824, headroom3674112. The final delivery recount
is authoritative after new records. Existing prefixes, including additive
`vis36-`, and limits remain:1GiB owned evidence,16MiB file/encoded,128MiB TAR/logical,
2000 members, XZ dictionary16MiB/decoder32MiB, Python hard AS384MiB. TMP is separately
accounted, not durable offload. No qualified proof deletion, cap raise or re-encoding.

Complete command retention: `vis36-dialog-gates.tar.xz`,59232 bytes, SHA
`6059dc3454e8a65a61b90f8f593499dd0d587d00531f943276a71082662df6a6`.
It retains47 actual command JSON/full-log pairs and3 helper programs:97 members,
327072 logical bytes, independently restored byte-exact in memory and from the
durable archive. Gate inverse VmPeak244285440/HWM73371648 bytes stayed below384MiB.
The first delivery-packer check named `release` instead of the actual recorded
`release-build`; it failed before writes. Only the unsealed helper's literal label
was corrected; no command/result was invented or repeated. Actual allocation after
this receipt and gate bundle:1070145536/1073741824, headroom3596288 bytes.

Next: source-check the observed successful empty Shell result presentation against
the frozen TUI contract, then all remaining R1–R6/VIS01–45/supplements and final
current-source R6/V09. **Do not finish T44 on this local modal qualification.**
