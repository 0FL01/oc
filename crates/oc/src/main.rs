//! Binary wiring: legacy `--smoke`, headless `run`, `sessions list`.
//!
//! Stdout carries only the answer (text or NDJSON); diagnostics go to
//! stderr. Exit codes: 0 success, 1 error/usage, 130 interrupted.

mod auth_cmd;
mod bootstrap;
mod cli;
mod clipboard;
mod headless;
mod models_cmd;
mod tui_cmd;

use clap::Parser as _;

fn main() -> std::process::ExitCode {
    let args = cli::Args::parse();
    if matches!(
        args.command,
        Some(cli::Command::NativeOwner { .. } | cli::Command::Run { .. })
    ) {
        // Before any worker thread exists, so every arena honours the limit.
        oc_adapters::memory::tune_owner_allocator();
    }
    // A small fixed pool keeps the native host responsive without scaling one
    // process's resident stack footprint with the machine's CPU count.
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(bootstrap::run(args))
}
