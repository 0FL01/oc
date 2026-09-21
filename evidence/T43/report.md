# T43 report — R1 config compatibility parity + R2 limits removal

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
