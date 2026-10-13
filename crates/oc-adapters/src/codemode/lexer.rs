//! Tokenizer for the Code Mode JavaScript subset.
//!
//! Tokens are produced up front so the parser can look ahead for arrow functions. Regular
//! expression literals use the usual previous-token heuristic.

use super::ast::{Pos, Str};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Tok {
    Ident(Str),
    Num(f64),
    Str(Str),
    Template {
        cooked: Option<Str>,
        raw: Str,
        head: bool,
        tail: bool,
    },
    Regex(Str, Str),
    Punct(&'static str),
    Private(Str),
    Eof,
}

#[derive(Debug, Clone)]
pub(super) struct Token {
    pub tok: Tok,
    pub pos: Pos,
    pub nl_before: bool,
}

#[derive(Debug, Clone)]
pub(super) struct SyntaxError {
    pub message: String,
    pub pos: Pos,
    pub unsupported: bool,
}

const PUNCTUATORS: &[&str] = &[
    ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=", "=>", "==", "!=",
    "<=", ">=", "&&", "||", "??", "?.", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=",
    "**", "<<", ">>", "{", "}", "(", ")", "[", "]", ";", ",", "<", ">", "+", "-", "*", "/", "%",
    "&", "|", "^", "!", "~", "?", ":", "=", ".", "@",
];

const REGEX_KEYWORDS: &[&str] = &[
    "return",
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "case",
    "do",
    "else",
    "await",
    "yield",
];

struct Lexer<'s> {
    chars: Vec<char>,
    src: &'s str,
    i: usize,
    line: u32,
    line_start: usize,
    tokens: Vec<Token>,
    braces: Vec<bool>,
}

pub(super) fn tokenize(src: &str) -> Result<Vec<Token>, SyntaxError> {
    let mut lexer = Lexer {
        chars: src.chars().collect(),
        src,
        i: 0,
        line: 1,
        line_start: 0,
        tokens: Vec::new(),
        braces: Vec::new(),
    };
    lexer.run()?;
    Ok(lexer.tokens)
}

fn is_id_start(c: char) -> bool {
    c == '$' || c == '_' || c.is_alphabetic()
}

fn is_id_part(c: char) -> bool {
    c == '$' || c == '_' || c == '\u{200c}' || c == '\u{200d}' || c.is_alphanumeric()
}

fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

impl Lexer<'_> {
    fn pos(&self) -> Pos {
        Pos {
            line: self.line,
            column: u32::try_from(self.i - self.line_start).unwrap_or(u32::MAX),
        }
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.i + offset).copied()
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T, SyntaxError> {
        Err(SyntaxError {
            message: message.into(),
            pos: self.pos(),
            unsupported: false,
        })
    }

    fn newline(&mut self, c: char) {
        if c == '\r' && self.peek(0) == Some('\n') {
            self.i += 1;
        }
        self.line += 1;
        self.line_start = self.i;
    }

    fn regex_allowed(&self) -> bool {
        match self.tokens.last().map(|token| &token.tok) {
            None => true,
            Some(Tok::Num(_) | Tok::Str(_) | Tok::Regex(..) | Tok::Private(_)) => false,
            Some(Tok::Template { tail, .. }) => !tail,
            Some(Tok::Ident(name)) => REGEX_KEYWORDS.contains(&name.as_ref()),
            Some(Tok::Punct(p)) => !matches!(*p, ")" | "]" | "}" | "++" | "--"),
            Some(Tok::Eof) => false,
        }
    }

    fn push(&mut self, tok: Tok, pos: Pos, nl_before: bool) {
        self.tokens.push(Token {
            tok,
            pos,
            nl_before,
        });
    }

    fn run(&mut self) -> Result<(), SyntaxError> {
        if self.src.starts_with("#!") {
            while let Some(c) = self.peek(0) {
                if is_line_terminator(c) {
                    break;
                }
                self.i += 1;
            }
        }
        let mut nl_before = false;
        loop {
            let Some(c) = self.peek(0) else {
                let pos = self.pos();
                self.push(Tok::Eof, pos, true);
                return Ok(());
            };
            if is_line_terminator(c) {
                self.i += 1;
                self.newline(c);
                nl_before = true;
                continue;
            }
            if c.is_whitespace() || c == '\u{feff}' {
                self.i += 1;
                continue;
            }
            if c == '/' && self.peek(1) == Some('/') {
                while let Some(c) = self.peek(0) {
                    if is_line_terminator(c) {
                        break;
                    }
                    self.i += 1;
                }
                continue;
            }
            if c == '/' && self.peek(1) == Some('*') {
                self.i += 2;
                loop {
                    match self.peek(0) {
                        None => return self.error("Unterminated comment"),
                        Some('*') if self.peek(1) == Some('/') => {
                            self.i += 2;
                            break;
                        }
                        Some(c) if is_line_terminator(c) => {
                            self.i += 1;
                            self.newline(c);
                            nl_before = true;
                        }
                        Some(_) => self.i += 1,
                    }
                }
                continue;
            }
            let pos = self.pos();
            let tok = self.token(c)?;
            self.push(tok, pos, nl_before);
            nl_before = false;
        }
    }

    fn token(&mut self, c: char) -> Result<Tok, SyntaxError> {
        if is_id_start(c) || c == '\\' {
            return Ok(Tok::Ident(self.identifier()?));
        }
        if c == '#' {
            self.i += 1;
            if !self.peek(0).is_some_and(is_id_start) {
                return self.error("Unexpected character '#'");
            }
            return Ok(Tok::Private(self.identifier()?));
        }
        if c.is_ascii_digit() || (c == '.' && self.peek(1).is_some_and(|d| d.is_ascii_digit())) {
            return self.number();
        }
        if c == '"' || c == '\'' {
            return self.string(c);
        }
        if c == '`' {
            self.i += 1;
            return self.template(true);
        }
        if c == '}' && self.braces.last() == Some(&true) {
            self.braces.pop();
            self.i += 1;
            return self.template(false);
        }
        if c == '/' && self.regex_allowed() {
            return self.regex();
        }
        for punct in PUNCTUATORS {
            if self.matches(punct) {
                if *punct == "?." && self.peek(2).is_some_and(|d| d.is_ascii_digit()) {
                    continue;
                }
                self.i += punct.chars().count();
                match *punct {
                    "{" => self.braces.push(false),
                    "}" => {
                        self.braces.pop();
                    }
                    _ => {}
                }
                return Ok(Tok::Punct(punct));
            }
        }
        self.error(format!("Unexpected character '{c}'"))
    }

    fn matches(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(offset, expected)| self.peek(offset) == Some(expected))
    }

    fn identifier(&mut self) -> Result<Str, SyntaxError> {
        let mut out = String::new();
        loop {
            match self.peek(0) {
                Some('\\') => {
                    if self.peek(1) != Some('u') {
                        return self.error("Invalid escape in identifier");
                    }
                    self.i += 2;
                    let cp = self.unicode_escape()?;
                    out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                }
                Some(c)
                    if (out.is_empty() && is_id_start(c)) || (!out.is_empty() && is_id_part(c)) =>
                {
                    out.push(c);
                    self.i += 1;
                }
                _ => break,
            }
        }
        Ok(out.into())
    }

    fn number(&mut self) -> Result<Tok, SyntaxError> {
        let radix = match (self.peek(0), self.peek(1)) {
            (Some('0'), Some('x' | 'X')) => 16,
            (Some('0'), Some('o' | 'O')) => 8,
            (Some('0'), Some('b' | 'B')) => 2,
            _ => 10,
        };
        let value = if radix != 10 {
            self.i += 2;
            let mut digits = String::new();
            while let Some(c) = self.peek(0) {
                if c == '_' {
                    self.i += 1;
                    continue;
                }
                if !c.is_digit(radix) {
                    break;
                }
                digits.push(c);
                self.i += 1;
            }
            if digits.is_empty() {
                return self.error("Invalid number");
            }
            digits.chars().fold(0.0, |acc, d| {
                acc * f64::from(radix) + f64::from(d.to_digit(radix).unwrap_or(0))
            })
        } else {
            let mut text = String::new();
            let mut seen_dot = false;
            let mut seen_exp = false;
            while let Some(c) = self.peek(0) {
                if c == '_' {
                    self.i += 1;
                    continue;
                }
                if c.is_ascii_digit() {
                    text.push(c);
                } else if c == '.' && !seen_dot && !seen_exp {
                    seen_dot = true;
                    text.push(c);
                } else if (c == 'e' || c == 'E') && !seen_exp {
                    seen_exp = true;
                    text.push('e');
                    if matches!(self.peek(1), Some('+' | '-')) {
                        self.i += 1;
                        text.push(self.peek(0).unwrap_or('+'));
                    }
                } else {
                    break;
                }
                self.i += 1;
            }
            if text.len() > 1
                && text.starts_with('0')
                && text.chars().all(|c| c.is_ascii_digit())
                && text.chars().all(|c| c < '8')
            {
                // Legacy octal literal such as 0755.
                text.chars().fold(0.0, |acc, d| {
                    acc * 8.0 + f64::from(d.to_digit(8).unwrap_or(0))
                })
            } else {
                match text.parse::<f64>() {
                    Ok(value) => value,
                    Err(_) => return self.error("Invalid number"),
                }
            }
        };
        if self.peek(0) == Some('n') {
            return Err(SyntaxError {
                message: "BigInt literals are not supported".into(),
                pos: self.pos(),
                unsupported: true,
            });
        }
        if self.peek(0).is_some_and(is_id_start) {
            return self.error("Identifier directly after number");
        }
        Ok(Tok::Num(value))
    }

    fn hex_digits(&mut self, count: usize) -> Result<u32, SyntaxError> {
        let mut value = 0u32;
        for _ in 0..count {
            let Some(d) = self.peek(0).and_then(|c| c.to_digit(16)) else {
                return self.error("Invalid hexadecimal escape sequence");
            };
            value = value * 16 + d;
            self.i += 1;
        }
        Ok(value)
    }

    /// Reads the part after `\u`, returning one code unit or code point.
    fn unicode_escape(&mut self) -> Result<u32, SyntaxError> {
        if self.peek(0) == Some('{') {
            self.i += 1;
            let mut value = 0u32;
            let mut any = false;
            while let Some(d) = self.peek(0).and_then(|c| c.to_digit(16)) {
                value = value.saturating_mul(16).saturating_add(d);
                any = true;
                self.i += 1;
            }
            if !any || self.peek(0) != Some('}') || value > 0x10ffff {
                return self.error("Invalid Unicode escape sequence");
            }
            self.i += 1;
            return Ok(value);
        }
        self.hex_digits(4)
    }

    /// Reads one escape after the backslash; a line continuation adds nothing.
    fn escape(&mut self, units: &mut Vec<u16>) -> Result<(), SyntaxError> {
        let Some(c) = self.peek(0) else {
            return self.error("Unterminated string");
        };
        self.i += 1;
        let push_char = |units: &mut Vec<u16>, c: char| {
            let mut buf = [0u16; 2];
            units.extend_from_slice(c.encode_utf16(&mut buf));
        };
        match c {
            'n' => units.push(0x0a),
            't' => units.push(0x09),
            'r' => units.push(0x0d),
            'b' => units.push(0x08),
            'f' => units.push(0x0c),
            'v' => units.push(0x0b),
            '0' if !self.peek(0).is_some_and(|d| d.is_ascii_digit()) => units.push(0),
            'x' => {
                let value = self.hex_digits(2)?;
                units.push(u16::try_from(value).unwrap_or(0xfffd));
            }
            'u' => {
                let value = self.unicode_escape()?;
                if value > 0xffff {
                    push_char(units, char::from_u32(value).unwrap_or('\u{fffd}'));
                } else {
                    units.push(u16::try_from(value).unwrap_or(0xfffd));
                }
            }
            c if is_line_terminator(c) => self.newline(c),
            c => push_char(units, c),
        }
        Ok(())
    }

    fn string(&mut self, quote: char) -> Result<Tok, SyntaxError> {
        self.i += 1;
        let mut units = Vec::new();
        loop {
            let Some(c) = self.peek(0) else {
                return self.error("Unterminated string");
            };
            if c == quote {
                self.i += 1;
                break;
            }
            if c == '\n' || c == '\r' {
                return self.error("Unterminated string");
            }
            self.i += 1;
            if c == '\\' {
                self.escape(&mut units)?;
            } else {
                let mut buf = [0u16; 2];
                units.extend_from_slice(c.encode_utf16(&mut buf));
            }
        }
        Ok(Tok::Str(String::from_utf16_lossy(&units).into()))
    }

    fn template(&mut self, head: bool) -> Result<Tok, SyntaxError> {
        let mut units = Vec::new();
        let mut raw = String::new();
        let mut cooked_ok = true;
        loop {
            let Some(c) = self.peek(0) else {
                return self.error("Unterminated template literal");
            };
            if c == '`' {
                self.i += 1;
                return Ok(Tok::Template {
                    cooked: cooked_ok.then(|| String::from_utf16_lossy(&units).into()),
                    raw: raw.into(),
                    head,
                    tail: true,
                });
            }
            if c == '$' && self.peek(1) == Some('{') {
                self.i += 2;
                self.braces.push(true);
                return Ok(Tok::Template {
                    cooked: cooked_ok.then(|| String::from_utf16_lossy(&units).into()),
                    raw: raw.into(),
                    head,
                    tail: false,
                });
            }
            self.i += 1;
            if c == '\\' {
                let start = self.i;
                if self.escape(&mut units).is_err() {
                    cooked_ok = false;
                    self.i = start + 1;
                }
                raw.push('\\');
                raw.extend(&self.chars[start..self.i]);
            } else if is_line_terminator(c) {
                if c == '\r' && self.peek(0) == Some('\n') {
                    self.i += 1;
                }
                units.push(0x0a);
                raw.push('\n');
                self.line += 1;
                self.line_start = self.i;
            } else {
                let mut buf = [0u16; 2];
                units.extend_from_slice(c.encode_utf16(&mut buf));
                raw.push(c);
            }
        }
    }

    fn regex(&mut self) -> Result<Tok, SyntaxError> {
        self.i += 1;
        let mut pattern = String::new();
        let mut in_class = false;
        loop {
            let Some(c) = self.peek(0) else {
                return self.error("Unterminated regular expression");
            };
            if is_line_terminator(c) {
                return self.error("Unterminated regular expression");
            }
            self.i += 1;
            match c {
                '\\' => {
                    pattern.push(c);
                    match self.peek(0) {
                        Some(next) if !is_line_terminator(next) => {
                            pattern.push(next);
                            self.i += 1;
                        }
                        _ => return self.error("Unterminated regular expression"),
                    }
                }
                '[' => {
                    in_class = true;
                    pattern.push(c);
                }
                ']' => {
                    in_class = false;
                    pattern.push(c);
                }
                '/' if !in_class => break,
                c => pattern.push(c),
            }
        }
        let mut flags = String::new();
        while let Some(c) = self.peek(0).filter(|c| is_id_part(*c)) {
            flags.push(c);
            self.i += 1;
        }
        Ok(Tok::Regex(pattern.into(), flags.into()))
    }
}
