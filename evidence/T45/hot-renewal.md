# T45/R9/DCP11 — whole-past hot renewal and native compact recovery

## Coordinator qualification — 2026-10-03

The frozen whole-past renewal atomic is independently reviewed and verified.
The execution source is `44bb7d7d39990c949f91c46a477a92e951cecb69` plus the
reviewed source diff; current HEAD `f2e8e7aeff540c3ad1dc395a0f60cf77ef792ddd`
adds owner documentation only. The historical handoff below is preserved.

- Reviewed replacement/protection/occurrence projection, metadata-first compact,
  transactional checkpoint/RAW coupling and conversation/Fork migration seams.
  Two read-only scoped reviews found no concrete introduced counterexample;
  static review is not a substitute for the following native proofs.
- Fresh owner filter `renewal` passed six tests, including host-overflow recovery;
  the explicit host-recovery filter passed one. Workspace fmt, strict locked
  all-targets Clippy and the normal debug rebuild exited zero.
- After that last build, both normal ELFs independently passed the 8-versus-4100
  inactive-archive comparison: each workload made 43 physical POSTs, performed
  twelve standalone renewals and three compacts, preserved RAW and executed the
  effect once. Equal-hot checkpoint/request peaks were 38,193/55,173 bytes,
  selected graph transfer at most two rows/36,798 bytes, live ancestry depth zero.
- Additional actual native proofs passed for debug host overflow, release model
  overflow, debug pre-COMMIT failure and release post-COMMIT crash. Ordinary
  overflowing admission sent zero requests; manual `/compact` recovered. Fault
  rollback published no false progress; restart dispatched no cached work.
- These are eight independent native workloads (four comparison workloads plus
  four directed cases), not a repeated full viewport/backend matrix. Their managed
  output is `tool_10127efbd0012mBQ5nQX0cabxQ`. All owned groups and HTTP/PTY
  workers joined before exact temporary cleanup.
- Debug `78801e2aafb7add6ebc12480d607295c4d7e7298314b944824b9fcb88a132a60`
  and release `b6e48c707ef7deb13ab4ed86422a912c468913be5c7b797aa6b1752fbd6096a9`
  remained unchanged through all direct proofs. No intervening Cargo build.
- Python47, docs/progress structural validation and diff checks exited zero.
  The complete current compiled-source workspace gate below remains 1467/0/10;
  no production source changed after it. No paid/live/authoring-auth inputs used.

Whole T45/R9, defaults/controls/children, newly planned task packs and donor
upgrade remain separate. This qualification does not resume T44 or replenish
the exhausted T27 allowance and does not claim whole-product readiness.

## Current result — frozen whole-past atomic qualified

Rust execution base: `44bb7d7d39990c949f91c46a477a92e951cecb69` plus the dirty
implementation listed below. Current Git HEAD is
`f2e8e7aeff540c3ad1dc395a0f60cf77ef792ddd`: the owner's intervening commits change
documentation/planning only, with no Rust/Cargo/scripts/evidence difference from
the execution base. Owner commits `3a4589327` (exact packs/task memory) and
`f2e8e7aef` (qualified DCP3.2.0 donor-upgrade planning) are preserved. Active T45/0005 stays
unchanged. No source changed after the final builds; native receipts below follow
the last builds without intervening Cargo. This is an uncommitted-source handoff.

| Frozen obligation | Status | Evidence |
| --- | --- | --- |
| Whole-covered omission consumes old blocks; authored references resolve once | PASS | omission/reference owner, legacy depth boundary, twelve standalone replacements |
| No persistent ancestry or historical member-vector exhaustion | PASS | standalone endpoint coverage; 4100 covered legacy members; active ancestry0 |
| Old summary/requirement/investigation/native media/control-call arguments really forgotten | PASS | actual updated-requirement mixed wire, old sentinels absent, RAW hash unchanged |
| Current explicit protections reselected; released historical suffix not pinned | PASS | original-ID/derived-legacy protection, recent-source aging, compact/recompress/release/Fork owners |
| Compact consolidates checkpoint/selected facts/fresh tail without RAW/cold dependency | PASS | three actual manual compacts, restart, deleted source image, no Git |
| Host/model overflow recovery selects floor before content | PASS | same-session normal admission fails with zero POST; actual PTY compact and continue recover |
| Indexed bounded active/addressed metadata, no inactive archive member/mark loading | PASS | exact index-plan RED repair; 8/4100 blocks+members+marks equal HOT transfer/serialization |
| Reused-call decisions retain producer identity; unused archive slots cannot alias fresh calls | PASS | causal owner, nullable live eligibility and versioned owned-frame mode |
| Immutable RAW/provenance/Undo/Redo/Fork and delivered current-task seam preserved | PASS | full suite; current root8/root32/child32; A→B; RAW fault/crash; family/RAW owners |
| Cancel/finite retry/effects/crash/COMMIT failure and truthful progress | PASS | atomic typed receipts; no false progress on fault/cancel; restart zero replay; effect once |
| Workspace and normal native source/hash association | PASS | 42 summaries1467/0/10, strict Clippy/fmt, both normal builds and direct final receipts |
| Defaults40/55/false, allowSubAgents/full DCP12 controls, profiles/parallel jobs | NOT_RUN | separate subsequent atomic |
| New owner task-pack lane renewal, CTX01/CTX02 exact packs and quote authority | NOT_RUN | separate owner planning introduced at current HEAD; outside this frozen whole-past atomic |
| Newly planned qualified DCP3.2.0 donor upgrade | NOT_RUN | separate owner amendment at current HEAD; no donor baseline/config/control upgrade in this atomic |
| T53/T56, paused T44 paired pixels and whole R9/T45/READY | NOT_RUN | unchanged separate scope; no whole-product claim |

### Minimal representation and RED causes

Unreleased migration9 extends existing tables only: nullable `compression_blocks.hot`
and `session_checkpoint.selection`, nullable projection-row `active` and indexed
active/legacy/invalid selectors. A fixed versioned `dcp.projection_owned.` preference
selects legacy compatibility versus owned live eligibility. No new table/store/crate,
dependency, archival framework, epoch clock or hash/signature manifest was added.
New standalone coverage stores endpoints, not every historical member. Old authored
blocks/RAW/immutable context objects stay durable; inactive live selections are
released. Current/native and historical replacement, scoped marks and progress
commit atomically. Existing old snapshots restore trailing NULL fields; explicit
Fork rebases actual selected source references and owned-frame preference.

The historical frozen checkpoints below retain every counterexample and repair:
omitted covered blocks previously raised overlap; 4100 members hit a historical
row limit; valid legacy depth gained an artificial extra level; old protection was
lost or blindly inherited; overflow compact refused before choosing its floor;
omitted producer rank reassigned a reused-call decision; archive-wide DELETE touched
unrelated marks; unused legacy slots later hid fresh producers; invalid metadata
could recall RAW; JSON arrow predicates missed the existing active expression index;
initial anchors falsely marked a completed assistant unfinished; compact left live
covered roots; and first-producer compatibility retained old `compress` arguments
containing obsolete summary/requirements. Each source repair followed its recorded
falsifiable RED and stayed in the existing owner. Projection-control-only groups
have no default initial pin; explicit protections and genuine mixed native producers
still retain their intended semantics. Legacy derived literal facts keep actual
source ranges, not invented original message IDs, and reselect under current policy.

Five first-full assertions/fixtures and the later AUD20 removal count were reconciled
to the approved new observable contract with stronger actual before/after/RAW/atomic
assertions. Resource/time thresholds, partial-commit denials, protected/error/reused
identity checks and family/Fork guards were not weakened; no test was disabled.

### Final gate logs — exact paths, exits and counts

All Cargo commands are serial offline/locked, jobs3 / test threads1, prescribed
TMPDIR and unchanged1798-second operational watchdog. Precompile is separate.
Every current final gate below exits0; expected RED failures remain lossless.

| Exact log path | Exit | Result / seconds |
| --- | --- | --- |
| `evidence/T45/hot-renewal-logs/self-summary-final-fmt.log.gz` | 0 | workspace fmt check, 2.640 |
| `evidence/T45/hot-renewal-logs/self-summary-final-clippy.log.gz` | 0 | strict workspace locked offline all-targets `-D warnings`, 3.677 |
| `evidence/T45/hot-renewal-logs/renewal-final-precompile.log.gz` | 0 | workspace locked offline `test --no-run`, 6.710 |
| `evidence/T45/hot-renewal-logs/renewal-final-workspace.log.gz` | 0 | **42 summaries; 1467 passed / 0 failed / 10 ignored**, 1010.589 |
| `evidence/T45/hot-renewal-logs/renewal-final-debug.log.gz` | 0 | normal workspace debug build, 8.542 |
| `evidence/T45/hot-renewal-logs/renewal-final-release.log.gz` | 0 | normal workspace release build, 148.017 |
| `evidence/T45/hot-renewal-logs/renewal-final-debug-help.log.gz` | 0 | normal debug help, 0.012 |
| `evidence/T45/hot-renewal-logs/renewal-final-release-help.log.gz` | 0 | normal release help, 0.015 |
| `evidence/T45/hot-renewal-logs/renewal-native-compare-debug-02.log.gz` | 0 | actual small/large mixed workflow, 24.147 |
| `evidence/T45/hot-renewal-logs/renewal-native-compare-release-02.log.gz` | 0 | actual small/large mixed workflow, 12.289 |
| `evidence/T45/hot-renewal-logs/renewal-native-host-debug-02.log.gz` | 0 | actual host overflow recovery, 3.642 |
| `evidence/T45/hot-renewal-logs/renewal-native-model-debug-02.log.gz` | 0 | actual model overflow recovery, 3.396 |
| `evidence/T45/hot-renewal-logs/renewal-native-fault-debug-02.log.gz` | 0 | before-COMMIT failure/recovery, 4.866 |
| `evidence/T45/hot-renewal-logs/renewal-native-cancel-debug-02.log.gz` | 0 | actual cancellation/recovery, 5.061 |
| `evidence/T45/hot-renewal-logs/renewal-native-crash-debug-02.log.gz` | 0 | after-COMMIT crash/restart, 4.766 |
| `evidence/T45/hot-renewal-logs/renewal-native-host-release.log.gz` | 0 | actual host overflow recovery, 2.133 |
| `evidence/T45/hot-renewal-logs/renewal-native-model-release.log.gz` | 0 | actual model overflow recovery, 1.967 |
| `evidence/T45/hot-renewal-logs/renewal-native-fault-release.log.gz` | 0 | before-COMMIT failure/recovery, 2.485 |
| `evidence/T45/hot-renewal-logs/renewal-native-cancel-release.log.gz` | 0 | actual cancellation/recovery, 2.661 |
| `evidence/T45/hot-renewal-logs/renewal-native-crash-release.log.gz` | 0 | after-COMMIT crash/restart, 2.402 |
| `evidence/T45/hot-renewal-logs/renewal-current-hot-debug.log.gz` | 0 | root8/root32/child32/retry/partial/RAW, 76.003 |
| `evidence/T45/hot-renewal-logs/renewal-current-hot-release.log.gz` | 0 | same current-task regressions, 69.221 |
| `evidence/T45/hot-renewal-logs/renewal-current-switch-debug.log.gz` | 0 | A→B/current RAW fault/crash/restart, 17.700 |
| `evidence/T45/hot-renewal-logs/renewal-current-switch-release.log.gz` | 0 | same current-task switch proofs, 14.014 |
| `evidence/T45/hot-renewal-logs/renewal-nearest-retry.log.gz` | 0 | current T50 retry, 4.116 |
| `evidence/T45/hot-renewal-logs/renewal-nearest-tool.log.gz` | 0 | issuing batch, 2.823 |
| `evidence/T45/hot-renewal-logs/renewal-nearest-compact.log.gz` | 0 | selected compact model, 1.356 |
| `evidence/T45/hot-renewal-logs/renewal-nearest-cold.log.gz` | 0 | registered ColdArtifact chain, 5.796 |
| `evidence/T45/hot-renewal-logs/renewal-nearest-mutation.log.gz` | 0 | actual release mutation, 0.936 |
| `evidence/T45/hot-renewal-logs/renewal-final-test-temp-cleanup.log.gz` | 0 | six exact joined test directories, 0.037 |
| `evidence/T45/hot-renewal-logs/renewal-final-python-02.log.gz` | 0 | Python **47/47**, 14.109 |
| `evidence/T45/hot-renewal-logs/renewal-final-docs-02.log.gz` | 0 | current documentation check, 0.199 |
| `evidence/T45/hot-renewal-logs/renewal-final-progress-02.log.gz` | 0 | generated-view structure check, 0.193 |
| `evidence/T45/hot-renewal-logs/renewal-final-size.log.gz` | 0 | changed-source advisory, 1.732 |
| `evidence/T45/hot-renewal-logs/renewal-final-evidence-validation.log.gz` | 0 | AST/42-summary totals/Python47/native metrics/hash/ownership/quota, 0.650 |

Current normal ELF SHA-256, verified unchanged before/after the actual workloads:

```text
debug   78801e2aafb7add6ebc12480d607295c4d7e7298314b944824b9fcb88a132a60
release b6e48c707ef7deb13ab4ed86422a912c468913be5c7b797aa6b1752fbd6096a9
```

### Actual native semantic and resource receipts

The final fixture starts with OLD_REQUIREMENT/old path/result/move and OLD_SUMMARY,
then accepts one actual changed requirement/path/result/next-move user directive.
Every subsequent prompt is generic continuation, not facts reinjection. Actual
auxiliary request input must contain the known current control facts. Twelve range
replacements, three manual compacts and restart continue that state while actual
wire omits OLD_REQUIREMENT/OLD_SUMMARY/OLD_REASONING/OLD_INVESTIGATION/
OLD_TOOL_PAYLOAD/OPAQUE_OLD/input_image. The first real native image/protocol group
is compatible, then renewal genuinely forgets it. The owned source image was
physically deleted after its first Read; the project has no Git dependency. Canonical
first-turn RAW stays byte-exact at all later requests/restart and the shell effect
file contains exactly one committed effect.

| Current measurement, both normal builds | 8 inactive blocks/members/marks | 4100 inactive blocks/members/marks |
| --- | ---: | ---: |
| Peak observed pre-POST current checkpoint | **38193 B** | **38193 B** |
| Peak actual main request JSON | **55173 B** | **55173 B** |
| Maximum graph transfer delta | **2 rows / 36798 UTF-8 B** | **2 rows / 36798 UTF-8 B** |
| Causal identity transfer delta | 0 rows / 0 B | 0 rows / 0 B |
| Final current checkpoint / latest wire | 1837 / 14491 B | 1837 / 14491 B |
| Active blocks maximum / persistent ancestry depth | 1 / 0 | 1 / 0 |
| Physical POSTs / durable dispatches | **43 / 43** | **43 / 43** |
| Actual main / compact / title lanes | 39 / 3 / 1 | 39 / 3 / 1 |
| Effect count / implicit RAW pages | 1 / 0 | 1 / 0 |
| Peak whole-process VmHWM, debug | 65683456 B | 66379776 B |
| Peak whole-process VmHWM, release | 31322112 B | 31272960 B |
| Last DB bytes, debug / release | 1179648 / 1171456 | 6668288 / 6668288 |
| Last WAL bytes / occupied4096-page frames, debug | 4144752 / 1006 | 4132392 / 1003 |
| Last WAL bytes / frames, release | 4132392 / 1003 | 4293072 / 1042 |
| Last-process read/write bytes, debug | 0 / 5750784 | 0 / 6103040 |
| Last-process read/write bytes, release | 0 / 5709824 | 0 / 5935104 |
| Last-process rchar/wchar, debug | 42849911 / 5474736 | 45196919 / 6236924 |
| Last-process rchar/wchar, release | 41989759 / 5424180 | 44729983 / 5708491 |
| Last-process syscr/syscw, debug | 18511 / 3367 | 19084 / 3723 |
| Last-process syscr/syscw, release | 18301 / 3087 | 18970 / 3155 |

Original RAW hashes debug-small/debug-large/release-small/release-large:

```text
50c607fccc72b2adf360800bca16898f9b374023a1a35bb8237038ed4cc8c1e5
947390ba7da6883c9cb3c9dbf9d0486c9536fc1eb6c359db36a0fbcc62d6167f
e1cc21a780eb03fbb3a6ad101524b0f9394ca854af7e2aed50b231456264160b
e2cb4a1afd6f9e30f5207fb7c2e40caa03d5caab734dd1c61a27bdae8b5cf2dd
```

Host recovery starts with a closed legacy assistant larger than16 MiB; model
recovery uses270000 B with the SAME valid65536-context/2048-output configuration
that already completed the prime (67500 input tokens plus fixed/schema exceed
context). Ordinary same-session normal admission fails exit1 with zero provider
POSTs. The real PTY `/compact` dispatches one bounded summary and continues the
same session: six total POSTs=main4/compact1/title1, effect1. Host diagnostic is
capacity_exceeded; model uses the existing generic InvalidConfig/initialize wrapper,
not a changed invalid fixture configuration. Progress records admitted_renewal,
source-selected JSON2 B/replacement177 B, omitted_whole_prefix=true, priorselection0/
newselection2. It does not pretend to have measured/copied the overflowing before wire.

Fault and cancel cases each have12 POSTs=main9/compact2/title1: old RAW/live pack
survive, no false success progress, explicit later compact succeeds and restart
dispatches zero replay POSTs. Crash has12=main10/compact1/title1: committed checkpoint
and RAW survive SIGKILL, open work is unknown, restart dispatches zero replay and
effect remains once. Cancellation uses actual running-operation display, two
CSI-u Escape presses150 ms apart and durable cancelled acknowledgement; no T44
footer/pixel claim. Existing finite retry/cancel/backoff/effect/security limits stay.

Measurement scope: graph counters count actual transferred graph strings/hot JSON;
identity counters count transferred producer metadata. They do not measure every
SQLite scalar/accounting/addressed query or internal page read. Identity0 here is
because no owned eligible decisions remain; unrelated NULL archives do not trigger
metadata loading. The causal owner separately exercises retained decisions. Six
existing counters cover HOT journal strings, explicit RAW pages and bounded UI parts.
Counters reset by actual process PID on restart. VmRSS/VmHWM are physical whole-
process memory, not isolated Rust allocator retention. WAL frames are occupancy/
reuse, not cumulative writes; I/O is the last process after restart, not lifetime
sum or SQLite-only amplification. Checkpoint peaks are observed pre-POST values,
not every callback serialization. Disk RAW/archive growth is allowed. No product
token/byte/result/step/per-file/resource threshold was raised.

### Changed paths and exact ownership cleanup

```text
crates/oc-adapters/src/dcp.rs
crates/oc-adapters/src/dcp/renewal_tests.rs
crates/oc-adapters/src/instructions.rs
crates/oc-adapters/src/runtime/context.rs
crates/oc-adapters/src/runtime/tests.rs
crates/oc-adapters/src/runtime/turn.rs
crates/oc-adapters/src/runtime/compaction_renewal_tests.rs
crates/oc-adapters/src/runtime_compaction.rs
crates/oc-adapters/src/runtime_compaction_tests.rs
crates/oc-adapters/src/storage.rs
crates/oc-adapters/src/storage_compaction.rs
crates/oc-adapters/src/storage_conversation.rs
crates/oc-adapters/src/storage_conversation_tests.rs
crates/oc-adapters/src/storage_dcp_view.rs
crates/oc-adapters/src/storage_fork.rs
crates/oc-adapters/src/storage_fork_context.rs
crates/oc-adapters/src/storage_shell_jobs.rs
crates/oc-adapters/src/storage_turn_history.rs
crates/oc-adapters/src/tools/turn_history.rs
crates/oc-adapters/tests/dcp_atomic.rs
crates/oc-adapters/tests/runtime/context.rs
crates/oc-adapters/tests/session_rename.rs
crates/oc/tests/dcp_runtime.rs
crates/oc/tests/pty_t39/lifecycle.rs
docs/CODE_MAP.md
evidence/T45/check_hot_renewal.py
evidence/T45/native_hot_renewal.py
evidence/T45/hot-renewal.md
evidence/T45/hot-renewal-logs/
```

Current coupled-proof roots below are joined/removed, relative to the exact owned
parent `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`:

```text
t50-background-owned-x0z05yst
t50-background-owned-up6xxvz1
t50-background-owned-ffmk_aeh
t50-background-owned-vlcl8q7f
t50-background-owned-_of4v1x5
t50-background-owned-9azzqq85
t50-background-owned-13du62oe
t50-background-owned-xyjrf1v_
t50-background-owned-y38jsbnn
t50-background-owned-9z4y8uy8
t50-background-owned-jaone63g
t50-background-owned-5h6jv7t_
t50-background-owned-9l3jzjxb
t50-background-owned-2_0hnkus
oc-tui-workspace-primary-1868100
oc-tui-workspace-stale-1868100
oc-tui-workspace-vanished-1868100
oc-tui-workspace-primary-1897175
oc-tui-workspace-stale-1897175
oc-tui-workspace-vanished-1897175
```

Every fixture handler/server/PTY reader/process is joined before exact TempDir
removal. Known test_db leftovers are removed only after the exact test PID exits;
earlier cleanup receipts retain their exact paths. No foreign prune, retained Cargo
tree, stage/commit/push, progress/spec/GOAL/planning edit or paid/live call was made.
Real authoring HOME/auth, `.local/live.env` and foreign `.opencode/` are untouched;
the exhausted24G/15C/1MCP allowance stays unchanged. Final evidence checks append
quota, Python47/docs/progress/diff/advisory and explicit ownership release receipts.

The first final Python/docs/progress checks observed stale generated NOW during
concurrent owner planning updates (four of47 Python tests errored; docs exit1,
progress exit2). The subsequent read-only comparison found that the owner's view
already matched its journal; this coordinator made no reindex/progress/plan edit.
All47 and both structural checks then pass. The failed logs and comparison are
retained. This transient documentation state changed no qualified Rust content.

Final evidence validation reports739775 B of new retained evidence at validation
time (261 lossless logs); the completed log and this receipt remain below740 KiB,
within1 MiB. Every individual retained artifact and decompressed log is below16 MiB.
All four final mixed profiles have exact38193/55173 B checkpoint/wire peaks and
2/36798 graph rows/bytes. The validator also checks the twelve delivered-current-
task fixture paths below as absent, alongside the twenty whole-past/test paths above:

```text
t54-retry-l4e3xaij
t54-retry-utv1e0r1
t54-retry-c22x93xd
t54-retry-khrvzr7u
t54-retry-nlfgduzx
t54-retry-ns5zy0pj
t50-background-owned-jtz55sgl
t50-background-owned-wo4rxhif
t50-background-owned-x58hvzdn
t50-background-owned-2mup_n2d
t50-background-owned-m60t32m5
t50-background-owned-iw7jc0gk
```

**Release:** sole mutation, Cargo and owned offline-fixture coordination returns to
the parent. All acquired processes/listeners/HTTP handlers/PTY readers/watchdogs
are joined; no owned temporary directory or process remains. This closes only the
frozen whole-past renewal/recovery atomic, not whole R9/T45/READY or the separate
owner planning listed NOT_RUN above. Source and evidence remain uncommitted.

## Historical frozen design, RED experiments and implementation checkpoints

Final fixture semantic strengthening: prime an actual OLD_REQUIREMENT/old path and
choose an initial OLD_SUMMARY, then issue one explicit changed requirement/path/
result/next-move user directive before recompression. Subsequent prompts contain
only continuation labels. The next replacement must remove both old sentinels and
carry the changed facts through compact/restart/continue. This changes only the
owned synthetic fixture and strengthens continuity/intentional omission receipts;
normal Rust source, budgets and the final ELF pair remain unchanged. Rerun the
normal mixed/overflow/fault/cancel/crash cases after this fixture-only change.

`red-obsolete-self-summary` (normal debug ELF, exit1) now proves obsolete summary
and requirement sentinels survived recompression specifically in the previous
`compress` function-call arguments. The first-producer compatibility exception
was retaining completed projection-control-only response groups. Freeze the narrow
repair before source editing: such groups have no default first-selection pin;
explicit current tool/path/tag/live protections still take precedence, mixed groups
with a genuine native producer retain their initial compatibility, and pure opaque/
reasoning producers keep the existing first-pass behavior. Do not inspect summary
keywords or alter RAW, issuing snapshots or provider/tool schemas. Extend the
existing lifecycle owner with a genuine old closed compress call/result and assert
its obsolete authored summary is absent from first selected wire while RAW stays
exact. All final source gates/builds/native receipts must follow this repair.

The full follow-up run completed42 summaries1466/1/10: AUD20 expected only its two
new strategy removals, but now also genuinely forgets the prior closed compress
call/result (new_tools3, cumulative prunes5). Before adjusting the assertion, freeze
the exact reconciliation: keep every protected/read/error/reused-ID expectation;
add before/after assertions identifying the one removed projection-control pair and
assert its canonical RAW survives. This is a changed measured removal count, not
a relaxed resource baseline, inherited-pruning allowance or skipped failure.

### Frozen live-mark ownership repair (after scoped retirement)

`red-unused-fresh-alias` reproduces a further causal failure: scoped retirement
correctly preserves an unrelated legacy archive slot, but a later genuine fresh
producer with that call ID inherits the old hidden decision. The row key lacks
live-frame eligibility. Unreleased migration9 will add a nullable boolean `active`
to the existing occurrence projection table and an indexed live-frame selector.
A fixed versioned per-session `dcp.projection_owned.` preference distinguishes
legacy NULL eligibility before first normalization from owned frames afterward.
Replacement retires only indexed active rows and bounded actual before-keys,
then writes retained mapped decisions as active. Unrelated NULL archival rows
remain unchanged and ineligible after normalization. New ordinary decisions are
active. The existing context-version/Fork owners carry the flag and preference;
old snapshots restore NULL/absent mode. This is no epoch clock, per-archive vector,
new table/store/framework or archive scan. The measured RED requires this minimal
eligibility metadata; merely leaving stale keys or deleting the whole archive is
incorrect. Progress, RAW/HOT installation and eligibility change share COMMIT.

Late legacy-policy RED (`red-legacy-protection`, exit101): continuing explicit user
protection retained a legacy runtime footer only on its first standalone replacement;
the next replacement lost it because native metadata carried no fact for that footer.
Freeze the minimal repair before source editing: carry bounded literal derived legacy
protection separately from original-ID facts, re-evaluate current user/tag/path policy
at every renewal and compact, and discard it under default policy. No fabricated
original identity, ancestry dependency or RAW reload; use existing hot/selection JSON.

Late inactive-mark RED (`red-inactive-mark-retirement`, exit101): a scoped compact
tried to DELETE an unrelated inactive archive mark because replacement cleared the
entire session projection table. Freeze indexed retirement of only the admitted
before-window keys, followed by bounded surviving producer decisions. Keep the
unused archival rows out of runtime reads; no archive-wide cleanup/write scan,
new schema or growing durable key list. Success/progress remains one transaction.

Late physical-plan RED (`red-active-index-plan`) distinguishes JSON operators:
SQLite uses only the session prefix for `hot->>'$.active'=1`, while the existing
expression index uses both session and active for `json_extract(hot,'$.active')=1`.
Before the narrow repair, freeze exact indexed predicates for selected-byte and
causal metadata queries and split retirement's native/legacy OR into indexed UNION
candidates. This eliminates known-inactive descriptor scans; no schema/index,
payload guard, transaction semantics or product threshold changes are needed.

The first complete workspace execution (`final-workspace`, exit101) completed all
42 summaries:1462/5/10. Before adapting the affected assertions, freeze the exact
contract reconciliation: native endpoint coverage intentionally stores no enumerated
membership rows; crash recovery must still expose all32 complete standalone endpoint
blocks or none, never partial commit. A newly accepted current user is the unfinished
anchor, so all four preceding closed legacy rows are now advertised. Recompression
may forget old unprotected native groups and released verbatim protection: assert
their actual absence and measured removed content, rather than forever retention.
Continue testing currently explicit protection independently. The family-deletion
fixture needs named columns for additive schema9. No resource threshold, timeout,
failure/cancel atomicity assertion or foreign-reference guard is weakened.

`red-legacy-depth` proves the transition edge: referencing an already admitted
legacy graph at its old depth boundary added one synthetic candidate level and
refused renewal. Freeze the narrow repair: faithfully resolve bounded existing
references into ephemeral standalone preparation nodes before validating the new
batch. Original context/version objects remain unchanged; byte/cycle and actual
within-batch expansion guards remain. A renewal no longer ages an old valid graph.

The unchanged compaction/Fork gate caught SQLite JSON subtype preservation in new
version snapshots (`retired-prefix-owners`): json_set updates can make json_array
store the metadata as an object rather than quoted JSON text. Accept the same typed
object/array or serialized JSON representation during explicit Fork import, then
validate/rebase it normally. Do not rewrite immutable source version objects.

`native-mixed-develop-11` is the actual mixed-workflow retirement RED: compression
and recompression succeeded, but `/compact` left wholly covered blocks flagged
live with old selected payload. Freeze the minimal repair before editing: retire
fully covered known/legacy blocks and release their live selection in the existing
checkpoint/current-turn transaction, using indexed active/end metadata. Preserve
authored summary, RAW and immutable version objects; straddling blocks stay live.

The later normal-ELF shape receipt corrects the initial no-gain diagnosis below:
the first ordinary assistant output is already plain text, and `bash` requires
native argv (the early fixture used an invalid command shape). After correcting
the fixture, `native-mixed-develop-09` still refuses no-gain44562→44652: initial
anchors were captured before accepting the current user, falsely labeling the
previous completed assistant as unfinished. Before the next source edit, freeze
the minimal repair: regenerate initial anchor metadata with the accepted current
user as the sole unfinished tail, without adding that user to prior wire history.
The structural provider-message exception remains narrow; it was not this cause.

`native-mixed-develop-04` supplies the normal-ELF counterexample to the broad
first-producer exception: ordinary canonical provider `message` output entered
the supposedly protocol-only pack, making a real compression no-gain11855→11945.
Before repairing it, freeze the narrow structural distinction: native reasoning,
call/media/opaque groups retain initial compatibility, while a standalone ordinary
provider message is replaceable text. Whole groups that contain actual calls retain
their messages; there is no summary-text policy or default historical task pin.

`red-malformed-hot` (exit101) proves missing selection flags were silently excluded
from active roots, potentially recalling covered RAW. Before the corrective edit,
freeze a schema9 partial invalid-metadata index and constant indexed scope guard:
malformed/version/flag-invalid non-NULL metadata fails closed, without scanning or
loading the inactive archive. This adds an index to the already justified migration,
not a store or eager archive normalization. Legacy NULL metadata remains supported.

## Compatibility selection refinement — before the corrective edit

The lifecycle fixture exposed accidental first-pass selection of plain legacy
assistant text as a protocol producer (`lifecycle-owners-02`, no gain11758→17340).
Narrow the previously frozen first-producer exception structurally to groups that
actually contain native provider output. Plain message text is replaceable; real
call/media/reasoning producers retain the existing first-pass compatibility. The
fixture separates its short explicit tag from the unprotected large investigation.

`red-reused-mark-02` is a new causal counterexample: after omitting the old selected
producer before measuring identities, a surviving reused call lost its own purged
decision and inherited the old rank. The next minimal repair derives a bounded
ordered identity window from active HOT JSON/typed metadata in SQLite, transferring
only call IDs/original occurrences and owner IDs before selection, never arguments,
media, RAW segments or inactive archive payload. Indexed mark lookup and immutable
identity remapping then serve both the chosen summary input and installed tail.
No additional schema or lifetime map is needed.

The existing checkpoint selection JSON also needs exact currently protected message
facts when a message has no provider journal. The selected minimal representation
is a tagged `protected_message` value in the same bounded selection array, alongside
actual selected journals. Its original message ID rebases only on explicit Fork;
current user/tag/path/recent-turn policy re-evaluates it on every replacement. It
is not model-generated policy or a permanent suffix. No additional schema is added.

The next owner checkpoint selects compaction packs in SQLite before transfer:
unprotected old selection is omitted structurally, legacy authored references are
expanded once into an ephemeral standalone input, and explicit/live protections
remain selected or fail resource admission honestly. Current-turn installation
uses the existing checkpoint selection field for foreign-turn facts in the same
RAW/HOT transaction; it never nests a foreign journal in current working state.
Selected occurrence marks are remapped by immutable producer identity, not rank.
Progress receipts will distinguish measured selected-input/replacement bytes,
metadata-admitted prefix omission, no gain and irreducibility. Counting serialization
does not allocate an overflowing before-wire copy; no hidden summary loop is added.

Implementation checkpoint (not qualification): `compact-escape-02` passes the
three directed RED owners, including metadata-before-content host escape.
`selection-compile-02` passes the existing compaction filter; `renewal-regressions-03`
passes34 DCP-related owners including unchanged MCP12/TOOL16 rich projection,
Undo/Redo/Fork and previous inactive-archive tests. No final full workspace/build
or normal-ELF mixed workload has run. `compact-escape-01` hit the unchanged1798s
operational watchdog: an i64::MAX gap sentinel was treated as a real interval and
looped before dispatch. The owner branch was repaired; its lossless log remains.
Only the two exact joined test-owned directories `.tmpsdBd7b` and `.tmpV7H49j`
under the prescribed TMPDIR were removed (`compact-watchdog-owned-cleanup`).
No external provider/effect was dispatched by that hung test.

Remaining coupled work includes compaction policy selection before native pack
payload transfer, explicit selected past facts at the current-task transaction,
typed progress/no-gain/irreducible receipts, checkpoint/nudge/anchor reconciliation,
new renewal/Fork/protection/reused-occurrence tests, and complete normal-ELF/full
gates. This checkpoint is NOT whole-atomic PASS.

The unchanged MCP12 and TOOL16 projection tests exposed a real first-compression
compatibility contract: a newly selected producer's native call/media pair remains
usable after the first message collapse. Their assertions are retained. This is
not a model-text policy (words such as "Keep" are never parsed). The selected safe
boundary is structural: previously uncompressed original producers may enter the
first chosen pack; recompression consumes that pack and reselects only groups
protected by current tool/path/tag/live-control policy. Omitted old pack payloads
are genuinely gone. Native compact likewise replaces the chosen pack; it does not
permanently pin original media, protocol groups or terminal tasks. This preserves
initial rich projection while making renewal forgetful, without a new tool option,
schema, provider path or lifetime protection. The existing large-archive fixture's
seven-column positional INSERT also needs explicit column names for additive
schema9; its payload and assertions do not change.

## Frozen atomic — before RED or source edits

Source base: `44bb7d7d39990c949f91c46a477a92e951cecb69`, active T45/0005.
Inherited `.opencode/` is excluded. The delivered current-task RAW/HOT seam and
its page-integrity/explicit-family-deletion repairs remain prerequisites, not
work reopened by this atomic. Owner planning changes remain unmodified.

| Obligation | Required observable result | RED / nearest owner |
| --- | --- | --- |
| Standalone replacement | All wholly covered old blocks consumed, including omitted placeholders/content; chosen authored references resolve once, with no persistent ancestry | `dcp.rs` planning/expansion; `storage_dcp_view.rs` addressed snapshot |
| Real forgetting | Covered obsolete closed calls/results/reasoning/media and released terminal task/pack leave actual wire, not merely message estimates | `runtime/context.rs` wire projection and existing protected policy |
| Explicit protection | Re-evaluate current policy against bounded selected original facts/groups; do not blindly inherit old protected suffixes | current `ProtectedSpec`/tool/path/tag owners |
| Lifetime independence | Cross prior depth pattern and >4096 historical members without transferring covered IDs; indexed bounded active/addressed descriptors and payload | storage range coverage/planning/commit |
| Compact consolidation | Checkpoint + chosen working state + eligible work become one replacement and unchanged fresh tail; forgotten RAW is never recalled | `runtime_compaction.rs`, `storage_compaction.rs` |
| Reachable recovery | Actual `/compact` works after ordinary host/model input admission overflow; select revision/floor/closed boundary before payload | compaction/context bounds and actual PTY command |
| Outcomes/safety | Measured shrinking/progress, no-gain or irreducible failure is truthful; stale/cycle/foreign/cancel leave prior state; no hidden summary loop or new limits | existing admitted counted summary/retry/cancel/transaction owners |
| Durable history | RAW, original occurrence/receipt/provenance, explicit Undo/Redo/Fork and family deletion remain valid; no effects replay | delivered RAW/HOT owner and conversation version/fork tests |
| Equal-hot workload | Mixed compress/continue/recompress/compact/continue/compress/restart/compact/continue, no cold files/Git dependency, small/large inactive archives have equal bounded hot work | normal rebuilt ELF loopback fixture, physical POST/read/bytes/RAM/DB/WAL/I/O receipts |

### Selected safe hypothesis and storage necessity

The existing planner's `consumed` set is based on anchors/placeholders, not all
wholly covered blocks. `plan_active_compression` appends old protected suffixes
without a current-policy decision. The SQL addressed snapshot enumerates all
covered IDs (LIMIT4097), commit deletes/reinserts memberships, and active graph
loading follows authored placeholders after commit. Wire projection deliberately
retains covered canonical tool/reasoning input. Compaction obtains active content
before a boundary, creating a host-overflow circular refusal.

Use the existing SQLite compression/block/version owners. Prefer endpoint coverage
and standalone resolved working content; operation-specific payload/depth guards
remain, but elapsed successful history is not an input to them. New plans retain
only bounded active/addressed endpoints and selected facts, never a covered-ID
vector or all-session mark set. Durable legacy memberships remain historical and
are not recopied for each renewal.

A minimal versioned hot descriptor is necessary to distinguish legacy authored
graphs from new standalone content (including literal placeholder-like protected
text), active range coverage from inactive provenance, and currently selected
protected facts/protocol groups from old blindly inherited suffixes. Existing
block start/end IDs cannot express these distinctions. If an additive schema is
used, it will be one narrow compression metadata relation in the same DB, tracked
by existing conversation versions, with indexed active/endpoints and bounded
selected payload. It is not an archive service, general memory framework, queue,
new crate/dependency or raw-history rewrite. Schema shape is provisional until
the falsifiable storage tests establish the smallest adequate representation.

Legacy valid graphs are addressed/bounded transitions, not an eager whole-archive
normalization. Explicit authored references may be resolved during preparation;
only the chosen standalone content remains a live dependency after commit.
Current explicit protections preserve original selected facts/groups; terminal
runtime task/pack protection is not a permanent historical fixed lane.

### Frozen workload and acceptance measurements

1. Keep `CONTROL_OBJECTIVE`, changed requirement, selected path/result and next
   move. Omit sentinels in old summary, obsolete requirement/investigation, large
   closed tool/media/opaque group and terminal task/pack. Add finite bounded work
   between cycles; every deliberate omission must stay absent from subsequent
   actual wire and resident selected state through recompress/compact/restart.
2. Mixed sequence: compress → continue → recompress → compact → continue →
   compress → restart → compact → continue. No user cold notes or Git history.
   Explicit Undo/read/Fork are separate admitted historical selections.
3. Compare equal hot state over small/large inactive block/member/mark archives,
   including >4096 historical members and repeated former-depth transitions.
   Record actual loaded rows/bytes/live depth, hot serialization, peak/retained
   whole-process RAM, dispatch/socket counts, effects, DB/WAL occupancy and process
   I/O. Scope counters honestly; archive disk growth is allowed.
4. Actual `/compact` host overflow and model overflow, then same-session normal
   continuation; before/after commit faults, crash/restart, cancellation/no replay.
   Reuse nearest current-task RAW/HOT and T50 selection/retry/mutation/resource
   checks rather than a new exhaustive matrix.

### Gates and ownership

Serial offline Cargo: jobs3, threads1, locked dependencies, TMPDIR
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`, separate
precompile and full workspace tests, unchanged1798s operational watchdog and
Bash1800000. Required fmt, strict workspace all-target Clippy, full workspace
tests, normal debug/release builds/help, Python47/docs/progress/diff/advisory.
Actual native receipts follow the LAST builds with no intervening Cargo.
New retained evidence/logs <=1 MiB, individual logs <=16 MiB; failed logs retained
losslessly compressed. Join exact owned processes/HTTP handlers/PTY readers before
exact TempDir cleanup; no retained Cargo trees or foreign pruning.

Defaults40/55/false, allowSubAgents/full DCP12 controls, profiles/parallel jobs,
T53/T56 and paused T44 are separate. No paid/live/.local/env/real HOME/auth/campaign,
progress/spec/GOAL/planning changes or stage/commit/push. This atomic cannot mark
whole R9/T45/READY.

## Execution

RED before production edits: `red-replacement-02` reproduces omitted-block
`ExistingOverlap` and unreachable manual host-overflow compaction. The first
covered-members fixture lacked its DCP schema; corrected fixture rerun
`red-covered-members` reproduces the actual4096 addressed-row refusal. The
earlier fixture/import failures remain lossless, not product counterexamples.

Selected minimal persisted form, frozen before its production edit: schema9 adds
one nullable versioned `hot` metadata column to existing compression blocks and
one nullable selected-facts column to the existing session checkpoint. Indexed
active/legacy block metadata separates inactive history from roots; no new table
or store is needed. Existing conversation row-object payloads gain trailing nullable
fields, with old seven/six-field snapshots still valid. Replace tracking triggers
without bootstrap/normalizing the old archive. Fork rebases selected original
references and skips reinterpretation of standalone literal placeholders. This
is a narrower representation than the provisional metadata relation above.

Production implementation and final qualification pending; the table remains
the required finish line, not PASS evidence.

First implementation checkpoint: `standalone-owners-01` exits0 (two new owner
scenarios). Wholly covered omitted blocks are consumed; explicit references are
resolved into standalone payload; addressed4100-member legacy coverage transfers
only bounded endpoint rows. Schema9 nullable columns/indexes and tracked trailing
fields are implemented without archival bootstrap. Full-wire/protection/mark
retirement, compact overflow recovery, exact resource qualification, version/Fork
integration and full final gates remain pending. No overall atomic PASS yet.
