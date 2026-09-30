# T50 R5 / TOOL16 — frozen read obligations

## Result

**Scheduling dependency — required shared nested-instruction prerequisite is absent.**
Coordinator confirmed this is unfinished approved T45/R10 implementation, not an
external blocker. The existing user mandate authorizes continuing that owner slice;
T50 is temporarily suspended through the single progress owner, then will resume.
No production implementation or R5 PASS is claimed. Frozen before RED against source base
`1afc3d46bc4fe8128840855b6d89942a1ab38f19`; tracked worktree clean at
admission, inherited untracked `.opencode/` untouched. T50 active 0005.
R5 is an atomic behavioral slice; none of the rows below is a PASS claim.

| Obligation | Required observable proof |
| --- | --- |
| Text and directory pages | Actual model calls to normal debug/release ELF: default 2000, 1-based offset/limit and line references; sorted bounded directory entries and coherent next cursor. Truthful root/child schemas and effective read ceilings. Existing String clients remain compatible. |
| Real validated image output | Actual selected Responses capability metadata, validated image bytes/MIME, ordered inline image content alongside readable call-output metadata. Original call ID and source facts durably committed before continuation; restart/fork/DCP/compaction preserve the admitted result after source-file mutation without reread/reexecution. PDF, unsupported model/file, malformed image and exhausted budgets are actionable non-success. No MCP provenance invented for local files. |
| Path, policy, resource and cancellation boundaries | Existing central Deny/Ask/Always and General/Explore ceilings before bytes I/O; own-data-root, no-follow ancestors and descriptor-held regular files; FIFO/swap/symlink refusals. Bounded chunk reads, sorted directory scan, model/retained-facts/byte budgets and invocation cancellation through the existing owned blocking-dispatch seam; join before cleanup/finish. No implicit URI/network or model fallback. |
| Shared nested instructions | Successful text/directory reads invoke T45/R10's admitted nested AGENTS lifecycle with canonical provenance and unchanged-content dedup. Root baseline remains owned by the shared runtime/config assembler; no second loader or implementation of all T45. |

Pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`:
`opencode/packages/core/src/tool/plugin/read.ts:15–27,82–105,169–204`
and `tool/read-filesystem.ts`. Native PDF exclusion is explicit.

## Checks

Current normal-ELF RED completed; minimal fix/GREEN remains blocked. Pending: affected
Files/tools/runtime/provider scenarios; fmt all check, strict locked workspace
all-target Clippy, locked workspace tests, separate normal debug/release builds
and help, then direct ELF read and directed current R4/search/shell/recovery/MCP/
retry risks. Cargo serial, jobs=3, test threads=1, offline, timeout 900000ms,
owned disk TMPDIR. Reports/logs remain small and synthetic. No live campaign.

Preflight: uid=1003; MemAvailable=6548 MiB; available filesystem space=193 GiB.

## Risks

Source verification found only baseline instruction assembly:
`composition.rs:630–649`, `defs.rs:1393–1467`,
`runtime.rs:1494–1535`, `runtime/turn.rs:17–29`.
`composition` admits global/root instruction bytes and calls
`defs::load_instruction_texts`; `Runtime::publish_workspace` installs them as
fixed input between turns. `lane_fixed_input` renders that baseline. None of
these implements nested read admission, a per-session admitted source set,
chronological durable nested updates or unchanged-content dedup. The actual
read dispatch at `runtime/turn.rs:2885–2887` invokes `execute_batch`; the current
`tools.rs:838–870` read executor does not call any instruction owner.

Targeted searches across all `crates/**/*.rs` for nested AGENTS, instruction
updates/state and AGENTS.md confirmed those baseline/test references only.
T45/R10's approved contract (`docs/goals/2026-09-21-config-compat-and-subagents.md:116–122`)
describes this lifecycle as a required outcome; a contract is not an implemented
hook. No existing admitted nested lifecycle is available to reuse at this HEAD.
This prerequisite is not waived by read/media tests, and a separate T50
instruction loader is forbidden by this delegation. Implementing a minimal
shared T45/R10 prerequisite needs an explicit amended execution envelope or a
prior owner slice; merely rediscovering AGENTS in `tool_read` is insufficient.

Current read/media facts remain unqualified: `Files::read` reopens by path and
materializes the file before bounding text; directories are refused and binary
NUL is rejected. Images need a vetted decoder, typed local-source facts and
shared provider/log/projection budget lowering, not fabricated MCP results.
No Cargo/dependency/lock/configuration changes were attempted after discovering
the instruction blocker.

T44 visual presentation remains PAUSED. T27's real ledger remains exhausted
(24 generation requests, 15 catalog requests, 1 MCP search, 301273 bytes);
no reset, older allowance, new live campaign or user-environment qualification.
R6–R8 remain separate, and no T50 DONE/product readiness claim is made.

## Next

Parent: qualify the minimal shared T45/R10 nested-admission seam (successful-read
hook plus durable source/provenance/dedup owned by the existing runtime/config
assembler), then resume the frozen atomic R5 slice and extend the same fixture
for directory/media/policy/cancellation/replay. Do not mark T50/R5 complete or
weaken the frozen acceptance. Parent owns review/stage/commit/progress; this
coordinator does none of those actions.

## Current direct-ELF RED evidence

Commands used the owned disk TMPDIR and `PYTHONDONTWRITEBYTECODE=1`:

```text
python3 evidence/T50/native_read.py target/debug/oc      # exit 1
python3 evidence/T50/native_read.py target/release/oc    # exit 1
```

`native_read.py` calls the ELF directly, with a synthetic loopback Responses peer
and isolated synthetic HOME/config/store/project. Each guard performs two actual
main model requests and confirms the original `read-call` output matches the
already-committed completed ToolOp before replying to the continuation request.
It excludes bounded auxiliary title traffic from main-call counts. These are
model-driven read results, not schema-only probes or empty-file shortcuts.

| ELF | text_default RED | nested_lifecycle RED | Current SHA256 |
| --- | --- | --- | --- |
| normal debug | 50 fixture lines; required `2000: fixture_2000` absent | successful two-line read; root baseline present; nested instruction count 0 | `3fbf74fbdb651252d9b3b322f57f4c02c47fec0871dfca56700f9d6ba785abaa` |
| normal release | 50 fixture lines; required `2000: fixture_2000` absent | successful two-line read; root baseline present; nested instruction count 0 | `b0081f0e8b5216bd5917705c42c6f562a2b57e8892dab5edd1f64a1019ab6ba2` |

Final counts: **2 RED cases / 4 main requests per ELF**; 4 RED cases / 8 main
requests total. Each application's exit was 0; the guard command exits 1 on the
missing frozen behavior. The first helper attempt rejected a legitimate bounded
auxiliary request; that fixture issue was corrected before these source REDs.
HTTP workers and the owned application process were joined before every TempDir
cleanup, including failed assertions. No live calls, shell effects or outstanding
fixture resources remain.

Nearest source corroboration is `tools.rs:856` (`unwrap_or(50)`),
`runtime.rs:439` (advertises 50), and the baseline-only owner chain above.
No Rust was edited, no Cargo command ran, and full workspace log/build/green
qualification is **NOT_RUN_BLOCKED**, not inherited from historical R4 evidence.
Only the new report and reusable direct-ELF guard are dirty; no CODE_MAP production
path changes are needed. All mutation/Cargo/native-fixture ownership is released
to the parent at handoff.

Final structural checks: `python3 scripts/check_docs.py` exit 0;
`git diff --check` exit 0; HEAD unchanged. The owned TMPDIR has no remaining
`t50-read-*` fixtures. No stage, commit, push, progress, GOAL/spec or inherited
`.opencode/` operation was performed.
