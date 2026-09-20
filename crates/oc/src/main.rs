//! `oc` binary entrypoint: thin wiring over core/adapters/tui.
//!
//! The full headless/TUI runtime arrives in M1 (T05/T06). For T01 this only
//! proves the binary links all three library crates, parses `--help` via
//! Clap, and can run offline without Node/Bun/upstream.

mod bootstrap;
mod cli;

use anyhow::Result;
use clap::Parser as _;

fn main() -> Result<()> {
    let args = cli::Args::parse();
    bootstrap::run(args)
}
