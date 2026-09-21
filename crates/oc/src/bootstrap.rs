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
    // Bare `oc` is the local TUI, but only on a real terminal: a redirected
    // invocation is not silently turned into a hidden headless/daemon run.
    if args.command.is_none() && !crate::tui_cmd::interactive_ready() {
        eprintln!(
            "error: bare `oc` needs an interactive terminal (stdin/stdout are not a TTY); \
             use `oc run \"<prompt>\"` for headless use"
        );
        return ExitCode::from(2);
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
        None => crate::tui_cmd::run_tui(&data_dir, None).await,
        Some(Command::Run {
            prompt,
            session,
            json,
            image,
        }) => {
            if image.is_some() {
                eprintln!(
                    "error: unsupported modality: image input; this application profile accepts text only"
                );
                return ExitCode::from(2);
            }
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
