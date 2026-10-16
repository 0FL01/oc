//! Owned syntax tree for the confined Code Mode JavaScript subset.

use std::rc::Rc;

pub(super) type Str = super::value::JsStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VarKind {
    Var,
    Let,
    Const,
}

#[derive(Debug)]
pub(super) struct Program {
    pub body: Vec<Stmt>,
    pub var_names: Vec<Str>,
}

#[derive(Debug)]
pub(super) enum Stmt {
    Expr(Expr),
    Var(VarKind, Vec<Declarator>),
    Func(Rc<Function>),
    Class(Rc<Class>),
    Return(Option<Expr>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    Block(Vec<Stmt>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    For(Box<ForStmt>),
    ForIn(Box<ForEach>),
    ForOf(Box<ForEach>),
    Break(Option<Str>),
    Continue(Option<Str>),
    Throw(Expr, Pos),
    Try(Box<TryStmt>),
    Switch(Expr, Vec<Case>),
    Labeled(Str, Box<Stmt>),
    Empty,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Pos {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug)]
pub(super) struct Declarator {
    pub target: Pattern,
    pub init: Option<Expr>,
}

#[derive(Debug)]
pub(super) enum ForInit {
    Var(VarKind, Vec<Declarator>),
    Expr(Expr),
}

#[derive(Debug)]
pub(super) struct ForStmt {
    pub init: Option<ForInit>,
    pub test: Option<Expr>,
    pub update: Option<Expr>,
    pub body: Stmt,
}

#[derive(Debug)]
pub(super) enum ForTarget {
    Decl(VarKind, Pattern),
    Assign(Pattern),
}

#[derive(Debug)]
pub(super) struct ForEach {
    pub target: ForTarget,
    pub iterable: Expr,
    pub body: Stmt,
    pub is_await: bool,
}

#[derive(Debug)]
pub(super) struct TryStmt {
    pub block: Vec<Stmt>,
    pub param: Option<Pattern>,
    pub handler: Option<Vec<Stmt>>,
    pub finalizer: Option<Vec<Stmt>>,
}

#[derive(Debug)]
pub(super) struct Case {
    pub test: Option<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Debug)]
pub(super) enum Pattern {
    Ident(Str),
    Object(Vec<PatProp>, Option<Box<Pattern>>),
    Array(Vec<Option<Pattern>>, Option<Box<Pattern>>),
    Default(Box<Pattern>, Box<Expr>),
    /// Assignment-only member target such as `obj.x` in `[obj.x] = value`.
    Expr(Box<Expr>),
}

#[derive(Debug)]
pub(super) struct PatProp {
    pub key: PropName,
    pub value: Pattern,
}

#[derive(Debug)]
pub(super) enum PropName {
    Static(Str),
    Computed(Box<Expr>),
}

#[derive(Debug)]
pub(super) enum FuncBody {
    Block(Vec<Stmt>),
    Expr(Box<Expr>),
}

#[derive(Debug)]
pub(super) struct Function {
    pub name: Option<Str>,
    pub params: Vec<Pattern>,
    pub rest: Option<Pattern>,
    pub body: FuncBody,
    pub is_async: bool,
    pub is_arrow: bool,
    pub is_generator: bool,
    pub var_names: Vec<Str>,
}

#[derive(Debug)]
pub(super) enum MethodKind {
    Method,
    Get,
    Set,
}

#[derive(Debug)]
pub(super) struct ClassMember {
    pub key: PropName,
    pub is_static: bool,
    pub kind: MethodKind,
    pub func: Rc<Function>,
}

#[derive(Debug)]
pub(super) struct ClassField {
    pub key: PropName,
    pub is_static: bool,
    pub value: Option<Expr>,
}

#[derive(Debug)]
pub(super) struct Class {
    pub name: Option<Str>,
    pub extends: Option<Expr>,
    pub constructor: Option<Rc<Function>>,
    pub members: Vec<ClassMember>,
    pub fields: Vec<ClassField>,
}

#[derive(Debug)]
pub(super) struct Template {
    pub cooked: Vec<Option<Str>>,
    pub raw: Vec<Str>,
    pub exprs: Vec<Expr>,
}

#[derive(Debug)]
pub(super) enum ArrayElem {
    Expr(Expr),
    Spread(Expr),
    Hole,
}

#[derive(Debug)]
pub(super) enum Arg {
    Expr(Expr),
    Spread(Expr),
}

#[derive(Debug)]
pub(super) enum PropDef {
    Value(PropName, Expr),
    Shorthand(Str),
    /// `{ a = 1 }`: only valid once converted into an assignment pattern.
    CoverInit(Str, Box<Expr>),
    Method(PropName, MethodKind, Rc<Function>),
    Spread(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnaryOp {
    Not,
    Minus,
    Plus,
    BitNot,
    Typeof,
    Void,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Exp,
    Eq,
    NotEq,
    StrictEq,
    StrictNotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Shl,
    Shr,
    UShr,
    BitAnd,
    BitOr,
    BitXor,
    In,
    InstanceOf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LogicalOp {
    And,
    Or,
    Nullish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AssignOp {
    Assign,
    Op(BinaryOp),
    Logical(LogicalOp),
}

#[derive(Debug)]
pub(super) enum MemberKey {
    Name(Str),
    Computed(Box<Expr>),
}

#[derive(Debug)]
pub(super) enum Expr {
    Num(f64),
    Str(Str),
    Bool(bool),
    Null,
    Template(Box<Template>),
    Tagged(Box<Expr>, Rc<Template>),
    Regex(Str, Str),
    Ident(Str, Pos),
    This,
    Array(Vec<ArrayElem>),
    Object(Vec<PropDef>),
    Function(Rc<Function>),
    Class(Rc<Class>),
    Unary(UnaryOp, Box<Expr>),
    Update {
        increment: bool,
        prefix: bool,
        target: Box<Expr>,
    },
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Logical(LogicalOp, Box<Expr>, Box<Expr>),
    Assign(AssignOp, Box<Pattern>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Call {
        callee: Box<Expr>,
        args: Vec<Arg>,
        optional: bool,
        pos: Pos,
    },
    New(Box<Expr>, Vec<Arg>, Pos),
    Member {
        object: Box<Expr>,
        key: MemberKey,
        optional: bool,
    },
    SuperMember(MemberKey),
    SuperCall(Vec<Arg>),
    Chain(Box<Expr>),
    Seq(Vec<Expr>),
    Await(Box<Expr>),
    Yield(Option<Box<Expr>>, bool),
}

impl Expr {
    pub(super) fn is_anonymous_function(&self) -> bool {
        match self {
            Expr::Function(func) => func.name.is_none(),
            Expr::Class(class) => class.name.is_none(),
            _ => false,
        }
    }
}
