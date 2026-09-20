//! Minimal CLI surface for T05.
//!
//! `run`/`sessions` arrive here; full `tui`/`models`/`config` arrive in
//! M1/M2/M5. Bare `oc` without subcommand or `--smoke` is a usage error.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Native `oc` command line.
#[derive(Debug, Parser)]
#[command(name = "oc", version, about = "Native oc agent runtime")]
pub struct Args {
    /// Isolate storage under an explicit data directory.
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,
    /// Print linked crate versions and exit without side effects.
    #[arg(long, default_value_t = false)]
    pub smoke: bool,
    /// Subcommand; required unless `--smoke` is given.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Headless and session commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run one headless prompt through the local runtime.
    Run {
        /// User prompt text.
        prompt: String,
        /// Session id; a fresh `s-<nanos>` id is minted when absent.
        #[arg(long)]
        session: Option<String>,
        /// Emit NDJSON events on stdout (diagnostics stay on stderr).
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Inspect persisted sessions.
    Sessions {
        #[command(subcommand)]
        action: SessionsAction,
    },
}

/// Session subcommands.
#[derive(Debug, Subcommand)]
pub enum SessionsAction {
    /// List known session ids.
    List,
}
