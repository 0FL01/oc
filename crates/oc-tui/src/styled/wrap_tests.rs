use super::*;

#[test]
fn source_word_wrap_uses_pinned_punctuation_boundaries_without_restyling() {
    let text = Style::default().fg(Color::White);
    let token = Style::default()
        .fg(Color::Rgb(255, 190, 100))
        .add_modifier(Modifier::ITALIC);
    let markdown = Line::new(vec![
        Span::styled(
            "**strong 中文 😀** and *emphasis* with `literal` and ",
            text,
        ),
        Span::styled("[link](https://example.invalid).", token),
    ]);
    let zig = Line::new(vec![
        Span::styled("pub fn main() void { const value: u32 = 42; ", text),
        Span::styled("std.debug.print", token),
        Span::styled("(\"{d}\", .{value}); }", text),
    ]);
    for (line, expected) in [
        (
            markdown,
            vec![
                "**strong 中文 😀** and *emphasis* with `literal` and [link](https://",
                "example.invalid).",
            ],
        ),
        (
            zig,
            vec![
                "pub fn main() void { const value: u32 = 42; std.debug.print(\"{d}\", .{",
                "value}); }",
            ],
        ),
    ] {
        let before = line.clone();
        let rows = wrap_code_line_limited(&line, 69, usize::MAX);
        assert_eq!(
            rows.iter().map(Line::plain_text).collect::<Vec<_>>(),
            expected
        );
        let cells = |lines: &[Line]| {
            lines
                .iter()
                .flat_map(|line| line.spans())
                .flat_map(|span| span.content().chars().map(move |ch| (ch, span.style())))
                .collect::<Vec<_>>()
        };
        assert_eq!(cells(&rows), cells(std::slice::from_ref(&line)));
        assert_eq!(line, before);
        let mut streamed = Vec::new();
        visit_source_space_line(&line, 69, |row| streamed.push(row));
        assert_eq!(streamed, rows);
        assert_eq!(wrap_code_line_limited(&line, 69, 1), rows[..1]);
    }

    let punctuated = Line::styled("Hello,World.Test", token);
    for wrap in [wrap_code_line_limited, wrap_permission_line_limited] {
        let rows = wrap(&punctuated, 8, 3);
        assert_eq!(
            rows.iter().map(Line::plain_text).collect::<Vec<_>>(),
            ["Hello,", "World.", "Test"]
        );
        assert!(
            rows.iter()
                .flat_map(|row| row.spans())
                .all(|span| span.style() == token)
        );
    }
}

#[test]
fn source_word_wrap_preserves_indentation_unicode_boundaries_and_width_limits() {
    for (source, width, expected) in [
        ("abc中文 def", 6, vec!["abc", "中文 ", "def"]),
        ("中文abc", 5, vec!["中文", "abc"]),
        ("ab\u{00a0}cdef", 4, vec!["ab\u{00a0}", "cdef"]),
        ("     x", 2, vec!["  ", "  ", " x"]),
        (" aaaa😀", 5, vec![" ", "aaaa", "😀"]),
        ("abc\u{2010}def", 5, vec!["abc\u{2010}", "def"]),
        ("a \u{0301}", 1, vec!["a", " \u{0301}"]),
        ("中\u{2060}ab", 3, vec!["中\u{2060}a", "b"]),
    ] {
        let line = Line::plain(source);
        let rows = wrap_code_line_limited(&line, width, 10);
        assert_eq!(
            rows.iter().map(Line::plain_text).collect::<Vec<_>>(),
            expected,
            "{source:?}"
        );
        assert!(
            rows.iter()
                .all(|row| row.spans().iter().map(span_width).sum::<usize>() <= width)
        );
        let mut streamed = Vec::new();
        visit_source_space_line(&line, width, |row| streamed.push(row));
        assert_eq!(streamed, rows);
        assert_eq!(wrap_code_line_limited(&line, width, 0), Vec::<Line>::new());
    }
    let style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::ITALIC);
    let indent = Line::styled("\u{00a0}xx", style);
    let indented = wrap_permission_line_limited(&indent, 2, 3);
    assert_eq!(
        indented.iter().map(Line::plain_text).collect::<Vec<_>>(),
        ["\u{00a0}", "xx"]
    );
    assert_eq!(wrap_permission_line_limited(&indent, 2, 1), indented[..1]);
    for (source, expected) in [
        ("\u{00a0}x", ["\u{00a0}", "x"]),
        ("a \u{0301}", ["a", " \u{0301}"]),
    ] {
        let line = Line::styled(source, style);
        let rows = wrap_permission_line_limited(&line, 1, 2);
        assert_eq!(
            rows.iter().map(Line::plain_text).collect::<Vec<_>>(),
            expected
        );
        assert!(
            rows.iter()
                .flat_map(|row| row.spans())
                .all(|span| span.style() == style)
        );
        assert_eq!(wrap_permission_line_limited(&line, 1, 1), rows[..1]);
    }
    let accent = Style::default().fg(Color::Yellow);
    for separator in [" ", "\u{00a0}"] {
        let line = Line::new(vec![
            Span::styled(format!("a{separator}"), style),
            Span::styled("\u{0301}", accent),
            Span::styled("b", style),
        ]);
        for wrap in [wrap_code_line_limited, wrap_permission_line_limited] {
            let rows = wrap(&line, 1, 3);
            assert_eq!(
                rows.iter().map(Line::plain_text).collect::<Vec<_>>(),
                [
                    "a".to_owned(),
                    format!("{separator}\u{0301}"),
                    "b".to_owned()
                ]
            );
            assert_eq!(
                rows[1].spans(),
                &[
                    Span::styled(separator, style),
                    Span::styled("\u{0301}", accent),
                ]
            );
            assert_eq!(wrap(&line, 1, 2), rows[..2]);
        }
    }
    let marked_cjk = Line::new(vec![
        Span::styled("中文", style),
        Span::styled("\u{0301}", accent),
        Span::styled("abc", style),
    ]);
    assert_eq!(
        wrap_code_line_limited(&marked_cjk, 5, 2)
            .iter()
            .map(Line::plain_text)
            .collect::<Vec<_>>(),
        ["中文\u{0301}", "abc"]
    );
}
