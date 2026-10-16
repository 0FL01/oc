use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value as Json, json};

use super::*;

fn tool(path: &str, description: &str, input: Json) -> CodeModeTool {
    CodeModeTool {
        path: path.into(),
        description: description.into(),
        input_schema: input,
        output_schema: None,
    }
}

fn catalog() -> Arc<Catalog> {
    Arc::new(Catalog::new(
        vec![
            tool(
                "echo.say",
                "Echo the input back",
                json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}),
            ),
            tool(
                "math.add",
                "Add two numbers",
                json!({"type": "object", "properties": {"a": {"type": "number"}, "b": {"type": "number"}}, "required": ["a", "b"]}),
            ),
            tool("fail.always", "Always fails", json!({"type": "object"})),
            tool(
                "slow.wait",
                "Waits for a partner call",
                json!({"type": "object"}),
            ),
            tool(
                "context7.resolve-library-id",
                "Resolve a library id",
                json!({"type": "object", "properties": {"libraryName": {"type": "string"}}, "required": ["libraryName"]}),
            ),
        ],
        vec![("math".into(), "Arithmetic helpers".into())],
    ))
}

struct Run {
    outcome: Outcome,
    requests: Vec<String>,
}

fn run_with(code: &str, limits: ExecuteLimits, cancel: Arc<AtomicBool>) -> Run {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let handler_log = log.clone();
    let outcome = runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(20),
            run(code.to_string(), catalog(), limits, &cancel, move |request| {
                let log = handler_log.clone();
                let barrier = barrier.clone();
                async move {
                    match request {
                        HostRequest::Tool { path, input, reply } => {
                            log.lock().unwrap().push(format!("{path} {input}"));
                            let result = match path.as_str() {
                                "echo.say" => Ok(input),
                                "math.add" => {
                                    let a = input["a"].as_f64().unwrap_or(0.0);
                                    let b = input["b"].as_f64().unwrap_or(0.0);
                                    Ok(json!(a + b))
                                }
                                "slow.wait" => {
                                    barrier.wait().await;
                                    Ok(json!("released"))
                                }
                                "fail.always" => Err("tool exploded".to_string()),
                                other => Ok(json!({ "called": other })),
                            };
                            let _ = reply.send(result);
                        }
                        HostRequest::Fetch { request, reply } => {
                            log.lock().unwrap().push(format!("fetch {} {}", request.method, request.url));
                            let body = json!({
                                "method": request.method,
                                "url": request.url,
                                "headers": request.headers,
                                "body": request.body.map(|b| String::from_utf8_lossy(&b).into_owned()),
                            });
                            let _ = reply.send(Ok(FetchResponse {
                                url: request.url,
                                status: 200,
                                status_text: "OK".into(),
                                redirected: false,
                                headers: vec![("Content-Type".into(), "application/json".into())],
                                body: body.to_string().into_bytes(),
                            }));
                        }
                    }
                }
            }),
        )
        .await
        .expect("program finished in time")
    });
    let requests = log.lock().unwrap().clone();
    Run { outcome, requests }
}

fn js(code: &str) -> Outcome {
    run_with(
        code,
        ExecuteLimits::default(),
        Arc::new(AtomicBool::new(false)),
    )
    .outcome
}

#[track_caller]
fn value(code: &str) -> Json {
    let outcome = js(code);
    match outcome.value {
        Ok(value) => value,
        Err(err) => panic!("program failed: {err:?}\n{code}"),
    }
}

#[track_caller]
fn error(code: &str) -> Diagnostic {
    match js(code).value {
        Ok(value) => panic!("program succeeded with {value}\n{code}"),
        Err(err) => err,
    }
}

#[test]
fn results_follow_return_and_trailing_expression() {
    assert_eq!(value("return 1 + 2"), json!(3));
    assert_eq!(value("const x = 4; x * 2"), json!(8));
    assert_eq!(value("let y = 1;"), Json::Null);
    assert_eq!(value("return undefined"), Json::Null);
    assert_eq!(value("return 'text'"), json!("text"));
    assert_eq!(
        js("return {a: 1, b: [true, null]}").output(),
        "{\n  \"a\": 1,\n  \"b\": [\n    true,\n    null\n  ]\n}"
    );
    assert_eq!(error("   ").kind, "ParseError");
}

#[test]
fn numbers_format_like_javascript() {
    assert_eq!(
        value(
            "return [0.1 + 0.2, 1e21, 1e-7, 123456789012, -0, 2**53, (255).toString(16), (3.14159).toFixed(2), (0.5).toFixed(0), (1234.5).toPrecision(2), String(1/3)]"
        ),
        json!([
            0.30000000000000004,
            1e21,
            1e-7,
            123456789012u64,
            0,
            9007199254740992u64,
            "ff",
            "3.14",
            "1",
            "1.2e+3",
            "0.3333333333333333"
        ])
    );
    assert_eq!(
        value("return `${1e21} ${1e-7} ${0.000001} ${NaN} ${-Infinity}`"),
        json!("1e+21 1e-7 0.000001 NaN -Infinity")
    );
    assert_eq!(
        value(
            "return [parseInt('42px'), parseInt('0x1f'), parseFloat('3.5e2abc'), Number(''), Number('12 '), Number('abc')]"
        ),
        json!([42, 31, 350, 0, 12, null])
    );
}

#[test]
fn strings_use_utf16_semantics() {
    assert_eq!(
        value(
            "const s = 'привет😀'; return [s.length, s.slice(0, 6), s.charCodeAt(6), s.codePointAt(6), [...'a😀'].length]"
        ),
        json!([8, "привет", 55357, 128512, 2])
    );
    assert_eq!(
        value("return 'a-b-c'.split('-').map(x => x.toUpperCase()).join('+')"),
        json!("A+B+C")
    );
    assert_eq!(
        value("return 'Hello World'.replace(/(\\w+) (\\w+)/, '$2 $1')"),
        json!("World Hello")
    );
    assert_eq!(
        value("return 'a1b22c333'.replace(/\\d+/g, m => `[${m.length}]`)"),
        json!("a[1]b[2]c[3]")
    );
    assert_eq!(
        value("return '5'.padStart(3, '0') + '|' + 'ab'.repeat(3) + '|' + '  x '.trim()"),
        json!("005|ababab|x")
    );
    assert_eq!(
        value(
            "const m = 'key=value'.match(/(?<k>\\w+)=(?<v>\\w+)/); return [m.groups.k, m.groups.v, m.index]"
        ),
        json!(["key", "value", 0])
    );
    assert_eq!(
        value("return [...'a1b2'.matchAll(/[a-z](\\d)/g)].map(m => m[1])"),
        json!(["1", "2"])
    );
    assert_eq!(
        value("return 'one, two,three'.split(/,\\s*/)"),
        json!(["one", "two", "three"])
    );
    assert_eq!(
        value("return 'aXbXc'.replaceAll('X', '-') + 'abc'.at(-1) + 'abc'.includes('bc')"),
        json!("a-b-cctrue")
    );
}

#[test]
fn arrays_objects_and_destructuring() {
    assert_eq!(
        value(
            "const xs = [5, 1, 4]; const [first, ...rest] = xs; return { sorted: [...xs].sort((a, b) => a - b), first, rest, sum: xs.reduce((a, b) => a + b, 0), flat: [[1], [2, [3]]].flat(Infinity), from: Array.from({ length: 3 }, (_, i) => i * i) }"
        ),
        json!({"sorted": [1, 4, 5], "first": 5, "rest": [1, 4], "sum": 10, "flat": [1, 2, 3], "from": [0, 1, 4]})
    );
    assert_eq!(
        value(
            "const o = { a: 1, nested: { b: 2 } }; const { a, nested: { b }, missing = 'd' } = o; return { ...o, a: a + b, missing, keys: Object.keys(o), entries: Object.entries({x: 1}) }"
        ),
        json!({"a": 3, "nested": {"b": 2}, "missing": "d", "keys": ["a", "nested"], "entries": [["x", 1]]})
    );
    assert_eq!(
        value("return Object.fromEntries([['a', 1], ['b', 2]])"),
        json!({"a": 1, "b": 2})
    );
    assert_eq!(value("return [3, 1, 10, 2].sort()"), json!([1, 10, 2, 3]));
    assert_eq!(
        value("const o = {}; o['2'] = 'b'; o.z = 'z'; o['1'] = 'a'; return Object.keys(o)"),
        json!(["1", "2", "z"])
    );
    assert_eq!(
        value("let a = 1, b = 2; [a, b] = [b, a]; return [a, b]"),
        json!([2, 1])
    );
    assert_eq!(
        value("return [1, [2, 3]].includes(1) && [NaN].includes(NaN) && [NaN].indexOf(NaN) === -1"),
        json!(true)
    );
}

#[test]
fn closures_classes_and_getters() {
    assert_eq!(
        value(
            "const fns = []; for (let i = 0; i < 3; i++) fns.push(() => i); return fns.map(f => f())"
        ),
        json!([0, 1, 2])
    );
    assert_eq!(error("class A { #x }").kind, "UnsupportedSyntax");
    assert_eq!(
        value(
            "class Animal { static count = 0; constructor(name) { this.name = name; Animal.count++ } speak() { return `${this.name} speaks` } get upper() { return this.name.toUpperCase() } }\nclass Dog extends Animal { kind = 'dog'; speak() { return super.speak() + ' (woof)' } }\nconst d = new Dog('rex'); return [d.speak(), d.upper, d.kind, d instanceof Animal, Animal.count, Object.keys(d)]"
        ),
        json!(["rex speaks (woof)", "REX", "dog", true, 1, ["name", "kind"]])
    );
    assert_eq!(
        value(
            "class MyError extends Error { constructor(m) { super(m); this.name = 'MyError'; this.code = 7 } }\ntry { throw new MyError('bad') } catch (e) { return [e instanceof MyError, e instanceof Error, String(e), e.code, e.message] }"
        ),
        json!([true, true, "MyError: bad", 7, "bad"])
    );
    assert_eq!(
        value(
            "const o = { _v: 1, get v() { return this._v * 10 }, set v(x) { this._v = x } }; o.v = 5; return o.v"
        ),
        json!(50)
    );
    assert_eq!(
        value(
            "function counter() { let n = 0; return { inc: () => ++n, get: () => n } } const c = counter(); c.inc(); c.inc(); return c.get()"
        ),
        json!(2)
    );
    assert_eq!(
        value("const f = function fact(n) { return n <= 1 ? 1 : n * fact(n - 1) }; return f(10)"),
        json!(3628800)
    );
    assert_eq!(
        value("function f() { return arguments.length } return f(1, 2, 3)"),
        json!(3)
    );
    assert_eq!(
        value("const bound = function () { return this.x }.bind({ x: 9 }); return bound()"),
        json!(9)
    );
}

#[test]
fn control_flow() {
    assert_eq!(
        value(
            "let out = []; outer: for (const i of [1, 2, 3]) { for (const j of [1, 2, 3]) { if (j === 2) continue outer; if (i === 3) break outer; out.push(`${i}${j}`) } } return out"
        ),
        json!(["11", "21"])
    );
    assert_eq!(
        value(
            "function f(x) { switch (x) { case 1: return 'one'; case 2: case 3: return 'few'; default: return 'many' } } return [f(1), f(3), f(9)]"
        ),
        json!(["one", "few", "many"])
    );
    assert_eq!(
        value("const ks = []; for (const k in { a: 1, b: 2 }) ks.push(k); return ks"),
        json!(["a", "b"])
    );
    assert_eq!(
        value("let i = 0; do { i++ } while (i < 5); return i"),
        json!(5)
    );
    assert_eq!(
        value(
            "const o = { a: { b: null } }; return [o?.a?.b?.c, o.x?.y.z, o.a.b ?? 'dflt', typeof nope, o.f?.()]"
        ),
        json!([null, null, "dflt", "undefined", null])
    );
    assert_eq!(
        value(
            "let log = []; try { try { throw new Error('x') } finally { log.push('f') } } catch (e) { log.push(e.message) } return log"
        ),
        json!(["f", "x"])
    );
    assert_eq!(
        value(
            "let x = null; x ??= 3; let y = 0; y ||= 4; let z = 1; z &&= 5; return [x, y, z, 2 ** 10, 7 % 3, -7 % 3, 1 << 4, -16 >> 2, -1 >>> 28]"
        ),
        json!([3, 4, 5, 1024, 1, -1, 16, -4, 15])
    );
}

#[test]
fn generators_and_iteration() {
    assert_eq!(
        value("function* range(n) { for (let i = 0; i < n; i++) yield i } return [...range(4)]"),
        json!([0, 1, 2, 3])
    );
    assert_eq!(
        value(
            "function* g() { const x = yield 1; yield x * 2 } const it = g(); return [it.next().value, it.next(5).value, it.next().done]"
        ),
        json!([1, 10, true])
    );
    assert_eq!(
        value(
            "const it = { from: 1, to: 3, [Symbol.iterator]() { let c = this.from, t = this.to; return { next: () => c <= t ? { value: c++, done: false } : { value: undefined, done: true } } } }; return Array.from(it)"
        ),
        json!([1, 2, 3])
    );
    assert_eq!(
        value(
            "async function* g() { yield 1; yield await Promise.resolve(2) } const out = []; for await (const v of g()) out.push(v); return out"
        ),
        json!([1, 2])
    );
}

#[test]
fn collections_dates_urls_and_encodings() {
    assert_eq!(
        value(
            "const m = new Map([['a', 1]]); m.set('b', 2); const s = new Set([1, 2, 2, 3]); return { size: m.size, b: m.get('b'), keys: [...m.keys()], set: [...s], has: s.has(2), map: m, setValue: s }"
        ),
        json!({"size": 2, "b": 2, "keys": ["a", "b"], "set": [1, 2, 3], "has": true, "map": {}, "setValue": [1, 2, 3]})
    );
    assert_eq!(
        value(
            "const d = new Date('2024-03-05T10:20:30.123Z'); return [d.toISOString(), d.getUTCDay(), d.getTime(), JSON.stringify({ d })]"
        ),
        json!([
            "2024-03-05T10:20:30.123Z",
            2,
            1709634030123u64,
            "{\"d\":\"2024-03-05T10:20:30.123Z\"}"
        ])
    );
    assert_eq!(
        value(
            "const d = new Date(Date.UTC(2024, 0, 31)); d.setUTCMonth(1); return d.toISOString().slice(0, 10)"
        ),
        json!("2024-03-02")
    );
    assert_eq!(
        value(
            "const u = new URL('https://example.com/a/b?x=1&y=two#frag'); u.searchParams.set('x', '2'); u.searchParams.append('z', 'a b'); return [u.hostname, u.pathname, u.search, u.hash, u.href, new URL('../c', 'https://h.io/a/b/').href]"
        ),
        json!([
            "example.com",
            "/a/b",
            "?x=2&y=two&z=a+b",
            "#frag",
            "https://example.com/a/b?x=2&y=two&z=a+b#frag",
            "https://h.io/a/c"
        ])
    );
    assert_eq!(
        value(
            "return [btoa('hello'), atob('aGVsbG8='), new TextDecoder().decode(new TextEncoder().encode('ж')), new TextEncoder().encode('ж').length, encodeURIComponent('a b&c'), decodeURIComponent('%D0%B6')]"
        ),
        json!(["aGVsbG8=", "hello", "ж", 2, "a%20b%26c", "ж"])
    );
    assert_eq!(
        value(
            "const id = crypto.randomUUID(); return /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(id)"
        ),
        json!(true)
    );
    assert_eq!(
        value(
            "return JSON.parse('{\"a\":[1,2,{\"b\":\"c\"}]}').a[2].b + JSON.stringify([1, 'x', { y: undefined, z: () => 1 }])"
        ),
        json!("c[1,\"x\",{}]")
    );
}

#[test]
fn tools_are_called_through_the_host() {
    let run = run_with(
        "const r = await tools.math.add({ a: 2, b: 3 }); const e = await tools.echo.say({ text: 'hi', skip: undefined }); return { r, e }",
        ExecuteLimits::default(),
        Arc::new(AtomicBool::new(false)),
    );
    assert_eq!(run.outcome.value, Ok(json!({"r": 5, "e": {"text": "hi"}})));
    assert_eq!(
        run.requests,
        vec![
            "math.add {\"a\":2,\"b\":3}".to_string(),
            "echo.say {\"text\":\"hi\"}".to_string()
        ]
    );
    assert_eq!(run.outcome.tool_calls, 2);
    assert_eq!(
        value("return await tools.context7['resolve-library-id']({ libraryName: 'react' })"),
        json!({"called": "context7.resolve-library-id"})
    );
    assert_eq!(
        value("return [typeof tools.math.add, Object.keys(tools), Object.keys(tools.math)]"),
        json!([
            "function",
            ["context7", "echo", "fail", "math", "slow"],
            ["add"]
        ])
    );
}

#[test]
fn promise_all_runs_tool_calls_concurrently() {
    // `slow.wait` only completes once two calls are in flight at the same time.
    assert_eq!(
        value("return await Promise.all([tools.slow.wait(), tools.slow.wait()])"),
        json!(["released", "released"])
    );
    assert_eq!(
        value(
            "const results = await Promise.allSettled([tools.math.add({ a: 1, b: 1 }), tools.fail.always()]); return results.map(r => r.status)"
        ),
        json!(["fulfilled", "rejected"])
    );
    assert_eq!(
        value(
            "return await Promise.race([new Promise(r => r('first')), tools.echo.say({ text: 'x' })])"
        ),
        json!("first")
    );
    assert_eq!(
        value(
            "const order = []; const p = (async () => { order.push('a'); await null; order.push('c') })(); order.push('b'); await p; return order"
        ),
        json!(["a", "b", "c"])
    );
    assert_eq!(
        value(
            "const p = new Promise((resolve) => resolve(1)).then(x => x + 1).then(x => { throw new Error('e' + x) }).catch(e => e.message).finally(() => {}); return await p"
        ),
        json!("e2")
    );
}

#[test]
fn tool_failures_and_unknown_tools_are_diagnostics() {
    assert_eq!(
        value("try { await tools.fail.always() } catch (e) { return e.message }"),
        json!("tool exploded")
    );
    let failure = error("await tools.fail.always()");
    assert_eq!(
        (failure.kind.as_str(), failure.message.as_str()),
        ("ToolFailure", "tool exploded")
    );
    let unknown = error("await tools.nope.missing({})");
    assert_eq!(unknown.kind, "UnknownTool");
    assert!(
        js("await tools.nope.missing({})")
            .output()
            .contains("Use search to find available tools.")
    );
    assert_eq!(error("await tools.math({})").kind, "UnknownTool");
    assert_eq!(error("await tools.math.add(1, 2)").kind, "InvalidToolInput");
    assert_eq!(error("await tools.math.add('x')").kind, "InvalidToolInput");
    let limited = run_with(
        "await tools.math.add({a:1,b:1}); await tools.math.add({a:1,b:1})",
        ExecuteLimits {
            max_tool_calls: Some(1),
            ..ExecuteLimits::default()
        },
        Arc::new(AtomicBool::new(false)),
    );
    assert_eq!(
        limited.outcome.value.unwrap_err().kind,
        "ToolCallLimitExceeded"
    );
}

#[test]
fn search_finds_tools() {
    let found = value("return search({ query: 'resolve library id' })");
    assert_eq!(
        found["items"][0]["path"],
        json!("tools.context7[\"resolve-library-id\"]")
    );
    assert!(
        found["items"][0]["signature"]
            .as_str()
            .unwrap()
            .starts_with("tools.context7[\"resolve-library-id\"](input: {")
    );
    assert_eq!(
        value("return (await tools.search({ query: 'tools.math.add' })).items.map(i => i.path)"),
        json!(["tools.math.add"])
    );
    assert_eq!(
        value("return search({ namespace: 'math' }).items.length"),
        json!(1)
    );
    assert_eq!(
        value("const search = () => 'local'; return search()"),
        json!("local")
    );
}

#[test]
fn fetch_uses_the_host_bridge() {
    let run = run_with(
        "const r = await fetch('https://api.example.com/x', { method: 'post', headers: { 'X-Key': 'v' }, body: JSON.stringify({ q: 1 }) }); const body = await r.json(); return [r.ok, r.status, r.headers.get('content-type'), body.method, body.body]",
        ExecuteLimits::default(),
        Arc::new(AtomicBool::new(false)),
    );
    assert_eq!(
        run.outcome.value,
        Ok(json!([true, 200, "application/json", "POST", "{\"q\":1}"]))
    );
    assert_eq!(
        run.requests,
        vec!["fetch POST https://api.example.com/x".to_string()]
    );
}

#[test]
fn errors_are_reported_with_context() {
    let thrown = error("const x = 1;\nthrow new Error('boom')");
    assert_eq!(
        (thrown.kind.as_str(), thrown.message.as_str()),
        ("ExecutionFailure", "Error: boom (line 2)")
    );
    assert_eq!(
        error("missing + 1").message,
        "ReferenceError: missing is not defined (line 1)"
    );
    assert!(
        error("const o = {};\no.run()")
            .message
            .starts_with("TypeError: o.run is not a function (line 2)")
    );
    assert_eq!(error("throw 'plain'").message, "Uncaught: plain");
    assert_eq!(error("throw { code: 1 }").message, "Uncaught: {\"code\":1}");
    let parse = error("let = ;");
    assert_eq!(parse.kind, "ParseError");
    assert!(parse.message.contains("(1:"));
    assert_eq!(error("import fs from 'fs'").kind, "UnsupportedSyntax");
    assert_eq!(error("return 10n").kind, "UnsupportedSyntax");
    assert_eq!(
        error("setTimeout(() => {}, 1)").message,
        "ReferenceError: Timers are not available in Code Mode"
    );
    assert_eq!(
        error("const c = 1; c = 2").message,
        "TypeError: Assignment to constant variable."
    );
    assert_eq!(
        value(
            "function f(n) { return f(n + 1) } try { f(0) } catch (e) { return e instanceof RangeError }"
        ),
        json!(true)
    );
}

#[test]
fn process_apis_are_unavailable() {
    for code in [
        "return process.env",
        "process.exit(0)",
        "return Bun.env",
        "return Deno.env",
    ] {
        let diagnostic = error(code);
        assert!(
            diagnostic.message.starts_with("ReferenceError:"),
            "{code}: {diagnostic:?}"
        );
        assert!(
            diagnostic.message.contains("not available in Code Mode"),
            "{code}: {diagnostic:?}"
        );
    }
    // These names are unavailable host globals, not reserved user identifiers.
    assert_eq!(
        value("const process = { env: 'local' }; return process.env"),
        json!("local")
    );
}

#[test]
fn dynamic_function_construction_is_unavailable() {
    for code in [
        "new Function('return 1')",
        "Function('return 1')",
        "new (Function.bind(null))('return 1')",
    ] {
        assert_eq!(
            error(code).message,
            "ReferenceError: Dynamic code evaluation is not available in Code Mode"
        );
    }
    assert_eq!(
        value("return Function.prototype.call.call((n) => n + 1, null, 2)"),
        json!(3)
    );
}

#[test]
fn console_log_byte_limit_includes_line_separators() {
    let run = run_with(
        "console.log('é'); console.log(''); console.log('x'); return 1",
        ExecuteLimits {
            max_log_bytes: 3,
            ..ExecuteLimits::default()
        },
        Arc::new(AtomicBool::new(false)),
    );
    assert_eq!(run.outcome.value, Ok(json!(1)));
    assert_eq!(run.outcome.logs, vec!["é"]);
    assert_eq!(run.outcome.logs_dropped, 2);
}

#[test]
fn resource_guards_stop_runaway_programs() {
    let limited = run_with(
        "while (true) {}",
        ExecuteLimits {
            max_steps: 100_000,
            ..ExecuteLimits::default()
        },
        Arc::new(AtomicBool::new(false)),
    );
    assert_eq!(limited.outcome.value.unwrap_err().kind, "TimeoutExceeded");
    assert_eq!(
        error("return 'x'.repeat(1 << 25)").message,
        "RangeError: Invalid string length: 33554432 bytes exceeds the Code Mode limit of 16777216"
    );
    assert!(
        error("return new Array(20000000)")
            .message
            .contains("exceeds the Code Mode limit")
    );
    assert!(error("const ps = []; for (let i = 0; i < 10001; i++) ps.push(new Promise(() => {})); return 1").message.contains("Too many pending promises"));
    let uncatchable = run_with(
        "try { while (true) {} } catch (e) { return 'caught' }",
        ExecuteLimits {
            max_steps: 100_000,
            ..ExecuteLimits::default()
        },
        Arc::new(AtomicBool::new(false)),
    );
    assert_eq!(
        uncatchable.outcome.value.unwrap_err().kind,
        "TimeoutExceeded"
    );
}

#[test]
fn cancellation_interrupts_a_busy_program() {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let started = std::time::Instant::now();
    let run = run_with(
        "let i = 0; while (true) { i++ }",
        ExecuteLimits {
            max_steps: u64::MAX,
            ..ExecuteLimits::default()
        },
        cancel,
    );
    assert_eq!(
        run.outcome.value.unwrap_err().message,
        "Execution was interrupted."
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn data_boundary_matches_the_donor_table() {
    assert_eq!(
        value(
            "const e = new TypeError('bad'); e.code = 3; return { set: new Set([1, 2]), map: new Map([[1, 2]]), params: new URLSearchParams({ a: '1 2' }), err: e, nan: NaN, f() {}, u: undefined, arr: [undefined, () => 1], date: new Date(0) }"
        ),
        json!({"set": [1, 2], "map": {}, "params": "a=1+2", "err": {"name": "TypeError", "message": "bad", "code": 3}, "nan": null, "arr": [null, null], "date": "1970-01-01T00:00:00.000Z"})
    );
    assert!(
        error("return new Uint8Array([1])")
            .message
            .contains("Uint8Array cannot cross")
    );
    assert_eq!(
        error("const a = {}; a.self = a; return a").message,
        "TypeError: Converting circular structure to JSON"
    );
    assert_eq!(value("return Promise.resolve(5)"), json!(5));
    assert_eq!(
        value("return JSON.parse('{\"__proto__\": {\"x\": 1}, \"y\": 2}')"),
        json!({"y": 2})
    );
}

#[test]
fn logs_and_warnings_are_formatted() {
    let outcome = js(
        "console.log('hello', 1, { a: [1, 'x'] }); console.warn('careful'); console.error(new Error('e')); return 'done'",
    );
    assert_eq!(
        outcome.output(),
        "done\n\nLogs:\nhello 1 {\"a\":[1,\"x\"]}\n[warn] careful\n[error] Error: e"
    );
    let unhandled = js("tools.fail.always(); await tools.math.add({ a: 1, b: 2 }); return 'ok'");
    assert_eq!(unhandled.value, Ok(json!("ok")));
    assert!(unhandled.output().contains("Warnings:\n- [ToolFailure] Unhandled rejection from an un-awaited promise: tool exploded"), "{}", unhandled.output());
    let failure = js("console.log('before'); throw new Error('after')");
    assert_eq!(failure.output(), "Error: after (line 1)\n\nLogs:\nbefore");
}

#[test]
fn signatures_and_catalog_follow_the_donor() {
    let schema = json!({
        "type": "object",
        "properties": {
            "owner": { "type": "string", "description": "Repository owner" },
            "after": { "type": "string", "description": "Cursor from the previous response's pageInfo" },
            "perPage": { "type": "number", "description": "Results per page", "default": 30 },
            "labels": { "type": "array", "items": { "type": "string" }, "description": "Filter by labels", "minItems": 1, "maxItems": 10 },
            "state": { "enum": ["open", "closed"] }
        },
        "required": ["owner"]
    });
    assert_eq!(
        schema_to_typescript(&schema, false),
        "{ owner: string; after?: string; perPage?: number; labels?: Array<string>; state?: \"open\" | \"closed\" }"
    );
    assert_eq!(
        schema_to_typescript(&schema, true),
        [
            "{",
            "  /** Repository owner */",
            "  owner: string,",
            "  /** Cursor from the previous response's pageInfo */",
            "  after?: string,",
            "  /** Results per page. @default 30 */",
            "  perPage?: number,",
            "  /** Filter by labels. @minItems 1 @maxItems 10 */",
            "  labels?: Array<string>,",
            "  state?: \"open\" | \"closed\",",
            "}",
        ]
        .join("\n")
    );
    let rendered = catalog().render();
    assert!(
        rendered.starts_with("The Code Mode tool catalog below is complete."),
        "{rendered}"
    );
    assert!(rendered.contains("- math (1 tool) // Arithmetic helpers\n  - tools.math.add(input: {\n  a: number,\n  b: number,\n}): Promise<unknown> // Add two numbers"), "{rendered}");
    assert!(
        rendered.contains("tools.fail.always(): Promise<unknown>"),
        "{rendered}"
    );
    let empty = Catalog::new(Vec::new(), Vec::new());
    assert!(
        empty
            .render()
            .starts_with("No Code Mode tools are currently available.")
    );
    assert_eq!(tool_expression("a.b-c.d"), "tools.a[\"b-c\"].d");
}
