//! Native port of OpenCode 2.2.5's non-PowerShell shell scanner and permission
//! projection (core/src/shell/{scan,parse}.ts, ff90ed91607cabf420a369e9009539c1aa0932af).
//! Copyright (c) 2025 opencode. MIT notice: fixtures/OpenCode-MIT.txt.
//!
//! This analyzes source, never executes it. An opaque nested construct rejects
//! the entire invocation; source spans are permission resources, not argv.
use regex::Regex;
use std::{cell::RefCell, rc::Rc, sync::LazyLock};

type Result<T> = std::result::Result<T, &'static str>;
type Command = Rc<RefCell<RawCommand>>;
type Heredocs = Rc<RefCell<Vec<Heredoc>>>;
const INPUT_LIMIT: usize = 64 * 1024;
const DEPTH_LIMIT: usize = 32;

macro_rules! re {
    (search $pattern:literal) => {{
        static RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($pattern).expect("static shell grammar"));
        &*RE
    }};
    ($pattern:literal) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(concat!("^(?:", $pattern, ")")).expect("static shell grammar")
        });
        &*RE
    }};
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    Bash,
    Zsh,
    Posix,
}
impl Dialect {
    pub(crate) fn for_shell(shell: &str) -> Self {
        match std::path::Path::new(shell)
            .file_name()
            .and_then(|s| s.to_str())
        {
            Some(name) if name.eq_ignore_ascii_case("bash") => Self::Bash,
            Some(name) if name.eq_ignore_ascii_case("zsh") => Self::Zsh,
            _ => Self::Posix,
        }
    }
}

pub(crate) struct PermissionCommand {
    pub resource: String,
    pub save: String,
}
pub(crate) struct Parsed {
    pub commands: Vec<PermissionCommand>,
    pub directories: Vec<String>,
}
#[derive(Default)]
struct RawCommand {
    resource: String,
    words: Vec<String>,
    raw_words: Vec<String>,
    declaration: bool,
    redirect_word_count: Option<usize>,
    redirect_prefix_end: Option<usize>,
}
#[derive(Default)]
struct Text {
    word: String,
    literal: String,
    output: String,
}
impl Text {
    fn append(&mut self, value: &str) {
        self.word.push_str(value);
        self.literal.push_str(value);
    }
}
struct Context {
    remaining: usize,
    dialect: Dialect,
    bound_dollar: bool,
    bound_open: bool,
    bound_bracket: bool,
    sink_bracket: bool,
    variable_sink: bool,
    eval_sink: bool,
}
impl Context {
    fn step(&mut self) -> Result<()> {
        self.remaining = self.remaining.checked_sub(1).ok_or("invalid-structure")?;
        Ok(())
    }
}
struct Scanner<'a, 'c> {
    input: &'a str,
    context: &'c mut Context,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Close {
    Paren,
    Brace,
    NoFork,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Word,
    Quoted,
    Arithmetic,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Evaluation {
    Arithmetic,
    Binding,
    Deferred,
}
#[derive(Clone, Copy)]
struct Span {
    open: Option<char>,
    close: Option<char>,
    reject: &'static str,
    mode: Mode,
}
const DOUBLE: Span = Span {
    open: None,
    close: Some('"'),
    reject: "",
    mode: Mode::Quoted,
};
const HEREDOC: Span = Span {
    open: None,
    close: None,
    reject: "",
    mode: Mode::Quoted,
};
const ARITHMETIC: Span = Span {
    open: Some('('),
    close: Some(')'),
    reject: ";#}",
    mode: Mode::Arithmetic,
};
const FOR_ARITHMETIC: Span = Span {
    reject: "#}",
    ..ARITHMETIC
};
const BRACKET_ARITHMETIC: Span = Span {
    open: None,
    close: Some(']'),
    reject: ";|&<>()[\n'\"\\#",
    mode: Mode::Arithmetic,
};
const SUBSCRIPT: Span = Span {
    open: Some('['),
    close: Some(']'),
    reject: "\"}",
    mode: Mode::Arithmetic,
};
const QUOTED_SUBSCRIPT: Span = Span {
    reject: "\"",
    mode: Mode::Quoted,
    ..SUBSCRIPT
};
const ASSIGNMENT_SUBSCRIPT: Span = Span {
    reject: " \t\n\r\u{b}\u{c};&|<>()",
    ..SUBSCRIPT
};
const ARITHMETIC_QUOTE: Span = Span {
    open: None,
    close: Some('\''),
    reject: "()[];\"}",
    mode: Mode::Quoted,
};
const ARITHMETIC_DOUBLE: Span = Span {
    reject: "()[];}",
    ..DOUBLE
};
const PARAMETER_QUOTE: Span = Span {
    reject: "\"}[]",
    ..ARITHMETIC_QUOTE
};

struct Heredoc {
    delimiter: String,
    dash_delimiter: Option<String>,
    quoted: bool,
    tabs: bool,
    binding: bool,
    owned: bool,
    command: Option<(Command, usize)>,
}
struct Structure {
    kind: &'static str,
    phase: &'static str,
    count: usize,
    saw_else: bool,
    saw_in: bool,
    pattern_started: bool,
    parenthesized: bool,
}
#[derive(Default)]
struct List<'a> {
    commands: Vec<Command>,
    nested: Vec<Command>,
    words: Vec<String>,
    raw_words: Vec<String>,
    heredocs: Heredocs,
    structures: Vec<Structure>,
    text: Text,
    word_started: bool,
    word_start: usize,
    word_end: usize,
    command_start: Option<usize>,
    redirect_word_count: Option<usize>,
    redirect_prefix_end: Option<usize>,
    ampersand_redirect_words: Option<usize>,
    command_word_index: Option<usize>,
    assignment_word: bool,
    assignment_head_unsafe: bool,
    invalid: Option<&'static str>,
    redirect_target: bool,
    dup_redirect: bool,
    here_string: bool,
    has_redirect: bool,
    has_file_redirect: bool,
    compound_end: bool,
    dangling: bool,
    outer: Option<&'a mut Text>,
}
impl List<'_> {
    fn statement(&mut self) {
        if let Some(s) = self.structures.last_mut() {
            s.count += 1;
        }
    }
    fn in_header(&self) -> bool {
        self.structures
            .last()
            .is_some_and(|s| matches!(s.phase, "header" | "pattern" | "do"))
    }
    fn at_start(&self) -> bool {
        self.words.is_empty() && !self.has_redirect && !self.compound_end && !self.in_header()
    }
    fn pending(&self) -> bool {
        self.word_started || !self.words.is_empty() || self.has_redirect || self.compound_end
    }
    fn crosses_heredoc(&self, input: &str, start: usize, end: usize) -> bool {
        !self.heredocs.borrow().is_empty() && input[start..end].contains('\n')
    }
}

fn ch(input: &str, at: usize) -> char {
    input
        .get(at..)
        .and_then(|s| s.chars().next())
        .unwrap_or('\0')
}
fn skip_continuations(input: &str, mut at: usize) -> usize {
    while input[at..].starts_with("\\\n") {
        at += 2;
    }
    at
}
fn boundary(input: &str, at: usize) -> bool {
    let at = skip_continuations(input, at);
    at == input.len() || " \t\n;&|()<>".contains(ch(input, at))
}
fn space_ahead(input: &str, at: usize) -> bool {
    " \t\n".contains(ch(input, skip_continuations(input, at)))
}
fn token(input: &str, at: usize) -> Option<&str> {
    let text = re!(r"[A-Za-z_][A-Za-z0-9_]*").find(&input[at..])?.as_str();
    boundary(input, at + text.len()).then_some(text)
}
fn declaration(name: &str) -> bool {
    matches!(
        name,
        "declare" | "typeset" | "export" | "readonly" | "local" | "unset" | "unsetenv"
    )
}
fn assignment_declaration(name: &str) -> bool {
    declaration(name) && !matches!(name, "unset" | "unsetenv")
}
fn binding_builtin(name: &str) -> bool {
    matches!(
        name,
        "declare"
            | "typeset"
            | "local"
            | "export"
            | "readonly"
            | "private"
            | "unset"
            | "read"
            | "getln"
            | "vared"
            | "getopts"
            | "mapfile"
            | "readarray"
            | "compgen"
            | "zparseopts"
            | "zregexparse"
            | "zformat"
            | "zstyle"
            | "zstat"
            | "stat"
    )
}
fn operator<'a>(input: &str, at: usize, operators: &[&'a str]) -> Result<Option<&'a str>> {
    let mut text = String::new();
    let mut cursor = at;
    while text.len() < 3 && cursor < input.len() {
        if input[cursor..].starts_with("\\\n") {
            cursor += 2;
        } else {
            let c = ch(input, cursor);
            text.push(c);
            cursor += c.len_utf8();
        }
    }
    let found = operators
        .iter()
        .find(|candidate| text.starts_with(**candidate))
        .copied();
    if found.is_some_and(|s| !input[at..].starts_with(s)) {
        return Err("invalid-structure");
    }
    Ok(found)
}

impl Scanner<'_, '_> {
    fn builtin_index(&self, s: &List<'_>) -> usize {
        let mut name = s.command_word_index.unwrap_or(s.words.len());
        while s.words.get(name).is_some_and(|w| {
            matches!(w.as_str(), "builtin" | "command" | "time" | "coproc" | "!")
                || (self.context.dialect != Dialect::Bash
                    && matches!(w.as_str(), "noglob" | "nocorrect" | "exec" | "-"))
        }) {
            let wrapper = &s.words[name];
            name += 1;
            while wrapper != "-" && s.words.get(name).is_some_and(|w| w.starts_with('-')) {
                if wrapper == "exec" && re!(r"-[cl]*a$").is_match(&s.words[name]) {
                    name += 1;
                }
                name += 1;
            }
        }
        name
    }
    fn word_evaluation(&self, s: &List<'_>) -> Option<Evaluation> {
        if s.redirect_target {
            return s.here_string.then_some(Evaluation::Deferred);
        }
        if let Some(st) = s.structures.last() {
            if st.kind == "repeat" && st.phase == "header" {
                return Some(Evaluation::Arithmetic);
            }
            if st.kind == "for" && st.phase == "header" && st.saw_in {
                return Some(Evaluation::Deferred);
            }
        }
        s.command_word_index?;
        let name = self.builtin_index(s);
        let command = s.words.get(name)?.as_str();
        if command == "let"
            || (self.context.dialect != Dialect::Bash
                && matches!(
                    command,
                    "integer"
                        | "float"
                        | "exit"
                        | "return"
                        | "shift"
                        | "break"
                        | "continue"
                        | "bye"
                        | "logout"
                ))
        {
            return Some(Evaluation::Arithmetic);
        }
        let options = &s.words[name + 1..];
        if command == "set" {
            return Some(
                if options
                    .iter()
                    .any(|w| re!(r"[-+][A-Za-z0-9]*A").is_match(w))
                {
                    Evaluation::Binding
                } else {
                    Evaluation::Deferred
                },
            );
        }
        if binding_builtin(command) {
            if options
                .iter()
                .any(|w| w.starts_with('-') && w.contains(['E', 'F', 'i', 'n']))
            {
                return Some(Evaluation::Arithmetic);
            }
            if s.assignment_word
                && assignment_declaration(command)
                && options.iter().all(|w| !w.starts_with('-'))
                && (matches!(command, "export" | "readonly")
                    || self.context.dialect == Dialect::Zsh
                    || !re!(search r"\+?=(?:\(|[$`])").is_match(&s.text.word))
            {
                return Some(Evaluation::Deferred);
            }
            return Some(Evaluation::Binding);
        }
        if matches!(command, "printf" | "print") {
            let flag = if command == "print" {
                re!(r"-[A-Za-z]*v")
            } else {
                re!(r"-v")
            };
            return options
                .iter()
                .chain(std::iter::once(&s.text.word))
                .any(|w| flag.is_match(w))
                .then_some(Evaluation::Binding);
        }
        if command == "wait"
            && (re!(r"-[nf]*p").is_match(&s.text.word)
                || s.words.last().is_some_and(|w| re!(r"-[nf]*p$").is_match(w)))
        {
            return Some(Evaluation::Binding);
        }
        if matches!(command, "test" | "[") {
            if s.words.last().is_some_and(|w| w == "-v") {
                return Some(Evaluation::Binding);
            }
            if self.context.dialect != Dialect::Bash && s.words.last().is_some_and(|w| w == "-t") {
                return Some(Evaluation::Arithmetic);
            }
        }
        None
    }
    fn finish_word(&mut self, s: &mut List<'_>) {
        if !s.word_started {
            return;
        }
        if s.text.word.contains("PS4") {
            self.context.eval_sink = true;
        }
        let evaluation = self.word_evaluation(s).or_else(|| {
            (s.command_word_index.is_none() && s.assignment_word).then_some(Evaluation::Deferred)
        });
        if self.context.dialect != Dialect::Bash && (s.text.word.contains("NULLCMD") ||
            ((s.assignment_word || evaluation == Some(Evaluation::Binding)) && re!(r"(?:-[A-Za-z]*v)?(?:(?:dis_)?(?:functions|[gs]?aliases)|options|commands)(?:\+?=|\[|$)").is_match(&s.text.word))) {
            s.invalid.get_or_insert("dynamic-execution");
        }
        if evaluation.is_some_and(|e| evaluates(&s.text, e, self.context)) {
            s.invalid.get_or_insert("dynamic-execution");
        }
        if s.text.word != s.text.literal
            && s.command_word_index.is_some()
            && s.words
                .get(self.builtin_index(s))
                .is_some_and(|w| matches!(w.as_str(), "declare" | "typeset" | "local"))
        {
            self.context.eval_sink = true;
        }
        if let Some(outer) = s.outer.as_mut() {
            outer
                .output
                .push_str(&format!(" {} {}", s.text.literal, s.text.output));
        }
        let repeat = s
            .structures
            .last()
            .is_some_and(|st| st.kind == "repeat" && st.phase == "header");
        if !s.redirect_target && repeat {
            s.structures.last_mut().expect("repeat").phase = "do";
        }
        if !s.redirect_target && !repeat {
            if !s.assignment_word && s.command_word_index.is_none() {
                s.command_word_index = Some(s.words.len());
            }
            s.command_start.get_or_insert(s.word_start);
            s.words.push(s.text.word.clone());
            s.raw_words
                .push(self.input[s.word_start..s.word_end].into());
        }
        if s.redirect_target {
            if !s.dup_redirect
                || !re!(r"(?:[0-9]+-?|-)$").is_match(&self.input[s.word_start..s.word_end])
            {
                s.has_file_redirect = true;
            }
            s.redirect_prefix_end.get_or_insert(s.word_end);
        }
        s.redirect_target = false;
        s.dup_redirect = false;
        s.here_string = false;
        s.text = Text::default();
        s.word_started = false;
        s.assignment_word = false;
        s.assignment_head_unsafe = false;
    }
    fn finish_command(&mut self, s: &mut List<'_>, boundary: bool, newline: bool) {
        self.finish_word(s);
        if s.redirect_target {
            s.invalid.get_or_insert("invalid-redirect");
        }
        s.redirect_target = false;
        if s.compound_end && !s.words.is_empty() {
            s.invalid.get_or_insert("invalid-structure");
        }
        let in_header = s.in_header();
        let mut owner = None;
        if let Some(name) = s.command_word_index {
            if s.words[name].is_empty() {
                s.invalid.get_or_insert("invalid-structure");
            }
            if !in_header {
                let resource = self.input[s.command_start.expect("command start")..s.word_end]
                    .trim_matches([' ', '\t', '\n'])
                    .to_owned();
                let decl = name == 0
                    && !s.has_redirect
                    && declaration(&s.raw_words[0])
                    && resource.starts_with(&s.raw_words[0]);
                let command = Rc::new(RefCell::new(RawCommand {
                    resource,
                    words: s.words[name..].to_vec(),
                    raw_words: s.raw_words[name..].to_vec(),
                    declaration: decl,
                    redirect_word_count: s
                        .redirect_word_count
                        .filter(|n| *n < s.words.len())
                        .map(|n| n - name),
                    redirect_prefix_end: None,
                }));
                s.commands.push(command.clone());
                let builtin = self.builtin_index(s);
                if let Some(name) = s.words.get(builtin) {
                    let args = &s.words[builtin + 1..];
                    if name == "eval" {
                        self.context.eval_sink = true;
                    }
                    if (name == "set"
                        && args
                            .iter()
                            .any(|w| re!(r"-[A-Za-z0-9]*k").is_match(w) || w == "keyword"))
                        || (name == "alias" && args.iter().any(|w| w.contains('=')))
                    {
                        s.invalid.get_or_insert("dynamic-execution");
                    }
                    if self.context.dialect != Dialect::Bash
                        && (name == "emulate"
                            || ((matches!(name.as_str(), "setopt" | "unsetopt")
                                || (name == "set"
                                    && args
                                        .iter()
                                        .any(|w| re!(r"[-+][A-Za-z0-9]*o").is_match(w))))
                                && args.iter().any(|w| {
                                    let option = re!(r"(?i:[-+][A-Za-z0-9]*?o)")
                                        .replace(w, "")
                                        .to_lowercase()
                                        .replace(['_', '-'], "");
                                    re!(r"-[A-Za-z0-9]*m").is_match(w)
                                        || matches!(
                                            option.strip_prefix("no").unwrap_or(&option),
                                            "globsubst"
                                                | "kshglob"
                                                | "promptsubst"
                                                | "promptvars"
                                                | "aliases"
                                        )
                                })))
                    {
                        s.invalid.get_or_insert("dynamic-execution");
                    }
                    if self.context.dialect != Dialect::Zsh
                        && name == "compgen"
                        && args
                            .iter()
                            .any(|w| w.starts_with('-') && w.contains(['C', 'F']))
                    {
                        s.invalid.get_or_insert("dynamic-execution");
                    }
                    if binding_builtin(name) {
                        for h in s.heredocs.borrow_mut().iter_mut() {
                            h.binding = true;
                        }
                    }
                }
                owner = Some(command);
            }
        } else if !in_header
            && let Some(start) = s.command_start
            && (s.has_file_redirect || (s.has_redirect && !s.compound_end))
        {
            let raw = &self.input[start..s.word_end];
            let leading = raw.len() - raw.trim_start_matches([' ', '\t', '\n']).len();
            let resource = raw[leading..].trim_end_matches([' ', '\t', '\n']);
            if !resource.is_empty() {
                let end = s.redirect_prefix_end.map_or(resource.len(), |n| {
                    n.saturating_sub(start + leading).clamp(1, resource.len())
                });
                let prefix = resource[..end].to_owned();
                let command = Rc::new(RefCell::new(RawCommand {
                    resource: resource.into(),
                    words: vec![prefix.clone()],
                    raw_words: vec![prefix],
                    redirect_prefix_end: Some(end),
                    ..RawCommand::default()
                }));
                s.commands.push(command.clone());
                owner = Some(command);
            }
        }
        for h in s.heredocs.borrow_mut().iter_mut() {
            if let Some(owner) = &owner
                && newline
                && !h.owned
            {
                h.command = Some((
                    owner.clone(),
                    s.command_start.expect("heredoc command start"),
                ));
            }
            h.owned = true;
        }
        if boundary && s.words.is_empty() && !s.has_redirect && !s.compound_end && !in_header {
            s.invalid.get_or_insert("invalid-structure");
        }
        if s.ampersand_redirect_words
            .is_some_and(|n| s.words.len() > n)
        {
            s.invalid.get_or_insert("invalid-redirect");
        }
        s.commands.append(&mut s.nested);
        if !in_header && (!s.words.is_empty() || s.has_redirect) {
            s.statement();
        }
        s.words.clear();
        s.raw_words.clear();
        s.command_word_index = None;
        s.command_start = None;
        s.dup_redirect = false;
        s.has_redirect = false;
        s.has_file_redirect = false;
        s.redirect_word_count = None;
        s.redirect_prefix_end = None;
        s.ampersand_redirect_words = None;
        s.compound_end = false;
        s.dangling = false;
    }
    fn end_list(&mut self, s: &mut List<'_>) -> Result<()> {
        if s.pending() {
            self.finish_command(s, false, true);
        }
        if s.dangling {
            return Err("invalid-structure");
        }
        Ok(())
    }
    fn close_list(&mut self, mut s: List<'_>, end: usize) -> Result<(Vec<Command>, usize)> {
        self.end_list(&mut s)?;
        if let Some(reason) = s.invalid {
            return Err(reason);
        }
        if !s.structures.is_empty() {
            return Err("compound-command");
        }
        Ok((s.commands, end))
    }
    fn list(
        &mut self,
        start: usize,
        depth: usize,
        close: Option<Close>,
        heredocs: Option<Heredocs>,
        outer: Option<&mut Text>,
    ) -> Result<(Vec<Command>, usize)> {
        if depth > DEPTH_LIMIT {
            return Err("invalid-structure");
        }
        let shared_heredocs = heredocs.is_some();
        let mut s = List {
            heredocs: heredocs.unwrap_or_default(),
            outer,
            word_start: start,
            word_end: start,
            ..List::default()
        };
        let input = self.input;
        let mut index = start;
        while index < input.len() {
            self.context.step()?;
            let c = ch(input, index);
            if input[index..].starts_with("\\\n") {
                index += 2;
                continue;
            }
            if !s.word_started {
                s.word_start = index;
                if matches!(c, ' ' | '\t') {
                    index += 1;
                    continue;
                }
                if c == '}'
                    && s.words.is_empty()
                    && !s.redirect_target
                    && matches!(close, Some(Close::Brace | Close::NoFork))
                    && s.structures.is_empty()
                    && (shared_heredocs || s.heredocs.borrow().is_empty())
                    && (close == Some(Close::NoFork) || boundary(input, index + 1))
                {
                    return self.close_list(s, index);
                }
                if c == '}' {
                    return Err(if s.structures.is_empty() {
                        "invalid-structure"
                    } else {
                        "compound-command"
                    });
                }
                if let Some(next) = self.command_start(&mut s, index, depth)? {
                    index = next;
                    continue;
                }
            }
            if c == '\\' && index + 1 >= input.len() {
                return Err("unterminated-escape");
            }
            if !s.assignment_word
                && ("'\"\\".contains(c) || (c == '$' && "'\"".contains(ch(input, index + 1))))
            {
                s.assignment_head_unsafe = true;
            }
            let in_case = s.structures.last().is_some_and(|st| st.phase == "pattern");
            let array = matches!(c, '=' | '(')
                && s.assignment_word
                && s.text.word.len() > 1
                && s.text.word.ends_with('=')
                && !s.text.word.ends_with("==")
                && s.command_word_index
                    .is_none_or(|name| assignment_declaration(&s.raw_words[name]));
            if c == '=' && (!s.word_started || array) && ch(input, index + 1) == '(' {
                let end = self.nested(index + 2, depth, &mut s.nested, Close::Paren, None)?;
                s.word_started = true;
                s.text.word.push_str(&input[index..=end]);
                index = end + 1;
                s.word_end = index;
                continue;
            }
            if "\\$'\"`<>".contains(c) {
                let allow = !s.assignment_word
                    && s.command_word_index.is_some()
                    && !s.raw_words.first().is_some_and(|w| declaration(w));
                if let Some(end) = self.unit(
                    index,
                    depth,
                    &mut s.nested,
                    Mode::Word,
                    allow,
                    Some(&mut s.text),
                )? {
                    s.word_started = true;
                    index = end + 1;
                    s.word_end = index;
                    continue;
                }
            }
            if c == '['
                && !in_case
                && !s.assignment_word
                && !s.assignment_head_unsafe
                && s.command_word_index.is_none()
                && re!(r"[A-Za-z_][A-Za-z0-9_]*$").is_match(&s.text.word)
            {
                let mut subscript = vec![];
                let end =
                    self.subscript(index + 1, depth, &mut subscript, ASSIGNMENT_SUBSCRIPT, None)?;
                if operator(input, end + 1, &["+=", "="])?.is_some() {
                    if self.context.dialect == Dialect::Posix
                        && input[index + 1..end].contains(['$', '`'])
                    {
                        return Err("invalid-structure");
                    }
                    s.nested.extend(subscript);
                    s.assignment_word = true;
                    s.text.word.push_str(&input[index..=end]);
                    index = end + 1;
                    s.word_end = index;
                    continue;
                }
            }
            if c == '('
                && (array
                    || ((in_case || self.context.dialect == Dialect::Bash)
                        && s.text.word.ends_with(['?', '*', '+', '@', '!'])))
            {
                let end = self.array_pattern(index, depth + 1, &mut s.nested, array)?;
                if s.crosses_heredoc(input, index, end) {
                    return Err("heredoc");
                }
                if array && end + 1 < input.len() && !" \t\n;&|<>)".contains(ch(input, end + 1)) {
                    return Err("invalid-structure");
                }
                s.word_started = true;
                s.text.word.push_str(&input[index..=end]);
                index = end + 1;
                s.word_end = index;
                continue;
            }
            if c == '#' && !s.word_started {
                let Some(end) = input[index..].find('\n') else {
                    break;
                };
                index += end;
                continue;
            }
            if "<>&|;\n".contains(c) {
                index = self.separator(&mut s, index, depth)?;
                continue;
            }
            if in_case && c == ')' {
                self.finish_word(&mut s);
                continue;
            }
            if c == ')' {
                if close != Some(Close::Paren)
                    || !s.structures.is_empty()
                    || (!shared_heredocs && !s.heredocs.borrow().is_empty())
                {
                    return Err("compound-command");
                }
                return self.close_list(s, index);
            }
            if c == '('
                && self.context.dialect == Dialect::Zsh
                && s.word_started
                && !in_case
                && let Some(end) = glob_qualifier(input, index)
            {
                s.text.append(&input[index..end]);
                index = end;
                s.word_end = index;
                continue;
            }
            if c == '(' {
                return Err(if s.word_started {
                    "dynamic-execution"
                } else {
                    "compound-command"
                });
            }
            if shell_whitespace(c) && !matches!(c, ' ' | '\t' | '\n') {
                return Err("invalid-structure");
            }
            if matches!(c, ' ' | '\t') {
                self.finish_word(&mut s);
                index += 1;
                continue;
            }
            s.word_started = true;
            if c == '='
                && !s.assignment_word
                && !s.assignment_head_unsafe
                && re!(r"[A-Za-z_][A-Za-z0-9_]*\+?$").is_match(&s.text.word)
            {
                s.assignment_word = true;
            }
            s.text.append(&c.to_string());
            index += c.len_utf8();
            s.word_end = index;
        }
        if let Some(close) = close {
            return Err(if close == Close::Paren {
                "command-substitution"
            } else {
                "invalid-structure"
            });
        }
        if !s.heredocs.borrow().is_empty() {
            return Err("heredoc");
        }
        self.close_list(s, input.len())
    }
    fn separator(&mut self, s: &mut List<'_>, index: usize, depth: usize) -> Result<usize> {
        let input = self.input;
        let c = ch(input, index);
        let redirect = if "<>&".contains(c) {
            operator(
                input,
                index,
                &[
                    "&>>", "&>", "<<<", "<<-", "<<", "<>", "<&", ">&", ">|", ">>", ">", "<",
                ],
            )?
        } else {
            None
        };
        if let Some(redirect) = redirect {
            if s.structures
                .last()
                .is_some_and(|st| st.kind == "case" && st.phase != "body")
            {
                return Err("invalid-redirect");
            }
            s.has_redirect = true;
            s.command_start.get_or_insert(s.word_start);
            if s.redirect_target {
                s.invalid.get_or_insert("invalid-redirect");
            }
            let fd = s.word_started
                && !s.assignment_head_unsafe
                && re!(r"(?:[0-9]+|\{[A-Za-z_][A-Za-z0-9_]*\})$").is_match(&s.text.word);
            if fd {
                s.text = Text::default();
                s.word_started = false;
            } else {
                self.finish_word(s);
            }
            if redirect.starts_with('&') && self.context.dialect == Dialect::Posix {
                s.ampersand_redirect_words.get_or_insert(s.words.len());
            }
            if s.redirect_word_count.is_none() && s.command_word_index.is_some() {
                s.redirect_word_count = Some(s.words.len());
            }
            let end = index + redirect.len();
            if matches!(redirect, "<<" | "<<-") {
                let (h, end) =
                    heredoc_delimiter(input, end, redirect == "<<-").ok_or("invalid-redirect")?;
                s.has_file_redirect = true;
                s.redirect_prefix_end.get_or_insert(
                    if " \t".contains(ch(input, index + redirect.len())) {
                        index + redirect.len()
                    } else {
                        end + 1
                    },
                );
                s.heredocs.borrow_mut().push(h);
                s.word_end = end + 1;
                s.redirect_target = false;
                s.dup_redirect = false;
                return Ok(end + 1);
            }
            if " \t\n".contains(ch(input, end)) {
                s.redirect_prefix_end.get_or_insert(end);
            }
            s.redirect_target = true;
            s.dup_redirect = matches!(redirect, "<&" | ">&");
            s.here_string = redirect == "<<<";
            return Ok(end);
        }
        if s.structures.last().is_some_and(|st| st.phase == "pattern") && c == '|' {
            self.finish_word(s);
            return Ok(index);
        }
        let case_body = s
            .structures
            .last()
            .is_some_and(|st| st.kind == "case" && st.phase == "body");
        let operators: &[&str] = match (self.context.dialect, case_body) {
            (Dialect::Zsh, true) => &[";;&", ";;", ";&", ";|", "&&", "||", "|&", "&!", "&|"],
            (Dialect::Zsh, false) => &["&&", "||", "|&", "&!", "&|"],
            (_, true) => &[";;&", ";;", ";&", ";|", "&&", "||", "|&"],
            (_, false) => &["&&", "||", "|&"],
        };
        let one = c.to_string();
        let sep = operator(input, index, operators)?.unwrap_or(&one);
        if case_body && sep.starts_with(';') && sep.len() > 1 {
            self.end_list(s)?;
            let st = s.structures.last_mut().expect("case");
            st.phase = "pattern";
            st.pattern_started = false;
            return Ok(index + sep.len());
        }
        if let Some(st) = s.structures.last() {
            if st.kind == "case" && matches!(st.phase, "header" | "pattern") {
                if sep != "\n" || (st.phase == "pattern" && (s.word_started || !s.words.is_empty()))
                {
                    return Err("compound-command");
                }
                self.finish_word(s);
                return self.read_heredocs(s, index, depth);
            }
            if matches!(st.kind, "for" | "repeat")
                && matches!(st.phase, "header" | "do")
                && (!matches!(sep, ";" | "\n")
                    || (st.phase == "header"
                        && !st.saw_in
                        && !s.word_started
                        && s.words.is_empty()))
            {
                return Err("compound-command");
            }
        }
        if sep == "\n" && !s.pending() {
            return self.read_heredocs(s, index, depth);
        }
        if sep == ";" && self.context.dialect == Dialect::Zsh && !s.pending() && !s.dangling {
            return Ok(index + 1);
        }
        self.finish_command(s, true, sep == "\n");
        if let Some(st) = s.structures.last_mut()
            && st.kind == "for"
            && st.phase == "header"
        {
            st.phase = "do";
        }
        s.dangling = !(matches!(sep, ";" | "\n") || (sep.starts_with('&') && sep != "&&"));
        if sep == "\n" && !s.heredocs.borrow().is_empty() {
            return Ok(index);
        }
        Ok(index + sep.len())
    }
    fn read_heredocs(&mut self, s: &mut List<'_>, index: usize, depth: usize) -> Result<usize> {
        let mut end = index;
        let pending = std::mem::take(&mut *s.heredocs.borrow_mut());
        for h in pending {
            let (source, body_end) = heredoc_body(self.input, end + 1, &h).ok_or("heredoc")?;
            if let Some((command, start)) = &h.command {
                command.borrow_mut().resource = self.input[*start..body_end].trim().into();
            }
            let mut text = if h.quoted {
                Text {
                    word: source.into(),
                    literal: source.into(),
                    output: String::new(),
                }
            } else {
                Text::default()
            };
            if !h.quoted {
                Scanner {
                    input: source,
                    context: self.context,
                }
                .span(0, depth, &mut s.commands, HEREDOC, true, Some(&mut text))?;
            }
            if h.binding {
                evaluates(&text, Evaluation::Deferred, self.context);
            }
            end = body_end;
        }
        Ok(if end < self.input.len() { end + 1 } else { end })
    }
    fn command_start(
        &mut self,
        s: &mut List<'_>,
        index: usize,
        depth: usize,
    ) -> Result<Option<usize>> {
        let input = self.input;
        let c = ch(input, index);
        if let Some(st) = s.structures.last_mut()
            && st.phase == "pattern"
        {
            if !st.pattern_started && s.words.is_empty() && c == '(' {
                st.pattern_started = true;
                return Ok(Some(index + 1));
            }
            if c == '|' {
                if s.words.is_empty() {
                    return Err("compound-command");
                }
                self.finish_word(s);
                s.structures.last_mut().expect("case").pattern_started = true;
                return Ok(Some(index + 1));
            }
            if c == ')' {
                if s.words.is_empty() {
                    return Err("compound-command");
                }
                self.finish_command(s, false, false);
                let st = s.structures.last_mut().expect("case");
                st.phase = "body";
                st.count = 0;
                return Ok(Some(index + 1));
            }
        }
        if s.structures
            .last()
            .is_some_and(|st| st.kind == "for" && st.phase == "header")
            && c == '('
            && ch(input, index + 1) != '('
        {
            let end = self.array_pattern(index, depth + 1, &mut s.nested, true)?;
            if s.crosses_heredoc(input, index, end) {
                return Err("heredoc");
            }
            self.finish_command(s, false, false);
            let st = s.structures.last_mut().expect("for");
            st.phase = "do";
            st.parenthesized = true;
            if !do_ahead(input, end + 1) {
                s.structures.pop();
            }
            return Ok(Some(end + 1));
        }
        if s.structures
            .last()
            .is_some_and(|st| st.kind == "repeat" && st.phase == "do")
            && !matches!(c, '\n' | '#' | ';')
            && token(input, index) != Some("do")
        {
            s.structures.pop();
        }
        if let Some(next) = self.keyword(s, index, depth)? {
            return Ok(Some(next));
        }
        if s.structures
            .last()
            .is_some_and(|st| st.kind == "for" && st.phase == "do")
            && !matches!(c, '\n' | '#' | ';')
        {
            if c != '{' || !space_ahead(input, index + 1) {
                return Err("compound-command");
            }
            s.structures.pop();
        }
        let at_start = s.at_start();
        if at_start
            && c == '('
            && let Some(end) = function_head(input, index)
        {
            return Ok(Some(end));
        }
        if at_start
            && c == '!'
            && " \t\n(".contains(ch(input, skip_continuations(input, index + 1)))
        {
            return Ok(Some(index + 1));
        }
        let for_header = s
            .structures
            .last()
            .is_some_and(|st| st.kind == "for" && st.phase == "header");
        let compound = if (at_start || for_header) && "([".contains(c) {
            operator(input, index, &["((", "[["])?
        } else {
            None
        };
        if compound == Some("((") {
            if for_header && !s.words.is_empty() {
                return Err("compound-command");
            }
            if let Some(end) = self.arithmetic(
                index + 2,
                depth,
                &mut s.commands,
                if for_header {
                    FOR_ARITHMETIC
                } else {
                    ARITHMETIC
                },
            )? {
                if s.crosses_heredoc(input, index, end) {
                    return Err("heredoc");
                }
                if !for_header && self.context.dialect == Dialect::Posix {
                    let (nested, stop) =
                        self.list(index + 1, depth + 1, Some(Close::Paren), None, None)?;
                    if stop != end {
                        return Err("compound-command");
                    }
                    let keys: Vec<_> = s.commands.iter().map(command_key).collect();
                    s.commands.extend(
                        nested
                            .into_iter()
                            .filter(|c| !keys.contains(&command_key(c))),
                    );
                }
                if for_header {
                    let st = s.structures.last_mut().expect("for");
                    st.phase = "do";
                    st.saw_in = true;
                } else {
                    s.statement();
                    s.compound_end = true;
                }
                return Ok(Some(end + 1));
            }
            if for_header {
                return Err("invalid-structure");
            }
        }
        if at_start && (c == '(' || (c == '{' && space_ahead(input, index + 1))) {
            let (commands, end) = self.list(
                index + 1,
                depth + 1,
                Some(if c == '{' { Close::Brace } else { Close::Paren }),
                Some(s.heredocs.clone()),
                None,
            )?;
            if input[index + 1..end].trim().is_empty() {
                return Err("invalid-structure");
            }
            s.commands.extend(commands);
            s.statement();
            s.compound_end = true;
            return Ok(Some(end + 1));
        }
        if at_start && compound == Some("[[") && space_ahead(input, index + 2) {
            let end = self.conditional(index + 2, depth, &mut s.commands)?;
            if s.crosses_heredoc(input, index, end) {
                return Err("heredoc");
            }
            s.statement();
            s.compound_end = true;
            return Ok(Some(end + 1));
        }
        Ok(None)
    }
    fn keyword(&mut self, s: &mut List<'_>, index: usize, depth: usize) -> Result<Option<usize>> {
        let input = self.input;
        let c = ch(input, index);
        if !c.is_ascii_alphabetic() && c != '_' {
            return Ok(None);
        }
        let word = token(input, index);
        let end = index + word.map_or(0, str::len);
        if let Some(st) = s.structures.last() {
            if st.kind == "case" && st.phase == "header" && word == Some("in") {
                if s.words.len() != 1 || s.has_redirect {
                    return Err("compound-command");
                }
                self.finish_command(s, false, false);
                let st = s.structures.last_mut().expect("case");
                st.phase = "pattern";
                st.pattern_started = false;
                return Ok(Some(end));
            }
            if st.kind == "for"
                && matches!(st.phase, "header" | "do")
                && !st.saw_in
                && !st.parenthesized
                && word == Some("in")
                && (st.phase == "do" || !s.words.is_empty())
            {
                let st = s.structures.last_mut().expect("for");
                st.saw_in = true;
                st.phase = "header";
                return Ok(Some(end));
            }
            if st.phase == "pattern"
                && !st.pattern_started
                && s.words.is_empty()
                && word == Some("esac")
            {
                s.structures.pop();
                s.statement();
                s.compound_end = true;
                return Ok(Some(end));
            }
            if st.kind == "for"
                && st.phase == "header"
                && !st.saw_in
                && s.words.len() == 1
                && !s.has_redirect
                && word == Some("do")
            {
                self.finish_command(s, false, false);
                s.structures.last_mut().expect("for").phase = "do";
            }
        }
        if let Some(word @ ("then" | "elif" | "else" | "fi" | "do" | "done" | "esac")) = word
            && s.words.is_empty()
            && !s.redirect_target
            && (!s.in_header()
                || (word == "do" && s.structures.last().is_some_and(|st| st.phase == "do")))
        {
            if s.has_redirect && self.context.dialect != Dialect::Zsh {
                return Err("compound-command");
            }
            self.end_list(s)?;
            let st = s.structures.last_mut().ok_or("compound-command")?;
            match word {
                "then" => {
                    if st.kind != "if" || st.phase != "condition" || st.count == 0 {
                        return Err("compound-command");
                    }
                    st.phase = "body";
                    st.count = 0;
                }
                "elif" | "else" => {
                    if st.kind != "if" || st.phase != "body" || st.count == 0 || st.saw_else {
                        return Err("compound-command");
                    }
                    st.phase = if word == "elif" { "condition" } else { "body" };
                    st.saw_else = word == "else";
                    st.count = 0;
                }
                "do" => {
                    if !matches!(st.kind, "for" | "while" | "until" | "repeat")
                        || !matches!(st.phase, "condition" | "do")
                        || (!matches!(st.kind, "for" | "repeat") && st.count == 0)
                    {
                        return Err("compound-command");
                    }
                    st.phase = "body";
                    st.count = 0;
                }
                _ => {
                    if (word == "fi" && (st.kind != "if" || st.phase != "body" || st.count == 0))
                        || (word == "done"
                            && (!matches!(st.kind, "for" | "while" | "until" | "repeat")
                                || st.phase != "body"
                                || st.count == 0))
                        || (word == "esac" && (st.kind != "case" || st.phase == "header"))
                    {
                        return Err("compound-command");
                    }
                    s.structures.pop();
                    s.statement();
                    s.compound_end = true;
                }
            }
            return Ok(Some(end));
        }
        if !s.at_start() {
            return Ok(None);
        }
        if let Some(end) = function_head(input, index) {
            return Ok(Some(end));
        }
        let kind = match word {
            Some("if") => "if",
            Some("while") => "while",
            Some("until") => "until",
            Some("for" | "select") => "for",
            Some("case") => "case",
            Some("repeat") if self.context.dialect != Dialect::Bash => "repeat",
            _ => "",
        };
        if !kind.is_empty() {
            if depth + s.structures.len() >= DEPTH_LIMIT {
                return Err("compound-command");
            }
            s.structures.push(Structure {
                kind,
                phase: if matches!(kind, "for" | "case" | "repeat") {
                    "header"
                } else {
                    "condition"
                },
                count: 0,
                saw_else: false,
                saw_in: false,
                pattern_started: false,
                parenthesized: false,
            });
            s.dangling = false;
            return Ok(Some(end));
        }
        Ok(compound_prefix(input, index, word))
    }
}

fn shell_whitespace(c: char) -> bool {
    matches!(c, '\u{9}'..='\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
fn command_key(command: &Command) -> (String, Vec<String>) {
    let command = command.borrow();
    (command.resource.clone(), command.words.clone())
}
fn function_body(input: &str, at: usize) -> bool {
    if "{(".contains(ch(input, at)) && at < input.len() {
        return true;
    }
    if input[at..].starts_with("[[") && space_ahead(input, at + 2) {
        return true;
    }
    re!(r"(?:if|while|until|for|select|case|repeat)")
        .find(&input[at..])
        .is_some_and(|m| {
            let end = skip_continuations(input, at + m.end());
            end == input.len() || " \t\n(".contains(ch(input, end))
        })
}
fn function_head(input: &str, at: usize) -> Option<usize> {
    let head = re!(r"(function[ \t](?:[ \t]|\\\n)*)?([A-Za-z_][A-Za-z0-9_.:+@%\-]*)?(?:[ \t]|\\\n)*(\([ \t]*\))?((?:[ \t\n]|\\\n|#[^\n]*\n)*)").captures(&input[at..])?;
    let name = head.get(2).map_or("", |m| m.as_str());
    if if head.get(1).is_some() {
        name.is_empty()
    } else {
        head.get(3).is_none()
            || matches!(
                name,
                "if" | "for"
                    | "select"
                    | "case"
                    | "then"
                    | "elif"
                    | "else"
                    | "fi"
                    | "do"
                    | "done"
                    | "in"
                    | "esac"
            )
    } {
        return None;
    }
    let end = at + head.get(0)?.end();
    function_body(input, end).then_some(end - head.get(4).map_or(0, |m| m.len()))
}
fn compound_ahead(input: &str, mut at: usize) -> bool {
    while ch(input, at) == '!' {
        at += 1;
        loop {
            at = skip_continuations(input, at);
            if !matches!(ch(input, at), ' ' | '\t') {
                break;
            }
            at += 1;
        }
    }
    if at >= input.len() {
        return false;
    }
    if "{(".contains(ch(input, at)) || (input[at..].starts_with("[[") && space_ahead(input, at + 2))
    {
        return true;
    }
    re!(r"(?:if|while|until|for|select|case|repeat)(?-u:\b)").is_match(&input[at..])
}
fn compound_prefix(input: &str, at: usize, word: Option<&str>) -> Option<usize> {
    let prefix = match word? {
        "coproc" => re!(r"coproc(?:[ \t]|\\\n)+(?:--(?:[ \t]|\\\n)+)?"),
        "time" => re!(r"time(?:[ \t]|\\\n)+(?:-p(?:[ \t]|\\\n)+)?(?:--(?:[ \t]|\\\n)+)?"),
        _ => return None,
    }
    .find(&input[at..])?;
    let end = at + prefix.end();
    if word == Some("coproc")
        && let Some(name) = re!(r"[A-Za-z_][A-Za-z0-9_]*(?:[ \t]|\\\n)+").find(&input[end..])
        && compound_ahead(input, end + name.end())
    {
        return Some(end + name.end());
    }
    compound_ahead(input, end).then_some(end)
}
fn do_ahead(input: &str, at: usize) -> bool {
    let skip = re!(r"(?:[ \t\n;]|\\\n|#[^\n]*(?:\n|$))*")
        .find(&input[at..])
        .expect("empty grammar match");
    let at = at + skip.end();
    if input[at..].starts_with("do") {
        let end = skip_continuations(input, at + 2);
        return end == input.len() || " \t\n;{(".contains(ch(input, end));
    }
    ch(input, at) == '{' && space_ahead(input, at + 1)
}
fn glob_qualifier(input: &str, at: usize) -> Option<usize> {
    let found = re!(r"\((?:[/F.@=*rwxAIERWsStUG^MTNDn\-]|[oO][nLlamcdN]|[amc][Mwhmsd]?[-+]?[0-9]+|L[kKmMpPgGtT]?[-+]?[0-9]+|[lY][-+]?[0-9]+|\[[-+]?[0-9]+(?:,[-+]?[0-9]+)?\]|,)+\)").find(&input[at..])?;
    let end = at + found.end();
    let next = skip_continuations(input, end);
    (next == input.len() || " \t\n;&|<>)]".contains(ch(input, next))).then_some(end)
}
fn safe_zsh_flags(input: &str, at: usize) -> Option<(usize, bool)> {
    if !input[at..].starts_with("${(") {
        return None;
    }
    let mut cursor = at + 3;
    let mut indirect = false;
    while cursor < input.len() {
        let c = ch(input, cursor);
        if c == ')' {
            return Some((cursor + 1, indirect));
        }
        if matches!(c, 'j' | 's') {
            cursor += 1;
            let delimiter = ch(input, cursor);
            if cursor == input.len() || "({[<\\".contains(delimiter) {
                return None;
            }
            cursor += delimiter.len_utf8();
            while cursor < input.len() && ch(input, cursor) != delimiter {
                if ch(input, cursor) == ')' {
                    return None;
                }
                cursor += ch(input, cursor).len_utf8();
            }
            if cursor == input.len() {
                return None;
            }
            cursor += delimiter.len_utf8();
            continue;
        }
        if !"@AaBbCcDEfFikLMmNnOopPqStUuVvWwXZz0-".contains(c) {
            return None;
        }
        indirect |= c == 'P';
        cursor += c.len_utf8();
        if c == 'q' && matches!(ch(input, cursor), '-' | '+') {
            cursor += 1;
        }
    }
    None
}
fn zsh_evaluates(input: &str, at: usize) -> bool {
    let mut cursor = at + 1;
    if input[cursor..].starts_with("{\\\n") {
        return true;
    }
    if input[cursor..].starts_with("{(") {
        let Some((end, _)) = safe_zsh_flags(input, at) else {
            return true;
        };
        cursor = end;
    }
    if ch(input, cursor) == '{' {
        cursor += 1;
    }
    while cursor < input.len() {
        cursor = skip_continuations(input, cursor);
        let c = ch(input, cursor);
        if c == '~' {
            return true;
        }
        if !"^=#+".contains(c) || cursor == input.len() {
            break;
        }
        cursor += 1;
    }
    false
}
fn evaluates(text: &Text, evaluation: Evaluation, context: &mut Context) -> bool {
    let combined = format!("{} {}", text.literal, text.output);
    let expansion = re!(search r"\$(?:\\.|\\\n)*[({]|`").is_match(&combined);
    let escape =
        re!(search r"\\(?:x[0-9a-fA-F]{1,2}|[0-7]{1,3}|u[0-9a-fA-F]{1,4}|U[0-9a-fA-F]{1,8})")
            .is_match(&combined);
    context.bound_dollar |= expansion || escape || text.literal.contains(['$', '`']);
    context.bound_open |= expansion || escape || text.literal.contains(['(', '{', '`']);
    context.bound_bracket |= expansion
        || escape
        || !text.output.trim_matches(shell_whitespace).is_empty()
        || combined.contains('[');
    if evaluation == Evaluation::Deferred {
        return false;
    }
    context.sink_bracket |= text.word.contains('[') || combined.contains('[');
    let variable = if evaluation == Evaluation::Arithmetic {
        text.word.bytes().enumerate().any(|(at, b)| {
            b.is_ascii_alphabetic()
                || b == b'_'
                || (b == b'$'
                    && !matches!(
                        text.word.as_bytes().get(at + 1),
                        Some(b'?' | b'#' | b'!' | b'-')
                    )
                    && !(text.word.as_bytes().get(at + 1) == Some(&b'$')
                        && text.word.as_bytes().get(at + 2) != Some(&b'[')))
        })
    } else {
        text.word.contains('$') || combined.contains('[')
    };
    context.variable_sink |= variable;
    expansion && (evaluation == Evaluation::Arithmetic || combined.contains('['))
}

impl Scanner<'_, '_> {
    fn unit(
        &mut self,
        index: usize,
        depth: usize,
        commands: &mut Vec<Command>,
        mode: Mode,
        allow_bracket: bool,
        mut text: Option<&mut Text>,
    ) -> Result<Option<usize>> {
        let input = self.input;
        let c = ch(input, index);
        let next = ch(input, index + 1);
        if c == '\\' {
            if mode == Mode::Quoted && !"$`\"\\\n".contains(next) {
                return Ok(None);
            }
            if index + 1 >= input.len() {
                return Err("unterminated-escape");
            }
            if let Some(text) = text
                && next != '\n'
            {
                text.append(&next.to_string());
            }
            return Ok(Some(index + next.len_utf8()));
        }
        if c == '$' && self.context.dialect != Dialect::Bash && zsh_evaluates(input, index) {
            return Err("dynamic-execution");
        }
        if c == '$'
            && self.context.dialect != Dialect::Bash
            && mode != Mode::Arithmetic
            && let Some(found) = re!(
                r"\$(?:[\^=+~#]|\\\n)*(?:[A-Za-z_](?:[A-Za-z0-9_]|\\\n)*|[@*#?$!0-9\-])(?:\\\n)*\["
            )
            .find(&input[index..])
        {
            let bracket = index + found.end();
            let mut subscript = Text::default();
            let end = self.subscript(
                bracket,
                depth,
                commands,
                if mode == Mode::Quoted {
                    QUOTED_SUBSCRIPT
                } else {
                    SUBSCRIPT
                },
                Some(&mut subscript),
            )?;
            if let Some(text) = text {
                text.word
                    .push_str(&format!("{}{}]", &input[index..bracket], subscript.word));
            }
            return Ok(Some(end));
        }
        let opener = if c == '$' {
            operator(input, index, &["$$", "$((", "$(", "${", "$[", "$'", "$\""])?
        } else if mode == Mode::Word && matches!(c, '<' | '>') {
            operator(input, index, &["<(", ">("])?
        } else {
            None
        };
        if opener == Some("$$") {
            let quote = skip_continuations(input, index + 2);
            if ch(input, quote) == '\'' {
                let end = input[quote + 1..]
                    .find('\'')
                    .map(|n| quote + 1 + n)
                    .ok_or("unterminated-quote")?;
                if input[quote + 1..end].contains('\\') && input[end + 1..].contains('\'') {
                    return Err("unterminated-quote");
                }
            }
            if let Some(text) = text {
                text.word.push_str("$$");
            }
            return Ok(Some(index + 1));
        }
        if mode == Mode::Word && opener == Some("$'") {
            let (value, end) = ansi_quote(input, index + 1, true).ok_or("unterminated-quote")?;
            let first = input[index + 2..]
                .find('\'')
                .map(|n| index + 2 + n)
                .ok_or("unterminated-quote")?;
            if end != first
                && ((self.context.dialect != Dialect::Bash
                    && index >= 2
                    && input.as_bytes()[index - 2] == b'$')
                    || (self.context.dialect == Dialect::Posix
                        && (input[first + 1..]
                            .find('\'')
                            .map(|n| first + 1 + n)
                            .is_some_and(|n| n < end)
                            || re!(search r##"[#"\n]|\\'"##).is_match(&input[first + 1..]))))
            {
                return Err("unterminated-quote");
            }
            if let Some(text) = text {
                text.append(&value);
            }
            return Ok(Some(end));
        }
        if mode == Mode::Arithmetic && opener == Some("$'") {
            let (value, end) = ansi_quote(input, index + 1, true).ok_or("invalid-structure")?;
            if input[index + 2..].find('\'').map(|n| index + 2 + n) != Some(end)
                || value.contains(['(', ')', '[', ']', ';', '"', '}'])
            {
                return Err("invalid-structure");
            }
            if let Some(text) = text.as_deref_mut() {
                text.append(&value);
            }
            return self
                .span(
                    index + 2,
                    depth,
                    commands,
                    ARITHMETIC_QUOTE,
                    allow_bracket,
                    text,
                )
                .map(Some);
        }
        if mode == Mode::Word && c == '\'' {
            let end = input[index + 1..]
                .find('\'')
                .map(|n| index + 1 + n)
                .ok_or("unterminated-quote")?;
            if let Some(text) = text {
                text.append(&input[index + 1..end]);
            }
            return Ok(Some(end));
        }
        if mode == Mode::Arithmetic && c == '\'' {
            return self
                .span(
                    index + 1,
                    depth,
                    commands,
                    ARITHMETIC_QUOTE,
                    allow_bracket,
                    text,
                )
                .map(Some);
        }
        if mode == Mode::Word && opener == Some("$\"") {
            return self
                .span(index + 2, depth, commands, DOUBLE, allow_bracket, text)
                .map(Some);
        }
        if mode != Mode::Quoted && c == '"' {
            return self
                .span(
                    index + 1,
                    depth,
                    commands,
                    if mode == Mode::Arithmetic {
                        ARITHMETIC_DOUBLE
                    } else {
                        DOUBLE
                    },
                    allow_bracket,
                    text,
                )
                .map(Some);
        }
        let end = if matches!(opener, Some("<(" | ">(")) {
            Some(self.nested(index + 2, depth, commands, Close::Paren, None)?)
        } else if (opener.is_some() && !matches!(opener, Some("$'" | "$\""))) || c == '`' {
            Some(self.dollar(
                index,
                depth,
                commands,
                mode != Mode::Word,
                allow_bracket,
                text.as_deref_mut(),
            )?)
        } else {
            None
        };
        if end.is_none() && c == '$' && (next.is_ascii_alphanumeric() || "_@*#?$!-".contains(next))
        {
            if let Some(text) = text {
                text.word.push('$');
            }
            return Ok(Some(index));
        }
        if let (Some(text), Some(end)) = (text, end) {
            text.word.push_str(&input[index..=end]);
        }
        Ok(end)
    }
    fn span(
        &mut self,
        start: usize,
        depth: usize,
        commands: &mut Vec<Command>,
        span: Span,
        allow_bracket: bool,
        mut text: Option<&mut Text>,
    ) -> Result<usize> {
        let input = self.input;
        let mut nesting = 0;
        let mut index = start;
        while index < input.len() {
            self.context.step()?;
            let c = ch(input, index);
            if Some(c) == span.close && nesting == 0 {
                return Ok(index);
            }
            if span.reject.contains(c)
                && !(c == '#'
                    && index > 0
                    && (input.as_bytes()[index - 1].is_ascii_digit()
                        || input.as_bytes()[index - 1] == b'$'))
            {
                return Err("invalid-structure");
            }
            if Some(c) == span.open {
                nesting += 1;
                if nesting + depth > DEPTH_LIMIT {
                    return Err("invalid-structure");
                }
            }
            if Some(c) == span.close {
                nesting -= 1;
            }
            let unit = if Some(c) == span.open || Some(c) == span.close {
                None
            } else {
                self.unit(
                    index,
                    depth,
                    commands,
                    span.mode,
                    allow_bracket,
                    text.as_deref_mut(),
                )?
            };
            if let Some(end) = unit {
                index = end + 1;
                continue;
            }
            if c == '\\'
                && (Some(ch(input, index + 1)) == span.open
                    || Some(ch(input, index + 1)) == span.close)
            {
                if let Some(text) = text.as_deref_mut() {
                    text.append(&input[index..index + 2]);
                }
                index += 2;
            } else {
                if let Some(text) = text.as_deref_mut() {
                    text.append(&c.to_string());
                }
                index += c.len_utf8();
            }
        }
        if span.close.is_some() {
            Err("unterminated-quote")
        } else {
            Ok(input.len())
        }
    }
    fn nested(
        &mut self,
        start: usize,
        depth: usize,
        commands: &mut Vec<Command>,
        close: Close,
        text: Option<&mut Text>,
    ) -> Result<usize> {
        let (nested, end) = self.list(start, depth + 1, Some(close), None, text)?;
        commands.extend(nested);
        Ok(end)
    }
    fn subscript(
        &mut self,
        start: usize,
        depth: usize,
        commands: &mut Vec<Command>,
        span: Span,
        text: Option<&mut Text>,
    ) -> Result<usize> {
        let mut empty = Text::default();
        let text = text.unwrap_or(&mut empty);
        let end = self.span(start, depth, commands, span, false, Some(text))?;
        self.context.sink_bracket = true;
        if evaluates(text, Evaluation::Arithmetic, self.context) {
            return Err("dynamic-execution");
        }
        Ok(end)
    }
    fn dollar(
        &mut self,
        start: usize,
        depth: usize,
        commands: &mut Vec<Command>,
        quoted: bool,
        allow_bracket: bool,
        mut text: Option<&mut Text>,
    ) -> Result<usize> {
        if depth >= DEPTH_LIMIT {
            return Err("command-substitution");
        }
        let input = self.input;
        if ch(input, start) == '`' {
            return self.backtick(start, depth + 1, commands, quoted, text);
        }
        if input[start..].starts_with("$((")
            && let Some(end) = self.arithmetic(start + 3, depth + 1, commands, ARITHMETIC)?
        {
            return Ok(end);
        }
        if input[start..].starts_with("$(") {
            return self.nested(start + 2, depth, commands, Close::Paren, text);
        }
        if let Some(open) = re!(r"\$\{(?:\\\n)*(?:[ \t\n]|\|)").find(&input[start..]) {
            return self.nested(start + open.end(), depth, commands, Close::NoFork, text);
        }
        if input[start..].starts_with("${") {
            let mut parameter = Text::default();
            let end = self.parameter(start + 2, depth + 1, commands, quoted, &mut parameter)?;
            let indirect = ch(input, skip_continuations(input, start + 2)) == '!'
                || (self.context.dialect != Dialect::Bash
                    && safe_zsh_flags(input, start).is_some_and(|(_, p)| p));
            if indirect {
                self.context.variable_sink = true;
                if evaluates(&parameter, Evaluation::Binding, self.context) {
                    return Err("dynamic-execution");
                }
            }
            if let Some(text) = text.as_mut() {
                text.literal.push_str(&parameter.literal);
                text.output.push_str(&parameter.output);
            }
            return Ok(end);
        }
        if !allow_bracket && self.context.dialect == Dialect::Posix {
            return Err("command-substitution");
        }
        let mut expression = Text::default();
        let end = self.span(
            start + 2,
            depth + 1,
            commands,
            BRACKET_ARITHMETIC,
            true,
            Some(&mut expression),
        )?;
        if evaluates(&expression, Evaluation::Arithmetic, self.context) {
            return Err("dynamic-execution");
        }
        Ok(end)
    }
    fn arithmetic(
        &mut self,
        start: usize,
        depth: usize,
        commands: &mut Vec<Command>,
        span: Span,
    ) -> Result<Option<usize>> {
        let mut inner = vec![];
        let mut text = Text::default();
        let end = self
            .span(start, depth, &mut inner, span, true, Some(&mut text))
            .map_err(|_| "invalid-structure")?;
        if operator(self.input, end, &["))"])?.is_none() {
            return Ok(None);
        }
        if evaluates(&text, Evaluation::Arithmetic, self.context) {
            return Err("dynamic-execution");
        }
        commands.extend(inner);
        Ok(Some(end + 1))
    }
    fn backtick(
        &mut self,
        start: usize,
        depth: usize,
        commands: &mut Vec<Command>,
        quoted: bool,
        text: Option<&mut Text>,
    ) -> Result<usize> {
        let input = self.input;
        let mut source = String::new();
        let mut index = start + 1;
        while index < input.len() {
            self.context.step()?;
            let c = ch(input, index);
            if c == '`' {
                let (inner, _) = Scanner {
                    input: &source,
                    context: self.context,
                }
                .list(0, depth, None, None, text)
                .map_err(|_| "command-substitution")?;
                commands.extend(inner);
                return Ok(index);
            }
            let next = ch(input, index + 1);
            if c == '\\' && ("$`\\\n".contains(next) || (quoted && next == '"')) {
                index += 1;
                if next != '\n' {
                    source.push(next);
                }
                index += next.len_utf8();
                continue;
            }
            source.push(c);
            index += c.len_utf8();
        }
        Err("command-substitution")
    }
    fn parameter(
        &mut self,
        start: usize,
        depth: usize,
        commands: &mut Vec<Command>,
        quoted: bool,
        text: &mut Text,
    ) -> Result<usize> {
        let input = self.input;
        let name_start = start
            + re!(r"(?:\\\n)*(?:\([^)]*\))?(?:[!#^=+~]|\\\n)*")
                .find(&input[start..])
                .expect("empty grammar match")
                .end();
        let mut colon =
            re!(r"(?:[A-Za-z_](?:[A-Za-z0-9_]|\\\n)*|[0-9](?:[0-9]|\\\n)*|[@*#?$!\-])(?:\\\n)*")
                .find(&input[name_start..])
                .map(|m| name_start + m.end());
        let name = colon.map_or(String::new(), |end| {
            input[name_start..end].replace("\\\n", "")
        });
        if self.context.dialect != Dialect::Bash
            && (matches!(name.as_str(), "NULLCMD" | "READNULLCMD")
                || (re!(r"(?:(?:dis_)?(?:functions|[gs]?aliases)|options|commands)$")
                    .is_match(&name)
                    && colon.is_some_and(|at| "[=:".contains(ch(input, at)))))
        {
            return Err("dynamic-execution");
        }
        if name == "PS4" {
            self.context.eval_sink = true;
        }
        let mut evaluation = None;
        let mut word_offset = 0;
        let mut literal_offset = 0;
        let mut index = start;
        while index < input.len() {
            self.context.step()?;
            let c = ch(input, index);
            if c == '}' {
                if evaluation.is_some_and(|e| {
                    evaluates(
                        &Text {
                            word: text.word[word_offset..].into(),
                            literal: text.literal[literal_offset..].into(),
                            output: text.output.clone(),
                        },
                        e,
                        self.context,
                    )
                }) {
                    return Err("dynamic-execution");
                }
                return Ok(index);
            }
            if Some(index) == colon && evaluation.is_none() {
                if c == ':' && !"-=?+".contains(ch(input, index + 1)) {
                    evaluation = Some(Evaluation::Arithmetic);
                }
                if c == '=' || (c == ':' && ch(input, index + 1) == '=') {
                    evaluation = Some(Evaluation::Deferred);
                }
                if evaluation.is_some() {
                    word_offset = text.word.len();
                    literal_offset = text.literal.len();
                }
            }
            if c == '@' && re!(r"@(?:\\\n)*[EP](?:\\\n)*\}").is_match(&input[index..]) {
                return Err("dynamic-execution");
            }
            if !quoted && self.context.dialect != Dialect::Bash && c == '(' && index > start {
                return Err("dynamic-execution");
            }
            if quoted && c == '\'' && index > 0 && input.as_bytes()[index - 1] == b'$' {
                text.literal.push_str(
                    &ansi_quote(input, index, true)
                        .ok_or("unterminated-quote")?
                        .0,
                );
            }
            let subscript = Some(index) == colon && c == '[';
            let unit = if subscript {
                Some(self.subscript(index + 1, depth, commands, SUBSCRIPT, None)?)
            } else if quoted && c == '\'' {
                Some(self.span(
                    index + 1,
                    depth,
                    commands,
                    PARAMETER_QUOTE,
                    false,
                    Some(text),
                )?)
            } else if quoted && c == '"' {
                Some(self.span(index + 1, depth, commands, DOUBLE, false, Some(text))?)
            } else {
                self.unit(
                    index,
                    depth,
                    commands,
                    if quoted { Mode::Quoted } else { Mode::Word },
                    false,
                    Some(text),
                )?
            };
            if quoted
                && input[index..].starts_with("$(")
                && unit.is_some_and(|end| input[index..=end].contains('}'))
            {
                return Err("command-substitution");
            }
            if (subscript || (index == name_start && input[index..].starts_with("${")))
                && let Some(end) = unit
            {
                colon = Some(skip_continuations(input, end + 1));
            }
            if let Some(end) = unit {
                index = end + 1;
            } else {
                text.append(&c.to_string());
                index += c.len_utf8();
            }
        }
        Err("command-substitution")
    }
    fn conditional(
        &mut self,
        start: usize,
        depth: usize,
        commands: &mut Vec<Command>,
    ) -> Result<usize> {
        let input = self.input;
        let mut words: Vec<Text> = vec![];
        let mut text = Text::default();
        let mut word_started = false;
        let mut has_word = false;
        let mut paren_depth = 0usize;
        let mut pattern_depth = 0usize;
        let mut index = start;
        while index < input.len() {
            self.context.step()?;
            let c = ch(input, index);
            if input[index..].starts_with("\\\n") {
                index += 2;
                continue;
            }
            if !word_started && has_word {
                words.push(std::mem::take(&mut text));
                has_word = false;
            }
            if !word_started
                && c == ']'
                && operator(input, index, &["]]"])?.is_some()
                && boundary(input, index + 2)
            {
                if paren_depth != 0 {
                    return Err("invalid-structure");
                }
                for (at, word) in words.iter().enumerate() {
                    let previous = at
                        .checked_sub(1)
                        .and_then(|i| words.get(i))
                        .map(|t| t.word.as_str())
                        .unwrap_or("");
                    let next = words.get(at + 1).map(|t| t.word.as_str()).unwrap_or("");
                    let arithmetic = |s| matches!(s, "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge");
                    if (previous == "-v" && evaluates(word, Evaluation::Binding, self.context))
                        || (((self.context.dialect != Dialect::Bash && previous == "-t")
                            || arithmetic(previous)
                            || arithmetic(next))
                            && evaluates(word, Evaluation::Arithmetic, self.context))
                    {
                        return Err("dynamic-execution");
                    }
                }
                return Ok(index + 1);
            }
            if !word_started && c == '#' {
                let end = input[index..].find('\n').ok_or("invalid-structure")?;
                index += end + 1;
                continue;
            }
            if matches!(c, ' ' | '\t' | '\n') {
                if pattern_depth == 0 {
                    word_started = false;
                }
                index += 1;
                continue;
            }
            if c == ';' {
                return Err("invalid-structure");
            }
            if matches!(c, '(' | ')') {
                if c == ')' && paren_depth == 0 {
                    return Err("invalid-structure");
                }
                let in_pattern = pattern_depth > 0
                    || (c == '('
                        && words
                            .last()
                            .is_some_and(|w| matches!(w.word.as_str(), "=" | "==" | "!=" | "=~")));
                if c == '(' {
                    paren_depth += 1;
                } else {
                    paren_depth -= 1;
                }
                if in_pattern {
                    if c == '(' {
                        pattern_depth += 1;
                    } else {
                        pattern_depth -= 1;
                    }
                    word_started = true;
                    has_word = true;
                    text.append(&c.to_string());
                    index += 1;
                    continue;
                }
                if has_word {
                    words.push(std::mem::take(&mut text));
                    has_word = false;
                }
                words.push(Text {
                    word: c.to_string(),
                    ..Text::default()
                });
                word_started = false;
                index += 1;
                continue;
            }
            let unit = self.unit(index, depth, commands, Mode::Word, true, Some(&mut text))?;
            if unit.is_none() && pattern_depth == 0 && "&|<>".contains(c) {
                if has_word {
                    words.push(std::mem::take(&mut text));
                    has_word = false;
                }
                words.push(Text {
                    word: c.to_string(),
                    ..Text::default()
                });
                word_started = false;
                index += 1;
                continue;
            }
            word_started = true;
            has_word = true;
            if let Some(end) = unit {
                index = end + 1;
            } else {
                text.append(&c.to_string());
                index += c.len_utf8();
            }
        }
        Err("invalid-structure")
    }
    fn array_pattern(
        &mut self,
        start: usize,
        depth: usize,
        commands: &mut Vec<Command>,
        array: bool,
    ) -> Result<usize> {
        if depth > DEPTH_LIMIT {
            return Err("command-substitution");
        }
        let input = self.input;
        let mut text = array.then(Text::default);
        let mut word_started = false;
        let mut index = start + 1;
        while index < input.len() {
            self.context.step()?;
            let c = ch(input, index);
            if input[index..].starts_with("\\\n") {
                index += 2;
                continue;
            }
            if matches!(c, ')' | ' ' | '\t' | '\n')
                && let Some(text) = &mut text
            {
                evaluates(text, Evaluation::Deferred, self.context);
                text.literal.clear();
                text.output.clear();
            }
            if c == ')' {
                return Ok(index);
            }
            if array && !word_started && c == '#' {
                index += input[index..].find('\n').ok_or("command-substitution")? + 1;
                continue;
            }
            if matches!(c, ' ' | '\t') || (array && c == '\n') {
                word_started = false;
                index += 1;
                continue;
            }
            if array
                && word_started
                && c == '('
                && self.context.dialect == Dialect::Zsh
                && let Some(end) = glob_qualifier(input, index)
            {
                if let Some(text) = &mut text {
                    text.literal.push_str(&input[index..end]);
                }
                index = end;
                continue;
            }
            if matches!(c, '\n' | ';' | '&') || (array && "|(".contains(c)) {
                return Err("command-substitution");
            }
            word_started = true;
            let unit = if array && c == '=' && ch(input, index + 1) == '(' {
                Some(self.nested(index + 2, depth, commands, Close::Paren, None)?)
            } else if c == '(' {
                Some(self.array_pattern(index, depth + 1, commands, array)?)
            } else if c == '[' {
                Some(self.subscript(index + 1, depth, commands, SUBSCRIPT, None)?)
            } else {
                self.unit(index, depth, commands, Mode::Word, false, text.as_mut())?
            };
            if let Some(end) = unit {
                index = end + 1;
            } else {
                if let Some(text) = &mut text {
                    text.literal.push(c);
                }
                index += c.len_utf8();
            }
        }
        Err("command-substitution")
    }
}

fn ansi_quote(input: &str, start: usize, unicode: bool) -> Option<(String, usize)> {
    let mut value = String::new();
    let mut index = start + 1;
    while index < input.len() {
        let c = ch(input, index);
        if c == '\'' {
            return Some((value, index));
        }
        if c != '\\' {
            value.push(c);
            index += c.len_utf8();
            continue;
        }
        index += 1;
        let escaped = ch(input, index);
        let literal = match escaped {
            'a' => Some('\u{7}'),
            'b' => Some('\u{8}'),
            'e' | 'E' => Some('\u{1b}'),
            'f' => Some('\u{c}'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            'v' => Some('\u{b}'),
            '\\' | '\'' | '"' | '?' => Some(escaped),
            _ => None,
        };
        if let Some(c) = literal {
            value.push(c);
            index += 1;
            continue;
        }
        let octal = matches!(escaped, '0'..='7');
        let (start, count, radix) = match escaped {
            'x' => (index + 1, 2, 16),
            'u' if unicode => (index + 1, 4, 16),
            'U' if unicode => (index + 1, 8, 16),
            _ if octal => (index, 3, 8),
            _ => return None,
        };
        let end = start
            + input[start..]
                .bytes()
                .take(count)
                .take_while(|b| {
                    if octal {
                        matches!(b, b'0'..=b'7')
                    } else {
                        b.is_ascii_hexdigit()
                    }
                })
                .count();
        if start == end {
            return None;
        }
        let point = u32::from_str_radix(&input[start..end], radix).ok()?;
        if point == 0 {
            return None;
        }
        value.push(char::from_u32(point)?);
        index = end;
    }
    None
}
fn heredoc_delimiter(input: &str, start: usize, tabs: bool) -> Option<(Heredoc, usize)> {
    let mut h = Heredoc {
        delimiter: String::new(),
        dash_delimiter: None,
        quoted: false,
        tabs,
        binding: false,
        owned: false,
        command: None,
    };
    let mut quote = None;
    let mut end = start;
    let mut started = false;
    let mut index = start;
    while index < input.len() {
        let c = ch(input, index);
        if !started && matches!(c, ' ' | '\t') {
            index += 1;
            continue;
        }
        if !started && c == '#' {
            return None;
        }
        if quote.is_none() && " \t\n;&|()<>".contains(c) {
            return started.then_some((h, end));
        }
        if quote.is_none() && c == '$' && input[index + 1..].starts_with("\\\n") {
            return None;
        }
        if quote.is_none() && input[index..].starts_with("$'") {
            let (value, last) = ansi_quote(input, index + 1, false)?;
            if input[index + 2..].find('\'').map(|n| index + 2 + n) != Some(last) {
                return None;
            }
            h.dash_delimiter = Some(format!(
                "{}${}",
                h.dash_delimiter.as_ref().unwrap_or(&h.delimiter),
                &input[index + 2..last]
            ));
            h.delimiter.push_str(&value);
            h.quoted = true;
            started = true;
            end = last;
            index = last + 1;
            continue;
        }
        if quote.is_none() && input[index..].starts_with("$\"") {
            return None;
        }
        if Some(c) == quote {
            quote = None;
            end = index;
            index += 1;
            continue;
        }
        if c == '\\' && quote != Some('\'') {
            if index + 1 == input.len() {
                return None;
            }
            let next = ch(input, index + 1);
            if next == '\n' {
                index += 2;
                continue;
            }
            if quote.is_none() || "$`\"\\".contains(next) {
                h.quoted = true;
                started = true;
                if let Some(dash) = &mut h.dash_delimiter {
                    dash.push(next);
                }
                h.delimiter.push(next);
                end = index + next.len_utf8();
                index += 1 + next.len_utf8();
                continue;
            }
        }
        if quote.is_none() && matches!(c, '\'' | '"') {
            quote = Some(c);
            h.quoted = true;
            started = true;
            end = index;
            index += 1;
            continue;
        }
        if let Some(dash) = &mut h.dash_delimiter {
            dash.push(c);
        }
        h.delimiter.push(c);
        started = true;
        end = index + c.len_utf8() - 1;
        index += c.len_utf8();
    }
    (started && quote.is_none()).then_some((h, end))
}
fn heredoc_body<'a>(input: &'a str, mut start: usize, h: &Heredoc) -> Option<(&'a str, usize)> {
    let body_start = start;
    let mut line_start = start;
    let mut line = String::new();
    let mut backslashes = 0;
    while start <= input.len() {
        let index = input[start..].find('\n').map_or(input.len(), |n| start + n);
        let text = if h.tabs && start == line_start {
            input[start..index].trim_start_matches('\t')
        } else {
            &input[start..index]
        };
        let run = text.bytes().rev().take_while(|b| *b == b'\\').count();
        backslashes = if run == text.len() {
            backslashes + run
        } else {
            run
        };
        let continued = !h.quoted && backslashes % 2 == 1 && index < input.len();
        let limit = h
            .delimiter
            .len()
            .max(h.dash_delimiter.as_ref().map_or(0, String::len));
        if line.len() <= limit {
            line.push_str(if continued {
                &text[..text.len() - 1]
            } else {
                text
            });
        }
        if continued {
            backslashes -= 1;
            start = index + 1;
            continue;
        }
        if h.dash_delimiter.as_ref() == Some(&line) {
            return None;
        }
        if line == h.delimiter {
            if start > line_start
                && input
                    .get(index + 1..)
                    .is_some_and(|s| s.contains(&h.delimiter))
            {
                return None;
            }
            return Some((&input[body_start..line_start], index));
        }
        if index == input.len() {
            break;
        }
        line.clear();
        backslashes = 0;
        start = index + 1;
        line_start = start;
    }
    None
}

fn static_word(raw: &str) -> bool {
    let mut quote = None;
    let mut ansi = false;
    let mut segment_started = false;
    let mut leading_dot = false;
    let mut segment_glob = false;
    let mut ansi_escape = false;
    let mut unquoted_glob = false;
    let mut braces = vec![];
    let mut index = 0;
    while index < raw.len() {
        let c = ch(raw, index);
        let next = ch(raw, index + c.len_utf8());
        let mut literal = c;
        let mut glob = false;
        match quote {
            Some('\'') if ansi => {
                if c == '\\' {
                    ansi_escape = true;
                    if unquoted_glob {
                        return false;
                    }
                    index += 1 + next.len_utf8();
                    continue;
                }
                if c == '\'' {
                    quote = None;
                    ansi = false;
                    index += 1;
                    continue;
                }
                index += c.len_utf8();
            }
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                    index += 1;
                    continue;
                }
                index += c.len_utf8();
            }
            Some('"') => {
                if c == '\\' && "$`\"\\\n".contains(next) {
                    index += 1 + next.len_utf8();
                    if next == '\n' {
                        continue;
                    }
                    literal = next;
                } else {
                    if c == '"' {
                        quote = None;
                        index += 1;
                        continue;
                    }
                    if c == '`'
                        || (c == '$'
                            && (next.is_ascii_alphanumeric()
                                || "_@*#?$!({[-".contains(next)
                                || raw[index + 1..].starts_with("\\\n")))
                    {
                        return false;
                    }
                    index += c.len_utf8();
                }
            }
            _ => {
                if c == '\\' {
                    index += 1 + next.len_utf8();
                    if next == '\n' {
                        continue;
                    }
                    literal = next;
                } else {
                    if matches!(c, '\'' | '"') {
                        quote = Some(c);
                        index += 1;
                        continue;
                    }
                    if c == '$' && matches!(next, '\'' | '"') {
                        quote = Some(next);
                        ansi = next == '\'';
                        index += 2;
                        continue;
                    }
                    if c == '`'
                        || (matches!(c, '<' | '>' | '=') && next == '(')
                        || (c == '$'
                            && (next.is_ascii_alphanumeric()
                                || "_@*#?$!({[-".contains(next)
                                || raw[index + 1..].starts_with("\\\n")))
                    {
                        return false;
                    }
                    if c == '{' {
                        braces.push(false);
                    }
                    if (c == ',' || (c == '.' && next == '.'))
                        && let Some(brace) = braces.last_mut()
                    {
                        *brace = true;
                    }
                    if c == '}' && braces.pop() == Some(true) {
                        return false;
                    }
                    glob = matches!(c, '*' | '?' | '[');
                    index += c.len_utf8();
                }
            }
        }
        unquoted_glob |= glob;
        if ansi_escape && unquoted_glob {
            return false;
        }
        if literal == '/' {
            segment_started = false;
            leading_dot = false;
            segment_glob = false;
            continue;
        }
        if !segment_started {
            segment_started = true;
            leading_dot = literal == '.';
        }
        segment_glob |= glob;
        if leading_dot && segment_glob {
            return false;
        }
    }
    quote.is_none()
}

// The one-word arities equal the unknown-command fallback. Keep only entries
// that choose a longer raw-token prefix; longest matching entry wins.
fn prefix(words: &[String]) -> &[String] {
    for length in (1..=words.len().min(3)).rev() {
        let arity = match words[..length].join(" ").as_str() {
            "aws"
            | "az"
            | "bun run"
            | "bun x"
            | "cargo add"
            | "cargo run"
            | "consul kv"
            | "deno task"
            | "doctl"
            | "docker builder"
            | "docker compose"
            | "docker container"
            | "docker image"
            | "docker network"
            | "docker volume"
            | "eksctl create"
            | "gcloud"
            | "gh"
            | "git config"
            | "git remote"
            | "git stash"
            | "ip addr"
            | "ip link"
            | "ip netns"
            | "ip route"
            | "kind create"
            | "kubectl kustomize"
            | "kubectl rollout"
            | "mc admin"
            | "npm exec"
            | "npm init"
            | "npm run"
            | "npm view"
            | "openssl req"
            | "openssl x509"
            | "pnpm dlx"
            | "pnpm exec"
            | "pnpm run"
            | "podman container"
            | "podman image"
            | "pulumi stack"
            | "sfdx"
            | "terraform workspace"
            | "vault auth"
            | "vault kv"
            | "yarn dlx"
            | "yarn run" => 3,
            "bazel" | "brew" | "bun" | "cargo" | "cdk" | "cf" | "cmake" | "composer" | "consul"
            | "crictl" | "deno" | "docker" | "eksctl" | "firebase" | "flyctl" | "git" | "go"
            | "gradle" | "helm" | "heroku" | "hugo" | "ip" | "kind" | "kubectl" | "kustomize"
            | "make" | "mc" | "minikube" | "mongosh" | "mysql" | "mvn" | "ng" | "npm" | "npx"
            | "nvm" | "nx" | "openssl" | "pip" | "pipenv" | "pnpm" | "poetry" | "podman"
            | "psql" | "pulumi" | "python" | "pyenv" | "rake" | "rbenv" | "redis-cli"
            | "rustup" | "serverless" | "skaffold" | "sls" | "sst" | "swift" | "systemctl"
            | "terraform" | "tmux" | "turbo" | "ufw" | "vault" | "vercel" | "volta" | "wp"
            | "yarn" => 2,
            _ => continue,
        };
        return &words[..words.len().min(arity)];
    }
    &words[..words.len().min(1)]
}
fn directory_index(raw: &[String]) -> Option<usize> {
    let cwd = |s: &str| {
        matches!(
            s,
            "cd" | "chdir" | "popd" | "pushd" | "push-location" | "set-location"
        )
    };
    if raw.first().is_some_and(|s| cwd(s)) {
        return Some(0);
    }
    let mut index = 0;
    while raw
        .get(index)
        .is_some_and(|s| matches!(s.as_str(), "builtin" | "command"))
    {
        let wrapper = &raw[index];
        index += 1;
        while raw.get(index).is_some_and(|s| s.starts_with('-')) {
            let flag = &raw[index];
            index += 1;
            if flag == "--" {
                break;
            }
            if wrapper == "command" && re!(r"-[p]*[vV]").is_match(flag) {
                return None;
            }
        }
    }
    (index > 0 && raw.get(index).is_some_and(|s| cwd(s))).then_some(index)
}
fn known_directory(
    value: &str,
    raw: &str,
    home: Option<&str>,
    cwd: &str,
    passwd: Option<&str>,
) -> Option<String> {
    if !static_word(raw) {
        return None;
    }
    if value == "~" {
        return home.map(str::to_owned);
    }
    // Node's path.join appends a tilde suffix even with extra leading slashes;
    // Rust must not let that suffix replace the captured home/cwd base.
    if let Some(rest) = value.strip_prefix("~/") {
        return home.map(|h| {
            std::path::Path::new(h)
                .join(rest.trim_start_matches('/'))
                .to_string_lossy()
                .into_owned()
        });
    }
    if raw.starts_with('~') {
        if value == "~+" {
            return Some(cwd.into());
        }
        if let Some(rest) = value.strip_prefix("~+/") {
            return Some(
                std::path::Path::new(cwd)
                    .join(rest.trim_start_matches('/'))
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        if re!(r"~(?:[-+](?:[0-9]+)?|[0-9]+)(?:$|/)").is_match(value) {
            return None;
        }
        if let Some(user) = re!(r"~([A-Za-z_][A-Za-z0-9._\-]*)(?:$|/)")
            .captures(value)
            .and_then(|c| c.get(1))
        {
            for line in passwd?.lines().filter(|l| !l.starts_with('#')) {
                let fields: Vec<_> = line.split(':').collect();
                if fields.first() == Some(&user.as_str())
                    && let Some(home) = fields.get(5).filter(|h| !h.is_empty())
                {
                    return Some(
                        std::path::Path::new(home)
                            .join(value[user.end()..].trim_start_matches('/'))
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
            return None;
        }
    }
    Some(value.into())
}

pub(crate) fn parse(
    input: &str,
    dialect: Dialect,
    cwd: &str,
    home: Option<&str>,
) -> Result<Parsed> {
    if input.encode_utf16().take(INPUT_LIMIT + 1).count() > INPUT_LIMIT {
        return Err("invalid-structure");
    }
    let mut context = Context {
        remaining: INPUT_LIMIT * DEPTH_LIMIT,
        dialect,
        bound_dollar: false,
        bound_open: false,
        bound_bracket: false,
        sink_bracket: false,
        variable_sink: false,
        eval_sink: false,
    };
    let (commands, _) = Scanner {
        input,
        context: &mut context,
    }
    .list(0, 0, None, None, None)?;
    if context.bound_dollar
        && context.bound_open
        && (context.eval_sink
            || ((context.bound_bracket || context.sink_bracket) && context.variable_sink))
    {
        return Err("dynamic-execution");
    }
    // A local, per-invocation read, never NSS/process execution or an ambient
    // environment lookup. This also gives the Linux fallback when HOME is absent.
    let passwd = commands
        .iter()
        .any(|c| c.borrow().words.iter().any(|w| w.starts_with('~')))
        .then(|| std::fs::read_to_string("/etc/passwd").ok())
        .flatten();
    // SAFETY: reads the process identity; no pointers or state are modified.
    let uid = unsafe { libc::geteuid() };
    let home = home.or_else(|| {
        passwd.as_deref()?.lines().find_map(|line| {
            let mut fields = line.split(':');
            fields.next()?;
            fields.next()?;
            (fields.next()?.parse::<u32>().ok()? == uid)
                .then(|| fields.nth(2))
                .flatten()
        })
    });
    let mut output = Parsed {
        commands: vec![],
        directories: vec![],
    };
    for command in commands {
        let item = command.borrow();
        if item.declaration {
            continue;
        }
        if let Some(end) = item.redirect_prefix_end {
            let separator = match ch(&item.resource, end) {
                '\t' => "\t",
                '\n' => "\n",
                _ => " ",
            };
            output.commands.push(PermissionCommand {
                resource: item.resource.clone(),
                save: format!("{}{separator}*", &item.resource[..end]),
            });
            continue;
        }
        if let Some(index) = directory_index(&item.raw_words)
            && item.words[index] != "popd"
        {
            let mut end_options = false;
            let operands: Vec<_> = item.words[index + 1..]
                .iter()
                .zip(&item.raw_words[index + 1..])
                .filter(|(word, _)| {
                    if end_options {
                        return true;
                    }
                    if word.as_str() == "--" {
                        end_options = true;
                        return false;
                    }
                    word.as_str() == "-" || !word.starts_with('-')
                })
                .collect();
            let mut complete = !operands.is_empty();
            for (word, raw) in operands {
                if word != "-"
                    && let Some(directory) =
                        known_directory(word, raw, home, cwd, passwd.as_deref())
                {
                    output.directories.push(directory);
                } else {
                    complete = false;
                }
            }
            if index == 0 && complete {
                continue;
            }
        }
        let words = item
            .redirect_word_count
            .map_or(item.raw_words.as_slice(), |count| &item.raw_words[..count]);
        output.commands.push(PermissionCommand {
            resource: item.resource.clone(),
            save: format!("{} *", prefix(words).join(" ")),
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w7_ts_source_spans_cover_lists_substitutions_groups_and_redirects() {
        // shell-scan/{scan,bash-safety}.test.ts and shell-parse-syntax.test.ts.
        let heredoc = "cat <<FIRST <<'SECOND'\n$(printf first)\nFIRST\n$(not_a_command)\nSECOND";
        let cases: &[(&str, &[&str])] = &[
            (
                "git status && curl evil | sed s/x/y/",
                &["git status", "curl evil", "sed s/x/y/"],
            ),
            (
                "printf '%s\\n' 'x; rm -rf /' && printf foo\\|bar",
                &["printf '%s\\n' 'x; rm -rf /'", "printf foo\\|bar"],
            ),
            (
                "echo \"$(curl evil | sed s/x/y/)\"",
                &[
                    "echo \"$(curl evil | sed s/x/y/)\"",
                    "curl evil",
                    "sed s/x/y/",
                ],
            ),
            (
                "OUT=$(printf out) X=`printf value` printenv >$(printf path)",
                &[
                    "OUT=$(printf out) X=`printf value` printenv >$(printf path)",
                    "printf out",
                    "printf value",
                    "printf path",
                ],
            ),
            (
                "echo '$(ignored)' \"$(echo \"$(pwd)\")\"",
                &[
                    "echo '$(ignored)' \"$(echo \"$(pwd)\")\"",
                    "echo \"$(pwd)\"",
                    "pwd",
                ],
            ),
            (
                "if true; then rm -rf /; else echo safe; fi",
                &["true", "rm -rf /", "echo safe"],
            ),
            ("{ printf ok; } > output", &["printf ok", "> output"]),
            ("{ printf ok; } 2>&1", &["printf ok"]),
            (
                "(printf ok) >output && pwd",
                &["printf ok", ">output", "pwd"],
            ),
            (
                "cat <(printf secret)",
                &["cat <(printf secret)", "printf secret"],
            ),
            (heredoc, &[heredoc, "printf first"]),
            (
                "true && cat <<'EOF' && git status\nhello\nEOF",
                &["true", "cat <<'EOF'", "git status"],
            ),
        ];
        for (source, expected) in cases {
            let parsed = parse(source, Dialect::Posix, "/workspace", Some("/home/fixture"))
                .unwrap_or_else(|e| panic!("{source:?}: {e}"));
            assert_eq!(
                parsed
                    .commands
                    .iter()
                    .map(|c| c.resource.as_str())
                    .collect::<Vec<_>>(),
                *expected,
                "{source:?}"
            );
        }
    }

    #[test]
    fn w7_ts_ordinary_bash_and_zsh_syntax_is_not_blanket_opaque() {
        // Exact non-PowerShell corpus in shell-parse-syntax.test.ts:29–66.
        let cases = [
            "cat <<'EOF'\n$(not_a_command)\nEOF",
            "cat <<EOF\n$(printf hello)\nEOF",
            "cat <<-EOF\n\thello\n\tEOF",
            "cat <<EOF\nhello\nEOF\nprintf done",
            "cat <<< \"$(printf hello)\"",
            "for file in a b; do printf '%s' \"$file\"; done",
            "for file in $(printf file); do printf '%s' \"$file\"; done",
            "for file in a b; do if test -n \"$file\"; then printf '%s' \"$file\"; fi; done",
            "while IFS= read -r file; do printf '%s' \"$file\"; done < input",
            "until test -f ready; do sleep 1; done",
            "if true; then if false; then printf no; else printf yes; fi; fi",
            "if true; then :; X=$(printf value); fi",
            "case \"$target\" in *.ts) printf typescript;; *) printf other;; esac",
            "greet() { printf hello; }; greet",
            "function greet { printf hello; }; greet",
            "printf before; { printf grouped; }; (printf subshell)",
            "time git status",
            "time -p git status",
            "coproc git status",
            "if [[ -f file ]]; then cat file; fi",
            "[[ $(printf yes) = yes ]]",
            "echo ${value:-default}",
            "echo \"${value:-$(printf fallback)}\"",
            "echo ${value//before/after}",
            "echo ${arr[$(printf index)]}",
            "printf '%s' $'line1\\nline2'",
            "printf '%s' $'\\u2713\\U0001f642'",
            "printf \"%s\" $\"hello\"",
            "echo $((1 + ${value:-2}))",
            "echo $((array[$(printf 0)]))",
            "echo $[1 + 2]",
            "((count++))",
            "for ((i=0; i<2; i++)); do printf ok; done",
            "echo `printf \\2`",
            "F='$HOME/bin'; SIZE=4096; N=$(( (SIZE + 1023) / 1024 )); echo $N",
        ];
        for dialect in [Dialect::Bash, Dialect::Zsh] {
            for source in cases {
                assert!(
                    parse(source, dialect, "/workspace", Some("/home/fixture")).is_ok(),
                    "{source:?}"
                );
            }
        }
        let source = "for f in locks/waiter-*(N); do echo \"${(P)f}\"; done; ls -d src/**/target(N/) *.ts(.OL[1,3]); nohup sleep 1 >/dev/null 2>&1 &! ; echo started";
        let parsed = parse(source, Dialect::Zsh, "/workspace", None).unwrap();
        assert_eq!(
            parsed
                .commands
                .iter()
                .map(|c| c.resource.as_str())
                .collect::<Vec<_>>(),
            [
                "echo \"${(P)f}\"",
                "ls -d src/**/target(N/) *.ts(.OL[1,3])",
                "nohup sleep 1 >/dev/null 2>&1",
                "echo started"
            ]
        );
    }

    #[test]
    fn w7_ts_opaque_nested_or_dynamic_input_refuses_the_whole_scan() {
        // bash-safety.test.ts and shell-parse-syntax.test.ts:153–179.
        for source in [
            "'' > output",
            "''",
            "> output (printf ok)",
            "(printf ok) >output pwd",
            "{ printf ok; } >output X=x",
            "(printf ok) &>output pwd",
            "echo $(printf ok &&)",
            "echo $(printf ${value:-fallback)",
            "if true; fi",
            "if true; then rm x; else; fi",
            "if; then rm x; fi",
            "printf ok && # comment",
            "\"if\" true; then printf safe; fi",
            "X=x if true; then printf safe; fi",
            "(printf ok) &&",
            "{ printf ok; } ||",
            "(printf ok) |",
            "(printf ok) |&",
        ] {
            assert!(
                parse(source, Dialect::Posix, "/workspace", None).is_err(),
                "{source:?}"
            );
        }
        for source in [
            "echo *(e:'rm -rf /':)",
            "echo *(+cmd)",
            "echo *(+1)",
            "echo *(P:-f:)",
            "echo *(u:root:)",
            "echo *(:Q)",
            "a=(1); r='a[$(printf pwn)]'; echo ${(P)r}",
        ] {
            assert_eq!(
                parse(source, Dialect::Zsh, "/workspace", None).err(),
                Some("dynamic-execution"),
                "{source:?}"
            );
        }
        for source in [
            "x=\"a$(printf '\\x5b')\\$(touch pwn)]\"; echo $((x))",
            "x=$(printf 'a\\x5b\\x24\\x28touch pwn\\x29]'); echo $((x))",
            "x=$(echo 'a{$(touch pwn)]' | tr '{' '\\133'); (( x ))",
            "F='$HOME/bin'; SIZE=$(stat -f %z file); N=$(( (SIZE + 1023) / 1024 )); echo $N",
        ] {
            assert_eq!(
                parse(source, Dialect::Bash, "/workspace", None).err(),
                Some("dynamic-execution"),
                "{source:?}"
            );
        }
        let nested = format!("echo {}pwd{}", "$(".repeat(33), ")".repeat(33));
        assert!(parse(&nested, Dialect::Posix, "/workspace", None).is_err());
        assert_eq!(
            parse(
                &format!("echo {}", "x".repeat(INPUT_LIMIT)),
                Dialect::Posix,
                "/workspace",
                None
            )
            .err(),
            Some("invalid-structure")
        );
    }

    #[test]
    fn w7_dialect_and_permission_projection_retain_ts_semantics() {
        for dialect in [Dialect::Bash, Dialect::Zsh] {
            let parsed = parse("(( x = 1 )); y", dialect, "/workspace", None).unwrap();
            assert_eq!(
                parsed
                    .commands
                    .iter()
                    .map(|c| c.resource.as_str())
                    .collect::<Vec<_>>(),
                ["y"]
            );
            assert!(parse("true &>/dev/null next", dialect, "/workspace", None).is_ok());
        }
        let parsed = parse("(( x = 1 )); y", Dialect::Posix, "/workspace", None).unwrap();
        assert_eq!(
            parsed
                .commands
                .iter()
                .map(|c| c.resource.as_str())
                .collect::<Vec<_>>(),
            ["x = 1", "y"]
        );
        assert!(parse("true &>/dev/null next", Dialect::Posix, "/workspace", None).is_err());
        let parsed = parse("git status --short && npm run test unit && > output; export X=1; cd .; command cd /outside", Dialect::Bash, "/workspace", Some("/home/fixture")).unwrap();
        assert_eq!(
            parsed
                .commands
                .iter()
                .map(|c| c.save.as_str())
                .collect::<Vec<_>>(),
            ["git status *", "npm run test *", "> *", "command *"]
        );
        assert_eq!(parsed.directories, [".", "/outside"]);
        assert_eq!(Dialect::for_shell("/bin/bash"), Dialect::Bash);
        assert_eq!(Dialect::for_shell("/bin/BASH"), Dialect::Bash);
        assert_eq!(Dialect::for_shell("/bin/zsh"), Dialect::Zsh);
        assert_eq!(Dialect::for_shell("/bin/dash"), Dialect::Posix);
    }

    #[test]
    fn w7_tilde_double_slash_keeps_the_home_and_cwd_base() {
        let parsed = parse(
            "cd ~//workspace; cd ~+//worktree",
            Dialect::Bash,
            "/workspace",
            Some("/home/test"),
        )
        .unwrap();
        assert!(parsed.commands.is_empty());
        assert_eq!(
            parsed.directories,
            ["/home/test/workspace", "/workspace/worktree"]
        );
    }
}
