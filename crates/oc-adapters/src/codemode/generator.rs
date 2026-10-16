//! Generators: the function body is a suspended future that hands values through a slot.

use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use futures_util::future::LocalBoxFuture;

use super::interp::{Abrupt, Cx, FnCtx, R, arg};
use super::promise;
use super::value::{Kind, Obj, UserFn, Value};

pub(super) enum Resume {
    Next(Value),
    Throw(Value),
    Return(Value),
}

#[derive(Default)]
pub(super) struct GenSlot {
    to_body: RefCell<Option<Resume>>,
    from_body: RefCell<Option<Value>>,
    is_async: Cell<bool>,
}

pub(super) struct GenState {
    body: RefCell<Option<LocalBoxFuture<'static, R<Value>>>>,
    slot: Rc<GenSlot>,
    started: Cell<bool>,
    running: Cell<bool>,
    is_async: bool,
}

pub(super) fn create(
    cx: &Cx,
    user: Rc<UserFn>,
    func: &Obj,
    ctx: Option<Rc<FnCtx>>,
    args: Vec<Value>,
    slot: Rc<GenSlot>,
) -> Value {
    let is_async = user.def.is_async;
    slot.is_async.set(is_async);
    let body_cx = cx.clone();
    let body: LocalBoxFuture<'static, R<Value>> =
        Box::pin(async move { body_cx.run_function_opt(&user, ctx, args).await });
    let state = Rc::new(GenState {
        body: RefCell::new(Some(body)),
        slot,
        started: Cell::new(false),
        running: Cell::new(false),
        is_async,
    });
    let fallback = if is_async {
        cx.protos.async_generator.clone()
    } else {
        cx.protos.generator.clone()
    };
    let proto = match func.own_data("prototype") {
        Some(Value::Obj(p)) => p,
        _ => fallback,
    };
    if is_async && !proto.ptr_eq(&cx.protos.async_generator) {
        proto.borrow_mut().proto = Some(cx.protos.async_generator.clone());
    }
    Value::Obj(Obj::new(Some(proto), Kind::Generator(state)))
}

struct YieldWait(Rc<GenSlot>);

impl Future for YieldWait {
    type Output = Resume;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Resume> {
        match self.0.to_body.borrow_mut().take() {
            Some(resume) => Poll::Ready(resume),
            None => Poll::Pending,
        }
    }
}

pub(super) async fn yield_value(cx: &Cx, slot: &Rc<GenSlot>, value: Value) -> R<Value> {
    let value = if slot.is_async.get() {
        cx.await_value(value).await?
    } else {
        value
    };
    *slot.from_body.borrow_mut() = Some(value);
    match YieldWait(slot.clone()).await {
        Resume::Next(value) => Ok(value),
        Resume::Throw(error) => Err(Abrupt::Throw(error)),
        Resume::Return(value) => Err(Abrupt::GenReturn(value)),
    }
}

pub(super) async fn yield_delegate(cx: &Cx, slot: &Rc<GenSlot>, iterable: Value) -> R<Value> {
    let mut iter = cx.get_iterator(&iterable, false).await?;
    while let Some(value) = cx.iter_step(&mut iter).await? {
        Box::pin(yield_value(cx, slot, value)).await?;
    }
    Ok(Value::Undefined)
}

enum Step {
    Yielded(Value),
    Done(R<Value>),
}

async fn drive(state: &Rc<GenState>, resume: Option<Resume>) -> Step {
    let Some(mut body) = state.body.borrow_mut().take() else {
        return Step::Done(Ok(Value::Undefined));
    };
    if let Some(resume) = resume {
        *state.slot.to_body.borrow_mut() = Some(resume);
    }
    state.running.set(true);
    let step = poll_fn(|cx| {
        if let Some(value) = state.slot.from_body.borrow_mut().take() {
            return Poll::Ready(Step::Yielded(value));
        }
        match body.as_mut().poll(cx) {
            Poll::Ready(result) => Poll::Ready(Step::Done(result)),
            Poll::Pending => match state.slot.from_body.borrow_mut().take() {
                Some(value) => Poll::Ready(Step::Yielded(value)),
                None => Poll::Pending,
            },
        }
    })
    .await;
    state.running.set(false);
    if let Step::Yielded(_) = step {
        *state.body.borrow_mut() = Some(body);
    }
    step
}

async fn resume(cx: &Cx, state: &Rc<GenState>, mode: Resume) -> R<Value> {
    if state.running.get() {
        return Err(cx.type_error("Generator is already running"));
    }
    if !state.started.get() {
        state.started.set(true);
        match mode {
            Resume::Next(_) => {}
            Resume::Return(value) => {
                state.body.borrow_mut().take();
                return Ok(cx.iter_result(value, true));
            }
            Resume::Throw(error) => {
                state.body.borrow_mut().take();
                return Err(Abrupt::Throw(error));
            }
        }
        return finish(cx, drive(state, None).await);
    }
    if state.body.borrow().is_none() {
        return match mode {
            Resume::Next(_) => Ok(cx.iter_result(Value::Undefined, true)),
            Resume::Return(value) => Ok(cx.iter_result(value, true)),
            Resume::Throw(error) => Err(Abrupt::Throw(error)),
        };
    }
    finish(cx, drive(state, Some(mode)).await)
}

fn finish(cx: &Cx, step: Step) -> R<Value> {
    match step {
        Step::Yielded(value) => Ok(cx.iter_result(value, false)),
        Step::Done(Ok(value)) => Ok(cx.iter_result(value, true)),
        Step::Done(Err(Abrupt::GenReturn(value))) => Ok(cx.iter_result(value, true)),
        Step::Done(Err(err)) => Err(err),
    }
}

fn this_state(cx: &Cx, this: &Value) -> R<Rc<GenState>> {
    if let Value::Obj(obj) = this
        && let Kind::Generator(state) = &obj.borrow().kind
    {
        return Ok(state.clone());
    }
    Err(cx.type_error("next method called on incompatible receiver"))
}

fn method(cx: &Cx, proto: &Obj, name: &str, make: fn(Value) -> Resume) {
    cx.method_async(proto, name, 1, move |cx, this, args| {
        Box::pin(async move {
            let state = this_state(&cx, &this)?;
            let mode = make(arg(&args, 0));
            if state.is_async {
                let (promise_value, cell) = promise::new_promise(&cx)?;
                let task_cx = cx.clone();
                tokio::task::spawn_local(async move {
                    let result = resume(&task_cx, &state, mode).await;
                    promise::settle_with(&task_cx, &cell, result);
                });
                return Ok(promise_value);
            }
            resume(&cx, &state, mode).await
        })
    });
}

pub(super) fn install(cx: &Cx) {
    for proto in [
        cx.protos.generator.clone(),
        cx.protos.async_generator.clone(),
    ] {
        method(cx, &proto, "next", Resume::Next);
        method(cx, &proto, "return", Resume::Return);
        method(cx, &proto, "throw", Resume::Throw);
    }
    cx.method_sym(
        &cx.protos.iterator,
        &cx.sym_iterator.clone(),
        "[Symbol.iterator]",
        |_, this, _| Ok(this.clone()),
    );
    cx.method_sym(
        &cx.protos.async_generator,
        &cx.sym_async_iterator.clone(),
        "[Symbol.asyncIterator]",
        |_, this, _| Ok(this.clone()),
    );
}
