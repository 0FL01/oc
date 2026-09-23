//! Actual-binary, offline Zen Free qualification: dynamic catalog, Chat tool
//! continuation, restart and a price change between startup and generation.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const MODEL: &str = "fixture/zen-future-unlisted-model";
const SESSION: &str = "s-zen-offline";
const TIMEOUT: Duration = Duration::from_secs(15);

struct Fixture {
    _root: tempfile::TempDir,
    home: PathBuf,
    project: PathBuf,
    listener: TcpListener,
    campaign_dir: PathBuf,
}

struct Request {
    method: String,
    path: String,
    headers: String,
    body: Option<Value>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("isolated Zen fixture");
        let home = root.path().join("home");
        let project = root.path().join("project");
        std::fs::create_dir_all(home.join("config/opencode")).expect("config root");
        std::fs::create_dir_all(&project).expect("project root");
        let campaign_dir = home.join("campaign");
        std::fs::create_dir(&campaign_dir).expect("campaign root");
        std::fs::set_permissions(
            &campaign_dir,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .expect("private campaign root");
        std::fs::write(project.join("note.txt"), "offline Zen read evidence\n")
            .expect("tool fixture");
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback peer");
        listener.set_nonblocking(true).expect("nonblocking peer");
        let address = listener.local_addr().expect("peer address");
        std::fs::write(
            home.join("config/opencode/opencode.json"),
            json!({
                "model": format!("opencode/{MODEL}"),
                "permissions": {"read": "allow"},
                "provider": {"opencode": {
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {"baseURL": format!("http://{address}/zen/v1"),
                                "timeout": false, "setCacheKey": false}
                }}
            })
            .to_string(),
        )
        .expect("configuration");
        Self {
            _root: root,
            home,
            project,
            listener,
            campaign_dir: campaign_dir
                .canonicalize()
                .expect("canonical campaign root"),
        }
    }

    fn spawn(&self, label: &str, prompt: &str) -> Child {
        Command::new(env!("CARGO_BIN_EXE_oc"))
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join("config"))
            .env("XDG_DATA_HOME", self.home.join("data"))
            .env("XDG_CACHE_HOME", self.home.join("cache"))
            .env("XDG_STATE_HOME", self.home.join("state"))
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .env("OC_TEST_ZEN_CAMPAIGN_DIR", &self.campaign_dir)
            .env(
                "OC_TEST_ZEN_METADATA_URL",
                format!("http://{}/api.json", self.listener.local_addr().unwrap()),
            )
            .current_dir(&self.project)
            .args(["run", "--session", SESSION, prompt])
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(self.home.join(format!("{label}.out"))).unwrap())
            .stderr(std::fs::File::create(self.home.join(format!("{label}.err"))).unwrap())
            .spawn()
            .expect("actual oc binary")
    }

    fn run(&self, label: &str, prompt: &str, price_change_after: Option<usize>) -> Run {
        self.run_with_compression(label, prompt, price_change_after, None)
    }

    fn run_with_compression(
        &self,
        label: &str,
        prompt: &str,
        price_change_after: Option<usize>,
        compress_range: Option<(&str, &str)>,
    ) -> Run {
        let before_slots = std::fs::read_dir(&self.campaign_dir).unwrap().count();
        let mut child = self.spawn(label, prompt);
        let deadline = Instant::now() + TIMEOUT;
        let mut requests = Vec::new();
        loop {
            match self.listener.accept() {
                Ok((mut socket, _)) => {
                    let request = read_request(&mut socket);
                    let metadata_fetch = requests
                        .iter()
                        .filter(|request: &&Request| request.path == "/api.json")
                        .count();
                    let chat_count = requests
                        .iter()
                        .filter(|request: &&Request| request.method == "POST")
                        .count();
                    match (request.method.as_str(), request.path.as_str()) {
                        ("GET", "/api.json") => {
                            let price = if price_change_after.is_some_and(|n| metadata_fetch > n) {
                                0.01
                            } else {
                                0.0
                            };
                            respond_json(&mut socket, &metadata(price));
                        }
                        ("GET", "/zen/v1/models") => {
                            respond_json(
                                &mut socket,
                                &json!({"object":"list", "data":[
                                    {"object":"model", "id":MODEL}
                                ]}),
                            );
                        }
                        ("POST", "/zen/v1/chat/completions") if price_change_after != Some(0) => {
                            let body = request.body.as_ref().expect("Chat JSON");
                            let reply = match (label, chat_count) {
                                ("first", 0) => chat_tool_read(),
                                ("first", 1) => chat_text("read complete"),
                                ("first", 2) => chat_text("Zen fixture session"),
                                ("compress", 0) => {
                                    let (start, end) = compress_range.expect("DCP range");
                                    chat_tool_call(
                                        "call_zen_compress",
                                        "compress",
                                        json!({
                                            "topic": "completed local read",
                                            "content": [{
                                                "startId": start,
                                                "endId": end,
                                                "summary": "The local note was read; the tool returned offline Zen read evidence."
                                            }]
                                        }),
                                    )
                                }
                                ("compress", 1) => chat_text("compressed turn complete"),
                                ("child", 0) => chat_tool_call(
                                    "call_zen_child",
                                    "subagent",
                                    json!({"agent":"helper", "description":"Offline child", "prompt":"inspect fixture"}),
                                ),
                                ("child", 1) => chat_text("child inspected fixture"),
                                ("child", 2) => chat_text("parent received child result"),
                                ("child", 3) => chat_text("Zen child session"),
                                ("reopened", 0) => chat_text("reopened"),
                                _ => panic!("unscripted Chat generation #{chat_count}"),
                            };
                            // A separate streamed text delta precedes the tool call.
                            if label == "first" && chat_count == 0 {
                                assert_eq!(
                                    body["messages"].as_array().unwrap().last().unwrap()["content"],
                                    prompt
                                );
                            }
                            respond_sse(&mut socket, &reply);
                        }
                        ("POST", "/zen/v1/chat/completions") => {
                            respond_error(&mut socket);
                        }
                        _ => panic!(
                            "unexpected local request: {} {}",
                            request.method, request.path
                        ),
                    }
                    requests.push(request);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Some(status) = child.try_wait().expect("poll binary") {
                        let sent = requests.iter().filter(|r| r.method == "POST").count();
                        assert_eq!(
                            std::fs::read_dir(&self.campaign_dir).unwrap().count(),
                            before_slots + sent,
                            "main, child and title HTTP sends must all consume durable campaign slots"
                        );
                        return Run {
                            status,
                            stdout: std::fs::read_to_string(self.home.join(format!("{label}.out")))
                                .unwrap(),
                            stderr: std::fs::read_to_string(self.home.join(format!("{label}.err")))
                                .unwrap(),
                            requests,
                        };
                    }
                    if Instant::now() >= deadline {
                        child.kill().expect("stop hung binary");
                        child.wait().expect("reap binary");
                        panic!("Zen binary timeout; requests={}", requests.len());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept: {error}"),
            }
        }
    }

    fn data_dir(&self) -> &Path {
        &self.home
    }

    fn set_fixture_key(&self, key: &str) {
        let path = self.home.join("config/opencode/opencode.json");
        let mut config: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        config["provider"]["opencode"]["options"]["apiKey"] = key.into();
        std::fs::write(path, config.to_string()).unwrap();
    }

    fn enable_dcp(&self) {
        let path = self.home.join("config/opencode/opencode.json");
        let mut config: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        config["plugin"] = json!(["@tarquinen/opencode-dcp@3.1.15"]);
        config["permissions"]["compress"] = json!("allow");
        std::fs::write(path, config.to_string()).unwrap();
        std::fs::write(
            self.project.join("dcp.jsonc"),
            json!({
                "enabled": true,
                "autoUpdate": false,
                "compress": {"mode": "range", "permission": "allow"},
                "protectedFilePatterns": []
            })
            .to_string(),
        )
        .unwrap();
    }

    fn enable_subagent(&self) {
        let path = self.home.join("config/opencode/opencode.json");
        let mut config: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        config["permissions"]["subagent"] = json!("allow");
        config["agent"] = json!({"helper": {
            "description": "Offline Zen child", "mode": "subagent",
            "prompt": "You are a fixture child."
        }});
        std::fs::write(path, config.to_string()).unwrap();
    }
}

struct Run {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
    requests: Vec<Request>,
}

fn read_request(socket: &mut TcpStream) -> Request {
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut data = Vec::new();
    let mut chunk = [0; 4096];
    let end = loop {
        let count = socket.read(&mut chunk).expect("HTTP headers");
        assert!(count > 0, "early EOF");
        data.extend_from_slice(&chunk[..count]);
        assert!(data.len() < 262_144, "request exceeds fixture bound");
        if let Some(index) = data.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8(data[..end].to_vec()).expect("headers UTF-8");
    let mut first = headers.lines().next().unwrap().split_whitespace();
    let method = first.next().unwrap().to_owned();
    let path = first.next().unwrap().to_owned();
    let length: usize = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, length)| length.trim().parse().expect("content length"))
        .unwrap_or(0);
    assert!(length < 262_144, "request body bound");
    while data.len() < end + length {
        let count = socket.read(&mut chunk).expect("HTTP body");
        assert!(count > 0, "early body EOF");
        data.extend_from_slice(&chunk[..count]);
    }
    let body =
        (length > 0).then(|| serde_json::from_slice(&data[end..end + length]).expect("JSON body"));
    Request {
        method,
        path,
        headers,
        body,
    }
}

fn respond_json(socket: &mut TcpStream, body: &Value) {
    respond(socket, "application/json", &body.to_string());
}

fn respond_sse(socket: &mut TcpStream, body: &str) {
    respond(socket, "text/event-stream", body);
}

fn respond(socket: &mut TcpStream, content_type: &str, body: &str) {
    write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("peer response");
}

fn respond_error(socket: &mut TcpStream) {
    socket
        .write_all(
            b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .expect("unexpected Chat rejection");
}

fn metadata(input_price: f64) -> Value {
    json!({"opencode": {
        "id": "opencode", "npm": "@ai-sdk/openai-compatible",
        "api": "https://opencode.ai/zen/v1",
        "models": {MODEL: {
            "id": MODEL, "name": "Unlisted fixture only",
            "cost": {"input": input_price, "output": 0},
            "limit": {"context": 32768, "output": 2048},
            "tool_call": true,
            "modalities": {"input": ["text"], "output": ["text"]}
        }}
    }})
}

fn sse(value: Value) -> String {
    format!("data: {value}\n\n")
}

fn chat_text(text: &str) -> String {
    let mut events = String::new();
    events.push_str(&sse(
        json!({"choices":[{"index":0,"delta":{"content":text}}]}),
    ));
    events.push_str(&sse(
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}),
    ));
    events.push_str(&sse(
        json!({"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":4}}),
    ));
    events.push_str("data: [DONE]\n\n");
    events
}

fn chat_tool_read() -> String {
    [
        sse(json!({"choices":[{"index":0,"delta":{"content":"starting "}}]})),
        sse(json!({"choices":[{"index":0,"delta":{"tool_calls":[{
            "index":0,"id":"call_zen_read","type":"function",
            "function":{"name":"read","arguments":"{\"path\":"}
        }]}}]})),
        sse(json!({"choices":[{"index":0,"delta":{"tool_calls":[{
            "index":0,"function":{"arguments":"\"note.txt\"}"}
        }]}}]})),
        sse(json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]})),
        "data: [DONE]\n\n".to_owned(),
    ]
    .concat()
}

fn chat_tool_call(id: &str, name: &str, arguments: Value) -> String {
    [
        sse(json!({"choices":[{"index":0,"delta":{"tool_calls":[{
            "index":0,"id":id,"type":"function",
            "function":{"name":name,"arguments":arguments.to_string()}
        }]}}]})),
        sse(json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]})),
        "data: [DONE]\n\n".to_owned(),
    ]
    .concat()
}

fn assert_catalog_pairs(requests: &[Request], expected: usize) {
    let paths: Vec<_> = requests
        .iter()
        .map(|request| request.path.as_str())
        .collect();
    assert_eq!(
        paths.iter().filter(|path| **path == "/api.json").count(),
        expected,
        "{paths:?}"
    );
    assert_eq!(
        paths
            .iter()
            .filter(|path| **path == "/zen/v1/models")
            .count(),
        expected,
        "{paths:?}"
    );
    for window in requests.windows(2) {
        if window[0].path == "/api.json" {
            assert_eq!(
                window[1].path, "/zen/v1/models",
                "catalog must use live inventory"
            );
        }
    }
    for (index, request) in requests.iter().enumerate() {
        if request.method == "POST" {
            assert!(index >= 2, "Chat sent without catalog preflight");
            assert_eq!(requests[index - 2].path, "/api.json");
            assert_eq!(requests[index - 1].path, "/zen/v1/models");
        }
    }
    for request in requests.iter().filter(|request| request.method == "GET") {
        assert!(
            !request
                .headers
                .to_ascii_lowercase()
                .contains("authorization:")
        );
    }
}

fn assert_chat(request: &Request) -> &Value {
    assert_chat_for_session(request, SESSION)
}

fn assert_chat_for_session<'a>(request: &'a Request, session: &str) -> &'a Value {
    assert_eq!(request.path, "/zen/v1/chat/completions");
    let headers = request.headers.to_ascii_lowercase();
    assert!(headers.contains(&format!("user-agent: {}\r\n", oc_adapters::USER_AGENT)));
    assert!(headers.contains(&format!("x-opencode-session: {session}\r\n")));
    assert!(
        !headers.contains("x-opencode-client:"),
        "no impersonated upstream headers"
    );
    let body = request.body.as_ref().expect("Chat body");
    assert_eq!(body["model"], MODEL, "selected model must be sent verbatim");
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
    assert!(
        !headers.contains("authorization:"),
        "keyless means no bearer"
    );
    assert!(body.get("input").is_none(), "Chat request, not Responses");
    body
}

#[test]
fn zen_free_binary_stream_tool_result_final_and_reopen() {
    let fixture = Fixture::new();
    let first = fixture.run("first", "read the local note", None);
    assert!(first.status.success(), "{}", first.stderr);
    assert_eq!(first.stdout.trim(), "starting read complete");
    assert_catalog_pairs(&first.requests, 4);
    let chat: Vec<_> = first
        .requests
        .iter()
        .filter(|r| r.method == "POST")
        .collect();
    assert_eq!(chat.len(), 3, "text and call, tool result and final, title");
    let initial = assert_chat(chat[0]);
    assert!(
        initial["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["function"]["name"] == "read")
    );
    let second = assert_chat(chat[1]);
    let messages = second["messages"].as_array().unwrap();
    assert!(
        messages
            .iter()
            .any(|message| message["role"] == "assistant" && message["content"] == "starting ")
    );
    let call = messages
        .iter()
        .find(|message| message["role"] == "assistant" && message.get("tool_calls").is_some())
        .expect("assistant function call");
    assert_eq!(call["tool_calls"][0]["id"], "call_zen_read");
    assert_eq!(call["tool_calls"][0]["function"]["name"], "read");
    let output = messages
        .iter()
        .find(|message| message["role"] == "tool")
        .expect("tool result");
    assert_eq!(output["tool_call_id"], "call_zen_read");
    assert!(
        output["content"]
            .as_str()
            .unwrap()
            .contains("offline Zen read evidence")
    );
    let title = assert_chat(chat[2]);
    assert!(title["tools"].as_array().is_some_and(Vec::is_empty));
    assert_eq!(title["messages"][0]["role"], "developer");
    let db = oc_adapters::storage::Db::open(&fixture.data_dir().join("data/oc")).unwrap();
    assert!(
        db.read_history(SESSION)
            .unwrap()
            .iter()
            .any(|(_, text)| text.contains("read complete"))
    );
    drop(db);

    let reopened = fixture.run("reopened", "confirm the stored read", None);
    assert!(reopened.status.success(), "{}", reopened.stderr);
    assert_eq!(reopened.stdout.trim(), "reopened");
    assert_catalog_pairs(&reopened.requests, 2);
    let chats: Vec<_> = reopened
        .requests
        .iter()
        .filter(|r| r.method == "POST")
        .collect();
    assert_eq!(chats.len(), 1, "title persists across restart");
    let body = assert_chat(chats[0]);
    let history = body["messages"].as_array().unwrap();
    assert!(
        history
            .iter()
            .any(|message| message["role"] == "tool" && message["tool_call_id"] == "call_zen_read")
    );
    assert!(
        history
            .iter()
            .any(|message| message["role"] == "assistant" && message["content"] == "read complete")
    );
    assert!(history.iter().any(
        |message| message["role"] == "user" && message["content"] == "confirm the stored read"
    ));
}

#[test]
fn zen_free_binary_dcp_retains_tool_pair_after_compression_and_restart() {
    let fixture = Fixture::new();
    let padding = "closed-turn-filler ".repeat(160);
    let first_prompt = format!("read the local note; {padding}");
    let first = fixture.run("first", &first_prompt, None);
    assert!(first.status.success(), "{}", first.stderr);
    assert_eq!(first.stdout.trim(), "starting read complete");

    let db = oc_adapters::storage::Db::open(&fixture.data_dir().join("data/oc")).unwrap();
    let before = db.read_history_full(SESSION).unwrap();
    assert_eq!(before.len(), 2);
    let start = before[0].0.clone();
    let end = before[1].0.clone();
    drop(db);
    fixture.enable_dcp();

    let compressed = fixture.run_with_compression(
        "compress",
        "compress the completed local read",
        None,
        Some((&start, &end)),
    );
    assert!(compressed.status.success(), "{}", compressed.stderr);
    assert_eq!(compressed.stdout.trim(), "compressed turn complete");
    assert_catalog_pairs(&compressed.requests, 3);
    let chats: Vec<_> = compressed
        .requests
        .iter()
        .filter(|request| request.method == "POST")
        .collect();
    assert_eq!(chats.len(), 2, "compress call and its continuation");
    let initial = assert_chat(chats[0]);
    assert!(initial["tools"].as_array().unwrap().iter().any(|tool| {
        tool["function"]["name"] == "compress"
            && tool["function"]["parameters"]["properties"]["content"]["items"]
                ["properties"]["startId"]["type"]
                == "string"
    }));
    let initial_messages = initial["messages"].to_string();
    assert!(initial_messages.contains(&start) && initial_messages.contains(&end));
    let after = assert_chat(chats[1]);
    let messages = after["messages"].as_array().unwrap();
    let result = messages
        .iter()
        .find(|item| item["role"] == "tool" && item["tool_call_id"] == "call_zen_compress")
        .expect("structured compress tool result");
    assert!(
        !result["content"].as_str().unwrap().starts_with("error:"),
        "compression succeeded: {result}"
    );

    let db = oc_adapters::storage::Db::open(&fixture.data_dir().join("data/oc")).unwrap();
    let raw = db.read_history_full(SESSION).unwrap();
    assert_eq!(&raw[..before.len()], &before);
    let blocks = db.load_compression_blocks(SESSION).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(
        (blocks[0].start_msg.as_str(), blocks[0].end_msg.as_str()),
        (start.as_str(), end.as_str())
    );
    drop(db);

    let reopened = fixture.run("reopened", "use the completed read after restart", None);
    assert!(reopened.status.success(), "{}", reopened.stderr);
    assert_catalog_pairs(&reopened.requests, 2);
    let request = reopened
        .requests
        .iter()
        .find(|r| r.method == "POST")
        .unwrap();
    let messages = assert_chat(request)["messages"].as_array().unwrap();
    assert!(
        messages
            .iter()
            .any(|message| message["content"].as_str().is_some_and(|text| {
                text.contains("[compressed b") && text.contains("offline Zen read evidence")
            }))
    );
    assert!(
        !serde_json::to_string(messages).unwrap().contains(&padding),
        "covered filler must be absent"
    );
    let call_index = messages
        .iter()
        .position(|message| {
            message["role"] == "assistant" && message["tool_calls"][0]["id"] == "call_zen_read"
        })
        .expect("original tool call survives DCP projection");
    let result_index = messages
        .iter()
        .position(|message| message["role"] == "tool" && message["tool_call_id"] == "call_zen_read")
        .expect("original tool result survives DCP projection");
    assert!(
        call_index < result_index,
        "tool call must precede its result"
    );
    assert!(
        messages[result_index]["content"]
            .as_str()
            .unwrap()
            .contains("offline Zen read evidence")
    );
    assert!(messages.iter().any(|message| {
        message["role"] == "user" && message["content"] == "use the completed read after restart"
    }));
    let db = oc_adapters::storage::Db::open(&fixture.data_dir().join("data/oc")).unwrap();
    assert_eq!(
        &db.read_history_full(SESSION).unwrap()[..raw.len()],
        raw.as_slice()
    );
    assert_eq!(db.load_compression_blocks(SESSION).unwrap(), blocks);
}

#[test]
fn zen_free_binary_child_has_own_session_and_fresh_chat_eligibility() {
    let fixture = Fixture::new();
    fixture.enable_subagent();
    let run = fixture.run("child", "delegate the offline fixture", None);
    assert!(run.status.success(), "{}", run.stderr);
    assert_eq!(run.stdout.trim(), "parent received child result");
    assert_catalog_pairs(&run.requests, 5);
    let chats: Vec<_> = run.requests.iter().filter(|r| r.method == "POST").collect();
    assert_eq!(chats.len(), 4, "parent, child, parent continuation, title");

    let db = oc_adapters::storage::Db::open(&fixture.data_dir().join("data/oc")).unwrap();
    let children = db.children_of(SESSION).unwrap();
    assert_eq!(children.len(), 1);
    let child = &children[0];
    assert_ne!(child, SESSION);
    assert_eq!(
        db.session_meta(child).unwrap().parent_id.as_deref(),
        Some(SESSION)
    );
    let parent = assert_chat(chats[0]);
    assert!(
        parent["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| { tool["function"]["name"] == "subagent" })
    );
    let child_request = assert_chat_for_session(chats[1], child);
    let child_messages = child_request["messages"].as_array().unwrap();
    assert!(child_messages.iter().any(|message| {
        message["role"] == "user"
            && message["content"]
                == "You are a subagent spawned by another session.\ninspect fixture"
    }));
    assert!(
        !serde_json::to_string(child_messages)
            .unwrap()
            .contains("delegate the offline fixture")
    );
    let continuation = assert_chat(chats[2]);
    let messages = continuation["messages"].as_array().unwrap();
    let call = messages
        .iter()
        .position(|message| {
            message["role"] == "assistant" && message["tool_calls"][0]["id"] == "call_zen_child"
        })
        .expect("parent subagent call");
    let result = messages
        .iter()
        .position(|message| {
            message["role"] == "tool" && message["tool_call_id"] == "call_zen_child"
        })
        .expect("parent subagent result");
    assert!(call < result);
    assert!(
        messages[result]["content"]
            .as_str()
            .unwrap()
            .contains(&format!(
                "<subagent sessionID=\"{child}\" state=\"completed\">"
            ))
    );
    assert!(
        messages[result]["content"]
            .as_str()
            .unwrap()
            .contains("child inspected fixture")
    );
    assert_chat(chats[3]);
    assert!(
        db.read_history(child)
            .unwrap()
            .iter()
            .any(|(_, text)| { text == "child inspected fixture" })
    );
}

#[test]
fn zen_free_binary_rechecks_price_before_first_generation() {
    let fixture = Fixture::new();
    let run = fixture.run("price-changed", "do not send a paid generation", Some(0));
    assert!(!run.status.success(), "paid model must fail closed");
    assert_catalog_pairs(&run.requests, 2);
    assert!(
        run.requests.iter().all(|r| r.method == "GET"),
        "no paid Chat POST"
    );
    assert!(
        run.stderr
            .contains("Zen Free eligibility could not be verified"),
        "visible admission failure: {}",
        run.stderr
    );
    assert!(!run.stdout.contains("do not send a paid generation"));
}

#[test]
fn zen_free_binary_rechecks_price_after_tool_round_before_continuation() {
    let fixture = Fixture::new();
    let run = fixture.run("first", "read the local note", Some(1));
    assert!(!run.status.success(), "revoked price must fail closed");
    assert_catalog_pairs(&run.requests, 3);
    let chats: Vec<_> = run.requests.iter().filter(|r| r.method == "POST").collect();
    assert_eq!(chats.len(), 1, "no paid continuation or title");
    assert_chat(chats[0]);
    assert!(
        run.stderr
            .contains("Zen Free eligibility could not be verified")
    );
}

#[test]
fn zen_free_binary_key_is_explicit_and_public_pseudokey_is_refused() {
    let fixture = Fixture::new();
    fixture.set_fixture_key("test-zen-credential");
    let run = fixture.run("first", "read the local note", None);
    assert!(run.status.success(), "{}", run.stderr);
    for request in run.requests.iter().filter(|r| r.method == "POST") {
        assert!(
            request
                .headers
                .to_ascii_lowercase()
                .contains("authorization: bearer test-zen-credential\r\n")
        );
        assert_eq!(request.path, "/zen/v1/chat/completions");
    }
    assert!(
        run.requests
            .iter()
            .filter(|r| r.method == "GET")
            .all(|r| !r.headers.to_ascii_lowercase().contains("authorization:"))
    );

    let fixture = Fixture::new();
    fixture.set_fixture_key("public");
    let rejected = fixture.run("public", "do not send", None);
    assert!(!rejected.status.success());
    assert!(
        rejected.requests.is_empty(),
        "pseudokey must be refused before network"
    );
    assert!(!rejected.stderr.contains("Bearer public"));
}
