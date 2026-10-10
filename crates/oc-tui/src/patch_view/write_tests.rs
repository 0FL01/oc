use super::*;
use crate::{history::card_from_row, theme::SyntaxToken};
use oc_core::tool_output::{Presentation, Write};

#[test]
fn completed_write_uses_only_the_safe_captured_postimage_and_shared_syntax() {
    let theme = Theme::dark();
    let content = "/* café\n中文 😀 */\nfn main() {\n\tlet value = \"世界\";\n}\n";
    let mut row = super::tests::row(Some(super::tests::effects()), "completed");
    row.name = "write".into();
    row.input = Some(r#"{"path":"foreign.rs","content":"unsafe requested secret"}"#.into());
    let mut presentation = Presentation::new("actual result", 13, false);
    presentation.write = Some(Write {
        path: "captured.rs".into(),
        path_limited: false,
        content: content.into(),
        content_bytes: content.len() as u64,
        content_limited: false,
    });
    row.output_presentation = Some(Box::new(presentation.clone()));
    let card = card_from_row(&row);
    let original = card.clone();
    for width in [0, 43, 80, 120, 160] {
        let lines = crate::tools::tool_block(&card, theme, width);
        let text = lines
            .iter()
            .map(Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("# Wrote captured.rs"));
        assert!(
            lines
                .iter()
                .any(|line| line.plain_text().trim_end() == "┃   1 /* café")
        );
        let first = lines
            .iter()
            .find(|line| line.plain_text().contains("1 /* café"))
            .unwrap();
        assert!(
            first
                .spans()
                .iter()
                .any(|span| span.content() == "1" && span.style().fg == Some(theme.text_muted()))
        );
        assert_eq!(
            first
                .spans()
                .iter()
                .filter(|span| span.content() == " " && span.style().fg.is_none())
                .count(),
            2
        );
        assert!(text.contains("  2 中文 😀 */"));
        assert!(text.contains("  3 fn main() {"));
        assert!(text.contains("  4   let value = \"世界\";"));
        assert!(
            lines
                .iter()
                .any(|line| line.plain_text().trim_end() == "┃   6")
        );
        assert!(!text.contains("unsafe requested secret") && !text.contains("foreign.rs"));
        assert!(!text.contains("context") && !text.contains("old") && !text.contains("101 +"));
        let comment = lines
            .iter()
            .find(|line| line.plain_text().contains("中文"))
            .unwrap();
        assert!(comment.spans().iter().any(|span| {
            span.style()
                .add_modifier
                .contains(ratatui::style::Modifier::ITALIC)
        }));
        let function = lines
            .iter()
            .find(|line| line.plain_text().contains("fn main"))
            .unwrap();
        assert!(
            function
                .spans()
                .iter()
                .any(|span| span.content().contains("fn")
                    && span.style().fg == Some(theme.syntax(SyntaxToken::Function)))
        );
        if width != 0 {
            assert!(lines.iter().all(|line| {
                line.spans().iter().map(styled::span_width).sum::<usize>() == width as usize
            }));
        }
    }
    assert_eq!(card, original);
    assert_eq!(card.patch_effects, row.patch_effects);
    assert_eq!(card.output_presentation.as_deref(), Some(&presentation));

    for (content, width, first, last) in [
        (
            format!("{}x", "x\n".repeat(119)),
            120,
            "┃     1 x",
            "┃   120 x",
        ),
        (
            "x".repeat(360),
            20,
            "┃    1 xxxxxxxxxxxxx",
            "┃      xxxxxxxxx",
        ),
    ] {
        let mut counted = row.clone();
        let write = counted
            .output_presentation
            .as_mut()
            .unwrap()
            .write
            .as_mut()
            .unwrap();
        write.content_bytes = content.len() as u64;
        write.content = content;
        let lines = crate::tools::tool_block(&card_from_row(&counted), theme, width);
        assert!(
            lines
                .iter()
                .any(|line| line.plain_text().trim_end() == first)
        );
        assert!(
            lines
                .iter()
                .any(|line| line.plain_text().trim_end() == last)
        );
    }

    // Cold/budget-omitted effects do not lose the separately recorded preview.
    let mut cold = row.clone();
    cold.patch_effects = None;
    let text = crate::tools::tool_block(&card_from_row(&cold), theme, 120)
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("  3 fn main() {"));

    for state in ["failed", "unknown", "cancelled"] {
        let mut failed = row.clone();
        failed.state = state.into();
        let failed = card_from_row(&failed);
        let text = crate::tools::tool_block(&failed, theme, 120)
            .iter()
            .map(Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Confirmed effects before failure:"));
        assert!(text.contains("101 + let value = 42;"));
        assert!(!text.contains("  3 fn main() {"));
    }
    let mut legacy = row.clone();
    legacy.output_presentation = None;
    let text = crate::tools::tool_block(&card_from_row(&legacy), theme, 120)
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("recorded Write content preview unavailable"));
    assert!(text.contains("1 confirmed files, +1 -1"));
    assert!(!text.contains("unsafe requested secret") && !text.contains("101 +"));
    let mut limited = row;
    limited
        .output_presentation
        .as_mut()
        .unwrap()
        .write
        .as_mut()
        .unwrap()
        .content_limited = true;
    let limited = card_from_row(&limited);
    assert!(limited.preview_limited());
    let text = crate::tools::tool_block(&limited, theme, 120)
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Write content preview limited"));
}
