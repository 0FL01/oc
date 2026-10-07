# T42 — offline qualification and compatibility fixes (F18, AUD38–AUD40)

## Current qualification — 2026-10-07

**AUD38/AUD39 PASS; scoped T42 complete. AUD40 does not declare product READY.**
Fixed code commit: `75568ed2ad6684328d6fb142e3aeb6d6a1543d24`, reviewed/pushed
to `origin/agent/oc-rust-port`, unchanged during qualification. This fulfills T30's
post-backend audit, not the historical 331-test receipt below.

### Reproduced failures, not waived gates

[current-changes.md](current-changes.md) records initial **1712/1/11**, allocation
stacks and fixes: discard foreign public catalog trees with full JSON validation;
keep golden HOME/XDG fixture authority. Public8MiB cap, atomic Go/OpenAI cache,
routing, AUD32 workload/baseline/2ms sampler/120s watchdog/64MiB bound remain.
No production profiling hook remains.

### Executed current gates

Pinned `rustc 1.93.0 (254b59607 2026-01-19)` and
`cargo 1.93.0 (083ac5135 2025-12-15)`; approved TMPDIR, jobs3/testthreads2,
serial Cargo, normal stacks. Every command below exited zero.

| Command / direct surface | Current result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --locked --workspace --no-fail-fast` | **46 result records: 1715 passed / 0 failed / 11 unchanged opt-in ignored** |
| `cargo build --locked`; `cargo build --locked --release` | Normal native debug/release ELF; PASS |
| `python3 evidence/T42/current_offline.py target/{debug,release}/oc` (separate invocations) | Fresh owned HOME/XDG/CARGO, Node/Bun/npx-free PATH; help/smoke + offline3/debug golden1 each, total8/0/0 |
| `python3 evidence/T45/native_chronology.py target/{debug,release}/oc` | Twelve PASS: root/child low/Default/mismatch, system placement, read once, immutable old turns, joined cleanup |
| `python3 evidence/T45/native_child_controls.py target/{debug,release}/oc` | Eight PASS; owned leaves reaped, stable identity, zero reopen RPC |
| `python3 evidence/T45/native_child_recovery.py target/{debug,release}/oc` | Forty PASS; dispatches match, safe-only resume, sticky unknown, no effect replay/reopen request |
| `cargo test --locked -p oc --test memory_bounds -- --nocapture` | Final fixed-commit measurement1/0/0; details below |
| `cargo metadata --locked --offline --format-version 1 --filter-platform x86_64-unknown-linux-gnu` | Four workspace members, 312 host graph packages, zero missing declared license/notice |
| Python scripts unittest discovery; docs/journal/diff checks | 47 PASS; structure and diff checks PASS, not product proof by themselves |

Full serial log:
`/home/opencode/.cache/opencode-tmp/opencode/t42-current-75568ed2a-final.log`,
ending `T42_CURRENT_75568ED2A_ALL_OFFLINE_GATES_PASS`. Independent aggregation
checked both the 46 workspace result records and the structured native receipts.
Adapters lib668/0/1, runtime120/0/0, subagent39/0/0, TUI446/0/0, binary97/0/0.
The eleven ignores were not counted as passing or changed to obtain green.

Final AUD32: archive8 → 3000 pairs ×16384 B plus turn logs, active2 pairs ×1024 B.
Small: baseline12, peakRSS56464, endPSS53731 KiB, DB942080 B. Large: baseline12,
peakRSS57256, endPSS54526 KiB, DB152944640 B; peak anonymous12768/file44488 KiB,
eight threads, zero children. Active provider history remains equal. Actual large
growth57244 KiB is below the unchanged65536 KiB limit.

The Node-free driver does not claim release golden: that existing Cargo test
embeds the debug binary. Release is directly exercised by help/smoke and the
three native child/chronology fixtures. All model peers here are synthetic;
credential-free public metadata may refresh, so this is **not zero network**.
No model/live-auth campaign or existing live ledger was used or reset.

### AUD39 — F01–F18 current evidence

Original repair SHAs remain in the historical table. The following current suites
ran on `75568ed2a`, not merely inspected.

| Finding | Repair anchor / current direct proof |
| --- | --- |
| F01 mock entry points | T31; golden_binary, e2e_offline3, PTY and native child/chronology ELF |
| F02 typed continuation/call_id | T34; responses4, runtime120, golden and child read/result/reopen |
| F03 EOF/cancel/terminal semantics | T34/T54; responses4, runtime, actual approval/cancel/recovery PTYs |
| F04 patch temp symlink | T32; patch_audit10, patch_effects4, binary golden |
| F05 grammar/preimage/partial outcome | Same T32; patch_audit/patch_effects and current applied-patch PTYs |
| F06 intent before effect/order | T33; durability1, blob_audit11, runtime/subagent recovery |
| F07 model compress/nudges | T36/T45; dcp_runtime4, runtime/DCP tests, golden/PTY compression |
| F08 workspace definitions | T35/T43; configured_workspace15, composition/defs, Location/profile/command/skill PTYs |
| F09 aliases/effective permissions | T35/T43; permissions8, configured_workspace, runtime resource refusals |
| F10 case-insensitive MCP headers | T37; mcp_remote27/1 unchanged ignore, mcp_application41 |
| F11 MCP generation/cleanup | T37/T46; mcp_application41, mcp_stdio21/1 unchanged ignore, Location PTY |
| F12 Unicode/egress | T38; webfetch owner tests, real-dispatch request guard, golden |
| F13 shell pipes/descendants | T38; shell_watchdog2, shell owner tests, actual child leaves/terminal cancellation |
| F14 terminal loop integration | T39/T56; pty16, pty_t39 51, pty_t42 34, terminals1; functional, not T44 pixels |
| F15 active vs archived history | T40/current75568ed2a; context_bounds3, memory_bounds1, soak4 and real-ELF metrics |
| F16 readable blob recovery | T33; blob_audit11 and current crash/recovery targets |
| F17 config/discovery | T35/T46/T51/T53; config/composition/discovery/models_dev, configured_workspace15, source-trust/PTY |
| F18 proof bypass/false live success | T41/T42; real-ELF golden/PTY/Node-free/native gates; real-provider remainder BLOCKED, never PASS |

### AUD40 — honest product boundary

No unresolved failure of these current audit regressions is known. This does not
complete the full expanded GOAL or T30: mandatory paired T44 scope stays PAUSED,
T57 AUTH06 is owner-deferred, and the original configured-OpenProxy campaign is
exhausted24/24 with unknown/reserved17 still spent. T45's original live child facet
is also unqualified; the Go campaign's13/24 and fake child captures are not a
substitute. Existing [T45 report](../T45/report.md) preserves the allowance fact
and smallest unlock; no new paid campaign/refund/reset is authorized here.
T30 must enumerate current A01–A13/full mandatory outcomes before READY. No
whole-product BUILD_READY/READY or visual/live waiver is issued by this audit.

Static review: core remains UI/storage/network-independent; four native Rust2024
crates, no Node/Bun/JS host. No manifest/lock changes from `c6e08c4bf`. Tracked-source
secret-pattern scan returned zero matches without printing values. Host dependency
license declarations were reviewed, including native WS MIT/Apache and rustls
Apache/ISC/MIT; this is not a legal opinion. DCP AGPL-3.0-or-later source pin,
adopted derivative attribution and notices in SOURCES/DCP/DECISIONS remain intact;
neither the 3.2.0 subset nor this Rust port is relabelled full MIT parity. User
`.opencode/`, credentials and authoring configuration were not read or staged.

## Historical qualification — 2026-09-21 (unchanged)

> The following original receipt, counts, supported-capability descriptions and
> next-step hints are historical. The current qualification above supersedes them;
> old failures are not rewritten into retrospective success.

Commit under test: see `checks.md` (SHA recorded per command). Scope: `audit/repairs/T42.md`
plus four compatibility/acceptance fixes requested by the owner on 2026-09-21 that the
repair cycle had not yet proven (bare `oc` TUI, one-lifecycle Location switch,
`apply_patch` diff card, config source trust). No new feature scope, no new crates.

## 1. What changed

| Gap | Fix | Code |
|---|---|---|
| Bare `oc` was a usage error | bare `oc` on a terminal launches the same TUI as `oc tui`; without a terminal it exits 2 with `use \`oc run "<prompt>"\` for headless use`; `oc tui` is the explicit equivalent | `crates/oc/src/bootstrap.rs`, `cli.rs`, `tui_cmd.rs` (`interactive_ready`) |
| No Location switch inside one lifecycle | `/location <path>` rebuilds the complete target generation (config, catalog, agents/skills/commands, MCP, runtime, session) before publication; only then the old MCP resources are closed and the state swapped; failure keeps the current Location; refused while a turn streams; generation-bound caches are dropped; sessions stay Location-bound and a return reopens the recorded session | `oc-core` `InboxMsg::SwitchLocation` + `LocationSnapshot`; `oc-adapters/src/application.rs` (`build_runtime`, `switch_target`, supervisor loop); `oc-tui` `CommandAction::SwitchLocation`, `PanelIntent::SwitchLocation`, `TuiState::reset_workspace`; `oc/src/tui_cmd.rs` |
| `apply_patch` cards showed only a 512-byte input preview | bounded diff summary (per file: op marker, path, +/- counts, hunk count, rename target; totals; file cap) rendered in the card, never a second copy of the patch | `oc-adapters/src/patch.rs` (`DiffSummary`, `diff_summary`), `oc-tui/src/history.rs`, `oc-tui/src/app.rs` (`card_row`) |
| Config source trust followed symlinks out of the admitted root | every `opencode.json/jsonc` must canonicalise inside the canonical root that declared it, the `.opencode` root must stay inside the Location root, and `AGENTS.md` must stay inside its root; escapes fail closed; in-root symlinks stay admitted | `oc-adapters/src/composition.rs` (`admit_instruction`, containment in the sources loop and `.opencode` admission) |

## 2. Reproducing failures captured before each fix (RED)

- `evidence/T42/red-config-trust.txt` — worktree at the pre-fix commit with the new tests
  appended: exit 101, `0 passed; 2 failed`
  (`symlink escape must fail closed: ()`, `symlinked local root must fail closed: ()`).
- `evidence/T42/red-pty.txt` — worktree at the pre-fix commit with `crates/oc/tests/pty_t42.rs`:
  exit 101, `0 passed; 3 failed`:
  - bare `oc` printed `usage: oc [--data-dir PATH] <run|sessions> | oc --smoke` and never drew the TUI;
  - `/location` was not a command (no `usage: /location` note);
  - the `apply_patch` card rendered `[src/lib.rs] -> update src/lib.rs (hash_before=f83…` — no bounded diff.
- Earlier composition RED run (same tests, before the fix in-tree): the two escape tests failed
  while the positive control (`in_root_symlinked_config_stays_admitted`) passed.

## 3. Executed qualification (exact commands, exit codes, counts)

Full log: `evidence/T42/qual-test.log`, clean-environment log: `evidence/T42/qual-cleanhome.log`.
Machine/versions: rustc 1.93.0 (254b59607 2026-01-19), cargo 1.93.0 (083ac5135 2025-12-15).

| Command | Exit | Result |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | 0 | clean |
| `cargo test --workspace --locked` | 0 | **331 passed / 0 failed / 4 ignored** |
| `cargo build --locked` | 0 | binary built |
| `target/debug/oc --help` | 0 | CLI surface intact |
| `git diff --check` | 0 | clean |
| `python3 scripts/progress.py check` | 0 | journal structure |
| `python3 scripts/check_docs.py` | 0 | docs/registry/journal structure |
| clean HOME/XDG, PATH without node/bun: `env PATH=<tmp>/bin:~/.cargo/bin <e2e_offline binary>` | 0 | **3 passed / 0 failed** (`no-node`, `no-bun` verified on that PATH) |
| `env -i HOME=<tmp> XDG_*=… PATH=/usr/bin:/bin target/debug/oc --smoke` | 0 | `oc smoke core=oc-core adapter=oc-adapters tui=oc-tui` |

Ignored (4, unchanged in kind): the three pre-existing external harnesses plus the
credential-gated `live_bounded_campaign` (T41). None counted as passing.

New executed tests in this task: 3 (composition symlink trust, incl. one positive control),
1 (`patch::diff_summary`), 3 (`crates/oc/tests/pty_t42.rs`: bare `oc`, `apply_patch` diff card,
A→B→A Location switch on the real binary under a PTY).

## 4. AUD38 — clean build and regression qualification

All of the commands above ran on one working tree at the SHA in `checks.md`. Mandatory audit
regressions were executed inside `cargo test --workspace --locked`: adversarial T32/T33/T34/T38
suites (`patch_audit`, `blob_audit`, `durability`, `responses`, `shell_watchdog`), the actual
binary golden campaign (`golden_binary`, 11/11 checks), actual binary PTY suites
(`pty`, `pty_t39`, `pty_t42`), long-history/context/resource qualification
(`context_bounds`, `memory_bounds` with RSS/PSS bounds, `soak`, `storage_lock`) and the
offline end-to-end path (`e2e_offline`, 3/3 in a Node-free PATH). Static checks: no secret
material matched `sk-*`/`ghp_*`/`AKIA*`/private-key patterns; no new dependencies
(`Cargo.toml`/`Cargo.lock` unchanged in this change set); AGPL provenance notices for DCP are
still present (`docs/DCP.md`, `docs/DECISIONS.md`, `AGENTS.md`).

## 5. AUD39 — findings mapped to current evidence

| Finding | Repair | Closing commit | Current evidence |
|---|---|---|---|
| F01 mock entry points | T31 | `e92b614` | `golden_binary`, `e2e_offline`, `pty*` on the real binary |
| F02 structured continuation / call_id | T34 | `9daa55c` | `responses` (4), `golden_binary` patch rounds |
| F03 EOF/cancel semantics | T34 | `9daa55c` | `responses`, `runtime` (31) |
| F04 patch temp path symlink | T32 | `18b0792` | `patch_audit` (10) |
| F05 patch grammar/preimages/partials | T32 | `18b0792` | `patch_audit` (10), `pty_t42` applied patch |
| F06 intent before side effect | T33 | `4e07cc7` | `durability` (1), `blob_audit` (11) |
| F07 model-driven compress/nudges | T36 | `839279c` | `dcp_runtime` (2), `pty_t39` compress round |
| F08 workspace definition semantics | T35 | `1aac158` | `configured_workspace` (6), `pty_t42` A/B commands and skills |
| F09 legacy permission aliases | T35 | `1aac158` | `configured_workspace` (6) |
| F10 Authorization header case | T37 | `4574a2d` | `mcp_remote` (20), `mcp_stdio` (10) |
| F11 MCP ownership per generation | T37 | `4574a2d` | `mcp_application` (7), `pty_t42` A/B MCP tool presence |
| F12 HTML UTF-8 panic / egress guard | T38 | `4788c64`/`33ab80c` | `oc-adapters` lib (136), `shell_watchdog` (2) |
| F13 shell hang on stdin/child pipes | T38 | `4788c64`/`33ab80c` | `shell_watchdog` (2) |
| F14 TUI not wired to the terminal loop | T39 | `6de9e9a`/`f759ccb` | `pty_t39` (3), `pty_t42` (3) |
| F15 outgoing cap vs history reads | T40 | `142173e`/`9e529a5` | `context_bounds` (3), `memory_bounds` (1) |
| F16 blob recovery digest | T33 | `4e07cc7` | `blob_audit` (11) |
| F17 config/discovery edge cases | T35 | `1aac158` | `composition` tests incl. new symlink trust, `configured_workspace` |
| F18 green evidence bypassing the shipped path | T41 + T42 | `b565993`/`e239315` + this report | binary golden/PTY/clean-env runs above; `e2e_live` blocked semantics |

No finding is closed by a mock-only or unit-only claim: every P0/P1 above has at least one
executed test on the shipped `oc` binary or the shipped adapters path. No
unsupported→supported transition without a test: `mode: subagent` remains an explicit
`UnsupportedCapability`.

## 6. AUD40 — readiness depends on actual gates

T30 FINAL must enumerate A01–A13 and the mandatory T27 live outcomes. This task does **not**
declare READY: the product is **offline-qualified** only. `READY` is impossible while the
mandatory live gate (T27: real OpenProxy Responses + mandatory `codex_web` MCP on the
production composition path) has not passed; the credential-gated live campaign remains
`BLOCKED` in this environment and is reported as such, never as passing. No unresolved P0/P1
code or security blocker is known after this run.

## 7. Risks and next step

- Risk: the Location switch is refused while a turn streams (explicit refusal, nothing lost);
  a queued-switch UX is out of scope for A08/A13.
- Risk: the clean-PATH scenario proves "no Node/Bun required" for the offline product path;
  live provider/MCP dependencies are exercised only by T27.
- Next step: `T27` — bounded live campaign on the actual binary with the owner's config
  (Responses + `codex_web`), then `T30` FINAL over A01–A13.
