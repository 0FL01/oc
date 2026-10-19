use std::cell::RefCell;

use oc_core::tool_output::{EXECUTE_CALLS_CAP, EXECUTE_INPUT_BYTES};
use serde_json::json;
use tokio::sync::oneshot;

use super::super::{CodeModeTool, FetchResponse};
use super::*;

fn catalog(root_search: bool) -> Arc<Catalog> {
    let mut tools = vec![CodeModeTool {
        path: "fixture.echo".into(),
        description: "A fixture call".into(),
        input_schema: json!({"type":"object"}),
        output_schema: None,
    }];
    if root_search {
        tools.push(CodeModeTool {
            path: "search".into(),
            ..tools[0].clone()
        });
    }
    Arc::new(Catalog::new(tools, Vec::new()))
}

#[derive(Debug)]
struct Captured {
    tool: String,
    input: Json,
    limited: bool,
}

fn capture(
    call: ObservedCall<'_>,
    calls: &RefCell<Vec<Captured>>,
    limited: &Cell<usize>,
) -> Option<usize> {
    let (tool, input, input_limited) = match call {
        ObservedCall::Tool { path, input } => (path, input.clone(), false),
        ObservedCall::Fetch(request) => (
            "fetch",
            json!({"method":request.method,"url":request.url}),
            false,
        ),
        ObservedCall::LocalSearch {
            input,
            input_limited,
        } => ("search", input.clone(), input_limited),
        ObservedCall::CallsLimited => {
            limited.set(limited.get() + 1);
            return None;
        }
    };
    let mut calls = calls.borrow_mut();
    let index = calls.len();
    calls.push(Captured {
        tool: tool.into(),
        input,
        limited: input_limited,
    });
    Some(index)
}

async fn local(
    code: &str,
    limits: ExecuteLimits,
    root_search: bool,
) -> (Outcome, Vec<Captured>, usize) {
    let calls = RefCell::new(Vec::new());
    let limited = Cell::new(0);
    let handlers = Cell::new(0);
    let outcome = tokio::time::timeout(
        Duration::from_secs(20),
        run_observed(
            code.into(),
            catalog(root_search),
            limits,
            &AtomicBool::new(false),
            |call| capture(call, &calls, &limited),
            |request, _| {
                handlers.set(handlers.get() + 1);
                async move {
                    match request {
                        HostRequest::Tool { input, reply, .. } => {
                            let _ = reply.send(Ok(input));
                        }
                        HostRequest::Fetch { .. } => panic!("local fixture must not fetch"),
                    }
                }
            },
        ),
    )
    .await
    .expect("observed execution finished");
    assert_eq!(limited.get(), 0);
    (outcome, calls.into_inner(), handlers.get())
}

// Awaiting the real outcome makes outcome-first scheduling deterministic. Re-arm
// only its delivery; do not consume the queued events or invoke effect handlers.
async fn completed(code: &str, limits: ExecuteLimits) -> (ObservedExecution, Outcome) {
    let mut execution = ObservedExecution::start(code.into(), catalog(false), limits);
    let outcome = tokio::time::timeout(Duration::from_secs(20), &mut execution.owner.outcome)
        .await
        .expect("interpreter finished without an observation consumer")
        .unwrap();
    let (done, result) = oneshot::channel();
    done.send(outcome.clone()).unwrap();
    execution.owner.outcome = result;
    (execution, outcome)
}

#[tokio::test]
async fn observed_search_only_final_drain_preserves_result_and_supplied_order() {
    let (execution, before) = completed(
        "const r = search({query:'call',namespace:'fixture',limit:12,offset:0}); return r.items.length;",
        ExecuteLimits::default(),
    ).await;
    let calls = RefCell::new(Vec::new());
    let limited = Cell::new(0);
    let outcome = execution
        .run(
            &AtomicBool::new(false),
            |call| capture(call, &calls, &limited),
            |_, _| -> std::future::Ready<()> {
                panic!("final drain must not construct an effect handler")
            },
        )
        .await;
    assert_eq!(outcome, before);
    assert_eq!(outcome.value, Ok(json!(1)));
    assert_eq!(outcome.tool_calls, 1);
    let calls = calls.into_inner();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool, "search");
    assert!(!calls[0].limited);
    assert_eq!(
        calls[0].input,
        json!({"query":"call","namespace":"fixture","limit":12,"offset":0})
    );
    assert_eq!(
        calls[0]
            .input
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["query", "namespace", "limit", "offset"]
    );
    assert_eq!(limited.get(), 0);
}

#[tokio::test]
async fn observed_search_identity_validation_and_conversion_remain_local() {
    for code in [
        "return search({namespace:'missing'})",
        "return tools.search({namespace:'missing'})",
    ] {
        let (outcome, calls, handlers) = local(code, ExecuteLimits::default(), false).await;
        assert_eq!(
            outcome.value,
            Ok(json!({"items":[],"remaining":0,"next":null}))
        );
        assert_eq!(outcome.tool_calls, 1);
        assert_eq!(calls.len(), 1);
        assert_eq!(handlers, 0);
    }
    let (outcome, calls, handlers) = local(
        "let reads=0; const input={get query(){reads++; return 'call'},ignored:'not displayed'}; search(input); return reads;",
        ExecuteLimits::default(), false,
    ).await;
    assert_eq!(
        outcome.value,
        Ok(json!(1)),
        "capture must not evaluate an input getter twice"
    );
    assert_eq!(handlers, 0);
    assert_eq!(calls[0].input, json!({"query":"call"}));
    assert!(calls[0].limited);

    let (outcome, calls, handlers) = local(
        "return await tools.search({query:'external'})",
        ExecuteLimits::default(),
        true,
    )
    .await;
    assert_eq!(outcome.value, Ok(json!({"query":"external"})));
    assert_eq!(handlers, 1);
    assert_eq!(
        calls.len(),
        1,
        "a registered search must not also emit local discovery"
    );
    let (outcome, calls, handlers) = local(
        "const search=()=>7; return search()",
        ExecuteLimits::default(),
        false,
    )
    .await;
    assert_eq!(outcome.value, Ok(json!(7)));
    assert!(calls.is_empty());
    assert_eq!(handlers, 0);

    for code in [
        "search({}, {})",
        "search(1)",
        "search({query:1})",
        "search({namespace:false})",
        "search({limit:0})",
        "search({limit:1.5})",
        "search({offset:-1})",
        "await tools.server.search({})",
    ] {
        let (outcome, calls, handlers) = local(code, ExecuteLimits::default(), false).await;
        assert!(outcome.value.is_err(), "{code}");
        assert_eq!(outcome.tool_calls, 0, "{code}");
        assert!(
            calls.is_empty(),
            "rejected input is not an executed search: {code}"
        );
        assert_eq!(handlers, 0);
    }
    let (outcome, calls, _) = local(
        "search(); search();",
        ExecuteLimits {
            max_tool_calls: Some(1),
            ..ExecuteLimits::default()
        },
        false,
    )
    .await;
    assert_eq!(outcome.value.unwrap_err().kind, "ToolCallLimitExceeded");
    assert_eq!(outcome.tool_calls, 1);
    assert_eq!(calls.len(), 1);
}

#[tokio::test]
async fn observed_search_backlog_is_bounded_before_the_consumer_runs() {
    for count in [EXECUTE_CALLS_CAP, EXECUTE_CALLS_CAP + 1, 64] {
        let code = format!(
            "for(let i=0;i<{count};i++) search({{query:'call',limit:12}}); return {count};"
        );
        let (mut execution, outcome) = completed(&code, ExecuteLimits::default()).await;
        execution.owner.join().await;
        assert_eq!(outcome.value, Ok(json!(count)));
        assert_eq!(
            outcome.tool_calls as usize, count,
            "retention is not an execution cap"
        );
        let mut searches = 0;
        let mut overflow = 0;
        while let Ok(event) = execution.events.try_recv() {
            match event {
                HostEvent::LocalSearch {
                    input,
                    input_limited,
                } => {
                    searches += 1;
                    assert!(!input_limited);
                    assert!(input.to_string().len() <= EXECUTE_INPUT_BYTES);
                }
                HostEvent::CallsLimited => overflow += 1,
                HostEvent::Request { .. } => panic!("local discovery has no external request"),
            }
        }
        assert_eq!(searches, EXECUTE_CALLS_CAP);
        assert_eq!(overflow, usize::from(count > EXECUTE_CALLS_CAP));
    }
    for (value, retained) in [
        ("x".repeat(500), true),
        ("x".repeat(501), false),
        ("\"\n界".repeat(512), false),
    ] {
        let code = format!(
            "return search({{query:{},namespace:null,limit:12,offset:0,ignored:'secret'}})",
            json!(value)
        );
        let (_, calls, handlers) = local(&code, ExecuteLimits::default(), false).await;
        assert_eq!(handlers, 0);
        let call = &calls[0];
        assert!(call.limited);
        assert!(call.input.to_string().len() <= EXECUTE_INPUT_BYTES);
        assert_eq!(
            call.input.get("query"),
            retained.then(|| json!(value)).as_ref()
        );
        if !retained {
            assert_eq!(call.input["namespace"], Json::Null);
            assert_eq!(call.input["limit"], 12);
            assert_eq!(call.input["offset"], 0);
        }
        assert!(call.input.get("ignored").is_none());
    }
}

#[tokio::test]
async fn observed_fifo_precedes_eight_slot_admission_and_caps_only_presentation() {
    let calls = RefCell::new(Vec::new());
    let limited = Cell::new(0);
    let tools_seen = Cell::new(0);
    let (ready, gate) = tokio::sync::watch::channel(false);
    let barrier = tokio::sync::Barrier::new(8);
    let active = Cell::new(0usize);
    let peak = Cell::new(0);
    let admitted = RefCell::new(Vec::new());
    let code = "const work=Array.from({length:12},(_,i)=>(async()=>{await tools.fixture.echo({index:i});search({query:'call'});return i})());
        const values=await Promise.all(work);await tools.fixture.echo({index:12});await fetch('https://fixture.test/');return values;";
    let outcome = tokio::time::timeout(
        Duration::from_secs(20),
        run_observed(
            code.into(),
            catalog(false),
            ExecuteLimits::default(),
            &AtomicBool::new(false),
            |call| {
                if matches!(call, ObservedCall::Tool { .. }) {
                    tools_seen.set(tools_seen.get() + 1);
                    if tools_seen.get() == 12 {
                        ready.send_replace(true);
                    }
                }
                capture(call, &calls, &limited)
            },
            |request, token| {
                let mut gate = gate.clone();
                let active = &active;
                let peak = &peak;
                let barrier = &barrier;
                admitted.borrow_mut().push(token);
                async move {
                    active.set(active.get() + 1);
                    peak.set(peak.get().max(active.get()));
                    match request {
                        HostRequest::Tool { input, reply, .. } => {
                            let index = input["index"].as_u64().unwrap();
                            if index < 8 {
                                barrier.wait().await;
                            }
                            while !*gate.borrow_and_update() {
                                gate.changed().await.unwrap();
                            }
                            active.set(active.get() - 1);
                            let _ = reply.send(Ok(input));
                        }
                        HostRequest::Fetch { request, reply } => {
                            active.set(active.get() - 1);
                            let _ = reply.send(Ok(FetchResponse {
                                url: request.url,
                                status: 200,
                                status_text: "OK".into(),
                                redirected: false,
                                headers: Vec::new(),
                                body: Vec::new(),
                            }));
                        }
                    }
                }
            },
        ),
    )
    .await
    .expect("queued effects and local discovery settle without admission deadlock");
    assert_eq!(outcome.value, Ok(json!((0..12).collect::<Vec<_>>())));
    assert_eq!(
        outcome.tool_calls, 25,
        "fetch keeps its pre-existing separate accounting"
    );
    assert_eq!(peak.get(), 8);
    assert_eq!(active.get(), 0);
    let calls = calls.into_inner();
    assert_eq!(calls.len(), EXECUTE_CALLS_CAP);
    assert!(calls[..12].iter().all(|call| call.tool == "fixture.echo"));
    assert!(calls[12..].iter().all(|call| call.tool == "search"));
    assert_eq!(limited.get(), 1);
    let admitted = admitted.into_inner();
    assert_eq!(&admitted[..12], &(0..12).map(Some).collect::<Vec<_>>());
    assert_eq!(
        &admitted[12..],
        &[None, None],
        "overflow cannot suppress real Tool/Fetch effects"
    );
}

#[tokio::test]
async fn observed_final_metadata_drain_never_constructs_unawaited_effects() {
    let (mut execution, _) =
        completed("return search({query:'call'})", ExecuteLimits::default()).await;
    let (sender, events) = mpsc::unbounded_channel();
    while let Ok(event) = execution.events.try_recv() {
        sender
            .send(event)
            .unwrap_or_else(|_| panic!("receiver present"));
    }
    let (reply, response) = oneshot::channel();
    sender
        .send(HostEvent::Request {
            request: HostRequest::Tool {
                path: "fixture.echo".into(),
                input: json!({}),
                reply,
            },
            observed: true,
        })
        .unwrap_or_else(|_| panic!("receiver present"));
    drop(sender);
    execution.events = events;
    let calls = RefCell::new(Vec::new());
    let limited = Cell::new(0);
    let outcome = execution
        .run(
            &AtomicBool::new(false),
            |call| capture(call, &calls, &limited),
            |_, _| -> std::future::Ready<()> {
                panic!("draining a request is not effect admission")
            },
        )
        .await;
    assert!(outcome.is_ok());
    assert!(
        response.await.is_err(),
        "unconfirmed effect reply must be dropped, not manufactured"
    );
    assert_eq!(
        calls
            .borrow()
            .iter()
            .map(|call| call.tool.as_str())
            .collect::<Vec<_>>(),
        ["search", "fixture.echo"]
    );
}
