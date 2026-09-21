//! Minimal CLI surface for T05.
//!
//! `run`/`sessions` arrive here; full `tui`/`models`/`config` arrive in
//! M1/M2/M5. Bare `oc` on a terminal launches the same local TUI as
//! `oc tui`; without a terminal it is an actionable usage error.

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
    /// Subcommand; bare `oc` launches the local TUI when stdin/stdout are a
    /// terminal (otherwise it fails with a headless hint).
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
        /// Image input is not supported by this text-only application profile.
        #[arg(long, value_name = "URL")]
        image: Option<String>,
    },
    /// Inspect persisted sessions.
    Sessions {
        #[command(subcommand)]
        action: SessionsAction,
    },
    /// Launch the local interactive TUI.
    Tui {
        /// Session id; a fresh id is minted when absent.
        #[arg(long)]
        session: Option<String>,
    },
}

/// Session subcommands.
#[derive(Debug, Subcommand)]
pub enum SessionsAction {
    /// List known session ids.
    List,
}
