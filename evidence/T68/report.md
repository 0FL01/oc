# T68 — native shared runtime qualification

Status: PASS — T68 only; R1–R5 verified
Date: 2026-10-18
Source base: `dc889207fb20b2985a49827ddf0f101ae8d1810f`
Reviewed source commit: `4af8c3ad646d283c5d7c79f269594931b4bb685c` (pushed to verified `origin/master`)
Contract: `docs/goals/2026-10-17-shared-tui-runtime.md`, R1–R5; SRV01–SRV05.

## Result

T68 implements one automatically elected native owner per UID/data root over a
private Unix-domain socket. TUI, `run`, `auth` and `sessions` use that authority;
ordinary second TUI startup no longer opens another Db. The actual retained Db,
exclusive flock, recovery and shared shell/child/terminal/credential resources have
one owner. No storage migration, schema, persistent retry ledger, TCP control
listener, privileged executor, Node/Bun host or public service manager was added.

Same-Location clients share deck/order/active route and bounded materialized live
state. Snapshot revisions and gap resnapshot repair observer projections without
resubmitting work. Unsent text/PNG editor state and exact model-draft receipts stay
client-local. Independent session root runners share canonical Location sources
and resource owners, not one mutable session/profile selection. Reservation precedes
async preparation and lasts through settlement. Issued requests/full batches retain
captured model/config/MCP authority; scoped controls cannot cancel a later turn or
change a sibling's execution.

The TUI lease count is global across Locations. C keeps A/B interactive work alive
after their viewers disconnect; the final global TUI disconnect uses ordinary Esc
cancellation, durable Interrupted and joined cleanup. Finite run/auth/sessions leases
are separate. Headless work survives last-TUI close and releases only its own work
on completion/client loss. Owner crash remains a truthful unknown/recovery boundary,
not a fabricated clean Interrupted or permission to replay an ambiguous mutation.

## Required outcomes and direct evidence

| Outcome | Direct evidence surface | Final result |
| --- | --- | --- |
| R1 / SRV01 | `pty_t42/shared_runtime.rs`: actual-ELF simultaneous ordinary TUI startup, one owner/retained lock/recovery identity, owner ELF and bridge identity, lost startup intent; endpoint stale-socket and framing fixtures | PASS, debug and rebuilt release |
| R2 / SRV02 | Actual paired PTYs with controlled provider stream: mirrored deck/active/live, late observer, local draft, old-client quit, one execution/effect; native gap resnapshot and exact attachment/model receipts; unchanged PNG/history codec and clipboard regressions | PASS, debug and rebuilt release |
| R3 / SRV03 | Actual three-Location A/B/C lifetime, last disconnect/reopen and no auto-resume; headless overlap, EOF/crash, auth/terminal-controller handoff; authority idle retirement after last operation interest | PASS, debug and rebuilt release |
| R4 / SRV04 | Same-directory provider barrier before release of either request and same-session pre-effect reservation refusal; actual stale-T1/picker and source/destination move overlap; captured publication/MCP lifecycle regressions | PASS, debug and rebuilt release |
| R5 / SRV05 | Real UDS `peer_cred`, private root/socket and descriptor-based registration checks, forged PID/foreign UID metadata/permissive socket/symlink/FIFO refusal; bounded wire/IPC and unchanged resource/durability suites | PASS boundary fixtures; real root/stfu pair NOT_RUN (owner-operated only) |

The foreign-UID metadata refusal is not described as a live cross-user test. Real
root/stfu launches are owner-operated only under SRV05; no authoring sudo, UID switch
or user-owner process manipulation was performed. Native owner/tools run under the
launching unprivileged UID, with no cross-user executor bridge.

## Checks

All product fixtures use isolated roots, fake local providers and their own
processes. No paid API campaign, user-store mutation or forced restart of a working
owner was needed. Cargo commands use one build owner, the normal `target` directory,
the approved `TMPDIR` (8 build jobs for debug, 2 for fat-LTO release).

| Check | Command / evidence | Result |
| --- | --- | --- |
| Native boundary repairs + authority | `.local/tools/cargo-nextest nextest run --locked -p oc-adapters --lib -E 'test(native::) \| test(application::authority::)'`; run `7eab9d6a-c34f-440a-b924-175950bf8701` | 9/9 PASS, 2.540s test phase |
| Full offline gate | `NEXTEST_SUCCESS_OUTPUT=final bash scripts/test.sh`; run `9179f01a-6711-4888-bb25-e35c0453a865`, `.local/t68-offline-gate-qualified-2.log` | 2085/2085 PASS, 12 existing opt-in skips; 381.170s tests, 430s wall; fmt/strict Clippy/doctests/debug build/help/version PASS |
| Rebuilt release actual ELF | `nextest run --release --locked -p oc2 --test pty_t42 --test recovery_startup --test configured_workspace -E 'test(shared_runtime) \| test(isolated_binary_startup_routes) \| test(isolated_discovery_startup_routes) \| test(aud14_aud15_aud17_binary_loads_isolated_a_b_workspace_and_pins_skill)'`; run `4b096bed-d4c2-4858-a2bb-eb64869db277`, `.local/t68-release-qualification.log` | 12/12 PASS, 55 outside-filter cases not selected; 29.246s tests, 426s wall including 6m36s cold build; longest case 27.003s |
| Resource measurements | Existing unchanged archive, equal-active-view and 120-turn workloads; aggregate native owner/client/descendant accounting | PASS; numeric receipts below, soak budget RECON recorded |
| Ordinary/locked debug artifact | `cargo build`; `cargo build --locked`; `cargo build -p oc2 --bin oc2 --locked`; debug/release help/version; `.local/t68-final-build.log` | PASS, exit0; final debug ELF 697854200 bytes, mode755 |
| Repo documentation/journal | `python3 scripts/check_docs.py`, `python3 scripts/progress.py check`; `test_check_docs.py`, `test_progress.py` | PASS; 14/14 and 15/15 Python checks |
| Diff/delivery | Reviewed staged diff/credential-shape screen, `git diff --check`, ordinary push to verified `origin/master` | PASS; source `4af8c3ad646d283c5d7c79f269594931b4bb685c`, 110 paths, source push ACK |

The full offline script preserves the complete default unit/integration set, the
12 existing opt-in skips, separate workspace doctests, strict all-target Clippy,
fmt and locked debug/help/version gates. Detailed T68 completion does not claim
whole-product A01–A13/READY or finish/resume T44/T61; affected existing gates remain
regressions rather than newly duplicated scenario ownership.

### Resource receipts (KiB unless stated otherwise)

- AUD32 actual binary: archive pairs 8→3000 × 16384 bytes plus turn logs, equal active
  pairs 2 × 1024 bytes. Small/large aggregate peak HWM 61704/62560; large incremental
  HWM 62548 < unchanged 65536 bound. Peak PSS 15169/15269; one owned child each;
  DB 954368/152956928 bytes, WAL 0/0. Large DB remains >100MiB.
- S07 equal view, 0/3000 archived rows: two native application processes; RSS
  72276/71792, PSS 41280/45354, HWM 72820/72932; identical retained bytes 31232 and
  152 window rows, unexpected children 0/0, queue lag 0/0.
- VIS38 DCP: archive 0/3000, source 0/49152000 bytes; two native processes, RSS
  66156/76388, PSS 40667/53581, seven threads, unexpected children 0/0,
  replay requests 0/0 and actual runs 1/1.
- Frozen 120-turn soak: RSS baseline/epoch1/end 8184/30364/31220; epoch DB deltas
  4302224/1176320 bytes, identical 123 rows each. Full-run case 32.662s required
  RECON: first epoch 17.720s vs second 13.570s, already exclusive scheduling.
  Unchanged narrow run `2fdad410-0897-4290-9766-142272352443` passed 28.172s with
  epochs 13.328s/14.133s. Wall-time syscall profile measured 2323 fsync calls/23.304s
  in an instrumented run: required FULL durability dominates I/O. No new timing
  exception is claimed; precise latency-variation causality is not claimed either.

## Significant diagnosis and corrections

- Review found a new IPC race: priority `CancelRequest` could overtake its ordinary
  request and be lost. A real framed UDS fixture first reproduced admission of the
  cancelled fresh UserShell, then verified exact pre-admission cancellation, no
  session creation and no shell effect. Bounded distinct future-cancellation IDs
  are consumed before execution admission; stale IDs are ignored, skipped IDs pruned,
  overload closes the peer. No durable retry/cancellation ledger was introduced.
- A FIFO at `native-owner.json` blocked in `open` before regular-file admission.
  Its deterministic no-writer regression first failed, then passed with O_NONBLOCK
  alongside unchanged NOFOLLOW/CLOEXEC and descriptor/UID/mode/size checks. Both red
  reproductions: run `29c8f5e4-033c-409a-b1e1-3394facdc6f1`; green suite above.
- The prior full run `847b8495-3420-44b1-9bc5-b090dce948fe` was RED: 2082 PASS/1 FAIL,
  12 existing opt-in skips, 378.306s test phase. V04 searched a complete reply in raw
  terminal diff bytes, although ratatui legitimately reused unchanged cells. Painted
  screen diagnostics showed the exact response and model/agent. Ten unique reply
  checks now use the existing reconstructed screen grid, retaining all ten main
  request/two title-request wire assertions, restart/persisted preferences and original
  deadlines. Targeted run `6a69d8ee-ede2-441c-937a-c039f3b54c6a`: PASS 6.570s.
  The same demonstrated raw-delta defect in one modal scenario was repaired with six
  unique painted-reply checks, preserving all six main wire assertions, persisted
  selection, keyboard/modal responsiveness and termios/exit checks. Rebuilt run
  `50b4470d-344e-4e96-a6b8-04939a8fcd12`: 3/3 PASS, 17.203s; modal 7.952s and the two
  previously timed-out startup scenarios 1.996s/7.249s.
- Run `4e226a42-9ea1-418e-928e-1d61d8ea1d5f` was invalidated when the owner cleaned
  part of `target` during execution: 1128 failures were missing test ELFs (exit70/ENOENT).
  It reported 953 PASS/1132 FAIL/12 opt-in skips, not qualification. The four assertion/
  timeout failures were isolated after rebuild rather than hidden by those launch
  failures. The three PTY cases passed above. Serial unchanged approval/storage RECON
  `b4ef8666-37b9-4021-8779-fdc5d4a63299`: 5/5 PASS, 60.195s; storage cases
  13.658s/13.451s/19.798s, approval commit/restart 11.890s and reconnect/reply 1.390s.
  These are back within original budgets; exact cleanup/I/O causality is not claimed.
  No deadline, workload or durability assertion was relaxed.
- The unchanged 120-turn soak initially exceeded 30s. RECON located full TERM-grace
  waits on already zombie-only groups. `/proc` liveness now skips only that wait;
  unreaped leader/PID reservation, final group KILL and full grace for live
  TERM-ignoring groups remain. Nearest regression tests passed; no workload, WAL FULL,
  resource bound, durability/security assertion or timeout was weakened.
- Full run `7fafe172-6823-4537-8fd3-60fb7197625d` reached 2084 PASS/1 FAIL/12 existing
  skips (371.976s tests, 916s wall including cold build). The sole failure was the
  Python MCP fixture's missing close receipt, after its sensitive-identity checks
  passed. A deterministic worker-directed TERM reproduced a Python signal-handler
  race despite main's mask: an unmasked worker could schedule main's handler between
  `closed=True` and receipt append. Workers now inherit blocked TERM while main
  remains the signal recipient. The same reproduction changed 0 receipts to exactly
  1; affected `mcp_stdio` run `f028f997-4caf-4159-b319-5f71812841fe` passed 21/21 in
  1.953s with its one pre-existing opt-in skip. No production teardown, security
  assertion, workload or timeout changed.
- The real 30000ms discovery-deadline case has an existing owner-approved <=40s
  exception documented next to the test (2026-10-17), not an agent-created exception.
- Documentation validation's isolated fixture omitted the T66 allowlist schema source.
  Its setup now copies that exact source, without changing validation or assertions.
  `test_check_docs.py` 14/14 and `test_progress.py` 15/15 passed before closeout.

## Changed areas

- `oc-adapters/src/native*`, `application/authority*`: election, peer admission,
  fragmented bounded RPC, lifecycle, sole resource owner, root reservation/live state.
- Existing application/composition/runtime publication, descendants, MCP retirement,
  scoped controls and cleanup; shell zombie-only wait correction and resource fixtures.
- `oc-core`: concrete local/native CoreApp transport and typed wire/workspace DTOs.
- `oc/src/{bootstrap,tui_cmd,headless,auth_cmd,cli}.rs`, `tui_cmd/shared.rs` and
  `oc-tui/src/app/*`: atomic consumer cutover, mirrored adoption, local drafts/receipts,
  typed owner-loss quit and auth/terminal handoff.
- Existing actual-binary/PTY/startup/durability/archive targets and support helpers;
  current architecture/config/navigation/skill docs and the docs-validator fixture.
  No archived evidence leaf, upstream snapshot, storage migration or user config was
  rewritten. Existing >5k application/turn owner seams are explained in CODE_MAP;
  no new native owner file exceeds that soft threshold.

## Compatibility and residual boundaries

- First owner retains its startup environment. A subsequent client cannot replace it;
  a new ELF/environment takes effect after all current leases finish normally.
- A legacy in-process owner without a native endpoint still requires the owner's
  normal quit. Do not remove `oc.lock`, kill its process or silently replace it.
- Shared workspace does not isolate concurrent filesystem edits; preimage/CAS/policy
  checks remain, and worktrees remain the filesystem-isolation mechanism.
- Owner disconnect does not prove whether a mutation committed. No automatic resend,
  hidden auto-resume or fabricated success/refusal was added.
- T44 remains temporarily paused/incomplete and T61 paused. This report closes only
  T68; it is not a whole-product READY claim.

## Delivery

Implementation `4af8c3ad646d283c5d7c79f269594931b4bb685c` was committed and pushed
normally to the verified `git@github.com:0FL01/oc.git` `master`; no force, tag or
release publication. This report and generated T68 journal state are the following
closeout commit, not an amendment to historical evidence.

Final debug artifact: `/home/stfu/ai/agent-frameworks/oc/target/debug/oc2`.
`oc2-dev` is build-ready for its next ordinary launch; the owner's interactive
alias was not launched. An existing owner keeps its old ELF/startup environment
until its current leases finish normally. No storage schema/migration or live
user-data/configuration change was made.
