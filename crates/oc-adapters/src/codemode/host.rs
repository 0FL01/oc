//! Interpreter side of the host bridge: `tools.*`, `search` and `fetch`.
//!
//! Program values never cross the bridge. Tool inputs leave as JSON and results come back as
//! JSON, exactly like a provider tool call would see them.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use oc_core::tool_output::{EXECUTE_CALLS_CAP, EXECUTE_INPUT_BYTES};
use serde_json::Value as Json;
use tokio::sync::{mpsc, oneshot};

use super::catalog::Catalog;
use super::interp::{Abrupt, Cx, ErrorKind, Fatal, R, arg};
use super::json;
use super::value::{JsStr, Kind, Obj, PropKey, Value};

/// An HTTP request issued by `fetch` inside a program.
#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct FetchResponse {
    pub url: String,
    pub status: u16,
    pub status_text: String,
    pub redirected: bool,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// A request the program makes of its host. Each carries a reply channel; dropping the reply
/// sender interrupts the program.
#[derive(Debug)]
pub enum HostRequest {
    Tool {
        path: String,
        input: Json,
        reply: oneshot::Sender<Result<Json, String>>,
    },
    Fetch {
        request: FetchRequest,
        reply: oneshot::Sender<Result<FetchResponse, String>>,
    },
}

pub(super) enum HostEvent {
    Request {
        request: HostRequest,
        observed: bool,
    },
    LocalSearch {
        input: Json,
        input_limited: bool,
    },
    CallsLimited,
}

/// Public executions keep their request-only channel and scheduling. Only the
/// native presentation path captures local, effect-free observations.
pub(super) enum Transport {
    RequestOnly(mpsc::UnboundedSender<HostRequest>),
    Observed {
        sender: mpsc::UnboundedSender<HostEvent>,
        starts: Cell<usize>,
    },
}

impl Transport {
    fn reserve(&self) -> bool {
        let Self::Observed { sender, starts } = self else {
            return false;
        };
        let count = starts.get();
        if count < EXECUTE_CALLS_CAP {
            starts.set(count + 1);
            true
        } else {
            if count == EXECUTE_CALLS_CAP {
                starts.set(EXECUTE_CALLS_CAP + 1);
                let _ = sender.send(HostEvent::CallsLimited);
            }
            false
        }
    }

    fn send(&self, request: HostRequest) -> Result<(), ()> {
        match self {
            Self::RequestOnly(sender) => sender.send(request).map_err(|_| ()),
            Self::Observed { sender, .. } => sender
                .send(HostEvent::Request {
                    request,
                    observed: self.reserve(),
                })
                .map_err(|_| ()),
        }
    }

    fn search(&self, input: &serde_json::Map<String, Json>) {
        if let Self::Observed { sender, .. } = self
            && self.reserve()
        {
            let (input, input_limited) = search_preview(input);
            // Losing disposable presentation metadata must not fail a local read.
            let _ = sender.send(HostEvent::LocalSearch {
                input,
                input_limited,
            });
        }
    }
}

/// Bound new queued metadata before cloning values. Search's validated fields
/// are scalars; admit whole values, never a possibly secret-bearing prefix.
fn search_preview(input: &serde_json::Map<String, Json>) -> (Json, bool) {
    let mut fields = serde_json::Map::new();
    let mut remaining = EXECUTE_INPUT_BYTES - 2; // Object braces.
    let mut limited = false;
    for (key, value) in input {
        if !matches!(key.as_str(), "query" | "namespace" | "limit" | "offset") {
            limited = true;
            continue;
        }
        let overhead = key.len() + 3 + usize::from(!fields.is_empty());
        let budget = remaining.saturating_sub(overhead);
        let size = match value {
            Json::String(text) => text.chars().try_fold(2usize, |size, ch| {
                let bytes = match ch {
                    '"' | '\\' | '\u{8}' | '\t' | '\n' | '\u{c}' | '\r' => 2,
                    '\0'..='\u{1f}' => 6,
                    ch => ch.len_utf8(),
                };
                size.checked_add(bytes).filter(|size| *size <= budget)
            }),
            Json::Null => Some(4),
            Json::Number(number) => Some(number.to_string().len()),
            _ => None,
        };
        if let Some(size) = size
            && overhead + size <= remaining
        {
            fields.insert(key.clone(), value.clone());
            remaining -= overhead + size;
        } else {
            limited = true;
        }
    }
    (Json::Object(fields), limited)
}

pub(super) struct Host {
    pub catalog: Arc<Catalog>,
    pub transport: Transport,
    pub max_tool_calls: Option<u32>,
    pub calls: Cell<u32>,
}

/// Throws an error carrying a donor diagnostic kind and optional suggestions.
pub(super) fn diagnostic_error(
    cx: &Cx,
    kind: &str,
    message: impl Into<String>,
    suggestions: &[&str],
) -> Abrupt {
    let error = cx.make_error(ErrorKind::Error, message);
    let meta = cx.new_object();
    meta.set_data("kind", Value::str(kind));
    meta.set_data(
        "suggestions",
        cx.array_value(suggestions.iter().map(|s| Value::str(*s)).collect()),
    );
    error.borrow_mut().props.insert(
        PropKey::Sym(cx.sym_diagnostic.clone()),
        super::value::Prop::hidden(Value::Obj(meta)),
    );
    Abrupt::Throw(Value::Obj(error))
}

fn record_call(cx: &Cx) -> R<()> {
    let calls = cx.host.calls.get() + 1;
    if let Some(max) = cx.host.max_tool_calls
        && calls > max
    {
        return Err(diagnostic_error(
            cx,
            "ToolCallLimitExceeded",
            format!("Execution exceeded its tool-call limit of {max}."),
            &[],
        ));
    }
    cx.host.calls.set(calls);
    Ok(())
}

fn single_input<'a>(cx: &Cx, name: &str, args: &'a [Value]) -> R<Option<&'a Value>> {
    match args.len() {
        0 => Ok(None),
        1 => Ok(Some(&args[0])),
        _ => Err(diagnostic_error(
            cx,
            "InvalidToolInput",
            format!("Tool '{name}' expects at most one input object."),
            &[],
        )),
    }
}

async fn search(cx: &Cx, args: &[Value]) -> R<Value> {
    let input = match single_input(cx, "search", args)? {
        Some(value) if !matches!(value, Value::Undefined) => json::to_host(cx, value).await?,
        _ => Json::Object(Default::default()),
    };
    let Json::Object(input) = input else {
        return Err(diagnostic_error(
            cx,
            "InvalidToolInput",
            "Invalid input for tool 'search': expected an object",
            &[],
        ));
    };
    let text = |key: &str| -> R<Option<String>> {
        match input.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(Json::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(diagnostic_error(
                cx,
                "InvalidToolInput",
                format!("Invalid input for tool 'search': {key} must be a string"),
                &[],
            )),
        }
    };
    let count = |key: &str, min: u64| -> R<Option<usize>> {
        match input.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(value) => match value.as_u64().filter(|n| *n >= min) {
                Some(n) => Ok(Some(n as usize)),
                None => Err(diagnostic_error(
                    cx,
                    "InvalidToolInput",
                    format!("Invalid input for tool 'search': {key} must be an integer >= {min}"),
                    &[],
                )),
            },
        }
    };
    let query = text("query")?;
    let namespace = text("namespace")?;
    let limit = count("limit", 1)?;
    let offset = count("offset", 0)?.unwrap_or(0);
    record_call(cx)?;
    let result = cx
        .host
        .catalog
        .search(query.as_deref(), namespace.as_deref(), limit, offset);
    let result = json::from_json(cx, &result);
    cx.host.transport.search(&input);
    Ok(result)
}

/// Calls a host tool by path from the `tools` tree.
pub(super) async fn tool_call(cx: &Cx, path: &[JsStr], args: Vec<Value>) -> R<Value> {
    cx.tick()?;
    let segments: Vec<String> = path.iter().map(ToString::to_string).collect();
    let name = segments.join(".");
    let Some(tool) = cx.host.catalog.get(&segments) else {
        if name == "search" {
            return search(cx, &args).await;
        }
        if cx.host.catalog.is_namespace(&segments) {
            return Err(diagnostic_error(
                cx,
                "UnknownTool",
                format!("Tool '{name}' is not callable."),
                &[],
            ));
        }
        return Err(diagnostic_error(
            cx,
            "UnknownTool",
            format!("Unknown tool '{name}'."),
            &["The tool may have been removed or renamed. Use search to find available tools."],
        ));
    };
    let input = match single_input(cx, &name, &args)? {
        Some(value) if !matches!(value, Value::Undefined) => json::to_host(cx, value).await?,
        _ => Json::Object(Default::default()),
    };
    if !input.is_object() {
        return Err(diagnostic_error(
            cx,
            "InvalidToolInput",
            format!("Invalid input for tool '{name}': expected an object"),
            &["The signature may have changed. Use search to get the current signature."],
        ));
    }
    record_call(cx)?;
    let (reply, response) = oneshot::channel();
    if cx
        .host
        .transport
        .send(HostRequest::Tool {
            path: tool.path.clone(),
            input,
            reply,
        })
        .is_err()
    {
        return Err(Abrupt::Fatal(Fatal::Cancelled));
    }
    match response.await {
        Ok(Ok(result)) => Ok(json::from_json(cx, &result)),
        Ok(Err(message)) => Err(diagnostic_error(cx, "ToolFailure", message, &[])),
        Err(_) => Err(Abrupt::Fatal(Fatal::Cancelled)),
    }
}

fn header_pairs(cx: &Cx, value: &Value) -> R<Vec<(String, String)>> {
    let mut out = Vec::new();
    match value {
        Value::Undefined | Value::Null => {}
        Value::Obj(obj) => {
            if let Kind::Headers(list) = &obj.borrow().kind {
                return Ok(list.clone());
            }
            if let Some(items) = obj.array_items() {
                for item in items {
                    let pair = item.as_obj().and_then(Obj::array_items).unwrap_or_default();
                    if pair.len() != 2 {
                        return Err(cx.type_error("Header entries must be [name, value] pairs"));
                    }
                    out.push((
                        cx.to_str(&pair[0])?.to_string(),
                        cx.to_str(&pair[1])?.to_string(),
                    ));
                }
            } else {
                for key in cx.own_keys(obj, true, false) {
                    if let PropKey::Str(name) = &key {
                        let value = cx.get_data(value, name)?;
                        out.push((name.to_string(), cx.to_str(&value)?.to_string()));
                    }
                }
            }
        }
        _ => return Err(cx.type_error("Invalid headers")),
    }
    Ok(out)
}

fn response_object(cx: &Cx, response: FetchResponse) -> Value {
    let obj = cx.new_object();
    let ok = (200..300).contains(&response.status);
    obj.set_data("url", Value::str(response.url.as_str()));
    obj.set_data("status", Value::Num(f64::from(response.status)));
    obj.set_data("statusText", Value::str(response.status_text.as_str()));
    obj.set_data("ok", Value::Bool(ok));
    obj.set_data("redirected", Value::Bool(response.redirected));
    let headers = Obj::new(
        Some(cx.protos.headers.clone()),
        Kind::Headers(
            response
                .headers
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
                .collect(),
        ),
    );
    obj.set_data("headers", Value::Obj(headers));
    let body: Rc<Vec<u8>> = Rc::new(response.body);
    let text_body = body.clone();
    let text = cx.native("text", 0, move |cx, _, _| {
        let text = String::from_utf8_lossy(&text_body).into_owned();
        let (promise, cell) = super::promise::new_promise(cx)?;
        let value = cx.string(text)?;
        super::promise::fulfill(cx, &cell, value);
        Ok(promise)
    });
    obj.set_hidden("text", Value::Obj(text));
    let json_body = body.clone();
    let json_fn = cx.native("json", 0, move |cx, _, _| {
        let (promise, cell) = super::promise::new_promise(cx)?;
        let text = String::from_utf8_lossy(&json_body);
        match json::parse(cx, &text) {
            Ok(value) => super::promise::fulfill(cx, &cell, value),
            Err(Abrupt::Throw(error)) => super::promise::reject(cx, &cell, error),
            Err(other) => return Err(other),
        }
        Ok(promise)
    });
    obj.set_hidden("json", Value::Obj(json_fn));
    let bytes_body = body;
    let bytes = cx.native("bytes", 0, move |cx, _, _| {
        let (promise, cell) = super::promise::new_promise(cx)?;
        let bytes = Obj::new(
            Some(cx.protos.bytes.clone()),
            Kind::Bytes(bytes_body.to_vec()),
        );
        super::promise::fulfill(cx, &cell, Value::Obj(bytes));
        Ok(promise)
    });
    obj.set_hidden("arrayBuffer", Value::Obj(bytes.clone()));
    obj.set_hidden("bytes", Value::Obj(bytes));
    Value::Obj(obj)
}

async fn fetch(cx: Cx, args: Vec<Value>) -> R<Value> {
    cx.tick()?;
    let input = arg(&args, 0);
    let url = match &input {
        Value::Obj(obj) => match &obj.borrow().kind {
            Kind::Url(url) => url.href.clone(),
            _ => {
                let href = cx.get_data(&input, "url")?;
                cx.to_str(&href)?.to_string()
            }
        },
        other => cx.to_str(other)?.to_string(),
    };
    let init = arg(&args, 1);
    let mut method = "GET".to_string();
    let mut headers = Vec::new();
    let mut body = None;
    if let Value::Obj(_) = &init {
        let m = cx.get_str(&init, "method").await?;
        if !m.is_nullish() {
            method = cx.to_str(&m)?.to_ascii_uppercase();
        }
        let h = cx.get_str(&init, "headers").await?;
        headers = header_pairs(&cx, &h)?;
        let b = cx.get_str(&init, "body").await?;
        body = match &b {
            Value::Undefined | Value::Null => None,
            Value::Str(s) => Some(s.as_bytes().to_vec()),
            Value::Obj(obj) => match &obj.borrow().kind {
                Kind::Bytes(bytes) => Some(bytes.clone()),
                Kind::SearchParams { list, .. } => {
                    if !headers
                        .iter()
                        .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                    {
                        headers.push((
                            "content-type".into(),
                            "application/x-www-form-urlencoded;charset=UTF-8".into(),
                        ));
                    }
                    Some(super::builtins::web::serialize_params(list).into_bytes())
                }
                _ => {
                    return Err(
                        cx.type_error("fetch body must be a string, Uint8Array or URLSearchParams")
                    );
                }
            },
            other => Some(cx.to_str(other)?.as_bytes().to_vec()),
        };
    }
    let (reply, response) = oneshot::channel();
    let request = FetchRequest {
        url,
        method,
        headers,
        body,
    };
    if cx
        .host
        .transport
        .send(HostRequest::Fetch { request, reply })
        .is_err()
    {
        return Err(Abrupt::Fatal(Fatal::Cancelled));
    }
    match response.await {
        Ok(Ok(response)) => Ok(response_object(&cx, response)),
        Ok(Err(message)) => Err(cx.type_error(format!("fetch failed: {message}"))),
        Err(_) => Err(Abrupt::Fatal(Fatal::Cancelled)),
    }
}

pub(super) fn install(cx: &Cx) {
    let root: Rc<[JsStr]> = Rc::from(Vec::new());
    cx.define_global("tools", cx.tool_ref(root));
    let search_fn = cx.native_async("search", 1, |cx, _, args| {
        Box::pin(async move { search(&cx, &args).await })
    });
    cx.define_global("search", Value::Obj(search_fn));
    let fetch_fn = cx.native("fetch", 2, |cx, _, args| {
        let task_cx = cx.clone();
        let args = args.to_vec();
        super::promise::spawn_promise(cx, async move { fetch(task_cx, args).await })
    });
    cx.define_global("fetch", Value::Obj(fetch_fn));
}
