# T50 R6 / TOOL17 — frozen atomic slice

## Result

### Coordinator restoration and final independent qualification

**R6/TOOL17 PASS; artifacts restored and remaining directed checks completed.**
The prior artifact-loss handoff below is historical, not a current blocker.
Coordinator verified unchanged Git HEAD/diff, absence of Cargo/rustc work and
adequate owned storage, then rebuilt normally without reset, clean, deletion,
dependency upgrades or source changes. The cause of the earlier disappearance
is not established; no external writer or deletion command is asserted.

Rebuilt `cargo build --locked`, `cargo build --release --locked` and both helps
exited0. Independent `cargo test -p oc-adapters --lib --locked webfetch`:
**16 passed**, including total DNS/body/conversion budgets, joined cancellation,
SSRF, redirect/auth and parser/format tests. Fresh workspace fmt and strict
all-target locked Clippy exited0; a final normal debug build restored the exact
pre-loss hashes. Full current-source qualification remains the unchanged
**1386/0/10** workspace run recorded below; no Rust changed after that gate.

After the last Cargo build, coordinator directly ran both normal ELFs:

| Direct checker | Result per ELF |
| --- | --- |
| `native_webfetch.py` | 27 PASS;79 physical POSTs (52 main/27 title),22 GETs,26 terminal tool rows |
| `native_read.py` | 20 PASS;44 main +1 compaction request |
| `native_question.py` | 14 PASS;30 main/child requests; actual custom-editor Ctrl+C recovery |
| `../T45/native_instructions.py --ask-only` | 6 PASS;12 requests; source Ask zero opens/body, permanent Allow admitted |

All owned fixture groups and HTTP workers joined before TempDir cleanup.
Binary hashes matched before/after every campaign and the entire chain:

* debug `a1f44ad41d94f36dd70a14b87859d4df02c4ff151d34a3af695314eee110314b`
* release `083327efdb4fb3d57ad34f88be81fca09772006b5ad2773652670fec3e5dfffb`

Source association: `40a201e61ed7c81fa0ed52b82392d9c2d9ac8643` plus reviewed
R6 dirty source, not a clean-base binary claim. Python47, docs/progress and diff
checks also exited0. Restoration and independent checks were run serially with
the recorded commands; no standalone raw-log filepath is claimed. Read-only review found
no concrete new contract/bounds/security violation. Cooperative parser/libc/kernel
limitations below remain explicit. No live, paid, user-config or campaign action.
R7/R8 and whole T50 remain open; T44 PAUSED and T27 exhausted allowance unchanged.

### Current source qualification / artifact-loss handoff

R6 implementation and captured TOOL17 behavior **PASS** on base
`40a201e61ed7c81fa0ed52b82392d9c2d9ac8643` + the dirty owner files listed below.
Current artifact availability **FAIL / BLOCKED_ARTIFACT_LOSS**: after debug's
14-case question regression completed and verified its after-hash, the complete
`target/` directory disappeared. The next release-question command failed before
any fixture effect (`FileNotFoundError`); `ls target/debug/oc target/release/oc`
exit2 confirms both absent. HEAD and tracked diff are unchanged. This owner ran
no clean/remove/target deletion command, and did not rebuild after this unknown
external event. No current-ELF availability or whole-T50 READY claim is made.

| Frozen obligation | Observed result |
| --- | --- |
| GET/all formats/defaults/finite seconds/strict options/schema | PASS: owner RED→GREEN and actual debug/release next-request capture; default30/markdown, explicit0.5 seconds, unknown/null/auth/121-second inputs fail without GET |
| Unicode/structure/MIME/status/original/final URL | PASS: both normal ELFs, headings/lists/fenced+inline code/links/tables; raw HTML preserved, plain/JSON never inferred to be HTML, 404 and redirects reported truthfully |
| Total deadline/cancel/join | PASS: held resolver within40ms budget, held-body timeout/cancel, current-thread heartbeat and active conversion cancellation with **one observed worker join, active0 before terminal result**; native held-body0.15 seconds fails, SIGINT produces cancelled row/exit130 with no continuation |
| SSRF/credentials/caps/permissions | PASS: existing TOOL07/08/AUD25/26 plus approval lifecycle in current workspace; binary default-loopback/Deny/Ask/static-private/scheme/password/auth and final-hop guards count zero forbidden GET; redirect loop exactly6 requests, oversize body/output remains capped and flagged |
| Legacy API/regressions | PASS: public `fetch`, `FetchOptions`, `FetchResult`, `extract_text`, `html_to_text` compatibility and legacy first-hop-only bearer tests; R5/T45/MCPmedia/question/search/T54/T55/A10 suites retained and current workspace green |
| Post-last-build directed regressions | Read20 cases **PASS each ELF**, question14 **PASS debug**; release question **BLOCKED** and additional direct T45 Ask-only **NOT_RUN due artifact loss**. T45 owner/runtime regressions already PASS in full workspace |
| Current normal deliverables | FAIL: binaries missing after successful build/help/fetch/Read/debug-question proof; restore only after parent reconciles external ownership/event |

Normal hashes actually measured before/after both TOOL17 proof runs, associated
with the current BASE+DIRTY source (not a claim files still exist):

* debug `a1f44ad41d94f36dd70a14b87859d4df02c4ff151d34a3af695314eee110314b`
* release `083327efdb4fb3d57ad34f88be81fca09772006b5ad2773652670fec3e5dfffb`

Both final fetch logs: **27 cases / 79 physical provider requests** each
(52 main +27 real auxiliary title requests), **22 GETs /26 tool rows** each
(10 completed,15 failed,1 cancelled; Ask creates0 rows). All owned native PGIDs
and HTTP threads joined before automatic TempDir cleanup. Fake-only; no live,
paid, browser, env/config/HOME inspection or historical artifact deletion.

Base `40a201e61ed7c81fa0ed52b82392d9c2d9ac8643`; tracked clean at entry,
inherited untracked `.opencode/` excluded. Sole mutation/Cargo/fake-fixture owner.
Donor `2670273ff17da96f85c5826ced57aa1b368754fa`,
`packages/core/src/tool/{plugin/webfetch,html-markdown}.ts`.
Qualification pending; this table is frozen before RED.

| Required observable contract | Falsifier / evidence |
| --- | --- |
| GET; text/markdown/html; default markdown; finite seconds >0, <=120, default30; strict known fields | Owner argument/schema tests; fake Responses advertises schema and captures next-request result |
| HTML readable Unicode, headings/lists/code/links/tables; original/final URL/status/MIME/requested format | Parser-backed renderer tests and direct debug/release ELF fake HTTP redirects/all formats/plain/JSON/status |
| One deadline includes DNS, each hop, body, conversion; cancellation joins owned conversion | Held DNS/body, real async heartbeat/cancel/join tests; direct fake held-body timeout/cancel where available |
| Actual-dial SSRF every hop, no credentials/proxy/cookies/auth or unsafe error URLs; caps preserved | Mandatory TOOL07/08/AUD25/26 plus direct invalid/deny/auth/private/scheme/redirect/oversize effects |
| Permission Deny/Ask/source resource and typed terminal outcomes; public client compatibility | Existing approval lifecycle and fetch tests; native durable tool row/call-result capture |
| Existing R5/T45/MCP/question/search/T54/T55 regressions | Targeted owner tests plus fresh mandatory workspace checks; directed post-build normal ELF regressions |

## Checks

Current complete workspace log (supersedes the initial pre-seam workspace log):
`/home/opencode/ai/oc/evidence/T50/webfetch-final-workspace20261001.log`.
Exact command `cargo test --workspace --locked --no-fail-fast`, **exit0**,
**1386 passed /0 failed /10 unchanged opt-in ignores /42 summaries**,
829.64 seconds, full raw output retained126874 bytes, not truncated.
All Cargo commands used jobs3/testthreads1/offline=true/the prescribed TMPDIR,
one Cargo at a time; tool call timeout900000ms. Separate no-run compilation passed.

| Command / final log under `evidence/T50/` | Exit / counts |
| --- | --- |
| `cargo test -p oc-adapters --lib --locked webfetch`; `webfetch-targeted20261001.log` | 0;16 passed (6 new,10 existing); same tests passed in current full workspace |
| `cargo fmt --all -- --check`; `webfetch-final-fmt20261001.log` | 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings`; `webfetch-final-clippy20261001.log` | 0 |
| `cargo test --workspace --locked --no-run`; `webfetch-final-compile20261001.log` | 0 |
| `cargo build --locked`; `webfetch-final-build-debug20261001.log` | 0 |
| `cargo build --release --locked`; `webfetch-final-build-release20261001.log` | 0 |
| `target/{debug,release}/oc --help`; `webfetch-final-help-{debug,release}20261001.log` | 0 each |
| `python3 -B evidence/T50/native_webfetch.py target/{debug,release}/oc`; `webfetch-final-native-{debug,release}20261001.log` | 0 each;27 cases each; exact request/effect totals above |
| `python3 -B evidence/T50/native_read.py target/{debug,release}/oc`; `webfetch-reg-read-{debug,release}20261001.log` | 0 each;20 cases,44 main+1 compaction requests each (legacy fixture does not report auxiliary title count) |
| `python3 -B evidence/T50/native_question.py target/debug/oc`; `webfetch-reg-question-debug20261001.log` | 0;14 cases,29 reported main requests, matching before/after hash |
| Same question command for release; `webfetch-reg-question-release20261001.log` | 1;missing ELF before fixture startup |
| `ls target/debug/oc target/release/oc`; `webfetch-artifact-loss20261001.log` | 2;both absent |
| `python3 -B -m unittest discover -s scripts -p 'test_*.py'`; `webfetch-python20261001.log` | 0;Python47 |
| `python3 -B scripts/check_docs.py`; `webfetch-docs-serial20261001.log` | 0;first parallel attempt collided with progress check lock, retained diagnostic |
| `python3 -B scripts/progress.py check`; `webfetch-progress20261001.log` | 0;read-only structure |
| `python3 -B scripts/code_size.py --base 40a201e61ed7c81fa0ed52b82392d9c2d9ac8643 --changed`; `webfetch-final-code-size20261001.log` | 0;all changed owner files below5k |
| `git diff --check`, reviewed tracked/untracked owner code | 0;no delivery/staging/progress/spec/GOAL/ACCEPT mutation |

Preflight PASS: UID1003; MemAvailable7369498624 bytes; disk186222837760 bytes.
One Cargo at a time, jobs3, testthreads1, offline by default, owned disk TMPDIR,
900000ms command limit. New reports/logs budget <=1MiB; no historical cleanup.
Required: nearest RED/GREEN, fmt, strict all-target clippy, workspace locked
no-fail-fast tests, normal debug/release locked build/help, Python47, readonly
docs/progress, diff/code-size, then normal-ELF proof with hashes before/after.

## Risks

The retained source gates and pre-loss ELF proofs are factual. Missing current
artifacts and incomplete release-question/additional direct-AGENTS proof remain
unresolved. Parent must reconcile why `target/` disappeared and confirm sole
ownership before restoring normal builds and repeating post-build proofs.

Conversion checks are cooperative: one HTML tokenizer instruction, bounded
final code-fence scan/copy or kernel/libc DNS operation is not preempted. Logical
DNS deadline/cancel drops the async lookup and prevents a later HTTP dispatch;
it cannot forcibly cancel libc's underlying getaddrinfo worker. Conversion worker
joins before every normally returned terminal result, including cancel/timeout;
an abruptly dropped/terminated runtime is not claimed to complete an async join.
Markdown structure is useful, not byte-identical donor whitespace/irregular-table
layout; excessive nesting fails explicitly. No new managed output store.

Existing dependency graph has no identified HTML parser/converter. A justified
exact parser pin may require dependency-only fetch; no existing dependency
upgrade. Renderer must be streaming/bounded and cooperatively check cancel and
deadline, run off async executor, and join before terminal result. These checks
cannot claim kernel or third-party parser instruction preemption. Legacy public
fetch/extract APIs must retain compatibility. No browser/JS/search/live campaign.

## Next

Mutation/Cargo/fake-fixture authority **RELEASED** to parent after this handoff.
Parent: review the small R6 diff, reconcile artifact loss, restore/qualify normal
debug+release artifacts, finish remaining directed proofs, then own delivery and
continue R7/R8. T50 itself is not READY; T44 PAUSED/T27 campaign exhausted unchanged.

Owner changed paths: `Cargo.lock`, `crates/oc-adapters/Cargo.toml`,
`src/{application,runtime,tools,webfetch}.rs` within that crate,
`src/webfetch/{render.rs,format_tests.rs,PROVENANCE.md}`,
`src/tools/tests/webfetch.rs`, `docs/CODE_MAP.md`,
`evidence/T50/{native_webfetch.py,webfetch_check.py,webfetch.md,webfetch-*.log}`.
New exact dependency `html5ever =0.35.0`,19 added locked packages including it;
no existing upgrade/download. Justification, complete donor MIT notice, cached
build and MIT/Apache transitive provenance are in
`crates/oc-adapters/src/webfetch/PROVENANCE.md`. New logs/reports are bounded well
below1MiB; largest raw log126874 bytes; no foreign/historical cleanup.

Establish nearest falsifiable RED, implement only R6, run prescribed gates and
record factual results here. Parent owns staging/delivery/progress and later R7/R8;
T44 stays PAUSED and T27 live campaign remains exhausted.

### Execution observations (append-only)

- RED: `cargo test -p oc-adapters --lib --locked tool17_` exit101,
  0/2 passed: unknown format accepted; catalog format/default absent.
- First GREEN owner pack16 passed. Initial mandatory workspace exit0:
  1386 passed / 0 failed / 10 existing ignores / 42 summaries,
  `webfetch-workspace20261001.log`. This is **superseded qualification** while
  the application test-exception seam below is rebuilt/requalified.
- Initial direct fixture failed before any GET: application construction passed
  false unconditionally to Runtime's existing loopback option. Separate explicit
  `OC_TEST_WEBFETCH_ALLOW_LOOPBACK=1` now bridges only that existing test exception;
  provider flag stays independent, defaults remain refused, private remains refused.
  Fixture SSE added/done/completed graph also corrected. Initial failed diagnostic
  log retained; no live effect, orphan or historical cleanup.
