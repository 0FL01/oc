//! Runtime values, objects and primitive conversions of the Code Mode interpreter.

use std::cell::{OnceCell, Ref, RefCell, RefMut};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use indexmap::IndexMap;

use super::ast::{Class, Function};
use super::builtins::regexp::RegExpData;
use super::generator::GenState;
use super::interp::{Env, NativeFn};
use super::promise::PromiseCell;

/// Immutable JavaScript string. Text is stored as UTF-8; UTF-16 code units are materialized
/// lazily for non-ASCII strings so `length` and indexing follow JavaScript semantics.
#[derive(Clone)]
pub(super) struct JsStr(Rc<StrInner>);

struct StrInner {
    text: Box<str>,
    units: OnceCell<Box<[u16]>>,
}

impl JsStr {
    pub(super) fn as_str(&self) -> &str {
        &self.0.text
    }

    pub(super) fn is_ascii(&self) -> bool {
        self.0.text.is_ascii()
    }

    pub(super) fn units(&self) -> &[u16] {
        self.0
            .units
            .get_or_init(|| self.0.text.encode_utf16().collect())
    }

    pub(super) fn len16(&self) -> usize {
        if self.is_ascii() {
            self.0.text.len()
        } else {
            self.units().len()
        }
    }

    pub(super) fn unit_at(&self, index: usize) -> Option<u16> {
        if self.is_ascii() {
            self.0.text.as_bytes().get(index).map(|b| u16::from(*b))
        } else {
            self.units().get(index).copied()
        }
    }

    /// Substring by UTF-16 indices, clamped to the string.
    pub(super) fn slice16(&self, start: usize, end: usize) -> JsStr {
        let len = self.len16();
        let end = end.min(len);
        let start = start.min(end);
        if start == 0 && end == len {
            return self.clone();
        }
        if self.is_ascii() {
            return JsStr::from(&self.0.text[start..end]);
        }
        JsStr::from(String::from_utf16_lossy(&self.units()[start..end]))
    }

    pub(super) fn ptr_eq(&self, other: &JsStr) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl From<&str> for JsStr {
    fn from(value: &str) -> Self {
        JsStr(Rc::new(StrInner {
            text: value.into(),
            units: OnceCell::new(),
        }))
    }
}

impl From<String> for JsStr {
    fn from(value: String) -> Self {
        JsStr(Rc::new(StrInner {
            text: value.into_boxed_str(),
            units: OnceCell::new(),
        }))
    }
}

impl std::ops::Deref for JsStr {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0.text
    }
}

impl AsRef<str> for JsStr {
    fn as_ref(&self) -> &str {
        &self.0.text
    }
}

impl PartialEq for JsStr {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other) || self.0.text == other.0.text
    }
}

impl Eq for JsStr {}

impl Hash for JsStr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.text.hash(state);
    }
}

impl PartialOrd for JsStr {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JsStr {
    /// JavaScript compares strings by UTF-16 code units.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.is_ascii() && other.is_ascii() {
            return self.0.text.cmp(&other.0.text);
        }
        self.units().cmp(other.units())
    }
}

impl fmt::Debug for JsStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0.text, f)
    }
}

impl fmt::Display for JsStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.text)
    }
}

pub(super) struct Symbol {
    pub description: Option<JsStr>,
}

#[derive(Clone)]
pub(super) struct Sym(pub Rc<Symbol>);

impl PartialEq for Sym {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Sym {}

impl Hash for Sym {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::ptr::hash(Rc::as_ptr(&self.0), state);
    }
}

#[derive(Clone)]
pub(super) enum Value {
    Undefined,
    Null,
    Bool(bool),
    Num(f64),
    Str(JsStr),
    Sym(Sym),
    Obj(Obj),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Undefined => f.write_str("undefined"),
            Value::Null => f.write_str("null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Num(n) => f.write_str(&number_to_string(*n)),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Sym(_) => f.write_str("Symbol()"),
            Value::Obj(_) => f.write_str("[object]"),
        }
    }
}

impl Value {
    pub(super) fn str(text: impl Into<JsStr>) -> Value {
        Value::Str(text.into())
    }

    pub(super) fn is_nullish(&self) -> bool {
        matches!(self, Value::Undefined | Value::Null)
    }

    pub(super) fn as_obj(&self) -> Option<&Obj> {
        match self {
            Value::Obj(obj) => Some(obj),
            _ => None,
        }
    }

    pub(super) fn is_callable(&self) -> bool {
        self.as_obj().is_some_and(Obj::is_callable)
    }

    pub(super) fn truthy(&self) -> bool {
        match self {
            Value::Undefined | Value::Null => false,
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0 && !n.is_nan(),
            Value::Str(s) => !s.is_empty(),
            Value::Sym(_) | Value::Obj(_) => true,
        }
    }

    pub(super) fn type_of(&self) -> &'static str {
        match self {
            Value::Undefined => "undefined",
            Value::Null => "object",
            Value::Bool(_) => "boolean",
            Value::Num(_) => "number",
            Value::Str(_) => "string",
            Value::Sym(_) => "symbol",
            Value::Obj(obj) if obj.is_callable() => "function",
            Value::Obj(_) => "object",
        }
    }

    pub(super) fn strict_equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Sym(a), Value::Sym(b)) => a == b,
            (Value::Obj(a), Value::Obj(b)) => a.ptr_eq(b),
            _ => false,
        }
    }

    pub(super) fn same_value_zero(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Num(a), Value::Num(b)) if a.is_nan() && b.is_nan() => true,
            _ => self.strict_equals(other),
        }
    }

    pub(super) fn same_value(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Num(a), Value::Num(b)) => {
                (a.is_nan() && b.is_nan())
                    || (a == b && a.is_sign_negative() == b.is_sign_negative())
            }
            _ => self.strict_equals(other),
        }
    }
}

/// Key for Map/Set entries with SameValueZero semantics.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) enum MapKey {
    Undefined,
    Null,
    Bool(bool),
    Num(u64),
    Str(JsStr),
    Sym(Sym),
    Obj(usize),
}

impl MapKey {
    pub(super) fn of(value: &Value) -> MapKey {
        match value {
            Value::Undefined => MapKey::Undefined,
            Value::Null => MapKey::Null,
            Value::Bool(b) => MapKey::Bool(*b),
            Value::Num(n) => {
                let n = if *n == 0.0 { 0.0 } else { *n };
                MapKey::Num(if n.is_nan() {
                    f64::NAN.to_bits()
                } else {
                    n.to_bits()
                })
            }
            Value::Str(s) => MapKey::Str(s.clone()),
            Value::Sym(s) => MapKey::Sym(s.clone()),
            Value::Obj(o) => MapKey::Obj(o.id()),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) enum PropKey {
    Str(JsStr),
    Sym(Sym),
}

impl PropKey {
    pub(super) fn as_str(&self) -> Option<&JsStr> {
        match self {
            PropKey::Str(s) => Some(s),
            PropKey::Sym(_) => None,
        }
    }

    pub(super) fn to_value(&self) -> Value {
        match self {
            PropKey::Str(s) => Value::Str(s.clone()),
            PropKey::Sym(s) => Value::Sym(s.clone()),
        }
    }
}

impl From<&str> for PropKey {
    fn from(value: &str) -> Self {
        PropKey::Str(value.into())
    }
}

impl From<JsStr> for PropKey {
    fn from(value: JsStr) -> Self {
        PropKey::Str(value)
    }
}

#[derive(Clone)]
pub(super) enum Slot {
    Data(Value),
    Accessor { get: Option<Obj>, set: Option<Obj> },
}

#[derive(Clone)]
pub(super) struct Prop {
    pub slot: Slot,
    pub enumerable: bool,
    pub writable: bool,
    pub configurable: bool,
}

impl Prop {
    pub(super) fn data(value: Value) -> Prop {
        Prop {
            slot: Slot::Data(value),
            enumerable: true,
            writable: true,
            configurable: true,
        }
    }

    pub(super) fn hidden(value: Value) -> Prop {
        Prop {
            slot: Slot::Data(value),
            enumerable: false,
            writable: true,
            configurable: true,
        }
    }
}

pub(super) struct ClassInfo {
    pub derived: bool,
    pub def: Rc<Class>,
    pub field_keys: Vec<PropKey>,
    pub env: Env,
}

pub(super) struct UserFn {
    pub def: Rc<Function>,
    pub env: Env,
    pub home: RefCell<Option<Obj>>,
    pub class: Option<Rc<ClassInfo>>,
    pub is_class_constructor: bool,
}

pub(super) struct BoundFn {
    pub target: Obj,
    pub this: Value,
    pub args: Vec<Value>,
}

pub(super) enum Func {
    User(Rc<UserFn>),
    Native(Rc<NativeFn>),
    Bound(Rc<BoundFn>),
    /// A path in the host `tools` tree; calling it performs a host tool call.
    Tool(Rc<[JsStr]>),
}

impl Clone for Func {
    fn clone(&self) -> Self {
        match self {
            Func::User(f) => Func::User(f.clone()),
            Func::Native(f) => Func::Native(f.clone()),
            Func::Bound(f) => Func::Bound(f.clone()),
            Func::Tool(p) => Func::Tool(p.clone()),
        }
    }
}

pub(super) enum IterKind {
    /// Iterates over a live array object.
    Array { array: Obj, mode: IterMode },
    /// A snapshot of values (Map/Set/String/URLSearchParams iteration).
    List(Vec<Value>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum IterMode {
    Keys,
    Values,
    Entries,
}

pub(super) struct IterState {
    pub kind: IterKind,
    pub index: usize,
    pub done: bool,
}

pub(super) struct UrlData {
    pub href: String,
    pub search_params: Option<Obj>,
}

pub(super) enum Kind {
    Plain,
    Array(Vec<Value>),
    Function(Func),
    Error,
    Date(f64),
    RegExp(Rc<RegExpData>),
    Map(IndexMap<MapKey, (Value, Value)>),
    Set(IndexMap<MapKey, Value>),
    Promise(Rc<PromiseCell>),
    Bytes(Vec<u8>),
    Iter(IterState),
    Generator(Rc<GenState>),
    Url(UrlData),
    SearchParams {
        list: Vec<(String, String)>,
        url: Option<Obj>,
    },
    Headers(Vec<(String, String)>),
    Boxed(Value),
}

pub(super) struct ObjData {
    pub proto: Option<Obj>,
    pub props: IndexMap<PropKey, Prop>,
    pub kind: Kind,
    pub extensible: bool,
    pub frozen: bool,
}

#[derive(Clone)]
pub(super) struct Obj(Rc<RefCell<ObjData>>);

impl Obj {
    pub(super) fn new(proto: Option<Obj>, kind: Kind) -> Obj {
        let obj = Rc::new(RefCell::new(ObjData {
            proto,
            props: IndexMap::new(),
            kind,
            extensible: true,
            frozen: false,
        }));
        super::interp::register_object(Rc::downgrade(&obj));
        Obj(obj)
    }

    pub(super) fn borrow(&self) -> Ref<'_, ObjData> {
        self.0.borrow()
    }

    pub(super) fn borrow_mut(&self) -> RefMut<'_, ObjData> {
        self.0.borrow_mut()
    }

    pub(super) fn ptr_eq(&self, other: &Obj) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub(super) fn id(&self) -> usize {
        Rc::as_ptr(&self.0) as *const () as usize
    }

    pub(super) fn is_callable(&self) -> bool {
        matches!(self.borrow().kind, Kind::Function(_))
    }

    pub(super) fn is_array(&self) -> bool {
        matches!(self.borrow().kind, Kind::Array(_))
    }

    pub(super) fn func(&self) -> Option<Func> {
        match &self.borrow().kind {
            Kind::Function(f) => Some(f.clone()),
            _ => None,
        }
    }

    pub(super) fn proto(&self) -> Option<Obj> {
        self.borrow().proto.clone()
    }

    /// Own data property lookup without walking the prototype chain or calling getters.
    pub(super) fn own_data(&self, key: &str) -> Option<Value> {
        match self
            .borrow()
            .props
            .get(&PropKey::from(key))
            .map(|p| &p.slot)
        {
            Some(Slot::Data(value)) => Some(value.clone()),
            _ => None,
        }
    }

    pub(super) fn set_hidden(&self, key: &str, value: Value) {
        self.borrow_mut()
            .props
            .insert(key.into(), Prop::hidden(value));
    }

    pub(super) fn set_data(&self, key: impl Into<PropKey>, value: Value) {
        self.borrow_mut()
            .props
            .insert(key.into(), Prop::data(value));
    }

    pub(super) fn array_items(&self) -> Option<Vec<Value>> {
        match &self.borrow().kind {
            Kind::Array(items) => Some(items.clone()),
            _ => None,
        }
    }

    pub(super) fn is_error(&self) -> bool {
        matches!(self.borrow().kind, Kind::Error)
    }
}

/// Canonical array index for a property name.
pub(super) fn array_index(key: &str) -> Option<usize> {
    let bytes = key.as_bytes();
    if bytes.is_empty() || bytes.len() > 10 || (bytes.len() > 1 && bytes[0] == b'0') {
        return None;
    }
    if !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let value: u64 = key.parse().ok()?;
    (value < u64::from(u32::MAX)).then_some(value as usize)
}

/// JavaScript `Number.prototype.toString()` for radix 10.
pub(super) fn number_to_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n == 0.0 {
        return "0".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if n < 0.0 {
        return format!("-{}", number_to_string(-n));
    }
    // Shortest round-trip digits and decimal exponent.
    let sci = format!("{n:e}");
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let k = digits.len() as i32;
    let point = exp + 1;
    if k <= point && point <= 21 {
        let mut out = digits;
        out.extend(std::iter::repeat_n('0', (point - k) as usize));
        out
    } else if 0 < point && point <= 21 {
        let (int, frac) = digits.split_at(point as usize);
        format!("{int}.{frac}")
    } else if -6 < point && point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else {
        let sign = if point - 1 < 0 { '-' } else { '+' };
        let e = (point - 1).abs();
        if k == 1 {
            format!("{digits}e{sign}{e}")
        } else {
            format!("{}.{}e{sign}{e}", &digits[..1], &digits[1..])
        }
    }
}

pub(super) fn number_to_radix(n: f64, radix: u32) -> String {
    if radix == 10 || !n.is_finite() {
        return number_to_string(n);
    }
    let negative = n < 0.0;
    let mut int = n.abs().trunc();
    let mut frac = n.abs() - int;
    let mut digits = Vec::new();
    if int == 0.0 {
        digits.push('0');
    }
    while int >= 1.0 {
        let d = (int % f64::from(radix)) as u32;
        digits.push(std::char::from_digit(d, radix).unwrap_or('0'));
        int = (int / f64::from(radix)).trunc();
    }
    digits.reverse();
    let mut out: String = digits.into_iter().collect();
    if frac > 0.0 {
        out.push('.');
        for _ in 0..52 {
            frac *= f64::from(radix);
            let d = frac.trunc() as u32;
            out.push(std::char::from_digit(d, radix).unwrap_or('0'));
            frac -= f64::from(d);
            if frac == 0.0 {
                break;
            }
        }
    }
    if negative { format!("-{out}") } else { out }
}

/// JavaScript `StringToNumber`.
pub(super) fn string_to_number(text: &str) -> f64 {
    let s = text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if s.is_empty() {
        return 0.0;
    }
    let radix = |prefix: &[&str], radix: u32| -> Option<f64> {
        for p in prefix {
            if let Some(rest) = s.strip_prefix(p) {
                if rest.is_empty() || !rest.chars().all(|c| c.is_digit(radix)) {
                    return Some(f64::NAN);
                }
                return Some(rest.chars().fold(0.0, |acc, d| {
                    acc * f64::from(radix) + f64::from(d.to_digit(radix).unwrap_or(0))
                }));
            }
        }
        None
    };
    if let Some(n) = radix(&["0x", "0X"], 16)
        .or_else(|| radix(&["0o", "0O"], 8))
        .or_else(|| radix(&["0b", "0B"], 2))
    {
        return n;
    }
    match s {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    let valid = s
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        && s.chars().any(|c| c.is_ascii_digit());
    if !valid {
        return f64::NAN;
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}

pub(super) fn to_int32(n: f64) -> i32 {
    to_uint32(n) as i32
}

pub(super) fn to_uint32(n: f64) -> u32 {
    if !n.is_finite() || n == 0.0 {
        return 0;
    }
    let int = n.trunc();
    let modulo = int.rem_euclid(4_294_967_296.0);
    modulo as u32
}

/// `ToIntegerOrInfinity`.
pub(super) fn to_integer(n: f64) -> f64 {
    if n.is_nan() { 0.0 } else { n.trunc() + 0.0 }
}

/// Resolves a relative index argument (as used by `slice`) against a length.
pub(super) fn relative_index(n: f64, len: usize) -> usize {
    let len_f = len as f64;
    let n = to_integer(n);
    if n < 0.0 {
        (len_f + n).max(0.0) as usize
    } else {
        n.min(len_f) as usize
    }
}
