# T44 Shell command wrapping and cold hover — qualified slice

Status: qualified source/presentation/effect slice; whole frozen T44 remains
**ACTIVE / NOT_PASS**. This is not global VIS16/VIS17 acceptance or final R6.

## Pinned Terminal TUI owner and preserved contracts

The pinned Terminal TUI `ShellDisplay` uses character wrap for its collapsed
command, with at most two command rows; expanded commands retain word wrap.
The actual120-column proof previously lost the real separator before `31` on
the continuation. Native now uses the small bounded styled character wrapper
only for collapsed Shell commands. It preserves graphemes, display columns,
source whitespace and span styles; zero row limit and the existing oversized
single-glyph fallback remain bounded. Expanded commands and output keep their
existing independent wrap/collapse policies.

The pinned `BlockTool` box has padding and background, but no output-child
foreground. Both native model and direct-user Shell padding now inherit the
terminal foreground rather than the muted process-output foreground. Command
text remains the theme's separate text color; process text is still muted.
Generic cards and their style policy were not changed.

Cold Shell hover had a separate physical-pointer cause. Sessions dialogs
correctly suppress underlying transcript hover and clear that pointer on close.
Their matched mouse release was consequently lost when the picker owner opened
the same-Location session. Only that accepted Sessions mouse route now retains
the actual release coordinate and frame. Current painted targets re-hit it;
no operation-hover identity, press, selection or old hit is restored. Refusal,
cross-Location routing, keyboard opens, read-only/busy/action guards and existing
deck/CAS ownership are unchanged. No new query, registry, store, poll, grant,
runtime Node/Bun dependency or replay path was introduced.

RAW input/output, wire names, provider/tool graph, execution, permission routing,
secret/control redaction, actual capture references, resource caps and typed
status remain unchanged. Literal marker text is not stripped or interpreted as
capture authority. The renderer still does not fabricate a zero-exit notice,
full-output reference or a different process body to resemble Original.

## Meaningful regressions and current gates

The actual command separator assertion was RED before character wrap. The
same new renderer scenario then caught inherited-padding FG before its fix;
it also checks Unicode/wide glyphs, styles, row limits, original card/row equality,
expanded literal body and no model-preview substitution or fake success.
One new binary routing scenario was RED before pointer retention. It exercises
both cold Home and a retained root, real painted Sessions mouse down/up,
owner refusal followed by acceptance, unchanged drafts/deck on refusal and the
same current physical pointer only after accepted routing. Existing direct-user
Shell coverage retains every old body/identity/action assertion and now verifies
that both model/direct padding inherit FG while actual process text stays muted.

- Full locked workspace no-fail-fast suite: **1836 PASS /0 FAIL /11 unchanged
  opt-in ignored**, all46 targets, including112 binary and512 TUI tests.
  This is one complete green run, not a union of partial targets.
- `cargo fmt --all -- --check`, strict locked workspace all-target Clippy and
  diff checks PASS. Locked and ordinary debug builds, locked release build and
  both `oc --help` PASS.
- Release recovery startup4 covers19 routes/8 GET-only requests/no Responses and
  restored terminal. Actual TERM01, two VIS31 controls, S07 and actual AUD32 PASS:
  nine additional release gates, without threshold or deadline changes.
- TERM01 baseline/two-PTY RSS18532/22496KiB, FD24/26, threads8/10; idle300ms0 CPU
  ticks. VIS31 idle1s0 ticks,165/250Hz input/provider burst, worker lag0.
- S07 archive0→3000 RSS26868→26892KiB/HWM27964→28108/PSS24582→24680; retained24232,
  window152/cache8581, children0/lag0, draw max126471945→121214818ns. Actual AUD32
  binary8→3000 pairs16KiB+logs/two active1KiB: HWM20784→23168KiB, endRSS19676→20420,
  endPSS17155→17760, DB942080→152944640, children0/eight threads/WAL0. Original
  memory guards hold; these are bounded synthetic gates, not real-user strace.
- Root Python54 and Node probe syntax checks PASS. The locked debug ELF was
  restored after release gates and before capture; no Cargo ran during acquisition,
  retention or independent audit.
  Physical source map540 files/291177 lines/11681814 UTF-8 bytes; existing soft
  advisories application5460 and turn5307 are unchanged. Their next natural seam
  is tool settlement/publication, not a cosmetic counter move or cap waiver.

## Actual current full-frame proof

Fresh exclusive `shell-wrap-current-003/inventory` and
`shell-wrap-regressions-003/{ordinary,temporal,matched}` preserve actual inputs,
15s/five-identical-full-observation stability, owned PGACK, fonts/geometry, clocks,
cursor observations, full opaque PNGs and strict unchanged comparators.

Both absolute120-cell command rows are **240/240 equal at completion, reopen and
restart**, including symbols, FG, BG, modifiers and width. Both commands begin
at x5/y11, continue at x5/y12, end their first row in `30` and preserve ` 31 32 33`.
Padding FG is `#ffffff`, command text `#eeeeee`; the inner surface is unhovered
`#141414` at completion and genuinely hovered `#1e1e1e` after both mouse reopens.
No extra pointer input or frame normalization was introduced to obtain equality.

The Root episode retains seven real states per side, busy Add/Home/same-root
return/draft ownership, completion while parked, Home-only close, natural
generation0 exit0 and painted Sessions reopen without replay. Four requests
include one real91-byte overwrite and one explicit cancelled generation. Native
has one session/two completed/cancelled turns; public cancellation text remains
INCOMPLETE, with no second tool/effect. Actual context1234+64/captured limit225000
holds. Write840 absolute styled cells per stage,39 grammar bodies14256 and2144
heading cells, narrow MD/Zig, accepted-user hover345 cells, RGB `#12ab34` and
the physical shrink tail hold. SDK corpus39/78/156 and all85 cache seals remain
valid; captured FIRST witness proves69 required inputs consumed,16 locals unclaimed.

Ordinary/default temporal each retain six requests/four calls/three MCP effects
and one17-byte Shell effect per side. Native has three label-only intents plus
five genuine presentation records/four operations/three complete resources.
Wire `vis16__output` and typed display `vis16_output` remain distinct. RAW,
provider results, cold resources and effects do not change or replay.

| Actual native Shell representation | UTF-8 bytes | Logical rows |
|---|---:|---:|
| Full genuine `Presentation.body` | 1606 | 83 |
| Separate `.shell.stdout` preview | 960 | 49 |
| Full served model output | 647 | 15 |
| Model prose component only | 257 | 12 |
| Complete cold capture | 1615 | 84 |

The body is not the960-byte stream field or257-byte model prose. Typed completed
exit0, Complete capture and body flags (`body_limited=false`, generated guidance
true, producer limit absent) remain unchanged; metadata is not parsed from prose.

The final self-contained `shell-wrap-r3-proof.tar.xz` and immutable inventory
retain all1442 original files/100375797 logical bytes in3851204 encoded bytes,
with1297 regular members/145 direct same-root/same-side byte-identical PNG aliases.
This includes230 full opaque PNGs/817248 styled cells/108076032 pixels, all144
temporal caret associations,72 matched pairs and every entire strict report.
Independent restoration checks every byte/membership, all708 current inputs in
three exact JS-locale manifests, three real locked builds and both actual ELFs.

| Immutable capture/proof association | Value |
|---|---|
| Actual capture base | `82e0d8016963b536ee363a2da5c2000f21555b4a` |
| Actual base tree | `231f61167627c64fe727c04938b150f1b5064f0d` |
| Captured dirty diff | `cdbae11cafe6eacd952510371888ee15a9b52fdbd78da51876955b9d4aea298a` |
| All708 JS-locale canonical source seal | `285b05d7d981e5d9fa99f9ac8ca5ce76fc2fccc54ebe22aa9256887140ce0e86` |
| Native ELF | `b96f4c8dd4609178c99fedd24a9e555fe5db17ba74af8e3a861101e58832c839` |
| Original ELF | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |
| Encoded final archive | `7d020a434ffc49741fad3a0b324f0b33a13aaf310a31ad14f03e29dab87c2e72` |
| TAR95528960 bytes | `c306742a7cd346d8028f8cd90ec24cb64b851a09f3eb4ba854e6781b241ffd2e` |
| Member inventory | `b5b3e713db14a8502368e230ecad8b9810032886e5e34f2436a77910fee2a682` |
| Retainer at pack | `c4b18eef44975e51ff6ef42ba96ff8293a3dfc98bb909b44bb68ff6c4ec1449e` |

Associations precede delivery and are not rewritten to a later commit. Original
remains pinned donor2670273ff17da96f85c5826ced57aa1b368754fa; licenses/provenance hold.

## Failed attempts, complete preservation and bounds

`shell-wrap-proof.tar.xz` remains the original3882732-byte first proof, including
its four gutter-FG/cold-hover findings; it is not replaced.
The second ordinary probe incorrectly required every command FG to be white
instead of just box padding. Both binaries failed that observer assertion after
real effects; their already captured command rows were equal. Only the assertion
was corrected before fresh003; no incomplete episode was replayed or overwritten.
`shell-wrap-diagnostics-proof.tar.xz` retains all308 original files/35237833 bytes
and46 full captures in1728580 bytes, with six direct same-side PNG aliases.
Both FAILED outcomes, historical708-source/two locked-build associations and
unmodified raw frames remain visible; these are not current-source proof claims.
Checkpoint0119's shorthand `all308 bytes` refers to all308 members' preserved
bytes, not308 total bytes; the exact logical extent is35237833. The immutable
checkpoint is retained, while the current named note uses `all308 files`.

One own untracked FAILED `subagent-cards-004` was losslessly archived only after
strict ownership/untracked/non-symlink preflight and durable full inverse checks.
All187 files/19938423 bytes and three directories preserve mode/UID/GID/mtimeNS in
966116 encoded bytes, full metadata receipt and a visible original-path
FAILED/NOT_QUALIFIED pointer. Immutable capture bytes/metadata/failure visibility
were preserved, not redundant loose encoding. Only that fresh private quarantine's
verified duplicate copies were removed; committed005, baselines, old claims and
foreign data stayed untouched. Archive seal:
`7c70a49908319c6e6022631bd43c7dfcad685a869d4d98d4c9f835f7ba76f966`.

Pure stdlib USTAR/single XZ retains all encoded PNG/JSON/VT bytes, with no baseline,
pixel predictor, correction library or PNG reencoding. File/encoded16MiB,
logical/TAR128MiB,2000 members,XZ dictionary16MiB/decoder32MiB and Python384MiB AS
hold. Independent Python peak/HWM341552/312784KiB. Exact owned allocation before
these small docs1057427456/1073741824B leaves16314368B; every `shell-wrap-` artifact
counts. Fresh scratch, all old proofs, failures and helpers remain preserved;
final evidence is durable in-repo, not offloaded, and no quota was raised.

## Remaining full contract

Current direct220 DIFFERENT/8 EQUAL/4 authorized NativeOnly and matched144
DIFFERENT remain unmasked. All114 direct and72 matched cursors are equal. Default
temporal3947 samples/23917 commands has0 phantoms/cycles, not current-source blink
qualification. Ordinary completion817 cells/57734 pixels, cold reopen/restart804
cells/57254 pixels each, expanded Shell8295 cells/1056802 pixels, Root Home46
cells/2076 pixels and real clocks8.2/8.3s, cancelled1.7/1.8s remain different.
Former32 edge pixels stay equal; narrow raster retains51 real clock pixels.

Next resolve the actual body/tail/footer/card-layout owners without substituting
model output, suppressing real loss, inventing notices or weakening redaction and
permissions. Continue every frozen R1–R6/VIS01–45 outcome and Thoughts/family/forms/
DCP/MCP/auth/read-only/pacing supplement, actual current full-frame/effect/resource
proof, then final current-source R6/V09. Command-row equality is not task finish.
