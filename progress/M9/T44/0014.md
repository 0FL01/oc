# T44 V07b in-flight MCP outcome and owner cleanup checkpoint

## Result

Base `9a3925f`, active T44. Raw-PTY Esc after actual remote/stdio `tools/call` now sends rmcp's request-ID cancellation instead of dropping an unresolved request. An unverified outcome (cancel, deadline, transport, malformed/unsupported post-effect result) is recorded `unknown`, stops continuation and retires the owner. Stdio child is reaped before an explicit retry; remote response/HTTP closure cannot prove server-side rollback, so subsequent remote calls are quarantined with safe `unsafe_retry` throughout the application lifetime, including reload, shutdown and Location switches. Explicit valid MCP `isError:true` remains a definitive failed result. Dropped futures mark durable turn/operation unknown. Independent review caught reload/shutdown, post-effect result, and Location replacement bypasses; all were reproduced and corrected. Actual fake-server PTY and runtime checks assert wire calls, SQLite status, provider requests and process cleanup, not just helpers.

## Checks

- Parent `cargo fmt --all -- --check && cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1 && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo build --locked && python3 scripts/check_docs.py && python3 scripts/progress.py check && git diff --check`: exit 0; five existing ignored tests. Final independent review: V07b runtime 2/2, raw-PTY application 10/10, durability 1/1, diff check all exit 0.
- Append-only `report.md` records initial failed cancellation, dropped/reload, unsupported response and Location overlap reproductions, earlier full-suite timeout/cursor flake and successful reruns. Remote fake retains first operation despite cancel/connection close; no external MCP or paid call.

## Risks

External remote effect may have completed despite cancellation. In-process quarantine is not persisted across process restart: durable `unknown` blocks automatic replay but a later human-requested call requires external reconciliation. No server-rollback claim. V07c other negative gates and measured RSS/PSS/CPU/frame-time are still open; `{file:}` substitution ancestor-race remains unqualified. All VIS/whole-frame parity gates remain open; owner's real-profile release-startup cause is not established.

## Next

V07c: qualify remaining S03–S08 with actual PTY/owner effects, test `{file:}` path swap without touching private config, and collect process/memory/render measurements after V06 Markdown. Then V08–V09 full paired VIS artifacts, comparator, independent review and fresh product qualification.
