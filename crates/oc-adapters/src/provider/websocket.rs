//! Native Responses channels. One bounded, session-scoped owner; no background
//! reader, generation retry loop, durable ticket or alternative auth/route.
use super::*;
use futures_util::{FutureExt, SinkExt, StreamExt};
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio::time::Instant;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite};
use tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig};

const ROTATE: Duration = Duration::from_secs(55 * 60);
const CHANNELS: usize = 8;
const CHECKPOINT_ITEMS: usize = 10_000;
const BETA: &str = "responses_websockets=2026-02-06";
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Transport {
    Http,
    WebSocket,
}

#[derive(Default)]
struct Pool {
    slots: Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<Slot>>>>,
    closed: AtomicBool,
}

/// Equality describes the shared owner, never a live socket or secret headers.
#[derive(Clone, Default)]
pub(crate) struct Channels(Arc<Pool>);
impl PartialEq for Channels {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Channels {}
impl std::fmt::Debug for Channels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponseChannels").finish_non_exhaustive()
    }
}

struct Slot {
    affinity: String,
    fallback: bool,
    rejected: Option<String>,
    touched: Instant,
    connection: Option<Connection>,
}
impl Default for Slot {
    fn default() -> Self {
        Self {
            affinity: String::new(),
            fallback: false,
            rejected: None,
            touched: Instant::now(),
            connection: None,
        }
    }
}
struct Connection {
    socket: Socket,
    opened: Instant,
    checkpoint: Option<Checkpoint>,
}

/// Append proof only: bounded hashes, no second transcript/output archive.
struct Checkpoint {
    response: String,
    invariant: String,
    prefix: Vec<String>,
}

impl Channels {
    fn slot(&self, session: &str) -> Result<Arc<tokio::sync::Mutex<Slot>>, ProviderError> {
        let mut slots = self.0.slots.lock().map_err(|_| ProviderError::Transport)?;
        if self.0.closed.load(Ordering::Acquire) {
            return Err(ProviderError::Cancelled);
        }
        if let Some(slot) = slots.get(session) {
            return Ok(slot.clone());
        }
        if slots.len() >= CHANNELS {
            let oldest = slots
                .iter()
                .filter(|(_, slot)| Arc::strong_count(slot) == 1)
                .filter_map(|(id, slot)| slot.try_lock().ok().map(|s| (id.clone(), s.touched)))
                .min_by_key(|(_, at)| *at)
                .map(|(id, _)| id)
                .ok_or(ProviderError::ByteLimit("active Responses channels"))?;
            // No owned/waiting exchange references this idle slot. Dropping it
            // closes its descriptor and append proof before admitting another.
            slots.remove(&oldest);
        }
        let slot = Arc::new(tokio::sync::Mutex::new(Slot::default()));
        slots.insert(session.to_owned(), slot.clone());
        Ok(slot)
    }

    pub(crate) async fn shutdown(&self) -> Result<(), ProviderError> {
        self.0.closed.store(true, Ordering::Release);
        let slots =
            std::mem::take(&mut *self.0.slots.lock().map_err(|_| ProviderError::Transport)?);
        let mut failed = false;
        for (_, slot) in slots {
            match tokio::time::timeout(Duration::from_secs(1), slot.lock()).await {
                Ok(mut slot) => {
                    if let Some(mut connection) = slot.connection.take() {
                        // Always drop the descriptor even if the peer does not
                        // acknowledge close. A failed cleanup is not success.
                        failed |= !matches!(
                            tokio::time::timeout(
                                Duration::from_secs(1),
                                connection.socket.close(None)
                            )
                            .await,
                            Ok(Ok(()))
                        );
                    }
                }
                Err(_) => failed = true,
            }
        }
        if failed {
            Err(ProviderError::Transport)
        } else {
            Ok(())
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn exchange<F: Future<Output = Result<(), ProviderError>> + Send>(
        &self,
        config: &ResponsesConfig,
        url: &str,
        headers: &reqwest::header::HeaderMap,
        body: &[u8],
        cancel: &AtomicBool,
        idle: Duration,
        total: Option<Instant>,
        observe: &mut (dyn FnMut(&StreamItem) + Send),
        dispatch: &mut (impl FnMut() -> F + Send),
    ) -> Result<Option<Generation>, ProviderError> {
        let context = config
            .wire
            .context
            .as_ref()
            .ok_or(ProviderError::InvalidConfig)?;
        let owner = self.slot(context.session())?;
        let mut slot = tokio::select! {
            biased;
            () = wait_cancel(cancel) => return Err(ProviderError::Cancelled),
            slot = async {
                if let Some(total) = total {
                    tokio::time::timeout_at(total, owner.lock()).await.map_err(|_| ProviderError::Deadline)
                } else { Ok(owner.lock().await) }
            } => slot?,
        };
        if self.0.closed.load(Ordering::Acquire) {
            return Err(ProviderError::Cancelled);
        }
        let mut headers = headers.clone();
        headers
            .entry("openai-beta")
            .or_insert(reqwest::header::HeaderValue::from_static(BETA));
        let sorted = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_bytes()))
            .collect::<BTreeMap<_, _>>();
        let affinity = crate::compaction::fingerprint(&(
            url,
            sorted,
            config.wire.openai.as_ref().map(|b| &b.scope),
            config.wire.endpoint.as_ref().map(|e| e.provenance()),
        ));
        if slot.affinity != affinity {
            slot.connection = None;
            slot.fallback = false;
            slot.rejected = None;
            slot.affinity = affinity;
        }
        slot.touched = Instant::now();
        if slot.fallback {
            return Ok(None);
        }
        // Taking ownership makes every cancellation/error path close the socket
        // and clear staged state. Only a fully consumed success puts it back.
        let mut connection = slot
            .connection
            .take()
            .filter(|c| c.opened.elapsed() < ROTATE);
        if let Some(active) = &mut connection {
            // Unsolicited data after the preceding terminal event cannot be
            // attributed to this new request. Control/close are not model data.
            for _ in 0..32 {
                match active.socket.next().now_or_never() {
                    Some(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => continue,
                    None => break,
                    _ => {
                        connection = None;
                        break;
                    }
                }
            }
        }
        let mut connection = match connection {
            Some(connection) => connection,
            None => match connect(config, url, &headers, cancel, total).await {
                Ok(socket) => Connection {
                    socket,
                    opened: Instant::now(),
                    checkpoint: None,
                },
                Err(
                    error @ (ProviderError::Cancelled
                    | ProviderError::PrivateHost
                    | ProviderError::InvalidConfig),
                ) => return Err(error),
                Err(ProviderError::Request(failure))
                    if matches!(
                        failure.kind,
                        FailureKind::ContentPolicy
                            | FailureKind::ContextOverflow
                            | FailureKind::Quota
                    ) =>
                {
                    return Err(ProviderError::Request(failure));
                }
                Err(error) => {
                    if total.is_some_and(|end| Instant::now() >= end) {
                        return Err(error);
                    }
                    slot.fallback = true;
                    return Ok(None);
                }
            },
        };
        let full = frame(body)?;
        let recovery = crate::compaction::fingerprint(&(context.operation(), &full));
        let request =
            incremental(&full, connection.checkpoint.as_ref()).unwrap_or_else(|| full.clone());
        let sent =
            String::from_utf8(bounded_json(&request)?).map_err(|_| ProviderError::InvalidConfig)?;
        if cancel.load(Ordering::Relaxed) {
            return Err(ProviderError::Cancelled);
        }
        if total.is_some_and(|end| Instant::now() >= end) {
            return Err(ProviderError::Deadline);
        }
        tokio::select! {
            biased;
            () = wait_cancel(cancel) => return Err(ProviderError::Cancelled),
            result = tokio::time::timeout_at(wait_deadline(idle,total), dispatch()) => {
                result.map_err(|_| channel_failure(Operation::Request,Delivery::NotSent,false,timeout_kind(total)))??;
            }
        }
        let send = tokio::select! {
            biased;
            () = wait_cancel(cancel) => return Err(ProviderError::Cancelled),
            result = tokio::time::timeout_at(wait_deadline(idle, total), connection.socket.send(Message::text(sent.clone()))) => result,
        };
        match send {
            Ok(Ok(())) => {}
            Ok(Err(error)) if affirmatively_not_sent(&error, &sent) => {
                slot.fallback = true;
                return Ok(None);
            }
            Ok(Err(_)) => {
                return Err(channel_failure(
                    Operation::Request,
                    Delivery::Unknown,
                    false,
                    TransportKind::Network,
                ));
            }
            Err(_) => {
                return Err(channel_failure(
                    Operation::Request,
                    Delivery::Unknown,
                    false,
                    timeout_kind(total),
                ));
            }
        }
        let mut parser = SseParser {
            redactions: headers.clone(),
            ..Default::default()
        };
        let mut items = Vec::new();
        let mut response: Option<String> = None;
        let mut committed = false;
        loop {
            let next = tokio::select! {
                biased;
                () = wait_cancel(cancel) => return Err(ProviderError::Cancelled),
                next = tokio::time::timeout_at(wait_deadline(idle, total), connection.socket.next()) => next,
            };
            let message = match next {
                Ok(Some(Ok(message))) => message,
                Ok(Some(Err(_))) | Ok(None) => {
                    return Err(channel_failure(
                        Operation::Read,
                        if response.is_some() {
                            Delivery::Accepted
                        } else {
                            Delivery::Unknown
                        },
                        committed,
                        TransportKind::Network,
                    ));
                }
                Err(_) => {
                    return Err(channel_failure(
                        Operation::Read,
                        if response.is_some() {
                            Delivery::Accepted
                        } else {
                            Delivery::Unknown
                        },
                        committed,
                        timeout_kind(total),
                    ));
                }
            };
            let text = match message {
                Message::Text(text) => text,
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(Some(close))
                    if close.code == tungstenite::protocol::frame::coding::CloseCode::Size
                        && response.is_none()
                        && !committed =>
                {
                    slot.fallback = true;
                    return Ok(None);
                }
                Message::Close(_) => {
                    return Err(channel_failure(
                        Operation::Read,
                        if response.is_some() {
                            Delivery::Accepted
                        } else {
                            Delivery::Unknown
                        },
                        committed,
                        TransportKind::Network,
                    ));
                }
                _ => return Err(ProviderError::InvalidOutput),
            };
            let value: serde_json::Value = serde_json::from_str(&text)
                .map_err(|_| structural(OutputStage::Decode, OutputCode::InvalidJson))?;
            let kind = value["type"].as_str();
            let provider_failure = matches!(kind, Some("error" | "response.failed"))
                || (kind.is_none() && value.get("error").is_some());
            let id = value
                .pointer("/response/id")
                .and_then(serde_json::Value::as_str);
            if kind == Some("response.created") {
                if response.is_some()
                    || id.is_none_or(|id| {
                        id.is_empty() || id.len() > 512 || id.chars().any(char::is_control)
                    })
                {
                    return Err(structural(
                        OutputStage::Decode,
                        OutputCode::IdentityConflict,
                    ));
                }
                response = id.map(str::to_owned);
            } else if kind.is_some_and(|kind| kind.starts_with("response."))
                && ((!provider_failure && response.is_none())
                    || id.is_some_and(|id| response.as_deref().is_some_and(|prior| prior != id)))
            {
                return Err(structural(
                    OutputStage::Decode,
                    OutputCode::IdentityConflict,
                ));
            }
            // A WS text frame may contain pretty JSON/newlines. Re-encode the
            // parsed event rather than treating its original framing as SSE.
            let data = format!("data: {value}\n\n");
            let mut forward = |item: &StreamItem| {
                committed |= !matches!(
                    item,
                    StreamItem::Usage { .. } | StreamItem::MessageBoundary { .. }
                );
                observe(item);
            };
            match parser.push_observed(data.as_bytes(), &mut forward) {
                Ok(mut fresh) => items.append(&mut fresh),
                Err(ProviderError::Request(mut failure)) => {
                    failure.http_status = Some(101);
                    failure.output_committed = committed;
                    // Upgrade headers belong to the connection, never this
                    // response. Common classifier/redactor ran before exposure.
                    failure.headers = RetryHeaders::default();
                    let code = ["/error/code", "/response/error/code", "/code"]
                        .iter()
                        .find_map(|path| value.pointer(path).and_then(serde_json::Value::as_str));
                    let control = matches!(
                        code,
                        Some("previous_response_not_found" | "websocket_connection_limit_reached")
                    ) || (request.get("previous_response_id").is_some()
                        && failure.kind == FailureKind::InvalidRequest);
                    if control
                        && !committed
                        && matches!(
                            failure.kind,
                            FailureKind::InvalidRequest | FailureKind::UnknownProvider
                        )
                    {
                        if slot.rejected.as_deref() == Some(recovery.as_str()) {
                            failure.kind = FailureKind::InvalidRequest;
                        } else {
                            // Affirmative channel rejection, not ambiguous loss:
                            // the existing runtime allowance retries a fresh full
                            // request once. No adapter-side generation loop.
                            slot.rejected = Some(recovery);
                            failure.kind = FailureKind::Transport;
                            failure.operation = Operation::Read;
                            failure.delivery = Delivery::Rejected;
                            failure.transport = Some(TransportKind::Network);
                        }
                    }
                    return Err(ProviderError::Request(failure));
                }
                Err(error) => return Err(error),
            }
            if parser.completed {
                break;
            }
            tokio::task::yield_now().await;
        }
        let tail = parser.finish()?;
        for item in &tail {
            observe(item);
        }
        items.extend(tail);
        // Already queued non-control data after terminal means the decoded
        // exchange was not fully consumed. Never publish its append checkpoint.
        let mut keep = true;
        for _ in 0..32 {
            match connection.socket.next().now_or_never() {
                Some(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => continue,
                None => break,
                Some(Some(Ok(Message::Close(_)))) | Some(None) => {
                    keep = false;
                    break;
                }
                _ => return Err(ProviderError::InvalidOutput),
            }
        }
        let generation = collect_generation(parser, items);
        if cancel.load(Ordering::Relaxed) {
            return Err(ProviderError::Cancelled);
        }
        connection.checkpoint = if generation.finish == FinishReason::Stop {
            checkpoint(&full, &generation.output, response.as_deref())
        } else {
            None
        };
        if keep {
            slot.connection = Some(connection);
        }
        slot.rejected = None;
        Ok(Some(generation))
    }
}

fn channel_failure(
    operation: Operation,
    delivery: Delivery,
    committed: bool,
    kind: TransportKind,
) -> ProviderError {
    let mut failure = PhysicalFailure::transport(operation, delivery, Some(101));
    failure.transport = Some(kind);
    failure.output_committed = committed;
    ProviderError::Request(Box::new(failure))
}

fn affirmatively_not_sent(error: &tungstenite::Error, sent: &str) -> bool {
    let tungstenite::Error::WriteBufferFull(message) = error else {
        return false;
    };
    match message.as_ref() {
        Message::Text(text) => text.as_str() == sent,
        Message::Frame(frame) => {
            frame.header().opcode
                == tungstenite::protocol::frame::coding::OpCode::Data(
                    tungstenite::protocol::frame::coding::Data::Text,
                )
                && frame.payload() == sent.as_bytes()
        }
        _ => false,
    }
}

fn wait_deadline(idle: Duration, total: Option<Instant>) -> Instant {
    let idle = Instant::now() + idle;
    total.map_or(idle, |total| total.min(idle))
}
fn timeout_kind(total: Option<Instant>) -> TransportKind {
    if total.is_some_and(|end| Instant::now() >= end) {
        TransportKind::Deadline
    } else {
        TransportKind::IdleTimeout
    }
}

async fn connect(
    config: &ResponsesConfig,
    url: &str,
    headers: &reqwest::header::HeaderMap,
    cancel: &AtomicBool,
    total: Option<Instant>,
) -> Result<Socket, ProviderError> {
    let deadline = Instant::now() + config.connect_timeout;
    let operation = async {
        let (_, addresses) =
            crate::endpoint::resolve(url, config.wire.endpoint.as_ref(), config.allow_private)
                .await?;
        let tcp = TcpStream::connect(addresses.as_slice())
            .await
            .map_err(|_| ProviderError::Transport)?;
        let peer = tcp.peer_addr().map_err(|_| ProviderError::Transport)?;
        if !crate::endpoint::peer_allowed(
            config.wire.endpoint.as_ref(),
            peer.ip(),
            config.allow_private,
        ) {
            return Err(ProviderError::PrivateHost);
        }
        let mut url = reqwest::Url::parse(url).map_err(|_| ProviderError::InvalidConfig)?;
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        url.set_scheme(scheme)
            .map_err(|_| ProviderError::InvalidConfig)?;
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|_| ProviderError::InvalidConfig)?;
        for (name, value) in headers {
            request.headers_mut().insert(name, value.clone());
        }
        request.headers_mut().insert(
            "user-agent",
            reqwest::header::HeaderValue::from_static(crate::USER_AGENT),
        );
        let limits = WebSocketConfig::default()
            .read_buffer_size(16 * 1024)
            .write_buffer_size(0)
            .max_write_buffer_size(REQUEST_BYTE_CAP + 64 * 1024)
            .max_frame_size(Some(SSE_BYTE_CAP))
            .max_message_size(Some(SSE_BYTE_CAP));
        match tokio_tungstenite::client_async_tls_with_config(request, tcp, Some(limits), None)
            .await
        {
            Ok((socket, _)) => Ok(socket),
            Err(tungstenite::Error::Http(response)) => {
                let status = response.status().as_u16();
                let value = response
                    .body()
                    .as_ref()
                    .filter(|b| b.len() <= 16 * 1024)
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok());
                let mut failure = failure::classified(value.as_ref(), Some(status), false);
                failure.message = value.as_ref().and_then(|v| error_message(v, headers));
                // This handshake is not a model-response retry override.
                Err(ProviderError::Request(Box::new(failure)))
            }
            Err(_) => Err(ProviderError::Transport),
        }
    };
    tokio::select! {
        biased;
        () = wait_cancel(cancel) => Err(ProviderError::Cancelled),
        result = tokio::time::timeout_at(total.map_or(deadline, |total| total.min(deadline)), operation) => result.map_err(|_| ProviderError::Deadline)?,
    }
}

fn frame(body: &[u8]) -> Result<serde_json::Value, ProviderError> {
    let mut value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| ProviderError::InvalidConfig)?;
    let object = value.as_object_mut().ok_or(ProviderError::InvalidConfig)?;
    for field in [
        "stream",
        "stream_options",
        "background",
        "previous_response_id",
    ] {
        object.remove(field);
    }
    object.insert("type".into(), "response.create".into());
    Ok(value)
}

fn invariant(value: &serde_json::Value) -> String {
    let mut value = value.clone();
    if let Some(value) = value.as_object_mut() {
        for field in ["input", "type", "previous_response_id"] {
            value.remove(field);
        }
    }
    digest(&value)
}
fn digest(value: &serde_json::Value) -> String {
    fn ordered(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(fields) => fields
                .iter()
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .map(|(k, v)| (k.clone(), ordered(v)))
                .collect::<serde_json::Map<_, _>>()
                .into(),
            serde_json::Value::Array(values) => {
                values.iter().map(ordered).collect::<Vec<_>>().into()
            }
            other => other.clone(),
        }
    }
    crate::compaction::fingerprint(&ordered(value))
}
fn comparable(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::json;
    match value["type"].as_str() {
        Some("function_call") => {
            json!({"type":"function_call","call_id":value["call_id"],"name":value["name"],"arguments":value["arguments"].as_str().and_then(|v|serde_json::from_str::<serde_json::Value>(v).ok()).unwrap_or_else(||value["arguments"].clone())})
        }
        Some("reasoning") => {
            json!({"type":"reasoning","summary":value["summary"],"encrypted_content":value["encrypted_content"]})
        }
        Some("message") if value["role"] == "assistant" => {
            let content = value["content"].as_array().map(|parts| {
                parts
                    .iter()
                    .map(|part| {
                        if part["type"] == "output_text" {
                            json!({"type":"output_text","text":part["text"]})
                        } else {
                            part.clone()
                        }
                    })
                    .collect::<Vec<_>>()
            });
            let mut message = json!({"role":"assistant","content":content});
            if let Some(phase) = value.get("phase") {
                message["phase"] = phase.clone();
            }
            message
        }
        _ => value.clone(),
    }
}
fn checkpoint(
    full: &serde_json::Value,
    output: &[serde_json::Value],
    response: Option<&str>,
) -> Option<Checkpoint> {
    let input = full["input"].as_array()?;
    if input.len().saturating_add(output.len()) > CHECKPOINT_ITEMS
        || input.iter().any(|i| i["type"] == "compaction_trigger")
    {
        return None;
    }
    Some(Checkpoint {
        response: response?.to_owned(),
        invariant: invariant(full),
        prefix: input
            .iter()
            .chain(output)
            .map(|i| digest(&comparable(i)))
            .collect(),
    })
}
fn incremental(
    full: &serde_json::Value,
    checkpoint: Option<&Checkpoint>,
) -> Option<serde_json::Value> {
    let checkpoint = checkpoint?;
    let input = full["input"].as_array()?;
    if invariant(full) != checkpoint.invariant
        || input.len() <= checkpoint.prefix.len()
        || !input
            .iter()
            .zip(&checkpoint.prefix)
            .all(|(item, hash)| digest(&comparable(item)) == *hash)
    {
        return None;
    }
    let mut request = full.clone();
    request["input"] = input[checkpoint.prefix.len()..].to_vec().into();
    request["previous_response_id"] = checkpoint.response.clone().into();
    Some(request)
}

#[cfg(test)]
mod tests;
