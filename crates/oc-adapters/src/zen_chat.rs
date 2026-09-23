//! Opt-in native Zen Free Chat Completions transport. Canonical runtime history
//! stays in Responses form; this module only translates at the wire boundary.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::provider::{
    self, ARGUMENT_BYTE_CAP, EVENT_CAP, GENERATION_BYTE_CAP, Generation, InputContent, InputItem,
    InputRole, ProviderError, REQUEST_BYTE_CAP, ResponsesConfig, SSE_BYTE_CAP, StreamItem,
    TEXT_DELTA_BYTE_CAP, ToolDef,
};

fn role(role: InputRole) -> &'static str {
    match role {
        InputRole::System => "system",
        InputRole::Developer => "developer",
        InputRole::User => "user",
        InputRole::Assistant => "assistant",
    }
}

fn content(parts: &[InputContent], assistant: bool) -> Result<Value, ProviderError> {
    let mut out = Vec::new();
    for part in parts {
        match part {
            InputContent::InputText { text } | InputContent::OutputText { text } => {
                out.push(json!({"type":"text", "text":text}));
            }
            InputContent::InputImage { image_url, detail } if !assistant => {
                let mut image = json!({"url":image_url});
                if let Some(detail) = detail {
                    image["detail"] = detail.clone().into();
                }
                out.push(json!({"type":"image_url", "image_url":image}));
            }
            _ => return Err(ProviderError::InvalidConfig),
        }
    }
    // Plain text works on Chat endpoints that do not accept multipart content.
    if out.iter().all(|part| part["type"] == "text") {
        Ok(Value::String(
            out.iter()
                .filter_map(|part| part["text"].as_str())
                .collect(),
        ))
    } else {
        Ok(Value::Array(out))
    }
}

fn messages(input: &[InputItem]) -> Result<Vec<Value>, ProviderError> {
    let mut messages = Vec::new();
    let mut calls = Vec::new();
    let mut pending = std::collections::BTreeSet::new();
    let flush = |messages: &mut Vec<Value>,
                 calls: &mut Vec<Value>,
                 pending: &mut std::collections::BTreeSet<String>|
     -> Result<(), ProviderError> {
        if !calls.is_empty() {
            for call in calls.iter() {
                let id = call["id"].as_str().ok_or(ProviderError::InvalidConfig)?;
                if !pending.insert(id.to_owned()) {
                    return Err(ProviderError::InvalidConfig);
                }
            }
            messages.push(json!({"role":"assistant", "content":null,
                "tool_calls":std::mem::take(calls)}));
        }
        Ok(())
    };
    for item in input {
        match item {
            InputItem::Message {
                role: r,
                content: parts,
            } => {
                flush(&mut messages, &mut calls, &mut pending)?;
                if !pending.is_empty() {
                    return Err(ProviderError::InvalidConfig);
                }
                messages.push(
                    json!({"role":role(*r),"content":content(parts, *r == InputRole::Assistant)?}),
                );
            }
            InputItem::FunctionCallOutput { call_id, output } => {
                flush(&mut messages, &mut calls, &mut pending)?;
                if !pending.remove(call_id) {
                    return Err(ProviderError::InvalidConfig);
                }
                messages.push(json!({"role":"tool","tool_call_id":call_id,"content":output}));
            }
            InputItem::ProviderOutput(value) => match value["type"].as_str() {
                Some("function_call") => {
                    if value["status"].as_str().is_some_and(|s| s != "completed") {
                        return Err(ProviderError::ResponseIncomplete);
                    }
                    let id = value["call_id"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or(ProviderError::InvalidConfig)?;
                    let name = value["name"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or(ProviderError::InvalidConfig)?;
                    let args = value["arguments"]
                        .as_str()
                        .ok_or(ProviderError::InvalidConfig)?;
                    if args.len() > ARGUMENT_BYTE_CAP {
                        return Err(ProviderError::ByteLimit("arguments"));
                    }
                    serde_json::from_str::<Value>(args)
                        .map_err(|_| ProviderError::InvalidConfig)?;
                    calls.push(json!({"id":id,"type":"function",
                        "function":{"name":name,"arguments":args}}));
                }
                Some("message") if value["role"] == "assistant" => {
                    flush(&mut messages, &mut calls, &mut pending)?;
                    if !pending.is_empty() {
                        return Err(ProviderError::InvalidConfig);
                    }
                    if value["status"].as_str().is_some_and(|s| s != "completed") {
                        return Err(ProviderError::ResponseIncomplete);
                    }
                    let parts = value["content"]
                        .as_array()
                        .ok_or(ProviderError::InvalidConfig)?;
                    let mut text = String::new();
                    for part in parts {
                        if part["type"] != "output_text" {
                            return Err(ProviderError::InvalidConfig);
                        }
                        text.push_str(part["text"].as_str().ok_or(ProviderError::InvalidConfig)?);
                    }
                    messages.push(json!({"role":"assistant","content":text}));
                }
                // Encrypted reasoning, refusal and unknown Responses items have
                // no faithful Chat representation: fail instead of losing them.
                _ => return Err(ProviderError::InvalidConfig),
            },
        }
    }
    flush(&mut messages, &mut calls, &mut pending)?;
    if !pending.is_empty() {
        return Err(ProviderError::InvalidConfig);
    }
    Ok(messages)
}

fn request_body(
    model: &str,
    input: &[InputItem],
    tools: &[ToolDef],
    max_output: u64,
) -> Result<Vec<u8>, ProviderError> {
    if model.is_empty() || max_output == 0 {
        return Err(ProviderError::InvalidConfig);
    }
    // Check borrowed input before duplicating it into the Chat projection.
    bounded_json(&(model, input, tools, max_output))?;
    let body = json!({"model":model,"messages":messages(input)?,"stream":true,
        "stream_options":{"include_usage":true},"max_tokens":max_output,
        "tools":tools.iter().map(|tool| json!({"type":"function","function":{
            "name":tool.name,"description":tool.description,"parameters":tool.parameters
        }})).collect::<Vec<_>>()});
    bounded_json(&body)
}

fn bounded_json(value: &impl serde::Serialize) -> Result<Vec<u8>, ProviderError> {
    struct Writer(Vec<u8>);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > REQUEST_BYTE_CAP {
                return Err(std::io::Error::other("request byte limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(Vec::new());
    serde_json::to_writer(&mut writer, value).map_err(|_| ProviderError::ByteLimit("request"))?;
    Ok(writer.0)
}

fn headers(config: &ResponsesConfig, session_id: &str) -> Result<HeaderMap, ProviderError> {
    if session_id.is_empty() {
        return Err(ProviderError::InvalidConfig);
    }
    let mut headers = HeaderMap::new();
    for (key, value) in &config.headers {
        let name =
            HeaderName::from_bytes(key.as_bytes()).map_err(|_| ProviderError::InvalidConfig)?;
        if matches!(
            name.as_str(),
            "authorization"
                | "user-agent"
                | "host"
                | "accept"
                | "content-type"
                | "content-length"
                | "transfer-encoding"
                | "connection"
                | "upgrade"
                | "trailer"
                | "te"
                | "proxy-authorization"
                | "proxy-connection"
        ) || name.as_str().starts_with("x-opencode-")
        {
            return Err(ProviderError::InvalidConfig);
        }
        let mut value = HeaderValue::from_str(value).map_err(|_| ProviderError::InvalidConfig)?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    // Official-client parity: a request without a configured key is anonymous
    // and carries the literal `public` marker the upstream plugin sets as its
    // apiKey. It is not a credential and grants nothing by itself; the gateway
    // decides whether anonymous/free use is allowed for this client.
    let key = config.api_key.trim();
    let bearer = if key.is_empty() || key.eq_ignore_ascii_case("public") {
        "public"
    } else {
        key
    };
    let mut auth = HeaderValue::from_str(&format!("Bearer {bearer}"))
        .map_err(|_| ProviderError::InvalidConfig)?;
    auth.set_sensitive(true);
    headers.insert("authorization", auth);
    let mut session =
        HeaderValue::from_str(session_id).map_err(|_| ProviderError::InvalidConfig)?;
    session.set_sensitive(true);
    headers.insert("x-opencode-session", session);
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    Ok(headers)
}

type DnsError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug)]
struct BlockedAddress;
impl std::fmt::Display for BlockedAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("non-public address refused")
    }
}
impl std::error::Error for BlockedAddress {}

pub(crate) struct GuardedResolver {
    pub(crate) allow_private: bool,
}
impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        let allow = self.allow_private;
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host, 0))
                .await
                .map_err(|err| Box::new(err) as DnsError)?;
            let addresses: Vec<_> = addresses.collect();
            if addresses.is_empty()
                || addresses
                    .iter()
                    .any(|address| !allowed(address.ip(), allow))
            {
                return Err(Box::new(BlockedAddress) as DnsError);
            }
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

pub(crate) fn allowed(ip: IpAddr, allow_private: bool) -> bool {
    if allow_private && ip.is_loopback() {
        return true;
    }
    let benchmark = match ip {
        IpAddr::V4(v4) => v4.octets()[0] == 198 && (v4.octets()[1] & 0xfe) == 18,
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .is_some_and(|v4| v4.octets()[0] == 198 && (v4.octets()[1] & 0xfe) == 18),
    };
    !benchmark && crate::webfetch::ip_is_public(ip)
}

fn url(config: &ResponsesConfig) -> Result<reqwest::Url, ProviderError> {
    let base = config.base_url.trim();
    let parsed = reqwest::Url::parse(base).map_err(|_| ProviderError::InvalidConfig)?;
    let official = parsed.scheme() == "https"
        && parsed.host_str() == Some("opencode.ai")
        && parsed.port().is_none();
    let fixture = config.allow_private
        && parsed.scheme() == "http"
        && matches!(parsed.host_str(), Some("127.0.0.1" | "[::1]"));
    if (!official && !fixture)
        || parsed.path().trim_end_matches('/') != "/zen/v1"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(ProviderError::InvalidConfig);
    }
    let endpoint = format!("{}/chat/completions", base.trim_end_matches('/'));
    reqwest::Url::parse(&endpoint).map_err(|_| ProviderError::InvalidConfig)
}

/// Admission at the point of each actual generation, including title/child
/// requests. A startup snapshot alone cannot guarantee a limited-time price.
/// Never submit a Chat request on stale or ambiguous catalog data.
#[allow(clippy::too_many_arguments)]
pub async fn stream_free_input_observed(
    config: &ResponsesConfig,
    model: &str,
    session_id: &str,
    input: &[InputItem],
    tools: &[ToolDef],
    max_output: u64,
    cancel: &AtomicBool,
    observe: &mut (dyn FnMut(&StreamItem) + Send),
) -> Result<Generation, ProviderError> {
    // Only the explicit loopback test mode can redirect the metadata feed.
    let metadata = if config.allow_private
        && matches!(
            reqwest::Url::parse(&config.base_url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned))
                .as_deref(),
            Some("127.0.0.1" | "[::1]")
        ) {
        std::env::var("OC_TEST_ZEN_METADATA_URL")
            .unwrap_or_else(|_| "https://models.dev/api.json".to_owned())
    } else {
        "https://models.dev/api.json".to_owned()
    };
    let verified = tokio::select! {
        biased;
        () = provider::wait_cancel(cancel) => return Err(ProviderError::Cancelled),
        result = crate::zen_catalog::fetch(&metadata, &config.base_url, config.allow_private) => result,
    }
    .map_err(|_| ProviderError::ZenFreeUnverified)?;
    if !verified.contains_key(model) {
        return Err(ProviderError::ZenFreeUnverified);
    }
    stream_input_observed(
        config, model, session_id, input, tools, max_output, cancel, observe,
    )
    .await
}

async fn preflight(url: &reqwest::Url, allow: bool) -> Result<(), ProviderError> {
    let host = url
        .host_str()
        .ok_or(ProviderError::InvalidConfig)?
        .trim_matches(['[', ']']);
    if let Ok(ip) = host.parse::<IpAddr>() {
        return if allowed(ip, allow) {
            Ok(())
        } else {
            Err(ProviderError::PrivateHost)
        };
    }
    let port = url
        .port_or_known_default()
        .ok_or(ProviderError::InvalidConfig)?;
    let addresses: Vec<_> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| ProviderError::PrivateHost)?
        .collect();
    if addresses.is_empty() || addresses.iter().any(|addr| !allowed(addr.ip(), allow)) {
        return Err(ProviderError::PrivateHost);
    }
    Ok(())
}

fn transport(err: &reqwest::Error) -> ProviderError {
    let mut source: Option<&dyn std::error::Error> = Some(err);
    while let Some(err) = source {
        if err.is::<BlockedAddress>() {
            return ProviderError::PrivateHost;
        }
        source = err.source();
    }
    if err.is_timeout() {
        ProviderError::Deadline
    } else {
        ProviderError::Transport
    }
}

// Optional product-test guard, not a claim of a billing limit for ordinary
// users. Atomic create_new slots are durable before the actual HTTP send;
// another process/restart cannot reuse a slot or exceed 24 attempts. The
// caller supplies a private existing campaign directory in an isolated HOME.
fn reserve_live_smoke(dir: &Path, max_output: u64) -> Result<(), ProviderError> {
    if max_output > 2048 || !dir.is_absolute() {
        return Err(ProviderError::InvalidConfig);
    }
    let metadata = std::fs::symlink_metadata(dir).map_err(|_| ProviderError::InvalidConfig)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || dir
            .canonicalize()
            .map_err(|_| ProviderError::InvalidConfig)?
            != dir
    {
        return Err(ProviderError::InvalidConfig);
    }
    #[cfg(unix)]
    if std::os::unix::fs::PermissionsExt::mode(&metadata.permissions()) & 0o077 != 0 {
        return Err(ProviderError::InvalidConfig);
    }
    for attempt in 0..24 {
        let path = dir.join(format!("generation-{attempt:02}"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        match options.open(path) {
            Ok(file) => {
                file.sync_all().map_err(|_| ProviderError::InvalidConfig)?;
                std::fs::File::open(dir)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|_| ProviderError::InvalidConfig)?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(ProviderError::InvalidConfig),
        }
    }
    Err(ProviderError::ZenCampaignExhausted)
}

#[derive(Default)]
struct Call {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
    announced: bool,
    item_id: String,
}

#[derive(Default)]
struct Decoder {
    line: Vec<u8>,
    data: String,
    events: usize,
    retained: usize,
    done: bool,
    finish: Option<String>,
    calls: BTreeMap<usize, Call>,
    items: Vec<StreamItem>,
    text: String,
    usage: Option<(u64, u64)>,
}

impl Decoder {
    fn emit(&mut self, item: StreamItem, observe: &mut (dyn FnMut(&StreamItem) + Send)) {
        observe(&item);
        self.items.push(item);
    }

    fn push(
        &mut self,
        bytes: &[u8],
        observe: &mut (dyn FnMut(&StreamItem) + Send),
    ) -> Result<(), ProviderError> {
        for segment in bytes.split_inclusive(|byte| *byte == b'\n') {
            if self.done {
                break;
            }
            if self.line.len().saturating_add(segment.len()) > SSE_BYTE_CAP {
                return Err(ProviderError::ByteLimit("SSE line"));
            }
            self.line.extend_from_slice(segment);
            if !segment.ends_with(b"\n") {
                continue;
            }
            let line = std::str::from_utf8(&self.line)
                .map_err(|_| ProviderError::InvalidUtf8)?
                .trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                self.dispatch(observe)?;
            } else if let Some(data) = line.strip_prefix("data:") {
                let data = data.strip_prefix(' ').unwrap_or(data);
                if self.data.len().saturating_add(data.len()).saturating_add(1) > SSE_BYTE_CAP {
                    return Err(ProviderError::ByteLimit("SSE event"));
                }
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(data);
            }
            self.line.clear();
        }
        Ok(())
    }

    fn dispatch(
        &mut self,
        observe: &mut (dyn FnMut(&StreamItem) + Send),
    ) -> Result<(), ProviderError> {
        if self.data.is_empty() {
            return Ok(());
        }
        self.events += 1;
        if self.events > EVENT_CAP {
            return Err(ProviderError::EventCap);
        }
        let data = std::mem::take(&mut self.data);
        if data == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        self.retained = self
            .retained
            .saturating_add(data.len().saturating_mul(4).saturating_add(256));
        if self.retained > GENERATION_BYTE_CAP {
            return Err(ProviderError::ByteLimit("generation"));
        }
        let value: Value = serde_json::from_str(&data).map_err(|_| ProviderError::Incomplete)?;
        if value.get("error").is_some() {
            return Err(ProviderError::Failed);
        }
        let choices = value["choices"]
            .as_array()
            .ok_or(ProviderError::Incomplete)?;
        if choices.len() > 1 {
            return Err(ProviderError::Incomplete);
        }
        if let (Some(input), Some(output)) = (
            value
                .pointer("/usage/prompt_tokens")
                .and_then(Value::as_u64),
            value
                .pointer("/usage/completion_tokens")
                .and_then(Value::as_u64),
        ) && self.usage.is_none()
        {
            self.usage = Some((input, output));
            self.emit(
                StreamItem::Usage {
                    input_tokens: input,
                    output_tokens: output,
                },
                observe,
            );
        }
        for choice in choices {
            if choice["index"].as_u64() != Some(0) {
                return Err(ProviderError::Incomplete);
            }
            let delta = choice["delta"]
                .as_object()
                .ok_or(ProviderError::Incomplete)?;
            if delta
                .keys()
                .any(|key| !matches!(key.as_str(), "role" | "content" | "tool_calls"))
                || !matches!(
                    delta.get("role"),
                    None | Some(Value::Null) | Some(Value::String(_))
                )
                || delta
                    .get("role")
                    .and_then(Value::as_str)
                    .is_some_and(|role| role != "assistant")
                || !matches!(
                    delta.get("content"),
                    None | Some(Value::Null) | Some(Value::String(_))
                )
                || !matches!(
                    delta.get("tool_calls"),
                    None | Some(Value::Null) | Some(Value::Array(_))
                )
                || !matches!(
                    choice.get("finish_reason"),
                    None | Some(Value::Null) | Some(Value::String(_))
                )
            {
                // Reasoning/refusals/audio and other opaque Chat extensions
                // cannot be preserved in a Responses continuation.
                return Err(ProviderError::InvalidConfig);
            }
            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                if self.finish.is_some() {
                    return Err(ProviderError::Incomplete);
                }
                for fragment in fragments(text) {
                    self.text.push_str(fragment);
                    self.emit(StreamItem::TextDelta(fragment.to_owned()), observe);
                }
            }
            if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                if self.finish.is_some() {
                    return Err(ProviderError::Incomplete);
                }
                for call in calls {
                    let fields = call.as_object().ok_or(ProviderError::Incomplete)?;
                    if fields
                        .keys()
                        .any(|key| !matches!(key.as_str(), "index" | "id" | "type" | "function"))
                        || !matches!(
                            fields.get("id"),
                            None | Some(Value::Null) | Some(Value::String(_))
                        )
                        || !matches!(
                            fields.get("type"),
                            None | Some(Value::Null) | Some(Value::String(_))
                        )
                        || !matches!(
                            fields.get("function"),
                            None | Some(Value::Null) | Some(Value::Object(_))
                        )
                    {
                        return Err(ProviderError::Incomplete);
                    }
                    let index =
                        usize::try_from(call["index"].as_u64().ok_or(ProviderError::Incomplete)?)
                            .map_err(|_| ProviderError::Incomplete)?;
                    if call["type"].as_str().is_some_and(|ty| ty != "function") {
                        return Err(ProviderError::Incomplete);
                    }
                    if let Some(function) = fields.get("function").and_then(Value::as_object)
                        && (function
                            .keys()
                            .any(|key| !matches!(key.as_str(), "name" | "arguments"))
                            || !matches!(
                                function.get("name"),
                                None | Some(Value::Null) | Some(Value::String(_))
                            )
                            || !matches!(
                                function.get("arguments"),
                                None | Some(Value::Null) | Some(Value::String(_))
                            ))
                    {
                        return Err(ProviderError::Incomplete);
                    }
                    let entry = self.calls.entry(index).or_default();
                    if let Some(id) = call["id"].as_str() {
                        if id.is_empty() || entry.id.as_deref().is_some_and(|old| old != id) {
                            return Err(ProviderError::Incomplete);
                        }
                        entry.id = Some(id.to_owned());
                    }
                    if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                        if name.is_empty() || entry.name.as_deref().is_some_and(|old| old != name) {
                            return Err(ProviderError::Incomplete);
                        }
                        entry.name = Some(name.to_owned());
                    }
                    if let Some(args) = call.pointer("/function/arguments").and_then(Value::as_str)
                    {
                        if entry.arguments.len().saturating_add(args.len()) > ARGUMENT_BYTE_CAP {
                            return Err(ProviderError::ByteLimit("arguments"));
                        }
                        entry.arguments.push_str(args);
                    }
                    let mut emitted = Vec::new();
                    if !entry.announced {
                        if let (Some(id), Some(name)) = (&entry.id, &entry.name) {
                            entry.item_id =
                                format!("zen_item_{index}_{:x}", Sha256::digest(id.as_bytes()));
                            if entry.item_id == *id {
                                entry.item_id.push('_');
                            }
                            emitted.push(StreamItem::ToolCallStarted {
                                item_id: entry.item_id.clone(),
                                call_id: id.clone(),
                                name: name.clone(),
                            });
                            entry.announced = true;
                            for fragment in fragments(&entry.arguments) {
                                emitted.push(StreamItem::ArgDelta {
                                    item_id: entry.item_id.clone(),
                                    delta: fragment.to_owned(),
                                });
                            }
                        }
                    } else if let Some(args) =
                        call.pointer("/function/arguments").and_then(Value::as_str)
                    {
                        for fragment in fragments(args) {
                            emitted.push(StreamItem::ArgDelta {
                                item_id: entry.item_id.clone(),
                                delta: fragment.to_owned(),
                            });
                        }
                    }
                    for item in emitted {
                        self.emit(item, observe);
                    }
                }
            }
            if let Some(reason) = choice["finish_reason"].as_str() {
                if !matches!(reason, "stop" | "tool_calls") || self.finish.is_some() {
                    return Err(ProviderError::ResponseIncomplete);
                }
                self.finish = Some(reason.to_owned());
            }
        }
        Ok(())
    }

    fn complete(self) -> Result<Generation, ProviderError> {
        if !self.done || !self.line.is_empty() || !self.data.is_empty() {
            return Err(ProviderError::Incomplete);
        }
        let finish = self
            .finish
            .as_deref()
            .ok_or(ProviderError::ResponseIncomplete)?;
        if (finish == "tool_calls") == self.calls.is_empty() {
            return Err(ProviderError::ResponseIncomplete);
        }
        let mut output = Vec::new();
        if !self.text.is_empty() {
            output.push(
                json!({"type":"message","role":"assistant","status":"completed",
                "content":[{"type":"output_text","text":self.text}]}),
            );
        }
        let mut ids = std::collections::BTreeSet::new();
        for (_, call) in self.calls {
            let (Some(id), Some(name)) = (call.id, call.name) else {
                return Err(ProviderError::ResponseIncomplete);
            };
            if !call.announced
                || call.arguments.is_empty()
                || serde_json::from_str::<Value>(&call.arguments).is_err()
                || !ids.insert(id.clone())
            {
                return Err(ProviderError::ResponseIncomplete);
            }
            output.push(
                json!({"type":"function_call","id":call.item_id,"call_id":id,
                "name":name,"arguments":call.arguments,"status":"completed"}),
            );
        }
        Ok(Generation {
            output,
            items: self.items,
            text: self.text,
            usage: self.usage,
        })
    }
}

fn fragments(text: &str) -> Vec<&str> {
    let mut rest = text;
    let mut result = Vec::new();
    while !rest.is_empty() {
        let mut end = rest.len().min(TEXT_DELTA_BYTE_CAP);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        result.push(&rest[..end]);
        rest = &rest[end..];
    }
    result
}

/// Stream a single Chat generation. Only terminal canonical output is executable;
/// emitted partial deltas never become function calls on failure/cancellation.
#[allow(clippy::too_many_arguments)]
pub async fn stream_input_observed(
    config: &ResponsesConfig,
    model: &str,
    session_id: &str,
    input: &[InputItem],
    tools: &[ToolDef],
    max_output: u64,
    cancel: &AtomicBool,
    observe: &mut (dyn FnMut(&StreamItem) + Send),
) -> Result<Generation, ProviderError> {
    if config.timeout == Some(true) {
        return Err(ProviderError::InvalidConfig);
    }
    let body = request_body(model, input, tools, max_output)?;
    let headers = headers(config, session_id)?;
    let url = url(config)?;
    tokio::select! {
        biased;
        () = provider::wait_cancel(cancel) => return Err(ProviderError::Cancelled),
        result = tokio::time::timeout(config.connect_timeout, preflight(&url, config.allow_private)) => {
            result.map_err(|_| ProviderError::Deadline)??;
        }
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .user_agent(crate::USER_AGENT)
        .connect_timeout(config.connect_timeout)
        .dns_resolver(Arc::new(GuardedResolver {
            allow_private: config.allow_private,
        }))
        .build()
        .map_err(|_| ProviderError::Transport)?;
    let idle = Duration::from_millis(config.chunk_timeout_ms);
    if let Some(dir) = std::env::var_os("OC_TEST_ZEN_CAMPAIGN_DIR") {
        reserve_live_smoke(Path::new(&dir), max_output)?;
    }
    let mut response = tokio::select! {
        biased;
        () = provider::wait_cancel(cancel) => return Err(ProviderError::Cancelled),
        result = tokio::time::timeout(idle, client.post(url).headers(headers).body(body).send()) => {
            result.map_err(|_| ProviderError::IdleTimeout)?.map_err(|err| transport(&err))?
        }
    };
    if let Some(peer) = response.remote_addr()
        && !allowed(peer.ip(), config.allow_private)
    {
        return Err(ProviderError::PrivateHost);
    }
    match response.status().as_u16() {
        200 => {}
        401 => return Err(ProviderError::Unauthorized),
        403 => return Err(ProviderError::Forbidden),
        429 => return Err(ProviderError::RateLimited),
        500..=599 => return Err(ProviderError::Server),
        status => return Err(ProviderError::HttpStatus(status)),
    }
    let mut decoder = Decoder::default();
    loop {
        let chunk = tokio::select! {
            biased;
            () = provider::wait_cancel(cancel) => return Err(ProviderError::Cancelled),
            result = tokio::time::timeout(idle, response.chunk()) => {
                result.map_err(|_| ProviderError::IdleTimeout)?.map_err(|err| transport(&err))?
            }
        };
        match chunk {
            Some(bytes) => decoder.push(&bytes, observe)?,
            None => break,
        }
        if decoder.done {
            break;
        }
    }
    decoder.complete()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::Ordering;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    #[ignore = "opt-in direct Zen Free synthetic smoke; durable campaign directory and explicit current model required"]
    async fn live_free_chat_text_smoke() {
        assert_eq!(std::env::var("OC_TEST_ZEN_LIVE").as_deref(), Ok("1"));
        let model =
            std::env::var("OC_TEST_ZEN_MODEL").expect("explicit current free Chat model ID");
        assert!(!model.is_empty());
        let dir = std::env::var_os("OC_TEST_ZEN_CAMPAIGN_DIR")
            .expect("existing private durable campaign directory");
        assert!(Path::new(&dir).is_dir());
        let config = ResponsesConfig {
            base_url: "https://opencode.ai/zen/v1".into(),
            api_key: std::env::var("OC_TEST_ZEN_API_KEY").unwrap_or_default(),
            timeout: Some(false),
            chunk_timeout_ms: 30_000,
            connect_timeout: Duration::from_secs(5),
            allow_private: false,
            headers: BTreeMap::new(),
            set_cache_key: false,
        };
        let cancel = AtomicBool::new(false);
        let result = stream_free_input_observed(
            &config,
            &model,
            "s-oc-zen-synthetic-smoke",
            &[InputItem::message(
                InputRole::User,
                "Synthetic test only. Please answer briefly with a single word.",
            )],
            &[],
            32,
            &cancel,
            &mut |_| {},
        )
        .await
        .expect("direct Zen Free text smoke (only sanitized error category)");
        assert!(!result.text.trim().is_empty(), "nonempty text response");
        assert!(result.output.iter().any(|item| item["type"] == "message"));
    }

    #[test]
    fn live_campaign_slots_survive_restart_and_cap_actual_sends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .unwrap();
        assert_eq!(
            reserve_live_smoke(&path, 2049),
            Err(ProviderError::InvalidConfig)
        );
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 0);
        for _ in 0..24 {
            assert_eq!(reserve_live_smoke(&path, 2048), Ok(()));
        }
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 24);
        assert_eq!(
            reserve_live_smoke(&path, 256),
            Err(ProviderError::ZenCampaignExhausted)
        );
    }

    fn config(base: String) -> ResponsesConfig {
        ResponsesConfig {
            base_url: base,
            api_key: "test-key".into(),
            timeout: Some(false),
            chunk_timeout_ms: 200,
            connect_timeout: Duration::from_secs(2),
            allow_private: true,
            headers: BTreeMap::new(),
            set_cache_key: false,
        }
    }

    fn event(value: Value) -> String {
        format!("data: {value}\n\n")
    }

    async fn server(
        status: &'static str,
        chunks: Vec<(String, u64)>,
    ) -> (String, Arc<Mutex<Vec<u8>>>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/zen/v1", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let captured = seen.clone();
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut raw = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 {
                    return;
                }
                raw.extend_from_slice(&buf[..n]);
                if let Some(end) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&raw[..end]);
                    let len: usize = headers
                        .lines()
                        .filter_map(|line| {
                            line.split_once(':')
                                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                                .and_then(|(_, value)| value.trim().parse().ok())
                        })
                        .next()
                        .unwrap_or(0);
                    if raw.len() >= end + 4 + len {
                        break;
                    }
                }
                assert!(raw.len() < REQUEST_BYTE_CAP);
            }
            *captured.lock().unwrap() = raw;
            socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
            for (chunk, delay) in chunks {
                tokio::time::sleep(Duration::from_millis(delay)).await;
                if socket.write_all(chunk.as_bytes()).await.is_err() {
                    return;
                }
            }
        });
        (base, seen, handle)
    }

    static CANCEL: AtomicBool = AtomicBool::new(false);

    #[tokio::test]
    async fn chat_roundtrip_headers_history_tools_and_interleaved_deltas() {
        let events = [
            event(json!({"choices":[{"index":0,"delta":{"content":"hello "}}]})),
            event(json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"id":"call_b","type":"function","function":{"name":"grep","arguments":"{\"pattern\":"}}]}}]})),
            event(json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"read","arguments":"{\"path\":"}}]}}]})),
            event(json!({"choices":[{"index":0,"delta":{"content":"world","tool_calls":[{"index":1,"function":{"arguments":"\"x\"}"}},{"index":0,"function":{"arguments":"\"y\"}"}}]}}]})),
            event(json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]})),
            event(json!({"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":8}})),
            "data: [DONE]\n\n".to_owned(),
        ].concat();
        let (base, seen, handle) = server("200 OK", vec![(events, 0)]).await;
        let input = vec![
            InputItem::message(InputRole::System, "rules"),
            InputItem::message(InputRole::User, "question"),
            InputItem::ProviderOutput(
                json!({"type":"message","id":"msg_old","role":"assistant","status":"completed","content":[{"type":"output_text","text":"before"}]}),
            ),
            InputItem::ProviderOutput(
                json!({"type":"function_call","id":"fc_old","call_id":"old_call","name":"read","arguments":"{}","status":"completed"}),
            ),
            InputItem::FunctionCallOutput {
                call_id: "old_call".into(),
                output: "file".into(),
            },
        ];
        let tool = ToolDef {
            name: "read".into(),
            description: "read a file".into(),
            parameters: json!({"type":"object"}),
        };
        let mut observed = Vec::new();
        let generation = stream_input_observed(
            &config(base),
            "dynamic-zen-model",
            "session-1",
            &input,
            &[tool],
            500,
            &CANCEL,
            &mut |item| observed.push(item.clone()),
        )
        .await
        .unwrap();
        handle.await.unwrap();
        let raw = seen.lock().unwrap();
        let request = String::from_utf8_lossy(&raw);
        let (header, body) = request.split_once("\r\n\r\n").unwrap();
        assert!(header.starts_with("POST /zen/v1/chat/completions HTTP/1.1\r\n"));
        let header = header.to_ascii_lowercase();
        assert!(header.contains("authorization: bearer test-key"));
        assert!(header.contains("x-opencode-session: session-1"));
        assert!(header.contains(&format!("user-agent: {}", crate::USER_AGENT)));
        assert!(!header.contains("x-opencode-client:"));
        let body: Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["model"], "dynamic-zen-model");
        assert_eq!(body["max_tokens"], 500);
        assert_eq!(body["messages"][0]["content"], "rules");
        assert_eq!(body["messages"][2]["content"], "before");
        assert_eq!(body["messages"][3]["tool_calls"][0]["id"], "old_call");
        assert_eq!(body["messages"][4]["tool_call_id"], "old_call");
        assert_eq!(body["tools"][0]["function"]["name"], "read");
        assert_eq!(generation.text, "hello world");
        assert_eq!(generation.usage, Some((12, 8)));
        assert_eq!(generation.items, observed);
        assert_eq!(generation.output.len(), 3);
        assert_eq!(generation.output[0]["content"][0]["text"], "hello world");
        assert_eq!(generation.output[1]["call_id"], "call_a");
        assert_eq!(generation.output[2]["call_id"], "call_b");
        for item in generation.output.iter().skip(1) {
            assert_ne!(item["id"], item["call_id"]);
            serde_json::from_str::<Value>(item["arguments"].as_str().unwrap()).unwrap();
        }
        assert!(observed.iter().any(|item| matches!(item, StreamItem::ArgDelta { item_id, delta } if item_id.starts_with("zen_item_1_") && delta == "\"x\"}")));
    }

    #[test]
    fn incomplete_and_opaque_rejected_without_executable_calls() {
        let mut decoder = Decoder::default();
        decoder.push(event(json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call","function":{"name":"read","arguments":"{"}}]},"finish_reason":"tool_calls"}]})).as_bytes(), &mut |_| {}).unwrap();
        decoder.push(b"data: [DONE]\n\n", &mut |_| {}).unwrap();
        assert!(matches!(
            decoder.complete(),
            Err(ProviderError::ResponseIncomplete)
        ));
        assert_eq!(
            messages(&[InputItem::ProviderOutput(
                json!({"type":"reasoning","encrypted_content":"secret"})
            )]),
            Err(ProviderError::InvalidConfig)
        );
        let mut decoder = Decoder::default();
        decoder
            .push(
                event(json!({"choices":[{"index":0,"delta":{"content":"partial"},"finish_reason":"stop"}]}))
                    .as_bytes(),
                &mut |_| {},
            )
            .unwrap();
        assert!(matches!(decoder.complete(), Err(ProviderError::Incomplete)));
    }

    #[test]
    fn malformed_chat_content_and_unpaired_history_never_succeed() {
        for delta in [
            json!({"content":{"text":"lost"}}),
            json!({"tool_calls":{"index":0}}),
            json!({"role":"tool","content":"lost"}),
            json!({"tool_calls":[{"index":0,"function":{"name":4}}]}),
        ] {
            let mut decoder = Decoder::default();
            let malformed =
                event(json!({"choices":[{"index":0,"delta":delta,"finish_reason":"stop"}]}));
            assert!(decoder.push(malformed.as_bytes(), &mut |_| {}).is_err());
        }
        let mut decoder = Decoder::default();
        let malformed =
            event(json!({"choices":[{"delta":{"content":"lost"},"finish_reason":"stop"}]}));
        assert_eq!(
            decoder.push(malformed.as_bytes(), &mut |_| {}),
            Err(ProviderError::Incomplete)
        );
        let call = InputItem::ProviderOutput(
            json!({"type":"function_call","id":"item","call_id":"call","name":"read","arguments":"{}"}),
        );
        let result = InputItem::FunctionCallOutput {
            call_id: "call".into(),
            output: "ok".into(),
        };
        assert_eq!(
            messages(std::slice::from_ref(&result)),
            Err(ProviderError::InvalidConfig)
        );
        assert_eq!(
            messages(&[call.clone(), result.clone(), result.clone()]),
            Err(ProviderError::InvalidConfig)
        );
        assert_eq!(
            messages(std::slice::from_ref(&call)),
            Err(ProviderError::InvalidConfig)
        );
        assert!(messages(&[InputItem::ProviderOutput(json!({"type":"function_call","id":"item","call_id":"call","name":"read","arguments":"{}"})), result]).is_ok());
        assert!(!allowed("198.18.0.1".parse().unwrap(), false));
    }

    #[tokio::test]
    async fn keyless_and_public_use_the_official_anonymous_marker() {
        for key in ["", "public", "PUBLIC", "  "] {
            let (base, seen, handle) = server("200 OK", vec![("data: [DONE]\n\n".into(), 0)]).await;
            let mut cfg = config(base);
            cfg.api_key = key.into();
            let _ =
                stream_input_observed(&cfg, "model", "session", &[], &[], 10, &CANCEL, &mut |_| {})
                    .await;
            handle.await.unwrap();
            let wire = String::from_utf8_lossy(&seen.lock().unwrap()).to_ascii_lowercase();
            assert!(
                wire.contains("authorization: bearer public\r\n"),
                "key={key:?} must be the anonymous marker: {wire}"
            );
        }
        let (base, seen, handle) = server("200 OK", vec![("data: [DONE]\n\n".into(), 0)]).await;
        let mut cfg = config(base);
        cfg.api_key = "oc_sk_fixture".into();
        let _ = stream_input_observed(&cfg, "model", "session", &[], &[], 10, &CANCEL, &mut |_| {})
            .await;
        handle.await.unwrap();
        let wire = String::from_utf8_lossy(&seen.lock().unwrap()).to_ascii_lowercase();
        assert!(
            wire.contains("authorization: bearer oc_sk_fixture\r\n"),
            "real key must be sent: {wire}"
        );
    }

    #[tokio::test]
    async fn statuses_cancel_and_header_spoof_refusal() {
        for (status, expected) in [
            ("401 Unauthorized", ProviderError::Unauthorized),
            ("403 Forbidden", ProviderError::Forbidden),
        ] {
            let (base, _, handle) = server(status, vec![]).await;
            let error = stream_input_observed(
                &config(base),
                "model",
                "session",
                &[],
                &[],
                10,
                &CANCEL,
                &mut |_| {},
            )
            .await
            .unwrap_err();
            assert_eq!(error, expected);
            handle.await.unwrap();
        }
        let mut cfg = config("http://127.0.0.1:9/zen/v1".into());
        cfg.headers
            .insert("User-Agent".into(), "upstream-client".into());
        assert_eq!(
            stream_input_observed(&cfg, "model", "session", &[], &[], 10, &CANCEL, &mut |_| {})
                .await,
            Err(ProviderError::InvalidConfig)
        );
        cfg.headers.clear();
        cfg.headers
            .insert("x-opencode-client".into(), "impostor".into());
        assert_eq!(
            stream_input_observed(&cfg, "model", "session", &[], &[], 10, &CANCEL, &mut |_| {})
                .await,
            Err(ProviderError::InvalidConfig)
        );
        cfg.headers.clear();
        cfg.api_key = "test-key".into();
        cfg.allow_private = false;
        assert_eq!(
            stream_input_observed(&cfg, "model", "session", &[], &[], 10, &CANCEL, &mut |_| {})
                .await,
            Err(ProviderError::InvalidConfig)
        );
        let (base, _, handle) = server(
            "200 OK",
            vec![
                (
                    event(json!({"choices":[{"index":0,"delta":{"content":"start"}}]})),
                    0,
                ),
                ("data: [DONE]\n\n".into(), 500),
            ],
        )
        .await;
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let mut cfg = config(base);
        cfg.chunk_timeout_ms = 1000;
        let error = stream_input_observed(
            &cfg,
            "model",
            "session",
            &[],
            &[],
            10,
            &cancel,
            &mut move |item| {
                if matches!(item, StreamItem::TextDelta(_)) {
                    flag.store(true, Ordering::Relaxed);
                }
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error, ProviderError::Cancelled);
        handle.abort();
    }

    #[tokio::test]
    async fn eof_and_idle_do_not_publish_executable_calls() {
        let started = event(
            json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,
            "id":"call","type":"function","function":{"name":"read","arguments":"{}"}}]}}]}),
        );
        let (base, _, handle) = server("200 OK", vec![(started.clone(), 0)]).await;
        let mut observed = Vec::new();
        let result = stream_input_observed(
            &config(base),
            "model",
            "session",
            &[],
            &[],
            10,
            &CANCEL,
            &mut |item| observed.push(item.clone()),
        )
        .await;
        assert_eq!(result, Err(ProviderError::Incomplete));
        assert!(
            observed
                .iter()
                .any(|item| matches!(item, StreamItem::ToolCallStarted { .. }))
        );
        handle.await.unwrap();
        let (base, _, handle) = server(
            "200 OK",
            vec![(started, 0), ("data: [DONE]\n\n".into(), 500)],
        )
        .await;
        let mut cfg = config(base);
        cfg.chunk_timeout_ms = 40;
        assert_eq!(
            stream_input_observed(&cfg, "model", "session", &[], &[], 10, &CANCEL, &mut |_| {})
                .await,
            Err(ProviderError::IdleTimeout)
        );
        handle.abort();
    }
}
