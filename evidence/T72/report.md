# T72 — worktree-local debug build guidance

Status: PASS (documentation-only).
Base: 5c40870aa3d1fdcd97f73fab53c8998cb266cdc0.

## Result

Replaced only AGENTS.md's live-dev section with four Debug-сборка rules. The soft
debug build stays worktree-local and reports the actual artifact path, not personal
launcher readiness. A binary already built by checks is sufficient; required gates
are unchanged. Removed personal shell-profile and fixed home-path dependencies.

## Checks

- Targeted content/scope assertions PASS: four rules, obsolete dependencies absent,
  build/reporting and all safety clauses retained; the rest of AGENTS.md is identical.
- `python3 scripts/check_docs.py`: PASS,72 tasks/202 acceptance specifications.
- `git diff --check`: PASS; reviewed doc-only diff.
- Rust tests/build omitted: no runtime or build configuration changes.

## Risks

Target/cache protections, no automatic interactive launch or owner kill/restart/lock
removal, shared config/data/startup-env/leases and migration reporting remain intact.
No historical evidence/checkpoint leaves rewritten. Other changes are required task
registration and generated progress/closeout metadata.

## Next

None. T72 is complete; no runtime follow-up opened.
