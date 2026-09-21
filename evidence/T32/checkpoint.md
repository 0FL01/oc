## Result

T32 verified implementation slice on base e92b614. Reproduced F04/F05 first:
all three independent regressions failed before production edits. Canonical
patchText/schema/consumers aligned; Add bytes and Move order corrected. All
hunks/paths/policy/filesystem permissions preflight before writes. Linux
directory-relative no-follow I/O, exclusive new temp inode, no-replace Add/Move,
preimage recheck and file/directory sync replace pathname-based mutation.
Partial outcomes retain every committed file and full hashes, including Update
before failed Move; tool result begins with error, never false success.

## Checks

Final patch_audit 10/10; workspace tests pass including pinned-parent race,
schema/tool route, DCP/TUI consumers, actual binary AUD01 and 13 PTY/restart tests.
fmt, workspace clippy -D warnings, locked build, oc --help and diff check exit 0.
Three existing external harnesses NOT RUN, not PASS. Exact bounded results:
evidence/T32/checks.md; initial red: evidence/T32/regression.md.

## Risks

T33 durability/permission storage failures and other repairs remain product gaps;
NOT READY and not BUILD_READY_LIVE_BLOCKED. Preimage check is not kernel CAS
against a malicious same-UID writer in the final syscall window; directory
handles do not constitute a sandbox. No all-files transaction/rollback; crash
may leave staging file but must never auto-replay. No live/paid calls or push.

## Next

Commit verified implementation, write evidence/T32/report.md with code SHA,
finish T32 through existing journal after docs checks. Then start ready T33 and
reproduce AUD06–08 storage/durable-intent failures on temporary fixtures only.
Changed runtime: patch.rs/private patch/fs.rs, runtime/tools schema, direct
DCP/TUI consumers/fixtures; no dependency or package boundary change.
