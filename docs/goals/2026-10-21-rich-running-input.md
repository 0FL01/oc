# Rich running input

Status: complete
Source: user request to continue steer/queue, support images and slash commands and busy `!shell`, then verify, commit, push and build; approved conversation plan.
Last updated: 2026-10-21

## Frozen finish line

- R1: Rich chat prompts and executable native/workspace commands support steer and queue, preserving command routing and image restoration. Evidence: actual-binary `running_rich_inbox_and_direct_shell_cross_actual_native_ipc_without_losing_media` and application command-routing regressions passed. Status: verified.
- R2: Direct user Shell admits during a model run, including Ask, isolated request cancellation and root cleanup. Evidence: application Ask/model-settlement and native receipt-versus-root cancellation regressions passed. Status: verified.
- R3: Durable pending input is never removed before acceptance; reattach, RAW restoration, conversation boundaries and fork preserve it without replay. Evidence: transaction rollback, two-image RAW retirement/reopen/fork, immutable physical retry, source-reopen and actual-binary parked-queue promotion regressions passed. Status: verified.
- R4: Relevant tests and full offline gate green, release binary built, scoped changes committed and pushed to verified origin. Evidence: final gate/build below; source commit `6a938daef526188d193a11ffeb64ef3ab827169c` pushed to `origin/master`, with the same SHA confirmed by `git ls-remote`. Status: verified.

## Boundaries

One native UID/data-root authority/Db; captured requests and full issued batches remain immutable; no unknown-effect replay. Preserve the pre-existing dirty steer/queue work and unrelated AGENTS edits. No user owner restart, interactive live client, paid provider calls or campaign reset.

Shell remains a direct operation, not an inbox item. Local UI/lifecycle slash actions remain local. Command+image combinations retain the existing idle restriction. Commands expand once at enqueue; stale command generations fail visibly and remain restorable. No startup queue scan/auto-run; explicit promotion may start an idle root.

## Change envelope

Core inbox DTO/API; native wire/routing/version; existing application/runtime inbox, command and Shell owners; storage inbox/acceptance/checkpoint/image/history/fork consumers; TUI composer/inbox/Shell and binary projection; matching tests; CONTRACTS/DECISIONS. No new dependency, service, generic scheduler or second storage owner.

## Execution

Complete the frozen outcomes inside this envelope. Work on the smallest unresolved outcome; evidence does not create new requirements. Finish after current checks and affected constraints pass. Record a concrete external dependency only if no permitted in-scope action can progress an outcome.

## Verified result

Implementation covers rich bounded storage/atomic delivery, actor-owned command boundaries, distinct background command operations, side-Shell admission, image history/fork metadata and rich composer restoration. Initial HEAD `29c86bbd1`; 35 tracked dirty files and four new inbox files inherited and preserved.

Current evidence: final `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4 TMPDIR=/home/stfu/.cache/opencode-tmp/opencode bash scripts/test.sh` passed: fmt, strict workspace/all-targets clippy, 2239 nextest passes with twelve existing skips (396.815s), doctests, debug build/help/version. Final release build and release `--help`/`--version` passed; binary is `target/release/oc2`, SHA-256 `88173cba2d69435faa8fa91a622be05a3eb3cc1b4c80c13e93b9d6dc735f315d`. PTY fixtures follow the actual contracts: title requests are counted separately, busy input steers rather than rejects, projectless startup uses protocol 4, idle Esc preserves drafts, and reload/navigation wait for completed operation state rather than a retained toast. Logs remain ignored under `target/rich-running-input-*`.

Command provenance includes the command catalog and routing-profile digest because numeric generation IDs restart at 1. The stale-source fixture reloads through the actual Location owner, not a root follower. Soak diagnosis: isolated baseline 31.95s; removing an unnecessary journal clone alone still took 30.27s and was reverted. Syscall wall-time profiling identified 2316 durable fsync calls taking 20.97s, not a stalled provider or cancellation. The minimal envelope expansion is the existing storage compaction/turn checkpoint owner: a completed generation's usage anchor and canonical journal now commit together, before tool effects, rather than in two adjacent FULL-durable transactions. The rollback/reopen regression passed; the frozen soak passed in 27.35s alone and 26.937s in the final full gate without load/guard changes. No live provider/user-owner actions or known external blocker.

## Completion

- R1–R4 verified. Source and tests committed as `6a938daef526188d193a11ffeb64ef3ab827169c`; push and remote SHA verified against `git@github.com:0FL01/oc.git`, branch `master`.
- Final commands: full `bash scripts/test.sh`; `cargo build -p oc2 --bin oc2 --locked --release`; release `--help` and `--version`; working/staged diff checks. Build/test commands used `CARGO_INCREMENTAL=0`, four Cargo build jobs and the approved disk TMPDIR.
- Closure checked 59 scoped paths, unchanged workload/security/durability guards and all test durations. Only the existing UI07 exception exceeded 30s (32.537s, within 40s). Unrelated inherited `AGENTS.md` changes were excluded and preserved.
- Storage has an explicit forward inbox schema upgrade; native wire version is 4. No second owner, service or scheduler, no startup queue execution, no user-owner restart and no live/paid generation. Offline success does not claim whole-product live/visual qualification.
- Final status: complete.
