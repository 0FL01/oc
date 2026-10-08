use super::*;
use crate::app::{HOME_EXAMPLES, TabPresentation};
use crate::events::KeyAction;
use oc_core::core_app::{CoreApp, MockProvider};
use oc_core::domain::SessionId;
use oc_core::queries::{
    AgentEntry, CatalogSnapshot, HistoryMessage, HistoryPage, ModelEntry, TabIndicators,
    VariantEntry,
};
use oc_core::session::Role;
use ratatui::{Terminal, backend::TestBackend, style::Color};

mod home_mcp;

fn msg(seq: i64, role: Role, text: &str) -> HistoryMessage {
    HistoryMessage {
        id: oc_core::session::MessageId(format!("fixture-{seq}")),
        turn: None,
        model_switch: None,
        seq,
        role,
        text: text.to_string(),
    }
}

fn page(rows: Vec<HistoryMessage>) -> HistoryPage {
    let total = rows.len();
    HistoryPage {
        parent_id: None,
        title: None,
        reverted: None,
        rows,
        total,
        has_older: false,
        has_newer: false,
    }
}

fn catalog() -> CatalogSnapshot {
    CatalogSnapshot {
        chrome: oc_core::queries::TuiChrome {
            devtools: Some(true),
            ..Default::default()
        },
        auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
        provider: "ludka2".to_string(),
        models: vec![ModelEntry {
            display_name: String::new(),
            provider_name: String::new(),
            price: None,
            id: "a".to_string(),
            variants: vec![VariantEntry {
                name: "low".to_string(),
                disabled: false,
                reasoning_effort: Some("low".to_string()),
            }],
            context: 1000,
            context_known: true,
            output_known: true,
            output: 100,
        }],
        model_id: "a".to_string(),
        // Geometry goldens include a variant label only because it is selected.
        variant: Some("low".to_string()),
        agents: vec![AgentEntry {
            color_index: 0,
            id: "x".to_string(),
            description: "first profile".to_string(),
            model: Some("a".to_string()),
            variant: None,
        }],
        agent_id: Some("x".to_string()),
        commands: Vec::new(),
        command_descriptions: Default::default(),
    }
}

async fn golden_state() -> TuiState {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    std::mem::forget(guard);
    let id = SessionId::new("s-golden").expect("id");
    app.create_session(id.clone()).await.expect("create");
    let mut state = TuiState::new(app, id);
    state.apply_catalog(catalog());
    state.attach_page(&page(vec![
        msg(1, Role::User, "hello"),
        msg(2, Role::Assistant, "hi there"),
    ]));
    state
}

#[tokio::test]
async fn vis11_pending_only_changes_composer_roles_and_restores_on_deadline() {
    let mut state = golden_state().await;
    state.handle_paste("unsent draft");
    state.handle_paste("first\nsecond\nthird");
    let theme = Theme::dark();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let normal = terminal.backend().buffer().clone();
    let metadata = metadata_line(&state, theme, 111, 120).unwrap();
    let regions = shell_regions(&state, Rect::new(0, 0, 120, 40));
    let regions = session_regions(&state, session_main(&state, regions.session), 40);
    state.handle_key(KeyAction::Leader).await;
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let pending = terminal.backend().buffer();
    let chip_cells: Vec<_> = normal
        .content
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.bg == theme.warning())
        .map(|(index, _)| index)
        .collect();
    assert!(
        !chip_cells.is_empty(),
        "actual paste chip has an independent background"
    );
    for index in chip_cells {
        assert_eq!(normal.content[index].fg, theme.background());
        assert!(normal.content[index].modifier.contains(Modifier::BOLD));
        assert_eq!(normal.content[index], pending.content[index]);
    }
    for y in 0..40 {
        for x in 0..120 {
            assert_eq!(normal[(x, y)].symbol(), pending[(x, y)].symbol());
            if y < regions.prompt.y || y > regions.underline.y {
                assert_eq!(normal[(x, y)], pending[(x, y)], "outside composer {x},{y}");
            }
        }
    }
    let muted = metadata_line(&state, theme, 111, 120).unwrap();
    assert_eq!(metadata.to_string(), muted.to_string());
    for (before, after) in metadata.spans.iter().zip(&muted.spans) {
        let expected = match before.content.as_ref() {
            "X" => theme.border(),
            "a" => theme.text_muted(),
            _ => before.style.fg.unwrap(),
        };
        assert_eq!(after.style.fg, Some(expected));
    }
    let body = regions.prompt;
    assert_eq!(pending[(body.x, body.y + 1)].fg, theme.border());
    assert!(
        pending
            .content
            .iter()
            .any(|cell| cell.symbol() == "u" && cell.fg == theme.text_muted())
    );
    assert!(state.tick_ui(state.next_ui_deadline().unwrap()));
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(terminal.backend().buffer(), &normal);
}

#[tokio::test]
async fn vis11_modal_pending_composer_uses_existing_backdrop_and_restores_without_input() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut state = golden_state().await;
    state.handle_paste("unsent Unicode αβ draft");
    state.handle_key(KeyAction::Commands).await;
    for key in "zzzz-no-such-command".chars() {
        state.handle_key(KeyAction::Char(key)).await;
    }
    let area = Rect::new(0, 0, 120, 40);
    let regions = shell_regions(&state, area);
    let regions = session_regions(&state, session_main(&state, regions.session), 40);
    let point = (
        regions.prompt.x + 1 + layout::session_padding(120),
        regions.prompt.y + 1,
    );
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let normal = terminal.backend().buffer().clone();
    assert_eq!(normal[point].symbol(), "u");
    assert_eq!(normal[point].fg, Color::Rgb(98, 98, 98));
    let action = state
        .terminal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))
        .unwrap();
    state.handle_key(action).await;
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let pending = terminal.backend().buffer();
    assert_eq!(pending[point].fg, Color::Rgb(53, 53, 53));
    for y in 0..40 {
        for x in 0..120 {
            assert_eq!(normal[(x, y)].symbol(), pending[(x, y)].symbol());
            if y < regions.prompt.y || y > regions.underline.y {
                assert_eq!(normal[(x, y)], pending[(x, y)], "outside composer {x},{y}");
            }
        }
    }
    assert!(state.tick_ui(state.next_ui_deadline().unwrap()));
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(terminal.backend().buffer(), &normal);
    assert_eq!(state.input(), "unsent Unicode αβ draft");
}

#[tokio::test]
async fn vis25_inline_rows_on_home_and_session_and_after_tab() {
    for home in [false, true] {
        let mut state = if home {
            let (app, guard) = CoreApp::spawn(MockProvider::echo());
            std::mem::forget(guard);
            let mut state = TuiState::new_home(app);
            state.apply_catalog(catalog());
            state
        } else {
            golden_state().await
        };
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        state.handle_paste("/side");
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let rows = screen(&state, 120, 40);
        let (y, row) = rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.contains("/sidebar"))
            .unwrap();
        assert!(row.contains("Toggle sidebar"));
        let x = UnicodeWidthStr::width(&row[..row.find("/sidebar").unwrap()]) as u16;
        // The upstream anchor is the entire prompt box, including its
        // left edge (`prompt/index.tsx:1649`, `autocomplete.tsx:851-860`).
        let (anchor_x, anchor_width) = if home {
            let width = (120 - 2 * layout::session_padding(120)).min(75);
            ((120 - width).div_ceil(2), width)
        } else {
            let shell = shell_regions(&state, Rect::new(0, 0, 120, 40));
            let main = session_main(&state, shell.session);
            let body = session_regions(&state, main, 40).prompt;
            (body.x, body.width)
        };
        assert_eq!(
            x,
            anchor_x + 2,
            "row text starts after the left border and padding"
        );
        assert_eq!(
            terminal.backend().buffer()[(anchor_x, y as u16)].symbol(),
            "┃"
        );
        assert_eq!(
            terminal.backend().buffer()[(anchor_x + anchor_width - 1, y as u16)].symbol(),
            "┃"
        );
        let focused = Theme::dark()
            .color("background.action.primary.$focused")
            .unwrap();
        assert_eq!(terminal.backend().buffer()[(x, y as u16)].bg, focused);
        state.handle_key(KeyAction::Tab).await;
        assert_eq!(state.input(), "/sidebar ");
        let rows = screen(&state, 120, 40);
        assert_eq!(
            rows.iter().filter(|row| row.contains("/sidebar")).count(),
            1,
            "completion remains only in the draft"
        );
        state.handle_key(KeyAction::SelectHome).await;
        state.handle_paste("/zzzznotacommand");
        assert!(
            screen(&state, 120, 40)
                .iter()
                .any(|row| row.contains("No matching commands"))
        );
    }
}

#[tokio::test]
async fn slash_descriptions_align_to_full_inventory_on_home_and_session() {
    for home in [true, false] {
        let mut state = if home {
            let (app, guard) = CoreApp::spawn(MockProvider::echo());
            std::mem::forget(guard);
            let mut state = TuiState::new_home(app);
            state.apply_catalog(catalog());
            state
        } else {
            golden_state().await
        };
        state.handle_paste("/");
        let unfiltered = state.slash_options().unwrap();
        let builtin_width = unfiltered
            .iter()
            .map(|option| option.name.len() + 1)
            .max()
            .unwrap()
            + 2;
        assert!(
            unfiltered
                .iter()
                .any(|option| option.name == "dcp-compress")
        );
        assert!(
            unfiltered
                .iter()
                .all(|option| option.display_width == builtin_width)
        );
        let rows = screen(&state, 120, 40);
        for name in ["agents", "cards"] {
            let option = unfiltered
                .iter()
                .find(|option| option.name == name)
                .unwrap();
            let row = rows
                .iter()
                .find(|row| row.contains(&format!("/{name}")))
                .unwrap();
            let x = row.find(&option.description).unwrap();
            assert_eq!(
                x,
                row.find(&format!("/{name}")).unwrap() + builtin_width + 1
            );
        }
        state.apply_catalog({
            let mut snapshot = catalog();
            snapshot.commands = vec!["extraordinarily-long-workspace-command".into()];
            snapshot
        });
        let full_width = "/extraordinarily-long-workspace-command".len() + 2;
        assert!(full_width > builtin_width);
        assert!(
            state
                .slash_options()
                .unwrap()
                .iter()
                .all(|option| option.display_width == full_width)
        );
        // The long workspace command is not a match for /ren. It still
        // fixes the column for the filtered built-in suggestion.
        state.handle_paste("ren");
        let name = if home { "reload" } else { "rename" };
        let option = state
            .slash_options()
            .unwrap()
            .into_iter()
            .find(|option| option.name == name)
            .unwrap();
        assert_eq!(option.display_width, full_width);
        let rows = screen(&state, 120, 40);
        let (y, row) = rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.contains(&format!("/{name}")))
            .unwrap();
        let anchor_x = if home {
            (120_u16 - 75).div_ceil(2)
        } else {
            let shell = shell_regions(&state, Rect::new(0, 0, 120, 40));
            let main = session_main(&state, shell.session);
            session_regions(&state, main, 40).prompt.x
        } as usize;
        assert_eq!(
            UnicodeWidthStr::width(&row[..row.find(&format!("/{name}")).unwrap()]),
            anchor_x + 2
        );
        let description_x = anchor_x + 3 + full_width;
        assert_eq!(
            UnicodeWidthStr::width(&row[..row.find(&option.description).unwrap()]),
            description_x
        );
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        assert_eq!(
            terminal.backend().buffer()[(description_x as u16, y as u16)].symbol(),
            &option.description[..1]
        );
        assert_eq!(
            terminal.backend().buffer()[((description_x - 1) as u16, y as u16)].symbol(),
            " "
        );

        state.apply_catalog(catalog());
        let narrowed = state
            .slash_options()
            .unwrap()
            .into_iter()
            .find(|option| option.name == name)
            .unwrap();
        assert_eq!(
            narrowed.display_width, builtin_width,
            "Location command removal updates width"
        );
    }
}

#[tokio::test]
async fn slash_label_and_description_have_independent_foregrounds() {
    for home in [true, false] {
        let mut state = if home {
            let (app, guard) = CoreApp::spawn(MockProvider::echo());
            std::mem::forget(guard);
            let mut state = TuiState::new_home(app);
            state.apply_catalog(catalog());
            state
        } else {
            golden_state().await
        };
        state.handle_paste("/");
        let options = state.slash_options().unwrap();
        assert!(options.len() > 1);
        let rows = screen(&state, 120, 40);
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        let theme = Theme::dark();
        for (index, option) in options.iter().take(2).enumerate() {
            let (y, row) = rows
                .iter()
                .enumerate()
                .find(|(_, row)| row.contains(&format!("/{}", option.name)))
                .unwrap();
            let name_x =
                UnicodeWidthStr::width(&row[..row.find(&format!("/{}", option.name)).unwrap()])
                    as u16;
            let description_x =
                UnicodeWidthStr::width(&row[..row.find(&option.description).unwrap()]) as u16;
            let (fg, bg, description_fg) = if index == 0 {
                let fg = theme.color("text.action.primary.$focused").unwrap();
                (
                    fg,
                    theme.color("background.action.primary.$focused").unwrap(),
                    fg,
                )
            } else {
                (
                    theme.text(),
                    theme.background_raised_high(),
                    theme.text_muted(),
                )
            };
            let name = &buffer[(name_x, y as u16)];
            let description = &buffer[(description_x, y as u16)];
            assert_eq!(name.symbol(), "/");
            assert_eq!(name.fg, fg);
            assert_eq!(name.bg, bg);
            assert_eq!(description.fg, description_fg);
            assert_eq!(description.bg, bg);
            assert_eq!(buffer[(description_x - 1, y as u16)].bg, bg);
        }
    }
}

#[tokio::test]
async fn slash_clips_description_and_wide_label_at_grapheme_boundaries() {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    std::mem::forget(guard);
    let mut state = TuiState::new_home(app);
    let mut snapshot = catalog();
    snapshot.commands = vec!["aaa".into(), "aab".into()];
    snapshot.command_descriptions.insert(
        "aab".into(),
        "界界界界e\u{301}🙂Z trailing text that must not wrap".into(),
    );
    state.apply_catalog(snapshot);
    state.handle_paste("/");
    let options = state.slash_options().unwrap();
    let index = options
        .iter()
        .position(|option| option.name == "aab")
        .unwrap();
    assert_eq!(index, 1);
    let theme = Theme::dark();
    let label_width = 1 + options[index].display_width as u16;
    let body = Rect::new(2, 15, label_width + 11 + 2, 1);
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
    terminal
        .draw(|frame| render_slash(frame, &state, theme, body))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let y = body.y - 10 + index as u16;
    // Eleven cells remain after the full-inventory padded label.
    let desc_x = body.x + 1 + label_width;
    assert_eq!(buffer[(desc_x, y)].symbol(), " ");
    assert_eq!(buffer[(desc_x + 1, y)].symbol(), "界");
    assert_eq!(buffer[(desc_x + 9, y)].symbol(), "e\u{301}");
    assert_eq!(buffer[(body.right() - 2, y)].symbol(), " ");
    assert_eq!(
        buffer[(body.right() - 2, y)].bg,
        theme.background_raised_high()
    );
    assert_eq!(buffer[(body.right() - 2, y)].fg, theme.text());
    assert_eq!(buffer[(body.right() - 1, y)].symbol(), "┃");
    assert_eq!(buffer[(body.right() - 2, body.y)].symbol(), " ");

    let mut snapshot = catalog();
    snapshot.commands = vec!["界".repeat(9)];
    state.apply_catalog(snapshot);
    state.handle_paste("界");
    let option = &state.slash_options().unwrap()[0];
    assert_eq!(option.display_width, 21);
    let body = Rect::new(2, 15, 9, 1);
    terminal
        .draw(|frame| render_slash(frame, &state, theme, body))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let y = body.y - 1;
    assert_eq!(buffer[(body.x + 2, y)].symbol(), "/");
    assert_eq!(buffer[(body.x + 3, y)].symbol(), "界");
    assert_eq!(buffer[(body.x + 5, y)].symbol(), "界");
    assert_eq!(buffer[(body.right() - 2, y)].symbol(), " ");
    assert_eq!(buffer[(body.right() - 1, y)].symbol(), "┃");
}

#[tokio::test]
async fn slash_overlay_erases_home_logo_before_painting_blank_option_cells() {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    std::mem::forget(guard);
    let mut state = TuiState::new_home(app);
    state.apply_catalog(catalog());
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let home = terminal.backend().buffer().clone();

    state.handle_paste("/");
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let rows = screen(&state, 120, 40);
    let options = state.slash_options().unwrap();
    assert!(options.len() >= 7, "overlay must reach the logo");
    let first = &options[0];
    let (y, _row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains(&format!("/{}", first.name)))
        .expect("slash option above the Home logo");
    let prompt_x = (120_u16 - 75).div_ceil(2);
    let logo_cell = options
        .iter()
        .take(10)
        .enumerate()
        .find_map(|(index, option)| {
            let label = format!(
                " /{:<width$} {}",
                option.name,
                option.description,
                width = option.display_width - 1
            );
            let start = prompt_x + 1 + UnicodeWidthStr::width(label.as_str()) as u16;
            (start..prompt_x + 74)
                .find(|&x| home[(x, y as u16 + index as u16)].symbol() != " ")
                .map(|x| (x, y as u16 + index as u16))
        });
    let (logo_x, logo_y) = logo_cell.expect("Home logo lies behind a slash option's blank padding");

    let theme = Theme::dark();
    let buffer = terminal.backend().buffer();
    assert_eq!(
        buffer[(logo_x, logo_y)].symbol(),
        " ",
        "logo glyph must be erased"
    );
    for (row_offset, option) in options.iter().take(10).enumerate() {
        let label = format!(
            " /{:<width$} {}",
            option.name,
            option.description,
            width = option.display_width - 1
        );
        let start = prompt_x + 1 + UnicodeWidthStr::width(clip_placeholder(&label, 73)) as u16;
        let expected_bg = if row_offset == 0 {
            theme.color("background.action.primary.$focused").unwrap()
        } else {
            theme.background_raised_high()
        };
        let expected_fg = if row_offset == 0 {
            theme.color("text.action.primary.$focused").unwrap()
        } else {
            theme.text()
        };
        for x in start..prompt_x + 74 {
            let cell = &buffer[(x, y as u16 + row_offset as u16)];
            assert_eq!(cell.symbol(), " ", "row={row_offset} x={x}");
            assert_eq!(cell.bg, expected_bg, "row={row_offset} x={x}");
            assert_eq!(cell.fg, expected_fg, "row={row_offset} x={x}");
        }
    }
    assert_eq!(buffer[(prompt_x, y as u16)].symbol(), "┃");
    assert_eq!(buffer[(prompt_x + 74, y as u16)].symbol(), "┃");
    assert!(rows.iter().any(|row| row.contains("┃  /")));
}

#[tokio::test]
async fn vis26_file_rows_anchor_to_home_and_session_prompt() {
    for home in [false, true] {
        let mut state = if home {
            let (app, guard) = CoreApp::spawn(MockProvider::echo());
            std::mem::forget(guard);
            let mut view = TuiState::new_home(app);
            view.apply_catalog(catalog());
            view
        } else {
            golden_state().await
        };
        state.chrome.location = Some("/fixture".into());
        state.handle_paste("look @sr");
        let key = state.mention_request().unwrap();
        assert!(state.apply_file_suggestions(
            key,
            oc_core::queries::FileSuggestionsSnapshot {
                location: "/fixture".into(),
                generation: 3,
                paths: vec!["src/lib.rs".into(), "src/main.rs".into()],
                truncated: false,
            }
        ));
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let rows = screen(&state, 120, 40);
        let (y, row) = rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.contains("src/lib.rs"))
            .unwrap();
        let x = UnicodeWidthStr::width(&row[..row.find("src/lib.rs").unwrap()]) as u16;
        let prompt = if home {
            let width = (120 - 2 * layout::session_padding(120)).min(75);
            let px = (120 - width).div_ceil(2);
            assert_eq!(terminal.backend().buffer()[(px, y as u16)].symbol(), "┃");
            px
        } else {
            session_regions(
                &state,
                session_main(
                    &state,
                    shell_regions(&state, Rect::new(0, 0, 120, 40)).session,
                ),
                40,
            )
            .prompt
            .x
        };
        assert_eq!(x, prompt + 2);
        assert_eq!(
            terminal.backend().buffer()[(x, y as u16)].bg,
            Theme::dark()
                .color("background.action.primary.$focused")
                .unwrap()
        );
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(x - 1, y as u16)].fg, Color::Rgb(255, 255, 255));
        assert_eq!(buffer[(x - 1, y as u16)].bg, buffer[(x, y as u16)].bg);
        assert_eq!(buffer[(prompt, y as u16)].bg, Theme::dark().background());
        assert_eq!(buffer[(x + 12, y as u16)].fg, Color::Rgb(255, 255, 255));
        state.handle_key(KeyAction::Down).await;
        state.handle_key(KeyAction::Tab).await;
        assert_eq!(state.input(), "look @src/main.rs ");
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let (draft_y, draft) = screen(&state, 120, 40)
            .into_iter()
            .enumerate()
            .find(|(_, row)| row.contains("look @src/main.rs"))
            .unwrap();
        let at = UnicodeWidthStr::width(&draft[..draft.find('@').unwrap()]) as u16;
        let buffer = terminal.backend().buffer();
        let mention = &buffer[(at, draft_y as u16)];
        assert_eq!(mention.fg, Theme::dark().warning());
        assert!(mention.modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(at + 11, draft_y as u16)].fg, mention.fg);
        assert_eq!(buffer[(at - 1, draft_y as u16)].fg, Theme::dark().text());
        assert!(
            !buffer[(at + 12, draft_y as u16)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert!(
            !screen(&state, 120, 40)
                .iter()
                .any(|row| row.contains("src/lib.rs"))
        );
        state.handle_paste(" @zzzz");
        let key = state.mention_request().unwrap();
        assert!(state.apply_file_suggestions(
            key,
            oc_core::queries::FileSuggestionsSnapshot {
                location: "/fixture".into(),
                generation: 3,
                paths: Vec::new(),
                truncated: false,
            }
        ));
        assert!(
            screen(&state, 120, 40)
                .iter()
                .any(|row| row.contains("No matching files"))
        );
    }
}

#[tokio::test]
async fn home_empty_prompt_shows_one_muted_example_and_preserves_caret() {
    let mut state = golden_state().await;
    state.home = true;
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let rows = screen(&state, 80, 24);
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains("Ask anything… \""))
        .expect("Home placeholder");
    let start = row.find("Ask anything… \"").unwrap();
    let start_col = UnicodeWidthStr::width(&row[..start]);
    let expected = format!("Ask anything… \"{}\"", state.home_example);
    assert!(HOME_EXAMPLES.contains(&state.home_example));
    assert!(row[start..].starts_with(&expected));
    for x in start_col..start_col + UnicodeWidthStr::width(expected.as_str()) {
        assert_eq!(buffer[(x as u16, y as u16)].fg, Theme::dark().text_muted());
        assert_eq!(
            buffer[(x as u16, y as u16)].bg,
            Theme::dark().decrease(Theme::dark().background_panel())
        );
    }
    assert_eq!(
        buffer[(
            (start_col + UnicodeWidthStr::width(expected.as_str())) as u16,
            y as u16
        )]
            .fg,
        Color::Rgb(255, 255, 255),
        "spaces after the placeholder retain the upstream canvas foreground"
    );
    assert_eq!(state.prompt_layout(70).1, (0, 0));
}

#[tokio::test]
async fn prompt_metadata_layout_gaps_keep_canvas_foreground() {
    let mut state = golden_state().await;
    state.home = true;
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let rows = screen(&state, 80, 24);
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains("X · a"))
        .expect("Home metadata");
    let start = UnicodeWidthStr::width(&row[..row.find("X · a").unwrap()]);
    let white = Color::Rgb(255, 255, 255);
    assert_eq!(
        buffer[(start as u16, y as u16)].fg,
        Theme::dark().categorical_agents()[0]
    );
    assert_eq!(buffer[((start + 1) as u16, y as u16)].fg, white);
    assert_eq!(
        buffer[((start + 2) as u16, y as u16)].fg,
        Theme::dark().text_muted()
    );
    assert_eq!(buffer[((start + 3) as u16, y as u16)].fg, white);
    assert_eq!(
        buffer[((start + 4) as u16, y as u16)].fg,
        Theme::dark().text()
    );
    assert_eq!(buffer[((start + 18) as u16, y as u16)].fg, white);
}

#[tokio::test]
async fn restored_home_version_colors_only_its_glyphs() {
    let mut state = golden_state().await;
    state.home = true;
    state.chrome.devtools = Some(false);
    state.set_tab_strip(
        vec![TabPresentation {
            title: Some("Restored".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(SessionId("restored".into()))
        }],
        0,
        true,
    );
    let theme = Theme::dark();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let version = env!("CARGO_PKG_VERSION");
    let version_x = 118 - version.len() as u16;
    for x in 0..version_x {
        let cell = &buffer[(x, 38)];
        assert_eq!(cell.symbol(), " ", "Home version padding at x={x}");
        assert_eq!(cell.fg, Color::Rgb(255, 255, 255), "x={x}");
        assert_eq!(cell.bg, theme.background(), "x={x}");
    }
    for (offset, glyph) in version.chars().enumerate() {
        let cell = &buffer[(version_x + offset as u16, 38)];
        assert_eq!(cell.symbol(), glyph.to_string());
        assert_eq!(cell.fg, theme.text_muted());
        assert_eq!(cell.bg, theme.background());
    }
    for x in 118..120 {
        let cell = &buffer[(x, 38)];
        assert_eq!(cell.symbol(), " ");
        assert_eq!(cell.fg, Color::Rgb(255, 255, 255));
        assert_eq!(cell.bg, theme.background());
    }
    let rows = screen(&state, 120, 40);
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains("X · a"))
        .expect("Home prompt metadata");
    let x = UnicodeWidthStr::width(&row[..row.find("X · a").unwrap()]) as u16;
    for (offset, symbol, fg) in [
        (0, "X", theme.categorical_agents()[0]),
        (1, " ", Color::Rgb(255, 255, 255)),
        (2, "·", theme.text_muted()),
        (3, " ", Color::Rgb(255, 255, 255)),
        (4, "a", theme.text()),
    ] {
        let cell = &buffer[(x + offset, y as u16)];
        assert_eq!(cell.symbol(), symbol);
        assert_eq!(cell.fg, fg);
        assert_eq!(cell.bg, theme.decrease(theme.background_panel()));
    }
}

#[tokio::test]
async fn home_version_slot_matches_pinned_width_and_height_breakpoints() {
    let mut state = golden_state().await;
    state.home = true;
    state.chrome.devtools = Some(false);
    let version = env!("CARGO_PKG_VERSION");
    for (width, height, visible) in [
        (43, 24, false),
        (44, 24, false),
        (63, 24, false),
        (64, 11, false),
        (64, 12, true),
        (64, 24, true),
        (120, 40, true),
    ] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        let version_row = height - if height < 16 { 1 } else { 2 };
        let version_x = width - 2 - version.len() as u16;
        let actual = (version_x..version_x + version.len() as u16)
            .map(|x| buffer[(x, version_row)].symbol())
            .collect::<String>();
        if visible {
            assert_eq!(actual, version, "Home {width}x{height}");
        } else {
            assert_ne!(actual, version, "Home {width}x{height}");
        }
    }
}

#[tokio::test]
async fn empty_narrow_home_footer_keeps_original_logo_and_prompt_rows() {
    let mut state = golden_state().await;
    state.home = true;
    state.chrome.devtools = Some(false);
    for (width, logo_y, prompt_y) in [(44, 8, 14), (63, 8, 14), (64, 7, 13), (120, 15, 21)] {
        let rows = screen(&state, width, if width == 120 { 40 } else { 24 });
        let locate = |needle: &str| rows.iter().position(|row| row.contains(needle));
        assert_eq!(locate("█▀▀█ █▀▀█"), Some(logo_y), "Home width {width}");
        assert_eq!(
            locate("Ask anything…"),
            Some(prompt_y),
            "Home width {width}"
        );
    }
}

#[tokio::test]
async fn short_home_shrinks_the_top_spacer_before_clipping_the_prompt() {
    let mut state = golden_state().await;
    state.home = true;
    state.chrome.devtools = Some(false);
    for height in 12..=15 {
        let rows = screen(&state, 44, height);
        let locate = |needle: &str| rows.iter().position(|row| row.contains(needle));
        assert_eq!(locate("█▀▀█ █▀▀█"), Some(height as usize - 11));
        assert_eq!(locate("Ask anything…"), Some(height as usize - 5));
        assert_eq!(locate("X · a"), Some(height as usize - 3));
    }
}

#[tokio::test]
async fn home_hint_is_hidden_when_typing_and_session_has_no_hint() {
    let mut state = golden_state().await;
    state.home = true;
    state.handle_key(KeyAction::Char('x')).await;
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let rows = screen(&state, 80, 24);
    assert!(!rows.iter().any(|row| row.contains("Ask anything…")));
    assert!(rows.iter().any(|row| row.contains("┃  x")));
    state.home = false;
    state.handle_key(KeyAction::Backspace).await;
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert!(
        !screen(&state, 80, 24)
            .iter()
            .any(|row| row.contains("Ask anything…"))
    );
}

#[tokio::test]
async fn home_hint_respects_actual_text_width_at_breakpoints() {
    let mut state = golden_state().await;
    state.home = true;
    for (width, max_cells) in [(24, 19), (43, 38), (44, 35), (120, 70)] {
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let rows = screen(&state, width, 24);
        let row = rows
            .iter()
            .find(|row| row.contains("Ask anything… \""))
            .unwrap();
        let hint = &row[row.find("Ask anything… \"").unwrap()..];
        let full_hint = format!("Ask anything… \"{}\"", state.home_example);
        assert_eq!(hint, clip_placeholder(&full_hint, max_cells).trim_end());
        assert!(UnicodeWidthStr::width(hint) <= max_cells, "{width}: {hint}");
    }
    assert_eq!(clip_placeholder("a界e\u{301}z", 2), "a");
    assert_eq!(clip_placeholder("a界e\u{301}z", 3), "a界");
    assert_eq!(clip_placeholder("a界e\u{301}z", 4), "a界e\u{301}");
}

#[tokio::test]
async fn root_canvas_blanks_have_upstream_truecolor_foreground() {
    let mut state = golden_state().await;
    state.chrome.devtools = Some(false);
    state.chrome.sidebar_hidden = true;
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    for home in [true, false] {
        state.home = home;
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        let blank = if home { (0, 0) } else { (0, 15) };
        assert_eq!(buffer[blank].symbol(), " ");
        assert_eq!(buffer[blank].bg, Theme::dark().background());
        assert_eq!(buffer[blank].fg, Color::Rgb(255, 255, 255));
        if !home {
            let user_border = buffer
                .content
                .iter()
                .find(|cell| cell.symbol() == "┃")
                .expect("user border");
            assert_ne!(user_border.fg, Color::Rgb(255, 255, 255));
        }
    }
}

#[tokio::test]
async fn restored_session_user_padding_and_empty_prompt_keep_canvas_foreground() {
    let mut state = golden_state().await;
    state.chrome.devtools = Some(false);
    state.chrome.sidebar_hidden = true;
    state.set_tab_strip(
        vec![
            TabPresentation {
                title: Some("Old".into()),
                home: false,
                busy: false,
                ..TabPresentation::new(SessionId("old".into()))
            },
            TabPresentation {
                title: Some("Second".into()),
                home: false,
                busy: false,
                ..TabPresentation::new(SessionId("second".into()))
            },
        ],
        1,
        true,
    );
    let theme = Theme::dark();
    let white = Color::Rgb(255, 255, 255);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    for x in 5..=115 {
        let cell = &buffer[(x, 34)];
        assert_eq!(cell.symbol(), " ", "prompt at x={x}");
        assert_eq!(cell.bg, theme.decrease(theme.background_panel()), "x={x}");
        assert_eq!(cell.fg, white, "prompt at x={x}");
    }
    let prompt_border = &buffer[(2, 34)];
    assert_eq!(prompt_border.symbol(), "┃");
    assert_ne!(prompt_border.fg, white);
    for x in 3..=4 {
        let cell = &buffer[(x, 3)];
        assert_eq!(cell.symbol(), " ", "user padding at x={x}");
        assert_eq!(cell.bg, theme.user_message_background(), "x={x}");
        assert_eq!(cell.fg, white, "user padding at x={x}");
    }
    let user_text = &buffer[(5, 3)];
    assert_eq!(user_text.symbol(), "h");
    assert_eq!(user_text.fg, theme.text());
    assert_eq!(user_text.bg, theme.user_message_background());
    let user_border = &buffer[(2, 3)];
    assert_eq!(user_border.symbol(), "┃");
    assert_ne!(user_border.fg, white);

    state.handle_key(KeyAction::Char('d')).await;
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(5, 34)].symbol(), "d");
    assert_eq!(buffer[(5, 34)].fg, theme.text());
    assert_eq!(buffer[(5, 34)].bg, theme.decrease(theme.background_panel()));
    assert_eq!(buffer[(6, 34)].fg, white);
}

#[tokio::test]
async fn selected_session_tab_title_is_bold() {
    let state = golden_state().await;
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(3, 0)].symbol(), "U");
    assert_eq!(buffer[(3, 0)].fg, Theme::dark().text());
    assert!(buffer[(3, 0)].modifier.contains(Modifier::BOLD));
    assert!(!buffer[(1, 0)].modifier.contains(Modifier::BOLD));
    for x in 0..3 {
        assert_eq!(buffer[(x, 0)].symbol(), " ");
        assert_eq!(buffer[(x, 0)].fg, Color::Rgb(255, 255, 255));
        assert_eq!(
            buffer[(x, 0)].bg,
            Theme::dark().decrease(Theme::dark().background_panel())
        );
    }
}

#[tokio::test]
async fn vis39_selected_animated_spinner_attributes_and_prefix_match_u47_fallback() {
    let mut state = golden_state().await;
    state.set_tab_strip(
        vec![TabPresentation {
            busy: true,
            title: Some("Own running".into()),
            ..TabPresentation::new(SessionId("own".into()))
        }],
        0,
        false,
    );
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    for animations in [true, false] {
        state.chrome.animations = Some(animations);
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let cells = terminal.backend().buffer();
        assert_eq!(cells[(1, 0)].symbol(), "⠋");
        assert_eq!(cells[(1, 0)].modifier.contains(Modifier::BOLD), !animations);
        for x in [0, 2] {
            assert_eq!(
                cells[(x, 0)].fg,
                if animations {
                    Color::Rgb(238, 238, 238)
                } else {
                    Color::Rgb(255, 255, 255)
                }
            );
            assert!(!cells[(x, 0)].modifier.contains(Modifier::BOLD));
        }
        assert!(cells[(3, 0)].modifier.contains(Modifier::BOLD));
    }
    state.chrome.animations = Some(true);
    state.chrome.tab_indicators = TabIndicators::Numbers;
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(1, 0)].symbol(), "1");
    assert!(
        terminal.backend().buffer()[(1, 0)]
            .modifier
            .contains(Modifier::BOLD)
    );
    state.chrome.tab_indicators = TabIndicators::Status;
    state.tab_attention.insert(0);
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(1, 0)].symbol(), "!");
    assert!(
        terminal.backend().buffer()[(1, 0)]
            .modifier
            .contains(Modifier::BOLD)
    );
}

#[tokio::test]
async fn vis41_real_vertical_detail_uses_owner_location_fallback_without_synthetic_metadata() {
    let mut state = golden_state().await;
    state.chrome.vertical_tabs_width = 42;
    state.chrome.animations = Some(false);
    state.set_tab_strip(
        vec![TabPresentation {
            title: Some("Real title".into()),
            detail: Some("actual-project".into()),
            ..TabPresentation::new(SessionId("real-location".into()))
        }],
        0,
        false,
    );
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let cells = terminal.backend().buffer();
    assert_eq!(
        (3..17).map(|x| cells[(x, 2)].symbol()).collect::<String>(),
        "actual-project"
    );
    assert_eq!(
        cells[(3, 2)].fg,
        tint(Theme::dark().text_muted(), cells[(3, 2)].bg, 0.35)
    );
    assert!(!cells[(3, 2)].modifier.contains(Modifier::BOLD));
    assert!(!screen(&state, 80, 24)[0].contains("actual-project"));
}

#[test]
fn single_tab_render_uses_layout_without_an_inert_add_control() {
    let theme = Theme::dark();
    for width in [32, 80, 120] {
        let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
        terminal
            .draw(|frame| {
                render_tabs(
                    frame,
                    theme,
                    Rect::new(0, 0, width, 1),
                    Some("Tab"),
                    TabIndicators::Numbers,
                    false,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(31, 0)].bg, theme.decrease(theme.background_panel()));
        if width > 32 {
            assert_ne!(buffer[(32, 0)].bg, theme.decrease(theme.background_panel()));
            assert_eq!(buffer[(32, 0)].symbol(), " ");
        }
        assert!(!buffer.content.iter().any(|cell| cell.symbol() == "+"));
    }
}

#[tokio::test]
async fn selected_tab_indicator_tracks_busy_turn_and_explicit_numbers() {
    let mut state = golden_state().await;
    state.chrome.devtools = Some(false);
    state.chrome.sidebar_hidden = true;
    let theme = Theme::dark();
    let tab_bg = theme.decrease(theme.background_panel());
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    state.chrome.tab_indicators = TabIndicators::Numbers;
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), " ");
    assert_eq!(buffer[(1, 0)].symbol(), "1");
    assert_eq!(buffer[(1, 0)].fg, tint(theme.text(), tab_bg, 0.25));
    assert_eq!(buffer[(2, 0)].symbol(), " ");
    assert_eq!(buffer[(3, 0)].symbol(), "U");

    state.chrome.tab_indicators = TabIndicators::Status;
    state.handle_key(KeyAction::Char('h')).await;
    state.handle_key(KeyAction::Enter).await;
    assert!(state.is_busy());
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), " ");
    assert_eq!(buffer[(1, 0)].symbol(), "⠋");
    assert_eq!(buffer[(1, 0)].fg, tint(theme.primary(), theme.text(), 0.85));
    assert_eq!(buffer[(1, 0)].bg, tab_bg);
    assert_eq!(buffer[(2, 0)].symbol(), " ");
    assert_eq!(buffer[(3, 0)].symbol(), "U");

    state.chrome.tab_indicators = TabIndicators::Numbers;
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(1, 0)].symbol(), "1");
}

#[tokio::test]
async fn retained_strip_paints_real_tabs_busy_states_and_only_available_add() {
    let mut state = golden_state().await;
    state.chrome.devtools = Some(false);
    let tabs = vec![
        TabPresentation {
            title: Some("First".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(SessionId("first".into()))
        },
        TabPresentation {
            title: Some("Running".into()),
            home: false,
            busy: true,
            ..TabPresentation::new(SessionId("running".into()))
        },
        TabPresentation {
            title: Some("Last".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(SessionId("last".into()))
        },
    ];
    state.set_tab_strip(tabs.clone(), 1, true);
    let area = Rect::new(0, 0, 80, 24);
    let strip = tab_strip(&state, area).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let first = strip.tabs.iter().find(|t| t.index == 0).unwrap().rect;
    let running = strip.tabs.iter().find(|t| t.index == 1).unwrap().rect;
    assert_eq!(buffer[(first.x + 3, 0)].symbol(), "F");
    assert_eq!(buffer[(first.x + 3, 0)].fg, Theme::dark().text_muted());
    assert!(!buffer[(first.x + 3, 0)].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(running.x + 1, 0)].symbol(), "⠋");
    assert_eq!(buffer[(running.x + 1, 0)].fg, Theme::dark().primary());
    assert_eq!(buffer[(running.x + 3, 0)].symbol(), "R");
    assert!(buffer[(running.x + 3, 0)].modifier.contains(Modifier::BOLD));
    let add = strip.add.unwrap();
    assert_eq!(add.width, 3);
    assert_eq!(buffer[(add.x, 0)].symbol(), " ");
    assert_eq!(buffer[(add.x + 1, 0)].symbol(), "+");
    assert_eq!(buffer[(add.x + 1, 0)].fg, Theme::dark().text_muted());
    assert_eq!(buffer[(add.x + 2, 0)].symbol(), " ");

    state.chrome.tab_indicators = TabIndicators::Numbers;
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(
        terminal.backend().buffer()[(running.x + 1, 0)].symbol(),
        "2"
    );
    assert_eq!(
        terminal.backend().buffer()[(strip.tabs[2].rect.x + 1, 0)].symbol(),
        "3"
    );
    state.set_tab_strip(tabs, 1, false);
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert!(tab_strip(&state, area).unwrap().add.is_none());
    assert!(!(0..80).any(|x| terminal.backend().buffer()[(x, 0)].symbol() == "+"));
}

#[test]
fn vis41_marquee_uses_u59_cell_cursor_and_generated_separator_fixtures() {
    let text = |title, width, offset| {
        marquee_parts(title, width, offset)
            .iter()
            .map(|(text, _)| *text)
            .collect::<String>()
    };
    assert_eq!(text("Short", 10, 8), "Short");
    assert_eq!(text("Exact fit", 9, 8), "Exact fit");
    for (offset, expected) in [
        (0, "A long s"),
        (2, "long ses"),
        (15, "title · "),
        (20, " · A lon"),
        (23, "A long s"),
    ] {
        assert_eq!(text("A long session title", 8, offset), expected);
    }
    assert_eq!(
        marquee_parts("A · title", 6, 7),
        [
            ("l", false),
            ("e", false),
            (" ", false),
            ("·", true),
            (" ", false),
            ("A", false)
        ]
    );
    assert_eq!(marquee_parts("A · title", 6, 2)[0], ("·", false));
    assert_eq!(text("Plan 🧭 the release", 8, 5), "🧭 the r");
    assert_eq!(text("Plan 🧭 the release", 8, 6), " the rel");
    assert_eq!(text("e\u{301}🧑‍💻界tail", 4, 1), "🧑‍💻界");
    assert!(marquee_parts("title", 0, 1).is_empty());
}

#[tokio::test]
async fn vis39_painted_permission_priority_number_mode_and_static_busy_fallback() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, SessionId("indicator".into()));
    let mut tab = TabPresentation::new(SessionId("indicator".into()));
    tab.busy = true;
    tab.attention = Some(TabAttention::Question);
    state.set_tab_strip(vec![tab.clone()], 0, false);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(1, 0)].symbol(), "?");
    assert_eq!(state.next_ui_deadline(), None);
    state.tab_attention.insert(0);
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(1, 0)].symbol(), "!");
    state.chrome.tab_indicators = TabIndicators::Numbers;
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_eq!(
        terminal.backend().buffer()[(1, 0)].symbol(),
        "1",
        "attention tints but cannot replace ordinal"
    );
    assert_eq!(state.next_ui_deadline(), None);
    state.chrome.tab_indicators = TabIndicators::Status;
    state.chrome.animations = Some(false);
    state.tab_attention.clear();
    tab.attention = None;
    state.set_tab_strip(vec![tab], 0, false);
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(1, 0)].symbol(), "⠋");
    assert_eq!(state.next_ui_deadline(), None);
    state.chrome.animations = Some(true);
    terminal.draw(|f| render(f, &state)).unwrap();
    let spinner = state.next_ui_deadline().unwrap() + std::time::Duration::from_millis(80);
    assert!(state.tick_ui(spinner));
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(1, 0)].symbol(), "⠙");
}

#[test]
fn vis39_u48_styled_sweep_whitecaps_and_blend_order_preserve_title_attributes() {
    assert_eq!(
        blend_tab_pulse_color(
            Color::Rgb(10, 20, 30),
            [
                (Color::Rgb(110, 120, 130), 1.0, 0.16),
                (Color::Rgb(210, 220, 230), 1.0, 0.14),
                (Color::Rgb(250, 240, 230), 1.0, 0.1),
                (Color::Rgb(90, 80, 70), 1.0, 0.18),
            ]
        ),
        Color::Rgb(75, 80, 84)
    );
    let theme = Theme::dark();
    let pulse = TabPulseFrame {
        running: 1.0,
        sweep_clock: 1.4,
        whitecap: 0.85,
        dim: 1.0,
        ..Default::default()
    };
    let mut terminal = Terminal::new(TestBackend::new(32, 1)).unwrap();
    terminal
        .draw(|frame| {
            let rect = frame.area();
            frame.render_widget(
                Paragraph::new(deck_tab_line(
                    theme,
                    32,
                    Some("Title"),
                    TabIndicators::Numbers,
                    true,
                    0,
                    true,
                    false,
                    true,
                    (false, false),
                    None,
                    (0, 0.0, 0),
                    pulse,
                    false,
                    false,
                    2,
                )),
                rect,
            );
            paint_tab_pulse(frame, rect, theme, pulse, false);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let bg = theme.decrease(theme.background_panel());
    assert_eq!(buffer[(1, 0)].symbol(), "1");
    assert_eq!(
        buffer[(1, 0)].fg,
        tint(theme.hue("interactive", 200).unwrap(), theme.text(), 0.85)
    );
    assert_eq!(buffer[(0, 0)].bg, bg);
    assert_ne!(
        buffer[(22, 0)].bg,
        bg,
        "source running front paints the background"
    );
    assert_eq!(buffer[(31, 0)].bg, bg);
    assert_eq!(buffer[(3, 0)].symbol(), "T");
    assert_eq!(buffer[(3, 0)].fg, theme.text());
    assert!(buffer[(3, 0)].modifier.contains(Modifier::BOLD));
    assert!(!buffer[(22, 0)].modifier.contains(Modifier::BOLD));
    assert_eq!(
        buffer[(30, 0)].symbol(),
        " ",
        "busy hover retains the close guard"
    );

    // Noncompact source rail limits pulse layers to ten cells; the actual
    // second row receives its lower color stop, without invented metadata.
    let mut rail = Terminal::new(TestBackend::new(36, 2)).unwrap();
    let pulse = TabPulseFrame {
        glow: 1.0,
        dim: 0.7,
        ..Default::default()
    };
    rail.draw(|frame| {
        let rect = frame.area();
        frame.render_widget(
            Block::default().style(Style::default().bg(theme.background_panel())),
            rect,
        );
        paint_tab_pulse(frame, rect, theme, pulse, true);
    })
    .unwrap();
    let buffer = rail.backend().buffer();
    assert_ne!(buffer[(1, 0)].bg, buffer[(1, 1)].bg);
    assert_eq!(buffer[(10, 0)].bg, theme.background_panel());
    assert_eq!(buffer[(10, 1)].bg, theme.background_panel());
    assert!(!buffer.content.iter().any(|cell| cell.symbol() != " "));
}

#[tokio::test]
async fn vis39_actual_compact_number_ignition_settles_without_a_hidden_pulse() {
    let mut state = golden_state().await;
    state.chrome.vertical_tabs_width = 6;
    state.chrome.tab_indicators = TabIndicators::Numbers;
    state.chrome.animations = Some(true);
    let mut first = TabPresentation::new(SessionId("running".into()));
    first.busy = true;
    let second = TabPresentation::new(SessionId("selected".into()));
    state.set_tab_strip(vec![first.clone(), second.clone()], 1, false);
    let area = Rect::new(0, 0, 80, 24);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let strip = tab_strip(&state, area).unwrap();
    assert!(strip.vertical && strip.compact);
    let first_cell = (strip.tabs[0].rect.x + 2, strip.tabs[0].rect.y);
    let selected_cell = (strip.tabs[1].rect.x + 2, strip.tabs[1].rect.y);
    let theme = Theme::dark();
    assert_eq!(terminal.backend().buffer()[first_cell].symbol(), "1");
    assert_eq!(
        terminal.backend().buffer()[first_cell].fg,
        theme.hue("interactive", 200).unwrap()
    );
    assert_eq!(terminal.backend().buffer()[selected_cell].symbol(), "2");
    assert_eq!(terminal.backend().buffer()[selected_cell].fg, theme.text());
    assert_eq!(
        state.next_ui_deadline(),
        None,
        "compact numbers mount no U48 pulse or spinner"
    );
    first.busy = false;
    state.set_tab_strip(vec![first, second], 1, false);
    let pulse = state.tab_pulse(0);
    assert_eq!(pulse.whitecap, 0.85);
    assert_eq!(pulse.running, 0.0);
    assert_eq!(pulse.flash, 0.0);
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(
        terminal.backend().buffer()[first_cell].bg,
        theme.background_panel()
    );
    assert_ne!(
        terminal.backend().buffer()[first_cell].fg,
        theme.hue("interactive", 200).unwrap()
    );
    assert!(state.next_ui_deadline().is_some());
    state.tick_ui(std::time::Instant::now() + std::time::Duration::from_millis(700));
    assert_eq!(state.tab_pulse(0).whitecap, 0.0);
    assert_eq!(state.next_ui_deadline(), None);
}

#[test]
fn vis41_styled_fades_use_grapheme_ordinals_and_only_dim_the_generated_dot() {
    let theme = Theme::dark();
    let bg = theme.decrease(theme.background_panel());
    for (offset, expected, dot, alpha) in [(7, "le · A", 3, 0.44), (2, "· titl", 0, 0.92)] {
        let line = deck_tab_line(
            theme,
            11,
            Some("A · title"),
            TabIndicators::Numbers,
            false,
            0,
            true,
            false,
            true,
            (true, false),
            None,
            (offset, 1.0, 0),
            TabPulseFrame::default(),
            false,
            false,
            2,
        );
        let mut terminal = Terminal::new(TestBackend::new(11, 1)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(Paragraph::new(line.clone()), frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(
            (3..9).map(|x| buffer[(x, 0)].symbol()).collect::<String>(),
            expected
        );
        let faded = tint(theme.text(), bg, alpha);
        assert_eq!(
            buffer[(3 + dot, 0)].fg,
            if offset == 7 {
                tint(faded, bg, 0.55)
            } else {
                faded
            }
        );
        for x in 3..9 {
            assert_eq!(buffer[(x, 0)].bg, bg);
            assert!(buffer[(x, 0)].modifier.contains(Modifier::BOLD));
        }
        assert_eq!(buffer[(9, 0)].symbol(), "✕");
        assert!(!buffer[(10, 0)].modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(10, 0)].bg, bg);
    }
}

#[tokio::test]
async fn vis41_busy_hover_scrolls_without_close_and_vertical_compact_is_reachable() {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, SessionId("real-tab".into()));
    state.chrome.animations = Some(false);
    let mut tab = TabPresentation::new(SessionId("real-tab".into()));
    tab.title = Some("abcdefghijklmnopqrstuvwxyz123".into()); // resting fit29, hovered overflow27
    tab.busy = true;
    state.set_tab_strip(vec![tab.clone()], 0, false);
    let area = Rect::new(0, 0, 80, 24);
    let moved = |x, y| MouseEvent {
        kind: MouseEventKind::Moved,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| render(f, &state)).unwrap();
    state.handle_mouse(moved(3, 0), area);
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(1, 0)].symbol(), "⠋");
    assert_eq!(
        state.tab_close_cell(area, 0, tab_strip(&state, area).unwrap().tabs[0].rect),
        None
    );
    assert_eq!(terminal.backend().buffer()[(30, 0)].symbol(), " ");
    let delay = state.next_ui_deadline().unwrap();
    assert!(state.tick_ui(delay));
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(3, 0)].symbol(), "b");
    assert_eq!(
        terminal.backend().buffer()[(1, 0)].symbol(),
        "⠋",
        "off mode only freezes spinner/fade tween"
    );

    state.clear_mouse_position();
    state.chrome.vertical_tabs_width = 42;
    tab.busy = false;
    tab.title = Some("V".repeat(40));
    state.set_tab_strip(vec![tab], 0, false);
    assert!(
        !tab_strip(&state, area).unwrap().vertical,
        "configured vertical80 still falls back to horizontal"
    );
    assert!(
        !tab_strip(&state, Rect::new(0, 0, 105, 24))
            .unwrap()
            .vertical
    );
    assert!(
        tab_strip(&state, Rect::new(0, 0, 106, 24))
            .unwrap()
            .vertical
    );
    let area = Rect::new(0, 0, 120, 24);
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
    let strip = tab_strip(&state, area).unwrap();
    assert!(strip.vertical && !strip.compact);
    assert_eq!(strip.tabs[0].rect, Rect::new(0, 1, 42, 2));
    state.handle_mouse(moved(3, 1), area);
    terminal.draw(|f| render(f, &state)).unwrap();
    let bg = tab_background(Theme::dark(), true, true, true, false);
    assert_eq!(terminal.backend().buffer()[(3, 1)].bg, bg);
    assert_eq!(terminal.backend().buffer()[(3, 2)].bg, bg);
    assert_eq!(terminal.backend().buffer()[(40, 1)].symbol(), "✕");
    assert!(state.next_ui_deadline().is_some());

    state.clear_mouse_position();
    state.chrome.vertical_tabs_width = 6; // actual admitted rail width, still leaves the preferred pane
    let compact = Rect::new(0, 0, 80, 24);
    let strip = tab_strip(&state, compact).unwrap();
    assert!(strip.vertical && strip.compact);
    state.handle_mouse(moved(3, strip.tabs[0].rect.y), compact);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_eq!(
        state.next_ui_deadline(),
        None,
        "compact Infinity cannot start marquee"
    );
    assert_eq!(terminal.backend().buffer()[(2, 2)].symbol(), "V");
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn retained_add_colors_follow_real_pointer_and_eligibility() {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    let mut state = golden_state().await;
    state.set_tab_strip(
        vec![TabPresentation {
            title: Some("Old".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(SessionId("old".into()))
        }],
        0,
        true,
    );
    let area = Rect::new(0, 0, 120, 40);
    let add = tab_strip(&state, area).unwrap().add.unwrap();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let colors = |state: &TuiState, terminal: &mut Terminal<TestBackend>| {
        terminal.draw(|frame| render(frame, state)).unwrap();
        (add.x..add.right())
            .map(|x| {
                let cell = &terminal.backend().buffer()[(x, add.y)];
                (cell.symbol().to_owned(), cell.fg, cell.bg)
            })
            .collect::<Vec<_>>()
    };
    let theme = Theme::dark();
    let idle = colors(&state, &mut terminal);
    assert_eq!(idle[1].0, "+");
    assert_eq!(idle[1].1, theme.text_muted());
    let moved = |x, y| MouseEvent {
        kind: MouseEventKind::Moved,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(moved(add.x + 1, add.y), area);
    let hovered = colors(&state, &mut terminal);
    for (x, (symbol, fg, bg)) in hovered.iter().enumerate() {
        assert_eq!(symbol, &idle[x].0);
        assert_eq!(*fg, theme.text());
        assert_eq!(
            *bg,
            theme.color("background.action.primary.$hovered").unwrap()
        );
    }
    state.handle_mouse(moved(add.right(), add.y), area);
    assert_eq!(colors(&state, &mut terminal), idle);
    state.restore_tab_hover_at((add.right(), add.y, area));
    assert_eq!(colors(&state, &mut terminal), idle);
    state.handle_mouse(moved(add.x + 1, add.y), area);
    state.clear_mouse_position(); // resize invalidates the painted coordinate
    assert_eq!(colors(&state, &mut terminal), idle);
    state.handle_key(KeyAction::Commands).await;
    state.restore_tab_hover_at((add.x + 1, add.y, area));
    assert_eq!(state.mouse_position(), None, "modal cannot restore hover");
    state.close_panel();
    state.handle_mouse(moved(add.x + 1, add.y), area);
    state.set_tab_strip(state.tab_presentation().0.to_vec(), 0, false);
    assert!(tab_strip(&state, area).unwrap().add.is_none());
}

#[tokio::test]
async fn retained_home_promotes_a_single_new_session_slot_and_consumes_a_row() {
    let mut state = golden_state().await;
    state.home = true;
    state.chrome.devtools = Some(false);
    assert!(tab_strip(&state, Rect::new(0, 0, 80, 24)).is_none());
    let baseline = screen(&state, 80, 24);
    assert!(baseline[0].is_empty());
    state.set_tab_strip(
        vec![TabPresentation {
            title: Some("Old".into()),
            home: false,
            busy: true,
            ..TabPresentation::new(SessionId("old".into()))
        }],
        0,
        true,
    );
    let area = Rect::new(0, 0, 80, 24);
    let strip = tab_strip(&state, area).unwrap();
    assert_eq!(strip.tabs.len(), 2);
    assert_eq!(strip.tabs[1].index, 1);
    assert!(strip.add.is_none(), "promoted Home replaces the idle plus");
    let frame = screen(&state, 80, 24);
    assert!(
        frame[0].contains("Old") && frame[0].contains("+ New session"),
        "{}",
        frame[0]
    );
    assert!(frame.join("\n").contains("Ask anything"));
    assert_ne!(baseline, frame);
    assert_eq!(shell_regions(&state, area).session.y, 1);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_eq!(
        terminal.backend().buffer()[(strip.tabs[0].rect.x + 1, 0)].symbol(),
        "⠋"
    );
    assert_eq!(
        terminal.backend().buffer()[(strip.tabs[1].rect.x + 1, 0)].symbol(),
        "+"
    );
    assert!(
        terminal.backend().buffer()[(strip.tabs[1].rect.x + 1, 0)]
            .modifier
            .contains(Modifier::BOLD),
        "the selected Home indicator is bold like the pinned upstream"
    );
    assert!(
        terminal.backend().buffer()[(strip.tabs[1].rect.x + 3, 0)]
            .modifier
            .contains(Modifier::BOLD)
    );
}

#[tokio::test]
async fn close_glyph_only_on_hovered_eligible_tab_and_title_fade_moves_left() {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    let mut state = golden_state().await;
    state.home = true;
    state.set_tab_strip(
        vec![TabPresentation {
            title: Some("abcdefghijklmnopqrstuvwxyz123456789".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(SessionId("long".into()))
        }],
        0,
        false,
    );
    let area = Rect::new(0, 0, 80, 24);
    let strip = tab_strip(&state, area).unwrap();
    let first = strip.tabs[0].rect;
    let home = strip.tabs[1].rect;
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let moved = |x| MouseEvent {
        kind: MouseEventKind::Moved,
        column: x,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_ne!(
        terminal.backend().buffer()[(first.right() - 2, 0)].symbol(),
        "✕"
    );
    assert_ne!(
        terminal.backend().buffer()[(home.right() - 2, 0)].symbol(),
        "✕"
    );
    state.handle_mouse(moved(first.x + 3), area);
    terminal.draw(|f| render(f, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(first.right() - 2, 0)].symbol(), "✕");
    assert_eq!(
        buffer[(first.right() - 2, 0)].fg,
        tint(Theme::dark().text_muted(), Theme::dark().text(), 0.6)
    );
    let hover_bg = Theme::dark()
        .color("background.action.primary.$hovered")
        .unwrap();
    assert_eq!(buffer[(first.right() - 1, 0)].symbol(), " ");
    assert_eq!(
        buffer[(first.right() - 3, 0)].fg,
        tint(Theme::dark().text(), hover_bg, 0.92)
    );
    assert_ne!(buffer[(home.right() - 2, 0)].symbol(), "✕");
    state.handle_mouse(moved(home.x + 3), area);
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_ne!(
        terminal.backend().buffer()[(first.right() - 2, 0)].symbol(),
        "✕"
    );
    assert_eq!(
        terminal.backend().buffer()[(home.right() - 2, 0)].symbol(),
        "✕"
    );
    state.close_panel();
    state.set_tab_strip(
        vec![TabPresentation {
            title: None,
            home: false,
            busy: true,
            ..TabPresentation::new(SessionId("busy".into()))
        }],
        0,
        false,
    );
    state.home = false;
    state.handle_mouse(moved(first.x + 3), area);
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_ne!(
        terminal.backend().buffer()[(first.right() - 2, 0)].symbol(),
        "✕"
    );
    state.set_tab_strip(
        vec![TabPresentation {
            title: Some("Long title".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(SessionId("long".into()))
        }],
        0,
        false,
    );
    let narrow = Rect::new(0, 0, 4, 24);
    state.handle_mouse(moved(2), narrow);
    let mut clipped = Terminal::new(TestBackend::new(4, 24)).unwrap();
    clipped.draw(|f| render(f, &state)).unwrap();
    assert!((0..4).all(|x| clipped.backend().buffer()[(x, 0)].symbol() != "✕"));

    state.handle_mouse(moved(first.x + 3), area);
    state.handle_key(KeyAction::Char('x')).await;
    state.handle_key(KeyAction::Enter).await;
    assert!(state.is_busy());
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_ne!(
        terminal.backend().buffer()[(first.right() - 2, 0)].symbol(),
        "✕"
    );
}

#[tokio::test]
async fn narrow_deck_overflow_and_two_digit_numbers_use_painted_rectangles() {
    let mut state = golden_state().await;
    state.chrome.tab_indicators = TabIndicators::Numbers;
    state.set_tab_strip(
        (0..12)
            .map(|i| TabPresentation {
                title: Some(format!("Tab {i}")),
                home: false,
                busy: false,
                ..TabPresentation::new(SessionId(format!("tab-{i}")))
            })
            .collect(),
        10,
        true,
    );
    let area = Rect::new(0, 0, 31, 24);
    let strip = tab_strip(&state, area).unwrap();
    assert!(strip.before_marker.is_some() && strip.after_marker.is_some());
    let active = strip.tabs.iter().find(|t| t.index == 10).unwrap().rect;
    let mut terminal = Terminal::new(TestBackend::new(31, 24)).unwrap();
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(active.x, 0)].symbol(), "1");
    assert_eq!(terminal.backend().buffer()[(active.x + 1, 0)].symbol(), "1");
    assert_eq!(terminal.backend().buffer()[(active.x + 3, 0)].symbol(), "T");
    assert_eq!(strip.hit_test(strip.before_marker.unwrap().x, 0), None);
    assert_eq!(strip.hit_test(strip.after_marker.unwrap().x, 0), None);
}

#[test]
fn selected_tab_fades_only_the_last_four_visible_overflow_graphemes() {
    let theme = Theme::dark();
    let tab_bg = theme.decrease(theme.background_panel());
    let mut terminal = Terminal::new(TestBackend::new(40, 2)).unwrap();
    terminal
        .draw(|frame| {
            render_tabs(
                frame,
                theme,
                Rect::new(0, 0, 40, 1),
                Some("abcdefghijklmnopqrstuvwxyz123456789"),
                TabIndicators::Numbers,
                false,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    for x in 3..28 {
        assert_eq!(buffer[(x, 0)].fg, theme.text(), "cell {x}");
        assert!(buffer[(x, 0)].modifier.contains(Modifier::BOLD));
    }
    for (x, symbol, color) in [
        (28, "z", 0xc4),
        (29, "1", 0x92),
        (30, "2", 0x61),
        (31, "3", 0x2f),
    ] {
        assert_eq!(buffer[(x, 0)].symbol(), symbol);
        assert_eq!(buffer[(x, 0)].fg, Color::Rgb(color, color, color));
        assert_eq!(buffer[(x, 0)].bg, tab_bg);
        assert!(buffer[(x, 0)].modifier.contains(Modifier::BOLD));
    }
    assert_eq!(buffer[(1, 0)].symbol(), "1");
    assert_eq!(buffer[(1, 0)].fg, tint(theme.text(), tab_bg, 0.25));
    // Upstream TabIndicator gives the selected label the title's bold
    // attributes (also true for the synthetic Home `+` indicator).
    assert!(buffer[(1, 0)].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(32, 0)].symbol(), " ");
    assert_ne!(buffer[(32, 0)].bg, tab_bg);
}

#[test]
fn selected_tab_short_and_exact_titles_stay_bright_and_padding_stays_blank() {
    let theme = Theme::dark();
    let exact = "a".repeat(29);
    for (width, title) in [(32, "Short"), (32, exact.as_str())] {
        let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
        terminal
            .draw(|frame| {
                render_tabs(
                    frame,
                    theme,
                    Rect::new(0, 0, width, 1),
                    Some(title),
                    TabIndicators::Status,
                    false,
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        for x in 3..(3 + title.len() as u16) {
            assert_eq!(buffer[(x, 0)].fg, theme.text(), "{title} cell {x}");
        }
        if title == "Short" {
            assert_eq!(buffer[(8, 0)].symbol(), " ");
            assert_eq!(buffer[(8, 0)].bg, theme.decrease(theme.background_panel()));
            assert!(!buffer[(8, 0)].modifier.contains(Modifier::BOLD));
        }
    }
    let mut terminal = Terminal::new(TestBackend::new(7, 1)).unwrap();
    terminal
        .draw(|frame| {
            render_tabs(
                frame,
                theme,
                Rect::new(0, 0, 7, 1),
                Some("overflows"),
                TabIndicators::Status,
                false,
            );
        })
        .unwrap();
    for x in 3..7 {
        assert_eq!(terminal.backend().buffer()[(x, 0)].fg, theme.text());
    }
    let exact_unicode = format!("{}e\u{301}界", "a".repeat(26));
    let mut terminal = Terminal::new(TestBackend::new(32, 1)).unwrap();
    terminal
        .draw(|frame| {
            render_tabs(
                frame,
                theme,
                Rect::new(0, 0, 32, 1),
                Some(&exact_unicode),
                TabIndicators::Status,
                false,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(29, 0)].symbol(), "e\u{301}");
    assert_eq!(buffer[(30, 0)].symbol(), "界");
    assert_eq!(buffer[(30, 0)].fg, theme.text());
}

#[test]
fn selected_tab_clips_whole_unicode_graphemes_at_cell_boundary() {
    let theme = Theme::dark();
    let mut terminal = Terminal::new(TestBackend::new(32, 1)).unwrap();
    let title = format!("{}e\u{301}界ZQRmore", "a".repeat(23));
    terminal
        .draw(|frame| {
            render_tabs(
                frame,
                theme,
                Rect::new(0, 0, 32, 1),
                Some(&title),
                TabIndicators::Status,
                false,
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(26, 0)].symbol(), "e\u{301}");
    assert_eq!(buffer[(26, 0)].fg, theme.text());
    assert_eq!(buffer[(27, 0)].symbol(), "界");
    for (x, color) in [(27, 0xc4), (29, 0x92), (30, 0x61), (31, 0x2f)] {
        assert_eq!(buffer[(x, 0)].fg, Color::Rgb(color, color, color));
    }
    assert_eq!(buffer[(29, 0)].symbol(), "Z");
    assert_eq!(buffer[(30, 0)].symbol(), "Q");
    assert_eq!(buffer[(31, 0)].symbol(), "R");

    let title = format!("{}界tail", "a".repeat(28));
    terminal
        .draw(|frame| {
            render_tabs(
                frame,
                theme,
                Rect::new(0, 0, 32, 1),
                Some(&title),
                TabIndicators::Status,
                false,
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(30, 0)].symbol(), "a");
    assert_eq!(buffer[(31, 0)].symbol(), " ");
    assert_eq!(buffer[(31, 0)].bg, theme.decrease(theme.background_panel()));
    assert!(!buffer[(31, 0)].modifier.contains(Modifier::BOLD));
}

#[tokio::test]
async fn v03_tall_viewport_and_short_top_placement() {
    let mut state = golden_state().await;
    let short = screen(&state, 120, 80);
    assert!(
        short[3].contains("hello"),
        "short history starts below one-row top padding"
    );
    let text = (0..70).map(|i| format!("ROW-{i:03}\n")).collect::<String>();
    state.attach_page(&page(vec![msg(1, Role::Assistant, &text)]));
    let tall = screen(&state, 120, 80);
    assert!(tall.iter().filter(|r| r.contains("ROW-")).count() > 35);
    let wide = screen(&state, 160, 48);
    assert!(wide.iter().any(|r| r.contains("Context")), "actual sidebar");
}

#[tokio::test]
async fn scrolled_resize_keeps_top_row_and_draft_while_bottom_stays_pinned() {
    let mut state = golden_state().await;
    state.chrome.devtools = Some(false);
    let text = (0..90).map(|i| format!("ROW-{i:03}\n")).collect::<String>();
    state.attach_page(&page(vec![msg(1, Role::Assistant, &text)]));
    for key in "draft".chars() {
        state.handle_key(KeyAction::Char(key)).await;
    }
    state.handle_key(KeyAction::Left).await;
    let draft = state.input().to_string();
    let caret = state.prompt_layout(60).1;
    let first_row = |frame: &[String]| {
        frame.iter().find_map(|line| {
            line.split("ROW-")
                .nth(1)
                .and_then(|tail| tail.get(..3))
                .and_then(|number| number.parse::<usize>().ok())
        })
    };

    let pinned = screen(&state, 160, 48);
    assert!(pinned.join("\n").contains("ROW-089"));
    let smaller_pinned = screen(&state, 80, 24);
    assert!(smaller_pinned.join("\n").contains("ROW-089"));
    assert_eq!(state.scroll(), 0);

    let _ = screen(&state, 160, 48);
    while first_row(&screen(&state, 160, 48)) != Some(41) {
        assert!(state.scroll() < 90, "expected to reach ROW-041");
        state.scroll_transcript(true);
    }
    let wide = screen(&state, 160, 48);
    assert_eq!(first_row(&wide), Some(41));
    let narrow = screen(&state, 80, 24);
    assert_eq!(
        first_row(&narrow),
        Some(41),
        "shrink must not jump to the bottom"
    );
    assert!(narrow.join("\n").contains("ROW-055"), "{narrow:?}");
    assert!(narrow.join("\n").contains("draft"));
    state.scroll_transcript(false);
    assert_eq!(first_row(&screen(&state, 80, 24)), Some(42));
    state.scroll_transcript(true);
    assert_eq!(first_row(&screen(&state, 80, 24)), Some(41));
    let restored = screen(&state, 160, 48);
    assert_eq!(first_row(&restored), Some(41));
    assert!(restored.join("\n").contains("ROW-079"), "{restored:?}");
    assert_eq!(state.input(), draft);
    assert_eq!(state.prompt_layout(60).1, caret);

    while first_row(&screen(&state, 160, 48)) != Some(0) {
        assert!(state.scroll() < 95, "expected to reach the top edge");
        state.scroll_transcript(true);
    }
    assert_eq!(first_row(&screen(&state, 80, 24)), Some(0));
    assert_eq!(first_row(&screen(&state, 160, 48)), Some(0));
}

#[tokio::test]
async fn v03_paste_keeps_full_draft_behind_compact_prompt() {
    let mut state = golden_state().await;
    state.handle_paste("draft-one\ndraft-two\ndraft-three");
    let frame = screen(&state, 80, 24).join("\n");
    assert_eq!(state.input(), "draft-one\ndraft-two\ndraft-three");
    assert!(frame.contains("[Pasted ~3 lines]"));
    assert!(!frame.contains("draft-one"));
    assert!(!frame.contains("draft-two"));
    assert!(!frame.contains("draft-three"));
}

#[tokio::test]
async fn v03_rendered_row_scroll_and_chrome_conditions() {
    let mut state = golden_state().await;
    let long = format!(
        "FIRST-ANCHOR {} LAST-ANCHOR",
        "wrapped payload ".repeat(800)
    );
    state.attach_page(&page(vec![msg(1, Role::Assistant, &long)]));
    assert!(!screen(&state, 80, 40).join("\n").contains("FIRST-ANCHOR"));
    for _ in 0..300 {
        state.scroll_transcript(true);
    }
    assert!(screen(&state, 80, 40).join("\n").contains("FIRST-ANCHOR"));
    let detached = state.scroll();
    state.handle_paste("draft-one\ndraft-two");
    for width in [80, 120, 160, 43, 44, 119, 120, 121, 160] {
        let frame = screen(&state, width, 48).join("\n");
        assert_eq!(frame.contains("Context"), width > 120, "{width}");
        assert_eq!(state.scroll(), detached);
        assert!(frame.contains("draft-two"));
    }
    let displayed = state.display_scroll();
    assert!(
        state.scroll() > displayed,
        "resize clamps the retained request"
    );
    state.scroll_transcript(false);
    assert_eq!(state.scroll(), displayed - 1);
    state.chrome.sidebar_hidden = true;
    assert!(!screen(&state, 160, 48).join("\n").contains("Context"));
    state.chrome.sidebar_hidden = false;
    state.parent_id = Some("parent".into());
    assert!(!screen(&state, 160, 48).join("\n").contains("Context"));
    state.parent_id = None;
    state.chrome.vertical_tabs_width = 42;
    assert!(!screen(&state, 160, 48).join("\n").contains("Context"));
    assert!(screen(&state, 163, 48).join("\n").contains("Context"));
    state.chrome.devtools = Some(false);
    let hidden = screen(&state, 120, 40);
    assert!(!hidden.join("\n").contains("Native runtime"));
    assert!(hidden[38].contains("ctrl+p commands"));
    state.chrome.devtools = Some(true);
    let shown = screen(&state, 120, 40);
    assert!(shown[39].contains("Native runtime"));
    assert!(shown[37].contains("ctrl+p commands"));
}

#[tokio::test]
async fn context_footer_uses_reported_generation_without_billing_missing_rounds() {
    use oc_core::queries::{HistoryTurn, TranscriptPart};

    let mut state = golden_state().await;
    let mut snapshot = catalog();
    snapshot.models[0].context = 225_000;
    state.apply_catalog(snapshot);
    assert_eq!(state.context_usage(), None);

    let mut answer = msg(3, Role::Assistant, "answer");
    answer.turn = Some(HistoryTurn {
        model_label: "fixture".into(),
        parts: vec![TranscriptPart::Text("answer".into())],
        usage: None,
        context_usage: Some((6000, 763)),
        streamed_ms: Some(1000),
        ..Default::default()
    });
    state.attach_page(&page(vec![answer.clone()]));
    assert_eq!(state.context_usage(), Some((6763, Some(225_000))));
    assert!(
        footer_line(&state, Theme::dark(), 116, 120)
            .to_string()
            .contains("6.8K (3%)")
    );
    let meta = state
        .history()
        .rows()
        .last()
        .and_then(|r| r.meta.as_ref())
        .unwrap();
    assert_eq!(meta.input_tokens, None);
    assert_eq!(meta.output_tokens, None);
    assert!(
        !state
            .transcript_lines(116, 120)
            .iter()
            .any(|line| line.plain_text().contains("tok/s"))
    );

    // TurnFinished carries no separate context event: the durable page
    // replaces the completed live footer with the same reported pair.
    let turn = oc_core::core_app::WorkerTurnId("context-turn".into());
    state.begin_compress_turn(turn.clone());
    state.apply_finished(&turn, "answer", 100);
    state.attach_page(&page(vec![answer.clone()]));
    assert_eq!(state.context_usage(), Some((6763, Some(225_000))));
    assert!(
        footer_line(&state, Theme::dark(), 116, 120)
            .to_string()
            .contains("6.8K (3%)")
    );

    answer.turn.as_mut().unwrap().usage = Some((1200, 300));
    state.attach_page(&page(vec![answer.clone()]));
    assert_eq!(state.context_usage(), Some((6763, Some(225_000))));
    let meta = state
        .history()
        .rows()
        .last()
        .and_then(|r| r.meta.as_ref())
        .unwrap();
    assert_eq!(meta.input_tokens, Some(1200));
    assert_eq!(meta.output_tokens, Some(300));
    let transcript = state
        .transcript_lines(116, 120)
        .iter()
        .map(|line| line.plain_text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(transcript.contains("300.0 tok/s"), "{transcript}");

    answer.turn.as_mut().unwrap().context_usage = None;
    state.attach_page(&page(vec![answer.clone()]));
    assert_eq!(state.context_usage(), Some((1500, Some(225_000))));
    answer.turn.as_mut().unwrap().usage = None;
    state.attach_page(&page(vec![answer]));
    assert_eq!(state.context_usage(), None);
}

#[tokio::test]
async fn v03_sidebar_uses_dto_title_usage_and_styled_boundary() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut state = golden_state().await;
    state.chrome.devtools = Some(false);
    state.chrome.location = Some("/workspace/real-project".into());
    state.session_title = Some("Actual title".into());
    state.begin_compress_turn(oc_core::core_app::WorkerTurnId("usage".into()));
    state.close_panel(); // inspect the base chrome, without the modal backdrop
    state.apply_usage(
        &oc_core::core_app::WorkerTurnId("usage".into()),
        300,
        20,
        100,
    );
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    terminal.draw(|f| render(f, &state)).unwrap();
    let b = terminal.backend().buffer();
    assert_eq!(b[(117, 10)].bg, Theme::dark().background());
    assert_eq!(b[(118, 10)].bg, Theme::dark().background_panel());
    let text = screen(&state, 160, 48).join("\n");
    for value in [
        "Actual title",
        "320 tokens",
        "32% used",
        "/workspace/real-project",
    ] {
        assert!(text.contains(value), "{value}");
    }
    for width in [43, 44] {
        let footer = footer_line(&state, Theme::dark(), width - 4, width).to_string();
        assert!(footer.contains("320 (32%)"));
        assert!(!footer.contains("ctrl+p"));
    }
    state.parent_id = Some("parent".into());
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_eq!(
        terminal.backend().buffer()[(118, 10)].bg,
        Theme::dark().background()
    );
}

#[tokio::test]
async fn sidebar_padding_keeps_canvas_foreground_while_labels_are_styled() {
    let mut state = golden_state().await;
    state.chrome.devtools = Some(false);
    state.chrome.location = Some("/workspace/real-project".into());
    state.session_title = Some("Actual title".into());
    state.begin_compress_turn(oc_core::core_app::WorkerTurnId("usage".into()));
    state.close_panel();
    state.apply_usage(
        &oc_core::core_app::WorkerTurnId("usage".into()),
        300,
        20,
        100,
    );
    let theme = Theme::dark();
    let white = Color::Rgb(255, 255, 255);
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let sidebar = shell_regions(&state, Rect::new(0, 0, 160, 48)).session;
    let sidebar = Rect::new(
        sidebar.right() - layout::SESSION_SIDEBAR_WIDTH,
        sidebar.y,
        layout::SESSION_SIDEBAR_WIDTH,
        sidebar.height,
    );
    let toast = toast_rect(&state, Rect::new(0, 0, 160, 48));
    let labels = [
        ("Actual title", theme.text(), true),
        ("Context", theme.text(), true),
        ("320 tokens", theme.text_muted(), false),
        ("32% used", theme.text_muted(), false),
        ("/workspace/real-project", theme.text_muted(), false),
    ];
    let mut muted_spaces = Vec::new();
    for (label, foreground, bold) in labels {
        let cells = (sidebar.y..sidebar.bottom()).flat_map(|y| {
            (sidebar.x..sidebar.right()).filter_map(move |x| {
                (x + label.len() as u16 <= sidebar.right()
                    && buffer[(x, y)].symbol() == &label[..1])
                    .then_some((x, y))
            })
        });
        let (x, y) = cells
            .into_iter()
            .find(|&(x, y)| {
                label
                    .chars()
                    .enumerate()
                    .all(|(index, ch)| buffer[(x + index as u16, y)].symbol() == ch.to_string())
            })
            .unwrap_or_else(|| panic!("missing sidebar label {label}"));
        for (index, glyph) in label.chars().enumerate() {
            let x = x + index as u16;
            let cell = &buffer[(x, y)];
            if glyph == ' ' && foreground == theme.text_muted() {
                muted_spaces.push((x, y));
                continue;
            }
            assert_eq!(cell.fg, foreground, "{label} foreground at ({x},{y})");
            assert_eq!(cell.modifier.contains(Modifier::BOLD), bold, "{label} bold");
        }
    }
    let mut blanks = 0;
    for y in sidebar.y..sidebar.bottom() {
        for x in sidebar.x..sidebar.right() {
            let cell = &buffer[(x, y)];
            if cell.symbol() == " "
                    && cell.bg == theme.background_panel()
                    && !muted_spaces.contains(&(x, y))
                    && !toast.is_some_and(|rect| rect.contains((x, y).into()))
                    // Ratatui fills the remainder of styled label rows with
                    // their text foreground; the label cells are checked above.
                    && !(sidebar.y + 1..=sidebar.y + 5).contains(&y)
                    && y != sidebar.bottom() - 2
            {
                assert_eq!(cell.fg, white, "sidebar blank at ({x},{y})");
                blanks += 1;
            }
        }
    }
    assert!(blanks > 1609, "expected the predominantly empty sidebar");
}

#[tokio::test]
async fn paired_wrapped_sidebar_title_styles_separator_not_final_tail() {
    let mut state = golden_state().await;
    // Pinned VIS05 scroll-away: the title wraps just before "ассистенту".
    let title = "Какие инструменты доступны ассистенту";
    state.session_title = Some(title.into());
    state.chrome.devtools = Some(false);
    state.close_panel();
    let theme = Theme::dark();
    let white = Color::Rgb(255, 255, 255);
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let session = shell_regions(&state, Rect::new(0, 0, 160, 48)).session;
    let sidebar_x = session.right() - layout::SESSION_SIDEBAR_WIDTH;
    let title_x = sidebar_x + 2;
    let first_y = session.y + 1;
    let first = "Какие инструменты доступны";
    let second = "ассистенту";
    for (row, expected) in [(first_y, first), (first_y + 1, second)] {
        let printed = (title_x..session.right() - 2)
            .map(|x| buffer[(x, row)].symbol().to_string())
            .collect::<String>();
        assert_eq!(printed.trim_end(), expected);
    }
    let separator = &buffer[(title_x + text_width(first) as u16, first_y)];
    assert_eq!(separator.symbol(), " ");
    assert_eq!(separator.fg, theme.text());
    assert!(separator.modifier.contains(Modifier::BOLD));
    let final_tail = &buffer[(title_x + text_width(second) as u16, first_y + 1)];
    assert_eq!(final_tail.symbol(), " ");
    assert_eq!(final_tail.fg, white);
    assert!(!final_tail.modifier.contains(Modifier::BOLD));
    for (row, after) in [
        (first_y, title_x + text_width(first) as u16 + 1),
        (first_y + 1, title_x + text_width(second) as u16),
    ] {
        for x in after..session.right() - 2 {
            let cell = &buffer[(x, row)];
            assert_eq!(cell.symbol(), " ", "({x},{row})");
            assert_eq!(cell.fg, white, "({x},{row})");
            assert!(!cell.modifier.contains(Modifier::BOLD), "({x},{row})");
        }
    }
}

#[tokio::test]
async fn hyphenated_sidebar_title_wraps_like_paired_121x40_frame() {
    let mut state = golden_state().await;
    state.session_title = Some("OVERLAPTITLE-".repeat(8));
    state.chrome.devtools = Some(false);
    let mut terminal = Terminal::new(TestBackend::new(121, 40)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let session = shell_regions(&state, Rect::new(0, 0, 121, 40)).session;
    let title_x = session.right() - layout::SESSION_SIDEBAR_WIDTH + 2;
    let title_y = session.y + 1;
    let expected = "OVERLAPTITLE-OVERLAPTITLE-";
    assert_eq!(title_x, 81);
    assert_eq!(title_y, 2);
    for y in title_y..title_y + 4 {
        let row = (title_x..title_x + expected.len() as u16)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>();
        assert_eq!(row, expected, "title row {y}");
        let tail = &buffer[(title_x + expected.len() as u16, y)];
        assert_eq!(tail.symbol(), " ");
        assert!(!tail.modifier.contains(Modifier::BOLD), "hyphen row {y}");
    }
    let context = (title_x..title_x + 7)
        .map(|x| buffer[(x, 7)].symbol())
        .collect::<String>();
    assert_eq!(context, "Context");
    assert_eq!(buffer[(title_x, 6)].symbol(), " ");

    // A hyphen break and an unbreakable title both keep graphemes intact.
    assert_eq!(
        wrap_sidebar_title("e\u{301}-e\u{301}-e\u{301}-", 4),
        [
            ("e\u{301}-e\u{301}-".into(), false),
            ("e\u{301}-".into(), false)
        ]
    );
    assert_eq!(
        wrap_sidebar_title(&"🧑‍💻".repeat(3), 3),
        vec![("🧑‍💻".into(), false); 3]
    );
}

#[tokio::test]
async fn hard_wrapped_unicode_sidebar_title_has_no_separator_cell() {
    let mut state = golden_state().await;
    state.session_title = Some("界".repeat(22));
    let theme = Theme::dark();
    let white = Color::Rgb(255, 255, 255);
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                Block::default().style(Style::default().fg(white).bg(theme.background())),
                frame.area(),
            );
            render_sidebar(frame, &state, theme, frame.area());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    // 17 double-width glyphs fill the first 34-cell wrapped row; five
    // remain on the final row. Neither wrap splits at a space.
    assert_eq!(buffer[(36, 1)].fg, white);
    assert_eq!(buffer[(12, 2)].symbol(), " ");
    assert_eq!(buffer[(12, 2)].fg, white);
    assert!(!buffer[(12, 2)].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(13, 2)].fg, white);
}

/// The buffer rows with trailing spaces removed, so snapshots stay small
/// enough to review while leading padding is still asserted.
fn screen(state: &TuiState, width: u16, height: u16) -> Vec<String> {
    crate::views::render_test(state, width, height)
        .into_iter()
        .map(|row| row.trim_end().to_string())
        .collect()
}

fn right_aligned(text: &str, width: usize) -> String {
    format!("{}{text}", " ".repeat(width.saturating_sub(text.len())))
}

#[test]
fn footer_clipping_preserves_styled_graphemes_and_cell_budget() {
    let first = Style::default().fg(Color::Red);
    let second = Style::default().fg(Color::Blue);
    let source = || {
        vec![
            Span::styled("a\u{301}界", first),
            Span::styled("👩\u{200d}💻z", second),
        ]
    };
    let clipped = clip_footer_spans(source(), 4);
    assert_eq!(Line::from(clipped.clone()).width(), 3);
    assert_eq!(clipped.len(), 1, "a wide grapheme cannot use the last cell");
    assert_eq!(clipped[0].content, "a\u{301}界");
    assert_eq!(clipped[0].style, first);
    let clipped = clip_footer_spans(source(), 5);
    assert_eq!(Line::from(clipped.clone()).width(), 5);
    assert_eq!(clipped[1].content, "👩\u{200d}💻");
    assert_eq!(clipped[1].style, second);
    assert!(clip_footer_spans(source(), 0).is_empty());
}

#[tokio::test]
async fn vis28_narrow_running_footer_keeps_right_hints_and_dynamic_usage() {
    let mut state = golden_state().await;
    let turn = oc_core::core_app::WorkerTurnId("footer-clip".into());
    state.begin_compress_turn(turn.clone());
    state.close_panel();
    let hints = "shift+tab agents  ctrl+p commands";
    for animations in [true, false] {
        state.chrome.animations = Some(animations);
        for (width, height) in [(43, 24), (44, 24), (80, 24), (120, 40)] {
            let rows = screen(&state, width, height);
            let y = if height == 24 { 21 } else { 37 };
            let row = &rows[y];
            let indicator = if animations {
                "■⬝⬝⬝⬝⬝⬝⬝"
            } else {
                "[⋯]"
            };
            let running = format!("   {indicator} esc interrupt");
            if width == 43 {
                assert!(
                    row.starts_with(&format!("  {indicator} esc interrupt")),
                    "{width} {animations}: {row}"
                );
                assert!(!row.contains("ctrl+p"));
            } else if width == 44 {
                let clipped = if animations {
                    "   ■⬝⬝⬝"
                } else {
                    "   [⋯] "
                };
                assert_eq!(row, &format!("{clipped}  {hints}"));
            } else {
                assert_eq!(
                    row,
                    &format!(
                        "{running}{}",
                        right_aligned(
                            hints,
                            (width - 2) as usize - UnicodeWidthStr::width(running.as_str())
                        )
                    )
                );
            }
            assert!(UnicodeWidthStr::width(row.as_str()) <= width as usize);

            if width >= 44 {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| render(frame, &state)).unwrap();
                let buffer = terminal.backend().buffer();
                let hint_x = width - 2 - hints.len() as u16;
                assert_eq!(buffer[(hint_x, y as u16)].symbol(), "s");
                assert_eq!(buffer[(width - 3, y as u16)].symbol(), "s");
                assert_eq!(buffer[(hint_x + 18, y as u16)].fg, Theme::dark().text());
            }
        }
    }

    state.apply_usage(&turn, 300, 20, 100);
    for width in [44, 80, 160] {
        let height = if width == 160 { 48 } else { 24 };
        let area = Rect::new(0, 0, width, height);
        let footer = session_regions(
            &state,
            session_main(&state, shell_regions(&state, area).session),
            width,
        )
        .footer;
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let usage = "320 (32%)";
        let suffix = if width == 44 {
            usage.to_string()
        } else {
            format!("{usage}  ctrl+p commands")
        };
        let start = footer.right() - suffix.len() as u16;
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(start, footer.y)].symbol(), "3", "width={width}");
        assert_eq!(
            buffer[(footer.right() - 1, footer.y)].symbol(),
            if width == 44 { ")" } else { "s" }
        );
        let line = footer_line(&state, Theme::dark(), footer.width, width);
        assert!(line.width() <= footer.width as usize);
        assert!(line.to_string().ends_with(&suffix));
    }
    state.apply_finished(&turn, "", 0);
    let mut idle_state = golden_state().await;
    idle_state.chrome.location = Some("/workspace/project".into());
    let idle = screen(&idle_state, 44, 24);
    assert!(idle[21].contains("ctrl+p commands"));
    assert!(idle[21].starts_with("  /…/p"), "{}", idle[21]);
    assert!(screen(&idle_state, 80, 24)[21].contains("/workspace/project"));
}

#[tokio::test]
async fn vis28_footer_scanner_tracks_agent_and_stays_below_metadata() {
    use std::time::{Duration, Instant};

    let theme = Theme::dark();
    let mut state = golden_state().await;
    let mut snapshot = catalog();
    snapshot.agents[0].color_index = 3;
    state.apply_catalog(snapshot);
    let turn = oc_core::core_app::WorkerTurnId("scanner-turn".into());
    let idle = screen(&state, 80, 24);
    assert!(!idle.join("\n").contains("■"));
    state.begin_compress_turn(turn.clone());
    state.close_panel();
    let now = Instant::now();
    assert!(!state.tick_scanner(now));
    let start = screen(&state, 80, 24);
    assert!(
        start[21].starts_with("   ■⬝⬝⬝⬝⬝⬝⬝ esc interrupt"),
        "{start:?}"
    );
    assert_eq!(start[20], idle[20], "agent/model row must not shift");
    assert!(!state.tick_scanner(now + Duration::from_millis(39)));
    assert!(state.tick_scanner(now + Duration::from_millis(40)));
    let next = screen(&state, 80, 24);
    assert!(
        next[21].starts_with("   ■■⬝⬝⬝⬝⬝⬝ esc interrupt"),
        "{next:?}"
    );
    assert_eq!(next[20], start[20]);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(
        terminal.backend().buffer()[(4, 21)].fg,
        theme.categorical_agents()[3]
    );

    state.chrome.animations = Some(false);
    let still = state.scanner_frame();
    assert!(!state.tick_scanner(now + Duration::from_secs(2)));
    assert_eq!(state.scanner_frame(), still);
    let off = screen(&state, 80, 24);
    assert!(off[21].starts_with("   [⋯] esc interrupt"), "{off:?}");
    assert_eq!(off[20], start[20]);
    state.handle_key(KeyAction::Cancel).await;
    let armed = screen(&state, 80, 24);
    assert!(
        armed[21].starts_with("   [⋯] esc again to interrupt"),
        "{armed:?}"
    );
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(7, 21)].fg, theme.warning());
    assert_eq!(terminal.backend().buffer()[(11, 21)].fg, theme.warning());
    state.chrome.animations = Some(true);
    state.chrome.service_diagnostics_omitted = 2;
    let with_issues = screen(&state, 80, 24);
    assert!(
        with_issues[21].contains("esc again to interrupt"),
        "service status hid the foreground interrupt guard: {with_issues:?}"
    );
    assert!(with_issues[21].contains("2 issues"));
    state.chrome.service_diagnostics_omitted = 0;
    state.chrome.animations = Some(false);
    state.apply_finished(&turn, "", 0);
    assert!(!screen(&state, 80, 24)[21].contains("[⋯]"));
    assert!(!screen(&state, 80, 24)[21].contains("esc interrupt"));
    let cancelled = oc_core::core_app::WorkerTurnId("cancelled".into());
    state.begin_compress_turn(cancelled.clone());
    state.close_panel();
    state.apply_interrupted(&cancelled, "", 0);
    assert!(!screen(&state, 80, 24)[21].contains("[⋯]"));

    // No selected agent: use the theme's border.base rather than a label
    // lookup or the first categorical agent color.
    let mut snapshot = catalog();
    snapshot.agent_id = None;
    state.apply_catalog(snapshot);
    state.chrome.animations = Some(true);
    state.begin_compress_turn(oc_core::core_app::WorkerTurnId("second".into()));
    state.close_panel();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(3, 21)].fg, theme.border());
}

#[tokio::test]
async fn v02_auto_marker_is_session_capability_not_agent_rules() {
    use oc_core::queries::AutoAcceptState;
    let mut state = golden_state().await;
    for (mode, visible) in [
        (AutoAcceptState::Unsupported, false),
        (AutoAcceptState::Disabled, false),
        (AutoAcceptState::Enabled, true),
    ] {
        let mut snapshot = catalog();
        snapshot.auto_accept = mode;
        snapshot.agents[0].color_index = 3;
        state.apply_catalog(snapshot);
        let line = super::metadata_line(&state, crate::theme::Theme::dark(), 111, 120).unwrap();
        assert_eq!(line.to_string().contains("auto"), visible);
        assert_eq!(
            line.spans[0].style.fg,
            Some(crate::theme::Theme::dark().categorical_agents()[3])
        );
    }
    state.reset_workspace();
    assert_eq!(state.auto_accept, AutoAcceptState::Unsupported);
}

#[tokio::test]
async fn no_selected_variant_has_no_metadata_label_or_overlay() {
    let mut state = golden_state().await;
    let mut snapshot = catalog();
    snapshot.variant = None;
    state.apply_catalog(snapshot);
    for width in [44, 80, 120, 160] {
        let metadata = metadata_line(&state, Theme::dark(), width, width).unwrap();
        assert_eq!(metadata.to_string(), "X · a ludka2");
    }
    assert!(
        state
            .picker
            .as_ref()
            .unwrap()
            .selection()
            .unwrap()
            .variant
            .is_none()
    );
}

#[tokio::test]
async fn prompt_metadata_fits_the_painted_row_at_43_44_and_120() {
    let mut state = golden_state().await;
    let mut snapshot = catalog();
    snapshot.models[0].display_name = "MiMo-V2.6-Flash".into();
    snapshot.models[0].provider_name = "OpenCode".into();
    snapshot.variant = Some("Free".into());
    snapshot.models[0].variants = vec![VariantEntry {
        name: "Free".into(),
        disabled: false,
        reasoning_effort: None,
    }];
    snapshot.agents[0].id = "Reader".into();
    snapshot.agent_id = Some("Reader".into());
    state.apply_catalog(snapshot);
    state.chrome.devtools = Some(false);
    for (width, expected) in [
        (43, "MiMo-V2.6-Flash · Free"),
        (44, "Reader · MiMo-V2.6-Flash · Free"),
        (120, "Reader · MiMo-V2.6-Flash OpenCode · Free"),
    ] {
        let rows = screen(&state, width, 48);
        let row = rows.iter().find(|row| row.contains("MiMo-")).unwrap();
        assert!(row.contains(expected), "{width}: {row}");
        assert!(!row.contains("OpenC") || width == 120, "{width}: {row}");
    }
    let mut terminal = Terminal::new(TestBackend::new(44, 48)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(5, 44)].fg, state.agent_color(Some("Reader")));
    assert_eq!(buffer[(14, 44)].fg, Theme::dark().text());
    assert_eq!(buffer[(33, 44)].fg, Theme::dark().warning());
    assert_eq!(buffer[(39, 44)].fg, Color::Rgb(255, 255, 255));
    assert_eq!(buffer[(39, 44)].symbol(), " ");
}

#[tokio::test]
async fn prompt_metadata_candidates_and_grapheme_fallback() {
    let mut state = golden_state().await;
    let mut snapshot = catalog();
    snapshot.models[0].display_name = "模型 Very Long Model Name".into();
    snapshot.models[0].provider_name = "Organization / Short".into();
    snapshot.variant = None;
    snapshot.auto_accept = oc_core::queries::AutoAcceptState::Enabled;
    state.apply_catalog(snapshot);
    let theme = Theme::dark();
    let metadata = |width| {
        metadata_line(&state, theme, width, 120)
            .unwrap()
            .to_string()
    };
    assert_eq!(
        metadata(60),
        "X auto · 模型 Very Long Model Name Organization / Short"
    );
    assert_eq!(
        metadata(50),
        "X · 模型 Very Long Model Name Organization / Short"
    );
    assert_eq!(metadata(39), "X · 模型 Very Long Model Name Short");
    assert_eq!(metadata(32), "X · 模型 Very Long Model Name");
    assert_eq!(metadata(13), "X · 模型 Ver…");
    assert_eq!(UnicodeWidthStr::width(metadata(13).as_str()), 13);
    // Fit exactly the painted label, preserving the donor's word boundaries.
    for (id, label) in [
        ("build", "Build"),
        ("build-yolo", "Build-Yolo"),
        ("ßeta", "ßEta"),
    ] {
        let mut snapshot = catalog();
        snapshot.agents[0].id = id.into();
        snapshot.agent_id = Some(id.into());
        snapshot.models[0].display_name = "Very Long Model Name".into();
        snapshot.variant = None;
        state.apply_catalog(snapshot);
        let budget = UnicodeWidthStr::width(label) as u16 + 12;
        let line = metadata_line(&state, theme, budget, 120).unwrap();
        assert_eq!(line.to_string(), format!("{label} · Very Lon…"));
        assert_eq!(line.width(), budget as usize);
        assert_eq!(line.spans[0].style.fg, Some(state.agent_color(Some(id))));
        assert_eq!(state.active_agent(), Some(id));
        assert!(
            !metadata_line(&state, theme, budget, 43)
                .unwrap()
                .to_string()
                .contains(label)
        );
    }
}

#[tokio::test]
async fn prompt_metadata_uses_upstream_budget_not_only_painted_width() {
    let mut state = golden_state().await;
    let mut snapshot = catalog();
    snapshot.agents[0].id = "x".into();
    snapshot.agent_id = Some("x".into());
    snapshot.models[0].display_name = "a".into();
    snapshot.models[0].provider_name = "P".repeat(26);
    snapshot.variant = None;
    state.apply_catalog(snapshot);
    let row = screen(&state, 44, 48)
        .into_iter()
        .find(|line| line.contains("X · a"))
        .expect("metadata row");
    assert!(row.contains("X · a"));
    assert!(
        !row.contains('P'),
        "provider must be omitted at 44 columns: {row}"
    );
}

#[tokio::test]
async fn golden_screen_80x24() {
    let state = golden_state().await;
    let hints = "shift+tab agents  ctrl+p commands";
    let mut expected = vec![String::new(); 24];
    // Tabs rail: default idle status leaves the indicator blank before the title.
    expected[0] = "   Untitled session".to_string();
    // Transcript: sticky bottom, content padding 2. Iteration 3a renders
    // the upstream message presentation: the user block carries the `┃`
    // border with 1/2 padding (`routes/session/index.tsx:2298-2335`), the
    // assistant text sits at paddingLeft=3 and each upstream row has
    // `marginTop=1` (`routes/session/index.tsx:1435`).
    expected[2] = "  ┃".to_string();
    expected[3] = "  ┃  hello".to_string();
    expected[4] = "  ┃".to_string();
    expected[5] = String::new();
    expected[6] = "     hi there".to_string();
    // Status row is empty while pinned; prompt box rows follow.
    expected[16] = "  ┃".to_string();
    expected[17] = "  ┃".to_string();
    expected[18] = "  ┃".to_string();
    expected[19] = "  ┃  X · a ludka2 · low".to_string();
    expected[20] = format!("  ╹{}", "▀".repeat(75));
    expected[21] = format!("  {}", right_aligned(hints, 76));
    expected[22] = String::new();
    // Devtools bar (local channel default), height 1.
    expected[23] = "  Native runtime  ○ UI".to_string();
    assert_eq!(screen(&state, 80, 24), expected);
}

#[tokio::test]
async fn golden_screen_120x40() {
    let state = golden_state().await;
    let hints = "shift+tab agents  ctrl+p commands";
    let mut expected = vec![String::new(); 40];
    expected[0] = "   Untitled session".to_string();
    // Same message presentation as 80x24, just taller.
    expected[2] = "  ┃".to_string();
    expected[3] = "  ┃  hello".to_string();
    expected[4] = "  ┃".to_string();
    expected[5] = String::new();
    expected[6] = "     hi there".to_string();
    expected[32] = "  ┃".to_string();
    expected[33] = "  ┃".to_string();
    expected[34] = "  ┃".to_string();
    expected[35] = "  ┃  X · a ludka2 · low".to_string();
    expected[36] = format!("  ╹{}", "▀".repeat(115));
    expected[37] = format!("  {}", right_aligned(hints, 116));
    expected[39] = "  Native runtime  ○ UI".to_string();
    assert_eq!(screen(&state, 120, 40), expected);
}

/// 44 is the only session-route switch this iteration renders (narrow
/// padding, metadata slots, footer hints); 60/80 keep that chrome. The
/// sidebar predicate switches above 120 (`component/session-frame.tsx:106`).
#[tokio::test]
async fn resize_switches_layout_at_upstream_breakpoints() {
    let state = golden_state().await;
    for width in [44, 60, 80, 120] {
        let frame = screen(&state, width, 24).join("\n");
        assert!(frame.contains("X · a ludka2 · low"), "{width}:\n{frame}");
        assert!(frame.contains("ctrl+p commands"), "{width}:\n{frame}");
    }
    assert!(layout::sidebar_auto(121));
    assert!(!layout::sidebar_auto(120));

    // Below 44 the padding drops to one cell and the agent/provider slots
    // and hints disappear (`routes/session/index.tsx:1277`,
    // `component/prompt/metadata.tsx:110-111`, `feature-plugins/prompt/footer.tsx:53`).
    let narrow = screen(&state, 43, 24);
    assert!(narrow[16].starts_with(" ┃"), "{:?}", narrow[16]);
    assert!(narrow[19].contains("┃ a"), "{:?}", narrow[19]);
    assert!(!narrow[19].contains("X ·"), "{:?}", narrow[19]);
    assert!(!narrow[19].contains("ludka2"), "{:?}", narrow[19]);
    assert!(!narrow.join("\n").contains("ctrl+p commands"));

    // Shrink -> grow is a pure re-layout: identical frames at the same size.
    let wide = screen(&state, 120, 40);
    let _ = screen(&state, 43, 24);
    assert_eq!(screen(&state, 120, 40), wide);
    assert_eq!(screen(&state, 43, 24), narrow);
}

async fn submit_and_reconcile(state: &mut TuiState) {
    state.handle_key(KeyAction::Enter).await;
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while state.active_turn().is_none() {
            tokio::task::yield_now().await;
            state.poll_submission();
        }
    })
    .await
    .expect("accepted turn");
}

#[tokio::test]
async fn message_stream_sticks_to_the_bottom() {
    let mut state = golden_state().await;
    let rows: Vec<HistoryMessage> = (0..40)
        .map(|i| msg(i, Role::User, &format!("line {i}")))
        .collect();
    state.attach_page(&page(rows));
    assert_eq!(state.scroll(), 0);

    // Pinned: the newest user block ends on the transcript's last row.
    let frame = screen(&state, 80, 24);
    assert_eq!(frame[13], "  ┃  line 39", "{frame:?}");
    assert_eq!(frame[14], "  ┃", "{frame:?}");
    assert!(!frame.join("\n").contains("Jump to latest"));

    // New rows keep the bottom pinned while at the bottom.
    for c in "go".chars() {
        state.handle_key(KeyAction::Char(c)).await;
    }
    submit_and_reconcile(&mut state).await;
    let turn = state.active_turn().expect("turn").clone();
    state.apply_finished(&turn, "fresh line", 0);
    assert_eq!(state.scroll(), 0);
    let frame = screen(&state, 80, 24);
    assert!(frame.join("\n").contains("fresh line"), "{frame:?}");

    // Scrolling up detaches: the newest row leaves the viewport and the
    // upstream jump affordance appears in the status row.
    for _ in 0..5 {
        state.scroll_transcript(true);
    }
    assert_eq!(state.scroll(), 5);
    let frame = screen(&state, 80, 24);
    assert!(!frame.join("\n").contains("fresh line"), "{frame:?}");
    assert!(frame[15].contains("Jump to latest ↓"), "{frame:?}");

    // Scrolling back to the bottom re-pins the stream.
    while state.scroll() > 0 {
        state.scroll_transcript(false);
    }
    let frame = screen(&state, 80, 24);
    assert!(frame.join("\n").contains("fresh line"), "{frame:?}");
    assert!(!frame[15].contains("Jump to latest"));
}

/// Iteration 3a: a live turn renders through the upstream message
/// presentation — collapsed reasoning, assistant markdown at paddingLeft=3
/// and the `agent · model · dur · tok/s` footer with provider usage.
#[tokio::test]
async fn golden_live_turn_with_reasoning_and_footer() {
    let mut state = golden_state().await;
    for c in "hi".chars() {
        state.handle_key(KeyAction::Char(c)).await;
    }
    submit_and_reconcile(&mut state).await;
    let turn = state.active_turn().expect("turn").clone();
    state.apply_reasoning_delta(&turn, "**Reading the code**\n\nbody");
    state.apply_delta(&turn, "All done.");
    // Running state: the static spinner fallback with the summary title.
    let running = screen(&state, 80, 24);
    assert!(
        running
            .iter()
            .any(|row| row.contains("⋯ Thinking: Reading the code")),
        "{running:?}"
    );
    state.apply_usage(&turn, 100, 200, 4000);
    state.apply_finished(&turn, "All done.", 1500);

    let frame = screen(&state, 80, 24);
    // Two committed rows, then the live turn: user block, assistant block
    // with the reasoning header, markdown body and footer.
    assert_eq!(frame[1], "  ┃  hello", "{frame:?}");
    assert_eq!(frame[2], "  ┃", "{frame:?}");
    assert_eq!(frame[3], "", "{frame:?}");
    assert_eq!(frame[4], "     hi there", "{frame:?}");
    assert_eq!(frame[5], "", "{frame:?}");
    assert_eq!(frame[6], "  ┃", "{frame:?}");
    assert_eq!(frame[7], "  ┃  hi", "{frame:?}");
    assert_eq!(frame[8], "  ┃", "{frame:?}");
    // Reasoning duration is measured by the view (wall clock), so only the
    // stable prefix is asserted; the turn duration comes from the event.
    assert!(
        frame[10].starts_with("     + Thought: Reading the code"),
        "{frame:?}"
    );
    assert_eq!(frame[12], "     All done.", "{frame:?}");
    assert_eq!(frame[14], "     X · a · 1.5s · 50.0 tok/s", "{frame:?}");
}

#[tokio::test]
async fn completed_footer_stays_one_row_and_clips_at_content_edge() {
    use oc_core::queries::HistoryTurn;

    let mut state = golden_state().await;
    state.chrome.devtools = Some(false);
    let mut completed = msg(2, Role::Assistant, "short answer");
    completed.turn = Some(HistoryTurn {
        id: "footer-geometry".into(),
        agent: Some("reader".into()),
        model_label: "MiMo-V2.6-Flash Free".into(),
        duration_ms: Some(125),
        status: "completed".into(),
        ..Default::default()
    });
    for (width, duration) in [(44, "125ms"), (63, "1.5s")] {
        completed.turn.as_mut().unwrap().duration_ms = Some(if width == 44 { 125 } else { 1500 });
        completed.turn.as_mut().unwrap().usage = (width == 63).then_some((20, 15));
        completed.turn.as_mut().unwrap().streamed_ms = (width == 63).then_some(100);
        state.attach_page(&page(vec![
            msg(1, Role::User, "question"),
            completed.clone(),
        ]));
        let rows = screen(&state, width, 24);
        let footer = rows.iter().find(|row| row.contains("Reader ·")).unwrap();
        assert!(
            footer.contains(&format!("Reader · MiMo-V2.6-Flash Free · {duration}")),
            "{width}x24: {rows:?}"
        );
        if width == 63 {
            assert!(footer.contains(" · 150.0 tok/s"), "{rows:?}");
        }
        assert_eq!(rows.iter().filter(|row| row.contains(duration)).count(), 1);
    }

    // A too-narrow terminal clips at its real edge, without adding a
    // continuation row that would change sticky paging or hit positions.
    let narrow = screen(&state, 38, 24);
    assert!(narrow.iter().any(|row| row.contains("Reader · MiMo")));
    assert!(!narrow.iter().any(|row| row.contains("1.5s")));
    let (short_lines, short_total, _) = state.visible_transcript_at_viewport(35, 38, 16);
    assert_eq!(
        short_lines
            .iter()
            .filter(|line| line.plain_text().contains("Reader ·"))
            .count(),
        1
    );
    completed.turn.as_mut().unwrap().duration_ms = None;
    state.attach_page(&page(vec![
        msg(1, Role::User, "question"),
        completed.clone(),
    ]));
    let (_, without_duration_total, _) = state.visible_transcript_at_viewport(35, 38, 16);
    assert_eq!(short_total, without_duration_total, "footer keeps one row");

    let baseline = screen(&state, 121, 24);
    let mut long = completed;
    long.turn.as_mut().unwrap().model_label = "very-long-model".repeat(20);
    state.attach_page(&page(vec![msg(1, Role::User, "question"), long]));
    let wide = screen(&state, 121, 24);
    let main = session_main(
        &state,
        shell_regions(&state, Rect::new(0, 0, 121, 24)).session,
    );
    assert!(
        wide.iter()
            .any(|row| row.contains("Reader · very-long-model"))
    );
    for (short, long) in baseline.iter().zip(&wide) {
        assert_eq!(
            short
                .chars()
                .skip(main.right() as usize)
                .collect::<String>(),
            long.chars().skip(main.right() as usize).collect::<String>(),
            "footer must not overwrite sidebar"
        );
    }
}

/// Interrupted turns keep the partial answer and mark the footer
/// (`routes/session/index.tsx:1977-1980`).
#[tokio::test]
async fn golden_interrupted_turn_footer() {
    let mut state = golden_state().await;
    for c in "go".chars() {
        state.handle_key(KeyAction::Char(c)).await;
    }
    submit_and_reconcile(&mut state).await;
    let turn = state.active_turn().expect("turn").clone();
    state.apply_delta(&turn, "partial answer");
    state.apply_interrupted(&turn, "partial answer", 1500);

    let frame = screen(&state, 80, 24);
    assert!(frame.join("\n").contains("partial answer"), "{frame:?}");
    assert_eq!(
        state
            .viewport()
            .iter()
            .filter(|line| line.contains("interrupted"))
            .count(),
        1,
        "{:?}",
        state.viewport()
    );
    assert_eq!(frame[14], "     X · a · 1.5s · interrupted", "{frame:?}");
}

#[tokio::test]
async fn footer_and_toast_use_theme_colors() {
    use ratatui::{Terminal, backend::TestBackend};

    let theme = Theme::dark();
    let mut state = golden_state().await;
    let turn = oc_core::core_app::WorkerTurnId("t-title".to_string());
    state.begin_compress_turn(turn.clone());
    state.push_note("something happened");
    state.close_panel(); // the toast/footer underlay is qualified independently

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("backend");
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .expect("draw");
    let buffer = terminal.backend().buffer();

    // Toast: side border in the warning variant color, raised-high
    // interior, note text on the first content row (padding 2/1).
    assert_eq!(buffer[(51, 1)].fg, theme.warning());
    assert_eq!(buffer[(54, 2)].bg, theme.background_raised_high());
    assert_eq!(buffer[(54, 2)].fg, theme.text());
    // Footer: margin + eight cells + gap then `esc interrupt`.
    assert_eq!(buffer[(3, 21)].symbol(), "■");
    assert_eq!(buffer[(3, 21)].fg, state.agent_color(state.active_agent()));
    assert_eq!(buffer[(12, 21)].fg, theme.text());
    assert_eq!(buffer[(16, 21)].fg, theme.text_muted());

    // DCP uses the same transient toast, with semantic error styling.
    state.apply_finished(&turn, "done", 0);
    state.notify_dcp(crate::dcp_panel::DcpOutcome::Failed {
        reason: "span open".to_string(),
    });
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("backend");
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let toast = toast_rect(&state, Rect::new(0, 0, 80, 24)).expect("DCP toast");
    assert_eq!(buffer[(toast.x, toast.y + 1)].fg, theme.error());
    assert!(state.dcp.notice().is_none(), "no duplicate footer notice");
}

#[test]
fn toast_wrap_is_word_aware_and_lossless() {
    assert_eq!(wrap_text("short note", 20), vec!["short note"]);
    assert_eq!(
        wrap_text("alpha beta gamma delta", 12),
        vec!["alpha beta", "gamma delta"]
    );
    // A word longer than the line splits instead of disappearing.
    assert_eq!(wrap_text("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    // Cyrillic is one cell wide, emoji are two.
    assert_eq!(wrap_text("привет мир", 9), vec!["привет", "мир"]);
    assert_eq!(wrap_text("🌍🌍", 3), vec!["🌍", "🌍"]);
}

#[tokio::test]
async fn toast_content_width_semantic_border_and_close() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut state = golden_state().await;
    let area = Rect::new(0, 0, 121, 40);
    state.push_note_variant("Configuration reloaded", NoteVariant::Success);
    let rect = toast_rect(&state, area).unwrap();
    assert_eq!(rect, Rect::new(88, 1, 31, 3));
    let backend = TestBackend::new(121, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(88, 2)].symbol(), "┃");
    assert_eq!(buffer[(88, 2)].fg, Theme::dark().success());
    assert_eq!(buffer[(115, 2)].symbol(), "x");
    assert_eq!(buffer[(115, 2)].fg, Theme::dark().text_muted());

    let mouse = |kind, column, row| MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    // A release alone cannot dismiss a note. A click inside can.
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 115, 2), area);
    assert_eq!(state.note(), Some("Configuration reloaded"));
    // Text selection and unrelated releases never activate the close cell.
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 91, 2), area);
    state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 115, 2), area);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 115, 2), area);
    assert_eq!(state.note(), Some("Configuration reloaded"));
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 115, 2), area);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 91, 2), area);
    assert_eq!(state.note(), Some("Configuration reloaded"));
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 115, 2), area);
    state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 115, 2), area);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 115, 2), area);
    assert_eq!(state.note(), Some("Configuration reloaded"));
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 115, 2), area);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 115, 2), area);
    assert_eq!(state.note(), None);

    for (variant, color) in [
        (NoteVariant::Info, Theme::dark().info()),
        (NoteVariant::Warning, Theme::dark().warning()),
        (NoteVariant::Error, Theme::dark().error()),
    ] {
        state.push_note_variant("generic note", variant);
        let rect = toast_rect(&state, area).unwrap();
        let backend = TestBackend::new(121, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        assert_eq!(terminal.backend().buffer()[(rect.x, rect.y)].fg, color);
    }
    state.push_note("other status");
    assert_eq!(state.note_variant(), Some(NoteVariant::Warning));
}

#[tokio::test]
async fn copied_toast_clears_sidebar_title_under_raised_interior() {
    let mut state = golden_state().await;
    state.chrome.devtools = Some(false);
    state.close_panel();
    state.session_title =
        Some("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ".into());
    let area = Rect::new(0, 0, 121, 40);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();

    state.push_note_variant("Copied to clipboard", NoteVariant::Info);
    let rect = toast_rect(&state, area).unwrap();
    let interior = Rect::new(rect.x + 1, rect.y, rect.width - 2, rect.height);
    let before = terminal.backend().buffer();
    assert_eq!(before[(interior.x, interior.y + 1)].symbol(), "L");

    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let theme = Theme::dark();
    for y in interior.y..interior.bottom() {
        let mut painted = String::new();
        for x in interior.x..interior.right() {
            let cell = &buffer[(x, y)];
            assert_eq!(
                cell.bg,
                theme.background_raised_high(),
                "background at ({x},{y})"
            );
            if y != rect.y + 1 || x < rect.x + 3 || (x >= rect.right() - 6 && x != rect.right() - 4)
            {
                assert_eq!(cell.symbol(), " ", "padding at ({x},{y})");
                assert_eq!(
                    cell.fg,
                    Color::Rgb(255, 255, 255),
                    "foreground at ({x},{y})"
                );
                assert_eq!(cell.modifier, Modifier::empty(), "modifier at ({x},{y})");
            }
            painted.push_str(cell.symbol());
        }
        let expected = if y == rect.y + 1 {
            "  Copied to clipboard  x  ".to_string()
        } else {
            " ".repeat(interior.width as usize)
        };
        assert_eq!(painted, expected, "toast interior row {y}");
    }
    for y in rect.y..rect.bottom() {
        assert_eq!(buffer[(rect.x, y)].symbol(), "┃");
        assert_eq!(buffer[(rect.right() - 1, y)].symbol(), "┃");
        assert_eq!(buffer[(rect.x, y)].fg, theme.info());
        assert_eq!(buffer[(rect.right() - 1, y)].fg, theme.info());
        assert!(buffer[(rect.x, y)].modifier.is_empty());
        assert!(buffer[(rect.right() - 1, y)].modifier.is_empty());
    }
    assert_eq!(buffer[(rect.x + 3, rect.y + 1)].fg, theme.text());
    assert_eq!(
        buffer[(rect.right() - 4, rect.y + 1)].fg,
        theme.text_muted()
    );
    assert_eq!(state.note(), Some("Copied to clipboard"));
}

#[tokio::test]
async fn toast_wraps_long_note_inside_max_width() {
    let mut state = golden_state().await;
    let message = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu";
    state.push_note(message);
    let rect = toast_rect(&state, Rect::new(0, 0, 80, 24)).unwrap();
    assert_eq!(rect.width, 59);
    assert_eq!(rect.x, 19);
    assert!(rect.height > 3);
    let lines = wrap_text(message, (rect.width - 9) as usize);
    assert_eq!(lines.join(" "), message);
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    for (row, line) in lines.iter().enumerate() {
        let painted: String = (rect.x + 3..rect.right() - 6)
            .map(|x| buffer[(x, rect.y + 1 + row as u16)].symbol())
            .collect();
        assert!(painted.starts_with(line), "{painted:?} vs {line:?}");
    }
    assert_eq!(buffer[(rect.right() - 4, rect.y + 1)].symbol(), "x");
    let narrow = toast_rect(&state, Rect::new(0, 0, 44, 24)).unwrap();
    assert_eq!(narrow.width, 36);
    assert_eq!(narrow.x, 6);
    state.push_note("🌍🌍");
    assert_eq!(toast_rect(&state, Rect::new(0, 0, 16, 24)), None);
    let backend = TestBackend::new(16, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(0, 23)].symbol(), "🌍");
    assert_eq!(terminal.backend().buffer()[(2, 23)].symbol(), "🌍");
    let rect = toast_rect(&state, Rect::new(0, 0, 17, 24)).unwrap();
    assert_eq!(rect.width, 11);
    let backend = TestBackend::new(17, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(
        terminal.backend().buffer()[(rect.x + 3, rect.y + 1)].symbol(),
        "🌍"
    );
}

#[tokio::test]
async fn prompt_box_glyphs_follow_the_padding_breakpoint() {
    let mut state = golden_state().await;
    let wide = screen(&state, 44, 24);
    assert!(wide[16].starts_with("  ┃"));
    assert!(wide[20].starts_with("  ╹▀"));
    // Input renders inside the box, after the inner 2-cell padding.
    for c in "hi".chars() {
        state.handle_key(KeyAction::Char(c)).await;
    }
    let wide = screen(&state, 44, 24);
    assert_eq!(wide[17], "  ┃  hi");

    let narrow = screen(&state, 43, 24);
    assert!(narrow[16].starts_with(" ┃"));
    assert!(narrow[20].starts_with(" ╹▀"));
    assert_eq!(narrow[17], " ┃ hi");
}
