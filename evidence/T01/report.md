# T01 — Workspace и зависимости

Status: PASS. Implementation commit: `35c38e5d0c64a4c172972a4c02773a7d9c5b2dfb`. Method: actual-host Cargo execution, offline compile smoke with crates.io dependency resolution (no live OpenProxy/MCP, no paid requests, no Docker).

## BUILD01 — PASS

- `rustc --version --verbose`, `cargo --version` — exit 0. Actual `rustc/cargo 1.93.0`, host `x86_64-unknown-linux-gnu`, commit-hash `254b59607d4417e9dffbc307138ae5c86280fe4c`. Pinned via `rust-toolchain.toml` channel `1.93.0` with `rustfmt,clippy`, target `x86_64-unknown-linux-gnu`, profile `minimal`. This matches T00-recorded actual toolchain, not the earlier 1.98.1 candidate.
- Workspace: `Cargo.toml` resolver `3`, edition `2024`, `rust-version 1.93`, four members `crates/oc-core`, `crates/oc-adapters`, `crates/oc-tui`, `crates/oc`. Single binary target `oc` at `crates/oc/src/main.rs`; `target/debug/oc` produced.
- `Cargo.lock` generated (`cargo generate-lockfile` exit 0, 285 packages, sha256 `6a14dbc7f46bf3090629da440821a48daaf25478d3529c233b01fda63c0a529a`) and committed. `cargo build --locked` and `cargo build` both exit 0.
- `cargo fmt --all -- --check` — exit 0. `cargo clippy --workspace --all-targets -- -D warnings` — exit 0 (one intentional `#[allow(clippy::manual_async_fn)]` on `ProviderPort::smoke_complete` to keep explicit `Send` bound; `async_fn_in_trait` alternative rejected to preserve auto-trait bounds).
- `cargo test --workspace --locked` — exit 0, 13 tests: `oc-core` 6 (ids, serde, app, runtime tick, port error, name), `oc-adapters` 5 (sqlite roundtrip, reqwest offline build, rmcp `ProtocolVersion::V_2025_11_25` + `Tool` linkage, core app, tokio), `oc-tui` 2 (TestBackend frame, crossterm keycode). `cargo test -p oc-core/adapters/tui` individually exit 0 before full workspace run.
- `target/debug/oc --help` — exit 0, prints `Usage: oc [OPTIONS]` with `--smoke`, `--help`, `--version`. `target/debug/oc --smoke` — exit 0, prints `oc smoke core=oc-core adapter=oc-adapters tui=oc-tui`. No Node/Bun/upstream invocation; smoke paths do no network I/O (reqwest only builds request to `127.0.0.1:9`, rusqlite in-memory, rmcp type reference, TestBackend render).
- Dependency directions verified by `cargo tree -p <crate> --depth 1`: `oc-core` only `tokio,serde,serde_json,thiserror,anyhow` (no ratatui/rusqlite/reqwest/rmcp); `oc-adapters` adds `rusqlite,reqwest,rmcp` over `oc-core`; `oc-tui` adds `ratatui,crossterm` over `oc-core` (no adapters); `oc` depends on all three plus `clap`. Matches `docs/ARCHITECTURE.md`.
- Direct sources/licenses/features (crates.io, `cargo metadata`): `tokio 1.53.1 MIT`, `serde 1.0.229 MIT OR Apache-2.0 derive`, `serde_json 1.0.151 MIT OR Apache-2.0`, `thiserror 2.0.20 MIT OR Apache-2.0`, `anyhow 1.0.104 MIT OR Apache-2.0`, `clap 4.6.7 MIT OR Apache-2.0 derive/std/help/usage`, `ratatui 0.30.2 MIT crossterm/all-widgets (rust-version 1.88)`, `crossterm 0.29.0 MIT`, `rusqlite 0.40.2 MIT bundled`, `reqwest 0.13.5 MIT OR Apache-2.0 json/stream + default rustls (rust-version 1.85)`, `rmcp 3.4.0 Apache-2.0 client/transport-child-process (rust-version 1.88)`. All rust-versions ≤1.93. No ORM/DI/HTTP daemon: `Cargo.lock` contains zero `axum,sea-orm,diesel,sqlx,actix-web,warp,rocket`.
- Build scripts reviewed: expected only transitive crypto/SQLite links `aws-lc-sys/links=aws_lc_0_45_0`, `libsqlite3-sys/links=sqlite3`, `aws-lc-rs`, `ring`, `sqlite-wasm-rs`, `wasm-bindgen-shared` (via `cargo metadata links`). These come from `reqwest/rustls` and `rusqlite/bundled`; no unexpected build script introduces network/service behavior. No `ORM/DI/Axum` added.
- `git diff --cached --check` exit 0; staged secret scan for `LUDKA/OPENAI_API/sk-/ghp_/AKIA` — no hits; `python3 scripts/check_docs.py` exit 0.

## Scope and limitations

- T01 establishes compile smoke and dependency directions only. No M1 runtime, SQLite persistence, headless/TUI workflow, config/discovery, provider wire, tools, MCP lifecycle, DCP or soak behavior is claimed.
- `rmcp` smoke pins `ProtocolVersion::V_2025_11_25` and `Tool` type linkage; full remote/stdio adapters remain T20/T21. `reqwest` client build is offline; no HTTP request sent.
- License review covers direct dependencies only; full transitive/source/license audit remains T29 (BUILD02/BUILD03/OPS01/OPS04).
