# T42 current qualification fixes — AUD38 / AUD32

Base: `c6e08c4bfe445c9c6d6de624aa6a820fca559ec7`. Task resumed for the explicit
T30 requirement to requalify AUD38/AUD39 after the backend changes. This is a
checked implementation checkpoint, **not completed AUD38 or product READY**;
the full fixed-commit workspace/debug/release qualification follows this slice.

## Reproductions and minimal fixes

1. A fresh owned HOME/XDG and Node/Bun/npx-free PATH passed the three adapter
   `e2e_offline` cases but exposed `SessionNotFound` in the actual-binary golden
   workflow. `Fixture::spawn` applied the parent environment **after** its owned
   HOME/XDG fields. Moving that existing inheritance before the explicit fixture
   fields preserves dev-tool access while keeping the fixture's data authority.
   No golden assertions or production environment policy changed.
2. The first full current workspace run produced **1712 passed / 1 failed /
   11 unchanged opt-in ignored**. The sole failure was actual-binary AUD32:
   3000 archived pairs, equal active context, 152944640-byte DB, large peak RSS
   125732 KiB, growth 125236 KiB above the unchanged 65536 KiB bound.
3. Additional peak composition exposed an anonymous-heap spike rather than an
   executable-file or worker-count effect: standalone large peak 125984 KiB,
   anonymous 80152 KiB, file 45832 KiB, eight threads. The SQL full-archive scan
   hypothesis was **not established** and no index/schema/worker change was made.
4. A temporary malloc interposer in the approved diagnostic cache captured the
   failing native ELF's allocation stacks, without response bodies or credentials.
   The 1/2/4/8 MiB buffer growth was `ReqwestDiscoveryClient::get` →
   `GoCatalog::refresh_inner` → application `provider_catalog::refresh_public`.
   The subsequent 32 MiB live-heap threshold was recursive `serde_json::Value`
   deserialization. This is the background **public models.dev metadata fetch**,
   not an authenticated provider generation or archive read. Its asynchronous
   completion explains why unchanged standalone runs also passed. Interposition
   is removed from the final test and is not a product dependency.
5. `models_dev::PublicDocument` now retains only the Go and OpenAI slices, then
   moves (rather than clones) them into the unchanged typed validation. Foreign
   values are traversed without a retained tree. A finite checked-discard visitor
   preserves the original JSON syntax, recursion and number-range checks; an
   optimized ignored-any skip would relax some of those checks. Recognized
   duplicate keys remain last-key-wins, including inside each Value slice.

The sole public source, 8 MiB wire cap, deadlines, two-slice validation, row/label
limits, same normalized cache schema, last-good/single-flight semantics, current
local overrides and Go/OpenAI consumers are unchanged. Unknown providers and
remote connection/credential fields do not enter the persisted cache. No new
dependency, provider routing, store, migration or auth behavior was introduced.

## Executed checkpoint checks

All commands use the pinned Rust 1.93.0, normal stacks, serial Cargo,
`CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2` and the approved TMPDIR.

- `cargo test --locked -p oc-adapters --lib models_dev`: **9/0/0**, including the
  large foreign-tree and JSON/duplicate conservation regressions and the existing
  shared two-provider, invalid-row, retirement, cache secrecy and single-flight cases.
- `cargo build --locked -p oc`: PASS; actual current debug ELF rebuilt.
- `cargo test --locked -p oc --test memory_bounds -- --nocapture`: **1/0/0**;
  then five independent executions of that current test target: **5/0/0**.
  Large peaks 57780 then 58616/56796/58744/57700/56484 KiB; anonymous peaks
  13480 then 13592/12100/13596/12676/12104 KiB; real large DB 152944640 bytes,
  equal active provider history and zero child processes throughout.
- `cargo test --locked -p oc --test golden_binary`: **1/0/0**.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: PASS.
- `cargo fmt --all -- --check` and `git diff --check`: PASS.
- `python3 evidence/T42/current_offline.py target/debug/oc`: fresh owned
  HOME/XDG/CARGO_HOME, explicit Node/Bun/npx-free PATH, native ELF help/smoke,
  **3 adapter offline cases + 1 actual debug-ELF golden workflow** PASS.
  `rustdoc` is an explicit Rust dev tool on that PATH, not a runtime JS dependency.

The diagnostic fields do not change the frozen memory workload, first-RSS
baseline, kernel HWM, sampling interval, deadline, process assertions or 64 MiB
threshold. Public catalog metadata work remains admitted; it was not disabled to
hide the peak. Script `AUD38_no_required_node` labels only the executed sub-gate.
No model/live-auth campaign was started and no existing live ledger was reset.

## Next and boundaries

Run the full mandatory qualification on the resulting fixed code commit, then
write the current F01–F18 matrix/report and finish only the scoped T42 outcomes.
Old reports remain historical. T44 visual qualification stays PAUSED; T57 AUTH06
is deferred; the original T27/T45 allowance blocker remains unchanged. No fake
peer or these public metadata reads qualify the mandatory real-provider gates.
