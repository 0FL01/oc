//! Bounded HTML token renderer using html5ever 0.35.0 (MIT/Apache-2.0).
//! Structure rules follow the pinned OpenCode html-markdown.ts (MIT), not a
//! regular-expression HTML parser. No DOM, resource loading or script execution.
use super::{CONTENT_CAP_BYTES, FetchError, FetchFormat};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer,
};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub(super) fn checkpoint(deadline: Instant, cancel: &AtomicBool) -> Result<(), FetchError> {
    if Instant::now() >= deadline {
        return Err(FetchError::Deadline);
    }
    if cancel.load(Ordering::Acquire) {
        return Err(FetchError::Cancelled);
    }
    Ok(())
}

pub(super) fn convert(
    body: &[u8],
    mime: Option<&str>,
    format: FetchFormat,
    deadline: Instant,
    cancel: &AtomicBool,
) -> Result<(String, bool), FetchError> {
    checkpoint(deadline, cancel)?;
    let text = String::from_utf8_lossy(body);
    if !matches!(mime, Some("text/html" | "application/xhtml+xml")) || format == FetchFormat::Html {
        let mut output = text.into_owned();
        let truncated = output.len() > CONTENT_CAP_BYTES;
        trim(&mut output, CONTENT_CAP_BYTES);
        checkpoint(deadline, cancel)?;
        return Ok((output, truncated));
    }
    let renderer = Renderer {
        state: RefCell::new(State::default()),
        format,
        deadline,
        cancel,
    };
    let tokenizer = Tokenizer::new(renderer, Default::default());
    let input = BufferQueue::default();
    let mut offset = 0;
    while offset < text.len() {
        checkpoint(deadline, cancel)?;
        let mut end = (offset + 8192).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        input.push_back(StrTendril::from_slice(&text[offset..end]));
        let _ = tokenizer.feed(&input);
        if let Some(error) = tokenizer.sink.state.borrow().error.clone() {
            return Err(error);
        }
        offset = end;
    }
    tokenizer.end();
    checkpoint(deadline, cancel)?;
    let mut state = tokenizer.sink.state.into_inner();
    if let Some(error) = state.error {
        return Err(error);
    }
    state.finish_code();
    while let Some(frame) = state.frames.pop() {
        state.close(frame, format);
    }
    let output = state.output.trim().to_string();
    Ok((output, state.truncated))
}

pub(super) fn trim(text: &mut String, cap: usize) {
    if text.len() <= cap {
        return;
    }
    let mut end = cap;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}

struct Renderer<'a> {
    state: RefCell<State>,
    format: FetchFormat,
    deadline: Instant,
    cancel: &'a AtomicBool,
}

#[derive(Default)]
struct State {
    output: String,
    frames: Vec<Frame>,
    code: Option<Code>,
    error: Option<FetchError>,
    truncated: bool,
    space: bool,
    table_row: usize,
    table_columns: usize,
}
struct Frame {
    name: String,
    suppressed: bool,
    suffix: String,
    ordered_next: Option<i64>,
}
struct Code {
    owner: &'static str,
    inline: bool,
    language: String,
    text: String,
}

impl State {
    fn append(&mut self, value: &str) {
        let room = CONTENT_CAP_BYTES.saturating_sub(self.output.len());
        let mut end = room.min(value.len());
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.output.push_str(&value[..end]);
        self.truncated |= end < value.len();
    }
    fn block(&mut self) {
        self.space = false;
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.append("\n");
        }
    }
    fn text(&mut self, value: &str, format: FetchFormat) {
        if let Some(code) = &mut self.code {
            let room = CONTENT_CAP_BYTES.saturating_sub(code.text.len());
            let mut end = room.min(value.len());
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            code.text.push_str(&value[..end]);
            self.truncated |= end < value.len();
            return;
        }
        for character in value.chars() {
            if character.is_whitespace() {
                self.space = true;
                continue;
            }
            if self.space && !self.output.is_empty() && !self.output.ends_with(['\n', ' ']) {
                self.append(" ");
            }
            self.space = false;
            if format == FetchFormat::Markdown && "\\`*_[]<>|~".contains(character) {
                self.append("\\");
            }
            self.append(character.encode_utf8(&mut [0; 4]));
        }
    }
    fn finish_code(&mut self) {
        let Some(code) = self.code.take() else {
            return;
        };
        // Longest backtick run determines a faithful, non-colliding fence.
        let longest = code
            .text
            .split(|c| c != '`')
            .map(str::len)
            .max()
            .unwrap_or(0);
        let fence = "`".repeat((longest + 1).max(if code.inline { 1 } else { 3 }));
        if code.inline && self.space && !self.output.ends_with(['\n', ' ']) {
            self.append(" ");
        }
        self.space = false;
        if !code.inline {
            self.block();
        }
        self.append(&fence);
        if !code.inline {
            self.append(&code.language);
            self.append("\n");
        }
        let padding = code.inline && code.text.starts_with(['`', ' ']);
        if padding {
            self.append(" ");
        }
        self.append(&code.text);
        if padding {
            self.append(" ");
        }
        if !code.inline && !code.text.ends_with('\n') {
            self.append("\n");
        }
        self.append(&fence);
        if !code.inline {
            self.block();
        }
    }
    fn close(&mut self, frame: Frame, format: FetchFormat) {
        if frame.suppressed {
            return;
        }
        if self
            .code
            .as_ref()
            .is_some_and(|code| code.owner == frame.name)
        {
            self.finish_code();
        }
        self.append(&frame.suffix);
        if matches!(frame.name.as_str(), "td" | "th") {
            self.append(" ");
        }
        if frame.name == "tr" {
            if format == FetchFormat::Markdown {
                self.append("|\n");
                if self.table_row == 0 {
                    for _ in 0..self.table_columns {
                        self.append("| --- ");
                    }
                    self.append("|\n");
                }
            } else {
                self.block();
            }
            self.table_row += 1;
        } else if is_block(&frame.name) {
            self.block();
        }
    }
    fn tag(&mut self, tag: Tag, format: FetchFormat) {
        let name = tag.name.to_string();
        if tag.kind == TagKind::EndTag {
            if let Some(index) = self.frames.iter().rposition(|frame| frame.name == name) {
                while self.frames.len() > index {
                    let frame = self.frames.pop().expect("frame");
                    self.close(frame, format);
                }
            }
            return;
        }
        if self.frames.len() >= 1024 {
            self.error = Some(FetchError::TooLarge);
            return;
        }
        let attr = |key: &str| {
            tag.attrs
                .iter()
                .find(|a| a.name.local.as_ref() == key)
                .map(|a| a.value.as_ref())
        };
        let suppressed = self.frames.last().is_some_and(|f| f.suppressed)
            || matches!(
                name.as_str(),
                "head"
                    | "script"
                    | "style"
                    | "noscript"
                    | "iframe"
                    | "object"
                    | "embed"
                    | "meta"
                    | "link"
                    | "template"
            )
            || attr("hidden").is_some()
            || attr("aria-hidden") == Some("true");
        let mut frame = Frame {
            name: name.clone(),
            suppressed,
            suffix: String::new(),
            ordered_next: None,
        };
        if !suppressed {
            if let Some(code) = &mut self.code {
                if name == "br" {
                    code.text.push('\n');
                }
                if name == "code" {
                    code.language = attr("class")
                        .unwrap_or("")
                        .split_whitespace()
                        .find_map(|part| {
                            part.strip_prefix("language-")
                                .or_else(|| part.strip_prefix("lang-"))
                        })
                        .unwrap_or("")
                        .chars()
                        .take(64)
                        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+'))
                        .collect();
                }
            } else {
                if is_block(&name) {
                    self.block();
                }
                if format == FetchFormat::Markdown {
                    match name.as_str() {
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                            self.append(&"#".repeat(usize::from(name.as_bytes()[1] - b'0')));
                            self.append(" ");
                        }
                        "strong" | "b" => {
                            self.append("**");
                            frame.suffix = "**".into();
                        }
                        "em" | "i" => {
                            self.append("*");
                            frame.suffix = "*".into();
                        }
                        "s" | "strike" | "del" => {
                            self.append("~~");
                            frame.suffix = "~~".into();
                        }
                        "a" => {
                            self.append("[");
                            frame.suffix =
                                format!("]({})", destination(attr("href").unwrap_or("")));
                        }
                        "pre" | "code" => {
                            self.code = Some(Code {
                                owner: if name == "pre" { "pre" } else { "code" },
                                inline: name == "code",
                                language: String::new(),
                                text: String::new(),
                            });
                        }
                        "ol" => {
                            frame.ordered_next =
                                Some(attr("start").and_then(|v| v.parse().ok()).unwrap_or(1));
                        }
                        "li" => {
                            let list = self
                                .frames
                                .iter_mut()
                                .rev()
                                .find(|f| matches!(f.name.as_str(), "ol" | "ul"));
                            let marker = if let Some(next) =
                                list.and_then(|f| f.ordered_next.as_mut())
                            {
                                if let Some(value) = attr("value").and_then(|v| v.parse().ok()) {
                                    *next = value;
                                }
                                let marker = format!("{next}. ");
                                *next = next.saturating_add(1);
                                marker
                            } else {
                                "- ".into()
                            };
                            let depth = self
                                .frames
                                .iter()
                                .filter(|f| f.name == "li")
                                .count()
                                .min(12);
                            self.append(&"  ".repeat(depth));
                            self.append(&marker);
                        }
                        "blockquote" => self.append("> "),
                        "hr" => {
                            self.append("---");
                            self.block();
                        }
                        "table" => self.table_row = 0,
                        "tr" => self.table_columns = 0,
                        "td" | "th" => {
                            self.append("| ");
                            self.table_columns += 1;
                        }
                        _ => {}
                    }
                } else if name == "td" || name == "th" {
                    self.append(" ");
                }
                if name == "br" {
                    self.block();
                }
            }
        }
        if !matches!(
            name.as_str(),
            "area"
                | "base"
                | "br"
                | "col"
                | "embed"
                | "hr"
                | "img"
                | "input"
                | "link"
                | "meta"
                | "param"
                | "source"
                | "track"
                | "wbr"
        ) {
            self.frames.push(frame);
        }
    }
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "h1" | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "p"
            | "div"
            | "section"
            | "article"
            | "main"
            | "ul"
            | "ol"
            | "li"
            | "pre"
            | "blockquote"
            | "table"
            | "tr"
            | "dl"
            | "dt"
            | "dd"
    )
}
fn destination(value: &str) -> String {
    let mut output = String::new();
    for c in value.chars() {
        if c.is_whitespace() {
            output.push_str("%20");
        } else {
            if "\\()".contains(c) {
                output.push('\\');
            }
            output.push(c);
        }
    }
    output
}

impl TokenSink for Renderer<'_> {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut state = self.state.borrow_mut();
        if state.error.is_some() {
            return TokenSinkResult::Continue;
        }
        if let Err(error) = checkpoint(self.deadline, self.cancel) {
            state.error = Some(error);
            return TokenSinkResult::Continue;
        }
        match token {
            Token::TagToken(tag) => {
                // The tokenizer, rather than a home-grown tag scanner, owns
                // HTML raw-text/script states and entity decoding.
                let raw = if tag.kind == TagKind::StartTag {
                    match tag.name.as_ref() {
                        "script" => Some(html5ever::tokenizer::states::RawKind::ScriptData),
                        "style" | "iframe" | "noscript" => {
                            Some(html5ever::tokenizer::states::RawKind::Rawtext)
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                state.tag(tag, self.format);
                if let Some(raw) = raw {
                    return TokenSinkResult::RawData(raw);
                }
            }
            Token::CharacterTokens(value) if !state.frames.last().is_some_and(|f| f.suppressed) => {
                let mut offset = 0;
                while offset < value.len() {
                    if let Err(error) = checkpoint(self.deadline, self.cancel) {
                        state.error = Some(error);
                        break;
                    }
                    let mut end = (offset + 4096).min(value.len());
                    while !value.is_char_boundary(end) {
                        end -= 1;
                    }
                    state.text(&value[offset..end], self.format);
                    offset = end;
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}
