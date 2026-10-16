# T51 R4B — real-user existing/fresh PTY + strace

## Result

**R4B PASS** on source revision `bb527947a41dc209c3c62bbe8d53277592ae0f2a`, branch
`agent/oc-rust-port`. Both mandatory real-user profiles actually ran from the
current repository as the current non-root account (UID 1003), with inherited
HOME/XDG/PATH/TERM/locale and unchanged configuration resolution. Existing data
was accessible: its real exclusive native lock was acquired and released, with
no competing-owner blocker. The fresh profile used the same configuration and
binary with a new private native data directory.

| E2E06 profile | Result | Evidence |
| --- | --- | --- |
| Source-derived regression | PASS, delivered R4A | `inherited-fixture.md`; retained 1297-test workspace qualification and two native inherited-selection cases |
| Real-user existing default store | **PASS** | Final run `T51-real-user-_1of_3b_`, `existing.trace`; bare `target/release/oc`, exit 0 |
| Real-user fresh native store | **PASS** | Same final run, `fresh.trace`; same binary/environment plus `--data-dir`, exit 0 |
| Explicit repair/durable recovery | PASS, delivered R4A | Controlled fake-service identity/effect/restart evidence in `inherited-fixture.md`; real-user choices were only viewed |

This report completes the owned R4B execution slice. Parent review and the
combined R1/R2/R3/R4 closeout remain with the parent coordinator; no task/progress
state was changed here. T44 remains PAUSED; no paired pixel/V09/full-GOAL claim.

## Checks

### Binary and preflight

The retained artifacts were hashed before the first launch and after the final
launch, unchanged throughout:

- Release: `9c56cb0ce2f67de8c01411032547b0c630b29aeb7af1e19ab3111ee7209e7af4`.
- Debug: `8d3b36e2e39affdc5fff18c160e1e61404eeb9e06351dbe78f1281d7738adfd1`.

These are the normal R4A rebuilt artifacts now associated with the delivered
Rust source at clean tracked HEAD `bb527947`. No Cargo/rebuild/Rust change was
needed for this slice. Initial Git status contained only inherited untracked
`.opencode/`; it was not manually inspected, edited, staged or copied. Only the
product's admitted ordinary startup read its configuration/directory metadata.

Final preflight: MemAvailable 8,616,882,176 bytes; approved cache disk free
208,365,502,464 bytes; inherited TERM and LANG present (values not recorded).
Both PTYs were 120 columns × 40 rows. The existing native root was owned by the
current UID, mode 0700; existing native lock mode 0664 and database mode 0644.
Fresh root mode 0700, native lock/database mode 0600. Existing permissions were
not modified. Protected config contents and runner credential/state files were
not read by the harness; inherited environment values were not dumped or copied
into evidence. `Popen` supplies no `env` override.

The opt-in one-off developer harness is `scripts/real_user_startup.py`. It reuses
the nearest trusted `visible_rows` function from
`crates/oc/tests/support/startup.py` by extracting only that function through AST,
without executing the hermetic campaign's module-level setup. UI evidence uses
fixed public menu markers and the common `ServiceDiagnostic` Display/enum shape.

### Exact commands and envelope

Repository cwd for every product launch: `/home/opencode/ai/oc`.

```text
sha256sum target/release/oc target/debug/oc
python3 scripts/real_user_startup.py --ack-real-user --baseline-from-run T51-real-user-vfw2bma0
python3 scripts/real_user_startup.py --audit-private-run T51-real-user-_1of_3b_
git diff --check
python3 scripts/check_docs.py
```

Final harness and metadata audit exited **0**; both actual product exits were
**0**. Docs check exited 0 (54 tasks / 161 acceptance specs; structural check
only). `git diff --check` exited 0. Product child commands, with private cache
and pipe descriptors represented by placeholders:

```text
strace -f -qq -yy -s 256 -e trace=openat,openat2,newfstatat,statx,flock,fcntl,connect,clone,clone3,fork,vfork,execve,execveat,wait4,waitid,exit,exit_group,close,chdir -e raw=execve,execveat -o /proc/self/fd/<owned-trace-pipe> target/release/oc
strace -f -qq -yy -s 256 -e trace=openat,openat2,newfstatat,statx,flock,fcntl,connect,clone,clone3,fork,vfork,execve,execveat,wait4,waitid,exit,exit_group,close,chdir -e raw=execve,execveat -o /proc/self/fd/<owned-trace-pipe> target/release/oc --data-dir <approved-cache>/T51-real-user-_1of_3b_/fresh-native
```

Only the spawned oc and its descendants were traced, never an unrelated PID.
No read/write/send/recv/ioctl payload trace, argv/env rendering, provider request
body, raw HTTP response or config dump. Private trace filenames/addresses are
sanitized to opaque identities before reporting. Output limits were frozen at
16 MiB per trace and 16 MiB in-memory PTY output, 1 GiB own aggregate with
100 MiB reserved headroom. Profile watchdog 180 seconds, including 15 seconds
reserved for graceful shutdown; bounded owned-signal fallback exists but was
unused. Product discovery/MCP deadlines were not changed. A 35-second ordinary
service observation interval followed Home before view actions.

### Pre-prompt UI and preservation

Both profiles: Home logo, Sessions/history list, Select agent, Select model,
Settings/configuration diagnostics, Select variant and MCP servers were actually
observed before any prompt. Existing Sessions was nonempty; fresh Sessions showed
the public `No sessions available` marker. History was browsed through its list;
no history entry was resumed/selected, so the saved deck and active choice were
not changed. Restored Home remained responsive with the existing typed
`agent_unavailable` diagnostic. Both profiles showed `ignored_setting` and
`unsupported_plugin`; the failed Settings entry's read-only details were opened
and the latter code was visibly rendered.

Exact interaction sequence in each final profile:

1. Ctrl+X L → Sessions; Escape. Shift+Tab → agent selector; Escape.
   Ctrl+X M → model selector; Escape. No Enter on these selectors.
2. Ctrl+P, filter `Open settings`, Enter to invoke the view command. Filter
   `failed`, Enter to show diagnostic details only (the mutating Permissions item
   cannot match this filter). Six Backspaces clear the filter; filter `ignored`.
   Escape clears the query; a second Escape closes Settings.
3. Ctrl+P, filter `Switch model variant`, Enter to invoke the view command;
   Escape without choosing a variant.
4. Ctrl+P, filter `MCP servers`, Enter to invoke the view; eight Down keys browse
   status rows; Escape. No toggle, reconnect, tool call or config action.
5. Ctrl+P, filter `Exit the app`, Enter; natural owned shutdown, exit 0.

No generation prompt, paid request submission, search or ordinary tool action
was created. No user's saved choice was entered, no draft/private history text
was retained in the report, no pref/deck reset, native data wipe or TS DB import.
The prior trace established the exact existing native root; SELECT-only native
SQLite aggregates were then captured **before final launch**, after startup
before views, and after exit. No SQL mutations or native lock operations were
performed by the observation connection.

| Native row counter | Existing: before launch / before views / after exit | Fresh: after initial startup / after exit |
| --- | --- | --- |
| sessions | 49 / 49 / 49 | 0 / 0 |
| messages | 54 / 54 / 54 | 0 / 0 |
| turns | 31 / 31 / 31 | 0 / 0 |
| turn_acceptances | 11 / 11 / 11 | 0 / 0 |
| tool_operations | 48 / 48 / 48 | 0 / 0 |
| events | 181 / 181 / 181 | 0 / 0 |
| patch_effects | 0 / 0 / 0 | 0 / 0 |
| prefs | 94 / 94 / 94 | 0 / 0 |

Existing ordered key/value/updated_at preference digest was identical at all
three observations:
`70e684b2008d51e82d8feff76a3fb958dc4894595361f558c8350f18683036da`.
Fresh preferences stayed empty (SHA256 empty digest
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`).
Fresh data directory was created empty immediately before its product launch;
the product created its native schema/lock/WAL normally, without a new session
or history/effect/preference row. Counts and digest include all native prefs,
not a selected subset concealing a deck rewrite.

### Actual metadata trace findings

| Source/root | Opaque root | Existing successful completed open lines / config opens | Fresh successful completed open lines / config opens |
| --- | --- | --- | --- |
| global | `path-99a5be980b486d71` | 27 / 1 | 27 / 1 |
| Location | `path-d7d77e9022651229` | 345 / 1 | 383 / 1 |
| .opencode | `path-732a69d13ddcc75b` | 4 / 0 | 4 / 0 |
| existing native store | `path-6538d377a791903a` | 5 / 0 | — |
| fresh native store | `path-27a995e76140d163` | — | 7 / 0 |

Counts are completed syscall lines, not counts of distinct files, and exclude
unfinished/resumed pairs from the open-line metric. In each profile the same
two actual config files opened in order: global `path-53464d0198ae687b`, then
Location `path-95c77dfe97396695`. The inherited .opencode directory/source root
was genuinely opened/probed; **zero** opencode.json/jsonc file opens in that
root were observed. This does not invent a third loaded JSON document. Full
precedence/unsupported-entry assertions remain the controlled R4A owner oracle.

Both native SQLite paths were observed. Each trace has exactly one successful
native `LOCK_EX` acquisition and one `LOCK_UN` release, zero would-block result.
Each has one successful raw `execve` (oc), nine clone3 call entries, nine thread
exit entries and one exit_group. Completed clone lines are 7 existing / 8 fresh
CLONE_THREAD successes, zero nonthread successes; thread interleaving explains
the incomplete-line difference. Only oc and strace process executables were
observed by exact owned PID/startticks. No other exec, npx/browser path metadata,
or successful JS/TS source-file open was observed.

Each final profile had **21 connect syscall entries**. Network activity is
ordinary admitted initial startup, not a zero-network fixture: the actual public
MCP inventory showed **two Connected and one Disabled** status rows in both
profiles. Those enabled external MCPs are explicit user dependencies; the
disabled entry stayed disabled. No browser/config enabling was performed.
Connect metadata includes socket/DNS/service activity and unfinished/resumed
calls; it is not an HTTP request or generation counter and cannot by itself
assign each connection to discovery versus MCP. Zero accepted prompt/tool
effects are independently established by the exact UI actions and native row
counters. The no-paid repair/generation/recovery wire oracle is R4A.

### Cleanup and bounded raw association

| Final profile | Duration | Raw trace bytes | PTY bytes, memory only | Exit | Terminal restored / ALT_LEAVE | Owned remaining / forced signal |
| --- | --- | --- | --- | --- | --- | --- |
| existing | 60.62s | 662,046 | 191,620 | 0 | yes / yes | 0 / no |
| fresh | 60.76s | 664,594 | 189,385 | 0 | yes / yes | 0 / no |

Raw traces and the aggregate-only JSON are in the approved external cache under
run ID `T51-real-user-_1of_3b_`, directory mode 0700, raw traces mode 0600.
No raw PTY was written to disk. No trace truncation; own aggregate across all
three paired runs was **6,277,181 bytes**, far below 1 GiB. No orphan was adopted
or reaped in the successful runs; native locks released normally. Original
default-store owners were never killed or attached, and no unknown external
effect was replayed.

### Harness refinement record

Initial paired run `T51-real-user-knnl4h9l` was **FAIL as qualification**: Home,
history, agent/model views and cleanup succeeded, but the harness incorrectly
assumed Ctrl+U clears a dialog query and one Escape closes filtered Settings.
It missed the subsequent variant view. Both product exits were 0 with unchanged
native counters/preferences and no generation/tools; this was not product RED.

Corrected paired run `T51-real-user-vfw2bma0` was **PASS** for mandatory local
views (both variants observed), natural cleanup and unchanged aggregate rows;
its trace safely established the default native root for the final pre-launch
observation. Final paired run above additionally opened safe failed-plugin
details, captured public connected/disabled MCP status markers and proved
before-launch → startup → after-exit store preservation. Retries were bounded
known view/startup observations after verified owned cleanup, not replay of an
uncertain tool. No product bug/source fix was required. Post-capture harness
edits only improve offline metadata audit, fail-exit handling and watchdog
enforcement; they do not alter the retained binary or the reported captures.

## Risks

No remaining R4B external blocker or reproduced product failure. Metadata tracing
deliberately does not inspect HTTP/config/history payloads, so provider wire
identity/refusal/repair/permission and fatal-security assertions rely on the
already delivered source-derived R4A/R1–R3 fake-service owner evidence. Real-user
profiles exercised read-only menus/history listing, not opening a new saved tab
or deliberately changing a user's model/agent/variant. The task closeout must
review all current R1–R4 evidence together; this report alone is not full READY.

## Next

Return sole mutation/PTY/strace/fixture ownership to the parent. Review only the
new opt-in harness and this sanitized evidence file, consolidate T51's factual
report and complete parent-owned closeout if R1/R2/R3/R4 are all qualified. No
progress/GOAL/spec/acceptance/staging/commit/push operation was performed here.
