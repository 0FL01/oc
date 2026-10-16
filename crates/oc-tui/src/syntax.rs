//! Shared native syntax grammar/query owner for Markdown and patch previews.
//! The inventory is compiled from repository-vendored, revision-pinned sources;
//! neither build nor runtime fetches a parser or starts a JS/WASM host.

use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeSet, VecDeque},
    sync::OnceLock,
    time::{Duration, Instant},
};

use ratatui::style::Style;
use streaming_iterator::StreamingIterator;
use tree_sitter::{ParseOptions, Parser, Query, QueryCursor, QueryCursorOptions};

use crate::{
    styled::{Line, Span},
    theme::Theme,
};

mod styles;

const SOURCE_BYTES: usize = 64 * 1024;
const CAPTURES: usize = 8192;
const CACHE_BYTES: usize = 256 * 1024;
const CACHE_ENTRIES: usize = 8;
const PARSE_TIME: Duration = Duration::from_millis(30);

struct Grammar {
    name: &'static str,
    aliases: &'static [&'static str],
    language: fn() -> tree_sitter::Language,
    highlights: &'static str,
    injections: &'static str,
    injection_nodes: &'static [(&'static str, &'static str)],
    injection_info: &'static [(&'static str, &'static str)],
    compiled: OnceLock<Result<Compiled, String>>,
}

include!(concat!(env!("OUT_DIR"), "/syntax_languages.rs"));

struct Compiled {
    highlights: Query,
    injections: Option<Query>,
}

impl Grammar {
    fn queries(&self) -> Result<&Compiled, Error> {
        self.compiled
            .get_or_init(|| {
                let language = (self.language)();
                Ok(Compiled {
                    highlights: Query::new(&language, self.highlights)
                        .map_err(|e| e.to_string())?,
                    injections: (!self.injections.is_empty())
                        .then(|| Query::new(&language, self.injections))
                        .transpose()
                        .map_err(|e| e.to_string())?,
                })
            })
            .as_ref()
            .map_err(|_| Error::Asset(self.name))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Limit,
    Asset(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Capture {
    start: usize,
    end: usize,
    scope: &'static str,
    conceal: Option<&'static str>,
    contains_injection: bool,
    is_injection: bool,
}

struct Cached {
    language: &'static str,
    source: String,
    captures: Vec<Capture>,
}

impl Cached {
    fn bytes(&self) -> usize {
        self.source.len() + self.captures.len() * size_of::<Capture>()
    }
}

#[derive(Default)]
struct Syntax {
    parser: Parser,
    cursor: QueryCursor,
    cache: VecDeque<Cached>,
    bytes: usize,
}

thread_local! {
    // Every parent/child/live/replay/patch renderer on the UI thread shares this
    // bounded token cache. Theme and viewport are deliberately not cached here.
    static SYNTAX: RefCell<Syntax> = RefCell::new(Syntax::default());
}

fn grammar(info: &str) -> Option<&'static Grammar> {
    let token = info.split_whitespace().next()?.to_lowercase();
    let normalized = token.strip_prefix('.').unwrap_or(&token);
    let path = normalized.replace('\\', "/");
    let basename = path.rsplit('/').next()?;
    let name = lookup(INFO_BASENAMES, &token)
        .or_else(|| lookup(INFO_BASENAMES, normalized))
        .or_else(|| lookup(INFO_BASENAMES, basename))
        .or_else(|| {
            basename
                .rsplit_once('.')
                .and_then(|(_, extension)| lookup(INFO_EXTENSIONS, extension))
        })
        .or_else(|| lookup(INFO_EXTENSIONS, normalized))
        .unwrap_or(normalized);
    registered(name)
}

fn lookup(pairs: &'static [(&'static str, &'static str)], value: &str) -> Option<&'static str> {
    pairs
        .iter()
        .find(|(key, _)| *key == value)
        .map(|(_, target)| *target)
}

fn registered(name: &str) -> Option<&'static Grammar> {
    GRAMMARS
        .iter()
        .find(|g| g.name == name || g.aliases.contains(&name))
}

impl Syntax {
    fn captures(&mut self, grammar: &'static Grammar, source: &str) -> Result<Vec<Capture>, Error> {
        if source.len() > SOURCE_BYTES {
            return Err(Error::Limit);
        }
        if let Some(index) = self
            .cache
            .iter()
            .position(|entry| entry.language == grammar.name && entry.source == source)
        {
            let entry = self.cache.remove(index).expect("existing cache entry");
            let result = entry.captures.clone();
            self.cache.push_back(entry);
            return Ok(result);
        }
        // Trusted, immutable query compilation is separate from bounded work on
        // untrusted message text; a cold query must not consume its parse budget.
        grammar.queries()?;
        let mut deadline = Instant::now() + PARSE_TIME;
        let mut result = self.parse(grammar, source, 0, true, &mut deadline)?;
        // The one-shot Markdown parser may add one closing-delimiter newline.
        // Its AST metadata is retained, but emitted byte ranges stay inside the
        // original payload just as the reference converter's substring slices.
        for capture in &mut result {
            capture.start = capture.start.min(source.len());
            capture.end = capture.end.min(source.len());
        }
        // OpenTUI's one-shot worker sorts stably by start, retaining query order
        // for equal starts. It does not apply Neovim locals/custom directives.
        result.sort_by_key(|capture| capture.start);
        let entry = Cached {
            language: grammar.name,
            source: source.to_owned(),
            captures: result.clone(),
        };
        if entry.bytes() <= CACHE_BYTES {
            while self.cache.len() >= CACHE_ENTRIES || self.bytes + entry.bytes() > CACHE_BYTES {
                let old = self.cache.pop_front().expect("nonempty bounded cache");
                self.bytes -= old.bytes();
            }
            self.bytes += entry.bytes();
            self.cache.push_back(entry);
        }
        Ok(result)
    }

    fn parse(
        &mut self,
        grammar: &'static Grammar,
        source: &str,
        offset: usize,
        inject: bool,
        deadline: &mut Instant,
    ) -> Result<Vec<Capture>, Error> {
        // All immutable query compilation, including a cold injected language,
        // is trusted setup rather than work on this message's untrusted text.
        // Pause only that setup time; parsing/querying share the same budget.
        let compilation = Instant::now();
        let compiled = grammar.queries()?;
        *deadline += compilation.elapsed();
        self.parser
            .set_language(&(grammar.language)())
            .map_err(|_| Error::Asset(grammar.name))?;
        // Exact outer highlightOnce workaround from OpenTUI 0.5.10. Injected
        // Markdown parses do not apply it, and source text is never amended.
        let parse_source = if inject && grammar.name == "markdown" && source.ends_with("```") {
            Cow::Owned(format!("{source}\n"))
        } else {
            Cow::Borrowed(source)
        };
        let mut stop = |_: &tree_sitter::ParseState| Instant::now() >= *deadline;
        let tree = self.parser.parse_with_options(
            &mut |byte, _| &parse_source.as_bytes()[byte..],
            None,
            Some(ParseOptions::new().progress_callback(&mut stop)),
        );
        let Some(tree) = tree else {
            // A cancelled parser must not resume that input on the next request.
            self.parser.reset();
            return Err(Error::Limit);
        };
        let mut captures = Vec::new();
        self.cursor.set_match_limit(CAPTURES as u32);
        let mut stop = |_: &tree_sitter::QueryCursorState| Instant::now() >= *deadline;
        {
            let mut found = self.cursor.captures_with_options(
                &compiled.highlights,
                tree.root_node(),
                parse_source.as_bytes(),
                QueryCursorOptions::new().progress_callback(&mut stop),
            );
            while let Some((matched, index)) = found.next() {
                let capture = matched.captures[*index];
                if captures.len() >= CAPTURES || Instant::now() >= *deadline {
                    return Err(Error::Limit);
                }
                let start = capture.node.start_byte();
                let end = capture.node.end_byte();
                if !parse_source.is_char_boundary(start) || !parse_source.is_char_boundary(end) {
                    return Err(Error::Asset(grammar.name));
                }
                let conceal = compiled
                    .highlights
                    .property_settings(matched.pattern_index)
                    .iter()
                    .find(|property| property.key.as_ref() == "conceal")
                    .map(|property| property.value.as_deref().unwrap_or(""));
                captures.push(Capture {
                    start: offset + start,
                    end: offset + end,
                    scope: compiled.highlights.capture_names()[capture.index as usize],
                    conceal,
                    contains_injection: false,
                    is_injection: !inject,
                });
            }
        }
        if self.cursor.did_exceed_match_limit() || Instant::now() >= *deadline {
            return Err(Error::Limit);
        }
        if inject && let Some(query) = &compiled.injections {
            let mut ranges = Vec::new();
            let mut stop = |_: &tree_sitter::QueryCursorState| Instant::now() >= *deadline;
            {
                let mut found = self.cursor.captures_with_options(
                    query,
                    tree.root_node(),
                    parse_source.as_bytes(),
                    QueryCursorOptions::new().progress_callback(&mut stop),
                );
                while let Some((matched, index)) = found.next() {
                    let capture = matched.captures[*index];
                    if !query.capture_names()[capture.index as usize].contains("injection") {
                        continue;
                    }
                    let node = capture.node;
                    let target = lookup(grammar.injection_nodes, node.kind())
                        .and_then(registered)
                        .or_else(|| {
                            (node.kind() == "code_fence_content")
                                .then(|| {
                                    node.parent()
                                        .and_then(|parent| {
                                            let mut walk = parent.walk();
                                            parent
                                                .children(&mut walk)
                                                .find(|child| child.kind() == "info_string")
                                        })
                                        .and_then(|info| {
                                            let mut walk = info.walk();
                                            info.children(&mut walk)
                                                .find(|child| child.kind() == "language")
                                        })
                                        .and_then(|language| {
                                            language.utf8_text(source.as_bytes()).ok()
                                        })
                                        .and_then(|language| {
                                            registered(
                                                lookup(grammar.injection_info, language)
                                                    .unwrap_or(language),
                                            )
                                        })
                                })
                                .flatten()
                        });
                    if let Some(target) = target {
                        let range = (node.start_byte(), node.end_byte(), target);
                        // The actual one-shot worker preserves every query
                        // capture, including multiple injection nodes/ranges.
                        ranges.push(range);
                    }
                    if ranges.len() >= CAPTURES || Instant::now() >= *deadline {
                        return Err(Error::Limit);
                    }
                }
            }
            if self.cursor.did_exceed_match_limit() || Instant::now() >= *deadline {
                return Err(Error::Limit);
            }
            // Language groups preserve first-seen order, as the worker's Map.
            let mut grouped = Vec::new();
            for (_, _, target) in &ranges {
                if !grouped.contains(&target.name) {
                    grouped.push(target.name);
                }
            }
            let ordered: Vec<_> = grouped
                .iter()
                .flat_map(|name| {
                    ranges
                        .iter()
                        .filter(move |(_, _, target)| target.name == *name)
                })
                .collect();
            for &&(start, end, target) in &ordered {
                captures.extend(self.parse(
                    target,
                    &source[start.min(source.len())..end.min(source.len())],
                    offset + start,
                    false,
                    deadline,
                )?);
                if captures.len() > CAPTURES {
                    return Err(Error::Limit);
                }
            }
            // OpenTUI derives metadata from actual containment, not from the
            // query which emitted a capture. Equal ranges are injections first.
            for capture in &mut captures {
                if Instant::now() >= *deadline {
                    return Err(Error::Limit);
                }
                capture.is_injection = false;
                capture.contains_injection = false;
                for &&(start, end, _) in &ordered {
                    let (start, end) = (offset + start, offset + end);
                    if capture.start >= start && capture.end <= end {
                        capture.is_injection = true;
                        break;
                    } else if capture.start <= start && capture.end >= end {
                        capture.contains_injection = true;
                        break;
                    }
                }
            }
        }
        Ok(captures)
    }
}

pub(crate) fn filetype(path: &str) -> Option<&'static str> {
    let filename = path.rsplit('/').next()?;
    let dot = filename.rfind('.')?;
    if dot == 0 {
        return None;
    }
    let language = PATH_TYPES
        .iter()
        .find(|(extension, _)| *extension == &filename[dot..])?
        .1;
    Some(match language {
        "javascript" | "javascriptreact" | "typescriptreact" => "typescript",
        other => other,
    })
}

/// Terminal controls are visible/safe, but TAB remains source for grammar input.
pub(crate) fn safe_line(source: &str) -> String {
    source
        .chars()
        .map(|c| {
            if c.is_control() && c != '\t' {
                '�'
            } else {
                c
            }
        })
        .collect()
}

/// OpenTUI's text buffer paints each TAB as two cells, independently of column.
/// Keep source TAB bytes through parsing; expand only styled display spans.
pub(crate) fn display_line(line: &Line) -> Line {
    Line::new(
        line.spans()
            .iter()
            .map(|span| Span::styled(span.content().replace('\t', "  "), span.style()))
            .collect(),
    )
}

/// Whole bounded source is parsed before splitting rows, so multi-line strings,
/// comments, injections and partial fences retain grammar-defined boundaries.
pub(crate) fn highlight(
    source: &str,
    info: Option<&str>,
    theme: &Theme,
    base: Style,
) -> Result<Vec<Line>, Error> {
    highlight_window(source, 0..source.len(), info, theme, base)
}

/// Parse the actual whole fence, but materialize only the indexed page's bytes.
pub(crate) fn highlight_window(
    source: &str,
    window: std::ops::Range<usize>,
    info: Option<&str>,
    theme: &Theme,
    base: Style,
) -> Result<Vec<Line>, Error> {
    highlight_options(source, info, theme, base, false, window)
}

fn highlight_options(
    source: &str,
    info: Option<&str>,
    theme: &Theme,
    base: Style,
    conceal: bool,
    window: std::ops::Range<usize>,
) -> Result<Vec<Line>, Error> {
    if source.len() > SOURCE_BYTES
        || window.start > window.end
        || window.end > source.len()
        || !source.is_char_boundary(window.start)
        || !source.is_char_boundary(window.end)
    {
        return Err(Error::Limit);
    }
    let Some(grammar) = info.and_then(grammar) else {
        return Ok(source[window]
            .split('\n')
            .map(|line| Line::new(vec![Span::styled(line, base)]))
            .collect());
    };
    // CodeRenderable's actual converter uses SyntaxStyle's `default` rule for
    // unhighlighted bytes, even when its plain/unknown-language fg differs.
    let base = base.fg(theme.text());
    let captures = SYNTAX.with(|owner| owner.borrow_mut().captures(grammar, source))?;
    let captures: Vec<_> = captures
        .into_iter()
        .filter_map(|mut capture| {
            // Empty grammar captures are part of the exact query response, but
            // have no rendered bytes and cannot open an unclosed active style.
            if capture.start == capture.end
                || capture.end <= window.start
                || capture.start >= window.end
            {
                return None;
            }
            capture.start = capture.start.max(window.start) - window.start;
            capture.end = capture.end.min(window.end) - window.start;
            Some(capture)
        })
        .collect();
    let source = &source[window];
    let mut boundaries = Vec::with_capacity(captures.len() * 2);
    for (index, capture) in captures.iter().enumerate() {
        boundaries.push((capture.start, true, index));
        boundaries.push((capture.end, false, index));
    }
    boundaries.sort_by_key(|&(offset, start, index)| (offset, start, index));
    let mut active: BTreeSet<usize> = BTreeSet::new();
    let mut spans = Vec::new();
    let mut cursor = 0;
    for (offset, start, index) in boundaries {
        if offset > cursor {
            let mut groups: Vec<_> = active.iter().map(|&i| (i, &captures[i])).collect();
            let concealed = groups.iter().find(|(_, c)| {
                conceal && {
                    c.conceal.is_some() || c.scope == "conceal" || c.scope.starts_with("conceal.")
                }
            });
            if let Some((_, capture)) = concealed {
                let replacement =
                    capture
                        .conceal
                        .unwrap_or(if capture.scope == "conceal.with.space" {
                            " "
                        } else {
                            ""
                        });
                if !replacement.is_empty() {
                    spans.push(Span::styled(replacement, Style::default().fg(theme.text())));
                }
            } else {
                let inside = groups.iter().any(|(_, c)| c.contains_injection);
                groups.sort_by_key(|(i, c)| (c.scope.bytes().filter(|&b| b == b'.').count(), *i));
                let mut style = base;
                for (_, capture) in groups {
                    if inside && !capture.is_injection && capture.scope == "markup.raw.block" {
                        continue;
                    }
                    if let Some(token) = styles::capture(capture.scope, theme) {
                        style = style.patch(token);
                    }
                }
                spans.push(Span::styled(&source[cursor..offset], style));
            }
            cursor = offset;
        }
        if start {
            active.insert(index);
        } else {
            active.remove(&index);
        }
    }
    if cursor < source.len() {
        spans.push(Span::styled(&source[cursor..], base));
    }
    let mut rows = vec![Vec::new()];
    for span in spans {
        for (index, part) in span.content().split('\n').enumerate() {
            if index > 0 {
                rows.push(Vec::new());
            }
            if !part.is_empty() {
                rows.last_mut()
                    .expect("current row")
                    .push(Span::styled(part, span.style()));
            }
        }
    }
    Ok(rows.into_iter().map(Line::new).collect())
}

#[cfg(test)]
mod tests;
