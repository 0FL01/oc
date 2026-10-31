//! Byte-bounded wire handling at the existing rmcp HTTP backend boundary.

use super::{Error, Headers, Http};
use crate::mcp_message::Fault;
use crate::mcp_remote::{MESSAGE_BYTES_CAP, metadata_bytes};
use futures_util::StreamExt as _;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use rmcp::model::{ClientJsonRpcMessage, ClientRequest, ErrorData, ServerJsonRpcMessage};
use rmcp::transport::common::client_side_sse::BoxedSseResponse;
use rmcp::transport::common::http_header::{
    EVENT_STREAM_MIME_TYPE, HEADER_LAST_EVENT_ID, HEADER_SESSION_ID, JSON_MIME_TYPE,
};
use rmcp::transport::streamable_http_client::{StreamableHttpError, StreamableHttpPostResponse};
use sse_stream::SseStream;
use std::sync::Arc;

impl Http {
    fn request(
        &self,
        method: reqwest::Method,
        uri: &str,
        session: Option<&str>,
        auth: Option<String>,
        headers: Headers,
    ) -> Result<reqwest::RequestBuilder, Error> {
        let mut request = self
            .0
            .client
            .request(method, uri)
            .header(ACCEPT, [EVENT_STREAM_MIME_TYPE, JSON_MIME_TYPE].join(", "));
        // Same reserved fields as rmcp; the worker supplies Protocol-Version.
        for (name, value) in headers {
            if name == ACCEPT
                || name.as_str().eq_ignore_ascii_case(HEADER_SESSION_ID)
                || name.as_str().eq_ignore_ascii_case(HEADER_LAST_EVENT_ID)
            {
                return Err(StreamableHttpError::ReservedHeaderConflict(
                    name.to_string(),
                ));
            }
            request = request.header(name, value);
        }
        if let Some(session) = session {
            request = request.header(HEADER_SESSION_ID, session);
        }
        if let Some(auth) = auth {
            request = request.bearer_auth(auth);
        }
        Ok(request)
    }

    async fn body(&self, mut response: reqwest::Response) -> Result<Vec<u8>, Error> {
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            self.0
                .fault
                .check(body.len().saturating_add(chunk.len()), MESSAGE_BYTES_CAP)?;
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    fn sse(&self, response: reqwest::Response, max: usize) -> BoxedSseResponse {
        let input = response.bytes_stream().boxed();
        let size = EventSize {
            fault: self.0.fault.clone(),
            max: max.min(MESSAGE_BYTES_CAP),
            bytes: 0,
            line: 0,
            cr: false,
        };
        let bytes =
            futures_util::stream::try_unfold((input, size), |(mut input, mut size)| async move {
                let Some(chunk) = input
                    .next()
                    .await
                    .transpose()
                    .map_err(std::io::Error::other)?
                else {
                    return Ok::<_, std::io::Error>(None);
                };
                size.observe(&chunk)?;
                Ok(Some((chunk, (input, size))))
            });
        self.stream(Box::pin(SseStream::from_bytes_stream(bytes)))
    }

    pub(super) async fn post(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session: Option<Arc<str>>,
        auth: Option<String>,
        headers: Headers,
        max: usize,
    ) -> Result<StreamableHttpPostResponse, Error> {
        self.0
            .fault
            .check(metadata_bytes(&message), MESSAGE_BYTES_CAP)?;
        let response = self
            .request(
                reqwest::Method::POST,
                &uri,
                session.as_deref(),
                auth,
                headers,
            )?
            .json(&message)
            .send()
            .await?;
        let status = response.status();
        if matches!(
            status,
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
        ) {
            response.error_for_status_ref()?;
        }
        if matches!(
            status,
            reqwest::StatusCode::ACCEPTED | reqwest::StatusCode::NO_CONTENT
        ) {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        if status == reqwest::StatusCode::NOT_FOUND && session.is_some() {
            return Err(StreamableHttpError::SessionExpired);
        }
        let notification = !matches!(&message, ClientJsonRpcMessage::Request(_));
        if status.is_success() && response.content_length() == Some(0) && notification {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned());
        let session_id = response
            .headers()
            .get(HEADER_SESSION_ID)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        if !status.is_success() {
            if content_type
                .as_deref()
                .is_some_and(|value| value.starts_with(JSON_MIME_TYPE))
            {
                let body = self.body(response).await?;
                if let Ok(message @ ServerJsonRpcMessage::Error(_)) = serde_json::from_slice(&body)
                {
                    return Ok(StreamableHttpPostResponse::Json(message, session_id));
                }
            }
            // Preserve rmcp's sessionless legacy discovery fallback, without
            // retaining or displaying arbitrary middleware error bodies.
            if session.is_none()
                && status.is_client_error()
                && let ClientJsonRpcMessage::Request(request) = &message
                && matches!(request.request, ClientRequest::DiscoverRequest(_))
            {
                return Ok(StreamableHttpPostResponse::Json(
                    ServerJsonRpcMessage::error(
                        ErrorData::invalid_request(
                            format!("server/discover rejected with HTTP {status}"),
                            None,
                        ),
                        Some(request.id.clone()),
                    ),
                    None,
                ));
            }
            return Err(StreamableHttpError::UnexpectedServerResponse(
                format!("HTTP {status}").into(),
            ));
        }
        match content_type.as_deref() {
            Some(value) if value.starts_with(EVENT_STREAM_MIME_TYPE) => Ok(
                StreamableHttpPostResponse::Sse(self.sse(response, max), session_id),
            ),
            Some(value) if value.starts_with(JSON_MIME_TYPE) => {
                let body = self.body(response).await?;
                match serde_json::from_slice(&body) {
                    Ok(message) => Ok(StreamableHttpPostResponse::Json(message, session_id)),
                    Err(_) if notification => Ok(StreamableHttpPostResponse::Accepted),
                    Err(_) => Err(StreamableHttpError::UnexpectedServerResponse(
                        "invalid JSON-RPC response".into(),
                    )),
                }
            }
            _ => Err(StreamableHttpError::UnexpectedContentType(content_type)),
        }
    }

    pub(super) async fn get(
        &self,
        uri: Arc<str>,
        session: Option<Arc<str>>,
        last: Option<String>,
        auth: Option<String>,
        headers: Headers,
        max: usize,
    ) -> Result<BoxedSseResponse, Error> {
        let mut request = self.request(
            reqwest::Method::GET,
            &uri,
            session.as_deref(),
            auth,
            headers,
        )?;
        if let Some(last) = last {
            request = request.header(HEADER_LAST_EVENT_ID, last);
        }
        let response = request.send().await?;
        if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            return Err(StreamableHttpError::ServerDoesNotSupportSse);
        }
        response.error_for_status_ref()?;
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned());
        if !content_type.as_deref().is_some_and(|value| {
            value.starts_with(EVENT_STREAM_MIME_TYPE) || value.starts_with(JSON_MIME_TYPE)
        }) {
            return Err(StreamableHttpError::UnexpectedContentType(content_type));
        }
        Ok(self.sse(response, max))
    }
}

// Count raw event fields across network chunks; SseStream still owns all SSE
// decoding, including multiline data, comments, IDs, retry and UTF-8.
struct EventSize {
    fault: Fault,
    max: usize,
    bytes: usize,
    line: usize,
    cr: bool,
}

impl EventSize {
    fn observe(&mut self, chunk: &[u8]) -> std::io::Result<()> {
        for byte in chunk {
            if self.cr && *byte == b'\n' {
                self.cr = false;
                if self.bytes != 0 {
                    self.bytes += 1;
                    self.fault.check(self.bytes, self.max)?;
                }
                continue;
            }
            self.cr = *byte == b'\r';
            if matches!(byte, b'\r' | b'\n') {
                if self.line == 0 {
                    self.bytes = 0;
                } else {
                    self.bytes += 1;
                }
                self.line = 0;
            } else {
                self.bytes += 1;
                self.line += 1;
            }
            self.fault.check(self.bytes, self.max)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_budget_spans_chunks_and_resets_at_a_split_crlf_delimiter() {
        let fault = Fault::default();
        let mut size = EventSize {
            fault: fault.clone(),
            max: MESSAGE_BYTES_CAP,
            bytes: 0,
            line: 0,
            cr: false,
        };
        size.observe(b"data: ").unwrap();
        size.observe(&vec![b'x'; MESSAGE_BYTES_CAP - 32]).unwrap();
        size.observe(b"\r").unwrap();
        size.observe(b"\n\r").unwrap();
        size.observe(b"\ndata: next\n\n").unwrap();
        assert_eq!(size.bytes, 0);
        assert!(fault.get().is_none());
        assert!(size.observe(&vec![b'x'; MESSAGE_BYTES_CAP + 1]).is_err());
        assert_eq!(
            fault.get(),
            Some((MESSAGE_BYTES_CAP + 1, MESSAGE_BYTES_CAP))
        );
    }
}
