//! Async tree-walking evaluator for the Code Mode JavaScript subset.
//!
//! Every JavaScript evaluation step is an async Rust function so tool calls and promise waits
//! suspend naturally. The interpreter runs on its own thread with a single-threaded executor;
//! values are `Rc`-based and never cross threads. A step budget and a cancellation flag are
//! checked between steps, so a runaway loop cannot outlive its turn.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::future::LocalBoxFuture;
use indexmap::IndexMap;

use super::ast::*;
use super::generator::{self, GenSlot};
use super::host::Host;
use super::promise::{self, PromiseCell};
use super::value::*;

pub(super) type R<T> = Result<T, Abrupt>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fatal {
    StepLimit,
    Cancelled,
}

pub(super) enum Abrupt {
    Throw(Value),
    /// Optional-chain short circuit; never escapes an `Expr::Chain`.
    Short,
    /// Generator `return()` unwinding through the generator body.
    GenReturn(Value),
    Fatal(Fatal),
}

pub(super) enum Completion {
    Normal,
    Return(Value),
    Break(Option<JsStr>),
    Continue(Option<JsStr>),
}

pub(super) type SyncImpl = Box<dyn Fn(&Cx, &Value, &[Value]) -> R<Value>>;
pub(super) type AsyncImpl = Box<dyn Fn(Cx, Value, Vec<Value>) -> LocalBoxFuture<'static, R<Value>>>;

pub(super) enum NativeImpl {
    Sync(SyncImpl),
    Async(AsyncImpl),
}

pub(super) struct NativeFn {
    pub name: JsStr,
    pub call: Option<NativeImpl>,
    /// Called with the `new.target` constructor as `this`.
    pub construct: Option<NativeImpl>,
}

#[derive(Clone)]
struct Binding {
    value: Value,
    mutable: bool,
    init: bool,
}

pub(super) struct FnCtx {
    this: RefCell<Option<Value>>,
    home: Option<Obj>,
    new_target: Option<Value>,
    func: Option<Obj>,
    args: RefCell<Vec<Value>>,
    pub(super) gen_slot: Option<Rc<GenSlot>>,
}

pub(super) struct Scope {
    vars: RefCell<Vec<(JsStr, Binding)>>,
    parent: Option<Env>,
    ctx: Option<Rc<FnCtx>>,
}

pub(super) type Env = Rc<Scope>;

// ----- heap registry -------------------------------------------------------------------
//
// Values are reference counted. Closures and scopes form cycles, so every object and scope is
// registered and explicitly emptied after the program finishes. Emptying all containers first
// also keeps teardown iterative for long linked structures.

#[derive(Default)]
struct Heap {
    objects: Vec<Weak<RefCell<ObjData>>>,
    scopes: Vec<Weak<Scope>>,
    object_mark: usize,
    scope_mark: usize,
}

thread_local! {
    static HEAP: RefCell<Heap> = RefCell::new(Heap::default());
}

pub(super) fn register_object(weak: Weak<RefCell<ObjData>>) {
    HEAP.with(|heap| {
        let mut heap = heap.borrow_mut();
        heap.objects.push(weak);
        if heap.objects.len() >= heap.object_mark.max(4096) * 2 {
            heap.objects.retain(|w| w.strong_count() > 0);
            heap.object_mark = heap.objects.len();
        }
    });
}

fn register_scope(weak: Weak<Scope>) {
    HEAP.with(|heap| {
        let mut heap = heap.borrow_mut();
        heap.scopes.push(weak);
        if heap.scopes.len() >= heap.scope_mark.max(4096) * 2 {
            heap.scopes.retain(|w| w.strong_count() > 0);
            heap.scope_mark = heap.scopes.len();
        }
    });
}

/// Breaks every reference cycle created by the program on this thread.
pub(super) fn teardown_heap() {
    let (objects, scopes) = HEAP.with(|heap| {
        let mut heap = heap.borrow_mut();
        (
            std::mem::take(&mut heap.objects),
            std::mem::take(&mut heap.scopes),
        )
    });
    let mut garbage: Vec<Box<dyn std::any::Any>> = Vec::new();
    let mut live_objects = Vec::new();
    for weak in objects {
        if let Some(obj) = weak.upgrade() {
            if let Ok(mut data) = obj.try_borrow_mut() {
                garbage.push(Box::new(std::mem::take(&mut data.props)));
                garbage.push(Box::new(std::mem::replace(&mut data.kind, Kind::Plain)));
                garbage.push(Box::new(data.proto.take()));
            }
            live_objects.push(obj);
        }
    }
    let mut live_scopes = Vec::new();
    for weak in scopes {
        if let Some(scope) = weak.upgrade() {
            if let Ok(mut vars) = scope.vars.try_borrow_mut() {
                garbage.push(Box::new(std::mem::take(&mut *vars)));
            }
            if let Some(ctx) = &scope.ctx {
                if let Ok(mut this) = ctx.this.try_borrow_mut() {
                    garbage.push(Box::new(this.take()));
                }
                if let Ok(mut args) = ctx.args.try_borrow_mut() {
                    garbage.push(Box::new(std::mem::take(&mut *args)));
                }
            }
            live_scopes.push(scope);
        }
    }
    drop(garbage);
    drop(live_objects);
    drop(live_scopes);
}

impl Scope {
    pub(super) fn new(parent: Option<Env>, ctx: Option<Rc<FnCtx>>) -> Env {
        let scope = Rc::new(Scope {
            vars: RefCell::new(Vec::new()),
            parent,
            ctx,
        });
        register_scope(Rc::downgrade(&scope));
        scope
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.vars
            .borrow()
            .iter()
            .position(|(n, _)| n.as_str() == name)
    }

    pub(super) fn declare(&self, name: &JsStr, value: Value, mutable: bool) {
        let mut vars = self.vars.borrow_mut();
        if let Some(slot) = vars.iter_mut().find(|(n, _)| n == name) {
            slot.1 = Binding {
                value,
                mutable,
                init: true,
            };
        } else {
            vars.push((
                name.clone(),
                Binding {
                    value,
                    mutable,
                    init: true,
                },
            ));
        }
    }

    fn declare_uninit(&self, name: &JsStr, mutable: bool) {
        let mut vars = self.vars.borrow_mut();
        let binding = Binding {
            value: Value::Undefined,
            mutable,
            init: false,
        };
        if let Some(slot) = vars.iter_mut().find(|(n, _)| n == name) {
            slot.1 = binding;
        } else {
            vars.push((name.clone(), binding));
        }
    }

    fn declare_var(&self, name: &JsStr) {
        if self.find(name).is_none() {
            self.vars.borrow_mut().push((
                name.clone(),
                Binding {
                    value: Value::Undefined,
                    mutable: true,
                    init: true,
                },
            ));
        }
    }

    fn fn_ctx(self: &Rc<Self>) -> Option<Rc<FnCtx>> {
        let mut scope = Some(self.clone());
        while let Some(current) = scope {
            if let Some(ctx) = &current.ctx {
                return Some(ctx.clone());
            }
            scope = current.parent.clone();
        }
        None
    }
}

// ----- realm ---------------------------------------------------------------------------

pub(super) struct Protos {
    pub object: Obj,
    pub function: Obj,
    pub array: Obj,
    pub error: Obj,
    pub type_error: Obj,
    pub range_error: Obj,
    pub syntax_error: Obj,
    pub reference_error: Obj,
    pub uri_error: Obj,
    pub aggregate_error: Obj,
    pub promise: Obj,
    pub map: Obj,
    pub set: Obj,
    pub weak_map: Obj,
    pub weak_set: Obj,
    pub date: Obj,
    pub regexp: Obj,
    pub string: Obj,
    pub number: Obj,
    pub boolean: Obj,
    pub symbol: Obj,
    pub iterator: Obj,
    pub generator: Obj,
    pub async_generator: Obj,
    pub bytes: Obj,
    pub url: Obj,
    pub search_params: Obj,
    pub headers: Obj,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorKind {
    Error,
    Type,
    Range,
    Syntax,
    Reference,
    Uri,
    Aggregate,
}

impl ErrorKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            ErrorKind::Error => "Error",
            ErrorKind::Type => "TypeError",
            ErrorKind::Range => "RangeError",
            ErrorKind::Syntax => "SyntaxError",
            ErrorKind::Reference => "ReferenceError",
            ErrorKind::Uri => "URIError",
            ErrorKind::Aggregate => "AggregateError",
        }
    }
}

pub(super) struct Limits {
    pub max_steps: u64,
    /// Native stack the evaluator may use; deep recursion becomes a `RangeError`.
    pub max_stack_bytes: usize,
    pub max_string_bytes: usize,
    pub max_array_len: usize,
    pub max_pending_promises: usize,
    pub max_log_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_steps: 50_000_000,
            max_stack_bytes: 192 << 20,
            max_string_bytes: 1 << 24,
            max_array_len: 10_000_000,
            max_pending_promises: 10_000,
            max_log_bytes: 1 << 20,
        }
    }
}

#[derive(Default)]
pub(super) struct Logs {
    pub lines: Vec<String>,
    pub bytes: usize,
    pub dropped: usize,
}

pub(super) struct Realm {
    pub protos: Protos,
    pub builtins: Env,
    pub sym_iterator: Sym,
    pub sym_async_iterator: Sym,
    pub sym_has_instance: Sym,
    pub sym_to_string_tag: Sym,
    /// Hidden key carrying diagnostic metadata on host-originated errors.
    pub sym_diagnostic: Sym,
    pub limits: Limits,
    pub steps: Cell<u64>,
    pub cancel: Arc<AtomicBool>,
    pub logs: RefCell<Logs>,
    pub host: Host,
    pub pending_promises: Cell<usize>,
    pub rejected: RefCell<Vec<Rc<PromiseCell>>>,
    pub symbol_registry: RefCell<IndexMap<JsStr, Sym>>,
}

#[derive(Clone)]
pub(super) struct Cx(Rc<Realm>);

impl std::ops::Deref for Cx {
    type Target = Realm;
    fn deref(&self) -> &Realm {
        &self.0
    }
}

fn new_sym(description: &str) -> Sym {
    Sym(Rc::new(Symbol {
        description: Some(description.into()),
    }))
}

thread_local! {
    /// Address near the base of the interpreter thread's stack, set by the runner.
    pub(super) static STACK_BASE: Cell<usize> = const { Cell::new(0) };
}

/// Marks the current stack position as the base for [`Cx::stack_check`].
#[inline(never)]
pub(super) fn mark_stack_base() {
    let marker = 0u8;
    STACK_BASE.with(|base| base.set(std::ptr::addr_of!(marker) as usize));
}

pub(super) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or(Value::Undefined)
}

impl Cx {
    pub(super) fn new(limits: Limits, cancel: Arc<AtomicBool>, host: Host) -> Cx {
        let object = Obj::new(None, Kind::Plain);
        let proto = |kind: Kind| Obj::new(Some(object.clone()), kind);
        let function = proto(Kind::Plain);
        let error = proto(Kind::Plain);
        let error_proto = || Obj::new(Some(error.clone()), Kind::Plain);
        let iterator = proto(Kind::Plain);
        let protos = Protos {
            function,
            array: proto(Kind::Array(Vec::new())),
            type_error: error_proto(),
            range_error: error_proto(),
            syntax_error: error_proto(),
            reference_error: error_proto(),
            uri_error: error_proto(),
            aggregate_error: error_proto(),
            error,
            promise: proto(Kind::Plain),
            map: proto(Kind::Plain),
            set: proto(Kind::Plain),
            weak_map: proto(Kind::Plain),
            weak_set: proto(Kind::Plain),
            date: proto(Kind::Plain),
            regexp: proto(Kind::Plain),
            string: proto(Kind::Plain),
            number: proto(Kind::Plain),
            boolean: proto(Kind::Plain),
            symbol: proto(Kind::Plain),
            generator: Obj::new(Some(iterator.clone()), Kind::Plain),
            async_generator: proto(Kind::Plain),
            iterator,
            bytes: proto(Kind::Plain),
            url: proto(Kind::Plain),
            search_params: proto(Kind::Plain),
            headers: proto(Kind::Plain),
            object,
        };
        let cx = Cx(Rc::new(Realm {
            protos,
            builtins: Scope::new(None, None),
            sym_iterator: new_sym("Symbol.iterator"),
            sym_async_iterator: new_sym("Symbol.asyncIterator"),
            sym_has_instance: new_sym("Symbol.hasInstance"),
            sym_to_string_tag: new_sym("Symbol.toStringTag"),
            sym_diagnostic: new_sym("diagnostic"),
            limits,
            steps: Cell::new(0),
            cancel,
            logs: RefCell::new(Logs::default()),
            host,
            pending_promises: Cell::new(0),
            rejected: RefCell::new(Vec::new()),
            symbol_registry: RefCell::new(IndexMap::new()),
        }));
        super::builtins::install(&cx);
        cx
    }

    pub(super) fn builtins_lookup(&self, name: &str) -> Option<Value> {
        self.builtins
            .vars
            .borrow()
            .iter()
            .find(|(n, _)| n.as_str() == name)
            .map(|(_, b)| b.value.clone())
    }

    pub(super) fn define_global(&self, name: &str, value: Value) {
        self.builtins.declare(&name.into(), value, true);
    }

    #[inline]
    pub(super) fn tick(&self) -> R<()> {
        let steps = self.steps.get() + 1;
        self.steps.set(steps);
        if steps > self.limits.max_steps {
            return Err(Abrupt::Fatal(Fatal::StepLimit));
        }
        if steps & 0x3ff == 0 && self.cancel.load(Ordering::Relaxed) {
            return Err(Abrupt::Fatal(Fatal::Cancelled));
        }
        Ok(())
    }

    /// Fails with a `RangeError` when evaluation nests too deeply for the native stack.
    #[inline(never)]
    pub(super) fn stack_check(&self) -> R<()> {
        let marker = 0u8;
        let here = std::ptr::addr_of!(marker) as usize;
        let base = STACK_BASE.with(Cell::get);
        if base != 0 && base.saturating_sub(here) > self.limits.max_stack_bytes {
            return Err(self.range_error("Maximum call stack size exceeded"));
        }
        Ok(())
    }

    // ----- errors ----------------------------------------------------------------------

    pub(super) fn error_proto(&self, kind: ErrorKind) -> Obj {
        let p = &self.protos;
        match kind {
            ErrorKind::Error => p.error.clone(),
            ErrorKind::Type => p.type_error.clone(),
            ErrorKind::Range => p.range_error.clone(),
            ErrorKind::Syntax => p.syntax_error.clone(),
            ErrorKind::Reference => p.reference_error.clone(),
            ErrorKind::Uri => p.uri_error.clone(),
            ErrorKind::Aggregate => p.aggregate_error.clone(),
        }
    }

    pub(super) fn make_error(&self, kind: ErrorKind, message: impl Into<String>) -> Obj {
        let obj = Obj::new(Some(self.error_proto(kind)), Kind::Error);
        let message: String = message.into();
        obj.set_hidden("stack", Value::str(format!("{}: {message}", kind.name())));
        obj.set_hidden("message", Value::str(message));
        obj
    }

    pub(super) fn throw(&self, kind: ErrorKind, message: impl Into<String>) -> Abrupt {
        Abrupt::Throw(Value::Obj(self.make_error(kind, message)))
    }

    pub(super) fn type_error(&self, message: impl Into<String>) -> Abrupt {
        self.throw(ErrorKind::Type, message)
    }

    pub(super) fn range_error(&self, message: impl Into<String>) -> Abrupt {
        self.throw(ErrorKind::Range, message)
    }

    // ----- constructors for common values ----------------------------------------------

    pub(super) fn new_object(&self) -> Obj {
        Obj::new(Some(self.protos.object.clone()), Kind::Plain)
    }

    pub(super) fn new_array(&self, items: Vec<Value>) -> Obj {
        Obj::new(Some(self.protos.array.clone()), Kind::Array(items))
    }

    pub(super) fn array_value(&self, items: Vec<Value>) -> Value {
        Value::Obj(self.new_array(items))
    }

    pub(super) fn check_array_len(&self, len: usize) -> R<()> {
        if len > self.limits.max_array_len {
            return Err(self.range_error(format!(
                "Array length {len} exceeds the Code Mode limit of {}",
                self.limits.max_array_len
            )));
        }
        Ok(())
    }

    pub(super) fn check_string_len(&self, bytes: usize) -> R<()> {
        if bytes > self.limits.max_string_bytes {
            return Err(self.range_error(format!(
                "Invalid string length: {bytes} bytes exceeds the Code Mode limit of {}",
                self.limits.max_string_bytes
            )));
        }
        Ok(())
    }

    pub(super) fn string(&self, text: String) -> R<Value> {
        self.check_string_len(text.len())?;
        Ok(Value::str(text))
    }

    fn function_object(&self, func: Func, name: &str, length: usize) -> Obj {
        let obj = Obj::new(Some(self.protos.function.clone()), Kind::Function(func));
        {
            let mut data = obj.borrow_mut();
            data.props.insert(
                "length".into(),
                Prop {
                    slot: Slot::Data(Value::Num(length as f64)),
                    enumerable: false,
                    writable: false,
                    configurable: true,
                },
            );
            data.props.insert(
                "name".into(),
                Prop {
                    slot: Slot::Data(Value::str(name)),
                    enumerable: false,
                    writable: false,
                    configurable: true,
                },
            );
        }
        obj
    }

    pub(super) fn native_obj(
        &self,
        name: &str,
        length: usize,
        call: Option<NativeImpl>,
        construct: Option<NativeImpl>,
    ) -> Obj {
        let native = NativeFn {
            name: name.into(),
            call,
            construct,
        };
        self.function_object(Func::Native(Rc::new(native)), name, length)
    }

    pub(super) fn native(
        &self,
        name: &str,
        length: usize,
        f: impl Fn(&Cx, &Value, &[Value]) -> R<Value> + 'static,
    ) -> Obj {
        self.native_obj(name, length, Some(NativeImpl::Sync(Box::new(f))), None)
    }

    pub(super) fn native_async(
        &self,
        name: &str,
        length: usize,
        f: impl Fn(Cx, Value, Vec<Value>) -> LocalBoxFuture<'static, R<Value>> + 'static,
    ) -> Obj {
        self.native_obj(name, length, Some(NativeImpl::Async(Box::new(f))), None)
    }

    pub(super) fn method(
        &self,
        target: &Obj,
        name: &str,
        length: usize,
        f: impl Fn(&Cx, &Value, &[Value]) -> R<Value> + 'static,
    ) {
        let func = self.native(name, length, f);
        target.set_hidden(name, Value::Obj(func));
    }

    pub(super) fn method_async(
        &self,
        target: &Obj,
        name: &str,
        length: usize,
        f: impl Fn(Cx, Value, Vec<Value>) -> LocalBoxFuture<'static, R<Value>> + 'static,
    ) {
        let func = self.native_async(name, length, f);
        target.set_hidden(name, Value::Obj(func));
    }

    pub(super) fn method_sym(
        &self,
        target: &Obj,
        sym: &Sym,
        name: &str,
        f: impl Fn(&Cx, &Value, &[Value]) -> R<Value> + 'static,
    ) {
        let func = self.native(name, 0, f);
        target
            .borrow_mut()
            .props
            .insert(PropKey::Sym(sym.clone()), Prop::hidden(Value::Obj(func)));
    }

    pub(super) fn getter(
        &self,
        target: &Obj,
        name: &str,
        f: impl Fn(&Cx, &Value, &[Value]) -> R<Value> + 'static,
    ) {
        let func = self.native(&format!("get {name}"), 0, f);
        target.borrow_mut().props.insert(
            name.into(),
            Prop {
                slot: Slot::Accessor {
                    get: Some(func),
                    set: None,
                },
                enumerable: false,
                writable: false,
                configurable: true,
            },
        );
    }

    /// Creates a constructor function object with its `prototype` link.
    pub(super) fn constructor(
        &self,
        name: &str,
        length: usize,
        proto: &Obj,
        call: Option<NativeImpl>,
        construct: Option<NativeImpl>,
    ) -> Obj {
        let ctor = self.native_obj(name, length, call, construct);
        ctor.borrow_mut().props.insert(
            "prototype".into(),
            Prop {
                slot: Slot::Data(Value::Obj(proto.clone())),
                enumerable: false,
                writable: false,
                configurable: false,
            },
        );
        proto.set_hidden("constructor", Value::Obj(ctor.clone()));
        ctor
    }

    pub(super) fn tool_ref(&self, path: Rc<[JsStr]>) -> Value {
        let name = path
            .last()
            .map(|s| s.as_str().to_string())
            .unwrap_or_default();
        Value::Obj(self.function_object(Func::Tool(path), &name, 1))
    }

    /// Prototype from `new_target.prototype`, falling back to `fallback`.
    pub(super) fn proto_from(&self, new_target: &Value, fallback: &Obj) -> Obj {
        if let Value::Obj(target) = new_target
            && let Some(Value::Obj(proto)) = target.own_data("prototype")
        {
            return proto;
        }
        fallback.clone()
    }

    // ----- conversions -----------------------------------------------------------------

    /// Synchronous `ToString` for primitives and built-in objects. User-defined `toString`
    /// methods are honoured only by the async evaluator paths.
    pub(super) fn to_str(&self, value: &Value) -> R<JsStr> {
        Ok(match value {
            Value::Undefined => "undefined".into(),
            Value::Null => "null".into(),
            Value::Bool(b) => if *b { "true" } else { "false" }.into(),
            Value::Num(n) => number_to_string(*n).into(),
            Value::Str(s) => s.clone(),
            Value::Sym(_) => {
                return Err(self.type_error("Cannot convert a Symbol value to a string"));
            }
            Value::Obj(obj) => super::builtins::object_to_string_builtin(self, obj)?,
        })
    }

    pub(super) fn to_num(&self, value: &Value) -> R<f64> {
        Ok(match value {
            Value::Undefined => f64::NAN,
            Value::Null => 0.0,
            Value::Bool(b) => f64::from(u8::from(*b)),
            Value::Num(n) => *n,
            Value::Str(s) => string_to_number(s),
            Value::Sym(_) => {
                return Err(self.type_error("Cannot convert a Symbol value to a number"));
            }
            Value::Obj(obj) => {
                let prim = super::builtins::object_to_primitive_builtin(self, obj, true)?;
                return self.to_num(&prim);
            }
        })
    }

    pub(super) fn to_property_key(&self, value: &Value) -> R<PropKey> {
        Ok(match value {
            Value::Str(s) => PropKey::Str(s.clone()),
            Value::Sym(s) => PropKey::Sym(s.clone()),
            Value::Num(n) => PropKey::Str(number_to_string(*n).into()),
            other => PropKey::Str(self.to_str(other)?),
        })
    }

    /// Full `ToPrimitive`, calling user-defined `valueOf`/`toString`.
    pub(super) async fn to_primitive(&self, value: &Value, prefer_string: bool) -> R<Value> {
        let Value::Obj(obj) = value else {
            return Ok(value.clone());
        };
        let order = if prefer_string {
            ["toString", "valueOf"]
        } else {
            ["valueOf", "toString"]
        };
        for name in order {
            let method = self.get(value, &name.into()).await?;
            if method.is_callable() {
                let result = self.call(&method, value.clone(), Vec::new()).await?;
                if !matches!(result, Value::Obj(_)) {
                    return Ok(result);
                }
            }
        }
        let _ = obj;
        Err(self.type_error("Cannot convert object to primitive value"))
    }

    pub(super) async fn to_string_async(&self, value: &Value) -> R<JsStr> {
        match value {
            Value::Obj(_) => {
                let prim = self.to_primitive(value, true).await?;
                self.to_str(&prim)
            }
            other => self.to_str(other),
        }
    }

    pub(super) async fn to_number_async(&self, value: &Value) -> R<f64> {
        match value {
            Value::Obj(_) => {
                let prim = self.to_primitive(value, false).await?;
                self.to_num(&prim)
            }
            other => self.to_num(other),
        }
    }

    pub(super) fn to_object(&self, value: &Value) -> R<Obj> {
        match value {
            Value::Obj(obj) => Ok(obj.clone()),
            Value::Undefined | Value::Null => {
                Err(self.type_error("Cannot convert undefined or null to object"))
            }
            other => {
                let proto = self.primitive_proto(other);
                Ok(Obj::new(proto, Kind::Boxed(other.clone())))
            }
        }
    }

    fn primitive_proto(&self, value: &Value) -> Option<Obj> {
        Some(match value {
            Value::Str(_) => self.protos.string.clone(),
            Value::Num(_) => self.protos.number.clone(),
            Value::Bool(_) => self.protos.boolean.clone(),
            Value::Sym(_) => self.protos.symbol.clone(),
            _ => return None,
        })
    }

    // ----- property access -------------------------------------------------------------

    /// Finds a property without calling getters. `Ok(None)` means absent.
    pub(super) fn lookup(&self, base: &Value, key: &PropKey) -> R<Option<Slot>> {
        let start = match base {
            Value::Obj(obj) => obj.clone(),
            Value::Str(s) => {
                if let PropKey::Str(k) = key {
                    if k.as_str() == "length" {
                        return Ok(Some(Slot::Data(Value::Num(s.len16() as f64))));
                    }
                    if let Some(index) = array_index(k) {
                        return Ok(s
                            .unit_at(index)
                            .map(|_| Slot::Data(Value::Str(s.slice16(index, index + 1)))));
                    }
                }
                self.protos.string.clone()
            }
            Value::Undefined | Value::Null => {
                let shown = match key {
                    PropKey::Str(s) => s.to_string(),
                    PropKey::Sym(_) => "Symbol()".to_string(),
                };
                let what = if matches!(base, Value::Null) {
                    "null"
                } else {
                    "undefined"
                };
                return Err(self.type_error(format!(
                    "Cannot read properties of {what} (reading '{shown}')"
                )));
            }
            other => match self.primitive_proto(other) {
                Some(proto) => proto,
                None => return Ok(None),
            },
        };
        let mut current = Some(start);
        while let Some(obj) = current {
            if let Some(slot) = self.own_slot(&obj, key) {
                return Ok(Some(slot));
            }
            current = obj.proto();
        }
        Ok(None)
    }

    /// Own property including virtual properties of exotic objects.
    pub(super) fn own_slot(&self, obj: &Obj, key: &PropKey) -> Option<Slot> {
        let data = obj.borrow();
        if let PropKey::Str(k) = key {
            match &data.kind {
                Kind::Array(items) => {
                    if let Some(index) = array_index(k) {
                        return items.get(index).map(|v| Slot::Data(v.clone()));
                    }
                    if k.as_str() == "length" {
                        return Some(Slot::Data(Value::Num(items.len() as f64)));
                    }
                }
                Kind::Bytes(bytes) => {
                    if let Some(index) = array_index(k) {
                        return bytes
                            .get(index)
                            .map(|v| Slot::Data(Value::Num(f64::from(*v))));
                    }
                    if k.as_str() == "length" || k.as_str() == "byteLength" {
                        return Some(Slot::Data(Value::Num(bytes.len() as f64)));
                    }
                }
                Kind::Boxed(Value::Str(s)) => {
                    if k.as_str() == "length" {
                        return Some(Slot::Data(Value::Num(s.len16() as f64)));
                    }
                    if let Some(index) = array_index(k) {
                        return s
                            .unit_at(index)
                            .map(|_| Slot::Data(Value::Str(s.slice16(index, index + 1))));
                    }
                }
                Kind::Function(Func::Tool(path)) => {
                    if let Some(prop) = data.props.get(key) {
                        return Some(prop.slot.clone());
                    }
                    if matches!(
                        k.as_str(),
                        "then" | "call" | "apply" | "bind" | "toString" | "constructor" | "toJSON"
                    ) {
                        return None;
                    }
                    let mut next: Vec<JsStr> = path.to_vec();
                    next.push(k.clone());
                    let path: Rc<[JsStr]> = next.into();
                    drop(data);
                    return Some(Slot::Data(self.tool_ref(path)));
                }
                _ => {}
            }
        }
        data.props.get(key).map(|prop| prop.slot.clone())
    }

    pub(super) async fn get(&self, base: &Value, key: &PropKey) -> R<Value> {
        match self.lookup(base, key)? {
            None => Ok(Value::Undefined),
            Some(Slot::Data(value)) => Ok(value),
            Some(Slot::Accessor {
                get: Some(getter), ..
            }) => {
                self.call(&Value::Obj(getter), base.clone(), Vec::new())
                    .await
            }
            Some(Slot::Accessor { get: None, .. }) => Ok(Value::Undefined),
        }
    }

    pub(super) async fn get_str(&self, base: &Value, key: &str) -> R<Value> {
        self.get(base, &PropKey::from(key)).await
    }

    /// Reads a data property without invoking getters (native fast path).
    pub(super) fn get_data(&self, base: &Value, key: &str) -> R<Value> {
        match self.lookup(base, &PropKey::from(key))? {
            Some(Slot::Data(value)) => Ok(value),
            _ => Ok(Value::Undefined),
        }
    }

    pub(super) async fn set(&self, base: &Value, key: PropKey, value: Value) -> R<()> {
        let obj = match base {
            Value::Obj(obj) => obj.clone(),
            Value::Undefined | Value::Null => {
                let shown = key.as_str().map(|s| s.to_string()).unwrap_or_default();
                return Err(self.type_error(format!(
                    "Cannot set properties of {} (setting '{shown}')",
                    if matches!(base, Value::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                )));
            }
            _ => {
                return Err(self.type_error("Cannot create property on primitive value"));
            }
        };
        // Exotic own properties.
        if let PropKey::Str(k) = &key {
            let mut data = obj.borrow_mut();
            let frozen = data.frozen;
            match &mut data.kind {
                Kind::Array(items) => {
                    if let Some(index) = array_index(k) {
                        if frozen {
                            return Err(self.type_error(format!(
                                "Cannot assign to read only property '{k}' of object"
                            )));
                        }
                        if index < items.len() {
                            items[index] = value;
                        } else {
                            self.check_array_len(index + 1)?;
                            items.resize(index, Value::Undefined);
                            items.push(value);
                        }
                        return Ok(());
                    }
                    if k.as_str() == "length" {
                        if frozen {
                            return Err(self.type_error(
                                "Cannot assign to read only property 'length' of object",
                            ));
                        }
                        let n = self.to_num(&value)?;
                        if n < 0.0 || n.fract() != 0.0 || n > f64::from(u32::MAX) {
                            return Err(self.range_error("Invalid array length"));
                        }
                        let n = n as usize;
                        self.check_array_len(n)?;
                        items.resize(n, Value::Undefined);
                        return Ok(());
                    }
                }
                Kind::Bytes(bytes) => {
                    if let Some(index) = array_index(k) {
                        if index < bytes.len() {
                            let n = self.to_num(&value)?;
                            bytes[index] = to_uint32(n) as u8;
                        }
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        // Own or inherited accessor / read-only data property.
        let mut current = Some(obj.clone());
        while let Some(holder) = current {
            let found = holder.borrow().props.get(&key).cloned();
            if let Some(prop) = found {
                match prop.slot {
                    Slot::Accessor {
                        set: Some(setter), ..
                    } => {
                        self.call(&Value::Obj(setter), base.clone(), vec![value])
                            .await?;
                        return Ok(());
                    }
                    Slot::Accessor { set: None, .. } => {
                        return Err(self.type_error(format!(
                            "Cannot set property {} of object which has only a getter",
                            key.as_str().map(|s| s.as_str()).unwrap_or("symbol")
                        )));
                    }
                    Slot::Data(_)
                        if !prop.writable || (holder.ptr_eq(&obj) && holder.borrow().frozen) =>
                    {
                        return Err(self.type_error(format!(
                            "Cannot assign to read only property '{}' of object",
                            key.as_str().map(|s| s.as_str()).unwrap_or("symbol")
                        )));
                    }
                    Slot::Data(_) => break,
                }
            }
            current = holder.proto();
        }
        let mut data = obj.borrow_mut();
        if let Some(prop) = data.props.get_mut(&key) {
            prop.slot = Slot::Data(value);
            return Ok(());
        }
        if !data.extensible {
            return Err(self.type_error(format!(
                "Cannot add property {}, object is not extensible",
                key.as_str().map(|s| s.as_str()).unwrap_or("symbol")
            )));
        }
        data.props.insert(key, Prop::data(value));
        Ok(())
    }

    /// Creates or replaces an own enumerable data property, as object literals do.
    pub(super) fn define(&self, obj: &Obj, key: PropKey, value: Value) -> R<()> {
        if let PropKey::Str(k) = &key {
            let mut data = obj.borrow_mut();
            if let Kind::Array(items) = &mut data.kind
                && let Some(index) = array_index(k)
            {
                if index < items.len() {
                    items[index] = value;
                } else {
                    self.check_array_len(index + 1)?;
                    items.resize(index, Value::Undefined);
                    items.push(value);
                }
                return Ok(());
            }
        }
        obj.borrow_mut().props.insert(key, Prop::data(value));
        Ok(())
    }

    pub(super) fn has_property(&self, obj: &Obj, key: &PropKey) -> bool {
        let mut current = Some(obj.clone());
        while let Some(o) = current {
            if self.own_slot(&o, key).is_some() {
                return true;
            }
            current = o.proto();
        }
        false
    }

    pub(super) fn has_own(&self, obj: &Obj, key: &PropKey) -> bool {
        if matches!(obj.borrow().kind, Kind::Function(Func::Tool(_))) {
            return obj.borrow().props.contains_key(key);
        }
        self.own_slot(obj, key).is_some()
    }

    pub(super) fn delete(&self, obj: &Obj, key: &PropKey) -> R<bool> {
        let mut data = obj.borrow_mut();
        if data.frozen {
            return Err(self.type_error("Cannot delete property of frozen object"));
        }
        if let (Kind::Array(items), PropKey::Str(k)) = (&mut data.kind, key)
            && let Some(index) = array_index(k)
        {
            if index < items.len() {
                items[index] = Value::Undefined;
            }
            return Ok(true);
        }
        if let Some(prop) = data.props.get(key)
            && !prop.configurable
        {
            return Err(self.type_error("Cannot delete non-configurable property"));
        }
        data.props.shift_remove(key);
        Ok(true)
    }

    /// Own property keys in JavaScript order: integer keys, strings, then symbols.
    pub(super) fn own_keys(
        &self,
        obj: &Obj,
        only_enumerable: bool,
        include_symbols: bool,
    ) -> Vec<PropKey> {
        let data = obj.borrow();
        let mut out = Vec::new();
        if let Kind::Function(Func::Tool(path)) = &data.kind {
            let prefix: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
            return self
                .host
                .catalog
                .children(&prefix.join("."))
                .into_iter()
                .map(|name| PropKey::Str(name.into()))
                .collect();
        }
        match &data.kind {
            Kind::Array(items) => {
                out.extend((0..items.len()).map(|i| PropKey::Str(i.to_string().into())))
            }
            Kind::Bytes(bytes) => {
                out.extend((0..bytes.len()).map(|i| PropKey::Str(i.to_string().into())))
            }
            Kind::Boxed(Value::Str(s)) => {
                out.extend((0..s.len16()).map(|i| PropKey::Str(i.to_string().into())))
            }
            _ => {}
        }
        let mut integers: Vec<(usize, PropKey)> = Vec::new();
        let mut strings = Vec::new();
        let mut symbols = Vec::new();
        for (key, prop) in &data.props {
            if only_enumerable && !prop.enumerable {
                continue;
            }
            match key {
                PropKey::Str(s) => match array_index(s) {
                    Some(index) => integers.push((index, key.clone())),
                    None => strings.push(key.clone()),
                },
                PropKey::Sym(_) => symbols.push(key.clone()),
            }
        }
        integers.sort_by_key(|(index, _)| *index);
        out.extend(integers.into_iter().map(|(_, key)| key));
        out.extend(strings);
        if include_symbols {
            out.extend(symbols);
        }
        if !only_enumerable && let Kind::Array(_) = data.kind {
            out.push("length".into());
        }
        out
    }

    /// Enumerable own string-keyed `[key, value]` pairs, calling getters.
    pub(super) async fn own_entries(&self, obj: &Obj) -> R<Vec<(JsStr, Value)>> {
        let keys = self.own_keys(obj, true, false);
        let base = Value::Obj(obj.clone());
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            let value = self.get(&base, &key).await?;
            if let PropKey::Str(s) = key {
                out.push((s, value));
            }
        }
        Ok(out)
    }

    // ----- calls -----------------------------------------------------------------------

    pub(super) fn call(
        &self,
        func: &Value,
        this: Value,
        args: Vec<Value>,
    ) -> LocalBoxFuture<'static, R<Value>> {
        let cx = self.clone();
        let Some(obj) = func.as_obj().filter(|o| o.is_callable()).cloned() else {
            let err = self.type_error(format!("{} is not a function", describe_value(func)));
            return Box::pin(async move { Err(err) });
        };
        let Some(f) = obj.func() else {
            unreachable!("checked callable")
        };
        match f {
            Func::Native(native) => match &native.call {
                Some(NativeImpl::Sync(f)) => {
                    let result = f(&cx, &this, &args);
                    Box::pin(async move { result })
                }
                Some(NativeImpl::Async(f)) => f(cx, this, args),
                None => {
                    let err = self.type_error(format!(
                        "Class constructor {} cannot be invoked without 'new'",
                        native.name
                    ));
                    Box::pin(async move { Err(err) })
                }
            },
            Func::Bound(bound) => {
                let mut all = bound.args.clone();
                all.extend(args);
                cx.call(&Value::Obj(bound.target.clone()), bound.this.clone(), all)
            }
            Func::User(user) => {
                if user.is_class_constructor {
                    let name = user
                        .def
                        .name
                        .clone()
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    let err = self.type_error(format!(
                        "Class constructor {name} cannot be invoked without 'new'"
                    ));
                    return Box::pin(async move { Err(err) });
                }
                Box::pin(async move { cx.call_user(user, obj, this, args, None).await })
            }
            Func::Tool(path) => {
                // Tool calls return promises so independent calls can run concurrently.
                let task_cx = cx.clone();
                let result = promise::spawn_promise(&cx, async move {
                    super::host::tool_call(&task_cx, &path, args).await
                });
                Box::pin(async move { result })
            }
        }
    }

    pub(super) fn construct(
        &self,
        func: &Value,
        args: Vec<Value>,
        new_target: Value,
    ) -> LocalBoxFuture<'static, R<Value>> {
        let cx = self.clone();
        let not_ctor = |cx: &Cx| {
            let err = cx.type_error(format!("{} is not a constructor", describe_value(func)));
            Box::pin(async move { Err(err) }) as LocalBoxFuture<'static, R<Value>>
        };
        let Some(obj) = func.as_obj().cloned() else {
            return not_ctor(self);
        };
        let Some(f) = obj.func() else {
            return not_ctor(self);
        };
        match f {
            Func::Native(native) => match &native.construct {
                Some(NativeImpl::Sync(f)) => {
                    let result = f(&cx, &new_target, &args);
                    Box::pin(async move { result })
                }
                Some(NativeImpl::Async(f)) => f(cx, new_target, args),
                None => not_ctor(self),
            },
            Func::Bound(bound) => {
                let mut all = bound.args.clone();
                all.extend(args);
                let target = Value::Obj(bound.target.clone());
                let new_target = match &new_target {
                    Value::Obj(nt) if nt.ptr_eq(&obj) => target.clone(),
                    _ => new_target,
                };
                cx.construct(&target, all, new_target)
            }
            Func::User(user) => {
                let def = &user.def;
                if def.is_arrow
                    || def.is_async
                    || def.is_generator
                    || (user.home.borrow().is_some() && !user.is_class_constructor)
                {
                    return not_ctor(self);
                }
                Box::pin(async move { cx.construct_user(user, obj, args, new_target).await })
            }
            Func::Tool(_) => not_ctor(self),
        }
    }

    async fn construct_user(
        &self,
        user: Rc<UserFn>,
        func_obj: Obj,
        args: Vec<Value>,
        new_target: Value,
    ) -> R<Value> {
        let derived = user.class.as_ref().is_some_and(|c| c.derived);
        let this = if derived {
            None
        } else {
            let proto = self.proto_from(&new_target, &self.protos.object);
            let obj = Obj::new(Some(proto), Kind::Plain);
            if let Some(class) = &user.class {
                self.init_fields(class, &func_obj, &obj).await?;
            }
            Some(Value::Obj(obj))
        };
        let ctx = Rc::new(FnCtx {
            this: RefCell::new(this),
            home: user.home.borrow().clone(),
            new_target: Some(new_target.clone()),
            func: Some(func_obj.clone()),
            args: RefCell::new(Vec::new()),
            gen_slot: None,
        });
        let has_body = user
            .class
            .as_ref()
            .is_none_or(|c| c.def.constructor.is_some());
        let result = if has_body {
            self.run_function(&user, ctx.clone(), args).await?
        } else {
            // Implicit constructor: `constructor(...args) { super(...args) }`.
            if derived {
                self.super_call(&ctx, args).await?;
            }
            Value::Undefined
        };
        if let Value::Obj(_) = result {
            return Ok(result);
        }
        let this = ctx.this.borrow().clone();
        match this {
            Some(this) => Ok(this),
            None => Err(self.throw(
                ErrorKind::Reference,
                "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
            )),
        }
    }

    async fn init_fields(&self, class: &ClassInfo, func_obj: &Obj, target: &Obj) -> R<()> {
        let fields: Vec<(usize, &ClassField)> = class
            .def
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.is_static)
            .collect();
        if fields.is_empty() {
            return Ok(());
        }
        let ctx = Rc::new(FnCtx {
            this: RefCell::new(Some(Value::Obj(target.clone()))),
            home: target.proto(),
            new_target: None,
            func: Some(func_obj.clone()),
            args: RefCell::new(Vec::new()),
            gen_slot: None,
        });
        let env = Scope::new(Some(class.env.clone()), Some(ctx));
        for (index, field) in fields {
            let value = match &field.value {
                Some(expr) => self.ev(expr, &env).await?,
                None => Value::Undefined,
            };
            let key = class.field_keys[index].clone();
            self.define(target, key, value)?;
        }
        Ok(())
    }

    async fn super_call(&self, ctx: &Rc<FnCtx>, args: Vec<Value>) -> R<Value> {
        let Some(func) = &ctx.func else {
            return Err(self.throw(ErrorKind::Syntax, "'super' keyword unexpected here"));
        };
        let parent = func.proto().map(Value::Obj).unwrap_or(Value::Null);
        if ctx.this.borrow().is_some() {
            return Err(self.throw(
                ErrorKind::Reference,
                "Super constructor may only be called once",
            ));
        }
        let new_target = ctx.new_target.clone().unwrap_or(Value::Undefined);
        let this = self.construct(&parent, args, new_target).await?;
        *ctx.this.borrow_mut() = Some(this.clone());
        if let (Some(Func::User(user)), Value::Obj(target)) = (func.func(), &this)
            && let Some(class) = &user.class
        {
            self.init_fields(class, func, target).await?;
        }
        Ok(Value::Undefined)
    }

    async fn call_user(
        &self,
        user: Rc<UserFn>,
        func_obj: Obj,
        this: Value,
        args: Vec<Value>,
        new_target: Option<Value>,
    ) -> R<Value> {
        let def = user.def.clone();
        let gen_slot = def.is_generator.then(|| Rc::new(GenSlot::default()));
        let ctx = if def.is_arrow {
            None
        } else {
            Some(Rc::new(FnCtx {
                this: RefCell::new(Some(this)),
                home: user.home.borrow().clone(),
                new_target,
                func: Some(func_obj.clone()),
                args: RefCell::new(Vec::new()),
                gen_slot: gen_slot.clone(),
            }))
        };
        if let Some(slot) = gen_slot {
            return Ok(generator::create(self, user, &func_obj, ctx, args, slot));
        }
        if def.is_async {
            let (promise, cell) = promise::new_promise(self)?;
            let cx = self.clone();
            let body = async move {
                let result = cx.run_function_opt(&user, ctx, args).await;
                match result {
                    Ok(value) => promise::resolve_value(&cx, &cell, value),
                    Err(Abrupt::Throw(error)) => promise::reject(&cx, &cell, error),
                    Err(Abrupt::Fatal(fatal)) => promise::fatal(&cell, fatal),
                    Err(Abrupt::Short | Abrupt::GenReturn(_)) => {
                        promise::resolve_value(&cx, &cell, Value::Undefined)
                    }
                }
            };
            spawn_eager(body);
            return Ok(promise);
        }
        self.run_function_opt(&user, ctx, args).await
    }

    pub(super) async fn run_function_opt(
        &self,
        user: &UserFn,
        ctx: Option<Rc<FnCtx>>,
        args: Vec<Value>,
    ) -> R<Value> {
        self.tick()?;
        self.stack_check()?;
        let env = Scope::new(Some(user.env.clone()), ctx);
        self.bind_params(&user.def, &env, args).await?;
        match &user.def.body {
            FuncBody::Expr(expr) => self.ev(expr, &env).await,
            FuncBody::Block(body) => {
                for name in &user.def.var_names {
                    env.declare_var(name);
                }
                match self.exec_body(body, &env).await? {
                    Completion::Return(value) => Ok(value),
                    _ => Ok(Value::Undefined),
                }
            }
        }
    }

    async fn run_function(&self, user: &UserFn, ctx: Rc<FnCtx>, args: Vec<Value>) -> R<Value> {
        self.run_function_opt(user, Some(ctx), args).await
    }

    pub(super) async fn bind_params(&self, def: &Function, env: &Env, args: Vec<Value>) -> R<()> {
        if let Some(ctx) = &env.ctx {
            *ctx.args.borrow_mut() = args.clone();
        }
        let mut args = args.into_iter();
        for param in &def.params {
            let value = args.next().unwrap_or(Value::Undefined);
            match param {
                Pattern::Ident(name) => env.declare(name, value, true),
                other => self.bind_pattern(other, value, env, Some(true)).await?,
            }
        }
        if let Some(rest) = &def.rest {
            let rest_values: Vec<Value> = args.collect();
            let array = self.array_value(rest_values);
            self.bind_pattern(rest, array, env, Some(true)).await?;
        }
        Ok(())
    }

    // ----- statements ------------------------------------------------------------------

    /// Instantiates hoisted declarations of a statement list into `env`.
    fn hoist(&self, body: &[Stmt], env: &Env) {
        for stmt in body {
            match stmt {
                Stmt::Var(VarKind::Let | VarKind::Const, decls) => {
                    let mut names = Vec::new();
                    for decl in decls {
                        collect_names(&decl.target, &mut names);
                    }
                    for name in names {
                        env.declare_uninit(&name, true);
                    }
                }
                Stmt::Class(class) => {
                    if let Some(name) = &class.name {
                        env.declare_uninit(name, true);
                    }
                }
                Stmt::Func(func) => {
                    if let Some(name) = &func.name {
                        let closure = self.make_closure(func.clone(), env.clone());
                        env.declare(name, closure, true);
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) async fn exec_body(&self, body: &[Stmt], env: &Env) -> R<Completion> {
        self.hoist(body, env);
        for stmt in body {
            let completion = self.exec(stmt, env, &[]).await?;
            if !matches!(completion, Completion::Normal) {
                return Ok(completion);
            }
        }
        Ok(Completion::Normal)
    }

    async fn exec_block(&self, body: &[Stmt], env: &Env) -> R<Completion> {
        let needs_scope = body.iter().any(|stmt| {
            matches!(
                stmt,
                Stmt::Var(VarKind::Let | VarKind::Const, _) | Stmt::Class(_) | Stmt::Func(_)
            )
        });
        if needs_scope {
            let scope = Scope::new(Some(env.clone()), None);
            self.exec_body(body, &scope).await
        } else {
            for stmt in body {
                let completion = self.exec(stmt, env, &[]).await?;
                if !matches!(completion, Completion::Normal) {
                    return Ok(completion);
                }
            }
            Ok(Completion::Normal)
        }
    }

    pub(super) fn exec<'a>(
        &'a self,
        stmt: &'a Stmt,
        env: &'a Env,
        labels: &'a [JsStr],
    ) -> LocalBoxFuture<'a, R<Completion>> {
        Box::pin(self.exec_inner(stmt, env, labels))
    }

    async fn exec_inner(&self, stmt: &Stmt, env: &Env, labels: &[JsStr]) -> R<Completion> {
        self.tick()?;
        match stmt {
            Stmt::Expr(expr) => {
                self.ev(expr, env).await?;
                Ok(Completion::Normal)
            }
            Stmt::Var(kind, decls) => {
                for decl in decls {
                    match (&decl.init, kind) {
                        (None, VarKind::Var) => {}
                        (None, _) => {
                            if let Pattern::Ident(name) = &decl.target {
                                self.initialize(env, name, Value::Undefined, *kind == VarKind::Let);
                            }
                        }
                        (Some(init), _) => {
                            let value = self.ev_named(init, env, &decl.target).await?;
                            match kind {
                                VarKind::Var => {
                                    self.bind_pattern(&decl.target, value, env, None).await?
                                }
                                VarKind::Let => {
                                    self.bind_pattern(&decl.target, value, env, Some(true))
                                        .await?
                                }
                                VarKind::Const => {
                                    self.bind_pattern(&decl.target, value, env, Some(false))
                                        .await?
                                }
                            }
                        }
                    }
                }
                Ok(Completion::Normal)
            }
            Stmt::Func(_) => Ok(Completion::Normal),
            Stmt::Class(class) => {
                let value = self.eval_class(class, env).await?;
                if let Some(name) = &class.name {
                    self.initialize(env, name, value, true);
                }
                Ok(Completion::Normal)
            }
            Stmt::Return(value) => {
                let value = match value {
                    Some(expr) => self.ev(expr, env).await?,
                    None => Value::Undefined,
                };
                Ok(Completion::Return(value))
            }
            Stmt::If(test, then, other) => {
                if self.ev(test, env).await?.truthy() {
                    self.exec(then, env, &[]).await
                } else if let Some(other) = other {
                    self.exec(other, env, &[]).await
                } else {
                    Ok(Completion::Normal)
                }
            }
            Stmt::Block(body) => self.exec_block(body, env).await,
            Stmt::While(test, body) => {
                loop {
                    self.tick()?;
                    if !self.ev(test, env).await?.truthy() {
                        break;
                    }
                    match self.exec(body, env, &[]).await? {
                        Completion::Break(label) if loop_label_matches(&label, labels) => break,
                        Completion::Continue(label) if loop_label_matches(&label, labels) => {
                            continue;
                        }
                        Completion::Normal => {}
                        other => return Ok(other),
                    }
                }
                Ok(Completion::Normal)
            }
            Stmt::DoWhile(body, test) => {
                loop {
                    self.tick()?;
                    match self.exec(body, env, &[]).await? {
                        Completion::Break(label) if loop_label_matches(&label, labels) => break,
                        Completion::Continue(label) if loop_label_matches(&label, labels) => {}
                        Completion::Normal => {}
                        other => return Ok(other),
                    }
                    if !self.ev(test, env).await?.truthy() {
                        break;
                    }
                }
                Ok(Completion::Normal)
            }
            Stmt::For(for_stmt) => self.exec_for(for_stmt, env, labels).await,
            Stmt::ForIn(each) => self.exec_for_in(each, env, labels).await,
            Stmt::ForOf(each) => self.exec_for_of(each, env, labels).await,
            Stmt::Break(label) => Ok(Completion::Break(label.clone())),
            Stmt::Continue(label) => Ok(Completion::Continue(label.clone())),
            Stmt::Throw(expr, pos) => {
                let value = self.ev(expr, env).await?;
                if let Value::Obj(obj) = &value
                    && obj.is_error()
                    && obj.own_data("line").is_none()
                    && pos.line > 0
                {
                    obj.set_hidden("line", Value::Num(f64::from(pos.line)));
                }
                Err(Abrupt::Throw(value))
            }
            Stmt::Try(try_stmt) => self.exec_try(try_stmt, env).await,
            Stmt::Switch(discriminant, cases) => {
                self.exec_switch(discriminant, cases, env, labels).await
            }
            Stmt::Labeled(label, body) => {
                let mut all = labels.to_vec();
                all.push(label.clone());
                match self.exec(body, env, &all).await? {
                    Completion::Break(Some(l)) if &l == label => Ok(Completion::Normal),
                    other => Ok(other),
                }
            }
            Stmt::Empty => Ok(Completion::Normal),
        }
    }

    fn initialize(&self, env: &Env, name: &JsStr, value: Value, mutable: bool) {
        env.declare(name, value, mutable);
    }

    async fn exec_for(&self, for_stmt: &ForStmt, env: &Env, labels: &[JsStr]) -> R<Completion> {
        let mut per_iteration = Vec::new();
        let mut current = env.clone();
        match &for_stmt.init {
            Some(ForInit::Var(kind, decls)) => {
                if *kind != VarKind::Var {
                    current = Scope::new(Some(env.clone()), None);
                    if *kind == VarKind::Let {
                        for decl in decls {
                            collect_names(&decl.target, &mut per_iteration);
                        }
                    }
                }
                let stmt_kind = *kind;
                for decl in decls {
                    let value = match &decl.init {
                        Some(init) => self.ev(init, &current).await?,
                        None => Value::Undefined,
                    };
                    let mutable = match stmt_kind {
                        VarKind::Var => None,
                        VarKind::Let => Some(true),
                        VarKind::Const => Some(false),
                    };
                    self.bind_pattern(&decl.target, value, &current, mutable)
                        .await?;
                }
            }
            Some(ForInit::Expr(expr)) => {
                self.ev(expr, env).await?;
            }
            None => {}
        }
        loop {
            self.tick()?;
            if let Some(test) = &for_stmt.test
                && !self.ev(test, &current).await?.truthy()
            {
                break;
            }
            match self.exec(&for_stmt.body, &current, &[]).await? {
                Completion::Break(label) if loop_label_matches(&label, labels) => break,
                Completion::Continue(label) if loop_label_matches(&label, labels) => {}
                Completion::Normal => {}
                other => return Ok(other),
            }
            if !per_iteration.is_empty() {
                let next = Scope::new(Some(env.clone()), None);
                for name in &per_iteration {
                    let value = current
                        .vars
                        .borrow()
                        .iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, b)| b.value.clone());
                    next.declare(name, value.unwrap_or(Value::Undefined), true);
                }
                current = next;
            }
            if let Some(update) = &for_stmt.update {
                self.ev(update, &current).await?;
            }
        }
        Ok(Completion::Normal)
    }

    async fn bind_for_target(&self, target: &ForTarget, value: Value, env: &Env) -> R<Env> {
        match target {
            ForTarget::Decl(VarKind::Var, pattern) => {
                self.bind_pattern(pattern, value, env, None).await?;
                Ok(env.clone())
            }
            ForTarget::Decl(kind, pattern) => {
                let scope = Scope::new(Some(env.clone()), None);
                self.bind_pattern(pattern, value, &scope, Some(*kind == VarKind::Let))
                    .await?;
                Ok(scope)
            }
            ForTarget::Assign(pattern) => {
                self.assign_pattern(pattern, value, env).await?;
                Ok(env.clone())
            }
        }
    }

    async fn exec_for_in(&self, each: &ForEach, env: &Env, labels: &[JsStr]) -> R<Completion> {
        let object = self.ev(&each.iterable, env).await?;
        let Value::Obj(obj) = (match object {
            Value::Undefined | Value::Null => return Ok(Completion::Normal),
            Value::Obj(_) => object,
            other => Value::Obj(self.to_object(&other)?),
        }) else {
            return Ok(Completion::Normal);
        };
        let mut keys: Vec<JsStr> = Vec::new();
        let mut current = Some(obj.clone());
        while let Some(o) = current {
            for key in self.own_keys(&o, true, false) {
                if let PropKey::Str(s) = key
                    && !keys.contains(&s)
                {
                    keys.push(s);
                }
            }
            current = o.proto();
        }
        for key in keys {
            self.tick()?;
            if !self.has_property(&obj, &PropKey::Str(key.clone())) {
                continue;
            }
            let scope = self
                .bind_for_target(&each.target, Value::Str(key), env)
                .await?;
            match self.exec(&each.body, &scope, &[]).await? {
                Completion::Break(label) if loop_label_matches(&label, labels) => break,
                Completion::Continue(label) if loop_label_matches(&label, labels) => {}
                Completion::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Completion::Normal)
    }

    async fn exec_for_of(&self, each: &ForEach, env: &Env, labels: &[JsStr]) -> R<Completion> {
        let iterable = self.ev(&each.iterable, env).await?;
        let mut iter = self.get_iterator(&iterable, each.is_await).await?;
        loop {
            self.tick()?;
            let Some(mut value) = self.iter_step(&mut iter).await? else {
                break;
            };
            if each.is_await {
                value = self.await_value(value).await?;
            }
            let scope = match self.bind_for_target(&each.target, value, env).await {
                Ok(scope) => scope,
                Err(err) => {
                    self.iter_close(&mut iter).await;
                    return Err(err);
                }
            };
            let completion = match self.exec(&each.body, &scope, &[]).await {
                Ok(completion) => completion,
                Err(err) => {
                    self.iter_close(&mut iter).await;
                    return Err(err);
                }
            };
            match completion {
                Completion::Break(label) if loop_label_matches(&label, labels) => {
                    self.iter_close(&mut iter).await;
                    break;
                }
                Completion::Continue(label) if loop_label_matches(&label, labels) => {}
                Completion::Normal => {}
                other => {
                    self.iter_close(&mut iter).await;
                    return Ok(other);
                }
            }
        }
        Ok(Completion::Normal)
    }

    async fn exec_try(&self, try_stmt: &TryStmt, env: &Env) -> R<Completion> {
        let mut result = self.exec_block(&try_stmt.block, env).await;
        if let (Err(Abrupt::Throw(error)), Some(handler)) = (&result, &try_stmt.handler) {
            let error = error.clone();
            let scope = Scope::new(Some(env.clone()), None);
            if let Some(param) = &try_stmt.param {
                self.bind_pattern(param, error, &scope, Some(true)).await?;
            }
            result = self.exec_block(handler, &scope).await;
        }
        if let Some(finalizer) = &try_stmt.finalizer {
            if matches!(result, Err(Abrupt::Fatal(_))) {
                return result;
            }
            match self.exec_block(finalizer, env).await? {
                Completion::Normal => {}
                other => return Ok(other),
            }
        }
        result
    }

    async fn exec_switch(
        &self,
        discriminant: &Expr,
        cases: &[Case],
        env: &Env,
        labels: &[JsStr],
    ) -> R<Completion> {
        let value = self.ev(discriminant, env).await?;
        let scope = Scope::new(Some(env.clone()), None);
        for case in cases {
            self.hoist(&case.body, &scope);
        }
        let mut start = None;
        for (index, case) in cases.iter().enumerate() {
            if let Some(test) = &case.test {
                let candidate = self.ev(test, &scope).await?;
                if candidate.strict_equals(&value) {
                    start = Some(index);
                    break;
                }
            }
        }
        if start.is_none() {
            start = cases.iter().position(|case| case.test.is_none());
        }
        let Some(start) = start else {
            return Ok(Completion::Normal);
        };
        for case in &cases[start..] {
            for stmt in &case.body {
                match self.exec(stmt, &scope, &[]).await? {
                    Completion::Normal => {}
                    Completion::Break(None) => return Ok(Completion::Normal),
                    Completion::Break(Some(label)) if labels.contains(&label) => {
                        return Ok(Completion::Normal);
                    }
                    other => return Ok(other),
                }
            }
        }
        Ok(Completion::Normal)
    }

    // ----- patterns --------------------------------------------------------------------

    /// Binds a declaration pattern. `mutable`: `None` = var, `Some(true)` = let, `Some(false)` = const.
    pub(super) fn bind_pattern<'a>(
        &'a self,
        pattern: &'a Pattern,
        value: Value,
        env: &'a Env,
        mutable: Option<bool>,
    ) -> LocalBoxFuture<'a, R<()>> {
        Box::pin(async move {
            match pattern {
                Pattern::Ident(name) => {
                    match mutable {
                        None => self.assign_var(env, name, value)?,
                        Some(m) => env.declare(name, value, m),
                    }
                    Ok(())
                }
                Pattern::Default(inner, default) => {
                    let value = if matches!(value, Value::Undefined) {
                        self.ev_named(default, env, inner).await?
                    } else {
                        value
                    };
                    self.bind_pattern(inner, value, env, mutable).await
                }
                Pattern::Object(props, rest) => {
                    self.destructure_object(
                        props,
                        rest.as_deref(),
                        value,
                        env,
                        &mut |cx, p, v, e| cx.bind_pattern(p, v, e, mutable),
                    )
                    .await
                }
                Pattern::Array(items, rest) => {
                    self.destructure_array(
                        items,
                        rest.as_deref(),
                        value,
                        env,
                        &mut |cx, p, v, e| cx.bind_pattern(p, v, e, mutable),
                    )
                    .await
                }
                Pattern::Expr(_) => {
                    Err(self.throw(ErrorKind::Syntax, "Invalid destructuring target"))
                }
            }
        })
    }

    /// Assignment to an existing pattern target (`a = 1`, `[a, b] = ...`).
    pub(super) fn assign_pattern<'a>(
        &'a self,
        pattern: &'a Pattern,
        value: Value,
        env: &'a Env,
    ) -> LocalBoxFuture<'a, R<()>> {
        Box::pin(async move {
            match pattern {
                Pattern::Ident(name) => self.assign_var(env, name, value),
                Pattern::Default(inner, default) => {
                    let value = if matches!(value, Value::Undefined) {
                        self.ev_named(default, env, inner).await?
                    } else {
                        value
                    };
                    self.assign_pattern(inner, value, env).await
                }
                Pattern::Object(props, rest) => {
                    self.destructure_object(
                        props,
                        rest.as_deref(),
                        value,
                        env,
                        &mut |cx, p, v, e| cx.assign_pattern(p, v, e),
                    )
                    .await
                }
                Pattern::Array(items, rest) => {
                    self.destructure_array(
                        items,
                        rest.as_deref(),
                        value,
                        env,
                        &mut |cx, p, v, e| cx.assign_pattern(p, v, e),
                    )
                    .await
                }
                Pattern::Expr(expr) => self.assign_expr_target(expr, value, env).await,
            }
        })
    }

    async fn assign_expr_target(&self, expr: &Expr, value: Value, env: &Env) -> R<()> {
        match expr {
            Expr::Member { object, key, .. } => {
                let base = self.ev(object, env).await?;
                let key = self.member_key(key, env).await?;
                self.set(&base, key, value).await
            }
            Expr::Ident(name, _) => self.assign_var(env, name, value),
            _ => Err(self.throw(ErrorKind::Syntax, "Invalid assignment target")),
        }
    }

    async fn destructure_object<'a, F>(
        &'a self,
        props: &'a [PatProp],
        rest: Option<&'a Pattern>,
        value: Value,
        env: &'a Env,
        bind: &mut F,
    ) -> R<()>
    where
        F: FnMut(&'a Cx, &'a Pattern, Value, &'a Env) -> LocalBoxFuture<'a, R<()>>,
    {
        if value.is_nullish() {
            return Err(self.type_error(format!(
                "Cannot destructure '{}' as it is {}.",
                describe_value(&value),
                if matches!(value, Value::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )));
        }
        let mut used = Vec::new();
        for prop in props {
            let key = match &prop.key {
                PropName::Static(name) => PropKey::Str(name.clone()),
                PropName::Computed(expr) => {
                    let k = self.ev(expr, env).await?;
                    self.to_property_key(&k)?
                }
            };
            let item = self.get(&value, &key).await?;
            used.push(key);
            bind(self, &prop.value, item, env).await?;
        }
        if let Some(rest) = rest {
            let out = self.new_object();
            if let Value::Obj(source) = &value {
                for key in self.own_keys(source, true, true) {
                    if used.contains(&key) {
                        continue;
                    }
                    let item = self.get(&value, &key).await?;
                    out.set_data(key, item);
                }
            }
            bind(self, rest, Value::Obj(out), env).await?;
        }
        Ok(())
    }

    async fn destructure_array<'a, F>(
        &'a self,
        items: &'a [Option<Pattern>],
        rest: Option<&'a Pattern>,
        value: Value,
        env: &'a Env,
        bind: &mut F,
    ) -> R<()>
    where
        F: FnMut(&'a Cx, &'a Pattern, Value, &'a Env) -> LocalBoxFuture<'a, R<()>>,
    {
        let mut iter = self.get_iterator(&value, false).await?;
        let mut done = false;
        for item in items {
            let next = if done {
                None
            } else {
                self.iter_step(&mut iter).await?
            };
            if next.is_none() {
                done = true;
            }
            if let Some(pattern) = item {
                bind(self, pattern, next.unwrap_or(Value::Undefined), env).await?;
            }
        }
        if let Some(rest) = rest {
            let mut remaining = Vec::new();
            if !done {
                while let Some(v) = self.iter_step(&mut iter).await? {
                    remaining.push(v);
                    self.check_array_len(remaining.len())?;
                }
            }
            let array = self.array_value(remaining);
            bind(self, rest, array, env).await?;
        } else if !done {
            self.iter_close(&mut iter).await;
        }
        Ok(())
    }

    // ----- variables -------------------------------------------------------------------

    fn lookup_var(&self, env: &Env, name: &JsStr) -> R<Value> {
        let mut scope: Option<&Env> = Some(env);
        while let Some(current) = scope {
            {
                let vars = current.vars.borrow();
                if let Some((_, binding)) = vars.iter().find(|(n, _)| n == name) {
                    if !binding.init {
                        return Err(self.throw(
                            ErrorKind::Reference,
                            format!("Cannot access '{name}' before initialization"),
                        ));
                    }
                    return Ok(binding.value.clone());
                }
            }
            scope = current.parent.as_ref();
        }
        {
            let vars = self.builtins.vars.borrow();
            if let Some((_, binding)) = vars.iter().find(|(n, _)| n == name) {
                return Ok(binding.value.clone());
            }
        }
        if name.as_str() == "arguments"
            && let Some(ctx) = env.fn_ctx()
        {
            let args = ctx.args.borrow().clone();
            return Ok(self.array_value(args));
        }
        Err(self.throw(ErrorKind::Reference, format!("{name} is not defined")))
    }

    /// Appends a source line to a freshly created error that has none.
    fn with_position(&self, err: Abrupt, pos: Pos) -> Abrupt {
        if let Abrupt::Throw(Value::Obj(obj)) = &err
            && obj.is_error()
            && pos.line > 0
            && obj.own_data("line").is_none()
        {
            obj.set_hidden("line", Value::Num(f64::from(pos.line)));
        }
        err
    }

    fn has_var(&self, env: &Env, name: &JsStr) -> bool {
        let mut scope: Option<&Env> = Some(env);
        while let Some(current) = scope {
            if current.vars.borrow().iter().any(|(n, _)| n == name) {
                return true;
            }
            scope = current.parent.as_ref();
        }
        self.builtins.vars.borrow().iter().any(|(n, _)| n == name)
    }

    pub(super) fn assign_var(&self, env: &Env, name: &JsStr, value: Value) -> R<()> {
        let mut scope: Option<&Env> = Some(env);
        while let Some(current) = scope {
            {
                let mut vars = current.vars.borrow_mut();
                if let Some((_, binding)) = vars.iter_mut().find(|(n, _)| n == name) {
                    if !binding.init {
                        return Err(self.throw(
                            ErrorKind::Reference,
                            format!("Cannot access '{name}' before initialization"),
                        ));
                    }
                    if !binding.mutable {
                        return Err(self.type_error("Assignment to constant variable."));
                    }
                    binding.value = value;
                    return Ok(());
                }
            }
            scope = current.parent.as_ref();
        }
        let mut vars = self.builtins.vars.borrow_mut();
        if let Some((_, binding)) = vars.iter_mut().find(|(n, _)| n == name) {
            binding.value = value;
            return Ok(());
        }
        Err(self.throw(ErrorKind::Reference, format!("{name} is not defined")))
    }

    // ----- functions and classes -------------------------------------------------------

    pub(super) fn make_closure(&self, def: Rc<Function>, env: Env) -> Value {
        let name = def
            .name
            .as_ref()
            .map(|n| n.as_str().to_string())
            .unwrap_or_default();
        let is_arrow = def.is_arrow;
        let is_generator = def.is_generator;
        let is_async = def.is_async;
        let length = def
            .params
            .iter()
            .take_while(|p| !matches!(p, Pattern::Default(..)))
            .count();
        let user = UserFn {
            def,
            env,
            home: RefCell::new(None),
            class: None,
            is_class_constructor: false,
        };
        let obj = self.function_object(Func::User(Rc::new(user)), &name, length);
        if !is_arrow && !is_async {
            let proto = if is_generator {
                Obj::new(Some(self.protos.generator.clone()), Kind::Plain)
            } else {
                let proto = self.new_object();
                proto.set_hidden("constructor", Value::Obj(obj.clone()));
                proto
            };
            obj.borrow_mut().props.insert(
                "prototype".into(),
                Prop {
                    slot: Slot::Data(Value::Obj(proto)),
                    enumerable: false,
                    writable: true,
                    configurable: false,
                },
            );
        }
        Value::Obj(obj)
    }

    fn make_method(&self, def: Rc<Function>, env: Env, home: &Obj, name: &str) -> Obj {
        let length = def
            .params
            .iter()
            .take_while(|p| !matches!(p, Pattern::Default(..)))
            .count();
        let user = UserFn {
            def,
            env,
            home: RefCell::new(Some(home.clone())),
            class: None,
            is_class_constructor: false,
        };
        self.function_object(Func::User(Rc::new(user)), name, length)
    }

    fn set_function_name(&self, value: &Value, name: &str) {
        if let Value::Obj(obj) = value
            && obj.is_callable()
        {
            let current = obj.own_data("name");
            if matches!(current, Some(Value::Str(ref s)) if s.is_empty()) {
                obj.borrow_mut().props.insert(
                    "name".into(),
                    Prop {
                        slot: Slot::Data(Value::str(name)),
                        enumerable: false,
                        writable: false,
                        configurable: true,
                    },
                );
            }
        }
    }

    async fn prop_name(&self, name: &PropName, env: &Env) -> R<PropKey> {
        match name {
            PropName::Static(s) => Ok(PropKey::Str(s.clone())),
            PropName::Computed(expr) => {
                let value = self.ev(expr, env).await?;
                self.to_property_key(&value)
            }
        }
    }

    fn define_method(
        &self,
        target: &Obj,
        key: PropKey,
        kind: &MethodKind,
        func: Obj,
        enumerable: bool,
    ) {
        let mut data = target.borrow_mut();
        match kind {
            MethodKind::Method => {
                data.props.insert(
                    key,
                    Prop {
                        slot: Slot::Data(Value::Obj(func)),
                        enumerable,
                        writable: true,
                        configurable: true,
                    },
                );
            }
            MethodKind::Get | MethodKind::Set => {
                let is_get = matches!(kind, MethodKind::Get);
                let entry = data.props.entry(key).or_insert(Prop {
                    slot: Slot::Accessor {
                        get: None,
                        set: None,
                    },
                    enumerable,
                    writable: false,
                    configurable: true,
                });
                if !matches!(entry.slot, Slot::Accessor { .. }) {
                    entry.slot = Slot::Accessor {
                        get: None,
                        set: None,
                    };
                }
                if let Slot::Accessor { get, set } = &mut entry.slot {
                    if is_get {
                        *get = Some(func);
                    } else {
                        *set = Some(func);
                    }
                }
            }
        }
    }

    async fn eval_class(&self, class: &Rc<Class>, env: &Env) -> R<Value> {
        let scope = Scope::new(Some(env.clone()), None);
        let (parent_ctor, parent_proto) = match &class.extends {
            None => (None, Some(self.protos.object.clone())),
            Some(expr) => {
                let parent = self.ev(expr, &scope).await?;
                match &parent {
                    Value::Null => (None, None),
                    Value::Obj(p) if p.is_callable() => {
                        let proto =
                            match self.get_str(&parent, "prototype").await? {
                                Value::Obj(proto) => Some(proto),
                                Value::Null => None,
                                _ => return Err(self.type_error(
                                    "Class extends value does not have valid prototype property",
                                )),
                            };
                        (Some(p.clone()), proto)
                    }
                    other => {
                        return Err(self.type_error(format!(
                            "Class extends value {} is not a constructor or null",
                            describe_value(other)
                        )));
                    }
                }
            }
        };
        let proto = Obj::new(parent_proto, Kind::Plain);
        let name = class
            .name
            .as_ref()
            .map(|n| n.as_str().to_string())
            .unwrap_or_default();
        let ctor_def = class.constructor.clone().unwrap_or_else(|| {
            Rc::new(Function {
                name: class.name.clone(),
                params: Vec::new(),
                rest: None,
                body: FuncBody::Block(Vec::new()),
                is_async: false,
                is_arrow: false,
                is_generator: false,
                var_names: Vec::new(),
            })
        });
        let mut field_keys = Vec::new();
        for field in &class.fields {
            field_keys.push(self.prop_name(&field.key, &scope).await?);
        }
        let info = Rc::new(ClassInfo {
            derived: class.extends.is_some(),
            def: class.clone(),
            field_keys,
            env: scope.clone(),
        });
        let length = ctor_def.params.len();
        let user = UserFn {
            def: ctor_def,
            env: scope.clone(),
            home: RefCell::new(Some(proto.clone())),
            class: Some(info.clone()),
            is_class_constructor: true,
        };
        let ctor = self.function_object(Func::User(Rc::new(user)), &name, length);
        if let Some(parent) = parent_ctor {
            ctor.borrow_mut().proto = Some(parent);
        }
        ctor.borrow_mut().props.insert(
            "prototype".into(),
            Prop {
                slot: Slot::Data(Value::Obj(proto.clone())),
                enumerable: false,
                writable: false,
                configurable: false,
            },
        );
        proto.set_hidden("constructor", Value::Obj(ctor.clone()));
        if let Some(name) = &class.name {
            scope.declare(name, Value::Obj(ctor.clone()), false);
        }
        for member in &class.members {
            let key = self.prop_name(&member.key, &scope).await?;
            let target = if member.is_static { &ctor } else { &proto };
            let name = match &key {
                PropKey::Str(s) => s.as_str().to_string(),
                PropKey::Sym(s) => format!(
                    "[{}]",
                    s.0.description.as_ref().map(|d| d.as_str()).unwrap_or("")
                ),
            };
            let func = self.make_method(member.func.clone(), scope.clone(), target, &name);
            self.define_method(target, key, &member.kind, func, false);
        }
        // Static fields run with `this` bound to the constructor.
        let statics: Vec<(usize, &ClassField)> = class
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.is_static)
            .collect();
        if !statics.is_empty() {
            let ctx = Rc::new(FnCtx {
                this: RefCell::new(Some(Value::Obj(ctor.clone()))),
                home: Some(ctor.clone()),
                new_target: None,
                func: Some(ctor.clone()),
                args: RefCell::new(Vec::new()),
                gen_slot: None,
            });
            let static_env = Scope::new(Some(scope.clone()), Some(ctx));
            for (index, field) in statics {
                let value = match &field.value {
                    Some(expr) => self.ev(expr, &static_env).await?,
                    None => Value::Undefined,
                };
                self.define(&ctor, info.field_keys[index].clone(), value)?;
            }
        }
        Ok(Value::Obj(ctor))
    }

    // ----- expressions -----------------------------------------------------------------

    /// Evaluates an expression, avoiding a boxed future for simple leaves.
    pub(super) async fn ev(&self, expr: &Expr, env: &Env) -> R<Value> {
        match expr {
            Expr::Num(n) => Ok(Value::Num(*n)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Null => Ok(Value::Null),
            Expr::Ident(name, pos) => self
                .lookup_var(env, name)
                .map_err(|err| self.with_position(err, *pos)),
            _ => self.eval(expr, env).await,
        }
    }

    /// Evaluates an initializer, naming anonymous functions after their binding.
    async fn ev_named(&self, expr: &Expr, env: &Env, target: &Pattern) -> R<Value> {
        let value = self.ev(expr, env).await?;
        if expr.is_anonymous_function()
            && let Pattern::Ident(name) = target
        {
            self.set_function_name(&value, name);
        }
        Ok(value)
    }

    pub(super) fn eval<'a>(&'a self, expr: &'a Expr, env: &'a Env) -> LocalBoxFuture<'a, R<Value>> {
        Box::pin(self.eval_inner(expr, env))
    }

    async fn member_key(&self, key: &MemberKey, env: &Env) -> R<PropKey> {
        match key {
            MemberKey::Name(name) => Ok(PropKey::Str(name.clone())),
            MemberKey::Computed(expr) => {
                let value = self.ev(expr, env).await?;
                self.to_property_key(&value)
            }
        }
    }

    fn this_value(&self, env: &Env) -> R<Value> {
        match env.fn_ctx() {
            Some(ctx) => match &*ctx.this.borrow() {
                Some(this) => Ok(this.clone()),
                None => Err(self.throw(
                    ErrorKind::Reference,
                    "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                )),
            },
            None => Ok(Value::Undefined),
        }
    }

    async fn eval_args(&self, args: &[Arg], env: &Env) -> R<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        for arg in args {
            match arg {
                Arg::Expr(expr) => out.push(self.ev(expr, env).await?),
                Arg::Spread(expr) => {
                    let value = self.ev(expr, env).await?;
                    let items = self.iterate_to_vec(&value).await?;
                    out.extend(items);
                    self.check_array_len(out.len())?;
                }
            }
        }
        Ok(out)
    }

    fn super_base(&self, env: &Env) -> R<(Value, Value)> {
        let ctx = env
            .fn_ctx()
            .ok_or_else(|| self.throw(ErrorKind::Syntax, "'super' keyword unexpected here"))?;
        let home = ctx
            .home
            .clone()
            .ok_or_else(|| self.throw(ErrorKind::Syntax, "'super' keyword unexpected here"))?;
        let this = ctx.this.borrow().clone().unwrap_or(Value::Undefined);
        Ok((home.proto().map(Value::Obj).unwrap_or(Value::Null), this))
    }

    async fn eval_inner(&self, expr: &Expr, env: &Env) -> R<Value> {
        match expr {
            Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null | Expr::Ident(..) => {
                self.ev(expr, env).await
            }
            Expr::This => self.this_value(env),
            Expr::Template(template) => {
                let mut out = String::new();
                for (index, part) in template.cooked.iter().enumerate() {
                    if let Some(part) = part {
                        out.push_str(part);
                    }
                    if let Some(expr) = template.exprs.get(index) {
                        let value = self.ev(expr, env).await?;
                        if let Value::Sym(_) = value {
                            return Err(
                                self.type_error("Cannot convert a Symbol value to a string")
                            );
                        }
                        out.push_str(&self.to_string_async(&value).await?);
                        self.check_string_len(out.len())?;
                    }
                }
                Ok(Value::str(out))
            }
            Expr::Tagged(tag, template) => {
                let (func, this) = self.eval_callee(tag, env).await?;
                let strings = self.new_array(
                    template
                        .cooked
                        .iter()
                        .map(|c| c.clone().map(Value::Str).unwrap_or(Value::Undefined))
                        .collect(),
                );
                let raw =
                    self.new_array(template.raw.iter().map(|r| Value::Str(r.clone())).collect());
                strings.set_hidden("raw", Value::Obj(raw));
                let mut args = vec![Value::Obj(strings)];
                for expr in &template.exprs {
                    args.push(self.ev(expr, env).await?);
                }
                self.call(&func, this, args).await
            }
            Expr::Regex(pattern, flags) => super::builtins::regexp::create(self, pattern, flags),
            Expr::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    match item {
                        ArrayElem::Expr(expr) => out.push(self.ev(expr, env).await?),
                        ArrayElem::Hole => out.push(Value::Undefined),
                        ArrayElem::Spread(expr) => {
                            let value = self.ev(expr, env).await?;
                            out.extend(self.iterate_to_vec(&value).await?);
                            self.check_array_len(out.len())?;
                        }
                    }
                }
                Ok(self.array_value(out))
            }
            Expr::Object(props) => self.eval_object(props, env).await,
            Expr::Function(def) => {
                let value = self.make_closure(def.clone(), env.clone());
                if let (Some(name), false) = (&def.name, def.is_arrow) {
                    // Named function expressions can refer to themselves.
                    let scope = Scope::new(Some(env.clone()), None);
                    scope.declare(name, value.clone(), false);
                    if let Value::Obj(obj) = &value
                        && let Some(Func::User(user)) = obj.func()
                    {
                        let rebound = UserFn {
                            def: user.def.clone(),
                            env: scope,
                            home: RefCell::new(None),
                            class: None,
                            is_class_constructor: false,
                        };
                        obj.borrow_mut().kind = Kind::Function(Func::User(Rc::new(rebound)));
                    }
                }
                Ok(value)
            }
            Expr::Class(class) => self.eval_class(class, env).await,
            Expr::Unary(op, operand) => self.eval_unary(*op, operand, env).await,
            Expr::Update {
                increment,
                prefix,
                target,
            } => {
                let old = self.ev(target, env).await?;
                let old = self.to_number_async(&old).await?;
                let new = if *increment { old + 1.0 } else { old - 1.0 };
                self.store(target, Value::Num(new), env).await?;
                Ok(Value::Num(if *prefix { new } else { old }))
            }
            Expr::Binary(op, left, right) => {
                let a = self.ev(left, env).await?;
                let b = self.ev(right, env).await?;
                self.binary(*op, a, b).await
            }
            Expr::Logical(op, left, right) => {
                let a = self.ev(left, env).await?;
                let short = match op {
                    LogicalOp::And => !a.truthy(),
                    LogicalOp::Or => a.truthy(),
                    LogicalOp::Nullish => !a.is_nullish(),
                };
                if short {
                    Ok(a)
                } else {
                    self.ev(right, env).await
                }
            }
            Expr::Assign(op, target, value) => self.eval_assign(*op, target, value, env).await,
            Expr::Cond(test, then, other) => {
                if self.ev(test, env).await?.truthy() {
                    self.ev(then, env).await
                } else {
                    self.ev(other, env).await
                }
            }
            Expr::Call {
                callee,
                args,
                optional,
                pos,
            } => {
                let (func, this) = self.eval_callee(callee, env).await?;
                if *optional && func.is_nullish() {
                    return Err(Abrupt::Short);
                }
                let args = self.eval_args(args, env).await?;
                if !func.is_callable() {
                    return Err(self.type_error(format!(
                        "{} is not a function{}",
                        describe_expr(callee),
                        position_suffix(*pos)
                    )));
                }
                self.call(&func, this, args).await
            }
            Expr::New(callee, args, pos) => {
                let func = self.ev(callee, env).await?;
                let args = self.eval_args(args, env).await?;
                let is_ctor = func.as_obj().is_some_and(|o| o.is_callable());
                if !is_ctor {
                    return Err(self.type_error(format!(
                        "{} is not a constructor{}",
                        describe_expr(callee),
                        position_suffix(*pos)
                    )));
                }
                self.construct(&func, args, func.clone()).await
            }
            Expr::Member {
                object,
                key,
                optional,
            } => {
                let base = self.ev(object, env).await?;
                if *optional && base.is_nullish() {
                    return Err(Abrupt::Short);
                }
                self.get_member(&base, key, env).await
            }
            Expr::SuperMember(key) => {
                let (home, this) = self.super_base(env)?;
                let key = self.member_key(key, env).await?;
                match self.lookup(&home, &key)? {
                    Some(Slot::Data(v)) => Ok(v),
                    Some(Slot::Accessor { get: Some(g), .. }) => {
                        self.call(&Value::Obj(g), this, Vec::new()).await
                    }
                    _ => Ok(Value::Undefined),
                }
            }
            Expr::SuperCall(args) => {
                let args = self.eval_args(args, env).await?;
                let ctx = env.fn_ctx().ok_or_else(|| {
                    self.throw(ErrorKind::Syntax, "'super' keyword unexpected here")
                })?;
                self.super_call(&ctx, args).await
            }
            Expr::Chain(inner) => match self.ev(inner, env).await {
                Err(Abrupt::Short) => Ok(Value::Undefined),
                other => other,
            },
            Expr::Seq(items) => {
                let mut last = Value::Undefined;
                for item in items {
                    last = self.ev(item, env).await?;
                }
                Ok(last)
            }
            Expr::Await(operand) => {
                let value = self.ev(operand, env).await?;
                self.await_value(value).await
            }
            Expr::Yield(value, delegate) => {
                let value = match value {
                    Some(expr) => self.ev(expr, env).await?,
                    None => Value::Undefined,
                };
                let ctx = env.fn_ctx().and_then(|c| c.gen_slot.clone());
                let Some(slot) = ctx else {
                    return Err(self.throw(ErrorKind::Syntax, "yield outside generator"));
                };
                if *delegate {
                    generator::yield_delegate(self, &slot, value).await
                } else {
                    generator::yield_value(self, &slot, value).await
                }
            }
        }
    }

    pub(super) async fn get_member(&self, base: &Value, key: &MemberKey, env: &Env) -> R<Value> {
        if let (Value::Obj(obj), MemberKey::Computed(expr)) = (base, key) {
            let k = self.ev(expr, env).await?;
            if let Value::Num(n) = k
                && n >= 0.0
                && n.fract() == 0.0
                && let Kind::Array(items) = &obj.borrow().kind
            {
                let index = n as usize;
                if index < items.len() {
                    return Ok(items[index].clone());
                }
            }
            let key = self.to_property_key(&k)?;
            return self.get(base, &key).await;
        }
        let key = self.member_key(key, env).await?;
        self.get(base, &key).await
    }

    async fn eval_callee(&self, callee: &Expr, env: &Env) -> R<(Value, Value)> {
        match callee {
            Expr::Member {
                object,
                key,
                optional,
            } => {
                let base = self.ev(object, env).await?;
                if *optional && base.is_nullish() {
                    return Err(Abrupt::Short);
                }
                let func = self.get_member(&base, key, env).await?;
                Ok((func, base))
            }
            Expr::SuperMember(key) => {
                let (home, this) = self.super_base(env)?;
                let key = self.member_key(key, env).await?;
                let func = self.get(&home, &key).await?;
                Ok((func, this))
            }
            other => Ok((self.ev(other, env).await?, Value::Undefined)),
        }
    }

    async fn eval_object(&self, props: &[PropDef], env: &Env) -> R<Value> {
        let obj = self.new_object();
        for prop in props {
            match prop {
                PropDef::Value(name, expr) => {
                    let key = self.prop_name(name, env).await?;
                    let value = self.ev(expr, env).await?;
                    if expr.is_anonymous_function()
                        && let PropKey::Str(s) = &key
                    {
                        self.set_function_name(&value, s);
                    }
                    if matches!(&key, PropKey::Str(s) if s.as_str() == "__proto__")
                        && matches!(name, PropName::Static(_))
                    {
                        match value {
                            Value::Obj(p) => obj.borrow_mut().proto = Some(p),
                            Value::Null => obj.borrow_mut().proto = None,
                            _ => {}
                        }
                        continue;
                    }
                    obj.set_data(key, value);
                }
                PropDef::Shorthand(name) => {
                    let value = self.lookup_var(env, name)?;
                    obj.set_data(name.clone(), value);
                }
                PropDef::CoverInit(..) => {
                    return Err(
                        self.throw(ErrorKind::Syntax, "Invalid shorthand property initializer")
                    );
                }
                PropDef::Method(name, kind, func) => {
                    let key = self.prop_name(name, env).await?;
                    let label = key
                        .as_str()
                        .map(|s| s.as_str().to_string())
                        .unwrap_or_default();
                    let method = self.make_method(func.clone(), env.clone(), &obj, &label);
                    self.define_method(&obj, key, kind, method, true);
                }
                PropDef::Spread(expr) => {
                    let value = self.ev(expr, env).await?;
                    self.copy_data_properties(&obj, &value).await?;
                }
            }
        }
        Ok(Value::Obj(obj))
    }

    pub(super) async fn copy_data_properties(&self, target: &Obj, source: &Value) -> R<()> {
        match source {
            Value::Obj(src) => {
                for key in self.own_keys(src, true, true) {
                    let value = self.get(source, &key).await?;
                    target.set_data(key, value);
                }
            }
            Value::Str(s) => {
                for i in 0..s.len16() {
                    target.set_data(
                        PropKey::Str(i.to_string().into()),
                        Value::Str(s.slice16(i, i + 1)),
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn eval_unary(&self, op: UnaryOp, operand: &Expr, env: &Env) -> R<Value> {
        match op {
            UnaryOp::Typeof => {
                if let Expr::Ident(name, _) = operand
                    && !self.has_var(env, name)
                    && name.as_str() != "arguments"
                {
                    return Ok(Value::str("undefined"));
                }
                let value = self.ev(operand, env).await?;
                Ok(Value::str(value.type_of()))
            }
            UnaryOp::Delete => match operand {
                Expr::Member { object, key, .. } => {
                    let base = self.ev(object, env).await?;
                    let key = self.member_key(key, env).await?;
                    match base {
                        Value::Obj(obj) => Ok(Value::Bool(self.delete(&obj, &key)?)),
                        _ => Ok(Value::Bool(true)),
                    }
                }
                Expr::Chain(inner) => match Box::pin(self.eval_unary(op, inner, env)).await {
                    Err(Abrupt::Short) => Ok(Value::Bool(true)),
                    other => other,
                },
                other => {
                    self.ev(other, env).await?;
                    Ok(Value::Bool(true))
                }
            },
            _ => {
                let value = self.ev(operand, env).await?;
                Ok(match op {
                    UnaryOp::Not => Value::Bool(!value.truthy()),
                    UnaryOp::Minus => Value::Num(-self.to_number_async(&value).await?),
                    UnaryOp::Plus => Value::Num(self.to_number_async(&value).await?),
                    UnaryOp::BitNot => {
                        Value::Num(f64::from(!to_int32(self.to_number_async(&value).await?)))
                    }
                    UnaryOp::Void => Value::Undefined,
                    UnaryOp::Typeof | UnaryOp::Delete => unreachable!("handled above"),
                })
            }
        }
    }

    async fn store(&self, target: &Expr, value: Value, env: &Env) -> R<()> {
        match target {
            Expr::Ident(name, _) => self.assign_var(env, name, value),
            Expr::Member { object, key, .. } => {
                let base = self.ev(object, env).await?;
                let key = self.member_key(key, env).await?;
                self.set(&base, key, value).await
            }
            _ => Err(self.throw(ErrorKind::Syntax, "Invalid assignment target")),
        }
    }

    async fn eval_assign(
        &self,
        op: AssignOp,
        target: &Pattern,
        value: &Expr,
        env: &Env,
    ) -> R<Value> {
        match op {
            AssignOp::Assign => {
                let v = self.ev_named(value, env, target).await?;
                // Simple member targets evaluate the object before the value in JS, but the
                // difference is only observable through side effects in the object expression.
                self.assign_pattern(target, v.clone(), env).await?;
                Ok(v)
            }
            AssignOp::Op(bin) => {
                let (current, base_key) = self.read_target(target, env).await?;
                let rhs = self.ev(value, env).await?;
                let result = self.binary(bin, current, rhs).await?;
                self.write_target(target, base_key, result.clone(), env)
                    .await?;
                Ok(result)
            }
            AssignOp::Logical(logical) => {
                let (current, base_key) = self.read_target(target, env).await?;
                let assign = match logical {
                    LogicalOp::And => current.truthy(),
                    LogicalOp::Or => !current.truthy(),
                    LogicalOp::Nullish => current.is_nullish(),
                };
                if !assign {
                    return Ok(current);
                }
                let result = self.ev_named(value, env, target).await?;
                self.write_target(target, base_key, result.clone(), env)
                    .await?;
                Ok(result)
            }
        }
    }

    async fn read_target(
        &self,
        target: &Pattern,
        env: &Env,
    ) -> R<(Value, Option<(Value, PropKey)>)> {
        match target {
            Pattern::Ident(name) => Ok((self.lookup_var(env, name)?, None)),
            Pattern::Expr(expr) => match &**expr {
                Expr::Member { object, key, .. } => {
                    let base = self.ev(object, env).await?;
                    let key = self.member_key(key, env).await?;
                    let value = self.get(&base, &key).await?;
                    Ok((value, Some((base, key))))
                }
                _ => Err(self.throw(ErrorKind::Syntax, "Invalid assignment target")),
            },
            _ => Err(self.throw(ErrorKind::Syntax, "Invalid assignment target")),
        }
    }

    async fn write_target(
        &self,
        target: &Pattern,
        base_key: Option<(Value, PropKey)>,
        value: Value,
        env: &Env,
    ) -> R<()> {
        match (target, base_key) {
            (_, Some((base, key))) => self.set(&base, key, value).await,
            (Pattern::Ident(name), None) => self.assign_var(env, name, value),
            _ => Err(self.throw(ErrorKind::Syntax, "Invalid assignment target")),
        }
    }

    pub(super) async fn binary(&self, op: BinaryOp, a: Value, b: Value) -> R<Value> {
        use BinaryOp::*;
        if let (Value::Num(x), Value::Num(y)) = (&a, &b) {
            let (x, y) = (*x, *y);
            return Ok(match op {
                Add => Value::Num(x + y),
                Sub => Value::Num(x - y),
                Mul => Value::Num(x * y),
                Div => Value::Num(x / y),
                Rem => Value::Num(js_rem(x, y)),
                Exp => Value::Num(js_pow(x, y)),
                Lt => Value::Bool(x < y),
                LtEq => Value::Bool(x <= y),
                Gt => Value::Bool(x > y),
                GtEq => Value::Bool(x >= y),
                StrictEq | Eq => Value::Bool(x == y),
                StrictNotEq | NotEq => Value::Bool(x != y),
                _ => return self.binary_slow(op, a, b).await,
            });
        }
        self.binary_slow(op, a, b).await
    }

    async fn binary_slow(&self, op: BinaryOp, a: Value, b: Value) -> R<Value> {
        use BinaryOp::*;
        Ok(match op {
            StrictEq => Value::Bool(a.strict_equals(&b)),
            StrictNotEq => Value::Bool(!a.strict_equals(&b)),
            Eq => Value::Bool(self.loose_equals(&a, &b).await?),
            NotEq => Value::Bool(!self.loose_equals(&a, &b).await?),
            Add => {
                let pa = self.to_primitive(&a, false).await?;
                let pb = self.to_primitive(&b, false).await?;
                if matches!(pa, Value::Str(_)) || matches!(pb, Value::Str(_)) {
                    let sa = self.to_str(&pa)?;
                    let sb = self.to_str(&pb)?;
                    self.check_string_len(sa.len() + sb.len())?;
                    let mut out = String::with_capacity(sa.len() + sb.len());
                    out.push_str(&sa);
                    out.push_str(&sb);
                    Value::str(out)
                } else {
                    Value::Num(self.to_num(&pa)? + self.to_num(&pb)?)
                }
            }
            Lt | LtEq | Gt | GtEq => {
                let pa = self.to_primitive(&a, false).await?;
                let pb = self.to_primitive(&b, false).await?;
                let ordering = if let (Value::Str(x), Value::Str(y)) = (&pa, &pb) {
                    Some(x.cmp(y))
                } else {
                    let x = self.to_num(&pa)?;
                    let y = self.to_num(&pb)?;
                    x.partial_cmp(&y)
                };
                let Some(ordering) = ordering else {
                    return Ok(Value::Bool(false));
                };
                Value::Bool(match op {
                    Lt => ordering.is_lt(),
                    LtEq => ordering.is_le(),
                    Gt => ordering.is_gt(),
                    _ => ordering.is_ge(),
                })
            }
            In => {
                let Value::Obj(obj) = &b else {
                    return Err(self.type_error(
                        "Cannot use 'in' operator to search for a key in a non-object",
                    ));
                };
                let key = self.to_property_key(&a)?;
                Value::Bool(self.has_property(obj, &key))
            }
            InstanceOf => Value::Bool(self.instance_of(&a, &b).await?),
            _ => {
                let x = self.to_number_async(&a).await?;
                let y = self.to_number_async(&b).await?;
                match op {
                    Sub => Value::Num(x - y),
                    Mul => Value::Num(x * y),
                    Div => Value::Num(x / y),
                    Rem => Value::Num(js_rem(x, y)),
                    Exp => Value::Num(js_pow(x, y)),
                    Shl => Value::Num(f64::from(to_int32(x).wrapping_shl(to_uint32(y) & 31))),
                    Shr => Value::Num(f64::from(to_int32(x).wrapping_shr(to_uint32(y) & 31))),
                    UShr => Value::Num(f64::from(to_uint32(x).wrapping_shr(to_uint32(y) & 31))),
                    BitAnd => Value::Num(f64::from(to_int32(x) & to_int32(y))),
                    BitOr => Value::Num(f64::from(to_int32(x) | to_int32(y))),
                    BitXor => Value::Num(f64::from(to_int32(x) ^ to_int32(y))),
                    _ => unreachable!("handled above"),
                }
            }
        })
    }

    pub(super) async fn instance_of(&self, value: &Value, ctor: &Value) -> R<bool> {
        let Value::Obj(ctor_obj) = ctor else {
            return Err(self.type_error("Right-hand side of 'instanceof' is not callable"));
        };
        if !ctor_obj.is_callable() {
            return Err(self.type_error("Right-hand side of 'instanceof' is not callable"));
        }
        if let Some(Func::Bound(bound)) = ctor_obj.func() {
            return Box::pin(self.instance_of(value, &Value::Obj(bound.target.clone()))).await;
        }
        let Value::Obj(obj) = value else {
            return Ok(false);
        };
        let proto = self.get_str(ctor, "prototype").await?;
        let Value::Obj(proto) = proto else {
            return Err(self.type_error("Function has non-object prototype in instanceof check"));
        };
        let mut current = obj.proto();
        while let Some(p) = current {
            if p.ptr_eq(&proto) {
                return Ok(true);
            }
            current = p.proto();
        }
        Ok(false)
    }

    pub(super) async fn loose_equals(&self, a: &Value, b: &Value) -> R<bool> {
        Ok(match (a, b) {
            (Value::Undefined | Value::Null, Value::Undefined | Value::Null) => true,
            (Value::Undefined | Value::Null, _) | (_, Value::Undefined | Value::Null) => false,
            (Value::Num(_), Value::Str(_)) | (Value::Str(_), Value::Num(_)) => {
                self.to_num(a)? == self.to_num(b)?
            }
            (Value::Bool(_), _) => {
                Box::pin(self.loose_equals(&Value::Num(self.to_num(a)?), b)).await?
            }
            (_, Value::Bool(_)) => {
                Box::pin(self.loose_equals(a, &Value::Num(self.to_num(b)?))).await?
            }
            (Value::Obj(_), Value::Obj(_)) => a.strict_equals(b),
            (Value::Obj(_), _) => {
                let prim = self.to_primitive(a, false).await?;
                Box::pin(self.loose_equals(&prim, b)).await?
            }
            (_, Value::Obj(_)) => {
                let prim = self.to_primitive(b, false).await?;
                Box::pin(self.loose_equals(a, &prim)).await?
            }
            _ => a.strict_equals(b),
        })
    }

    // ----- promises and iteration ------------------------------------------------------

    pub(super) async fn await_value(&self, value: Value) -> R<Value> {
        // `await` always resumes asynchronously, after code that follows the call site.
        tokio::task::yield_now().await;
        if let Value::Obj(obj) = &value {
            let cell = match &obj.borrow().kind {
                Kind::Promise(cell) => Some(cell.clone()),
                _ => None,
            };
            if let Some(cell) = cell {
                return promise::wait(self, &cell).await;
            }
            let then = self.get_str(&value, "then").await?;
            if then.is_callable() {
                let (promise, cell) = promise::new_promise(self)?;
                let (resolve, reject) = promise::resolving_functions(self, &cell);
                self.call(&then, value.clone(), vec![resolve, reject])
                    .await?;
                let _ = promise;
                return promise::wait(self, &cell).await;
            }
        }
        Ok(value)
    }

    pub(super) async fn get_iterator(&self, value: &Value, is_async: bool) -> R<Iter> {
        if let Value::Obj(obj) = value
            && obj.is_array()
        {
            let custom = obj
                .borrow()
                .props
                .contains_key(&PropKey::Sym(self.sym_iterator.clone()));
            if !custom {
                return Ok(Iter::Array {
                    array: obj.clone(),
                    index: 0,
                });
            }
        }
        if let Value::Str(s) = value {
            let chars: Vec<Value> = s.chars().map(|c| Value::str(c.to_string())).collect();
            return Ok(Iter::List {
                items: chars,
                index: 0,
            });
        }
        if is_async {
            let method = self
                .get(value, &PropKey::Sym(self.sym_async_iterator.clone()))
                .await?;
            if method.is_callable() {
                let iterator = self.call(&method, value.clone(), Vec::new()).await?;
                let next = self.get_str(&iterator, "next").await?;
                return Ok(Iter::Protocol {
                    iterator,
                    next,
                    is_async: true,
                });
            }
        }
        if value.is_nullish() {
            return Err(self.type_error(format!("{} is not iterable", describe_value(value))));
        }
        let method = self
            .get(value, &PropKey::Sym(self.sym_iterator.clone()))
            .await?;
        if !method.is_callable() {
            return Err(self.type_error(format!("{} is not iterable", describe_value(value))));
        }
        let iterator = self.call(&method, value.clone(), Vec::new()).await?;
        if !matches!(iterator, Value::Obj(_)) {
            return Err(self.type_error("Result of the Symbol.iterator method is not an object"));
        }
        let next = self.get_str(&iterator, "next").await?;
        Ok(Iter::Protocol {
            iterator,
            next,
            is_async: false,
        })
    }

    pub(super) async fn iter_step(&self, iter: &mut Iter) -> R<Option<Value>> {
        match iter {
            Iter::Array { array, index } => {
                let item = match &array.borrow().kind {
                    Kind::Array(items) => items.get(*index).cloned(),
                    _ => None,
                };
                if item.is_some() {
                    *index += 1;
                }
                Ok(item)
            }
            Iter::List { items, index } => {
                let item = items.get(*index).cloned();
                if item.is_some() {
                    *index += 1;
                }
                Ok(item)
            }
            Iter::Protocol {
                iterator,
                next,
                is_async,
            } => {
                let mut result = self.call(next, iterator.clone(), Vec::new()).await?;
                if *is_async {
                    result = self.await_value(result).await?;
                }
                if !matches!(result, Value::Obj(_)) {
                    return Err(self.type_error(format!(
                        "Iterator result {} is not an object",
                        describe_value(&result)
                    )));
                }
                if self.get_str(&result, "done").await?.truthy() {
                    return Ok(None);
                }
                Ok(Some(self.get_str(&result, "value").await?))
            }
            Iter::Done => Ok(None),
        }
    }

    pub(super) async fn iter_close(&self, iter: &mut Iter) {
        if let Iter::Protocol { iterator, .. } = iter
            && let Ok(ret) = self.get_str(iterator, "return").await
            && ret.is_callable()
        {
            let result = self.call(&ret, iterator.clone(), Vec::new()).await;
            if let (Ok(value), Iter::Protocol { is_async: true, .. }) = (result, &*iter) {
                let _ = self.await_value(value).await;
            }
        }
        *iter = Iter::Done;
    }

    pub(super) async fn iterate_to_vec(&self, value: &Value) -> R<Vec<Value>> {
        if let Value::Obj(obj) = value
            && let Some(items) = obj.array_items()
            && !obj
                .borrow()
                .props
                .contains_key(&PropKey::Sym(self.sym_iterator.clone()))
        {
            return Ok(items);
        }
        let mut iter = self.get_iterator(value, false).await?;
        let mut out = Vec::new();
        while let Some(item) = self.iter_step(&mut iter).await? {
            out.push(item);
            self.check_array_len(out.len())?;
            self.tick()?;
        }
        Ok(out)
    }

    /// Creates an iterator result object `{ value, done }`.
    pub(super) fn iter_result(&self, value: Value, done: bool) -> Value {
        let obj = self.new_object();
        obj.set_data("value", value);
        obj.set_data("done", Value::Bool(done));
        Value::Obj(obj)
    }

    // ----- program ---------------------------------------------------------------------

    /// Runs the program body as an implicit async function. The value of a trailing
    /// expression statement is the result when there is no explicit `return`.
    pub(super) async fn run_program(&self, program: &Program) -> R<Value> {
        let ctx = Rc::new(FnCtx {
            this: RefCell::new(Some(Value::Undefined)),
            home: None,
            new_target: None,
            func: None,
            args: RefCell::new(Vec::new()),
            gen_slot: None,
        });
        let env = Scope::new(None, Some(ctx));
        for name in &program.var_names {
            env.declare_var(name);
        }
        self.hoist(&program.body, &env);
        let last = program.body.len().saturating_sub(1);
        let mut result = Value::Undefined;
        for (index, stmt) in program.body.iter().enumerate() {
            if index == last
                && let Stmt::Expr(expr) = stmt
            {
                self.tick()?;
                result = self.ev(expr, &env).await?;
                break;
            }
            match self.exec(stmt, &env, &[]).await? {
                Completion::Return(value) => return Ok(value),
                Completion::Normal => {}
                Completion::Break(_) | Completion::Continue(_) => {
                    return Err(
                        self.throw(ErrorKind::Syntax, "Illegal break or continue statement")
                    );
                }
            }
        }
        Ok(result)
    }
}

pub(super) enum Iter {
    Array {
        array: Obj,
        index: usize,
    },
    List {
        items: Vec<Value>,
        index: usize,
    },
    Protocol {
        iterator: Value,
        next: Value,
        is_async: bool,
    },
    Done,
}

fn loop_label_matches(label: &Option<JsStr>, labels: &[JsStr]) -> bool {
    match label {
        None => true,
        Some(label) => labels.contains(label),
    }
}

fn collect_names(pattern: &Pattern, out: &mut Vec<JsStr>) {
    match pattern {
        Pattern::Ident(name) => out.push(name.clone()),
        Pattern::Object(props, rest) => {
            for prop in props {
                collect_names(&prop.value, out);
            }
            if let Some(rest) = rest {
                collect_names(rest, out);
            }
        }
        Pattern::Array(items, rest) => {
            for item in items.iter().flatten() {
                collect_names(item, out);
            }
            if let Some(rest) = rest {
                collect_names(rest, out);
            }
        }
        Pattern::Default(inner, _) => collect_names(inner, out),
        Pattern::Expr(_) => {}
    }
}

fn js_rem(x: f64, y: f64) -> f64 {
    if y == 0.0 || x.is_infinite() || x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if y.is_infinite() {
        return x;
    }
    x % y
}

fn js_pow(x: f64, y: f64) -> f64 {
    if y.is_nan() || ((x == 1.0 || x == -1.0) && y.is_infinite()) {
        return f64::NAN;
    }
    x.powf(y)
}

fn position_suffix(pos: Pos) -> String {
    if pos.line == 0 {
        String::new()
    } else {
        format!(" (line {})", pos.line)
    }
}

/// Short source-like rendering of a callee for error messages.
pub(super) fn describe_expr(expr: &Expr) -> String {
    match expr {
        Expr::Ident(name, _) => name.to_string(),
        Expr::This => "this".into(),
        Expr::Member {
            object,
            key,
            optional,
        } => {
            let base = describe_expr(object);
            let sep = if *optional { "?." } else { "." };
            match key {
                MemberKey::Name(name) => format!("{base}{sep}{name}"),
                MemberKey::Computed(inner) => match &**inner {
                    Expr::Str(s) => format!("{base}[{s:?}]"),
                    Expr::Num(n) => format!("{base}[{}]", number_to_string(*n)),
                    _ => format!("{base}[...]"),
                },
            }
        }
        Expr::Call { callee, .. } => format!("{}(...)", describe_expr(callee)),
        Expr::Chain(inner) => describe_expr(inner),
        Expr::SuperMember(MemberKey::Name(name)) => format!("super.{name}"),
        _ => "expression".into(),
    }
}

pub(super) fn describe_value(value: &Value) -> String {
    match value {
        Value::Str(s) => format!("{:?}", s.as_str()),
        Value::Obj(obj) if obj.is_callable() => "function".into(),
        Value::Obj(obj) if obj.is_array() => "array".into(),
        Value::Obj(_) => "object".into(),
        other => format!("{other:?}"),
    }
}

/// Polls a future once immediately so synchronous prefixes run in program order, then hands
/// any remainder to the local executor.
pub(super) fn spawn_eager<F: Future<Output = ()> + 'static>(future: F) {
    let mut future = Box::pin(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    if future.as_mut().poll(&mut context).is_pending() {
        tokio::task::spawn_local(future);
    }
}
