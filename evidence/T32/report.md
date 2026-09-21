# T32 — apply_patch safety, grammar and preflight

Status: **PASS for F04/F05, AUD03–AUD05**. Code commit: `7636595`.
Audit baseline findings were rechecked at `e92b614`, not by resetting the repo.
This report does not qualify A01–A13 as a whole or close T33–T42.

## Reproduction and evidence

- [Red baseline](regression.md): three executed failures before production edits,
  including overwritten temporary outside sentinel. No valuable data involved.
- [Executed checks](checks.md): bounded commands/results, no claimed live PASS.
- Tests: `crates/oc-adapters/tests/patch_audit.rs`, patch unit tests,
  `patch/fs.rs` handle-race test, tools schema/result tests, direct DCP/TUI tests.

| Acceptance / finding | Executed regression and resulting behavior |
|---|---|
| AUD03 / F04 | Literal `+hello` becomes exactly `hello\n`; empty Add, Unicode, empty-file Update, CRLF and absent final newline pass original TOOL02/03 tests. Canonical schema/tool route accepts only `patchText`; TUI/protection consumers use it too. Pinned upstream Move/context/EOF fixtures assert independent expected bytes. Ambiguous hunks and duplicate/ancestor/target path overlaps fail before commits. |
| AUD04 / F05 | Preplanted old `.tmp-<pid>` symlink leaves sentinel intact. Existing symlink parent and parent replaced between validation/execution cannot escape. Separate pinned-handle test swaps parent after staging and still commits only in the original directory. Concurrent Add and Move targets retain exact competing bytes; move failure reports the already committed source Update. |
| AUD05 / F05 | Late stale hunk and late unwritable destination directory leave the first file absent. Concurrent changed preimage is preserved. Injected parent-to-file replacement causes a real ENOTDIR I/O failure after first commit: failure reports exact op/path/hash. Failed Move after Update retains exact before/after hashes and no false new_path. Model-facing partial output starts with `error:` and lists every committed record. |

All three initial failures became PASS without weakening their assertions.
The old TOOL04 expectation that a stale hunk leaves an earlier file committed
was explicitly superseded by T32; it now requires no commits, while a distinct
actual I/O failure test preserves the required partial-commit contract.

## Implementation boundary

- Parser and preparation remain in `oc-adapters::patch`. Every operation is
  prepared and validated before first execution; 2 MiB patch, 8 MiB file and
  64 MiB retained before/after plan caps prevent the new preflight path from
  retaining arbitrary aggregate content.
- Private `patch/fs.rs` owns Linux handle-relative syscalls using the existing
  libc dependency. No new package/dependency/public filesystem abstraction.
  All descendant directories use `openat` with DIRECTORY/NOFOLLOW; temporary
  file creation is exclusive/no-follow; Add/Move uses atomic NOREPLACE.
- Recheck identity/metadata/bytes immediately before Update/Delete; file sync
  follows content+mode, directory sync follows namespace changes. Results are
  recorded before a later sync/move can fail. No automatic rollback/replay.
- Descriptor, executor, protection lookup and direct fixtures aligned to one
  `patchText` field. Formal grammar is not inferred from natural-language text.
- Exact profile and limits documented in `docs/TOOLS_MCP.md`. Source fixture
  provenance is in `fixtures/patch.json`, with pinned OpenCode MIT notice.

## Verification

Final patch integration **10 passed**; final workspace tests successful,
including actual-binary AUD01 and all 13 PTY/restart tests inherited from T31.
fmt, workspace clippy `-D warnings`, locked build, actual `oc --help`, diff check
all exit **0**. Journal/docs checks exit **0** (structure only).
Three pre-existing external harnesses ignored/NOT RUN, not PASS. No new ignored
test, live endpoint, paid call, user credential access or push.

## Supported boundaries / remaining risk

Strict matching deliberately refuses ambiguous/fuzzy patches. Existing CRLF
and final-newline style is preserved; no arbitrary upstream shell wrapper
support is claimed. New files are restrictive 0600/umask; updates preserve
ordinary mode bits, not special privilege bits.

Preimage fingerprint/check is not a kernel compare-and-swap against a malicious
same-UID writer in the final syscall window, as the original tool contract
already states. Pinned directories prevent following replacement symlinks but
are not a sandbox against moving already-open directories elsewhere. Syscall
sync ordering was exercised/reviewed; no hardware power-loss experiment claimed.
A crash may leave an unreferenced staging file; the tool never replays it.
Partial failure is not all-files atomicity.

Product gaps in T33 durability/fail-closed storage and T34–T42 remain unresolved.
**NOT READY**, not `BUILD_READY_LIVE_BLOCKED`. Next: ready T33, reproduce
AUD06–AUD08 with temporary storage fault fixtures before fixing the owner.
