# T44 accepted-user hover across resize — qualified slice

Whole frozen T44 R1–R6/VIS01–VIS45, all supplements and final current-source
R6/V09 remain **ACTIVE / NOT_PASS**. This receipt qualifies the actual hover
correction and its safety regressions, not the remaining full-frame differences.
Capture association is the immutable pre-delivery base/dirty source below; Git
determines the later reviewed commit/push. No original or historical proof is
normalized, overwritten or promoted to current acceptance.

## Cause and narrow source correction

The actual Root probe moves the pointer to physical(10,3), inside the accepted
user bubble, before resizing120×40 to120×320. Pinned **Terminal TUI** UserMessage
starts its local hover signal false on mount; actual MouseOver/MouseOut changes
the inner fill. Its hovered fill is `#1e1e1e`, unhovered fill `#141414`. The prior
native resize path discarded every non-tab pointer, causing the measured345-cell
completion mismatch. The donor web component is not this behavior's authority.

`TuiState::resize_mouse_position` now retains only the physical coordinates and
new frame when in bounds and no panel owns input. Hover re-hits **current painted
identity**; no old user/tool target is cached. Tab-region marquee retention still
requires the same admitted hovered tab ID. Resize still clears transcript
selection, painted ledger/generation, Message Actions/Revert presses, tab,
reasoning, footer and toast presses. It also clears the previously retained
exploration/tool press: a release after resize cannot authorize stale expansion.
Current-profile border, actual RAW/history, draft, controls and permission guards
remain unchanged; no new query/store/registry/poll or provider effect is introduced.

## Meaningful regression checks

The new existing-owner unit uses real ratatui TestBackend120×40→120×320→80×24,
Unicode/wrapped accepted text and current `#12ab34` profile. It verifies retained
physical hover/current background and border, immutable history/draft, invalidated
old down/up, then identical text with a **new message ID**: only a fresh paint and
press opens that exact Message Actions/Revert target. An out-of-frame pointer is
discarded. Baseline was RED before the source fix.

The existing real live/replayed MCP-card test additionally proves a pre-resize
tool press cannot expand even when its post-resize header has identical geometry.
Its baseline was RED before clearing exploration_down. Existing VIS41 same-ID
marquee and pending reload/quit receipt assertions remain. The reload test's old
pointer-None expectation was corrected to the new physical-pointer contract;
all held acknowledgement, draft, status and quit assertions were kept.

An initial test fixture resized Terminal buffers without resizing TestBackend;
pinned ratatui0.30.2/core0.1.2 draw autoresize correctly restored the old backend
dimensions. The test now resizes both and asserts actual frame dimensions, with
no production API/guard/deadline workaround. Whole closest TUI508 and strict
workspace Clippy passed; no tests were disabled or resource thresholds raised.

## Current gates

Actual full command, with child-only development resource settings:

```text
env CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 TMPDIR=/home/opencode/.cache/opencode-tmp/opencode sh -c 'cargo fmt --all -- --check && cargo test --locked --workspace --no-fail-fast -- --test-threads=1 && cargo build --locked && cargo build && cargo build --locked --release && target/debug/oc --help && target/release/oc --help'
```

All46 workspace targets: **1829 PASS / 0 FAIL / 11 unchanged opt-in ignored**;
508 TUI,111 binary,121 runtime integration and both55/34 PTY suites included.
Internal concurrent-scenario assertions remain unchanged. Strict locked all-target
workspace Clippy, fmt, builds and both help routes pass. The first full attempt's
old reload-pointer expectation failure remains recorded; the subsequent complete
46-target run is green, not a union of partial runs.

Release nine gates pass: startup four tests/19 routes, eight GET-only metadata
requests/no Responses, restored terminal; TERM01 actual raw/control/VT/resize/
focus/shutdown/crash/no-replay; VIS31 idle/high-rate input; S07 and actual AUD32.
TERM01 baseline/two-PTY RSS18380/21908 KiB, FD24→26, threads8→10, idle300ms1tick.
VIS31 idle1s0ticks/high-rate165/250Hz/workerlag0. S07 archive0→3000 retains24232 B,
window152/cache8581, no children/lag; HWM27444→27280 KiB. Actual AUD32 archive8→3000
pairs of16KiB plus logs/two active1KiB: HWM20436→22208 KiB, DB942080→152944640 B,
children0/eight threads/WAL0. Original bounds hold; this is not real-user strace.
Debug locked ELF was restored after all Cargo work and before capture/audit.
Physical source check:539 files/289596 lines/11614440 UTF-8 bytes. Existing soft
advisories application.rs5458/turn.rs5277 are unchanged in this slice; their next
natural settlement/publication seam is not a cosmetic file move or limit waiver.

## Actual paired frames, ownership and effects

Fresh Hover001 uses the unchanged seven real Root states per side: live partial,
parked Home, full completion, current-profile narrow draft, replay, explicit cancel
pending and cancelled. The same real painted Add/Home/busy-return/parked completion/
Home-only-close gestures preserve the Root and unsent draft. Generation0 exits
naturally, Sessions is mouse-reopened without replay; one explicit generation1
request is then cancelled through real two-Escape input and HTTP disconnect.

Each side has four requests, one real91B overwrite/one completed operation. Native
has one session/two turns, Completed/Cancelled; cancelled public text remains
**INCOMPLETE**, with no second tool/effect. RAW, recorded input/effects and model
tool graph remain unchanged. All39 grammars/78-case156-style corpus,85 frozen
Original cache seals and69 required first-witness consumptions hold;16 locals are
not claimed consumed. No early cache observer read was added during parking.

All **345 absolute inner cells x3–117/y2–4** match across binaries: completion
`#1e1e1e`, fresh replay `#141414`. Both hover flags are true. Each side's own
completion→replay changes345 BG-only cells/45961 pixels with equal cursor; those
different pointer/mount states are intentionally **not** an equality gate.
The full840 Write-row cells/gutters,14256 SDK source cells/2144 heading cells and
narrow480 source-row/160 heading-row cells have zero style difference. The current
`#12ab34` stripe, painted identities and completion/replay cursor(5,314) hold.

Ordinary56 captures:50 DIFFERENT/4 EQUAL plus4 authorized native-only entries.
Default temporal160 DIFFERENT, matched72 pairs/144 DIFFERENT, six owners/72 full
rasters per side,3999 samples/23674 commands, zero phantoms or raster cycles.
MCP three calls plus one real17B Shell effect per side preserve cold resources,
no replay and separate model257B/12-line versus UI960B/49-line output limits.
Full15-second/five-identical-observation stability, PGACK, opaque PNG, font,
cursor/parser/raster controls and strict comparator/tolerances are unchanged.

## Complete byte retention and source association

`hover-proof.tar.xz` plus its separate inventory preserves every original byte of
**1442 files / 100019846 logical bytes**, encoded3867612 B/TAR95119360 B. There are
1297 regular members and145 direct same-root/same-side byte-identical PNG aliases;
no cross-side aliases, PNG re-encoding, predictor/correction library or metadata
rewrite. Independent membership is sealed before encoding; whole inverse,
exclusive durable fsync and reopening/current-source audits pass.

Unchanged bounds:16MiB file+encoded,128MiB logical+TAR,2000 members,16MiB XZ
dictionary/32MiB decoder,384MiB Python AS and1GiB owned storage. Exact parent
pre-doc allocation1070129152 B/head3612672 B; hover-prefix artifacts are included.
No archival/deletion/offload in this slice: all fresh acquisitions and prior
Root diagnostics, Write1/R2/R3, grammar/wrap proofs, helpers, failures and reports
remain preserved. Retention/canonical-seal dev Node is not a production dependency.

Archive SHA256 `4a9ed18dd2089a92a4b3f5f6cea92bc7613e46ba5b8dffce9929a307d666edd8`;
TAR `6cbda583f2d31af9389a7b955df480027ed49e165c20ce38352a077744ecadd1`;
member inventory `f43b206daf7046524a3f380540259150db02a0f94efd1243bceeec082b4c9907`.
Actual capture base `c1b94a3ed3ab33519775ffbef062b36c91652aac`;
dirty code `49a68a71c8f1d63cb9eff02826d7c40fae93780b8bbff3921b7c5d885f5a783f`;
707-input canonical source `c6a5e9f5b1d4a71196e10954bd6d73163e92dd4a33e3b847f86cb8f45c7bf1ab`;
Native ELF `3f133c48774cc5ba162420b1f7d30eac8dde5b58b35416a23005fed5389329e0`;
Original ELF `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`;
retainer `feb55be43e79d7ce5e5338ae275a4bcebbd8faf68c364a837a84ee80ffa8eb04`.

Independent audit restores all1442 original bytes and checks all707 current inputs
in three real locked producer builds, donor2670273,230 complete opaque PNGs/
817248 styled cells/108076032 pixels, all144 temporal caret associations, entire
strict comparator outputs, body/styles, cache and effects. Python peak/HWM
303224/273232 KiB, inherited jitless dev-Node peak359296 KiB under unchanged AS.
Optional scratch report: `hover-independent-20261011-sole-verifier/report.json`;
required final proof and provenance are self-contained above, not offloaded there.

## Remaining full contract / next semantic work

Direct **222 DIFFERENT / 6 EQUAL plus4 authorized native-only**, matched144
DIFFERENT. Live-partial full4800-cell grid/cursor/PNG genuinely equals Original.
Complete/replay each differs by one real clock cell/65 pixels: Native7.9s versus
Original7.8s. Narrow differs by one clock cell/97 pixels including32 non-clock
right-edge pixels x673/y352–383. Parked Home differs47 cells/1873 pixels; cursor
equals. Cancelled has two clock cells/135 pixels, second turn1.6s versus1.7s.
Composer caret Native(21,18)/Original(24,18) remains different; search(14,9) matches.
One genuine Original hidden parser(3,15) with raster owner(24,18) is preserved.
**Blink is unqualified.** None of these differences is masked or normalized.

Continue actual raster-edge/Root Home/composer-caret/blink and real-pacing owners,
then every remaining Thoughts/family/forms/DCP/MCP/auth outcome and **ALL** frozen
R1–R6/VIS01–VIS45/supplements. Final current-source R6/V09 is mandatory before
task finish. Do not stop administratively, reduce acceptance, promote body/episode
equality to T44 PASS, or rewrite historical source/capture associations.
