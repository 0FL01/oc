# T50 — selected native tool parity

## Result

**PASS: all assigned R1–R10 and TOOL12–TOOL21 outcomes, including the approved
same-task switch, shell-control, CLI catalog, common-output and external-read
supplements.** This is selected native parity, not universal upstream parity,
full T44/V09, fresh paid-provider qualification or product READY.

Implementation base for the task's slices: `32dbf48184f9980318aea8dbec9506a480d0b212`.
Latest reviewed implementation: `c7265a864567d7721e3962ff5bfa3a12952330ba`.
Owner-only intervening plan commits were preserved; qualified binary associations
are the report-specific base plus reviewed implementation, not clean-base claims.

| Outcome | Required behavior and primary evidence | Delivery |
| --- | --- | --- |
| R1 / TOOL12 | Coherent real schemas, exact OC2 file predicate, policy narrowing, issuing-view execution; busy draft versus commit and same-task A→B→A, compatible history and actual request attribution. `file-mutations.md`, `live-model-switch.md` | `0804b7db`, `0504649d` |
| R2 / TOOL13 | Canonical command and hidden literal argv compatibility; independent durable jobs/notices, actual inventory/drains, same-PID conversion, selected kill, final flush, recovery and no replay. `foreground-shell.md`, `background-shell.md`, `shell-controls.md`, `shell-review.md` | `592eb188`, `1ef14dac`, `1335ef7d`, `68289e7a` |
| R3 / TOOL14 | Regex/literal/case/include/hidden/ignore, bounded deterministic scanning, active cancellation and invocation-only external scope. `search.md`, `external-read.md` | `b88dc0fe`, `c7265a86` |
| R4 / TOOL15 | Real bound question queue/consumer, ordered answers, free-form/multiple/review, dismissal/cancel/no-consumer, answer-before-continuation and replay. `question.md` | `1afc3d46` |
| R5 / TOOL16 | Numbered text2000, paged directories, validated typed image output, stored-image validation, descriptor/cancel/model caps, shared nested-source owner and external caches. `read.md`, `external-read.md` | `40a201e6`, `c7265a86` |
| R6 / TOOL17 | Text/markdown/html GET, truthful metadata, one DNS/hop/body/conversion deadline, joined cooperative cancellation and egress guards. `webfetch.md` | `cd2a9592` |
| R7 / TOOL18 | Bounded effective model metadata/paging without selection; scoped atomic rename and real title; complete selection-independent `oc models`, no runtime/store/generation effects. `model-session-tools.md`, `cli-models.md` | `0dc85e81`, `b9d02c09` |
| R8 / TOOL19 | Same-ID pending/applied move after the entire durable source turn, fresh destination turn, exact target/generation authority, original child/job provenance, quarantine, rollback/recovery. `session-move.md` | `71fcd0f0` |
| R9 / TOOL20 | Real write/edit matching/BOM/CRLF/EOF semantics through one descriptor-relative prepared mutation owner, preimage approval and confirmed durable effects; strict patch unchanged. `file-mutations.md` | `0804b7db` |
| R10 / TOOL21 | Captured config/common bounded publication; one registered redacted cold artifact; exact read/grep beyond1MiB; leases/TTL/quota/fault/recovery; full shell capture before discard and bounded live/hot tail. `tool-output-common.md`, `shell-output.md`, `shell-review.md` | `9d8b1b81`, `68289e7a` |

All evidence paths above are relative to `evidence/T50/`. Initial partial slices,
failed experiments and superseded limitations remain historical in those reports;
their initial pending labels are not the task's final outcome state.

## Checks

Latest complete locked serial workspace test gate: **1452 passed,0 failed,
10 unchanged opt-in ignored**,42 target summaries, exit0,933.33s. Command:
`cargo test --workspace --locked --no-fail-fast`, separately precompiled, jobs3,
threads1, offline and owned TMPDIR. Complete lossless logs and exit/index receipts:
`external-read-checks.tar.gz`; raw workspace SHA256
`ad3deda947027037c8a797bd90ff100c10d2bf0eede12b6a66b974121d7417cc`.
Earlier full source gates and failed attempts are preserved in the primary reports.
An increased outer runner watchdog permits complete serial execution; no product
deadline, test assertion, queue/output limit, ignored test or A10 baseline changed.

Final source also passed workspace fmt and strict locked all-target Clippy
`-D warnings`, normal locked debug/release builds and both help commands,
Python47, docs/progress structural checks, advisory physical-size check and diff.
Parent independently repeated external lib5/runtime1, strict Clippy/fmt/build,
all24 final normal-ELF external cases and Python47/docs/progress/diff after review.
No production source changed after the complete final workspace gate.

Current normal ELF fingerprints, unchanged through direct parent proofs:

```text
debug   2b2e99f2a3b4449e227083feef71eb9555600e2d3d8730c0e3b479a5efb57ca2
release c1fa152cc208448c117e3981dc17fd8b311390992c8b68848752ed6fd72cf825
```

Native proofs are actual shipped binaries with fake loopback services, durable
call/result/effect rows, real files and owned processes—not simulated cards.
Directed proof campaigns include21 foreground,26 background,14 search,14 question,
20 read,27 fetch,20 model/rename,35 move,22 mutation,7 live-switch,5 shell-control
cases per qualified ELF and the31-case plus5-rejected-form CLI catalog campaign.
They use their phase-associated report fingerprints rather than falsely labeling
all historic campaigns as executions of today's ELF.

Current shell capture proof: Complete3740084-byte resource, bounded1892-byte
continuation; distant read/grep/restart/move/current-Deny; cap16777216 bytes and
quota/registration failures retain actual effect/exit without replay. Model-switch,
compaction and restart projection retains one2616049-byte resource with2502-byte
hot preview, no automatic cold expansion. Short22-byte Interrupted output remains
completed/exit0/effect1 with explicit incompleteness and no logging-failure event.
Question56113 bytes live once in cold storage, hot25590/event309 bytes; exact answer
and replay, no duplicate full event. Bounds and source summaries are in the reports.

External final receipts per ELF:25 main+14 auxiliary requests,94 operations
(80 complete,14 expected failures);72 Allow calls with no consumer or auto flag,
zero grants, first/repeat/restart. Genuine Ask uses actual Once; absent-consumer,
sibling, Deny, symlink, data-root and budget negatives remain effective. Peers,
drains, PTY readers and owned process groups join before exact temporary cleanup.

## Risks

Shell is not a sandbox. Cooperative IO/decode/scan checks cannot preempt a
kernel-stalled syscall. Output cap16MiB, shared quota2GiB, TTL7d and normalized/
redacted publication ordering are explicit native limits, not unlimited/raw OS
ordering claims. Minimal credential-free shell environment and mutation/session
access ceilings remain. Partial/unknown effects are never replayed automatically.

Built-in websearch, Code Mode, browser and PDF remain excluded. T45 retains full
profile/prompt/child/DCP work; T53 retains future provider/catalog sources; T56
interactive PTYs and T44 paired visual qualification are separate. T27's exhausted
authorized live ledger remains24 generations/15 controls/1 search; no allowance
reset, fresh campaign or paid matrix occurred during T50 offline qualification.
Inherited `.opencode/` and foreign owner plans were not inspected or modified.

## Next

Close assigned T50 with this aggregate evidence, then execute the next ready
backend task through the one-active-task journal. Keep T44 paused until explicit
resume and do not claim whole-product readiness or the missing T27 live facet.
