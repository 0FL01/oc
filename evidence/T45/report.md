# T45 — BUILD_READY_LIVE_BLOCKED

Date: 2026-10-07. Frozen source:
`docs/goals/2026-09-21-config-compat-and-subagents.md` R3/R6–R10 and its
owner-approved supplements. Backend implementation and required offline gates
are qualified; **T45 is not DONE**. The original R3 configured-OpenProxy live
subagent facet remains NOT_RUN because its existing immutable campaign has no
generation allowance. This is independent of the owner-deferred T57 AUTH06.

## Current implementation and delivery

The former T53 dependency is resolved. Two current checked implementation/test
slices are committed and pushed to verified `origin agent/oc-rust-port`:

- `3e9325d1b`: preserve idle typed effort facts across nonprojected model-switch
  notices and accepted root/child inputs. Reuse the existing ordered journal,
  prompt assembler and T53 wire lowering; no new parser/schema/selection owner.
- `25df7dc6e`: forward the existing safe failed child `TurnReport.diagnostic` in
  its live terminal event, with the existing fixed fallback. Arbitrary runtime
  errors still use a fixed safe message; result persistence, policy and retries
  are unchanged. Actual Core consumer preserves settled effects and distinct
  historical-retry/final spans across bounded read and reopen.

Earlier checked component slices/commits remain documented in their owner
receipts below, including final pre-T57 T45 qualification at `16c83eaf3`.
This report reconciles those receipts with current gates; it does not rewrite
historical leaves, claim paired visual parity or weaken the frozen contract.

## Frozen outcome matrix

| Outcome | Current result and primary evidence |
| --- | --- |
| R1/R2, T43 prerequisites | Verified historical owner-config start and matching large/frontmatter fixtures in `evidence/T43/report.md`; current workspace retains those tests and guards. Not re-reading owner/authoring config or making another real request. T43 owns its independent foreground/config prerequisite closeout. |
| R3 SUB01/SUB02 backend | Offline qualified: actual foreground overlap/join/cancel and background independent progress; fresh own-profile/model history, continuation, permission/depth narrowing, command routing, durable notices, family/current-state/event projections and precise conversion/navigation/interrupt controls. `foreground-children.md`, `background-children.md`, `command-routing.md`, `child-controls.md`, `child-recovery.md` and current `child-diagnostic.md`. **Blocked only on original live facet below**, not on T44 visuals or further speculative backend work. |
| R4 delivery | Both current slices pushed separately; historical component receipts retain their checked commit identities. Factual report/progress closeout is a separate reviewed documentation slice. User `.opencode/` remains unread/untracked/unstaged. |
| R5 mandatory gates | Current final source: workspace **1713 passed / 0 failed / 11 unchanged opt-in ignored**, fmt, strict locked workspace/all-target Clippy and locked normal debug/release builds PASS. Exact commands/markers below. |
| R6 profiles/Plan/model references | Verified existing `builtin-profiles.md`, `plan-lifecycle.md`, `profile-discovery.md`, `profile-request.md`, `profile-color.md` and `round-removal.md`: real builtin/override/default/hidden eligibility, native Plan path/policy/reminders/reopen, canonical/legacy/Markdown listed-model binding and pre-effect unavailable refusal, own-lane request overlays, unsupported steps diagnostic without a successful-round stop. Current workspace preserves actual provider/effect regressions. T44 paired presentation is separate. |
| R7 host context | Verified native non-root bounded collector and shared default/custom/root/own-model-child/restart/Location assembler: `host-context.md`, with actual normal debug/release `native_host_context.py` receipts and existing current tests. Shell comes from its execution owner, not `$SHELL`; no privilege or environment dump, inventory or filesystem-access claim. |
| R8 CTX01/CTX02 | Verified `context-pack.md` plus `task-renewal.md`: exact selected public RAW text/roles/order and transactional parent branch/cutoff validation, DCP-off IDs, over-budget refusal before admission, frozen background snapshot/restart/continuation, quoted lower-authority provenance and truthful filtered preview. No transcript clone, harness/opaque selection or permanent HOT pin. |
| R9 DCP10–DCP12/A10 | Verified `hot-raw.md`, `hot-renewal.md`, `task-renewal.md`, `dcp-defaults.md`, `dcp-controls.md`, `child-dcp.md` and `dcp320.md`: immutable RAW, renewable bounded HOT during the same admitted task, latest-HOT recovery, intentional forgetting/recompression and reachable manual compact, 40%/55%/false defaults, effective off/manual/commands/debug/Ask/Deny, isolated child defaults and qualified DCP3.2.0 delta/pin/AGPL. Native extension and source-derived differences remain explicit; not full DCP/CodeMode parity or infinite physical resources. |
| R10 PRM01 | Verified `instructions.md`, `skill-preview.md`, `file-family.md`, profile/host receipts and current `chronology.md`: one captured assembler, base/custom plus applicable initial/nested/changed/removed AGENTS, metadata-only skill preview, truthful next-request file family and model attribution, current root/own-model-child chronological system/low/Default across native protocols and reopen. No second selector/parser, body auto-load, authority promotion, foreign child change or tool replay. |

All unqualified paired T44 VIS39/VIS43/profile/picker/prompt surfaces remain
independently **PAUSED**. Functional native PTY/protocol/SQLite qualification is
not visual parity. T50, T51, T53, T54 and T56 retain their own completed scopes;
this task consumes their existing seams, not a reverse whole-task dependency.

## Current checks

Serial Cargo: approved TMPDIR `/home/opencode/.cache/opencode-tmp/opencode`,
`CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2`, normal stacks and unchanged watchdogs.
No test disabling, new ignore, threshold/baseline change or suppressed lint.

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p oc-adapters --test subagent core_child_terminal_reason_and_final_span_reopen_without_retry_replay
cargo test --locked --workspace --no-fail-fast
cargo build --locked --release
git diff --check
```

Final barrier fixture: **1 PASS**, 3.18 s. Final workspace: independently summed
46 result records **1713/0/11**, excluding the separate targeted result; subagent
39/0, adapter library666/0/1, runtime120/0, TUI446/0 and binary97/0. Final log:
`/home/opencode/.cache/opencode-tmp/opencode/t45-child-diagnostic-final-barrier.log`,
ending `T45_CHILD_DIAGNOSTIC_FINAL_BARRIER_GATES_PASS`. Normal release finished
successfully; full existing storage/security/PTY/resource/recovery tests passed.

The same production code also passed normal debug/release builds and actual
binary scripts, in `t45-child-diagnostic-current2.log` in the same approved cache,
ending `T45_CHILD_DIAGNOSTIC_ALL_GATES_PASS`:

```text
cargo build --locked
cargo build --locked --release
python3 evidence/T45/native_child_controls.py target/debug/oc
python3 evidence/T45/native_child_controls.py target/release/oc
python3 evidence/T45/native_child_recovery.py target/debug/oc
python3 evidence/T45/native_child_recovery.py target/release/oc
python3 evidence/T45/native_chronology.py target/debug/oc
python3 evidence/T45/native_chronology.py target/release/oc
```

- Controls: **4 cases per binary**, source/generation/live routing/conversion,
  owned shell cleanup and stale/terminal-race refusal without repeated work.
- Recovery: **20 cases per binary**, safe unchanged job/turn/HOT resume versus
  mutation/MCP/corrupt admission quarantine, terminal notice once and idle reopen
  with no dispatch/effect. Donor-safe recovery is at-least-once, not exactly-once.
- Chronology: **6 cases per binary**, Responses/Chat/Messages declared support,
  unsupported and final-marker mismatch, low then Default reset, root/own-model
  child/reopen, settled read once and original turns unchanged. Each isolated
  case10 primary+1title requests; these are synthetic local counts, not a live
  allowance or pre-dial qualification. Owned peers/processes joined/reaped.
- Current chronology slice also qualified existing `native_instructions.py` on
  normal debug/release (15 requests/13 facts each); source/symlink/restart/Location
  and root/child guards intact. See `chronology.md` for the exact receipt.
- Python progress tests47 PASS; `scripts/check_docs.py`, progress structure and
  diff checks PASS. These validators do not themselves establish product PASS.

The final parent-order barrier is test-only and was followed by the full final
workspace chain. Production/native binaries from the earlier actual-binary chain
were unchanged; no stale binary claim or repeated real side effect was used.

## Exact remaining external prerequisite

R3 primary evidence explicitly requires the **existing bounded live run spawning
one subagent**; the original plan (`evidence/subagents/upstream-v2.0.12-plan.md`,
live-check section) specifies configured OpenProxy. No such receipt is present.
The existing T27 real workflow was root-only: its reviewed native config sets
`subagent` false (`evidence/T27/live_workflow.py:275–281`); its seven completed
tool operations contain no child. T53's separately authorized Go campaign also
has no live child and a different fixed authority, so neither can replace this
required facet. Local fake peers do not grant a real provider PASS.

Read-only current probe of the original guarded campaign owner
`evidence/T55/live_native.py::identity` and `Ledger::snapshot` confirms:

```json
{"existing_configured_campaign_counts":{"control":15,"generation":24,"mcp":1},"remaining_generation":0,"unknown_reservation_remains_spent":true,"network_dispatches":0,"ledger_unchanged":true}
```

The original campaign identity `39d44cb58b834de99544daf3c2eedab1` and request24/24
remain unchanged. Unknown/reserved17 is still spent. `evidence/T27/allowance-block.md`
records the no-refund/no-reset boundary. No Relay/ELF/network/credential reader
was invoked by this probe. A smaller model, anonymous/paid fallback, different
provider or counter reset cannot restore the exhausted approved authority.

**Smallest unlock:** an explicitly authorized allowance extension/new campaign
for this original configured-OpenProxy facet, with the existing durable pre-dial
all-lane/retry/title accounting and output/resource guards, or an explicit owner
deferral/waiver of that facet. Neither is inferred from deferring T57 AUTH06.
No further independent T45 coding outcome is unresolved. Keep the task blocked,
do not run `finish`; continue the independent T43 prerequisite closeout.

## Scope and final status

Reviewed diff stays within existing effort/history/turn and safe child callback
owners, nearby tests, actual native fixture and factual docs. No new DTO/store/
schema/retry framework, guard weakening, raw history mutation, credential export
or user config change. Historical evidence and T53 live13/24 are untouched.
Delivery: implementation slices **PUSHED**. Task: **BUILD_READY_LIVE_BLOCKED**,
not DONE; product READY and paired visual/OAuth/live-child PASS are not claimed.
