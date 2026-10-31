//! Native "copy as raw Markdown" for a transcript selection.
//!
//! Rows drawn from assistant text carry their source line range
//! (`styled::MarkdownOrigin`). A selection expands to whole source lines of
//! that text; a partially selected fenced code block keeps its original fence
//! lines. Every other row is copied exactly as the plain selection copy.
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
use std::ops::Range;

use crate::history::SourceText;
use crate::styled::{self, Line, RowCopy};

const TOO_LARGE: &str = "Selection exceeds clipboard size limit";

enum Block<'a> {
    Source(SourceText, Range<usize>),
    Plain(Vec<(&'a Line, usize, usize)>),
}

/// Same row/endpoint rules as `styled::copy_text`; selected Markdown rows of
/// one source collapse into one contiguous excerpt, parts join by a blank line.
pub(crate) fn copy_markdown<'a>(
    limit: usize,
    rows: impl Iterator<Item = (&'a Line, usize, usize)>,
) -> Result<Option<String>, &'static str> {
    let mut blocks: Vec<Block<'a>> = Vec::new();
    for (line, begin, finish) in rows {
        if !styled::copyable_row(line) {
            continue;
        }
        let length = line.plain_text().len();
        let first = begin.max(styled::selectable_start(line)).min(length);
        let last = finish.min(length);
        if first > last || (first == last && line.copy() != RowCopy::Text) {
            continue;
        }
        let origin = line
            .markdown()
            .and_then(|origin| Some((origin.source.as_ref()?, origin.range.clone())));
        match (origin, blocks.last_mut()) {
            (Some((source, range)), Some(Block::Source(current, span)))
                if current.token() == source.token() =>
            {
                *span = span.start.min(range.start)..span.end.max(range.end);
            }
            (Some((source, range)), _) => blocks.push(Block::Source(source.clone(), range)),
            (None, Some(Block::Plain(rows))) => rows.push((line, begin, finish)),
            (None, _) => blocks.push(Block::Plain(vec![(line, begin, finish)])),
        }
    }
    let mut out = String::new();
    for block in blocks {
        let text = match block {
            Block::Source(source, range) => {
                excerpt(source.completion_prefix(usize::MAX), range, limit)?
            }
            Block::Plain(rows) => styled::copy_text(limit, rows.into_iter())?.unwrap_or_default(),
        };
        if text.is_empty() {
            continue;
        }
        let separator = if out.is_empty() { "" } else { "\n\n" };
        if out.len().saturating_add(separator.len() + text.len()) > limit {
            return Err(TOO_LARGE);
        }
        out.push_str(separator);
        out.push_str(&text);
    }
    Ok((!out.is_empty()).then_some(out))
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |index| index + 1)
}

/// End of the line containing `at`, before its `\n` or `\r\n`.
fn line_end(text: &str, at: usize) -> usize {
    let end = text[at..].find('\n').map_or(text.len(), |index| at + index);
    if text[..end].ends_with('\r') {
        end - 1
    } else {
        end
    }
}

/// Whole source lines covering `range`, wrapped in the opening/closing fence
/// lines of a code block the excerpt starts or ends inside.
fn excerpt(text: &str, range: Range<usize>, limit: usize) -> Result<String, &'static str> {
    let start = range.start.min(text.len());
    let mut end = range.end.clamp(start, text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let start = line_start(text, start.min(end));
    if end > start && text[..end].ends_with('\n') {
        end -= 1;
    }
    let end = line_end(text, end.max(start));
    if end - start > limit {
        return Err(TOO_LARGE);
    }
    let (mut opener, mut closer) = (None, None);
    for (event, block) in Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter() {
        if block.start > end {
            break;
        }
        let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) = event else {
            continue;
        };
        let open = line_start(text, block.start)..line_end(text, block.start);
        let body_end = block.start + text[block.clone()].trim_end_matches(['\n', '\r']).len();
        let close = line_start(text, body_end)..body_end;
        let closed = close.start > open.start && {
            let fence = text[close.clone()].trim_start_matches([' ', '\t', '>']);
            fence.len() >= 3
                && (fence.bytes().all(|c| c == b'`') || fence.bytes().all(|c| c == b'~'))
        };
        let body = if closed { close.start } else { usize::MAX };
        if open.end < start && start < body {
            opener = Some(open.clone());
        }
        if closed && open.start <= end && end < close.start {
            closer = Some(close);
        }
    }
    let mut out = String::new();
    if let Some(open) = opener {
        out.push_str(&text[open]);
        out.push('\n');
    }
    out.push_str(&text[start..end]);
    if let Some(close) = closer {
        out.push('\n');
        out.push_str(&text[close]);
    }
    if out.len() > limit {
        return Err(TOO_LARGE);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cut(text: &str, needle: &str) -> String {
        let at = text.find(needle).unwrap();
        excerpt(text, at..at + needle.len(), usize::MAX).unwrap()
    }

    #[test]
    fn markdown_selection_excerpt_snaps_to_whole_source_lines() {
        let text = "Intro **bold** and `code`.\n\n- item *one*\n- item two\r\nTail\n";
        assert_eq!(cut(text, "bold"), "Intro **bold** and `code`.");
        assert_eq!(cut(text, "one*\n- item"), "- item *one*\n- item two");
        assert_eq!(cut(text, "two"), "- item two");
    }

    #[test]
    fn markdown_selection_excerpt_keeps_fences_of_a_partial_code_block() {
        let text = "Run:\n\n```sh\nmake\nmake test\n```\n\nDone\n";
        assert_eq!(cut(text, "make test"), "```sh\nmake test\n```");
        assert_eq!(cut(text, "Run:\n\n```sh\nmake"), "Run:\n\n```sh\nmake\n```");
        assert_eq!(
            cut(text, "make test\n```\n\nDone"),
            "```sh\nmake test\n```\n\nDone"
        );
        assert_eq!(
            cut(text, "Run:\n\n```sh\nmake\nmake test\n```"),
            "Run:\n\n```sh\nmake\nmake test\n```"
        );
        // A streaming fence has no closer to restore and none is invented.
        assert_eq!(
            cut("```rust\nfn a() {}\nfn b() {}", "fn b"),
            "```rust\nfn b() {}"
        );
    }

    #[test]
    fn markdown_selection_excerpt_respects_the_clipboard_limit() {
        let text = "x".repeat(32);
        assert_eq!(excerpt(&text, 3..4, 16), Err(TOO_LARGE));
        assert_eq!(excerpt(&text, 3..4, 32).unwrap(), text);
    }
}
