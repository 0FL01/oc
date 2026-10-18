use super::*;
use oc_core::queries::{HistoryPartWindow, HistoryTurn, PartState, TranscriptPart};

fn page(start: usize, end: usize, total: usize) -> HistoryPage {
    HistoryPage {
        rows: vec![HistoryMessage {
            id: oc_core::session::MessageId("answer".into()),
            seq: 42,
            role: Role::Assistant,
            text: "aggregate must not be replayed".into(),
            images: Vec::new(),
            model_switch: None,
            user_shell: None,
            child: None,
            shell_notice: None,
            turn: Some(HistoryTurn {
                id: "turn".into(),
                parts: (start..end)
                    .map(|n| TranscriptPart::Reasoning {
                        text: format!("part-{n}"),
                        duration_ms: None,
                    })
                    .collect(),
                part_states: (start..end)
                    .map(|sequence| PartState {
                        sequence,
                        status: "completed".into(),
                        ..Default::default()
                    })
                    .collect(),
                window: Some(HistoryPartWindow { start, end, total }),
                ..Default::default()
            }),
        }],
        total: 1,
        ..Default::default()
    }
}

#[test]
fn evicted_parts_of_one_message_are_reachable_in_both_directions() {
    let mut window = HistoryWindow::default();
    window.reset(&page(240, 480, 480));
    let older = window.older_part_cursor().unwrap();
    assert_eq!(
        older.sequence, 241,
        "first part evicted for the footer remains reachable"
    );
    assert_eq!(older.message.0, "answer");
    window.prepend_older(&page(1, 241, 480));
    assert_eq!(window.older_part_cursor().unwrap().sequence, 1);
    window.prepend_older(&page(0, 1, 480));
    assert!(window.older_part_cursor().is_none());
    assert_eq!(
        window.rows()[0].reasoning.as_ref().unwrap().identity,
        Some(ReasoningIdentity::Durable(42, 0))
    );
    let newer = window.newer_part_cursor().unwrap();
    assert_eq!(newer.sequence, 239);
    window.append_newer(&page(240, 480, 480));
    assert!(window.newer_part_cursor().is_none());
    assert_eq!(window.older_part_cursor().unwrap().sequence, 241);
    assert!(
        window
            .rows()
            .iter()
            .all(|row| !row.text.contains("aggregate must not"))
    );
    assert_eq!(
        window
            .rows()
            .iter()
            .filter(|row| row.meta.is_some())
            .count(),
        1
    );
    assert!(window.rows().len() <= WINDOW_ROWS);
    assert!(window.retained_bytes() <= WINDOW_BYTES);
}

#[test]
fn detached_same_message_disjoint_window_does_not_replace_its_canonical_parts() {
    let mut window = HistoryWindow::default();
    window.reset(&page(0, 240, 600));
    let before = window.rows().to_vec();
    window.refresh_completed(&page(360, 600, 600), true);
    assert_eq!(window.rows(), before);
    assert!(window.has_newer());
    assert_eq!(window.newer_part_cursor().unwrap().sequence, 239);
}

#[test]
fn a_large_canonical_text_is_not_evicted_leaving_only_its_footer() {
    let mut canonical = page(0, 1, 1);
    canonical.rows[0].turn.as_mut().unwrap().parts[0] =
        TranscriptPart::Text("λ".repeat(5 * 1024 * 1024));
    let mut window = HistoryWindow::default();
    window.reset(&canonical);
    assert_eq!(window.rows()[0].text.len(), 10 * 1024 * 1024);
    assert_eq!(window.rows().len(), 2);
    assert!(window.retained_bytes() <= WINDOW_BYTES);
}

#[test]
fn completion_reanchors_a_detached_active_window_to_its_assistant_message() {
    let mut active = page(0, 240, 600);
    active.rows[0].id = oc_core::session::MessageId("accepted-user".into());
    active.rows[0].seq = 41;
    active.rows[0].turn.as_mut().unwrap().status = "started".into();
    let mut window = HistoryWindow::default();
    window.reset(&active);
    window.refresh_completed(&page(360, 600, 600), true);
    assert_eq!(window.rows()[0].reasoning.as_ref().unwrap().text, "part-0");
    assert!(
        window
            .rows()
            .iter()
            .all(|row| row.seq == 42 && row.message_id.as_ref().unwrap().0 == "answer")
    );
    assert!(
        window
            .rows()
            .iter()
            .filter_map(|row| row.reasoning.as_ref())
            .all(|reasoning| !reasoning.running)
    );
    assert_eq!(window.newer_part_cursor().unwrap().message.0, "answer");
}
