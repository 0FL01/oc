//! Promises over the interpreter's single-threaded executor.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use super::interp::{Abrupt, Cx, ErrorKind, Fatal, NativeImpl, R, arg};
use super::value::{Kind, Obj, Value};

enum State {
    Pending(Vec<Waker>),
    Fulfilled(Value),
    Rejected(Value),
    Fatal(Fatal),
}

pub(super) struct PromiseCell {
    state: RefCell<State>,
    pub handled: Cell<bool>,
}

impl PromiseCell {
    pub(super) fn rejection(&self) -> Option<Value> {
        match &*self.state.borrow() {
            State::Rejected(value) => Some(value.clone()),
            _ => None,
        }
    }
}

pub(super) fn new_promise_with_proto(cx: &Cx, proto: Obj) -> R<(Value, Rc<PromiseCell>)> {
    if cx.pending_promises.get() >= cx.limits.max_pending_promises {
        return Err(cx.range_error(format!(
            "Too many pending promises (limit {}); await promises before creating more.",
            cx.limits.max_pending_promises
        )));
    }
    cx.pending_promises.set(cx.pending_promises.get() + 1);
    let cell = Rc::new(PromiseCell {
        state: RefCell::new(State::Pending(Vec::new())),
        handled: Cell::new(false),
    });
    let obj = Obj::new(Some(proto), Kind::Promise(cell.clone()));
    Ok((Value::Obj(obj), cell))
}

pub(super) fn new_promise(cx: &Cx) -> R<(Value, Rc<PromiseCell>)> {
    new_promise_with_proto(cx, cx.protos.promise.clone())
}

fn settle(cx: &Cx, cell: &PromiseCell, state: State) -> bool {
    let wakers = {
        let mut current = cell.state.borrow_mut();
        let State::Pending(wakers) = &mut *current else {
            return false;
        };
        let wakers = std::mem::take(wakers);
        *current = state;
        wakers
    };
    cx.pending_promises
        .set(cx.pending_promises.get().saturating_sub(1));
    for waker in wakers {
        waker.wake();
    }
    true
}

pub(super) fn fulfill(cx: &Cx, cell: &Rc<PromiseCell>, value: Value) {
    settle(cx, cell, State::Fulfilled(value));
}

pub(super) fn reject(cx: &Cx, cell: &Rc<PromiseCell>, error: Value) {
    if settle(cx, cell, State::Rejected(error)) && !cell.handled.get() {
        cx.rejected.borrow_mut().push(cell.clone());
    }
}

pub(super) fn fatal(cell: &Rc<PromiseCell>, fatal: Fatal) {
    let wakers = {
        let mut current = cell.state.borrow_mut();
        let State::Pending(wakers) = &mut *current else {
            return;
        };
        let wakers = std::mem::take(wakers);
        *current = State::Fatal(fatal);
        wakers
    };
    for waker in wakers {
        waker.wake();
    }
}

fn promise_cell(value: &Value) -> Option<Rc<PromiseCell>> {
    match value {
        Value::Obj(obj) => match &obj.borrow().kind {
            Kind::Promise(cell) => Some(cell.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Settles `cell` with `value`, adopting the state of promises and thenables.
pub(super) fn resolve_value(cx: &Cx, cell: &Rc<PromiseCell>, value: Value) {
    if let Some(source) = promise_cell(&value) {
        if Rc::ptr_eq(&source, cell) {
            let error = cx.make_error(ErrorKind::Type, "Chaining cycle detected for promise");
            reject(cx, cell, Value::Obj(error));
            return;
        }
        source.handled.set(true);
        let cx = cx.clone();
        let cell = cell.clone();
        tokio::task::spawn_local(async move {
            let result = wait(&cx, &source).await;
            settle_with(&cx, &cell, result);
        });
        return;
    }
    if let Value::Obj(obj) = &value
        && !obj.is_callable()
        && obj
            .borrow()
            .props
            .contains_key(&super::value::PropKey::from("then"))
    {
        let cx = cx.clone();
        let cell = cell.clone();
        tokio::task::spawn_local(async move {
            let result = cx.await_value(value).await;
            settle_with(&cx, &cell, result);
        });
        return;
    }
    if let Value::Obj(obj) = &value
        && obj
            .proto()
            .is_some_and(|p| !p.ptr_eq(&cx.protos.object) && !p.ptr_eq(&cx.protos.array))
    {
        // Instances may inherit `then` from a class; check it asynchronously.
        let cx = cx.clone();
        let cell = cell.clone();
        tokio::task::spawn_local(async move {
            let result = cx.await_value(value).await;
            settle_with(&cx, &cell, result);
        });
        return;
    }
    fulfill(cx, cell, value);
}

pub(super) fn settle_with(cx: &Cx, cell: &Rc<PromiseCell>, result: R<Value>) {
    match result {
        Ok(value) => fulfill(cx, cell, value),
        Err(Abrupt::Throw(error)) => reject(cx, cell, error),
        Err(Abrupt::Fatal(f)) => fatal(cell, f),
        Err(Abrupt::Short | Abrupt::GenReturn(_)) => fulfill(cx, cell, Value::Undefined),
    }
}

struct Wait(Rc<PromiseCell>);

impl Future for Wait {
    type Output = R<Value>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.0.state.borrow_mut();
        match &mut *state {
            State::Pending(wakers) => {
                if !wakers.iter().any(|w| w.will_wake(cx.waker())) {
                    wakers.push(cx.waker().clone());
                }
                Poll::Pending
            }
            State::Fulfilled(value) => Poll::Ready(Ok(value.clone())),
            State::Rejected(error) => Poll::Ready(Err(Abrupt::Throw(error.clone()))),
            State::Fatal(fatal) => Poll::Ready(Err(Abrupt::Fatal(*fatal))),
        }
    }
}

pub(super) async fn wait(_cx: &Cx, cell: &Rc<PromiseCell>) -> R<Value> {
    cell.handled.set(true);
    Wait(cell.clone()).await
}

/// Starts `work` immediately and returns a promise for its result.
pub(super) fn spawn_promise<F>(cx: &Cx, work: F) -> R<Value>
where
    F: Future<Output = R<Value>> + 'static,
{
    let (promise, cell) = new_promise(cx)?;
    let task_cx = cx.clone();
    super::interp::spawn_eager(async move {
        let result = work.await;
        settle_with(&task_cx, &cell, result);
    });
    Ok(promise)
}

/// JavaScript-visible `resolve`/`reject` pair for `cell`.
pub(super) fn resolving_functions(cx: &Cx, cell: &Rc<PromiseCell>) -> (Value, Value) {
    let done = Rc::new(Cell::new(false));
    let (c1, d1) = (cell.clone(), done.clone());
    let resolve = cx.native("resolve", 1, move |cx, _, args| {
        if !d1.replace(true) {
            resolve_value(cx, &c1, arg(args, 0));
        }
        Ok(Value::Undefined)
    });
    let (c2, d2) = (cell.clone(), done);
    let reject_fn = cx.native("reject", 1, move |cx, _, args| {
        if !d2.replace(true) {
            reject(cx, &c2, arg(args, 0));
        }
        Ok(Value::Undefined)
    });
    (Value::Obj(resolve), Value::Obj(reject_fn))
}

fn this_promise(cx: &Cx, this: &Value) -> R<Rc<PromiseCell>> {
    promise_cell(this).ok_or_else(|| {
        cx.type_error("Method Promise.prototype.then called on incompatible receiver")
    })
}

/// `promise.then(onFulfilled, onRejected)`.
pub(super) fn then(
    cx: &Cx,
    source: Rc<PromiseCell>,
    on_fulfilled: Value,
    on_rejected: Value,
) -> R<Value> {
    source.handled.set(true);
    let (derived, cell) = new_promise(cx)?;
    let cx = cx.clone();
    tokio::task::spawn_local(async move {
        let result = wait(&cx, &source).await;
        let next = match result {
            Ok(value) if on_fulfilled.is_callable() => {
                cx.call(&on_fulfilled, Value::Undefined, vec![value]).await
            }
            Err(Abrupt::Throw(error)) if on_rejected.is_callable() => {
                cx.call(&on_rejected, Value::Undefined, vec![error]).await
            }
            other => other,
        };
        match next {
            Ok(value) => resolve_value(&cx, &cell, value),
            other => settle_with(&cx, &cell, other),
        }
    });
    Ok(derived)
}

/// Converts any value into a promise cell (`Promise.resolve` semantics).
pub(super) fn to_promise(cx: &Cx, value: Value) -> R<Rc<PromiseCell>> {
    if let Some(cell) = promise_cell(&value) {
        return Ok(cell);
    }
    let (_, cell) = new_promise(cx)?;
    resolve_value(cx, &cell, value);
    Ok(cell)
}

enum Combinator {
    All,
    AllSettled,
    Race,
    Any,
}

async fn combinator(cx: Cx, kind: Combinator, iterable: Value) -> R<Value> {
    let items = cx.iterate_to_vec(&iterable).await?;
    let (derived, cell) = new_promise(&cx)?;
    let count = items.len();
    if count == 0 {
        match kind {
            Combinator::All | Combinator::AllSettled => {
                fulfill(&cx, &cell, cx.array_value(Vec::new()))
            }
            Combinator::Any => {
                let error = cx.make_error(ErrorKind::Aggregate, "All promises were rejected");
                error.set_hidden("errors", cx.array_value(Vec::new()));
                reject(&cx, &cell, Value::Obj(error));
            }
            Combinator::Race => {}
        }
        return Ok(derived);
    }
    let results = Rc::new(RefCell::new(vec![Value::Undefined; count]));
    let remaining = Rc::new(Cell::new(count));
    let kind = Rc::new(kind);
    for (index, item) in items.into_iter().enumerate() {
        let source = to_promise(&cx, item)?;
        source.handled.set(true);
        let (cx, cell, results, remaining, kind) = (
            cx.clone(),
            cell.clone(),
            results.clone(),
            remaining.clone(),
            kind.clone(),
        );
        tokio::task::spawn_local(async move {
            let result = wait(&cx, &source).await;
            let finish = |cx: &Cx, cell: &Rc<PromiseCell>, results: &RefCell<Vec<Value>>| {
                remaining.set(remaining.get() - 1);
                if remaining.get() == 0 {
                    let values = std::mem::take(&mut *results.borrow_mut());
                    match *kind {
                        Combinator::Any => {
                            let error =
                                cx.make_error(ErrorKind::Aggregate, "All promises were rejected");
                            error.set_hidden("errors", cx.array_value(values));
                            reject(cx, cell, Value::Obj(error));
                        }
                        _ => fulfill(cx, cell, cx.array_value(values)),
                    }
                }
            };
            match (&*kind, result) {
                (_, Err(Abrupt::Fatal(f))) => fatal(&cell, f),
                (Combinator::All, Ok(value)) => {
                    results.borrow_mut()[index] = value;
                    finish(&cx, &cell, &results);
                }
                (Combinator::All | Combinator::Race, Err(Abrupt::Throw(error))) => {
                    reject(&cx, &cell, error)
                }
                (Combinator::Race | Combinator::Any, Ok(value)) => fulfill(&cx, &cell, value),
                (Combinator::AllSettled, result) => {
                    let entry = cx.new_object();
                    match result {
                        Ok(value) => {
                            entry.set_data("status", Value::str("fulfilled"));
                            entry.set_data("value", value);
                        }
                        Err(Abrupt::Throw(error)) => {
                            entry.set_data("status", Value::str("rejected"));
                            entry.set_data("reason", error);
                        }
                        Err(_) => entry.set_data("status", Value::str("rejected")),
                    }
                    results.borrow_mut()[index] = Value::Obj(entry);
                    finish(&cx, &cell, &results);
                }
                (Combinator::Any, Err(Abrupt::Throw(error))) => {
                    results.borrow_mut()[index] = error;
                    finish(&cx, &cell, &results);
                }
                (_, Err(_)) => fulfill(&cx, &cell, Value::Undefined),
            }
        });
    }
    Ok(derived)
}

pub(super) fn install(cx: &Cx) {
    let proto = cx.protos.promise.clone();
    let construct: NativeImpl = NativeImpl::Async(Box::new(|cx, new_target, args| {
        Box::pin(async move {
            let executor = arg(&args, 0);
            if !executor.is_callable() {
                return Err(cx.type_error("Promise resolver is not a function"));
            }
            let proto = cx.proto_from(&new_target, &cx.protos.promise);
            let (promise, cell) = new_promise_with_proto(&cx, proto)?;
            let (resolve, reject_fn) = resolving_functions(&cx, &cell);
            match cx
                .call(
                    &executor,
                    Value::Undefined,
                    vec![resolve, reject_fn.clone()],
                )
                .await
            {
                Ok(_) => {}
                Err(Abrupt::Throw(error)) => {
                    cx.call(&reject_fn, Value::Undefined, vec![error]).await?;
                }
                Err(other) => return Err(other),
            }
            Ok(promise)
        })
    }));
    let ctor = cx.constructor("Promise", 1, &proto, None, Some(construct));
    cx.method(&proto, "then", 2, |cx, this, args| {
        let source = this_promise(cx, this)?;
        then(cx, source, arg(args, 0), arg(args, 1))
    });
    cx.method(&proto, "catch", 1, |cx, this, args| {
        let source = this_promise(cx, this)?;
        then(cx, source, Value::Undefined, arg(args, 0))
    });
    cx.method(&proto, "finally", 1, |cx, this, args| {
        let source = this_promise(cx, this)?;
        source.handled.set(true);
        let callback = arg(args, 0);
        let (derived, cell) = new_promise(cx)?;
        let cx = cx.clone();
        tokio::task::spawn_local(async move {
            let result = wait(&cx, &source).await;
            if callback.is_callable() {
                match cx.call(&callback, Value::Undefined, Vec::new()).await {
                    Ok(value) => {
                        if let Err(err) = cx.await_value(value).await {
                            settle_with(&cx, &cell, Err(err));
                            return;
                        }
                    }
                    Err(err) => {
                        settle_with(&cx, &cell, Err(err));
                        return;
                    }
                }
            }
            settle_with(&cx, &cell, result);
        });
        Ok(derived)
    });
    let to_string_tag = cx.sym_to_string_tag.clone();
    proto.borrow_mut().props.insert(
        super::value::PropKey::Sym(to_string_tag),
        super::value::Prop::hidden(Value::str("Promise")),
    );
    cx.method(&ctor, "resolve", 1, |cx, _, args| {
        let value = arg(args, 0);
        if promise_cell(&value).is_some() {
            return Ok(value);
        }
        let (promise, cell) = new_promise(cx)?;
        resolve_value(cx, &cell, value);
        Ok(promise)
    });
    cx.method(&ctor, "reject", 1, |cx, _, args| {
        let (promise, cell) = new_promise(cx)?;
        reject(cx, &cell, arg(args, 0));
        Ok(promise)
    });
    cx.method(&ctor, "withResolvers", 0, |cx, _, _| {
        let (promise, cell) = new_promise(cx)?;
        let (resolve, reject_fn) = resolving_functions(cx, &cell);
        let out = cx.new_object();
        out.set_data("promise", promise);
        out.set_data("resolve", resolve);
        out.set_data("reject", reject_fn);
        Ok(Value::Obj(out))
    });
    cx.method_async(&ctor, "all", 1, |cx, _, args| {
        Box::pin(combinator(cx, Combinator::All, arg(&args, 0)))
    });
    cx.method_async(&ctor, "allSettled", 1, |cx, _, args| {
        Box::pin(combinator(cx, Combinator::AllSettled, arg(&args, 0)))
    });
    cx.method_async(&ctor, "race", 1, |cx, _, args| {
        Box::pin(combinator(cx, Combinator::Race, arg(&args, 0)))
    });
    cx.method_async(&ctor, "any", 1, |cx, _, args| {
        Box::pin(combinator(cx, Combinator::Any, arg(&args, 0)))
    });
    cx.define_global("Promise", Value::Obj(ctor));
}
