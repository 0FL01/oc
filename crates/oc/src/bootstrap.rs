//! Minimal bootstrap wiring all workspace crates.

use crate::cli::Args;
use anyhow::Result;

/// Run the smoke binary: touch each library crate without network/side effects.
pub fn run(args: Args) -> Result<()> {
    let app = oc_core::application::AppHandle::smoke();
    let _adapter = oc_adapters::adapter_name();
    let _tui = oc_tui::tui_name();

    // Exercise each adapter smoke path offline.
    let _ = oc_adapters::smoke_memory_db()?;
    let _ = oc_adapters::build_smoke_client()?;
    let _ = oc_adapters::rmcp_smoke_marker();
    let _frame = oc_tui::render_smoke_frame(&app);

    if args.smoke {
        println!(
            "oc smoke core={} adapter={} tui={}",
            oc_core::core_name(),
            _adapter,
            _tui
        );
    } else {
        println!("oc T01 smoke: use --help for usage, --smoke for linked crates");
    }
    Ok(())
}
