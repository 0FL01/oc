//! Cancellation ownership around rmcp 3.4's HTTP backend. Its startup POST
//! and initial SSE response await outside the worker cancellation select.
//! Delegate wire handling to the SDK, but cancel those awaits and observe the
//! last HTTP owner dropping before reporting a failed/cancelled attach.

use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use futures_util::StreamExt as _;
use reqwest::header::{HeaderName, HeaderValue};
use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::common::client_side_sse::BoxedSseResponse;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClient, StreamableHttpError, StreamableHttpPostResponse,
};
use tokio::sync::oneshot;

type Error = StreamableHttpError<reqwest::Error>;
type Headers = HashMap<HeaderName, HeaderValue>;

#[derive(Clone)]
pub(super) struct Http(Arc<Owner>);

struct Owner {
    client: reqwest::Client,
    cancel: Arc<AtomicBool>,
    // Closed only after every SDK worker/request/stream HTTP owner is dropped.
    _closed: oneshot::Sender<()>,
}

pub(super) struct Cleanup {
    cancel: Arc<AtomicBool>,
    closed: oneshot::Receiver<()>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Cleanup {
    pub(super) async fn close(&mut self) -> Result<(), super::McpError> {
        self.cancel.store(true, Ordering::Relaxed);
        // Sender drop is the success condition; no detached submit/reaper task.
        let _ = tokio::time::timeout(super::CLOSE_TIMEOUT, &mut self.closed)
            .await
            .map_err(|_| super::McpError::CleanupFailed)?;
        Ok(())
    }
}

impl Http {
    pub(super) fn new(client: reqwest::Client) -> (Self, Cleanup) {
        let cancel = Arc::new(AtomicBool::new(false));
        let (closed, receiver) = oneshot::channel();
        (
            Self(Arc::new(Owner {
                client,
                cancel: cancel.clone(),
                _closed: closed,
            })),
            Cleanup {
                cancel,
                closed: receiver,
            },
        )
    }

    async fn run<T>(&self, operation: impl Future<Output = Result<T, Error>>) -> Result<T, Error> {
        tokio::select! {
            biased;
            () = crate::provider::wait_cancel(&self.0.cancel) => Err(StreamableHttpError::TransportChannelClosed),
            result = operation => result,
        }
    }

    fn stream(&self, stream: BoxedSseResponse) -> BoxedSseResponse {
        let owner = self.clone();
        Box::pin(
            stream.take_until(async move { crate::provider::wait_cancel(&owner.0.cancel).await }),
        )
    }
}

impl StreamableHttpClient for Http {
    type Error = reqwest::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session: Option<Arc<str>>,
        auth: Option<String>,
        headers: Headers,
    ) -> Result<StreamableHttpPostResponse, Error> {
        self.post_message_with_max_sse_event_size(
            uri,
            message,
            session,
            auth,
            headers,
            crate::webfetch::BODY_CAP_BYTES,
        )
        .await
    }

    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session: Option<Arc<str>>,
        auth: Option<String>,
        headers: Headers,
        max: usize,
    ) -> Result<StreamableHttpPostResponse, Error> {
        let response = self
            .run(
                self.0.client.post_message_with_max_sse_event_size(
                    uri, message, session, auth, headers, max,
                ),
            )
            .await?;
        Ok(match response {
            StreamableHttpPostResponse::Sse(stream, session) => {
                StreamableHttpPostResponse::Sse(self.stream(stream), session)
            }
            response => response,
        })
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session: Arc<str>,
        auth: Option<String>,
        headers: Headers,
    ) -> Result<(), Error> {
        self.run(self.0.client.delete_session(uri, session, auth, headers))
            .await
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session: Option<Arc<str>>,
        last: Option<String>,
        auth: Option<String>,
        headers: Headers,
    ) -> Result<BoxedSseResponse, Error> {
        self.get_stream_with_max_sse_event_size(
            uri,
            session,
            last,
            auth,
            headers,
            crate::webfetch::BODY_CAP_BYTES,
        )
        .await
    }

    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session: Option<Arc<str>>,
        last: Option<String>,
        auth: Option<String>,
        headers: Headers,
        max: usize,
    ) -> Result<BoxedSseResponse, Error> {
        let stream = self
            .run(
                self.0
                    .client
                    .get_stream_with_max_sse_event_size(uri, session, last, auth, headers, max),
            )
            .await?;
        Ok(self.stream(stream))
    }
}
