//! Code Mode (`execute`): MCP tools move behind `execute` by default, programs call them
//! through the runtime's admission path, and `codemode: false` keeps a server direct.

use super::*;

const SERVER: &str = r#"import json, sys
for line in sys.stdin:
    r = json.loads(line)
    method = r.get('method')
    if method == 'initialize':
        result = {'protocolVersion': '2025-11-25', 'capabilities': {'tools': {}}, 'serverInfo': {'name': 'fixture', 'version': '1'}}
    elif method == 'tools/list':
        result = {'tools': [
            {'name': 'add', 'description': 'Add two numbers', 'inputSchema': {'type': 'object', 'properties': {'a': {'type': 'number'}, 'b': {'type': 'number'}}, 'required': ['a', 'b']}},
            {'name': 'mark', 'description': 'Append a marker', 'inputSchema': {'type': 'object'}},
        ]}
    elif method == 'tools/call':
        params = r['params']
        args = params.get('arguments') or {}
        if params['name'] == 'add':
            total = args['a'] + args['b']
            result = {'content': [{'type': 'text', 'text': str(total)}], 'structuredContent': {'sum': total}, 'isError': False}
        else:
            with open(sys.argv[1], 'a') as f: f.write('M\n')
            result = {'content': [{'type': 'text', 'text': 'marked'}], 'isError': False}
    else:
        continue
    print(json.dumps({'jsonrpc': '2.0', 'id': r['id'], 'result': result}), flush=True)
"#;

fn fixture_generation(
    harness: &Harness,
    mut generation: Generation,
    codemode: Option<bool>,
) -> Generation {
    let script = harness._project.path().join("mcp.py");
    let marks = harness._project.path().join("marks");
    std::fs::write(&script, SERVER).unwrap();
    generation.mcp.insert(
        "fixture".to_string(),
        McpEntry {
            kind: "local".to_string(),
            enabled: true,
            command: vec![
                "/usr/bin/python3".to_string(),
                script.to_string_lossy().into_owned(),
                marks.to_string_lossy().into_owned(),
            ],
            timeout: Some(2000),
            codemode,
            ..Default::default()
        },
    );
    generation
}

fn permissions(mark: Permission) -> BTreeMap<String, Permission> {
    let mut permissions = allow_all();
    permissions.insert("execute".to_string(), Permission::Allow);
    permissions.insert("fixture__add".to_string(), Permission::Allow);
    permissions.insert("fixture__mark".to_string(), mark);
    permissions
}

fn tool_names(request: &serde_json::Value) -> Vec<String> {
    request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect()
}

fn call_output(request: &serde_json::Value, call_id: &str) -> String {
    request["input"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
        .and_then(|item| item["output"].as_str())
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn code_mode_routes_mcp_tools_through_execute() {
    let (harness, generation) = make_harness(permissions(Permission::Allow));
    let generation = fixture_generation(&harness, generation, None);
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    wait_initial_mcp(&runtime).await;
    let code = "const [r, again] = await Promise.all([tools.fixture.add({ a: 2, b: 3 }), tools.fixture.add({ a: 1, b: 1 })]);\n\
                await tools.fixture.mark({});\n\
                console.log('sum', r.sum);\n\
                return { sum: r.sum, again: again.sum }";
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_tool_call("exec-1", "execute", &serde_json::json!({ "code": code }))
                + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "s",
            "add numbers",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(
        (
            report.calls[0].name.as_str(),
            report.calls[0].state.as_str()
        ),
        ("execute", "completed")
    );
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("marks")).unwrap(),
        "M\n"
    );
    let requests = requests.lock().unwrap();
    let names = tool_names(&requests[0]);
    assert!(names.contains(&"execute".to_string()), "{names:?}");
    assert!(
        !names.iter().any(|name| name.starts_with("fixture__")),
        "Code Mode servers leave the direct list: {names:?}"
    );
    let execute = requests[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "execute")
        .unwrap();
    let description = execute["description"].as_str().unwrap();
    assert!(
        description.contains("tools.fixture.add(input: {"),
        "{description}"
    );
    assert!(
        description.contains("tools.fixture.mark(): Promise<unknown> // Append a marker"),
        "{description}"
    );
    let output = call_output(&requests[1], "exec-1");
    assert_eq!(
        output,
        "{\n  \"sum\": 5,\n  \"again\": 2\n}\n\nLogs:\nsum 5"
    );
    // Nested calls run under the execute operation; no separate model-visible calls.
    let ops = harness.db.list_tool_ops("s").unwrap();
    assert_eq!(ops.len(), 1);
    assert!(ops[0].op.ends_with("exec-1"));
}

#[tokio::test]
async fn code_mode_nested_calls_keep_their_own_permissions() {
    // `mark` asks, and no approval consumer is attached: the program sees a tool error
    // and nothing runs, while the allowed call still succeeds.
    let (harness, generation) = make_harness(permissions(Permission::Ask));
    let generation = fixture_generation(&harness, generation, None);
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    wait_initial_mcp(&runtime).await;
    let code = "const r = await tools.fixture.add({ a: 1, b: 2 });\n\
                try { await tools.fixture.mark({}) } catch (e) { return `${r.sum}: ${e.message}` }";
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_tool_call("exec-ask", "execute", &serde_json::json!({ "code": code }))
                + &sse_completed(),
            sse_completed(),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "s",
            "mark",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.calls[0].state, "completed");
    assert!(!harness._project.path().join("marks").exists());
    let output = call_output(&requests.lock().unwrap()[1], "exec-ask");
    assert_eq!(
        output,
        "3: Permission for 'fixture.mark' requires approval, but no approval consumer is attached."
    );
}

#[tokio::test]
async fn code_mode_denied_tools_leave_the_catalog_and_failures_are_reported() {
    let (harness, generation) = make_harness(permissions(Permission::Deny));
    let generation = fixture_generation(&harness, generation, None);
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    wait_initial_mcp(&runtime).await;
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_tool_call(
                "exec-deny",
                "execute",
                &serde_json::json!({ "code": "await tools.fixture.mark({})" }),
            ) + &sse_completed(),
            sse_completed(),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "s",
            "mark",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.calls[0].state, "failed");
    let requests = requests.lock().unwrap();
    let execute = requests[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "execute")
        .unwrap()
        .clone();
    assert!(
        !execute["description"]
            .as_str()
            .unwrap()
            .contains("tools.fixture.mark")
    );
    assert!(
        call_output(&requests[1], "exec-deny").starts_with("Unknown tool 'fixture.mark'."),
        "{}",
        call_output(&requests[1], "exec-deny")
    );
    assert!(!harness._project.path().join("marks").exists());
}

#[tokio::test]
async fn codemode_false_keeps_the_server_direct() {
    let (harness, generation) = make_harness(permissions(Permission::Allow));
    let generation = fixture_generation(&harness, generation, Some(false));
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    wait_initial_mcp(&runtime).await;
    let (base, _, requests) = Fake::start_recording(vec![sse_completed()], Duration::ZERO);
    runtime
        .run_turn(params(
            "s",
            "hello",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    let names = tool_names(&requests.lock().unwrap()[0]);
    assert!(names.contains(&"fixture__add".to_string()), "{names:?}");
    assert!(!names.contains(&"execute".to_string()), "{names:?}");
}

#[tokio::test]
async fn execute_is_absent_without_code_mode_tools_or_when_denied() {
    let (harness, generation) = make_harness(permissions(Permission::Allow));
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (base, _, requests) = Fake::start_recording(vec![sse_completed()], Duration::ZERO);
    runtime
        .run_turn(params(
            "s",
            "hello",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert!(!tool_names(&requests.lock().unwrap()[0]).contains(&"execute".to_string()));

    let mut denied = permissions(Permission::Allow);
    denied.insert("execute".to_string(), Permission::Deny);
    let (harness, generation) = make_harness(denied);
    let generation = fixture_generation(&harness, generation, None);
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    wait_initial_mcp(&runtime).await;
    let (base, _, requests) = Fake::start_recording(vec![sse_completed()], Duration::ZERO);
    runtime
        .run_turn(params(
            "s",
            "hello",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    let names = tool_names(&requests.lock().unwrap()[0]);
    assert!(!names.contains(&"execute".to_string()), "{names:?}");
    assert!(
        !names.iter().any(|name| name.starts_with("fixture__")),
        "{names:?}"
    );
}

fn http_echo_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/echo",
        listener.local_addr().unwrap().port()
    );
    std::thread::spawn(move || {
        for stream in listener.incoming().filter_map(Result::ok) {
            let mut reader = BufReader::new(stream);
            let mut request_line = String::new();
            let _ = reader.read_line(&mut request_line);
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            let reply = serde_json::json!({
                "request": request_line.trim(),
                "body": String::from_utf8_lossy(&body),
            })
            .to_string();
            let _ = reader.get_mut().write_all(
                format!(
                    "HTTP/1.1 201 Created\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                    reply.len()
                )
                .as_bytes(),
            );
        }
    });
    url
}

fn runtime_allowing_private<'a>(
    harness: &'a Harness,
    generation: Generation,
    allow_private: bool,
) -> Runtime<'a> {
    let project = harness._project.path();
    Runtime::new(
        &harness.db,
        "work",
        generation,
        ProtectedGlobs {
            patterns: Vec::new(),
        },
        oc_adapters::files::Files::new(project, harness._data.path()).unwrap(),
        oc_adapters::shell::Shell::new(project).unwrap(),
        BTreeMap::new(),
        oc_adapters::tools::ToolRoots {
            project: project.to_path_buf(),
            data: harness._data.path().to_path_buf(),
        },
        None,
        allow_private,
        DcpConfig::default(),
    )
    .unwrap()
}

#[tokio::test]
async fn code_mode_fetch_goes_through_the_webfetch_owner() {
    let url = http_echo_server();
    for allow_private in [true, false] {
        let (harness, generation) = make_harness(permissions(Permission::Allow));
        let generation = fixture_generation(&harness, generation, None);
        let runtime = runtime_allowing_private(&harness, generation, allow_private);
        runtime.create_session("s").unwrap();
        wait_initial_mcp(&runtime).await;
        let code = format!(
            "const r = await fetch({url:?}, {{ method: 'POST', body: 'payload' }});\nconst data = await r.json();\nreturn [r.status, r.ok, data.request, data.body]"
        );
        let (base, _, requests) = Fake::start_recording(
            vec![
                sse_tool_call(
                    "exec-fetch",
                    "execute",
                    &serde_json::json!({ "code": code }),
                ) + &sse_completed(),
                sse_completed(),
            ],
            Duration::ZERO,
        );
        runtime
            .run_turn(params(
                "s",
                "fetch",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap();
        let output = call_output(&requests.lock().unwrap()[1], "exec-fetch");
        if allow_private {
            assert_eq!(
                output,
                "[\n  201,\n  true,\n  \"POST /echo HTTP/1.1\",\n  \"payload\"\n]"
            );
        } else {
            assert_eq!(output, "TypeError: fetch failed: private host refused");
        }
    }
}
