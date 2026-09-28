// T52 only: deterministic in-memory render regression, NOT actual-PTY/parity.
// page() is the unchanged existing tui_render_alloc fixture.
use oc_core::core_app::{CoreApp, MockProvider};
use oc_core::domain::SessionId;
use oc_core::queries::{HistoryMessage, HistoryPage, HistoryTurn, ToolOpView, TranscriptPart};
use oc_core::session::Role;
use oc_tui::app::{TabPresentation, TuiState};
use ratatui::{Terminal, backend::{Backend, TestBackend}, style::{Color, Modifier}};
use serde_json::json;
use unicode_width::UnicodeWidthStr;

fn page(older: usize) -> HistoryPage {
    let table = (0..24)
        .map(|i| format!("| row {i:02} 界 | `value-{i}` | note {i} |\n"))
        .collect::<String>();
    let markdown = format!(
        "### Wide table\n| col A | col B | col C |\n| --- | --- | --- |\n{table}\n```rust\nfn unfinished() {{\n    let x = 42;\n"
    );
    // The real storage projection serves at most 2,048 output bytes. Model a
    // longer result through its bounded preview plus full-size metadata; no
    // 128 KiB result is materialized by this renderer fixture.
    let tool_preview = "result line: 0123456789 abcdefghijklmnop\n"
        .repeat(100)
        .chars()
        .take(2_048)
        .collect::<String>();
    let rows = (1..=200)
        .map(|seq| {
            let role = if seq % 2 == 0 {
                Role::Assistant
            } else {
                Role::User
            };
            let text = if seq == 200 {
                markdown.clone()
            } else {
                format!("current-{seq:03}: rendered message with Unicode 漢字")
            };
            let turn = (seq == 200).then(|| HistoryTurn {
                id: "render-stress-turn".into(),
                status: "completed".into(),
                model_label: "fixture".into(),
                parts: vec![
                    TranscriptPart::Text(markdown.clone()),
                    TranscriptPart::Tool(ToolOpView {
                        op: "render-stress-op".into(),
                        rowid: 1,
                        name: "bash".into(),
                        state: "completed".into(),
                        input: Some(r#"{"command":"generate output"}"#.into()),
                        output: Some(tool_preview.clone()),
                        output_bytes: 128 * 1024,
                        output_truncated: true,
                        patch_effects: None,
                        dcp: None,
                        dcp_topic: None,
                    }),
                ],
                ..HistoryTurn::default()
            });
            HistoryMessage {
                id: oc_core::session::MessageId(format!(
                    "fixture-{:08}",
                    i64::from(seq) + older as i64
                )),
                seq: i64::from(seq) + older as i64,
                role,
                text,
                turn,
                model_switch: None,
            }
        })
        .collect();
    HistoryPage {
        rows,
        total: 200 + older,
        has_older: older > 0,
        has_newer: false,
        ..HistoryPage::default()
    }
}

fn sgr_color(color: Color, background: bool) -> String {
    let channel = if background { 48 } else { 38 };
    match color {
        Color::Rgb(r, g, b) => format!("{channel};2;{r};{g};{b}"),
        Color::Indexed(n) => format!("{channel};5;{n}"),
        Color::Reset => if background { "49" } else { "39" }.into(),
        c => {
            let n = match c {
                Color::Black => 0, Color::Red => 1, Color::Green => 2, Color::Yellow => 3,
                Color::Blue => 4, Color::Magenta => 5, Color::Cyan => 6, Color::Gray => 7,
                Color::DarkGray => 8, Color::LightRed => 9, Color::LightGreen => 10,
                Color::LightYellow => 11, Color::LightBlue => 12, Color::LightMagenta => 13,
                Color::LightCyan => 14, Color::White => 15, _ => unreachable!(),
            };
            format!("{channel};5;{n}")
        }
    }
}

async fn render() {
    let (app, _guard) = CoreApp::spawn(MockProvider::echo());
    let id = SessionId::new("s-render-alloc").unwrap();
    let mut state = TuiState::new(app, id.clone());
    state.attach_page(&page(3000));
    state.chrome.animations = Some(false);
    state.set_tab_strip(vec![TabPresentation::new(id)], 0, true);
    let mut frames = Vec::new();
    for (width, height) in [(80, 24), (120, 40), (160, 48)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        for _ in 0..4 {
            terminal.draw(|frame| oc_tui::views::render_frame(frame, &state)).unwrap();
        }
        let cursor = terminal.backend_mut().get_cursor_position().unwrap();
        let buffer = terminal.backend().buffer();
        let mut cells = Vec::new();
        let mut vt = String::from("\x1b[2J\x1b[H");
        for y in 0..height {
            let mut row = Vec::new();
            let mut continuation_until = 0;
            for x in 0..width {
                let cell = &buffer[(x, y)];
                row.push(json!({"symbol":cell.symbol(),"fg":format!("{:?}",cell.fg),
                    "bg":format!("{:?}",cell.bg),"underline":format!("{:?}",cell.underline_color),
                    "modifier":cell.modifier.bits(),"skip":cell.skip}));
                if x < continuation_until { continue; }
                continuation_until = x.saturating_add(cell.symbol().width() as u16);
                vt += &format!("\x1b[{};{}H\x1b[0;{};{}", y + 1, x + 1,
                    sgr_color(cell.fg, false), sgr_color(cell.bg, true));
                for (flag, code) in [(Modifier::BOLD,1),(Modifier::DIM,2),(Modifier::ITALIC,3),
                    (Modifier::UNDERLINED,4),(Modifier::SLOW_BLINK,5),(Modifier::RAPID_BLINK,6),
                    (Modifier::REVERSED,7),(Modifier::HIDDEN,8),(Modifier::CROSSED_OUT,9)] {
                    if cell.modifier.contains(flag) { vt += &format!(";{code}"); }
                }
                vt += "m";
                vt += cell.symbol();
            }
            cells.push(row);
        }
        vt += &format!("\x1b[0m\x1b[{};{}H\x1b[?25h", cursor.y + 1, cursor.x + 1);
        frames.push(json!({"columns":width,"rows":height,"cells":cells,
            "cursor":{"x":cursor.x,"y":cursor.y},"vt":vt}));
    }
    println!("{}", serde_json::to_string(&frames).unwrap());
}

fn main() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(render());
}
