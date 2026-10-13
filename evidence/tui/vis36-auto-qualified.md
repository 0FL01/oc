# VIS36 — hidden CLI aliases and atomic effective approval consumer

Status: **ACTIVE_NOT_PASS** for full VIS36 / R6 / V09 / T44. This receipt closes
the bounded CLI-mode behavioral slice, not pixel parity or the frozen whole task.
Producer base: `601006d0911645477b9da583ab772d7a201deb7a` on the existing branch.
Captured identity is not rewritten to the later delivery commit.

## Source behavior and actual effects

`--auto`, hidden `--yolo`, and hidden `--dangerously-skip-permissions` use one global
CLI boolean. Bare TUI and explicit `tui`/`run`, prefix/suffix positions and literal
`--` prompt boundaries are tested. Actual binary help returns before malformed
fixture config/storage loads, exposes only `--auto`, and creates no native data.
Actual headless Ask admits Once for each explicit flag; saved config Autoaccept
alone still refuses, Deny still produces no marker, and no Always grants persist.
All three flags still surface headless `question_required` without answering it.

Settings saved Prompt plus CLI Auto formerly submitted two owner commands, briefly
publishing a Prompt consumer before the override re-registration. A real FIFO
query barrier between the owner ACK and the next frontend poll reproduced that
intermediate mode. The existing bounded owner command now carries the saved
preference separately from its effective process value. Save succeeds first;
failure leaves the old consumer/preference/mode intact. Successful publication
and pending Once drain use the captured OR in one owner operation. Existing
`set_permission_mode(bool)` and nonpersisting registration remain available.

Seven real approval-owner tests qualify pending and new requests, Prompt→Auto→
Prompt, rejection/cancel/error ACK/reload, CLI override, and exact child binding.
Auto retains the real Asked→Resolved(Once) event pair, never a waiting frontend
request or a durable Always grant. Tool start follows the matching resolution.
Root/child real marker effects and zero grants are observed; ordinary draft is
preserved. No policy, Deny/Plan/trust/preimage/child authority, RAW, schema, or
unknown-effect boundary is weakened. Question has its separate consumer.

## All acquisition attempts retained

Existing permission harness, actual pinned original and rebuilt native binaries,
121×40, hidden sidebar, real local Responses/permission backend/executor:

| Episode | Actual result | Whole PNGs | Strict comparisons |
| --- | --- | ---: | --- |
| 001 / cli-wide | Original PASS38; Native failed before first effect | 18 | 2 DIFFERENT / 32 BLOCKED |
| 002 / cli-wide-r2 | Original configure failed; Native once patch then failed Shell | 4 | 1 FAILED / 8 BLOCKED |
| 003 / cli-wide-r3 | BOTH PASS38 | 32 | 32 DIFFERENT |

001 emitted patch for a non-GPT fixture whose actual catalog offered edit/write;
the strict advertised-tool validator refused it. 002 exposed canonical upstream
`providers` versus native `provider`, then an unadvertised legacy `bash(argv)` call.
The fixture now explicitly selects `gpt-fixture-model-1` on both sides and emits
canonical `shell(command)`. Tests require the real config shape, model and schemas
and refuse the old model/missing advertisement. No production selector allowlist,
schema injection or admission bypass was added. 002's admitted patch prefix effect
is retained honestly. No failed episode, helper, PNG, VT or protocol was replayed,
rewritten, cropped, masked, normalized or re-encoded.

Current003 per origin: **18 valid provider requests / 18 completions**, 16 main and
2 title; **8 calls**, **11 read-only SQLite snapshots**, and **2 natural exit-0
generations**. Saved Autoaccept then saved Prompt is visible through actual Settings
and palette interaction; process CLI Auto still executes root and child Once.
Relaunch keeps saved Prompt and CLI override, but admits a fresh root for the new
submission; this is not a claim of same-root restart. Actual patch and Shell marker
effects, unchanged mixed-Deny files, and zero Always grants are checked.
Native final: 8 operations (7 completed / 1 failed), 8 completed turns, 16 messages,
one patch effect. Original V2 RAW: 32 messages, 8 user / 16 assistant / 8 idle,
8 tool parts (7 completed / 1 error), original IDs bound. SQLite is read-only, not
a syscall/CoreEvent trace; the typed owner tests establish transient ordering.

## Byte-exact proof and independent observations

`vis36-auto-proof.tar.xz`: **1,799,324 bytes**, all **363 files / 36,387,917 logical
bytes**, 6 same-run/same-side identical original encoded-PNG direct aliases.
All three episodes, parent logs/owners/drivers, images/cells/cursors/VT/clocks,
exact commands/build/source/protocol/effects are retained. Measure→pack→audit PASS.

- Archive SHA256: `9ed9865e3f99809bbc396ce0c9af7f39ed622d17487f2d4a4125a80370a6caae`.
- Decoded TAR 36,392,960 bytes: `1afacfa41b7d61574a31ef6092898ead95c37af3381e45972e5e1b8ec301b968`.
- Member inventory: `f52b231355cbca118f277442bba610c2995363b103057741a25ff9975db25787`.
- Retainer at pack: `014607b4d7c41dc0a2a168af0e2f4180b5a4f6e09db06164f90a5824d1cb9bc7`.

Independent before/after seals and manual single-XZ/canonical-USTAR inverse verify
every original byte/path/alias and directory metadata stability, actual source and
ELFs, and **105 prior proof files' bytes/metadata**. The observer initially assumed
38 unique stage names (there are two natural-exit proofs), then assumed an unqueried
`child_jobs` table and an absent capture-row status. All failed commands are kept.
The sealed original observer/before stay unchanged; two retained refinement wrappers
identify all 16 actual grid scenarios including both first-Escape frames. The second
wrapper checks exact equality with the capture lock's scenario set. No state or
metric is omitted and no image tolerance is introduced.

Independent current recount: **16 pairs / 32 whole PNGs**, **77,440 styled cells**
and **10,444,800 RGBA pixels** checked; **14,122 different cells**, **1,495,640
different pixels**, **11/16 cursor pairs equal**. ALL32 strict comparisons remain
DIFFERENT. Independent VmPeak **187,707,392 B**, VmHWM **127,942,656 B**, below 384 MiB.
After report 9,056 B SHA256 `986ba6ce103dc1d7cbfdfbc00af4f64ede2affde9443438e5732fd8f1ca99c0b`.

Current canonical **713** inputs seal:
`d3d02ec520b6f7f5cf9511244ead95ef15e12e1e1ff1bf46157f8a96011dbefe`.
Native debug ELF **484,373,016 B**, ET_DYN/x86-64:
`34e06ce943f45c4c067903b0ddd2469168f1fd245562860666afdc7506aa5a1c`.
Pinned donor **2670273ff17da96f85c5826ced57aa1b368754fa**, ELF **200,529,376 B**,
ET_EXEC/x86-64 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Only the 8 Rust owner/parser/test paths and 4 existing capture inputs changed
relative012. Old Question receipts/source flags are immutable historical evidence.

## Current code gates — actual complete results

- Workspace `cargo test --workspace --locked --no-fail-fast`: **1864 PASS / 0 FAIL /
  11 unchanged opt-in ignores / 46 targets**, 916.992 s. TUI530 / PTY58 remain.
- Strict locked workspace/all-target Clippy `-D warnings`, workspace fmt check,
  locked and ordinary debug build/help, locked release build/help PASS.
  Actual release build 197.918 s, not a debug ELF substituted into a release path.
- **9 actual release/resource cases** PASS: isolated startup/discovery2, TERM01,
  panic restoration, VIS31 pacing2, S07 equal archive, DCP equal archive, AUD32.
- Release S07 retained24,231 B at archives0/3000, queue lag0/children0;
  HWM26,656→28,396 KiB. AUD32 HWM21,952→22,208 KiB / children0.
- Root Python **70** PASS, including strict fixture six-subcase test and retainer16;
  changed JS syntax checks, docs/progress/diff checks PASS. No new ignore or weakened
  test, timeout, performance bound, goldens or baseline.
- Complete successful and failed command logs/programs are retained byte-exact in
  `vis36-auto-gates.tar.xz` with independent manual memory and durable inverse.
  **49 commands / 102 members / 323,382 logical B**, encoded **61,052 B**;
  SHA256 `1a044ef28e39050238fc6cdcae6263ee229db76ec3f422844df61ce69f967036`.
  Gate inverse VmPeak244,109,312 B / VmHWM73,465,856 B remains below 384 MiB.

## Accounting and continuation

The new `vis36-` prefix is ADDITIVE to all established owned-evidence prefixes,
under the SAME **1 GiB** cap. The legacy independent reader's printed quota retained
its older prefix list; its 2,609,152-B figure is **not current VIS36 headroom**.
The actual updated storage recount before this human receipt/gate bundle is
**1,073,123,328 / 1,073,741,824 B**, **618,496 B** headroom. Final delivery recount
includes this receipt and the bundle. 16 MiB/file+encoded, 128 MiB/TAR+logical,
2,000 members, 16 MiB XZ dictionary / 32 MiB decoder / 384 MiB Python AS unchanged.
TMP is separate transactional acquisition, not durable offload. No qualified proof
deletion, cap raise, PNG re-encoding or old receipt/checkpoint rewrite.
Actual post-bundle allocation: **1,073,205,248 B**, headroom **536,576 B**;
the same additive prefix list is used by the final delivery seal check.

Next confirmed visual gap: Settings clears its `Permissions` filter after a mode
ACK, whereas pinned DialogConfig retains it. Preserve query/selection through the
existing owner refresh and qualify actual pixels/cursor; do not attribute the
entire differing Settings frame to that gap. Continue every remaining frozen
R1–R6 / VIS01–VIS45 / supplement, then final current-source R6/V09. No T44 finish
or whole VIS36 PASS follows from this green bounded behavioral slice.
