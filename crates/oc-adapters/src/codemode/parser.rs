//! Recursive-descent parser for the Code Mode JavaScript subset.
//!
//! The program is parsed as the body of an implicit async function, so top-level `await` and
//! `return` are valid. Syntax outside the interpreter's allowlist is reported as unsupported
//! instead of being executed approximately.

use std::rc::Rc;

use super::ast::*;
use super::lexer::{SyntaxError, Tok, Token, tokenize};
use super::value::number_to_string;

const MAX_DEPTH: u32 = 400;

const RESERVED: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
];

#[derive(Clone, Copy)]
struct FnFlags {
    is_async: bool,
    is_generator: bool,
}

struct Parser {
    toks: Vec<Token>,
    i: usize,
    depth: u32,
    flags: Vec<FnFlags>,
    vars: Vec<Vec<Str>>,
}

type R<T> = Result<T, SyntaxError>;

pub(super) fn parse_program(src: &str) -> R<Program> {
    let toks = tokenize(src)?;
    let mut parser = Parser {
        toks,
        i: 0,
        depth: 0,
        flags: vec![FnFlags {
            is_async: true,
            is_generator: false,
        }],
        vars: vec![Vec::new()],
    };
    let mut body = Vec::new();
    while !parser.at_eof() {
        body.push(parser.statement()?);
    }
    let var_names = parser.vars.pop().unwrap_or_default();
    Ok(Program { body, var_names })
}

fn pattern_names(pattern: &Pattern, out: &mut Vec<Str>) {
    match pattern {
        Pattern::Ident(name) => {
            if !out.contains(name) {
                out.push(name.clone());
            }
        }
        Pattern::Object(props, rest) => {
            for prop in props {
                pattern_names(&prop.value, out);
            }
            if let Some(rest) = rest {
                pattern_names(rest, out);
            }
        }
        Pattern::Array(items, rest) => {
            for item in items.iter().flatten() {
                pattern_names(item, out);
            }
            if let Some(rest) = rest {
                pattern_names(rest, out);
            }
        }
        Pattern::Default(inner, _) => pattern_names(inner, out),
        Pattern::Expr(_) => {}
    }
}

impl Parser {
    fn tok(&self) -> &Tok {
        &self.toks[self.i].tok
    }

    fn tok_at(&self, offset: usize) -> &Tok {
        let index = (self.i + offset).min(self.toks.len() - 1);
        &self.toks[index].tok
    }

    fn token(&self) -> &Token {
        &self.toks[self.i]
    }

    fn pos(&self) -> Pos {
        self.toks[self.i].pos
    }

    fn at_eof(&self) -> bool {
        matches!(self.tok(), Tok::Eof)
    }

    fn advance(&mut self) -> Tok {
        let tok = self.toks[self.i].tok.clone();
        if self.i + 1 < self.toks.len() {
            self.i += 1;
        }
        tok
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.tok(), Tok::Punct(q) if *q == p)
    }

    fn is_punct_at(&self, offset: usize, p: &str) -> bool {
        matches!(self.tok_at(offset), Tok::Punct(q) if *q == p)
    }

    fn is_word(&self, word: &str) -> bool {
        matches!(self.tok(), Tok::Ident(name) if name.as_ref() == word)
    }

    fn is_word_at(&self, offset: usize, word: &str) -> bool {
        matches!(self.tok_at(offset), Tok::Ident(name) if name.as_ref() == word)
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        if self.is_punct(p) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if self.is_word(word) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn error<T>(&self, message: impl Into<String>) -> R<T> {
        Err(SyntaxError {
            message: message.into(),
            pos: self.pos(),
            unsupported: false,
        })
    }

    fn unsupported<T>(&self, message: impl Into<String>) -> R<T> {
        Err(SyntaxError {
            message: message.into(),
            pos: self.pos(),
            unsupported: true,
        })
    }

    fn unexpected<T>(&self) -> R<T> {
        let shown = match self.tok() {
            Tok::Eof => "end of input".to_string(),
            Tok::Ident(name) => format!("'{name}'"),
            Tok::Punct(p) => format!("'{p}'"),
            Tok::Num(n) => format!("number {}", number_to_string(*n)),
            Tok::Str(_) => "string".to_string(),
            Tok::Template { .. } => "template".to_string(),
            Tok::Regex(..) => "regular expression".to_string(),
            Tok::Private(name) => format!("'#{name}'"),
        };
        self.error(format!("Unexpected token {shown}"))
    }

    fn expect_punct(&mut self, p: &str) -> R<()> {
        if self.eat_punct(p) {
            Ok(())
        } else {
            self.unexpected()
        }
    }

    fn consume_semicolon(&mut self) -> R<()> {
        if self.eat_punct(";") || self.is_punct("}") || self.at_eof() || self.token().nl_before {
            return Ok(());
        }
        self.unexpected()
    }

    fn enter(&mut self) -> R<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.unsupported("Program is nested too deeply");
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn in_async(&self) -> bool {
        self.flags.last().is_some_and(|flags| flags.is_async)
    }

    fn in_generator(&self) -> bool {
        self.flags.last().is_some_and(|flags| flags.is_generator)
    }

    fn binding_identifier(&mut self) -> R<Str> {
        match self.tok().clone() {
            Tok::Ident(name) if !RESERVED.contains(&name.as_ref()) => {
                if (name.as_ref() == "await" && self.in_async())
                    || (name.as_ref() == "yield" && self.in_generator())
                {
                    return self.unexpected();
                }
                self.advance();
                Ok(name)
            }
            _ => self.unexpected(),
        }
    }

    // ----- statements -------------------------------------------------------------------

    fn statement(&mut self) -> R<Stmt> {
        self.enter()?;
        let result = self.statement_inner();
        self.leave();
        result
    }

    fn statement_inner(&mut self) -> R<Stmt> {
        let tok = self.tok().clone();
        match tok {
            Tok::Punct("{") => Ok(Stmt::Block(self.block()?)),
            Tok::Punct(";") => {
                self.advance();
                Ok(Stmt::Empty)
            }
            Tok::Ident(word) => match word.as_ref() {
                "var" | "const" => {
                    self.advance();
                    let kind = if word.as_ref() == "var" {
                        VarKind::Var
                    } else {
                        VarKind::Const
                    };
                    let decls = self.declarations(kind, false)?;
                    self.consume_semicolon()?;
                    Ok(Stmt::Var(kind, decls))
                }
                "let" if matches!(self.tok_at(1), Tok::Ident(_) | Tok::Punct("[" | "{")) => {
                    self.advance();
                    let decls = self.declarations(VarKind::Let, false)?;
                    self.consume_semicolon()?;
                    Ok(Stmt::Var(VarKind::Let, decls))
                }
                "function" => {
                    self.advance();
                    Ok(Stmt::Func(self.function_declaration(false)?))
                }
                "async" if self.is_word_at(1, "function") && !self.toks[self.i + 1].nl_before => {
                    self.advance();
                    self.advance();
                    Ok(Stmt::Func(self.function_declaration(true)?))
                }
                "class" => {
                    self.advance();
                    let class = self.class(true)?;
                    Ok(Stmt::Class(class))
                }
                "if" => {
                    self.advance();
                    self.expect_punct("(")?;
                    let test = self.expression(false)?;
                    self.expect_punct(")")?;
                    let then = self.statement()?;
                    let other = if self.eat_word("else") {
                        Some(Box::new(self.statement()?))
                    } else {
                        None
                    };
                    Ok(Stmt::If(test, Box::new(then), other))
                }
                "for" => self.for_statement(),
                "while" => {
                    self.advance();
                    self.expect_punct("(")?;
                    let test = self.expression(false)?;
                    self.expect_punct(")")?;
                    Ok(Stmt::While(test, Box::new(self.statement()?)))
                }
                "do" => {
                    self.advance();
                    let body = self.statement()?;
                    if !self.eat_word("while") {
                        return self.unexpected();
                    }
                    self.expect_punct("(")?;
                    let test = self.expression(false)?;
                    self.expect_punct(")")?;
                    self.eat_punct(";");
                    Ok(Stmt::DoWhile(Box::new(body), test))
                }
                "return" => {
                    self.advance();
                    let value = if self.is_punct(";")
                        || self.is_punct("}")
                        || self.at_eof()
                        || self.token().nl_before
                    {
                        None
                    } else {
                        Some(self.expression(false)?)
                    };
                    self.consume_semicolon()?;
                    Ok(Stmt::Return(value))
                }
                "break" | "continue" => {
                    self.advance();
                    let label = match self.tok().clone() {
                        Tok::Ident(name)
                            if !self.token().nl_before && !RESERVED.contains(&name.as_ref()) =>
                        {
                            self.advance();
                            Some(name)
                        }
                        _ => None,
                    };
                    self.consume_semicolon()?;
                    Ok(if word.as_ref() == "break" {
                        Stmt::Break(label)
                    } else {
                        Stmt::Continue(label)
                    })
                }
                "throw" => {
                    let pos = self.pos();
                    self.advance();
                    if self.token().nl_before {
                        return self.error("Illegal newline after throw");
                    }
                    let value = self.expression(false)?;
                    self.consume_semicolon()?;
                    Ok(Stmt::Throw(value, pos))
                }
                "try" => self.try_statement(),
                "switch" => self.switch_statement(),
                "debugger" => {
                    self.advance();
                    self.consume_semicolon()?;
                    Ok(Stmt::Empty)
                }
                "import" if !self.is_punct_at(1, ".") => {
                    self.unsupported("Imports are not available in Code Mode")
                }
                "export" => self.unsupported("Exports are not available in Code Mode"),
                "with" => self.unsupported("'with' statements are not supported"),
                _ if self.is_punct_at(1, ":") && !RESERVED.contains(&word.as_ref()) => {
                    self.advance();
                    self.advance();
                    if self.is_word("function") {
                        return self.unsupported("Labeled function declarations are not supported");
                    }
                    Ok(Stmt::Labeled(word, Box::new(self.statement()?)))
                }
                _ => self.expression_statement(),
            },
            _ => self.expression_statement(),
        }
    }

    fn expression_statement(&mut self) -> R<Stmt> {
        let expr = self.expression(false)?;
        self.consume_semicolon()?;
        Ok(Stmt::Expr(expr))
    }

    fn block(&mut self) -> R<Vec<Stmt>> {
        self.expect_punct("{")?;
        let mut body = Vec::new();
        while !self.is_punct("}") {
            if self.at_eof() {
                return self.unexpected();
            }
            body.push(self.statement()?);
        }
        self.advance();
        Ok(body)
    }

    fn declarations(&mut self, kind: VarKind, no_in: bool) -> R<Vec<Declarator>> {
        let mut decls = Vec::new();
        loop {
            let target = self.binding_target()?;
            if kind == VarKind::Var {
                let vars = self.vars.last_mut().expect("function var scope");
                pattern_names(&target, vars);
            }
            let init = if self.eat_punct("=") {
                Some(self.assignment(no_in)?)
            } else {
                None
            };
            decls.push(Declarator { target, init });
            if !self.eat_punct(",") {
                break;
            }
        }
        Ok(decls)
    }

    fn for_statement(&mut self) -> R<Stmt> {
        self.advance();
        let is_await = if self.is_word("await") {
            if !self.in_async() {
                return self.unexpected();
            }
            self.advance();
            true
        } else {
            false
        };
        self.expect_punct("(")?;
        let mut init = None;
        if self.is_punct(";") {
            // No initializer.
        } else if self.is_word("var")
            || self.is_word("const")
            || (self.is_word("let")
                && matches!(self.tok_at(1), Tok::Ident(_) | Tok::Punct("[" | "{")))
        {
            let kind = match self.advance() {
                Tok::Ident(word) if word.as_ref() == "var" => VarKind::Var,
                Tok::Ident(word) if word.as_ref() == "let" => VarKind::Let,
                _ => VarKind::Const,
            };
            let mut decls = self.declarations(kind, true)?;
            if decls.len() == 1
                && decls[0].init.is_none()
                && (self.is_word("of") || self.is_word("in"))
            {
                let of = self.is_word("of");
                self.advance();
                let target = ForTarget::Decl(kind, decls.pop().expect("one declarator").target);
                return self.for_each(target, of, is_await);
            }
            init = Some(ForInit::Var(kind, decls));
        } else {
            let expr = self.expression(true)?;
            if self.is_word("of") || self.is_word("in") {
                let of = self.is_word("of");
                self.advance();
                let target = ForTarget::Assign(self.to_pattern(expr)?);
                return self.for_each(target, of, is_await);
            }
            init = Some(ForInit::Expr(expr));
        }
        if is_await {
            return self.unexpected();
        }
        self.expect_punct(";")?;
        let test = if self.is_punct(";") {
            None
        } else {
            Some(self.expression(false)?)
        };
        self.expect_punct(";")?;
        let update = if self.is_punct(")") {
            None
        } else {
            Some(self.expression(false)?)
        };
        self.expect_punct(")")?;
        let body = self.statement()?;
        Ok(Stmt::For(Box::new(ForStmt {
            init,
            test,
            update,
            body,
        })))
    }

    fn for_each(&mut self, target: ForTarget, of: bool, is_await: bool) -> R<Stmt> {
        if is_await && !of {
            return self.unexpected();
        }
        let iterable = if of {
            self.assignment(false)?
        } else {
            self.expression(false)?
        };
        self.expect_punct(")")?;
        let body = self.statement()?;
        let each = Box::new(ForEach {
            target,
            iterable,
            body,
            is_await,
        });
        Ok(if of {
            Stmt::ForOf(each)
        } else {
            Stmt::ForIn(each)
        })
    }

    fn try_statement(&mut self) -> R<Stmt> {
        self.advance();
        let block = self.block()?;
        let mut param = None;
        let mut handler = None;
        if self.eat_word("catch") {
            if self.eat_punct("(") {
                param = Some(self.binding_target()?);
                self.expect_punct(")")?;
            }
            handler = Some(self.block()?);
        }
        let finalizer = if self.eat_word("finally") {
            Some(self.block()?)
        } else {
            None
        };
        if handler.is_none() && finalizer.is_none() {
            return self.error("Missing catch or finally after try");
        }
        Ok(Stmt::Try(Box::new(TryStmt {
            block,
            param,
            handler,
            finalizer,
        })))
    }

    fn switch_statement(&mut self) -> R<Stmt> {
        self.advance();
        self.expect_punct("(")?;
        let discriminant = self.expression(false)?;
        self.expect_punct(")")?;
        self.expect_punct("{")?;
        let mut cases = Vec::new();
        while !self.eat_punct("}") {
            let test = if self.eat_word("case") {
                Some(self.expression(false)?)
            } else if self.eat_word("default") {
                None
            } else {
                return self.unexpected();
            };
            self.expect_punct(":")?;
            let mut body = Vec::new();
            while !self.is_word("case") && !self.is_word("default") && !self.is_punct("}") {
                if self.at_eof() {
                    return self.unexpected();
                }
                body.push(self.statement()?);
            }
            cases.push(Case { test, body });
        }
        Ok(Stmt::Switch(discriminant, cases))
    }

    // ----- functions and classes --------------------------------------------------------

    fn function_declaration(&mut self, is_async: bool) -> R<Rc<Function>> {
        let is_generator = self.eat_punct("*");
        let name = self.binding_identifier()?;
        self.function_rest(Some(name), is_async, is_generator)
    }

    fn function_rest(
        &mut self,
        name: Option<Str>,
        is_async: bool,
        is_generator: bool,
    ) -> R<Rc<Function>> {
        self.flags.push(FnFlags {
            is_async,
            is_generator,
        });
        self.vars.push(Vec::new());
        let result = (|| {
            let (params, rest) = self.parameters()?;
            let body = self.block()?;
            Ok((params, rest, body))
        })();
        self.flags.pop();
        let var_names = self.vars.pop().unwrap_or_default();
        let (params, rest, body) = result?;
        Ok(Rc::new(Function {
            name,
            params,
            rest,
            body: FuncBody::Block(body),
            is_async,
            is_arrow: false,
            is_generator,
            var_names,
        }))
    }

    fn parameters(&mut self) -> R<(Vec<Pattern>, Option<Pattern>)> {
        self.expect_punct("(")?;
        let mut params = Vec::new();
        let mut rest = None;
        while !self.eat_punct(")") {
            if self.eat_punct("...") {
                rest = Some(self.binding_target()?);
                self.eat_punct(",");
                self.expect_punct(")")?;
                break;
            }
            params.push(self.binding_element()?);
            if !self.is_punct(")") {
                self.expect_punct(",")?;
            }
        }
        Ok((params, rest))
    }

    fn binding_element(&mut self) -> R<Pattern> {
        let target = self.binding_target()?;
        if self.eat_punct("=") {
            let value = self.assignment(false)?;
            return Ok(Pattern::Default(Box::new(target), Box::new(value)));
        }
        Ok(target)
    }

    fn binding_target(&mut self) -> R<Pattern> {
        if self.eat_punct("[") {
            let mut items = Vec::new();
            let mut rest = None;
            while !self.eat_punct("]") {
                if self.is_punct(",") {
                    self.advance();
                    items.push(None);
                    continue;
                }
                if self.eat_punct("...") {
                    rest = Some(Box::new(self.binding_target()?));
                    self.eat_punct(",");
                    self.expect_punct("]")?;
                    break;
                }
                items.push(Some(self.binding_element()?));
                if !self.is_punct("]") {
                    self.expect_punct(",")?;
                }
            }
            return Ok(Pattern::Array(items, rest));
        }
        if self.eat_punct("{") {
            let mut props = Vec::new();
            let mut rest = None;
            while !self.eat_punct("}") {
                if self.eat_punct("...") {
                    rest = Some(Box::new(Pattern::Ident(self.binding_identifier()?)));
                    self.eat_punct(",");
                    self.expect_punct("}")?;
                    break;
                }
                let shorthand = matches!(self.tok(), Tok::Ident(_))
                    && !self.is_punct_at(1, ":")
                    && !self.is_punct_at(1, "(");
                if shorthand {
                    let name = self.binding_identifier()?;
                    let mut value = Pattern::Ident(name.clone());
                    if self.eat_punct("=") {
                        let default = self.assignment(false)?;
                        value = Pattern::Default(Box::new(value), Box::new(default));
                    }
                    props.push(PatProp {
                        key: PropName::Static(name),
                        value,
                    });
                } else {
                    let key = self.property_name()?;
                    self.expect_punct(":")?;
                    let value = self.binding_element()?;
                    props.push(PatProp { key, value });
                }
                if !self.is_punct("}") {
                    self.expect_punct(",")?;
                }
            }
            return Ok(Pattern::Object(props, rest));
        }
        Ok(Pattern::Ident(self.binding_identifier()?))
    }

    fn property_name(&mut self) -> R<PropName> {
        match self.advance() {
            Tok::Ident(name) | Tok::Str(name) => Ok(PropName::Static(name)),
            Tok::Num(n) => Ok(PropName::Static(number_to_string(n).into())),
            Tok::Punct("[") => {
                let expr = self.assignment(false)?;
                self.expect_punct("]")?;
                Ok(PropName::Computed(Box::new(expr)))
            }
            Tok::Private(_) => {
                self.i -= 1;
                self.unsupported("Private class members are not supported")
            }
            _ => {
                self.i -= 1;
                self.unexpected()
            }
        }
    }

    /// Whether the current token can start a property name in an object or class body.
    fn at_property_name(&self) -> bool {
        matches!(
            self.tok(),
            Tok::Ident(_) | Tok::Str(_) | Tok::Num(_) | Tok::Punct("[") | Tok::Private(_)
        )
    }

    fn method(&mut self, is_async: bool, is_generator: bool) -> R<Rc<Function>> {
        self.function_rest(None, is_async, is_generator)
    }

    /// Parses get/set/async/generator prefixes. Returns `(kind, is_async, is_generator)`.
    fn method_prefix(&mut self) -> (MethodKind, bool, bool) {
        let next_is_name = |p: &Parser| {
            matches!(
                p.tok_at(1),
                Tok::Ident(_) | Tok::Str(_) | Tok::Num(_) | Tok::Punct("[") | Tok::Private(_)
            )
        };
        if (self.is_word("get") || self.is_word("set")) && next_is_name(self) {
            let kind = if self.is_word("get") {
                MethodKind::Get
            } else {
                MethodKind::Set
            };
            self.advance();
            return (kind, false, false);
        }
        if self.is_word("async")
            && !self.toks[self.i + 1].nl_before
            && (next_is_name(self) || self.is_punct_at(1, "*"))
        {
            self.advance();
            let generator = self.eat_punct("*");
            return (MethodKind::Method, true, generator);
        }
        if self.eat_punct("*") {
            return (MethodKind::Method, false, true);
        }
        (MethodKind::Method, false, false)
    }

    fn class(&mut self, declaration: bool) -> R<Rc<Class>> {
        let name = match self.tok() {
            Tok::Ident(word) if word.as_ref() != "extends" => Some(self.binding_identifier()?),
            _ if declaration => return self.unexpected(),
            _ => None,
        };
        let extends = if self.eat_word("extends") {
            Some(self.lhs_expression()?)
        } else {
            None
        };
        self.expect_punct("{")?;
        let mut constructor = None;
        let mut members = Vec::new();
        let mut fields = Vec::new();
        while !self.eat_punct("}") {
            if self.eat_punct(";") {
                continue;
            }
            let mut is_static = false;
            if self.is_word("static") && !self.is_punct_at(1, "(") && !self.is_punct_at(1, "=") {
                self.advance();
                if self.is_punct("{") {
                    return self.unsupported("Static initialization blocks are not supported");
                }
                is_static = true;
            }
            let (kind, is_async, is_generator) = self.method_prefix();
            if !self.at_property_name() {
                return self.unexpected();
            }
            let key = self.property_name()?;
            if self.is_punct("(") {
                let func = self.method(is_async, is_generator)?;
                let is_ctor =
                    matches!(&key, PropName::Static(name) if name.as_ref() == "constructor");
                if is_ctor && !is_static {
                    if !matches!(kind, MethodKind::Method) || is_async || is_generator {
                        return self
                            .error("Class constructor may not be an accessor, async or generator");
                    }
                    constructor = Some(func);
                } else {
                    members.push(ClassMember {
                        key,
                        is_static,
                        kind,
                        func,
                    });
                }
                continue;
            }
            if !matches!(kind, MethodKind::Method) || is_async || is_generator {
                return self.unexpected();
            }
            let value = if self.eat_punct("=") {
                self.flags.push(FnFlags {
                    is_async: false,
                    is_generator: false,
                });
                let value = self.assignment(false);
                self.flags.pop();
                Some(value?)
            } else {
                None
            };
            self.consume_semicolon()?;
            fields.push(ClassField {
                key,
                is_static,
                value,
            });
        }
        Ok(Rc::new(Class {
            name,
            extends,
            constructor,
            members,
            fields,
        }))
    }

    // ----- expressions ------------------------------------------------------------------

    fn expression(&mut self, no_in: bool) -> R<Expr> {
        let first = self.assignment(no_in)?;
        if !self.is_punct(",") {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat_punct(",") {
            items.push(self.assignment(no_in)?);
        }
        Ok(Expr::Seq(items))
    }

    fn matching_paren(&self, open: usize) -> Option<usize> {
        let mut depth = 0usize;
        for (index, token) in self.toks.iter().enumerate().skip(open) {
            match token.tok {
                Tok::Punct("(" | "[" | "{") => depth += 1,
                Tok::Template {
                    head: true,
                    tail: false,
                    ..
                } => depth += 1,
                Tok::Template {
                    head: false,
                    tail: true,
                    ..
                } => depth = depth.saturating_sub(1),
                Tok::Punct(")" | "]" | "}") => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(index);
                    }
                }
                Tok::Eof => return None,
                _ => {}
            }
        }
        None
    }

    fn arrow_ahead(&self, offset: usize) -> bool {
        match self.tok_at(offset) {
            Tok::Ident(name) if !RESERVED.contains(&name.as_ref()) => {
                self.is_punct_at(offset + 1, "=>")
                    && !self.toks[(self.i + offset + 1).min(self.toks.len() - 1)].nl_before
            }
            Tok::Punct("(") => self.matching_paren(self.i + offset).is_some_and(|close| {
                matches!(
                    self.toks.get(close + 1).map(|t| &t.tok),
                    Some(Tok::Punct("=>"))
                )
            }),
            _ => false,
        }
    }

    fn assignment(&mut self, no_in: bool) -> R<Expr> {
        self.enter()?;
        let result = self.assignment_inner(no_in);
        self.leave();
        result
    }

    fn assignment_inner(&mut self, no_in: bool) -> R<Expr> {
        if self.is_word("yield") && self.in_generator() {
            self.advance();
            if self.token().nl_before
                || matches!(
                    self.tok(),
                    Tok::Punct(")" | "]" | "}" | "," | ";" | ":") | Tok::Eof
                )
            {
                return Ok(Expr::Yield(None, false));
            }
            let delegate = self.eat_punct("*");
            let value = self.assignment(no_in)?;
            return Ok(Expr::Yield(Some(Box::new(value)), delegate));
        }
        if self.is_word("async") && !self.toks[self.i + 1].nl_before && self.arrow_ahead(1) {
            self.advance();
            return self.arrow(true, no_in);
        }
        if self.arrow_ahead(0) {
            return self.arrow(false, no_in);
        }
        let left = self.conditional(no_in)?;
        let op = match self.tok() {
            Tok::Punct(p) => match *p {
                "=" => Some(AssignOp::Assign),
                "+=" => Some(AssignOp::Op(BinaryOp::Add)),
                "-=" => Some(AssignOp::Op(BinaryOp::Sub)),
                "*=" => Some(AssignOp::Op(BinaryOp::Mul)),
                "/=" => Some(AssignOp::Op(BinaryOp::Div)),
                "%=" => Some(AssignOp::Op(BinaryOp::Rem)),
                "**=" => Some(AssignOp::Op(BinaryOp::Exp)),
                "<<=" => Some(AssignOp::Op(BinaryOp::Shl)),
                ">>=" => Some(AssignOp::Op(BinaryOp::Shr)),
                ">>>=" => Some(AssignOp::Op(BinaryOp::UShr)),
                "&=" => Some(AssignOp::Op(BinaryOp::BitAnd)),
                "|=" => Some(AssignOp::Op(BinaryOp::BitOr)),
                "^=" => Some(AssignOp::Op(BinaryOp::BitXor)),
                "&&=" => Some(AssignOp::Logical(LogicalOp::And)),
                "||=" => Some(AssignOp::Logical(LogicalOp::Or)),
                "??=" => Some(AssignOp::Logical(LogicalOp::Nullish)),
                _ => None,
            },
            _ => None,
        };
        let Some(op) = op else {
            return Ok(left);
        };
        let target = if op == AssignOp::Assign {
            self.to_pattern(left)?
        } else {
            match left {
                Expr::Ident(name, _) => Pattern::Ident(name),
                expr @ Expr::Member {
                    optional: false, ..
                } => Pattern::Expr(Box::new(expr)),
                _ => return self.error("Invalid assignment target"),
            }
        };
        self.advance();
        let value = self.assignment(no_in)?;
        Ok(Expr::Assign(op, Box::new(target), Box::new(value)))
    }

    fn arrow(&mut self, is_async: bool, no_in: bool) -> R<Expr> {
        self.flags.push(FnFlags {
            is_async,
            is_generator: false,
        });
        self.vars.push(Vec::new());
        let result = (|| {
            let (params, rest) = if self.is_punct("(") {
                self.parameters()?
            } else {
                (vec![Pattern::Ident(self.binding_identifier()?)], None)
            };
            self.expect_punct("=>")?;
            let body = if self.is_punct("{") {
                FuncBody::Block(self.block()?)
            } else {
                FuncBody::Expr(Box::new(self.assignment(no_in)?))
            };
            Ok((params, rest, body))
        })();
        self.flags.pop();
        let var_names = self.vars.pop().unwrap_or_default();
        let (params, rest, body) = result?;
        Ok(Expr::Function(Rc::new(Function {
            name: None,
            params,
            rest,
            body,
            is_async,
            is_arrow: true,
            is_generator: false,
            var_names,
        })))
    }

    fn to_pattern(&self, expr: Expr) -> R<Pattern> {
        match expr {
            Expr::Ident(name, _) => Ok(Pattern::Ident(name)),
            expr @ Expr::Member {
                optional: false, ..
            } => Ok(Pattern::Expr(Box::new(expr))),
            Expr::Assign(AssignOp::Assign, target, value) => Ok(Pattern::Default(target, value)),
            Expr::Array(items) => {
                let mut out = Vec::new();
                let mut rest = None;
                let count = items.len();
                for (index, item) in items.into_iter().enumerate() {
                    match item {
                        ArrayElem::Hole => out.push(None),
                        ArrayElem::Expr(expr) => out.push(Some(self.to_pattern(expr)?)),
                        ArrayElem::Spread(expr) if index + 1 == count => {
                            rest = Some(Box::new(self.to_pattern(expr)?));
                        }
                        ArrayElem::Spread(_) => {
                            return self.error("Rest element must be last element");
                        }
                    }
                }
                Ok(Pattern::Array(out, rest))
            }
            Expr::Object(props) => {
                let mut out = Vec::new();
                let mut rest = None;
                let count = props.len();
                for (index, prop) in props.into_iter().enumerate() {
                    match prop {
                        PropDef::Value(key, value) => out.push(PatProp {
                            key,
                            value: self.to_pattern(value)?,
                        }),
                        PropDef::Shorthand(name) => out.push(PatProp {
                            key: PropName::Static(name.clone()),
                            value: Pattern::Ident(name),
                        }),
                        PropDef::CoverInit(name, default) => out.push(PatProp {
                            key: PropName::Static(name.clone()),
                            value: Pattern::Default(Box::new(Pattern::Ident(name)), default),
                        }),
                        PropDef::Spread(expr) if index + 1 == count => {
                            rest = Some(Box::new(self.to_pattern(expr)?));
                        }
                        _ => return self.error("Invalid destructuring assignment target"),
                    }
                }
                Ok(Pattern::Object(out, rest))
            }
            _ => self.error("Invalid assignment target"),
        }
    }

    fn conditional(&mut self, no_in: bool) -> R<Expr> {
        let test = self.binary(0, no_in)?;
        if !self.eat_punct("?") {
            return Ok(test);
        }
        let then = self.assignment(false)?;
        self.expect_punct(":")?;
        let other = self.assignment(no_in)?;
        Ok(Expr::Cond(Box::new(test), Box::new(then), Box::new(other)))
    }

    fn binary_op(&self, no_in: bool) -> Option<(u8, BinOrLogical)> {
        use BinOrLogical::{B, L};
        let op = match self.tok() {
            Tok::Punct(p) => match *p {
                "??" => (1, L(LogicalOp::Nullish)),
                "||" => (1, L(LogicalOp::Or)),
                "&&" => (2, L(LogicalOp::And)),
                "|" => (3, B(BinaryOp::BitOr)),
                "^" => (4, B(BinaryOp::BitXor)),
                "&" => (5, B(BinaryOp::BitAnd)),
                "==" => (6, B(BinaryOp::Eq)),
                "!=" => (6, B(BinaryOp::NotEq)),
                "===" => (6, B(BinaryOp::StrictEq)),
                "!==" => (6, B(BinaryOp::StrictNotEq)),
                "<" => (7, B(BinaryOp::Lt)),
                ">" => (7, B(BinaryOp::Gt)),
                "<=" => (7, B(BinaryOp::LtEq)),
                ">=" => (7, B(BinaryOp::GtEq)),
                "<<" => (8, B(BinaryOp::Shl)),
                ">>" => (8, B(BinaryOp::Shr)),
                ">>>" => (8, B(BinaryOp::UShr)),
                "+" => (9, B(BinaryOp::Add)),
                "-" => (9, B(BinaryOp::Sub)),
                "*" => (10, B(BinaryOp::Mul)),
                "/" => (10, B(BinaryOp::Div)),
                "%" => (10, B(BinaryOp::Rem)),
                "**" => (11, B(BinaryOp::Exp)),
                _ => return None,
            },
            Tok::Ident(word) => match word.as_ref() {
                "instanceof" => (7, B(BinaryOp::InstanceOf)),
                "in" if !no_in => (7, B(BinaryOp::In)),
                _ => return None,
            },
            _ => return None,
        };
        Some(op)
    }

    fn binary(&mut self, min: u8, no_in: bool) -> R<Expr> {
        let mut left = self.unary()?;
        while let Some((prec, op)) = self.binary_op(no_in) {
            if prec < min || prec == 0 {
                break;
            }
            self.advance();
            // `**` is right-associative; everything else is left-associative.
            let next = if prec == 11 { prec } else { prec + 1 };
            let right = self.binary(next, no_in)?;
            left = match op {
                BinOrLogical::B(op) => Expr::Binary(op, Box::new(left), Box::new(right)),
                BinOrLogical::L(op) => Expr::Logical(op, Box::new(left), Box::new(right)),
            };
        }
        Ok(left)
    }

    fn unary(&mut self) -> R<Expr> {
        self.enter()?;
        let result = self.unary_inner();
        self.leave();
        result
    }

    fn unary_inner(&mut self) -> R<Expr> {
        let op = match self.tok() {
            Tok::Punct("!") => Some(UnaryOp::Not),
            Tok::Punct("-") => Some(UnaryOp::Minus),
            Tok::Punct("+") => Some(UnaryOp::Plus),
            Tok::Punct("~") => Some(UnaryOp::BitNot),
            Tok::Ident(word) => match word.as_ref() {
                "typeof" => Some(UnaryOp::Typeof),
                "void" => Some(UnaryOp::Void),
                "delete" => Some(UnaryOp::Delete),
                _ => None,
            },
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            let operand = self.unary()?;
            return Ok(Expr::Unary(op, Box::new(operand)));
        }
        if self.is_word("await") && self.in_async() {
            self.advance();
            let operand = self.unary()?;
            return Ok(Expr::Await(Box::new(operand)));
        }
        if self.is_punct("++") || self.is_punct("--") {
            let increment = self.is_punct("++");
            self.advance();
            let target = self.unary()?;
            self.check_update_target(&target)?;
            return Ok(Expr::Update {
                increment,
                prefix: true,
                target: Box::new(target),
            });
        }
        let expr = self.lhs_expression()?;
        if (self.is_punct("++") || self.is_punct("--")) && !self.token().nl_before {
            let increment = self.is_punct("++");
            self.check_update_target(&expr)?;
            self.advance();
            return Ok(Expr::Update {
                increment,
                prefix: false,
                target: Box::new(expr),
            });
        }
        Ok(expr)
    }

    fn check_update_target(&self, expr: &Expr) -> R<()> {
        match expr {
            Expr::Ident(..)
            | Expr::Member {
                optional: false, ..
            } => Ok(()),
            _ => self.error("Invalid left-hand side expression in update operation"),
        }
    }

    fn arguments(&mut self) -> R<Vec<Arg>> {
        self.expect_punct("(")?;
        let mut args = Vec::new();
        while !self.eat_punct(")") {
            if self.eat_punct("...") {
                args.push(Arg::Spread(self.assignment(false)?));
            } else {
                args.push(Arg::Expr(self.assignment(false)?));
            }
            if !self.is_punct(")") {
                self.expect_punct(",")?;
            }
        }
        Ok(args)
    }

    fn member_name(&mut self) -> R<Str> {
        match self.advance() {
            Tok::Ident(name) => Ok(name),
            Tok::Private(_) => {
                self.i -= 1;
                self.unsupported("Private class members are not supported")
            }
            _ => {
                self.i -= 1;
                self.unexpected()
            }
        }
    }

    fn lhs_expression(&mut self) -> R<Expr> {
        let mut expr = if self.is_word("new") {
            self.new_expression()?
        } else if self.is_word("super") {
            self.super_expression()?
        } else if self.is_word("import") {
            return self.unsupported("Imports are not available in Code Mode");
        } else {
            self.primary()?
        };
        let mut chained = false;
        loop {
            match self.tok().clone() {
                Tok::Punct(".") => {
                    self.advance();
                    let name = self.member_name()?;
                    expr = Expr::Member {
                        object: Box::new(expr),
                        key: MemberKey::Name(name),
                        optional: false,
                    };
                }
                Tok::Punct("?.") => {
                    self.advance();
                    chained = true;
                    if self.is_punct("(") {
                        let pos = self.pos();
                        let args = self.arguments()?;
                        expr = Expr::Call {
                            callee: Box::new(expr),
                            args,
                            optional: true,
                            pos,
                        };
                    } else if self.eat_punct("[") {
                        let key = self.expression(false)?;
                        self.expect_punct("]")?;
                        expr = Expr::Member {
                            object: Box::new(expr),
                            key: MemberKey::Computed(Box::new(key)),
                            optional: true,
                        };
                    } else {
                        let name = self.member_name()?;
                        expr = Expr::Member {
                            object: Box::new(expr),
                            key: MemberKey::Name(name),
                            optional: true,
                        };
                    }
                }
                Tok::Punct("[") => {
                    self.advance();
                    let key = self.expression(false)?;
                    self.expect_punct("]")?;
                    expr = Expr::Member {
                        object: Box::new(expr),
                        key: MemberKey::Computed(Box::new(key)),
                        optional: false,
                    };
                }
                Tok::Punct("(") => {
                    let pos = self.pos();
                    let args = self.arguments()?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                        optional: false,
                        pos,
                    };
                }
                Tok::Template { head: true, .. } => {
                    if chained {
                        return self.error("Tagged template cannot be used in optional chain");
                    }
                    let template = self.template()?;
                    expr = Expr::Tagged(Box::new(expr), Rc::new(template));
                }
                _ => break,
            }
        }
        Ok(if chained {
            Expr::Chain(Box::new(expr))
        } else {
            expr
        })
    }

    fn new_expression(&mut self) -> R<Expr> {
        let pos = self.pos();
        self.advance();
        if self.is_punct(".") {
            return self.unsupported("new.target is not supported");
        }
        let mut callee = if self.is_word("new") {
            self.new_expression()?
        } else {
            self.primary()?
        };
        loop {
            if self.eat_punct(".") {
                let name = self.member_name()?;
                callee = Expr::Member {
                    object: Box::new(callee),
                    key: MemberKey::Name(name),
                    optional: false,
                };
            } else if self.eat_punct("[") {
                let key = self.expression(false)?;
                self.expect_punct("]")?;
                callee = Expr::Member {
                    object: Box::new(callee),
                    key: MemberKey::Computed(Box::new(key)),
                    optional: false,
                };
            } else {
                break;
            }
        }
        let args = if self.is_punct("(") {
            self.arguments()?
        } else {
            Vec::new()
        };
        Ok(Expr::New(Box::new(callee), args, pos))
    }

    fn super_expression(&mut self) -> R<Expr> {
        self.advance();
        if self.is_punct("(") {
            return Ok(Expr::SuperCall(self.arguments()?));
        }
        if self.eat_punct(".") {
            return Ok(Expr::SuperMember(MemberKey::Name(self.member_name()?)));
        }
        if self.eat_punct("[") {
            let key = self.expression(false)?;
            self.expect_punct("]")?;
            return Ok(Expr::SuperMember(MemberKey::Computed(Box::new(key))));
        }
        self.error("'super' keyword unexpected here")
    }

    fn template(&mut self) -> R<Template> {
        let mut cooked = Vec::new();
        let mut raw = Vec::new();
        let mut exprs = Vec::new();
        loop {
            match self.advance() {
                Tok::Template {
                    cooked: c,
                    raw: r,
                    tail,
                    ..
                } => {
                    cooked.push(c);
                    raw.push(r);
                    if tail {
                        break;
                    }
                    exprs.push(self.expression(false)?);
                    if !matches!(self.tok(), Tok::Template { head: false, .. }) {
                        return self.unexpected();
                    }
                }
                _ => {
                    self.i -= 1;
                    return self.unexpected();
                }
            }
        }
        Ok(Template { cooked, raw, exprs })
    }

    fn primary(&mut self) -> R<Expr> {
        let pos = self.pos();
        match self.tok().clone() {
            Tok::Num(n) => {
                self.advance();
                Ok(Expr::Num(n))
            }
            Tok::Str(s) => {
                self.advance();
                Ok(Expr::Str(s))
            }
            Tok::Regex(pattern, flags) => {
                self.advance();
                Ok(Expr::Regex(pattern, flags))
            }
            Tok::Template { head: true, .. } => {
                let template = self.template()?;
                if template.cooked.iter().any(Option::is_none) {
                    return self.error("Invalid escape sequence in template");
                }
                Ok(Expr::Template(Box::new(template)))
            }
            Tok::Punct("(") => {
                self.advance();
                let expr = self.expression(false)?;
                self.expect_punct(")")?;
                Ok(expr)
            }
            Tok::Punct("[") => self.array_literal(),
            Tok::Punct("{") => self.object_literal(),
            Tok::Ident(word) => match word.as_ref() {
                "this" => {
                    self.advance();
                    Ok(Expr::This)
                }
                "true" | "false" => {
                    self.advance();
                    Ok(Expr::Bool(word.as_ref() == "true"))
                }
                "null" => {
                    self.advance();
                    Ok(Expr::Null)
                }
                "function" => {
                    self.advance();
                    let is_generator = self.eat_punct("*");
                    let name = match self.tok() {
                        Tok::Ident(_) => Some(self.binding_identifier()?),
                        _ => None,
                    };
                    Ok(Expr::Function(self.function_rest(
                        name,
                        false,
                        is_generator,
                    )?))
                }
                "async" if self.is_word_at(1, "function") && !self.toks[self.i + 1].nl_before => {
                    self.advance();
                    self.advance();
                    let is_generator = self.eat_punct("*");
                    let name = match self.tok() {
                        Tok::Ident(_) => Some(self.binding_identifier()?),
                        _ => None,
                    };
                    Ok(Expr::Function(self.function_rest(
                        name,
                        true,
                        is_generator,
                    )?))
                }
                "class" => {
                    self.advance();
                    Ok(Expr::Class(self.class(false)?))
                }
                _ => {
                    let name = self.binding_identifier()?;
                    Ok(Expr::Ident(name, pos))
                }
            },
            Tok::Private(_) => self.unsupported("Private class members are not supported"),
            _ => self.unexpected(),
        }
    }

    fn array_literal(&mut self) -> R<Expr> {
        self.advance();
        let mut items = Vec::new();
        while !self.eat_punct("]") {
            if self.eat_punct(",") {
                items.push(ArrayElem::Hole);
                continue;
            }
            if self.eat_punct("...") {
                items.push(ArrayElem::Spread(self.assignment(false)?));
            } else {
                items.push(ArrayElem::Expr(self.assignment(false)?));
            }
            if !self.is_punct("]") {
                self.expect_punct(",")?;
            }
        }
        Ok(Expr::Array(items))
    }

    fn object_literal(&mut self) -> R<Expr> {
        self.advance();
        let mut props = Vec::new();
        while !self.eat_punct("}") {
            if self.eat_punct("...") {
                props.push(PropDef::Spread(self.assignment(false)?));
            } else {
                let plain_name = matches!(self.tok(), Tok::Ident(_))
                    && matches!(self.tok_at(1), Tok::Punct("," | "}" | "="));
                if plain_name {
                    let Tok::Ident(name) = self.advance() else {
                        unreachable!("checked identifier")
                    };
                    if RESERVED.contains(&name.as_ref()) {
                        self.i -= 1;
                        return self.unexpected();
                    }
                    if self.eat_punct("=") {
                        let default = self.assignment(false)?;
                        props.push(PropDef::CoverInit(name, Box::new(default)));
                    } else {
                        props.push(PropDef::Shorthand(name));
                    }
                } else {
                    let (kind, is_async, is_generator) = self.method_prefix();
                    let key = self.property_name()?;
                    if self.is_punct("(") {
                        let func = self.method(is_async, is_generator)?;
                        props.push(PropDef::Method(key, kind, func));
                    } else {
                        if !matches!(kind, MethodKind::Method) || is_async || is_generator {
                            return self.unexpected();
                        }
                        self.expect_punct(":")?;
                        let value = self.assignment(false)?;
                        props.push(PropDef::Value(key, value));
                    }
                }
            }
            if !self.is_punct("}") {
                self.expect_punct(",")?;
            }
        }
        Ok(Expr::Object(props))
    }
}

enum BinOrLogical {
    B(BinaryOp),
    L(LogicalOp),
}
