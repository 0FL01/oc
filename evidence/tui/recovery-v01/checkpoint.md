# T44 recovery V01 checkpoint

## Result

Based on `679e683121722c0a00aac7acd95f2be91190ad9e`; no reset. Ordinary prompt
and manual `/dcp-compress` enqueue bounded acceptance receipts, retain edited
drafts, refuse duplicate Enter and process pending Esc/resize. Application remains
the turn/durability owner. Required MCP failures stay fatal to the turn and carry
safe stage/code/retryability. Anonymous remote no longer inherits codex bearer
requirements; explicit codex_web profile retains its contract.

Independent review exposed remaining synchronous compression and incomplete SDK
cleanup ownership. Follow-up fixes/tests are in report.md: stdio Child is now owned
through actual reap; HTTP owner closure and protocol mismatch cleanup are observed.
Native turn IDs are process-unique even under clock rollback; generation reset
invalidates pending state. Four crates/Rust2024/no production JS preserved.

## Checks

- Parent `cargo test --locked -p oc --test mcp_application v01_ -- --nocapture`:
  exit 0, 5 passed. Pending prompt cancellation/reap 21.702332 ms; compression
  38.26233 ms, both before fake release within 2 s. Durable counts and retry checked.
- Parent `cargo fmt --all -- --check` and `git diff --check`: each exit 0.
- Implementation checks and immutable earlier failures: report.md. Follow-up scope
  344 passed, 0 failed, 2 existing live ignores; clippy/build passed.
- Full final workspace/visual/live qualification NOT_RUN on this slice.

## Risks

Pre-existing in-flight MCP tools/call cancellation drops the caller future without
proving remote operation cancellation; tracked for V07/S08, not declared closed.
Full S06/live-replay and VIS01–VIS24 remain unverified. Actual user crw configuration
was not accessed; no cause guessed. Existing docs registry failure remains open.

## Next

V02: project safe real title/model/usage/history parts through existing application
and durable records; fixture mutation and restart checks. Then ordered V03–V09,
paired capture freeze/comparison/review and fresh qualification. Parent reviewed
production diff and HTTP lifecycle source. User ZIP is intentionally not staged.
