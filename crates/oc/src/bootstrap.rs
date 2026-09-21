//! Dispatch for the `oc` binary.

use std::io::{stderr, stdout};
use std::process::ExitCode;

use crate::cli::{Args, Command, SessionsAction};
use crate::headless;

/// Run the parsed CLI; never mixes diagnostics into stdout payloads.
pub async fn run(args: Args) -> ExitCode {
    if args.smoke && args.command.is_none() {
        return legacy_smoke();
    }
    let data_dir = match args
        .data_dir
        .clone()
        .map(Ok)
        .unwrap_or_else(headless::default_data_dir)
    {
        Ok(path) => path,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(1);
        }
    };
    match args.command {
        None => {
            eprintln!("usage: oc [--data-dir PATH] <run|sessions> | oc --smoke");
            ExitCode::from(2)
        }
        Some(Command::Run {
            prompt,
            session,
            json,
        }) => {
            let mut out = stdout().lock();
            let mut err = stderr().lock();
            headless::run_once_to_writers(prompt, session, json, &data_dir, &mut out, &mut err)
                .await
        }
        Some(Command::Sessions { action }) => match action {
            SessionsAction::List => {
                let mut out = stdout().lock();
                let mut err = stderr().lock();
                headless::list_to_writers(&data_dir, &mut out, &mut err)
            }
        },
        Some(Command::Tui { session }) => crate::tui_cmd::run_tui(&data_dir, session).await,
    }
}

fn legacy_smoke() -> ExitCode {
    let app = oc_core::application::AppHandle::smoke();
    let _ = oc_adapters::smoke_memory_db().unwrap_or(0);
    let _ = oc_adapters::build_smoke_client();
    let _ = oc_adapters::rmcp_smoke_marker();
    let _ = oc_tui::render_smoke_frame(&app);
    println!(
        "oc smoke core={} adapter={} tui={}",
        oc_core::core_name(),
        oc_adapters::adapter_name(),
        oc_tui::tui_name()
    );
    ExitCode::SUCCESS
}
