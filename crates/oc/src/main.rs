//! Binary wiring: legacy `--smoke`, headless `run`, `sessions list`.
//!
//! Stdout carries only the answer (text or NDJSON); diagnostics go to
//! stderr. Exit codes: 0 success, 1 error/usage, 130 interrupted.

mod bootstrap;
mod cli;
mod headless;
mod tui_cmd;

use clap::Parser as _;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args = cli::Args::parse();
    bootstrap::run(args).await
}
