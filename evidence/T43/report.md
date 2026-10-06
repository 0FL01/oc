# T43 report — DONE: config/foreground prerequisites

## Current scoped closure — 2026-10-07

Scope is the explicit `planning/tasks.json` T43 owner split and the shared
spec's 2026-09-22 Material Decision: **R1/R2, foreground slices1–4 and resource
permission/config prerequisites**. Remaining R3/background/commands/DCP and later
R6–R10 belong to T45, not a prerequisite for finishing T43. This closes the
already implemented prerequisite; it does not waive or declare full R3/goal READY.
The shared goal remains blocked on the original T45 live-child allowance facet.

The old T43 latest0003 scheduling blocker said to wait for the transferred
background remainder. That is superseded by the approved explicit owner split;
the foreground code was already landed in `7895f43`. Current T45 additions and
green unchanged-owner gates preserve those prerequisites. No new production
change, requirement, dependency edit, API expansion or speculative audit is needed.

| Owned outcome | Closure evidence |
| --- | --- |
| R1 seven warning shapes | Config compatibility commit `335482bdd` and direct real owner-start receipt preserved below. Current full workspace passes `commented_frontmatter_and_glob_permissions_follow_upstream`, `large_agent_and_command_bodies_load_within_the_global_budget`, `skill_frontmatter_comments_and_optional_metadata` and related config/definition tests. No re-reading owner/authoring config or new real request. |
| R2 artificial size caps | Same historical oversized-agent/command/skill/start proof plus current tests. The explicit single4MiB total guard and1MiB serving bounds remain; no cap/validation weakening. |
| Foreground prerequisites | `7895f43` and current `subagent` target39 PASS: actual fresh child creation/prompt/result/history, depth/config/mode/model/variant validation before effects, explicit-model override, caller-scoped continuation, permission narrowing and mid-stream cancellation/join. Later actual native foreground/child-control receipts under T45 reuse this implementation without changing T43's scope. |
| AUD42 resource authority | `backend-permissions.md` contains source-derived ordered rules/resource and selected/restored-primary/nested-child regressions. Current `permissions` target8 PASS and full runtime/subagent tests prove wildcard denial, every patch/rename resource, unmatched Ask, literal-star safety, central/profile/child non-widening and hidden/denied catalog filtering. Genuine approval does not bypass structural ceilings. |
| A02/A03/A05 and R4/R5 delivery/gates | Current final production source through `25df7dc6e` passes the full locked workspace1713/0/11, strict all-target Clippy/fmt and normal debug/release builds/help. Source slices already checked/pushed; this report/progress closure is a separate reviewed documentation commit. |

### Current checks and source association

The current production source is checked/pushed `25df7dc6e`; subsequent
`4d3513e4a` is T45 factual documentation/progress only. T43 closeout changes no
Rust, fixture, dependency, security or test baseline. Reuse the successful final
full-source chain rather than rerun unchanged broad gates after docs:

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --no-fail-fast
cargo build --locked --release
```

Actual result **1713 passed / 0 failed / 11 unchanged opt-in ignored**, independently
summed46 workspace records. Log:
`/home/opencode/.cache/opencode-tmp/opencode/t45-child-diagnostic-final-barrier.log`,
final `T45_CHILD_DIAGNOSTIC_FINAL_BARRIER_GATES_PASS`. Existing config fixtures,
actual dispatch resource regression and inherited child-catalog regression are
explicitly present as PASS, not inferred from total counts alone.

Additional scoped check during T43 resume:
`cargo test --locked -p oc-adapters --test permissions` — **8/0/0**;
normal `target/debug/oc --help` / `target/release/oc --help` — PASS. Serial normal
stacks, approved TMPDIR, jobs3/testthreads2; no raised watchdog/threshold/ignore.
Current normal debug/release controls4/recovery20/chronology6 cases each and
nested-instructions15requests/13facts each are qualified in the T45 primary report.

### Preserved boundaries and completion

Historical report below records the actual 2026-09-21 owner config/start and
earlier failure observations. It is not a claim of a new current real API call,
current fatal optional-MCP behavior or remaining unsupported subagents: later
T46/T51 fault isolation and T45 child/defaults supersede those historical states.
No old checkpoint or real ledger is rewritten. T53 remains13/24; original T27
generation24/24/unknown17 spent remains unchanged. T57 AUTH06 stays owner-deferred
and blocked; T44 paired visuals stay PAUSED. User `.opencode/` is unread/unstaged.

All **T43-owned** criteria are verified; finish its independent prerequisite
scope only. No full R3/live-child/OAuth/visual/product READY claim. Next: read
actual journal ready/dependency state; do not invent a new task or auto-resume a
paused/externally blocked lane.

## Historical R1/R2 report — 2026-09-21 (preserved)

Slice: `docs/goals/2026-09-21-config-compat-and-subagents.md` R1, R2 (R3–R5 open).

## Result

Owner config (`~/.config/opencode`) now loads without a single config diagnostic.
Before this slice bare `oc` printed 9 warnings; the 7 config warnings are gone and the
remaining two are accounted for: `dcp experimental.allowSubAgents` (removed by R3 when
subagents land) and the `crw` MCP attach failure (external: Cloudflare 403 Error 1010).

Fixed upstream-parity gaps (upstream tag `v2.0.12`, tree SHA
`2670273ff17da96f85c5826ced57aa1b368754fa`; upstream has no size limits at all):

- YAML comments: `#model:` lines are comments, trailing ` # …` is stripped, `provider/model#variant`
  stays intact; duplicate real keys still fail.
- `permission`: scalar for any identifier key, glob→action maps for
  `external_directory`/`edit`/`bash`/`webfetch`, custom action names such as
  `tavily-local_*` accepted (upstream `Record(String, Rule)`), legacy `write|edit → apply_patch`.
- Skills: `name?`/`description?`/`metadata?` optional, unknown fields (`license`,
  `compatibility`) ignored, no frontmatter required, flat `<skills>/<id>.md` loads, a directory
  without `SKILL.md` is skipped silently, skills without a description are excluded from the
  model auto-invoke catalog (upstream parity).
- Agents: `mode: primary|subagent|all` accepted (execution lands with R3).
- Commands: `agent`, `model`, `subagent`, `subtask` parsed and stored for markdown and inline
  JSON forms; no more `command execution field unsupported`.
- Limits removed: per-file/body caps for agent/command/skill, frontmatter line/byte caps,
  description/name caps. Remaining bounds: `MAX_TOTAL_BYTES = 4 MiB` (single global guard,
  documented deviation), `COMMAND_BYTES_CAP = 1 MiB`, `SKILL_BODY_CAP = 1 MiB` (serving bounds;
  previously 4 KiB/16 KiB and the 4 KiB one made the owner's 41 KiB `mge.md` load but fail on
  invocation), `MAX_DEF_ID_LEN`/`MAX_DEF_ROOTS`/`MAX_DEFS_PER_KIND`/`MAX_INSTRUCTIONS_*` unchanged.

## Checks

- `cargo fmt --all` clean; `cargo clippy --locked --workspace --all-targets -- -D warnings` exit 0.
- `cargo test --locked --workspace --no-fail-fast`: 337 passed / 0 failed / 4 ignored (ws5 log);
  a later full run showed 336/1 with `aud30_pty_paste_resize_error_recovery` failing in
  `wait_exit` (PTY timeout under parallel load). Rerun in isolation: 3 passed / 0 failed, and it
  passed in the earlier run too — pre-existing flakiness, untouched code path.
- Owner config, temp HOME with `crw` disabled:
  `session s-1790026313062208356` + `pong` on `ludka2` / `ocg/muse-spark-1.3-contributor`,
  only the DCP warning on stderr.
- Owner config, real HOME: only the DCP warning + `error: application: mcp attach failed for crw`
  (external Cloudflare block; attach failure is fatal by contract).
- No orphan `oc` processes after the runs (`pgrep` clean); one leftover process from an earlier
  timed-out run was killed.
- `cargo build --locked --release` succeeds.

## Risks

- R3 open: `mode: subagent|all` definitions load but are not runnable yet, and the DCP
  `allowSubAgents` warning remains until the subagent system lands.
- `crw` remains unusable from this client (Cloudflare 1010); owner config needs it disabled or
  repointed for any `oc` start to succeed.
- PTY test flakiness under heavy parallel load (pre-existing).

## Next

R3 slices 1–8 from `evidence/subagents/upstream-v2.0.12-plan.md`, committing and pushing each;
then the owner config should show zero warnings with `allowSubAgents` honored.
