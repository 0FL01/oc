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
    // Authentication uses the credential/attempt owner directly: no selected
    // model, configuration generation, MCP connection or session is required.
    if let Some(Command::Auth { action }) = args.command {
        let data = args
            .data_dir
            .map(Ok)
            .unwrap_or_else(headless::default_data_dir);
        return match data {
            Ok(data) => crate::auth_cmd::run(action, &data).await,
            Err(_) => {
                eprintln!("error: authentication data directory unavailable");
                ExitCode::from(1)
            }
        };
    }
    // The catalog command is read-only even under a fresh HOME. The ordinary
    // startup trace creates/truncates files and belongs to application startup.
    if !matches!(args.command, Some(Command::Models)) {
        oc_adapters::trace::init_default();
    }
    oc_adapters::trace::log(
        "startup.begin",
        &format!(
            "bin=oc args={} subcommand={}",
            std::env::args().count(),
            subcommand_kind(&args.command)
        ),
    );
    match std::env::current_dir() {
        Ok(_) => oc_adapters::trace::log("startup.cwd", "available=true"),
        Err(_) => oc_adapters::trace::log("startup.cwd", "available=false"),
    }
    {
        use std::io::IsTerminal as _;
        oc_adapters::trace::log(
            "startup.tty",
            &format!(
                "stdin={} stdout={}",
                std::io::stdin().is_terminal(),
                std::io::stdout().is_terminal()
            ),
        );
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
    if matches!(args.command, Some(Command::Models)) {
        let data = args
            .data_dir
            .clone()
            .or_else(|| headless::default_data_dir().ok());
        return crate::models_cmd::run(data.as_deref()).await;
    }
    let data_dir = match args
        .data_dir
        .clone()
        .map(Ok)
        .unwrap_or_else(headless::default_data_dir)
    {
        Ok(path) => {
            oc_adapters::trace::log("startup.data_dir", "configured=true");
            path
        }
        Err(error) => {
            oc_adapters::trace::log("startup.data_dir", &format!("error={error}"));
            eprintln!("error: {error}");
            return ExitCode::from(1);
        }
    };
    match args.command {
        Some(Command::Auth { .. }) => unreachable!("auth-only dispatch precedes runtime"),
        Some(Command::Models) => unreachable!("catalog-only dispatch precedes storage"),
        None => crate::tui_cmd::run_tui(&data_dir, None, args.auto).await,
        Some(Command::Run {
            prompt,
            session,
            agent,
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
            headless::run_once_to_writers(
                prompt, session, agent, json, args.auto, &data_dir, &mut out, &mut err,
            )
            .await
        }
        Some(Command::Sessions { action }) => match action {
            SessionsAction::List => {
                let mut out = stdout().lock();
                let mut err = stderr().lock();
                headless::list_to_writers(&data_dir, &mut out, &mut err)
            }
        },
        Some(Command::Tui { session }) => {
            crate::tui_cmd::run_tui(&data_dir, session, args.auto).await
        }
    }
}

fn subcommand_kind(command: &Option<Command>) -> &'static str {
    match command {
        None => "none",
        Some(Command::Run { .. }) => "run",
        Some(Command::Sessions { .. }) => "sessions",
        Some(Command::Tui { .. }) => "tui",
        Some(Command::Models) => "models",
        Some(Command::Auth { .. }) => "auth",
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
