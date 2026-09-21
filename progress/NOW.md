# NOW — актуальный handoff

State updated: 2026-09-21T15:37:56+00:00
Active: T37

Сверить Git status/diff до выполнения команд.
Task: T37 — MCP config, registry и владение ресурсами
Spec: audit/repairs/T37.md
Evidence target: evidence/T37/report.md

Закрыть F10, F11 по audit/repairs/T37.md. Проверять actual application path; checkpoint после каждого среза. Scope не расширять.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

T37 finished on base 839279c; report maps F10/F11 and AUD22–AUD24 to executed
regressions. Actual `oc` accepts unmodified user Authorization config, rejects
conflicting duplicates before network, reuses one MCP child/handshake/catalog
per config generation, closes it on reload/disable/shutdown with confirmed reap,
routes collision-renamed names by exact retained identity, rejects partial
oversized catalogs, distinguishes isError/unsupported modality/transport, and
uses the server `query`/`response_length` schema. Stdio children get trusted cwd,
minimal non-credential env and their own process group; disabled entries remain
zero-spawn. Cleanup failures now propagate to a non-zero exit. No live calls,
credentials, dependency or package changes.

## Checks

Initial RED: evidence/T37/regression.md. Final targeted and workspace commands
with exact counts: evidence/T37/checks.md. Runtime 31, remote MCP 20 + 1 ignored
live, stdio MCP 10 + 1 ignored real, actual MCP 7, core 17, adapter unit 124,
full workspace suites all exit 0. Workspace all-target clippy -D warnings, fmt
check, locked build, `oc --help`, progress/docs checks and `git diff --check`
pass. Three pre-existing external harnesses remain NOT RUN, not PASS.

## Risks

Overall goal is NOT complete and no READY claim is made. Raw HTTP/stdin frame
preallocation and process-wide output/queue lifetime stay with T40. User-facing
reload/location controls are T39. Live OpenProxy MCP and provider qualification
remain T27 after T42. T38–T42 plus FINAL T30 are still ahead.

## Next

Commit the verified T37 hardening, record the implementation hash in the report,
finish T37 through the existing progress engine and commit the generated
closeout. Then immediately start T38: read audit/repairs/T38.md, reproduce the
remaining shell/webfetch findings offline with temporary fixtures, and continue
the same regression-first cycle. Audit fragments must not be merged again.


Ready (до 5): нет
Blocked: T27

Done в журнале не означает READY всего продукта; см. GOAL.md.
