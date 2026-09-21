## Result

T36 implementation verified on base 1aac158, awaiting implementation commit and
task closeout. Actual `oc` now exposes
glob/grep/compress, executes all through permission + durable intent, sends DCP
anchors/nudges, commits multi-range blocks/tool projection/wire outcome/nudge
reset atomically, and continues from the smaller projection after restart.
Raw history remains unchanged. No live calls, credentials, dependencies or
generic plugin runtime. This is not overall goal completion.

## Checks

Initial actual-binary RED is in evidence/T36/regression.md. Final commands and
bounded results are in evidence/T36/checks.md. Targeted adapter unit 124, runtime
26, dcp_atomic 4, actual-binary DCP 2 and soak 4 all pass. Full locked workspace,
all-target clippy -D warnings, fmt check, locked build/help, docs/progress checks
and diff check all exit 0. Exactly three external-only harnesses remain NOT RUN.

## Risks

T36 does not close later T39 UI controls or T40 full-archive/lifetime bounds.
`showCompression`, notification channel and commands display are parsed/pinned
for T39; model DCP path is qualified here. Exact semantic summary quality cannot
be proven offline, but structural facts/tool graph/protections/no-replay are.
Live T27 remains product-blocked until T42.

## Next

Commit the verified implementation/docs/evidence and record its hash in the T36
report; finish T36 through progress.py and commit generated closeout. Then
immediately start T37, read its repair contract, and reproduce the MCP lifetime/
auth findings offline. Audit fragments must not be merged again. Git status is
authoritative; no push or live probe has occurred.
