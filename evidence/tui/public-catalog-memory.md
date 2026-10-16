# T44 — bounded public metadata prerequisite (2026-10-07)

Status: **RESOURCE_PREREQUISITE_QUALIFIED; T44 ACTIVE; VISUAL GATES OPEN**.
Git base: `0c4dc2402dc31c69db9f77afab83854376880e41`.
This receipt ships with the catalog wire-retention fix. Checks below identify the
current reviewed tree, including the separate uncommitted T44 presentation-facts
seam; they are not a clean-parent or whole-T44 qualification.

## Observed cause and change

The frozen actual-binary AUD32 guard exposed a startup allocation independent of
archive length: the credential-free public catalog retained its entire roughly
5.3 MiB wire body alongside transport buffers and the selected provider view.
Discarding foreign Value trees was already qualified in `75568ed2a`; that alone
did not bound these remaining buffers. Numeric-only temporary observations found
8 KiB maximum delivered chunks, not a multi-megabyte individual chunk.

The existing `DiscoveryClient` now has a default-compatible `get_chunks` method.
Its native implementation consumes successful chunks under the original attempt
deadline, checked 8 MiB byte cap, pinned DNS/peer admission, TLS verification,
no-proxy/no-redirect and no hidden retry. Non-2xx status is still classified before
consuming any error body. Existing whole-body callers keep `get`; its collector
uses bounded 256 KiB growth quanta rather than amortized doubling.

`models_dev/document.rs` frames one complete root member at a time across arbitrary
chunks. Serde remains the JSON validator; the existing public-document visitor
retains only Go/OpenAI and checks foreign JSON without retaining its Value tree.
Number/depth/UTF-8 validation and root/nested duplicate last-wins semantics remain.
Only a completely valid document reaches existing typed-record validation and
atomic cache publication. The public source, credential authority, TTL, single
flight, last-good data and failure classification are unchanged.

Metadata-only HTTP/2 stream/connection credit is 256 KiB with adaptation disabled,
so transport cannot independently grow its advertised receive windows while the
consumer validates a member. This is not a global generation/protocol change or
a claim that flow-control credit alone bounds process RSS. No dependency, worker,
store, artifact spool, provider route or fixture cache was added.

## Experiments versus qualification

Increasing exact reservation, dropping the whole body only after parsing, and
lowering HTTP/2 credit alone did not close the original guard. Incremental framing
alone also failed the deliberately held-response diagnostic. The combined owned
framing/transport bound passed that diagnostic; its largest member capacity was
512 KiB. These experiments informed the fix, not acceptance.

All observers, allocator hooks and the temporary one-second fake-peer hold were
removed before qualification. `crates/oc/tests/memory_bounds.rs` has no diff:
8 → 3,000 archived pairs × 16 KiB plus turn logs, two active pairs × 1 KiB,
2 ms sampling, 120-second deadline and the original 65,536 KiB growth guard remain.

Two independent clean normal-debug AUD32 runs passed:

| Run | Small peak HWM KiB | Large peak HWM KiB | Large peak anonymous/file KiB |
| --- | ---: | ---: | ---: |
| 1 | 56,420 | 58,500 | 14,268 / 44,232 |
| 2 | 54,992 | 58,456 | 14,260 / 44,196 |

Both had eight threads, zero children, large DB 152,944,640 bytes and zero ending
WAL bytes; equal active provider history and the real >100 MiB archive assertions
passed. Large peaks themselves are below the fixed guard, not merely growth from
an inflated baseline. The current full workspace also passed AUD32 unchanged.

## Current checks

Serial Cargo, normal stacks, build jobs 3/test threads 2, approved disk TMPDIR:

- `cargo fmt --all -- --check` — PASS.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --locked --workspace --no-fail-fast` — **1723/0/11**, independently
  summed from all 46 result records; unchanged opt-in ignores. Includes adapter
  library670, discovery19, models-dev11, runtime120, subagents39, TUI451,
  binary97, PTY T3952, MCP41 and recovery/startup4.
- `cargo build --locked`, `cargo build --locked --release`, ordinary `cargo build`
  and both binaries' `--help` — PASS.
- Actual release `support/startup.py` and `support/discovery_startup.py` — PASS:
  fatal/refusal/detail channels, trust/storage/Location boundaries, terminal
  restoration, exactly one authenticated discovery GET and zero Responses for
  discovery startup, including oversized/slow 401/403 bodies.
- Current actual debug/release TOOL21 chain — PASS on each: 13 main/5 auxiliary
  fake requests, one process effect, complete 3,740,084-byte artifact, actual
  registered read/grep and seven durable body/guidance facts; owned cleanup joined.
  These are prerequisite checks, not Generic/MCP rendering or full TOOL21 claims.
- Python operational/doc suites47, documentation/journal and `git diff --check`
  — PASS (structure, not visual evidence).

The current retained log `t44-public-catalog-stream-current2.log` ends
`T44_PUBLIC_CATALOG_STREAM_CURRENT_ALL_CARGO_GATES_PASS`. The first chain stopped
at Clippy's byte-slice spelling in a test; it was corrected without an allow or
weakened assertion. Full source was requalified, not resumed past the failed gate.

## Boundary

No acceptance threshold, workload, timeout, baseline, ignore, validation or
permission was weakened. Temporary diagnostic code is absent. `.opencode/`, user
configuration, AUTH06 deferral and original24/24 / Go13/24 live allowances remain
untouched; no paid generation was used. Next is the checked structured tool-body /
guidance / reference / capture seam and Generic/MCP VIS16/VIS17 consumption, then
all remaining frozen outcomes and final current-source R6/V09. No T44 finish,
paired pixel PASS or whole-product READY claim follows from this receipt.
