//! Reuse the native PTY suite's bounded text-screen decoder. A cell diff may
//! repaint only a suffix, so raw byte adjacency is not a rendered-text proof.
#[path = "../../../../crates/oc/tests/support/screen.rs"]
mod screen;

use std::io::Read;

fn main() -> std::io::Result<()> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(32 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    let screen = screen::render_screen(&bytes);
    let _ = screen.cursor;
    println!("{}", screen.rows().join("\n"));
    Ok(())
}
