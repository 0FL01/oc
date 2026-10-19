//! Native Execute presentation observations. Local search remains interpreter-local;
//! external requests retain their existing admission, reply and cleanup semantics.

use std::cell::Cell;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use serde_json::Value as Json;
use tokio::sync::mpsc;

use super::host::{HostEvent, Transport};
use super::{
    Catalog, Diagnostic, ExecuteLimits, ExecutionOwner, FetchRequest, HostRequest,
    MAX_CONCURRENT_REQUESTS, Outcome,
};

#[cfg(test)]
mod tests;

pub(crate) enum ObservedCall<'a> {
    Tool {
        path: &'a str,
        input: &'a Json,
    },
    Fetch(&'a FetchRequest),
    LocalSearch {
        input: &'a Json,
        input_limited: bool,
    },
    CallsLimited,
}

struct ObservedExecution {
    events: mpsc::UnboundedReceiver<HostEvent>,
    owner: ExecutionOwner,
}

impl ObservedExecution {
    fn start(code: String, catalog: Arc<Catalog>, limits: ExecuteLimits) -> Self {
        let (sender, events) = mpsc::unbounded_channel();
        Self {
            events,
            owner: ExecutionOwner::start(
                code,
                catalog,
                limits,
                Transport::Observed {
                    sender,
                    starts: Cell::new(0),
                },
            ),
        }
    }

    async fn run<O, H, F, T>(
        mut self,
        cancel: &AtomicBool,
        mut observe: O,
        mut handle: H,
    ) -> Outcome
    where
        O: FnMut(ObservedCall<'_>) -> Option<T>,
        H: FnMut(HostRequest, Option<T>) -> F,
        F: Future<Output = ()>,
    {
        if self.owner.thread.is_none() {
            return Outcome::failure(Diagnostic::new(
                "ExecutionFailure",
                "Code Mode runtime could not start a thread.",
            ));
        }
        let mut pending = FuturesUnordered::new();
        let mut queued = std::collections::VecDeque::new();
        let mut tick = tokio::time::interval(Duration::from_millis(50));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let outcome = loop {
            while pending.len() < MAX_CONCURRENT_REQUESTS {
                let Some((request, token)) = queued.pop_front() else {
                    break;
                };
                // Constructing the effect handler is itself observable. Keep it
                // at the old eight-slot admission point, not at event receipt.
                pending.push(handle(request, token));
            }
            tokio::select! {
                biased;
                outcome = &mut self.owner.outcome => {
                    break outcome.unwrap_or_else(|_| Outcome::failure(Diagnostic::new(
                        "ExecutionFailure",
                        "Code Mode interpreter stopped unexpectedly.",
                    )));
                }
                Some(event) = self.events.recv() => {
                    if let Some(request) = observe_event(event, &mut observe)
                        && !self.owner.cancel.load(Ordering::Relaxed)
                    {
                        queued.push_back(request);
                    }
                }
                Some(()) = pending.next(), if !pending.is_empty() => {}
                _ = tick.tick() => {
                    if cancel.load(Ordering::Relaxed) {
                        self.owner.cancel();
                        pending.clear();
                        queued.clear();
                    }
                }
            }
        };
        drop(pending);
        drop(queued);
        self.owner.join().await;
        // A synchronous search can finish before this receiver is first polled.
        // The producer is now gone: drain finite metadata, never start effects or
        // manufacture completion for requests whose effect was not confirmed.
        while let Ok(event) = self.events.try_recv() {
            drop(observe_event(event, &mut observe));
        }
        outcome
    }
}

fn observe_event<O, T>(event: HostEvent, observe: &mut O) -> Option<(HostRequest, Option<T>)>
where
    O: FnMut(ObservedCall<'_>) -> Option<T>,
{
    match event {
        HostEvent::Request { request, observed } => {
            let token = if observed {
                observe(match &request {
                    HostRequest::Tool { path, input, .. } => ObservedCall::Tool { path, input },
                    HostRequest::Fetch { request, .. } => ObservedCall::Fetch(request),
                })
            } else {
                None
            };
            Some((request, token))
        }
        HostEvent::LocalSearch {
            input,
            input_limited,
        } => {
            observe(ObservedCall::LocalSearch {
                input: &input,
                input_limited,
            });
            None
        }
        HostEvent::CallsLimited => {
            observe(ObservedCall::CallsLimited);
            None
        }
    }
}

pub(crate) async fn run_observed<O, H, F, T>(
    code: String,
    catalog: Arc<Catalog>,
    limits: ExecuteLimits,
    cancel: &AtomicBool,
    observe: O,
    handle: H,
) -> Outcome
where
    O: FnMut(ObservedCall<'_>) -> Option<T>,
    H: FnMut(HostRequest, Option<T>) -> F,
    F: Future<Output = ()>,
{
    ObservedExecution::start(code, catalog, limits)
        .run(cancel, observe, handle)
        .await
}
