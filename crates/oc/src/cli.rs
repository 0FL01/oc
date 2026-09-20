//! Minimal CLI surface for T01.
//!
//! Full `run`/`sessions`/`tui` commands arrive in M1. This keeps `--help`
//! stable and offline.

use clap::Parser;

/// Native `oc` command line.
#[derive(Debug, Parser)]
#[command(name = "oc", version, about = "Native oc agent runtime (T01 smoke)")]
pub struct Args {
    /// Print linked crate versions and exit without side effects.
    #[arg(long, default_value_t = false)]
    pub smoke: bool,
}
