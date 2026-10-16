# T44 — sessionless Home and durable first turn

## RECON and contract

Pinned OpenCode v2.0.12 navigates to sessionless Home on a new tab and
creates a durable session only when its first prompt is submitted
(`packages/tui/src/context/session-tabs.tsx:384-397`,
`packages/tui/src/component/prompt/index.tsx:1203-1234`). Native previously
created a durable root on bare TUI launch and for `/new`, leaving unused
sessions. Creating the root separately from turn admission would also leave
an orphan on a rejected first prompt. This slice does not yet implement a
retained tab deck or the clickable add control.

## Implementation and tests

Commit `2447063` adds one application-owned fresh-submit receipt: it resolves
the actual Home agent/model/variant, publishes that agent's permission lane,
checks exact model/variant/title selection and runtime token admission, then
atomically commits a new root, `session_created`, its Location binding,
optional scoped initial selection, `turn_started`, the user message and its
event. The receipt and `TurnStarted` follow commit. Existing-session submits
continue through their established path. SQLite fault-injection tests cover
turn/message/selection inserts, duplicates and clean retry. Runtime and
application tests cover pre-accept rejection, actual provider failures,
post-accept failure, busy refusal, and pinned selection.

Bare TUI Home now has no session ID. Empty/abandoned drafts do not create a
root; accepted first submit attaches the durable ID while pending edits remain
editable and rejection leaves the Home draft. Home model/agent/variant choices
use real Location/agent drafts without a fabricated session preference;
restart restores a saved draft, and an unchosen Home follows a changed
Location configuration on reload. Home `/location` validates and publishes
without creating a root, and failed switches retain the original draft and
selection. Existing attached/headless switches retain their session contract.
`/new` returns to sessionless Home, but it does not retain the previous view as
a tab. Real PTY tests assert zero session/event/turn/message/binding before
submit, one bound root and accepted user message afterward, explicit-session
compatibility and Home A→B→A switches. The prior startup test first failed
because Home `/location` had been refused; the sessionless switch path fixed
that regression and the complete serialized suite subsequently passed.

## Independent paired diagnostic

Two immutable attempts: `recovery-v08-sessionless-home-01/` (before the
Home draft-reload review fixes) and `recovery-v08-sessionless-home-final/`
(after them). Both use the installed pinned original SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
the explicit Reader primary-profile fixture SHA-256
`d18132f88c4a7a638a244b0ea92007163246fc3ce0bbe5bcb2b90df02a67bb39`,
120×40 and the same terminal profile ID
`e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`.
Each side executed the fake Responses/read provider contract; independent
real SGR clicks expanded and recollapsed the read group on both sides. The
final capture locks native binary SHA-256
`90a05e93855c32000427b7864217e9c6464e5bdcef771a74729318071909bf8d`.
Fresh native Home and completed transcript showed no identified semantic
regression against the earlier explicit-profile capture: the changed native
cells between attempts were the isolated Location path and elapsed timer.

The final whole frames remain **DIFFERENT**: Home 157/4800 styled cells,
2043/647040 PNG pixels; completed Session 205/4800 cells, 4670/647040
pixels. The randomized Home example differs between the independently running
processes, and real app versions, elapsed time and absent native `+` also
differ. This paired capture does **not** prove database state on the original
side, tabs, exact timing or VIS completion. The real-binary native PTY/SQLite
tests supply the pre-submit durability proof; the independent images supply
the render comparison. No masks, fabricated Build/version or frozen timer.

## Gates and remaining work

`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace
--no-fail-fast --quiet` passed with zero failures (TUI 194; adapters runtime
56; existing opt-in live ignores). Workspace fmt, all-target Clippy with
`-D warnings`, locked build, `python3 scripts/check_docs.py`,
`python3 scripts/progress.py check` and `git diff --check` passed. The pinned
original/native capture runner returns 1 because the full frames differ.

Still missing: bounded retained tab deck, actual add/select mouse actions,
upstream tab persistence/restart and attention states, multi-size/dialog PTY
pairing and remaining VIS01–VIS24/V08–V09 acceptance. Do not label this slice
or T44 a VIS PASS. `.opencode/` was not touched.
